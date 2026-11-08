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
/// Whether `actual` matches a pinned `f64` observation. The ClojureScript
/// corpus cannot distinguish longs: an integral observation is a Clojure long,
/// or an integral double when the source has a double operand.
fn matches_pinned(actual: Number, expected: &serde_json::Value, source: &str) -> bool {
    let bits = u64::from_str_radix(expected["bits"].as_str().unwrap(), 16).unwrap();
    match actual {
        Number::Long(_) => actual == num(f64::from_bits(bits)),
        Number::Double(actual) => {
            actual == bits
                && (num(f64::from_bits(bits)) == Number::Double(bits)
                    || source.contains("##")
                    || source.as_bytes().windows(3).any(|w| {
                        w[0].is_ascii_digit()
                            && (w[1] == b'.' || w[1] == b'e')
                            && (w[2].is_ascii_digit() || w[2] == b'-')
                    }))
        }
    }
}
/// The pinned observation when `actual` matches it, otherwise a distinct value.
fn number_observation(actual: Number, expected: &serde_json::Value, source: &str) -> serde_json::Value {
    if matches_pinned(actual, expected, source) {
        expected.clone()
    } else {
        serde_json::json!({ "tag": "number", "actual": format!("{actual:?}") })
    }
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
fn eval_bool(session: &mut Session, source: &str) -> bool {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |mut store, value| {
            match suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap()
            {
                2 => Ok(false),
                4 => Ok(true),
                other => panic!("Boolean {other}"),
            }
        })
        .unwrap()
}

fn check_array_cases(range: std::ops::Range<usize>) {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/array-cases.json")).unwrap();
    let mut session = Session::new().unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for case in &corpus["cases"].as_array().unwrap()[range.clone()] {
        assert!(ids.insert(case["id"].as_str().unwrap()));
        let source = case["source"].as_str().unwrap();
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "bool" => serde_json::json!({"tag":"bool", "value":eval_bool(&mut session, source)}),
            "f64" => {
                number_observation(eval_number(&mut session, source), &case["expected"], source)
            }
            tag => panic!("unexpected tag {tag}"),
        };
        assert_eq!(actual, case["expected"], "{source}");
        session.collect().unwrap();
    }
    assert_eq!(ids.len(), range.len());
}

#[test]
fn arrays_match_independently_encoded_primary_corpus() {
    check_array_cases(0..56);
}
#[test]
fn array_scalar_storage_identity_and_missing_slots() {
    check_array_cases(0..11);
}
#[test]
fn array_mutation_growth_and_shallow_clone() {
    check_array_cases(11..20);
}
#[test]
fn array_make_literal_dynamic_and_nested_dimensions() {
    check_array_cases(20..29);
}
#[test]
fn array_first_class_functions_and_native_protocols() {
    check_array_cases(29..36);
}
#[test]
fn array_argument_order_and_retained_function_values() {
    check_array_cases(36..40);
}

#[test]
fn array_macro_allocation_and_error_effects() {
    check_array_cases(40..56);
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
        "(make-array nil 2000 2000 2000)",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Language(_))),
            "{source}"
        );
        assert_eq!(eval_number(&mut session, "7"), num(7.0));
    }
    assert_eq!(eval_number(&mut session, "(let [n 1000001] (alength (make-array n)))"), num(1_000_001.0));
    for source in [
        "(alength)",
        "(aget (array))",
        "(aset (array) 0)",
        "(make-array)",
        "(make-array 1000001)",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Compile(_))),
            "{source}"
        );
    }
    // An outer empty dimension does not allocate or validate inaccessible leaves.
    assert_eq!(
        eval_number(&mut session, "(alength (make-array nil 0 -1))"),
        num(0.0)
    );
}

#[test]
fn qualified_array_macros_and_clone_aliases_survive_gc_and_growth() {
    let mut session = Session::new().unwrap();
    session
        .eval("(ns review.arrays (:require [cljs.core :as c]))")
        .unwrap();
    session.eval("(def a (c/array (c/array 7)))").unwrap();
    session.eval("(def copy (aclone a))").unwrap();
    session.eval("(def alias a)").unwrap();
    session.eval("(def a nil)").unwrap();
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(c/aset alias 3 42)"),
        num(42.0)
    );
    assert_eq!(
        eval_number(&mut session, "(c/alength alias)"),
        num(4.0)
    );
    assert_eq!(
        eval_number(&mut session, "(c/alength copy)"),
        num(1.0)
    );
    assert!(eval_bool(&mut session, "(undefined? (c/aget alias 2))"));
    session.eval("(c/aset copy 0 0 19)").unwrap();
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(c/aget alias 0 0)"),
        num(19.0)
    );
    // A local runtime var hides only the unqualified auto-referred macro.
    session.eval("(def alength (fn [a] 99))").unwrap();
    assert_eq!(
        eval_number(&mut session, "(alength alias)"),
        num(99.0)
    );
    assert_eq!(
        eval_number(&mut session, "(c/alength alias)"),
        num(4.0)
    );
}
