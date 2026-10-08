//! A receipt suppresses only the exact task whose trap was already reported.
use suss_compile::portable_session::{FutureState, Session, SessionError, SessionOptions};

#[test]
fn recovered_receipt_identifies_only_trapped_owner_and_clears_on_next_turn() {
    let mut session = Session::with_options(SessionOptions {
        fuel_per_operation: 50_000,
        ..Default::default()
    })
    .unwrap();
    session.eval("(def unrelated (suss.async/future* (throw 7))) (def trapped (suss.async/future* ((fn [] (loop [] (recur))))))").unwrap();
    assert!(!session.last_async_turn_recovered());
    assert!(session.run_async_turn().unwrap());
    assert!(!session.last_async_turn_recovered());
    let error = session.run_async_turn().unwrap_err();
    assert!(matches!(error, SessionError::Trap(_)));
    assert!(session.last_async_turn_recovered());
    let receipt = session.last_async_turn_recovered_owner().unwrap();
    let trapped = session.eval("trapped").unwrap();
    let unrelated = session.eval("unrelated").unwrap();
    assert!(session.values_identical(&receipt, &trapped).unwrap());
    assert!(!session.values_identical(&receipt, &unrelated).unwrap());
    let Some(FutureState::Failed(payload)) = session.future_state(&trapped).unwrap() else {
        panic!("trapped owner not Failed")
    };
    assert!(session.is_async_runtime_trap(&payload).unwrap());
    let Some(FutureState::Failed(other_payload)) = session.future_state(&unrelated).unwrap() else {
        panic!("unrelated owner not Failed")
    };
    assert!(!session.is_async_runtime_trap(&other_payload).unwrap());
    let mut foreign = Session::new().unwrap();
    let foreign_nil = foreign.eval("nil").unwrap();
    assert!(matches!(
        session.is_async_runtime_trap(&foreign_nil),
        Err(SessionError::ForeignValue)
    ));
    session.collect().unwrap();
    assert!(session.last_async_turn_recovered());
    assert!(!session.run_async_turn().unwrap());
    assert!(!session.last_async_turn_recovered());
    assert!(session.last_async_turn_recovered_owner().is_none());
    session.reset().unwrap();
    assert!(!session.last_async_turn_recovered());
}

#[test]
fn preclaim_fuel_trap_does_not_forge_a_recovery_receipt() {
    // Zero-budget turn traps before an active resume can be claimed. Recovery
    // may prune idempotently, but it did not fail an owner and has no receipt.
    let mut session = Session::with_options(SessionOptions {
        fuel_per_operation: 0,
        ..Default::default()
    })
    .unwrap();
    assert!(session.run_async_turn().is_err());
    assert!(!session.last_async_turn_recovered());
    assert!(session.last_async_turn_recovered_owner().is_none());
}
