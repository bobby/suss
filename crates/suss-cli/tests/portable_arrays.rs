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
/// The string that `source` evaluates to.
fn eval_string(session: &mut Session, source: &str) -> Result<String, String> {
    let value = session.eval(source).map_err(|error| error.to_string())?;
    let units = session
        .inspect(&value, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(array.elems(&mut store)?.map(|v| v.unwrap_i32() as u16).collect::<Vec<_>>())
        })
        .unwrap();
    Ok(String::from_utf16(&units).unwrap())
}

/// Suss's arrays are Clojure's (#425): each case prints as JVM Clojure 1.12
/// printed it, as `scripts/jvm_array_oracle.py` recorded.
#[test]
fn arrays_match_jvm_clojure() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/jvm-array-cases.json")).unwrap();
    let mut session = Session::new_repl().unwrap();
    let mut failures = vec![];
    for case in corpus["cases"].as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        let expected = case["expected"].as_str().unwrap();
        match eval_string(&mut session, &format!("(pr-str (do {source}))")) {
            Ok(actual) if actual == expected => {}
            actual => failures.push(format!("{}: {source}\n  JVM:  {expected}\n  Suss: {actual:?}", case["id"])),
        }
        session.collect().unwrap();
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), corpus["cases"].as_array().unwrap().len(), failures.join("\n"));
}

#[test]
fn retained_arrays_functions_and_old_core_values_cross_fragments_after_gc() {
    let mut session = Session::new().unwrap();
    session
        .eval("(def old (let [n 40] (array (fn [x] (+ n x)))))")
        .unwrap();
    let array = session.eval("old").unwrap();
    let getter = session.eval("aget").unwrap();
    let zero = session.eval("0").unwrap();
    let two = session.eval("2").unwrap();
    session.eval("(def old nil)").unwrap();
    session.collect().unwrap();
    let function = session.invoke(&getter, &[&array, &zero]).unwrap();
    let result = session.invoke(&function, &[&two]).unwrap();
    assert_eq!(number(&mut session, &result), num(42.0));
    let creator = session.eval("array").unwrap();
    session.enter_namespace("suss.core").unwrap();
    session.eval("(def array (fn [x] 99))").unwrap();
    session.enter_namespace("user").unwrap();
    session.collect().unwrap();
    let result = session.invoke(&creator, &[&two]).unwrap();
    let item = session.invoke(&getter, &[&result, &zero]).unwrap();
    assert_eq!(number(&mut session, &item), num(2.0));
    // The qualified macro remains separate from a redefined runtime function.
    assert_eq!(
        eval_number(&mut session, "(alength (cljs.core/array 7 8))"),
        num(2.0)
    );
}

#[test]
fn array_bounds_arity_and_errors_preserve_session_recovery() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    for source in [
        "(let [f alength] (f))",
        "(let [f aclone] (f nil nil))",
        "(let [f aget] (f (array)))",
        "(let [f aset] (f (array) 0))",
        "(let [n -1] (make-array n))",
        "(make-array -1)",
        "(aget (array 1) 1)",
        "(aset (array 1) 1 2)",
        // A negative inner size throws though the outer one is empty, as on the JVM.
        "(make-array nil 0 -1)",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Language(_))),
            "{source}"
        );
        assert_eq!(eval_number(&mut session, "7"), num(7.0));
    }
    // Any int length is allowed, as on the JVM.
    assert_eq!(eval_number(&mut session, "(let [n 3000000] (alength (make-array n)))"), num(3_000_000.0));
    for source in [
        "(alength)",
        "(aget (array))",
        "(aset (array) 0)",
        "(make-array)",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Compile(_))),
            "{source}"
        );
    }
    assert_eq!(eval_number(&mut session, "(alength (make-array 1000001))"), num(1_000_001.0));
}

#[test]
fn qualified_array_macros_and_clone_aliases_survive_gc() {
    let mut session = Session::new().unwrap();
    session
        .eval("(ns review.arrays (:require [cljs.core :as c]))")
        .unwrap();
    session.eval("(def a (c/array (c/array 7) nil))").unwrap();
    session.eval("(def copy (aclone a))").unwrap();
    session.eval("(def alias a)").unwrap();
    session.eval("(def a nil)").unwrap();
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "(c/aset alias 1 42)"), num(42.0));
    assert_eq!(eval_number(&mut session, "(c/alength alias)"), num(2.0));
    assert_eq!(eval_number(&mut session, "(c/alength copy)"), num(2.0));
    assert_eq!(eval_number(&mut session, "(if (nil? (c/aget copy 1)) 1 0)"), num(1.0));
    // The clone is shallow: both arrays hold the same inner array.
    session.eval("(c/aset copy 0 0 19)").unwrap();
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "(c/aget alias 0 0)"), num(19.0));
    // Arrays don't grow.
    assert_eq!(eval_number(&mut session, "(try (c/aset alias 2 1) (catch :default e 5))"), num(5.0));
    assert_eq!(eval_number(&mut session, "(c/alength alias)"), num(2.0));
    // A local runtime var hides only the unqualified auto-referred macro.
    session.eval("(def alength (fn [a] 99))").unwrap();
    assert_eq!(eval_number(&mut session, "(alength alias)"), num(99.0));
    assert_eq!(eval_number(&mut session, "(c/alength alias)"), num(2.0));
}
