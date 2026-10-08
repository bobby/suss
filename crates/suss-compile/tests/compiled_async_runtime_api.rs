//! Execute the shipped source API, rather than asserting only Wasm encoding.
use suss_compile::{portable_macros::CompiledMacros, portable_session::Session};

fn session() -> (Session, CompiledMacros) {
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    session.eval_with_macros("(ns async-api-test (:require [suss.async]) (:require-macros [suss.async :refer [future await]]))", &mut macros).unwrap();
    (session, macros)
}

fn assert_true(session: &mut Session, source: &str) {
    let value = session.eval(source).unwrap();
    session.collect().unwrap();
    session
        .inspect(&value, |store, value| {
            assert_eq!(
                value
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .unwrap()
                    .get_u32(),
                4,
                "{source}"
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn compiled_async_ready_failed_nil_and_nested_payload_survive_collection() {
    let (mut s, _) = session();
    s.eval("(def inner (suss.async/resolved nil)) (def outer (suss.async/resolved inner)) (def failed (suss.async/rejected nil))").unwrap();
    s.collect().unwrap();
    assert_true(&mut s, "(== 1 (suss.internal.async/status inner))");
    assert_true(&mut s, "(nil? (suss.internal.async/result inner))");
    assert_true(
        &mut s,
        "(identical? inner (suss.internal.async/result outer))",
    );
    assert_true(&mut s, "(== 2 (suss.internal.async/status failed))");
    assert_true(&mut s, "(nil? (suss.internal.async/result failed))");
}

#[test]
fn compiled_async_completion_first_terminal_wins() {
    let (mut s, _) = session();
    s.eval("(def done (suss.async/completion)) (def failed (suss.async/completion)) (def cancelled (suss.async/completion))").unwrap();
    assert_true(&mut s, "(== 0 (suss.internal.async/status done))");
    assert_true(&mut s, "(suss.async/resolve! done 42)");
    assert_true(&mut s, "(false? (suss.async/reject! done 99))");
    assert_true(&mut s, "(false? (suss.async/cancel! done))");
    assert_true(&mut s, "(== 42 (suss.internal.async/result done))");
    assert_true(&mut s, "(suss.async/reject! failed 17)");
    assert_true(&mut s, "(false? (suss.async/resolve! failed 99))");
    assert_true(&mut s, "(== 17 (suss.internal.async/result failed))");
    assert_true(&mut s, "(suss.async/cancel! cancelled)");
    assert_true(&mut s, "(false? (suss.async/cancel! cancelled))");
    assert_true(&mut s, "(false? (suss.async/resolve! cancelled nil))");
    assert_true(&mut s, "(== 3 (suss.internal.async/status cancelled))");
}

#[test]
fn compiled_async_completion_queues_waiter_without_inline_execution() {
    let (mut s, mut macros) = session();
    s.eval("(def trace 0) (def completion (suss.async/completion))")
        .unwrap();
    s.eval_with_macros(
        "(def task (suss.async/future (suss.async/await completion) (set! trace 42)))",
        &mut macros,
    )
    .unwrap();
    assert!(s.run_async_turn().unwrap());
    assert!(!s.run_async_turn().unwrap());
    assert_true(&mut s, "(suss.async/resolve! completion nil)");
    assert_true(&mut s, "(== trace 0)");
    assert!(s.run_async_turn().unwrap());
    assert!(!s.run_async_turn().unwrap());
    assert_true(&mut s, "(== trace 42)");
}

#[test]
fn compiled_async_task_cancel_and_invalid_operands_recover() {
    let (mut s, mut macros) = session();
    s.eval("(def trace 0)").unwrap();
    s.eval_with_macros(
        "(def task (suss.async/future (set! trace 99)))",
        &mut macros,
    )
    .unwrap();
    assert!(s.eval("(suss.async/resolve! task 17)").is_err());
    assert_true(&mut s, "(suss.async/cancel! task)");
    assert_true(&mut s, "(false? (suss.async/cancel! task))");
    assert_true(&mut s, "(== trace 0)");
    assert!(s.run_async_turn().unwrap());
    assert!(!s.run_async_turn().unwrap());
    assert_true(&mut s, "(== trace 0)");
    assert_true(&mut s, "(== 3 (suss.internal.async/status task))");
    for source in [
        "(suss.async/cancel! nil)",
        "(suss.async/resolve! 42 nil)",
        "(suss.async/reject! 42 nil)",
    ] {
        assert!(
            s.eval(source).is_err(),
            "invalid operand must fail: {source}"
        );
        assert_true(&mut s, "(== 42 42)");
    }
}

#[test]
fn host_settlement_cannot_bypass_cancelled_tasks_pending_cleanup() {
    use suss_compile::portable_session::SessionError;
    let (mut s, mut macros) = session();
    s.eval("(def trace 0) (def dependency (suss.async/completion)) (def cleanup (suss.async/completion))").unwrap();
    let task = s.eval_with_macros("(def task (suss.async/future (try (suss.async/await dependency) (finally (suss.async/await cleanup) (set! trace (+ trace 1))))))", &mut macros).unwrap();
    // A def result need not be its value; obtain the rooted task explicitly.
    drop(task);
    let task = s.eval("task").unwrap();
    assert!(s.run_async_turn().unwrap());
    assert!(!s.run_async_turn().unwrap());
    assert_true(&mut s, "(suss.async/cancel! task)");
    assert!(s.run_async_turn().unwrap());
    assert!(!s.run_async_turn().unwrap());
    s.collect().unwrap();
    assert_true(&mut s, "(== 0 (suss.internal.async/status task))");
    let payload = s.eval("99").unwrap();
    assert!(matches!(
        s.resolve_future(&task, &payload),
        Err(SessionError::Language(_))
    ));
    assert!(matches!(
        s.reject_future(&task, &payload),
        Err(SessionError::Language(_))
    ));
    assert_true(&mut s, "(== 0 (suss.internal.async/status task))");
    assert_true(&mut s, "(== trace 0)");
    assert_true(&mut s, "(suss.async/resolve! cleanup nil)");
    assert_true(&mut s, "(== trace 0)");
    assert!(s.run_async_turn().unwrap());
    assert!(!s.run_async_turn().unwrap());
    assert_true(&mut s, "(== trace 1)");
    assert_true(&mut s, "(== 3 (suss.internal.async/status task))");
    assert!(!s.resolve_future(&task, &payload).unwrap());
    assert!(!s.reject_future(&task, &payload).unwrap());
}
