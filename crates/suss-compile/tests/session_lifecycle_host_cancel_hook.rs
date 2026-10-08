//! Source cancellation must release the native producer, without resetting Store.
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use suss_compile::portable_session::Session;

#[test]
fn source_cancelled_host_completion_runs_native_cancel_hook_once() {
    let mut session = Session::new().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let cancelled = calls.clone();
    let pending = session
        .pending_future_with_cancel(move || {
            cancelled.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
    let cancel = session
        .eval("(fn [value] (suss.internal.async/cancel! value))")
        .unwrap();
    session.invoke(&cancel, &[&pending]).unwrap();
    assert!(
        !session.run_async_turn().unwrap(),
        "an idle frontend turn services source producer cancellation"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "source cancellation must cancel its native producer"
    );
    assert_eq!(session.stats().pending_host_requests, 0);
    session.collect().unwrap();
    session.reset().unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "collection and reset cannot replay the consumed hook"
    );
}

#[test]
fn source_settlement_retires_pending_producer_but_native_completion_does_not_cancel() {
    for operation in ["resolve!", "reject!"] {
        let mut session = Session::new().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let producer = calls.clone();
        let future = session
            .pending_future_with_cancel(move || {
                producer.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();
        let settle = session
            .eval(&format!(
                "(fn [value] (suss.internal.async/{operation} value 42))"
            ))
            .unwrap();
        session.invoke(&settle, &[&future]).unwrap();
        session.collect().unwrap();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "source {operation} retires its superseded producer"
        );
        session.reset().unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    for reject in [false, true] {
        let mut session = Session::new().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let producer = calls.clone();
        let future = session
            .pending_future_with_cancel(move || {
                producer.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();
        let payload = session.eval("42").unwrap();
        assert!(
            if reject {
                session.reject_future(&future, &payload)
            } else {
                session.resolve_future(&future, &payload)
            }
            .unwrap()
        );
        session.collect().unwrap();
        session.reset().unwrap();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "normal native completion must not invoke cancellation"
        );
    }
}

#[test]
fn interrupted_native_completion_never_cancels_its_finished_producer_on_retry() {
    for reject in [false, true] {
        let mut session = Session::new().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let producer = calls.clone();
        let future = session
            .pending_future_with_cancel(move || {
                producer.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();
        let payload = session.eval("42").unwrap();
        session.set_operation_fuel(35);
        let result = if reject {
            session.reject_future(&future, &payload)
        } else {
            session.resolve_future(&future, &payload)
        };
        assert!(result.is_err(), "bounded publication must run out of fuel");
        session.set_operation_fuel(1_000_000);
        session.collect().unwrap();
        // If fuel stopped before ownership transfer, the native operation has
        // not reached completion; retry completion transfers it before reset.
        let _ = if reject {
            session.reject_future(&future, &payload)
        } else {
            session.resolve_future(&future, &payload)
        }
        .unwrap();
        session.reset().unwrap();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "native completion cannot be reclassified as source cancellation"
        );
    }
}
