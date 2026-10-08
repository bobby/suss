//! Real subprocess/frontend lifecycle tests. Command stdin stays open while
//! separate FIFO endpoints suspend source I/O. No language execution threads.
#![cfg(unix)]
use nix::libc;
use std::{
    ffi::CString,
    fs::File,
    io::{self, Read, Write},
    os::fd::{AsRawFd, FromRawFd},
    path::Path,
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};
const DEADLINE: Duration = Duration::from_secs(60);
struct Repl {
    child: Child,
    input: Option<ChildStdin>,
    stdout: ChildStdout,
    stderr: ChildStderr,
    out: String,
    err: String,
}
impl Repl {
    fn new() -> Self {
        Self::with_fuel(None)
    }
    fn with_fuel(fuel: Option<u64>) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_suss"));
        command.arg("repl");
        if let Some(fuel) = fuel {
            command.arg("--fuel").arg(fuel.to_string());
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        for fd in [stdout.as_raw_fd(), stderr.as_raw_fd()] {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            assert!(flags >= 0);
            assert_eq!(
                unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) },
                0
            );
        }
        Self {
            child,
            input,
            stdout,
            stderr,
            out: String::new(),
            err: String::new(),
        }
    }
    fn send(&mut self, source: &str) {
        writeln!(self.input.as_mut().unwrap(), "{source}").unwrap();
    }
    fn observe(&mut self, timeout: Duration) {
        let mut fds = [
            libc::pollfd {
                fd: self.stdout.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: self.stderr.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let result = unsafe {
            libc::poll(
                fds.as_mut_ptr(),
                2,
                timeout.as_millis().min(i32::MAX as u128) as i32,
            )
        };
        if result < 0 {
            assert_eq!(
                io::Error::last_os_error().kind(),
                io::ErrorKind::Interrupted
            );
        }
        drain(&mut self.stdout, &mut self.out);
        drain(&mut self.stderr, &mut self.err);
    }
    fn wait(&mut self, description: &str, mut predicate: impl FnMut(&Self) -> bool) {
        let deadline = Instant::now() + DEADLINE;
        loop {
            self.observe(Duration::from_millis(10));
            if predicate(self) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{description} timed out; stdout={} stderr={}",
                self.out,
                self.err
            );
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "child exited before {description}; stdout={} stderr={}",
                self.out,
                self.err
            );
        }
    }
    fn line(&mut self, expected: &str) {
        self.wait(expected, |child| {
            child.out.lines().any(|line| line == expected)
        });
    }
    fn finish(&mut self) {
        self.send(":quit");
        self.input.take();
        let deadline = Instant::now() + DEADLINE;
        loop {
            self.observe(Duration::from_millis(10));
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success(), "{status}: {}", self.err);
                return;
            }
            assert!(
                Instant::now() < deadline,
                "REPL failed retirement: {} {}",
                self.out,
                self.err
            );
        }
    }
}
impl Drop for Repl {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn drain(reader: &mut impl Read, output: &mut String) {
    let mut bytes = [0; 4096];
    loop {
        match reader.read(&mut bytes) {
            Ok(0) => break,
            Ok(size) => {
                output.push_str(std::str::from_utf8(&bytes[..size]).expect("ASCII test outputs"))
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => panic!("output read: {error}"),
        }
    }
}
fn fifo(path: &Path) {
    let path = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(
        unsafe { libc::mkfifo(path.as_ptr(), 0o600) },
        0,
        "{}",
        io::Error::last_os_error()
    );
}
fn open(path: &Path, flags: i32) -> io::Result<File> {
    let path = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    let fd = unsafe { libc::open(path.as_ptr(), flags | libc::O_CLOEXEC) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: open returned this uniquely owned fd.
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn held_writer(path: &Path) -> File {
    // A real writer exists before the child opens the FIFO. The parent's read
    // endpoint also permits opening without a reader handshake or fake EOF.
    open(path, libc::O_RDWR | libc::O_NONBLOCK).unwrap()
}
fn literal(path: &Path) -> String {
    serde_json::to_string(path.to_str().unwrap()).unwrap()
}
fn setup(repl: &mut Repl) {
    repl.send("(ns user (:require [suss.async :as async]) (:require-macros [suss.async :refer [future await]]))");
    repl.send("(+ 7000 1)");
    repl.line("7001");
}

#[test]
fn session_lifecycle_idle_source_io_progresses_with_stdin_open_and_input_stays_responsive() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first");
    fifo(&first);
    let mut first_writer = held_writer(&first);
    let mut repl = Repl::new();
    setup(&mut repl);
    repl.send("(def trace 0)");
    repl.send(&format!(
        "(def task (future (set! trace (await (suss.io/read-byte {}))) (throw 9101)))",
        literal(&first)
    ));
    repl.send("(+ 7100 trace)");
    repl.line("7100");
    assert!(
        repl.err.is_empty(),
        "pending setup failed before byte delivery: {}",
        repl.err
    );
    // No further stdin bytes until the tracked asynchronous failure appears.
    // Its payload proves the continuation ran after a real FIFO completion.
    first_writer.write_all(&[65]).unwrap();
    repl.wait("idle asynchronous failure sentinel", |child| {
        child.err.contains("9101")
    });
    repl.send("(+ 7200 trace)");
    repl.line("7265");
    repl.send("(+ 7300 2)");
    repl.line("7302");
    repl.finish();
    assert_eq!(
        repl.err
            .lines()
            .filter(|line| line.contains("9101"))
            .count(),
        1,
        "{}",
        repl.err
    );
}

#[test]
fn session_lifecycle_sigint_cancels_suspended_source_and_awaited_finally_runs_once() {
    let dir = tempfile::tempdir().unwrap();
    let body = dir.path().join("body");
    let cleanup = dir.path().join("cleanup");
    fifo(&body);
    fifo(&cleanup);
    let body_guard = held_writer(&body);
    let cleanup_guard = held_writer(&cleanup);
    let mut repl = Repl::new();
    setup(&mut repl);
    repl.send("(def cleaned 0)");
    repl.send(&format!("(def task (future (try (await (suss.io/read-byte {})) (finally (set! cleaned (+ cleaned 1)) (await (suss.io/read-byte {})) (set! cleaned (+ cleaned 1))))))", literal(&body), literal(&cleanup)));
    repl.send("(+ 7400 cleaned)");
    repl.line("7400");
    let mut body_writer = writer_only_after_guard(&mut repl, &body, body_guard);
    assert_eq!(
        unsafe { libc::kill(repl.child.id() as i32, libc::SIGINT) },
        0
    );
    // Signal delivery is asynchronous: wait for input acknowledgment before
    // queuing source that must execute after cancellation retirement.
    repl.wait("source cancellation acknowledged", |child| {
        child.out.lines().any(|line| line == "^C")
    });
    let mut cleanup_writer = writer_only_after_guard(&mut repl, &cleanup, cleanup_guard);
    // The child owns a real cleanup reader with no bytes. Queue later input,
    // then release its awaited read; cleanup must finish before source executes.
    repl.send("(+ 7500 cleaned)");
    cleanup_writer.write_all(&[9]).unwrap();
    repl.line("7502");
    assert_reader_retired(&mut repl, &body, &mut body_writer);
    assert_reader_retired(&mut repl, &cleanup, &mut cleanup_writer);
    repl.send("(+ 7600 cleaned)");
    repl.line("7602");
    repl.send("(+ 7700 3)");
    repl.line("7703");
    assert_eq!(repl.out.lines().filter(|line| *line == "7502").count(), 1);
    assert_eq!(repl.out.lines().filter(|line| *line == "7602").count(), 1);
    assert_eq!(repl.out.lines().filter(|line| *line == "^C").count(), 1);
    repl.finish();
    assert!(
        !repl.err.contains("Uncaught language exception"),
        "{}",
        repl.err
    );
}

#[test]
fn session_lifecycle_reset_retires_old_io_endpoint_before_new_session_accepts_input() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old");
    fifo(&old);
    let mut writer = held_writer(&old);
    let mut repl = Repl::new();
    setup(&mut repl);
    repl.send("(def old-effect 0)");
    repl.send(&format!(
        "(def old-task (future (set! old-effect (await (suss.io/read-byte {})))))",
        literal(&old)
    ));
    repl.send("(+ 7800 old-effect)");
    repl.line("7800");
    repl.send(":reset");
    repl.send("(+ 7900 4)");
    repl.line("7904");
    // Real data arriving after reset must belong to a fresh consumer, never an
    // old request. Keeping this endpoint open does not grant old Session handles
    // any right to settle in the replacement Store.
    writer.write_all(&[99]).unwrap();
    repl.send("(ns user (:require [suss.async :as async]) (:require-macros [suss.async :refer [future await]]))");
    repl.send("(def fresh-effect 0)");
    repl.send(&format!(
        "(def fresh-task (future (set! fresh-effect (await (suss.io/read-byte {}))) (throw 9201)))",
        literal(&old)
    ));
    repl.wait("fresh consumer owns late byte", |child| {
        child.err.contains("9201")
    });
    repl.send("(+ 8000 fresh-effect)");
    repl.line("8099");
    repl.finish();
    assert!(!repl.err.contains("ForeignValue"), "{}", repl.err);
    assert_eq!(
        repl.err
            .lines()
            .filter(|line| line.contains("9201"))
            .count(),
        1,
        "{}",
        repl.err
    );
}

#[test]
fn session_lifecycle_source_io_eof_is_nil_and_host_failure_is_catchable_without_losing_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("empty");
    File::create(&empty).unwrap();
    let missing = dir.path().join("missing");
    let mut repl = Repl::new();
    setup(&mut repl);
    repl.send("(def eof-seen 0) (def error-seen 0)");
    repl.send(&format!(
        "(def eof-task (future (set! eof-seen (if (nil? (await (suss.io/read-byte {}))) 1 -1))))",
        literal(&empty)
    ));
    // Both terminal outcomes are independently observed; this test checks
    // recoverability, while the sentinel test above proves idle progression.
    repl.send(&format!("(def failed-task (future (try (await (suss.io/read-byte {})) (catch :default reason (set! error-seen 1)))))", literal(&missing)));
    // Poll via source observations under a deadline; success requires effects,
    // never a fixed number of turns or elapsed wall-clock sleep.
    let deadline = Instant::now() + DEADLINE;
    loop {
        repl.send("(+ 8204 eof-seen error-seen)");
        repl.observe(Duration::from_millis(10));
        assert!(
            repl.child.try_wait().unwrap().is_none(),
            "child exited: {} {}",
            repl.out,
            repl.err
        );
        if repl.out.lines().any(|line| line == "8206") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "EOF/failure effects missing: {} {}",
            repl.out,
            repl.err
        );
    }
    repl.send("(+ 8300 7)");
    repl.line("8307");
    repl.finish();
    assert!(
        !repl.err.contains("Uncaught language exception"),
        "{}",
        repl.err
    );
}

#[test]
fn session_lifecycle_recovered_async_fuel_trap_reports_once_and_keeps_cli_prompt_usable() {
    // Session/bootstrap construction uses its installation budget; one million
    // is the explicit per-operation budget, ample for these small source forms.
    // No require-macros preparation is needed for this compiler-tag regression.
    let mut repl = Repl::with_fuel(Some(1_000_000));
    repl.send("(+ 9000 1)");
    repl.line("9001");
    assert!(repl.err.is_empty(), "startup failed: {}", repl.err);
    repl.send("(def effects 0)");
    repl.send("(def trapped (suss.async/future* (set! effects (+ effects 1)) ((fn [] (loop [] (recur))))))");
    // No next command can itself pump the failing turn: its primary fuel error
    // must arrive asynchronously while command stdin remains open and idle.
    repl.wait("primary asynchronous fuel error", |child| {
        child.err.to_ascii_lowercase().contains("fuel")
    });
    repl.send("trapped");
    repl.line("#<future failed>");
    repl.send("(+ 9400 effects)");
    repl.line("9401");
    repl.send("(+ 20 22)");
    repl.line("42");
    // A genuine unrelated language failure must not be swallowed by the
    // trapped-owner receipt. Only that exact marker owner's duplicate is hidden.
    repl.send("(def unrelated (suss.async/future* (throw 9351)))");
    repl.wait("independent language failure", |child| {
        child.err.contains("Uncaught language exception: 9351")
    });
    repl.send("(+ 9500 effects)");
    repl.line("9501");
    repl.finish();
    assert_eq!(
        repl.err
            .lines()
            .filter(|line| line.starts_with("Error: Runtime trap:"))
            .count(),
        1,
        "{}",
        repl.err
    );
    assert_eq!(
        repl.err
            .lines()
            .filter(|line| line.starts_with("Error: Uncaught language exception:"))
            .count(),
        1,
        "{}",
        repl.err
    );
    assert_eq!(
        repl.err
            .lines()
            .filter(|line| line.starts_with("Error:"))
            .count(),
        2,
        "{}",
        repl.err
    );
    assert!(
        !repl.err.contains("transported runtime trap marker"),
        "{}",
        repl.err
    );
    assert!(
        !repl.err.contains("Uncaught language exception: nil"),
        "{}",
        repl.err
    );
}

// Retain a writer while removing the O_RDWR setup guard so the child's read
// cannot see a transient EOF. A fresh writer open then proves a child reader:
// unlike POLLERR, ENXIO is the portable nonblocking FIFO ownership check.
fn writer_only_after_guard(repl: &mut Repl, path: &Path, guard: File) -> File {
    let writer = open(path, libc::O_WRONLY | libc::O_NONBLOCK).unwrap();
    drop(guard);
    let deadline = Instant::now() + DEADLINE;
    loop {
        match open(path, libc::O_WRONLY | libc::O_NONBLOCK) {
            Ok(probe) => {
                drop(probe);
                return writer;
            }
            Err(error) if error.raw_os_error() == Some(libc::ENXIO) => {}
            Err(error) => panic!("FIFO reader probe {}: {error}", path.display()),
        }
        assert!(
            Instant::now() < deadline,
            "child did not establish FIFO reader {}: {} {}",
            path.display(),
            repl.out,
            repl.err
        );
        repl.observe(Duration::from_millis(10));
        assert!(repl.child.try_wait().unwrap().is_none());
    }
}
fn assert_reader_retired(repl: &mut Repl, path: &Path, writer: &mut File) {
    let deadline = Instant::now() + DEADLINE;
    loop {
        match open(path, libc::O_WRONLY | libc::O_NONBLOCK) {
            Err(error) if error.raw_os_error() == Some(libc::ENXIO) => break,
            Err(error) => panic!("FIFO retirement probe {}: {error}", path.display()),
            Ok(probe) => drop(probe),
        }
        assert!(
            Instant::now() < deadline,
            "native FIFO reader did not retire: {} {}",
            repl.out,
            repl.err
        );
        repl.observe(Duration::from_millis(10));
    }
    assert_eq!(
        writer.write_all(&[99]).unwrap_err().raw_os_error(),
        Some(libc::EPIPE)
    );
}

#[test]
fn session_lifecycle_cli_public_stream_writer_awaits_real_io_and_reader_distinguishes_nil_data_from_eof(
) {
    let dir = tempfile::tempdir().unwrap();
    let body = dir.path().join("stream-input");
    fifo(&body);
    let guard = held_writer(&body);
    let mut repl = Repl::new();
    setup(&mut repl);
    repl.send("(def pair (async/stream-pair 2)) (def reader (nth pair 0)) (def writer (nth pair 1)) (def observed 0)");
    repl.send(&format!("(def producer (future (let [byte (await (suss.io/read-byte {}))] (await (async/write-chunk writer [nil byte])) (async/close! writer))))", literal(&body)));
    repl.send("(def consumer (future (let [chunk (await (async/read-chunk reader 2)) eof (await (async/read-chunk reader 2))] (set! observed (if (and (= chunk [nil 65]) (async/stream-eof? eof) (not (async/stream-eof? nil))) 1 -1)) (throw 9601))))");
    repl.send("(+ 11000 observed)");
    repl.line("11000");
    assert!(repl.err.is_empty(), "stream pending setup: {}", repl.err);
    let mut input_writer = writer_only_after_guard(&mut repl, &body, guard);
    input_writer.write_all(&[65]).unwrap();
    // No command input drives these stream continuations: a tracked failure
    // proves the reader consumed the actual chunk and the subsequent EOF.
    repl.wait("idle public stream consumer", |child| {
        child.err.contains("9601")
    });
    repl.send("(+ 11100 observed)");
    repl.line("11101");
    assert_reader_retired(&mut repl, &body, &mut input_writer);
    repl.send("(+ 11200 2)");
    repl.line("11202");
    repl.finish();
    assert_eq!(
        repl.err
            .lines()
            .filter(|line| line.starts_with("Error:"))
            .count(),
        1,
        "{}",
        repl.err
    );
    assert!(repl.err.contains("Uncaught language exception: 9601"));
}

fn submit_pending_streams(repl: &mut Repl, body: &Path, cleanup: &Path) {
    repl.send("(def pair (async/stream-pair 1)) (def reader (nth pair 0)) (def writer (nth pair 1)) (def cleaned 0)");
    repl.send(&format!("(def producer (future (try (await (async/write-chunk writer [(await (suss.io/read-byte {}))])) (finally (set! cleaned (+ cleaned 1)) (await (suss.io/read-byte {})) (set! cleaned (+ cleaned 1)) (async/close! writer)))))", literal(body), literal(cleanup)));
    repl.send("(def consumer (future (try (await (async/read-chunk reader 1)) (finally (set! cleaned (+ cleaned 1))))))");
}

#[test]
fn session_lifecycle_cli_sigint_cancels_pending_streams_awaits_finally_and_retires_native_fds() {
    let dir = tempfile::tempdir().unwrap();
    let body = dir.path().join("body");
    let cleanup = dir.path().join("cleanup");
    fifo(&body);
    fifo(&cleanup);
    let body_guard = held_writer(&body);
    let cleanup_guard = held_writer(&cleanup);
    let mut repl = Repl::new();
    setup(&mut repl);
    submit_pending_streams(&mut repl, &body, &cleanup);
    repl.send("(+ 12000 cleaned)");
    repl.line("12000");
    let mut body_writer = writer_only_after_guard(&mut repl, &body, body_guard);
    assert_eq!(
        unsafe { libc::kill(repl.child.id() as i32, libc::SIGINT) },
        0
    );
    repl.wait("stream cancellation acknowledged", |child| {
        child.out.lines().any(|line| line == "^C")
    });
    let mut cleanup_writer = writer_only_after_guard(&mut repl, &cleanup, cleanup_guard);
    // Awaited finally is genuinely pending on a separate FIFO, not considered
    // retired just because cancellation was requested. Later input is queued.
    repl.send("(+ 12100 cleaned)");
    cleanup_writer.write_all(&[7]).unwrap();
    repl.line("12103");
    assert_reader_retired(&mut repl, &body, &mut body_writer);
    assert_reader_retired(&mut repl, &cleanup, &mut cleanup_writer);
    repl.send("(+ 12200 cleaned)");
    repl.line("12203");
    repl.send("(+ 12300 3)");
    repl.line("12303");
    // Rechecking retired endpoints after later work catches accidental reopen.
    assert_reader_retired(&mut repl, &body, &mut body_writer);
    repl.finish();
    assert!(repl.err.is_empty(), "{}", repl.err);
}

#[test]
fn session_lifecycle_cli_reset_waits_for_pending_stream_cleanup_and_late_bytes_cannot_cross_stores()
{
    let dir = tempfile::tempdir().unwrap();
    let body = dir.path().join("body");
    let cleanup = dir.path().join("cleanup");
    fifo(&body);
    fifo(&cleanup);
    let body_guard = held_writer(&body);
    let cleanup_guard = held_writer(&cleanup);
    let mut repl = Repl::new();
    setup(&mut repl);
    submit_pending_streams(&mut repl, &body, &cleanup);
    repl.send("(+ 13000 cleaned)");
    repl.line("13000");
    let mut body_writer = writer_only_after_guard(&mut repl, &body, body_guard);
    repl.send(":reset");
    // A live cleanup read proves reset retained the original Store and pumped
    // its finally rather than dropping source stream operations prematurely.
    let mut cleanup_writer = writer_only_after_guard(&mut repl, &cleanup, cleanup_guard);
    repl.send("(+ 13100 4)");
    cleanup_writer.write_all(&[8]).unwrap();
    repl.line("13104");
    assert_reader_retired(&mut repl, &body, &mut body_writer);
    assert_reader_retired(&mut repl, &cleanup, &mut cleanup_writer);
    repl.send("(ns user (:require [suss.async :as async]) (:require-macros [suss.async :refer [future await]]))");
    repl.send("(def fresh (async/stream-pair 1)) (def fresh-reader (nth fresh 0)) (def fresh-writer (nth fresh 1)) (def fresh-value 0)");
    repl.send("(def fresh-producer (future (await (async/write-chunk fresh-writer [17])) (async/close! fresh-writer)))");
    repl.send("(def fresh-consumer (future (let [chunk (await (async/read-chunk fresh-reader 1)) eof (await (async/read-chunk fresh-reader 1))] (set! fresh-value (if (and (= chunk [17]) (async/stream-eof? eof)) 17 -1)) (throw 9701))))");
    repl.wait("fresh stream after retirement", |child| {
        child.err.contains("9701")
    });
    repl.send("(+ 13200 fresh-value)");
    repl.line("13217");
    repl.finish();
    assert_eq!(
        repl.err
            .lines()
            .filter(|line| line.starts_with("Error:"))
            .count(),
        1,
        "{}",
        repl.err
    );
    assert!(repl.err.contains("Uncaught language exception: 9701"));
}
