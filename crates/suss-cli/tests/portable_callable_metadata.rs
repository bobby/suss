use suss_cli::portable_session::{Session, SessionValue};

/// An exact runtime number: a long, or a double by its bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Number {
    Long(i64),
    Double(u64),
}
/// The expected number for `value`: integral values are longs, as in Clojure.
fn num(value: f64) -> Number {
    if value.is_finite()
        && value.fract() == 0.0
        && !(value == 0.0 && value.is_sign_negative())
        && value.abs() < 9_223_372_036_854_775_808.0
    {
        Number::Long(value as i64)
    } else {
        Number::Double(value.to_bits())
    }
}
fn exact_number(store: impl wasmtime::AsContextMut, value: &wasmtime::Val) -> wasmtime::Result<Number> {
    Ok(
        match suss_compile::portable_session::runtime_number(store, value)?
            .expect("expected a runtime number")
        {
            suss_compile::portable_session::RuntimeNumber::Long(value) => Number::Long(value),
            suss_compile::portable_session::RuntimeNumber::Double(value) => {
                Number::Double(value.to_bits())
            }
        },
    )
}

fn number(session: &mut Session, value: &SessionValue) -> Number {
    session
        .inspect(value, |mut store, value| {
            exact_number(&mut store, &value)
        })
        .unwrap()
}
fn eval_number(session: &mut Session, source: &str) -> Number {
    let value = session.eval(source).unwrap();
    number(session, &value)
}
fn session() -> Session {
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session
}

#[test]
fn ordinary_metafn_calls_preserve_captured_functions_and_metadata_after_gc() {
    let mut session = session();
    session
        .eval("(def original (fn [x] (+ x 1))) (def wrapped (with-meta original {:doc 7}))")
        .unwrap();
    assert_eq!(eval_number(&mut session, "(wrapped 16)"), num(17.0));
    assert_eq!(
        eval_number(&mut session, "(get (meta wrapped) :doc)"),
        num(7.0)
    );
    session.eval("(def original (fn [x] (+ x 100)))").unwrap();
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "(wrapped 18)"), num(19.0));
    assert_eq!(
        eval_number(&mut session, "(original 18)"),
        num(118.0)
    );
    session
        .eval("(def rewrapped (with-meta wrapped {:doc 11}))")
        .unwrap();
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(rewrapped 22)"),
        num(23.0)
    );
    assert_eq!(
        eval_number(&mut session, "(get (meta rewrapped) :doc)"),
        num(11.0)
    );
    assert_eq!(
        eval_number(&mut session, "(get (meta wrapped) :doc)"),
        num(7.0)
    );
}

#[test]
fn function_literal_metadata_evaluates_lexical_values_and_keeps_callable_body() {
    let mut session = session();
    session
        .eval("(def annotated (let [answer 41] ^{:answer answer} (fn [x] (+ x 1))))")
        .unwrap();
    assert_eq!(
        eval_number(&mut session, "(get (meta annotated) :answer)"),
        num(41.0)
    );
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(annotated 42)"),
        num(43.0)
    );
}

#[test]
fn ordinary_collection_calls_use_canonical_ifn_methods() {
    let mut session = session();
    assert_eq!(eval_number(&mut session, "([17 19] 1)"), num(19.0));
    assert_eq!(eval_number(&mut session, "({:x 23} :x)"), num(23.0));
    assert_eq!(
        eval_number(&mut session, "({:x 23} :missing 29)"),
        num(29.0)
    );
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(let [lookup {:x 31}] (lookup :x))"),
        num(31.0)
    );
}


#[test]
fn ordinary_callable_wrong_arity_runs_operands_once_and_recovers() {
    let mut session = session();
    session.eval("(def lookup [1]) (def call-effects 0)").unwrap();
    assert!(matches!(session.eval("(lookup (do (set! call-effects 1) 0) (do (set! call-effects (+ call-effects 10)) 1) (do (set! call-effects (+ call-effects 100)) 2))"), Err(suss_cli::portable_session::SessionError::Language(_))));
    assert_eq!(eval_number(&mut session, "call-effects"), num(111.0));
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "(lookup 0)"), num(1.0));
}
