use suss_cli::portable_session::{Session, SessionValue};
use wasmtime::Val;

fn number(session: &mut Session, value: &SessionValue) -> u64 {
    session.inspect(value, |mut store, value| {
        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
        let fields = object.fields(&mut store)?.collect::<Vec<_>>();
        let [Val::F64(bits)] = fields.as_slice() else { panic!("Number layout") };
        Ok(*bits)
    }).unwrap()
}
fn eval_number(session: &mut Session, source: &str) -> u64 {
    let value = session.eval(source).unwrap();
    number(session, &value)
}
fn session() -> Session {
    let mut session = Session::new().unwrap();
    session.eval(include_str!("../../../runtime/core-import/suss/core.sus")).unwrap();
    session.enter_namespace("user").unwrap();
    session
}

#[test]
fn ordinary_metafn_calls_preserve_captured_functions_and_metadata_after_gc() {
    let mut session = session();
    session.eval("(def original (fn [x] (+ x 1))) (def wrapped (with-meta original {:doc 7}))").unwrap();
    assert_eq!(eval_number(&mut session, "(wrapped 16)"), 17.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "(get (meta wrapped) :doc)"), 7.0f64.to_bits());
    session.eval("(def original (fn [x] (+ x 100)))").unwrap();
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "(wrapped 18)"), 19.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "(original 18)"), 118.0f64.to_bits());
    session.eval("(def rewrapped (with-meta wrapped {:doc 11}))").unwrap();
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "(rewrapped 22)"), 23.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "(get (meta rewrapped) :doc)"), 11.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "(get (meta wrapped) :doc)"), 7.0f64.to_bits());
}

#[test]
fn function_literal_metadata_evaluates_lexical_values_and_keeps_callable_body() {
    let mut session = session();
    session.eval("(def annotated (let [answer 41] ^{:answer answer} (fn [x] (+ x 1))))").unwrap();
    assert_eq!(eval_number(&mut session, "(get (meta annotated) :answer)"), 41.0f64.to_bits());
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "(annotated 42)"), 43.0f64.to_bits());
}

#[test]
fn ordinary_collection_calls_use_canonical_ifn_methods() {
    let mut session = session();
    assert_eq!(eval_number(&mut session, "([17 19] 1)"), 19.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "({:x 23} :x)"), 23.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "({:x 23} :missing 29)"), 29.0f64.to_bits());
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "(let [lookup {:x 31}] (lookup :x))"), 31.0f64.to_bits());
}
