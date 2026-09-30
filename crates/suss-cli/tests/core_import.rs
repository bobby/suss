//! Execute the generated, provenance-checked upstream bootstrap form.
use suss_cli::portable_session::{Session, SessionError, SessionValue};
use wasmtime::{Rooted, Val};

const CORE: &str = include_str!("../../../runtime/core-import/suss/core.sus");

fn loaded() -> Session {
    let mut session = Session::new().unwrap();
    assert!(matches!(
        session.eval("(identity 7)"),
        Err(SessionError::Compile(_))
    ));
    session.eval(CORE).unwrap();
    session.enter_namespace("user").unwrap();
    session
}

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

#[test]
fn imported_identity_preserves_exact_values_and_object_identity_across_gc() {
    let mut session = loaded();
    session.eval("(deftype Item [value])").unwrap();
    let identity = session.eval("identity").unwrap();
    for source in [
        "nil",
        "false",
        "true",
        "-0.0",
        "##NaN",
        "##Inf",
        "\"\\uD800😀\"",
        "(fn [] 42)",
        "(Item. 7)",
        "(ex-info \"saved\" 9)",
    ] {
        let original = session.eval(source).unwrap();
        let root = session
            .inspect(&original, |store, value| {
                value.unwrap_anyref().unwrap().to_owned_rooted(store)
            })
            .unwrap();
        session.collect().unwrap();
        let returned = session.invoke(&identity, &[&original]).unwrap();
        session.collect().unwrap();
        assert!(
            session
                .inspect(&returned, |store, value| Rooted::ref_eq(
                    &store,
                    &root,
                    value.unwrap_anyref().unwrap()
                ))
                .unwrap(),
            "{source}"
        );
    }
}

#[test]
fn imported_core_aliases_and_captured_function_observe_live_redefinition() {
    let mut session = loaded();
    let original = session.eval("identity").unwrap();
    let root = session
        .inspect(&original, |store, value| {
            value.unwrap_anyref().unwrap().to_owned_rooted(store)
        })
        .unwrap();
    for source in [
        "cljs.core/identity",
        "suss.core/identity",
        "(let [f identity] f)",
    ] {
        let alias = session.eval(source).unwrap();
        assert!(
            session
                .inspect(&alias, |store, value| Rooted::ref_eq(
                    &store,
                    &root,
                    value.unwrap_anyref().unwrap()
                ))
                .unwrap()
        );
    }
    session
        .eval("(ns app (:require [cljs.core :as core])) (def call (fn [x] (core/identity x)))")
        .unwrap();
    session.enter_namespace("suss.core").unwrap();
    session.eval("(def identity (fn [x] 9))").unwrap();
    session.enter_namespace("app").unwrap();
    session.collect().unwrap();
    let value = session.eval("(call 7)").unwrap();
    assert_eq!(number(&mut session, &value), 9.0f64.to_bits());
    let arg = session.eval("7").unwrap();
    let value = session.invoke(&original, &[&arg]).unwrap();
    assert_eq!(number(&mut session, &value), 7.0f64.to_bits());
}

#[test]
fn imported_identity_evaluates_arguments_once_and_recovers_from_wrong_arity() {
    let mut session = loaded();
    session
        .eval("(def count 0) (def next (fn [] (def count (+ count 1))))")
        .unwrap();
    let value = session.eval("(identity (next))").unwrap();
    assert_eq!(number(&mut session, &value), 1.0f64.to_bits());
    assert!(matches!(
        session.eval("(identity (next) (next))"),
        Err(SessionError::Language(_))
    ));
    let value = session.eval("count").unwrap();
    assert_eq!(number(&mut session, &value), 3.0f64.to_bits());
    assert!(matches!(
        session.eval("(identity)"),
        Err(SessionError::Language(_))
    ));
    let value = session.eval("(identity 42)").unwrap();
    assert_eq!(number(&mut session, &value), 42.0f64.to_bits());
}

#[test]
fn imported_core_matches_independently_decoded_pinned_scalar_corpus() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/core-import-cases.json")).unwrap();
    let mut session = loaded();
    let mut ids = std::collections::BTreeSet::new();
    for case in corpus["cases"].as_array().unwrap() {
        assert!(ids.insert(case["id"].as_str().unwrap()));
        let source = case["source"].as_str().unwrap();
        let value = session.eval(source).unwrap();
        session.collect().unwrap();
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "f64" => {
                serde_json::json!({"tag":"f64", "bits":format!("{:016x}", number(&mut session, &value))})
            }
            "nil" | "bool" => {
                let sentinel = session
                    .inspect(&value, |store, value| {
                        Ok(value
                            .unwrap_anyref()
                            .unwrap()
                            .as_i31(&store)?
                            .unwrap()
                            .get_u32())
                    })
                    .unwrap();
                match sentinel {
                    0 => serde_json::json!({"tag":"nil"}),
                    2 => serde_json::json!({"tag":"bool", "value":false}),
                    4 => serde_json::json!({"tag":"bool", "value":true}),
                    _ => panic!("unexpected sentinel {sentinel}"),
                }
            }
            "string" => {
                let units = session
                    .inspect(&value, |mut store, value| {
                        let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                        Ok(array
                            .elems(&mut store)?
                            .map(|v| v.unwrap_i32() as u16)
                            .collect::<Vec<_>>())
                    })
                    .unwrap();
                serde_json::json!({"tag":"string", "units":units})
            }
            tag => panic!("unsupported expected tag {tag}"),
        };
        assert_eq!(actual, case["expected"], "{source}");
    }
    assert_eq!(ids.len(), 14);
}
