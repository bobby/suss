//! Native reset must retain unknown-producer cleanup until it can finish.
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use suss_compile::portable_session::{Session, SessionError};

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
        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
        assert_eq!(object.field(&mut store, 0)?.unwrap_f64(), 1.0);
        Ok(())
    }).unwrap();
    session.reset().unwrap();
    assert_eq!(closed.load(Ordering::SeqCst), 1);
    assert!(matches!(session.resolve_future(&cleanup, &payload), Err(SessionError::ForeignValue)));
    assert!(matches!(session.cancel_future(&task), Err(SessionError::ForeignValue)));
    assert!(!session.run_async_turn().unwrap());
}
