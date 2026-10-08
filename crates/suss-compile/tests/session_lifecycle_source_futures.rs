//! Source-future entry gates from accepted design section 9; not full async acceptance.
use suss_compile::portable_macros::CompiledMacros;
use suss_compile::portable_session::{Session, SessionError, SessionValue};

fn async_session() -> (Session, CompiledMacros) {
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    session.eval_with_macros(
        "(ns source-future-gates (:require [suss.async]) (:require-macros [suss.async :refer [future]]))",
        &mut macros,
    ).expect("the shipped async library must load in both declared phases");
    (session, macros)
}

fn is_false(session: &mut Session, value: &SessionValue) -> bool {
    session
        .inspect(value, |store, value| {
            Ok(value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .is_some_and(|value| value.get_u32() == 2))
        })
        .unwrap()
}

#[test]
fn session_lifecycle_source_future_does_not_synchronously_return_its_body_number() {
    let (mut session, mut macros) = async_session();
    let result = session
        .eval_with_macros("(number? (suss.async/future 42))", &mut macros)
        .expect("a source future form must compile without flattening into its body number");
    session.collect().unwrap();
    assert!(is_false(&mut session, &result));
}

#[test]
fn session_lifecycle_source_future_contains_body_language_failure() {
    let (mut session, mut macros) = async_session();
    let result = session
        .eval_with_macros("(number? (suss.async/future (throw 17)))", &mut macros)
        .expect("a future body failure must not become the caller's synchronous throw");
    session.collect().unwrap();
    assert!(is_false(&mut session, &result));
    let recovered = session.eval("42").unwrap();
    session.collect().unwrap();
    session
        .inspect(&recovered, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            assert_eq!(
                object.field(&mut store, 0)?.unwrap_f64().to_bits(),
                42.0f64.to_bits()
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn session_lifecycle_await_outside_future_is_a_contextual_compile_error() {
    let (mut session, mut macros) = async_session();
    let error = session
        .eval_with_macros("(suss.async/await 42)", &mut macros)
        .expect_err("await outside a suspendable body must fail compilation");
    let SessionError::Compile(error) = error else {
        panic!("await context rejection must be a located compile diagnostic");
    };
    assert!(error.span.start < error.span.end);
    assert!(
        error.message.contains("await")
            && error.message.contains("inside")
            && error.message.contains("future"),
        "context diagnostic: {}",
        error.message
    );
}

#[test]
fn session_lifecycle_scheduler_runs_compiled_body_once_and_recovers_from_trap() {
    let (mut session, mut macros) = async_session();
    session.eval("(def trace 0)").unwrap();
    session.eval_with_macros("(suss.async/future (set! trace (+ trace 1)) 42)", &mut macros).unwrap();
    assert!(session.run_async_turn().unwrap());
    assert!(!session.run_async_turn().unwrap());
    let trace = session.eval("trace").unwrap();
    session.inspect(&trace, |mut store, value| {
        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
        assert_eq!(object.field(&mut store, 0)?.unwrap_f64(), 1.0);
        Ok(())
    }).unwrap();
    session.eval_with_macros("(suss.async/future (set! trace (+ trace 1)) ((fn [] (loop [] (recur)))))", &mut macros).unwrap();
    session.set_operation_fuel(50_000);
    assert!(matches!(session.run_async_turn(), Err(SessionError::Trap(_))));
    assert!(!session.run_async_turn().unwrap());
    session.eval_with_macros("(suss.async/future (set! trace (+ trace 1)))", &mut macros).unwrap();
    assert!(session.run_async_turn().unwrap());
    let trace = session.eval("trace").unwrap();
    session.inspect(&trace, |mut store, value| {
        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
        assert_eq!(object.field(&mut store, 0)?.unwrap_f64(), 3.0);
        Ok(())
    }).unwrap();
}

#[test]
fn session_lifecycle_cancel_request_before_start_skips_body_and_checks_ownership() {
    let (mut session, mut macros) = async_session();
    session.eval("(def trace 0)").unwrap();
    let task = session.eval_with_macros("(suss.async/future (set! trace 99))", &mut macros).unwrap();
    assert!(session.cancel_future(&task).unwrap());
    assert!(!session.cancel_future(&task).unwrap());
    assert!(session.run_async_turn().unwrap());
    assert!(!session.run_async_turn().unwrap());
    let trace = session.eval("trace").unwrap();
    session.inspect(&trace, |mut store, value| {
        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
        assert_eq!(object.field(&mut store, 0)?.unwrap_f64(), 0.0);
        Ok(())
    }).unwrap();
    let mut other = Session::new().unwrap();
    assert!(matches!(other.cancel_future(&task), Err(SessionError::ForeignValue)));
    session.reset().unwrap();
    assert!(matches!(session.cancel_future(&task), Err(SessionError::ForeignValue)));
}

#[test]
fn session_lifecycle_host_completion_queues_waiter_without_inline_resumption() {
    let (mut session, mut macros) = async_session();
    session.eval("(def trace 0)").unwrap();
    let make = session.eval_with_macros("(fn [dependency] (suss.async/future (suss.async/await* dependency) (set! trace (+ trace 1))))", &mut macros).unwrap();
    let pending = session.pending_future().unwrap();
    let task = session.invoke(&make, &[&pending]).unwrap();
    assert!(session.run_async_turn().unwrap());
    assert!(!session.run_async_turn().unwrap());
    session.collect().unwrap();
    let payload = session.eval("42").unwrap();
    assert!(session.resolve_future(&pending, &payload).unwrap());
    assert!(!session.reject_future(&pending, &payload).unwrap());
    let trace = session.eval("trace").unwrap();
    session.inspect(&trace, |mut store, value| {
        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
        assert_eq!(object.field(&mut store, 0)?.unwrap_f64(), 0.0);
        Ok(())
    }).unwrap();
    assert!(session.run_async_turn().unwrap());
    assert!(!session.run_async_turn().unwrap());
    let trace = session.eval("trace").unwrap();
    session.inspect(&trace, |mut store, value| {
        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
        assert_eq!(object.field(&mut store, 0)?.unwrap_f64(), 1.0);
        Ok(())
    }).unwrap();
    drop(task);
    session.reset().unwrap();
    assert!(matches!(session.resolve_future(&pending, &payload), Err(SessionError::ForeignValue)));
}

#[test]
fn session_lifecycle_host_rejection_is_caught_and_foreign_payload_is_rejected() {
    let (mut session, mut macros) = async_session();
    session.eval("(def trace 0)").unwrap();
    let make = session.eval_with_macros("(fn [dependency] (suss.async/future (try (suss.async/await* dependency) (catch :default payload (set! trace payload)))))", &mut macros).unwrap();
    let pending = session.pending_future().unwrap();
    let task = session.invoke(&make, &[&pending]).unwrap();
    assert!(session.run_async_turn().unwrap());
    let mut other = Session::new().unwrap();
    let foreign = other.eval("99").unwrap();
    assert!(matches!(session.reject_future(&pending, &foreign), Err(SessionError::ForeignValue)));
    assert!(matches!(session.resolve_future(&pending, &foreign), Err(SessionError::ForeignValue)));
    assert!(!session.run_async_turn().unwrap());
    let payload = session.eval("17").unwrap();
    assert!(session.reject_future(&pending, &payload).unwrap());
    assert!(!session.resolve_future(&pending, &payload).unwrap());
    session.collect().unwrap();
    assert!(session.run_async_turn().unwrap());
    assert!(!session.run_async_turn().unwrap());
    let trace = session.eval("trace").unwrap();
    session.inspect(&trace, |mut store, value| {
        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
        assert_eq!(object.field(&mut store, 0)?.unwrap_f64(), 17.0);
        Ok(())
    }).unwrap();
    drop(task);
}

#[test]
fn session_lifecycle_host_settlement_cannot_bypass_pending_task_cleanup() {
    let (mut session, mut macros) = async_session();
    session.eval("(def trace 0)").unwrap();
    let make = session.eval_with_macros("(fn [dependency cleanup] (suss.async/future (try (suss.async/await* dependency) (finally (suss.async/await* cleanup) (set! trace (+ trace 1))))))", &mut macros).unwrap();
    let dependency = session.pending_future().unwrap();
    let cleanup = session.pending_future().unwrap();
    let task = session.invoke(&make, &[&dependency, &cleanup]).unwrap();
    assert!(session.run_async_turn().unwrap());
    assert!(session.cancel_future(&task).unwrap());
    assert!(session.run_async_turn().unwrap());
    let payload = session.eval("42").unwrap();
    assert!(matches!(session.resolve_future(&task, &payload), Err(SessionError::Language(_))));
    assert!(matches!(session.reject_future(&task, &payload), Err(SessionError::Language(_))));
    assert!(!session.run_async_turn().unwrap());
    session.collect().unwrap();
    assert!(session.resolve_future(&cleanup, &payload).unwrap());
    assert!(session.run_async_turn().unwrap());
    assert!(!session.run_async_turn().unwrap());
    let trace = session.eval("trace").unwrap();
    session.inspect(&trace, |mut store, value| {
        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
        assert_eq!(object.field(&mut store, 0)?.unwrap_f64(), 1.0);
        Ok(())
    }).unwrap();
}
