use suss_cli::portable_session::{Session, SessionValue};
use wasmtime::Val;

fn number(session: &mut Session, value: &SessionValue) -> u64 {
    session
        .inspect(value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Number layout")
            };
            Ok(*bits)
        })
        .unwrap()
}
fn eval_number(session: &mut Session, source: &str) -> u64 {
    let value = session.eval(source).unwrap();
    number(session, &value)
}
fn eval_bool(session: &mut Session, source: &str) -> bool {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |store, value| {
            match value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32()
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
                serde_json::json!({"tag":"f64", "bits":format!("{:016x}",eval_number(&mut session, source))})
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
    assert_eq!(number(&mut session, &result), 42.0f64.to_bits());
    let creator = session.eval("array").unwrap();
    session.enter_namespace("suss.core").unwrap();
    session.eval("(def array (fn [x] 99))").unwrap();
    session.enter_namespace("user").unwrap();
    session.collect().unwrap();
    let result = session.invoke(&creator, &[&two]).unwrap();
    let item = session.invoke(&getter, &[&result, &zero]).unwrap();
    assert_eq!(number(&mut session, &item), 2.0f64.to_bits());
    // The qualified macro remains separate from a redefined runtime function.
    assert_eq!(
        eval_number(&mut session, "(alength (cljs.core/array 7 8))"),
        2.0f64.to_bits()
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
        "(let [n 1000001] (make-array n))",
        "(make-array nil 2000 2000)",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Language(_))),
            "{source}"
        );
        assert_eq!(eval_number(&mut session, "7"), 7.0f64.to_bits());
    }
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
        0.0f64.to_bits()
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
        42.0f64.to_bits()
    );
    assert_eq!(
        eval_number(&mut session, "(c/alength alias)"),
        4.0f64.to_bits()
    );
    assert_eq!(
        eval_number(&mut session, "(c/alength copy)"),
        1.0f64.to_bits()
    );
    assert!(eval_bool(&mut session, "(undefined? (c/aget alias 2))"));
    session.eval("(c/aset copy 0 0 19)").unwrap();
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(c/aget alias 0 0)"),
        19.0f64.to_bits()
    );
    // A local runtime var hides only the unqualified auto-referred macro.
    session.eval("(def alength (fn [a] 99))").unwrap();
    assert_eq!(
        eval_number(&mut session, "(alength alias)"),
        99.0f64.to_bits()
    );
    assert_eq!(
        eval_number(&mut session, "(c/alength alias)"),
        4.0f64.to_bits()
    );
}
