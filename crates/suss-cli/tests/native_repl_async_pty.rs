//! Interactive tty acceptance: the actual child frontend, never a mock editor.
#![cfg(unix)]
use nix::libc;
use std::{
    ffi::CString,
    fs::File,
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    },
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
const DEADLINE: Duration = Duration::from_secs(60);
struct PtyRepl {
    child: Child,
    master: File,
    original: libc::termios,
    transcript: Vec<u8>,
}
impl PtyRepl {
    fn new() -> Self {
        let mut master = -1;
        let mut slave = -1;
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            },
            0,
            "{}",
            io::Error::last_os_error()
        );
        // SAFETY: openpty returned unique open descriptors.
        let master = unsafe { File::from_raw_fd(master) };
        let slave = unsafe { File::from_raw_fd(slave) };
        let mut original = std::mem::MaybeUninit::uninit();
        assert_eq!(
            unsafe { libc::tcgetattr(slave.as_raw_fd(), original.as_mut_ptr()) },
            0
        );
        let original = unsafe { original.assume_init() };
        // Establish that the retained master observes these same settings;
        // macOS invalidates slave ioctls when the session leader exits.
        let mut master_settings = std::mem::MaybeUninit::uninit();
        assert_eq!(
            unsafe { libc::tcgetattr(master.as_raw_fd(), master_settings.as_mut_ptr()) },
            0,
            "master baseline: {}",
            io::Error::last_os_error()
        );
        let master_settings = unsafe { master_settings.assume_init() };
        assert_eq!(master_settings.c_iflag, original.c_iflag);
        assert_eq!(master_settings.c_oflag, original.c_oflag);
        assert_eq!(master_settings.c_lflag, original.c_lflag);
        assert_eq!(master_settings.c_cc, original.c_cc);
        // Do not leak the master into the executed frontend. stdio owns
        // separate slave copies; pre_exec uses only async-signal-safe syscalls.
        for fd in [master.as_raw_fd(), slave.as_raw_fd()] {
            assert_eq!(
                unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) },
                0
            );
        }
        let mut command = Command::new(env!("CARGO_BIN_EXE_suss"));
        command
            .arg("repl")
            .env("TERM", "xterm")
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave));
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
                let group = libc::getpid();
                if libc::ioctl(0, libc::TIOCSPGRP as _, &group) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().unwrap();
        let flags = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
        assert!(flags >= 0);
        assert_eq!(
            unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
            0
        );
        Self {
            child,
            master,
            original,
            transcript: Vec::new(),
        }
    }
    fn write(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).unwrap();
    }
    fn line(&mut self, source: &str) {
        self.write(source.as_bytes());
        self.write(b"\r");
    }
    fn observe(&mut self) {
        let mut fd = libc::pollfd {
            fd: self.master.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut fd, 1, 10) };
        if result < 0 {
            assert_eq!(
                io::Error::last_os_error().kind(),
                io::ErrorKind::Interrupted
            );
        }
        let mut buffer = [0; 8192];
        loop {
            match self.master.read(&mut buffer) {
                Ok(0) => break,
                Ok(size) => self.transcript.extend_from_slice(&buffer[..size]),
                Err(error)
                    if error.kind() == io::ErrorKind::WouldBlock
                        || error.raw_os_error() == Some(libc::EIO) =>
                {
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => panic!("PTY read: {error}"),
            }
        }
    }
    fn wait_after(&mut self, start: usize, needle: &[u8]) {
        let deadline = Instant::now() + DEADLINE;
        loop {
            self.observe();
            if self.transcript[start..]
                .windows(needle.len())
                .any(|bytes| bytes == needle)
            {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "missing {:?}: {}",
                String::from_utf8_lossy(needle),
                String::from_utf8_lossy(&self.transcript)
            );
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "child exited: {}",
                String::from_utf8_lossy(&self.transcript)
            );
        }
    }
    fn query(&mut self, source: &str, output: &str) {
        let start = self.transcript.len();
        self.line(source);
        self.wait_after(start, format!("{output}\r\n").as_bytes());
    }
    fn finish(&mut self) {
        self.line(":quit");
        let deadline = Instant::now() + DEADLINE;
        loop {
            self.observe();
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(
                Instant::now() < deadline,
                "PTY exit failed: {}",
                String::from_utf8_lossy(&self.transcript)
            );
        }
        // Inspect through the retained master after normal child shutdown.
        // This is the same settings view verified at startup, not a skipped
        // restoration assertion when the controlling slave becomes hung up.
        let mut restored = std::mem::MaybeUninit::uninit();
        assert_eq!(
            unsafe { libc::tcgetattr(self.master.as_raw_fd(), restored.as_mut_ptr()) },
            0,
            "master restoration ioctl: {}",
            io::Error::last_os_error()
        );
        let restored = unsafe { restored.assume_init() };
        assert_eq!(restored.c_iflag, self.original.c_iflag);
        assert_eq!(restored.c_oflag, self.original.c_oflag);
        assert_eq!(restored.c_lflag, self.original.c_lflag);
        assert_eq!(restored.c_cc, self.original.c_cc);
    }
}
impl Drop for PtyRepl {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn fifo_guard(path: &Path) -> File {
    let path = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDWR | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    assert!(fd >= 0);
    unsafe { File::from_raw_fd(fd) }
}
fn literal(path: &Path) -> String {
    serde_json::to_string(path.to_str().unwrap()).unwrap()
}

#[test]
fn session_lifecycle_interactive_utf8_redraw_history_and_pending_sigint_cleanup_are_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let idle = dir.path().join("idle");
    let body = dir.path().join("body");
    let cleanup = dir.path().join("cleanup");
    let mut idle_writer = fifo_guard(&idle);
    let _body_writer = fifo_guard(&body);
    let mut cleanup_writer = fifo_guard(&cleanup);
    let mut repl = PtyRepl::new();
    repl.wait_after(0, b"=> ");
    repl.line("(ns user (:require [suss.async :as async]) (:require-macros [suss.async :refer [future await]]))");
    repl.query("(+ 9000 1)", "9001");
    repl.line("(def trace 0)");
    repl.line(&format!(
        "(def idle-task (future (set! trace (await (suss.io/read-byte {}))) (throw 9301)))",
        literal(&idle)
    ));
    repl.query("(+ 9000 trace)", "9000");
    // Leave both a partial form and split UTF-8 sequence at the active prompt.
    // The reported asynchronous failure must neither evaluate nor discard it.
    let before_partial = repl.transcript.len();
    repl.write(b"(def edited \"\xe2");
    // Rendering this prefix proves the first incomplete UTF-8 byte has already
    // reached an input turn before the remaining bytes are supplied.
    repl.wait_after(before_partial, b"(def edited \"");
    repl.write(b"\x82\xac");
    let before_failure = repl.transcript.len();
    idle_writer.write_all(&[65]).unwrap();
    repl.wait_after(before_failure, b"9301");
    repl.write(b"\")\r");
    repl.query("edited", "\"€\"");
    repl.query("(+ 9400 trace)", "9465");
    repl.line("(def calls 0)");
    repl.query("(do (set! calls (+ calls 1)) calls)", "1");
    let before_history = repl.transcript.len();
    repl.write(b"\x1b[A\r");
    repl.wait_after(before_history, b"2\r\n");
    repl.query("(+ 9500 calls)", "9502");
    repl.line("(def cleaned 0)");
    repl.line(&format!("(def cancelled-task (future (try (await (suss.io/read-byte {})) (finally (set! cleaned (+ cleaned 1)) (await (suss.io/read-byte {})) (set! cleaned (+ cleaned 1))))))", literal(&body), literal(&cleanup)));
    repl.query("(+ 9600 cleaned)", "9600");
    let before_decoded = repl.transcript.len();
    repl.write(b"(def must-not-publish \"\xe2\x82\xac");
    // Full UTF-8 render is an acknowledgement that this text has left the
    // kernel queue and lives in the editor's decoded partial-line state.
    repl.wait_after(before_decoded, b"(def must-not-publish \"\xe2\x82\xac");
    let before_interrupt = repl.transcript.len();
    // External SIGINT exercises the actual handler/self-pipe wake rather than
    // trusting an editor mock's Ctrl-C event. The child is in a real tty session.
    assert_eq!(
        unsafe { libc::kill(repl.child.id() as i32, libc::SIGINT) },
        0
    );
    repl.wait_after(before_interrupt, b"^C");
    cleanup_writer.write_all(&[7]).unwrap();
    repl.query("(+ 9700 cleaned)", "9702");
    repl.query("(+ 9800 cleaned)", "9802");
    repl.query("(+ 9900 3)", "9903");
    // A second cancellation covers the distinct unread-kernel-input case.
    // Stop acknowledgement proves these bytes cannot have reached the editor.
    repl.line(&format!("(def unread-task (future (try (await (suss.io/read-byte {})) (finally (set! cleaned (+ cleaned 1)) (await (suss.io/read-byte {})) (set! cleaned (+ cleaned 1))))))", literal(&body), literal(&cleanup)));
    repl.query("(+ 10000 cleaned)", "10002");
    assert_eq!(
        unsafe { libc::kill(repl.child.id() as i32, libc::SIGSTOP) },
        0
    );
    let stop_deadline = Instant::now() + DEADLINE;
    loop {
        let mut status = 0;
        let result = unsafe {
            libc::waitpid(
                repl.child.id() as i32,
                &mut status,
                libc::WNOHANG | libc::WUNTRACED,
            )
        };
        if result > 0 {
            assert!(libc::WIFSTOPPED(status));
            break;
        }
        assert!(result >= 0, "stop wait: {}", io::Error::last_os_error());
        assert!(
            Instant::now() < stop_deadline,
            "child never acknowledged SIGSTOP"
        );
        repl.observe();
    }
    repl.write(b"(def unread-prefix \"discard");
    let before_unread_interrupt = repl.transcript.len();
    assert_eq!(
        unsafe { libc::kill(repl.child.id() as i32, libc::SIGINT) },
        0
    );
    assert_eq!(
        unsafe { libc::kill(repl.child.id() as i32, libc::SIGCONT) },
        0
    );
    repl.wait_after(before_unread_interrupt, b"^C");
    cleanup_writer.write_all(&[8]).unwrap();
    repl.query("(+ 10100 cleaned)", "10104");
    repl.query("(+ 10200 5)", "10205");
    repl.finish();
    let text = String::from_utf8_lossy(&repl.transcript);
    assert!(!text.contains("invalid UTF-8"), "{text}");
    assert!(!text.contains("Unclosed collection"), "{text}");
}
