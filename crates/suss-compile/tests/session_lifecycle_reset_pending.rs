//! Native reset must retain unknown-producer cleanup until it can finish.
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use suss_compile::portable_session::{Session, SessionError};

/// A long, as Clojure's integer values are.
fn long_value(store: impl wasmtime::AsContextMut, value: &wasmtime::Val) -> wasmtime::Result<i64> {
    match suss_compile::portable_session::runtime_number(store, value)? {
        Some(suss_compile::portable_session::RuntimeNumber::Long(value)) => Ok(value),
        other => Err(wasmtime::Error::msg(format!("expected a long, observed {other:?}"))),
    }
}


#[test]
fn session_lifecycle_reset_pending_retains_cleanup_and_cancels_host_request_once() {
    let mut session = Session::new().unwrap();
    session.eval("(def effects 0)").unwrap();
    let closed = Arc::new(AtomicUsize::new(0));
    let observed = closed.clone();
    let dependency = session.pending_future_with_cancel(move || { observed.fetch_add(1, Ordering::SeqCst); }).unwrap();
    // Source completion is deliberately not a Session-owned host request.
    let cleanup = session.eval("(suss.internal.async/pending)").unwrap();
    let make = session.eval("(fn [dependency cleanup] (suss.async/future* (try (suss.async/await* dependency) (finally (suss.async/await* cleanup) (set! effects (+ effects 1))))))").unwrap();
    let task = session.invoke(&make, &[&dependency, &cleanup]).unwrap();
    assert!(session.run_async_turn().unwrap());
    assert!(matches!(session.reset(), Err(SessionError::ResetPending)));
    assert_eq!(closed.load(Ordering::SeqCst), 1);
    assert!(matches!(session.reset(), Err(SessionError::ResetPending)));
    assert_eq!(closed.load(Ordering::SeqCst), 1);
    let payload = session.eval("42").unwrap();
    assert!(session.resolve_future(&cleanup, &payload).unwrap());
    assert!(session.run_async_turn().unwrap());
    let effects = session.eval("effects").unwrap();
    session.inspect(&effects, |mut store, value| {
        assert_eq!(long_value(&mut store, &value)?, 1);
        Ok(())
    }).unwrap();
    session.reset().unwrap();
    assert_eq!(closed.load(Ordering::SeqCst), 1);
    assert!(matches!(session.resolve_future(&cleanup, &payload), Err(SessionError::ForeignValue)));
    assert!(matches!(session.cancel_future(&task), Err(SessionError::ForeignValue)));
    assert!(!session.run_async_turn().unwrap());
}
