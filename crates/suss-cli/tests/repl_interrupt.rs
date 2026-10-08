//! Ctrl-C (SIGINT) interrupts the running REPL evaluation and returns to the
//! prompt; the session keeps accepting input (design section 7; #15).
#![cfg(unix)]
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

#[test]
fn sigint_interrupts_running_evaluation_and_the_repl_continues() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_suss"))
        // An unbounded budget: only an interrupt can end the loop below.
        .args(["repl", "--fuel", &u64::MAX.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let (tx, lines) = mpsc::channel();
    for (stream, reader) in [
        ("out", Box::new(child.stdout.take().unwrap()) as Box<dyn std::io::Read + Send>),
        ("err", Box::new(child.stderr.take().unwrap())),
    ] {
        let tx = tx.clone();
        thread::spawn(move || {
            for line in BufReader::new(reader).lines().map_while(Result::ok) {
                let _ = tx.send((stream, line));
            }
        });
    }
    drop(tx);
    let deadline = Instant::now() + Duration::from_secs(600);
    let mut seen = Vec::new();
    let mut wait_for = |wanted: &dyn Fn(&(&str, String)) -> bool, timeout: Duration| -> bool {
        let until = Instant::now() + timeout;
        while Instant::now() < until.min(deadline) {
            if let Ok(line) = lines.recv_timeout(Duration::from_millis(100)) {
                let found = wanted(&line);
                seen.push(line);
                if found {
                    return true;
                }
            }
        }
        false
    };
    // Readiness: the compiled core is provisioned and the first answer printed.
    writeln!(stdin, "(+ 1 2)").unwrap();
    assert!(wait_for(&|(stream, line)| *stream == "out" && line == "3", Duration::from_secs(600)));
    // A signal that arrives before evaluation starts is discarded; keep sending.
    writeln!(stdin, "(loop [] (recur))\n(+ 40 2)\n:quit").unwrap();
    stdin.flush().unwrap();
    let pid = Pid::from_raw(i32::try_from(child.id()).unwrap());
    let mut interrupted = false;
    while !interrupted && Instant::now() < deadline {
        kill(pid, Signal::SIGINT).unwrap();
        interrupted = wait_for(
            &|(stream, line)| *stream == "err" && line.contains("Interrupted"),
            Duration::from_millis(300),
        );
    }
    assert!(interrupted, "no interruption reported: {seen:?}");
    assert!(wait_for(&|(stream, line)| *stream == "out" && line == "42", Duration::from_secs(120)), "{seen:?}");
    assert!(child.wait().unwrap().success(), "{seen:?}");
    assert!(
        !seen.iter().any(|(_, line)| line.contains("Runtime trap")),
        "the loop must be interrupted, not exhaust its fuel: {seen:?}"
    );
}
