//! A real secondary interrupt after consuming a native reset cancellation hook.
//! No bootstrap, sleeps, synthetic trap returns or replayed source effects.
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use suss_compile::portable_session::{Session, SessionError};

struct ReadGuard(Arc<AtomicUsize>);
impl Drop for ReadGuard {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn session_lifecycle_reset_interrupt_after_host_hook_preserves_pending_root_and_retries_once() {
    let mut session = Session::new().unwrap();
    session.eval("(def keep 17)").unwrap();
    let old = session.eval("(fn [] keep)").unwrap();
    let payload = session.eval("42").unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let called = calls.clone();
    let guard = ReadGuard(drops.clone());
    let interrupt = session.interrupt_handle().clone();
    let pending = session
        .pending_future_with_cancel(move || {
            called.fetch_add(1, Ordering::SeqCst);
            // This consumes the real hook, raises the engine epoch, and leaves
            // the request for the immediately following future-cancel entry.
            interrupt.interrupt();
            drop(guard);
        })
        .unwrap();
    let before = session.stats();
    assert_eq!(before.pending_host_requests, 1);

    let error = session
        .reset()
        .expect_err("real cancellation entry must be interrupted");
    assert!(
        error.is_interrupt(),
        "reset must retain the actual interrupt: {error:?}"
    );
    assert!(matches!(&error, SessionError::Trap(error)
        if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::Interrupt)));
    assert_eq!(
        session.stats(),
        before,
        "reset must preserve the old Store and roots"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(drops.load(Ordering::SeqCst), 1);

    // Independently inspect the pinned future storage, without settling it or
    // invoking another cancellation hook. The interrupt precedes publication.
    session
        .inspect(&pending, |mut store, value| {
            assert!(!store.has_pending_exception());
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let storage = object.field(&mut store, 1)?;
            let storage = storage.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let state = storage.get(&mut store, 0)?;
            let state = state.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let status = state.get(&mut store, 0)?;
            let status = status.unwrap_anyref().unwrap().as_i31(&store)?.unwrap();
            assert_eq!(
                status.get_u32(),
                0,
                "interrupted settlement leaves future Pending"
            );
            Ok(())
        })
        .unwrap();

    // A normal operation refreshes the budget/epoch deadline. The retained old
    // closure is usable, proving generation identity has not been replaced.
    session.set_operation_fuel(1_000_000);
    let kept = session.invoke(&old, &[]).unwrap();
    session
        .inspect(&kept, |mut store, value| {
            let number = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            assert_eq!(number.field(&mut store, 0)?.unwrap_f64(), 17.0);
            Ok(())
        })
        .unwrap();
    assert_eq!(session.stats().pending_host_requests, 1);

    session.reset().unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "retry must not replay the consumed hook"
    );
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(session.stats().pending_host_requests, 0);
    assert_eq!(session.stats().external_value_handles, 0);
    assert!(matches!(
        session.resolve_future(&pending, &payload),
        Err(SessionError::ForeignValue)
    ));
    assert!(matches!(
        session.reject_future(&pending, &payload),
        Err(SessionError::ForeignValue)
    ));
    assert!(matches!(
        session.invoke(&old, &[]),
        Err(SessionError::ForeignValue)
    ));
    assert!(!session.run_async_turn().unwrap());
    session.reset().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}
