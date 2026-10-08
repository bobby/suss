//! Executing native-profile factories; controllers perform no source evaluation.
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use suss_compile::portable_session::{
    FutureState, FutureStatus, Session, SessionError, SessionOptions, SessionValue,
};
use suss_reader::Symbol;

fn number(session: &mut Session, value: &SessionValue) -> f64 {
    session
        .inspect(value, |mut store, value| {
            let value = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            Ok(value.field(&mut store, 0)?.unwrap_f64())
        })
        .unwrap()
}

#[test]
fn source_factory_queues_owned_arguments_and_completion_never_resumes_inline() {
    let mut session = Session::new().unwrap();
    let queue = session
        .install_native_future_factory(Symbol::namespaced("native.factory", "read"), 1)
        .unwrap();
    session.eval("(ns app (:require [native.factory :as host])) (def trace 0) (def task (suss.async/future* (let [x (suss.async/await* (host/read 7))] (set! trace (+ trace x)) x)))").unwrap();
    assert_eq!(
        queue.pending(),
        0,
        "install and source declaration start no I/O"
    );
    assert!(session.run_async_turn().unwrap());
    assert!(!session.run_async_turn().unwrap());
    let mut requests = queue.drain();
    assert_eq!(requests.len(), 1);
    let request = requests.pop().unwrap();
    assert!(!request.cancelled && !request.is_cancelled());
    assert_eq!(number(&mut session, &request.args[0]), 7.0);
    assert_eq!(
        session.future_status(&request.future).unwrap(),
        Some(FutureStatus::Pending)
    );
    assert_eq!(session.stats().pending_host_requests, 1);
    session.collect().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let hook = calls.clone();
    request
        .set_cancel_hook(move || {
            hook.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
    let payload = session.eval("42").unwrap();
    assert!(session.resolve_future(&request.future, &payload).unwrap());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "native publication disarms cancellation before storage mutation"
    );
    let trace = session.eval("trace").unwrap();
    assert_eq!(number(&mut session, &trace), 0.0);
    assert!(!session.reject_future(&request.future, &payload).unwrap());
    assert!(session.run_async_turn().unwrap());
    let task = session.eval("task").unwrap();
    let Some(FutureState::Ready(result)) = session.future_state(&task).unwrap() else {
        panic!("task did not resolve")
    };
    assert_eq!(number(&mut session, &result), 42.0);
    assert_eq!(session.stats().pending_host_requests, 0);
    assert!(!session.run_async_turn().unwrap());
}

#[test]
fn reset_reinstalls_profile_same_controller_with_fresh_roots_and_rejects_late_requests() {
    let mut session = Session::new().unwrap();
    let queue = session
        .install_native_future_factory(Symbol::namespaced("native.factory", "read"), 1)
        .unwrap();
    session.eval("(native.factory/read 9)").unwrap();
    let old = queue.drain().pop().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let hook = calls.clone();
    old.set_cancel_hook(move || {
        hook.fetch_add(1, Ordering::SeqCst);
    })
    .unwrap();
    session.eval("(native.factory/read 10)").unwrap(); // Queued roots must also be drained.
    assert_eq!(queue.pending(), 1);
    session.reset().unwrap();
    assert!(old.is_cancelled());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(queue.pending(), 0);
    assert_eq!(session.stats().pending_host_requests, 0);
    let payload = session.eval("11").unwrap();
    assert!(matches!(
        session.resolve_future(&old.future, &payload),
        Err(SessionError::ForeignValue)
    ));
    assert!(matches!(
        session.future_status(&old.args[0]),
        Err(SessionError::ForeignValue)
    ));
    session.eval("(native.factory/read 12)").unwrap();
    let new = queue.drain().pop().unwrap();
    assert_ne!(old.id.session_identity, new.id.session_identity);
    assert!(new.id.generation > old.id.generation);
    assert!(!new.is_cancelled());
    assert_eq!(number(&mut session, &new.args[0]), 12.0);
    assert!(session.resolve_future(&new.future, &payload).unwrap());
    session.reset().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn source_cancellation_sweeps_factory_registry_and_invokes_attached_hook_once() {
    let mut session = Session::new().unwrap();
    let queue = session
        .install_native_future_factory(Symbol::namespaced("native.factory", "read"), 0)
        .unwrap();
    let future = session.eval("(native.factory/read)").unwrap();
    let request = queue.drain().pop().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let hook = calls.clone();
    request
        .set_cancel_hook(move || {
            hook.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
    let cancel = session
        .eval("(fn [f] (suss.internal.async/cancel! f))")
        .unwrap();
    session.invoke(&cancel, &[&future]).unwrap();
    assert!(!session.run_async_turn().unwrap());
    assert!(request.is_cancelled());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(session.stats().pending_host_requests, 0);
    session.collect().unwrap();
    session.reset().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn native_profile_is_runtime_only_and_filesystem_absence_only_and_bounded() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("native")).unwrap();
    std::fs::write(
        root.path().join("native/factory.sus"),
        "(ns native.factory)",
    )
    .unwrap();
    let mut session = Session::with_options(SessionOptions {
        source_paths: vec![root.path().to_owned()],
        ..Default::default()
    })
    .unwrap();
    assert!(
        session
            .install_native_future_factory(Symbol::namespaced("native.factory", "read"), 1)
            .is_err()
    );
    std::fs::remove_file(root.path().join("native/factory.sus")).unwrap();
    std::fs::create_dir(root.path().join("native/factory.sus")).unwrap();
    assert!(
        session
            .install_native_future_factory(Symbol::namespaced("native.factory", "read"), 1)
            .is_err()
    );
    let mut macros = Session::new_macro().unwrap();
    assert!(
        macros
            .install_native_future_factory(Symbol::namespaced("native.factory", "read"), 1)
            .is_err()
    );
    let mut session = Session::new().unwrap();
    let queue = session
        .install_native_future_factory(Symbol::namespaced("native.factory", "read"), 0)
        .unwrap();
    for _ in 0..64 {
        session.eval("(native.factory/read)").unwrap();
    }
    assert!(session.eval("(native.factory/read)").is_err());
    assert_eq!(queue.pending(), 64);
    assert_eq!(session.stats().pending_host_requests, 64);
    session.reset().unwrap();
    assert_eq!(queue.pending(), 0);
}

#[test]
fn second_factory_cannot_replace_source_binding_in_its_native_namespace() {
    let mut session = Session::new().unwrap();
    session
        .install_native_future_factory(Symbol::namespaced("native.factory", "read"), 0)
        .unwrap();
    session
        .eval("(ns native.factory) (def existing 37)")
        .unwrap();
    assert!(
        session
            .install_native_future_factory(Symbol::namespaced("native.factory", "existing"), 0)
            .is_err()
    );
    let existing = session.eval("existing").unwrap();
    assert_eq!(number(&mut session, &existing), 37.0);
    session
        .install_native_future_factory(Symbol::namespaced("native.factory", "other"), 0)
        .unwrap();
}

#[test]
fn genuine_native_exception_constructor_ignores_redefined_source_cells() {
    let mut session = Session::new().unwrap();
    session.eval("(ns suss.core) (def intercepted 0) (def ExceptionInfo (fn [message data cause] (set! intercepted (+ intercepted 1)) nil)) (def ex-info (fn [message data] (set! intercepted (+ intercepted 10)) nil))").unwrap();
    let message = session.eval("\"native failure\"").unwrap();
    let data = session.eval("37").unwrap();
    let exception = session.native_exception_info(&message, &data).unwrap();
    assert_eq!(session.future_status(&exception).unwrap(), None);
    session.collect().unwrap();
    session
        .inspect(&exception, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object
                .field(&mut store, 1)?
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            assert_eq!(fields.len(&store)?, 3);
            let text = fields
                .get(&mut store, 0)?
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            let units = text
                .elems(&mut store)?
                .map(|v| v.unwrap_i32() as u16)
                .collect::<Vec<_>>();
            assert_eq!(String::from_utf16(&units).unwrap(), "native failure");
            let data = fields
                .get(&mut store, 1)?
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap();
            assert_eq!(data.field(&mut store, 0)?.unwrap_f64(), 37.0);
            assert_eq!(
                fields
                    .get(&mut store, 2)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .unwrap()
                    .get_u32(),
                0
            );
            Ok(())
        })
        .unwrap();
    let intercepted = session.eval("intercepted").unwrap();
    assert_eq!(number(&mut session, &intercepted), 0.0);
    let mut other = Session::new().unwrap();
    let foreign = other.eval("nil").unwrap();
    assert!(matches!(
        session.native_exception_info(&foreign, &data),
        Err(SessionError::ForeignValue)
    ));
    assert!(matches!(
        session.native_exception_info(&message, &foreign),
        Err(SessionError::ForeignValue)
    ));
}

#[test]
fn frontend_host_cancellation_waits_for_source_retirement_then_consumes_hook_once() {
    let mut session = Session::new().unwrap();
    let queue = session
        .install_native_future_factory(Symbol::namespaced("native.factory", "read"), 0)
        .unwrap();
    session.eval("(def trace 0) (def task (suss.async/future* (try (suss.async/await* (native.factory/read)) (finally (set! trace (+ trace 1))))))").unwrap();
    assert!(session.run_async_turn().unwrap());
    let request = queue.drain().pop().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let hook = calls.clone();
    request
        .set_cancel_hook(move || {
            hook.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
    assert!(matches!(
        session.request_cancel_pending_host_requests(),
        Err(SessionError::ResetPending)
    ));
    assert!(!request.is_cancelled());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let task = session.eval("task").unwrap();
    assert!(session.cancel_future(&task).unwrap());
    assert!(session.run_async_turn().unwrap());
    assert_eq!(
        session.future_status(&task).unwrap(),
        Some(FutureStatus::Cancelled)
    );
    assert_eq!(session.async_task_counts().unwrap(), (0, 0, 0));
    let trace = session.eval("trace").unwrap();
    assert_eq!(number(&mut session, &trace), 1.0);
    session.request_cancel_pending_host_requests().unwrap();
    assert!(request.is_cancelled());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(session.stats().pending_host_requests, 0);
    session.request_cancel_pending_host_requests().unwrap();
    session.collect().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
