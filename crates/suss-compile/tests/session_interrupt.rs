//! Interrupting running code (design section 7: explicit cancellation; #15).
//! An interrupt traps only the requesting session's current operation. Effects
//! before the trap are not rolled back, the session accepts the next input, and
//! other sessions sharing the engine keep running.
#[path = "support/portable_decode.rs"]
mod portable_decode;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use portable_decode::{Decoder, Observation};
use suss_compile::portable_macros::CompiledMacros;
use suss_compile::portable_session::{
    InterruptHandle, Session, SessionError, SessionOptions, SessionValue,
};

const SPIN: &str = "(loop [] (recur))";

fn unbounded() -> SessionOptions {
    SessionOptions {
        fuel_per_operation: u64::MAX,
        ..SessionOptions::default()
    }
}

/// Repeatedly request interruption until `operation` returns, so a request
/// that arrives before the operation starts (and is discarded) cannot hang.
fn interrupted<T>(handle: InterruptHandle, operation: impl FnOnce() -> T) -> T {
    let done = Arc::new(AtomicBool::new(false));
    let interrupter = {
        let done = done.clone();
        thread::spawn(move || {
            while !done.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(50));
                handle.interrupt();
            }
        })
    };
    let result = operation();
    done.store(true, Ordering::SeqCst);
    interrupter.join().unwrap();
    result
}

fn is_interrupt(result: &Result<SessionValue, SessionError>) -> bool {
    matches!(result, Err(error) if error.is_interrupt() && error.to_string() == "Interrupted")
}

fn number(session: &mut Session, source: &str) -> f64 {
    let mut decoder = Decoder::capture(session, 1_000).unwrap();
    let value = session.eval(source).unwrap_or_else(|error| panic!("{source}: {error}"));
    match decoder.decode_session(session, &value).unwrap() {
        Observation::Number(bits) => f64::from_bits(bits),
        other => panic!("{source}: {other:?}"),
    }
}

#[test]
fn interrupt_stops_running_code_without_rolling_back_effects() {
    let mut session = Session::new_repl_with_options(unbounded()).unwrap();
    session.eval("(def progress (atom 0))").unwrap();
    let result = interrupted(session.interrupt_handle(), || {
        session.eval("(loop [] (swap! progress inc) (recur))")
    });
    assert!(is_interrupt(&result), "{:?}", result.map(|_| ()));
    // Effects before the interrupt remain; the session accepts the next input.
    assert!(number(&mut session, "(deref progress)") > 0.0);
    assert_eq!(number(&mut session, "(+ 1 2)"), 3.0);
}

#[test]
fn an_idle_interrupt_does_not_cancel_the_next_operation() {
    let mut session = Session::new_repl().unwrap();
    session.interrupt_handle().interrupt();
    assert_eq!(number(&mut session, "(+ 40 2)"), 42.0);
}

#[test]
fn interrupting_one_session_leaves_another_running() {
    let mut other = Session::new_repl_with_options(unbounded()).unwrap();
    let other_handle = other.interrupt_handle();
    let running = thread::spawn(move || other.eval(SPIN).map(|_| ()));
    let mut session = Session::new_repl_with_options(unbounded()).unwrap();
    let result = interrupted(session.interrupt_handle(), || session.eval(SPIN));
    assert!(is_interrupt(&result));
    // Every request advanced the shared engine epoch; the other Store continued.
    thread::sleep(Duration::from_millis(200));
    assert!(!running.is_finished(), "the other session must still be running");
    let other = interrupted(other_handle, || running.join().unwrap());
    assert!(matches!(&other, Err(error) if error.is_interrupt()), "{other:?}");
}

#[test]
fn interrupt_handles_survive_reset() {
    let mut session = Session::new_repl_with_options(unbounded()).unwrap();
    let handle = session.interrupt_handle();
    session.reset().unwrap();
    let result = interrupted(handle, || session.eval(SPIN));
    assert!(is_interrupt(&result));
    assert_eq!(number(&mut session, "(+ 1 2)"), 3.0);
}

#[test]
fn interrupt_stops_a_running_macro_expansion() {
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    macros.set_operation_fuel(u64::MAX);
    macros.define("(defmacro spin [] (loop [] (recur)))").unwrap();
    let result = interrupted(macros.interrupt_handle(), || {
        session.eval_with_macros("(spin)", &mut macros)
    });
    let error = result.map(|_| ()).expect_err("interrupted expansion").to_string();
    assert_eq!(error, "Compiled macro expansion failed: Interrupted at bytes 0..6");
    // Both phases accept the next input.
    macros.define("(defmacro answer [] 42)").unwrap();
    let value = session.eval_with_macros("(answer)", &mut macros).unwrap();
    let mut decoder = Decoder::capture(&mut session, 1_000).unwrap();
    assert_eq!(
        decoder.decode_session(&mut session, &value).unwrap(),
        Observation::Number(42f64.to_bits())
    );
}
