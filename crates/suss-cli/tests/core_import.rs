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
    assert_eq!(ids.len(), 50);
}

fn boolean(session: &mut Session, value: &SessionValue) -> bool {
    session
        .inspect(value, |store, value| {
            match value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32()
            {
                2 => Ok(false),
                4 => Ok(true),
                sentinel => panic!("unexpected Boolean {sentinel}"),
            }
        })
        .unwrap()
}

#[test]
fn imported_boolean_ports_use_cljs_truthiness_for_primitives_and_objects() {
    let mut session = loaded();
    session.eval("(deftype Item [value])").unwrap();
    for (source, truth) in [
        ("nil", false),
        ("false", false),
        ("true", true),
        ("0", true),
        ("-0.0", true),
        ("##NaN", true),
        ("##Inf", true),
        ("\"\"", true),
        ("\"\\uD800😀\"", true),
        ("(fn [] 42)", true),
        ("(Item. 7)", true),
        ("(ex-info \"m\" 7)", true),
        ("(ex-data (new ExceptionInfo \"m\"))", false),
        ("(ExceptionInfo)", true),
    ] {
        for (name, expected) in [("boolean", truth), ("not", !truth)] {
            let value = session.eval(&format!("({name} {source})")).unwrap();
            session.collect().unwrap();
            assert_eq!(boolean(&mut session, &value), expected, "{name}: {source}");
        }
    }
}

#[test]
fn imported_boolean_ports_keep_original_behavior_and_canonical_live_bindings() {
    let mut session = loaded();
    let old_not = session.eval("not").unwrap();
    let old_boolean = session.eval("boolean").unwrap();
    for name in ["not", "boolean"] {
        let original = session.eval(name).unwrap();
        let root = session
            .inspect(&original, |store, value| {
                value.unwrap_anyref().unwrap().to_owned_rooted(store)
            })
            .unwrap();
        let alias = session.eval(&format!("cljs.core/{name}")).unwrap();
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
    session.eval("(def read (fn [x] (not x)))").unwrap();
    session.enter_namespace("suss.core").unwrap();
    session
        .eval("(def nil? (fn [x] false)) (def false? (fn [x] false))")
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.collect().unwrap();
    let value = session.eval("(not nil)").unwrap();
    assert!(boolean(&mut session, &value));
    let value = session.eval("(boolean false)").unwrap();
    assert!(!boolean(&mut session, &value));
    session.enter_namespace("suss.core").unwrap();
    session
        .eval("(def not (fn [x] false)) (def boolean (fn [x] false))")
        .unwrap();
    session.enter_namespace("user").unwrap();
    let value = session.eval("(read nil)").unwrap();
    assert!(!boolean(&mut session, &value));
    let nil = session.eval("nil").unwrap();
    let truthy = session.eval("0").unwrap();
    session.collect().unwrap();
    let value = session.invoke(&old_not, &[&nil]).unwrap();
    assert!(boolean(&mut session, &value));
    let value = session.invoke(&old_boolean, &[&truthy]).unwrap();
    assert!(boolean(&mut session, &value));
}

#[test]
fn imported_boolean_port_arguments_run_once_before_arity_errors() {
    let mut session = loaded();
    session
        .eval("(def count 0) (def next (fn [] (def count (+ count 1))))")
        .unwrap();
    for name in ["not", "boolean"] {
        let value = session.eval(&format!("({name} (next))")).unwrap();
        assert_eq!(boolean(&mut session, &value), name == "boolean");
        assert!(matches!(
            session.eval(&format!("({name} (next) (next))")),
            Err(SessionError::Language(_))
        ));
        assert!(matches!(
            session.eval(&format!("({name})")),
            Err(SessionError::Language(_))
        ));
    }
    let count = session.eval("count").unwrap();
    assert_eq!(number(&mut session, &count), 6.0f64.to_bits());
    let value = session.eval("(not nil)").unwrap();
    assert!(boolean(&mut session, &value));
}

#[test]
fn imported_boolean_callee_is_captured_before_argument_rebinding_and_core_reload() {
    let mut session = loaded();
    session
        .eval("(ns app (:require [cljs.core :as core])) (def old-not core/not) (def old-boolean core/boolean)")
        .unwrap();
    session.collect().unwrap();
    // The call's callee read precedes an argument which replaces that same cell.
    let value = session
        .eval("(core/boolean (do (set! core/boolean (fn [x] false)) 0))")
        .unwrap();
    assert!(boolean(&mut session, &value));
    let value = session.eval("(core/boolean 0)").unwrap();
    assert!(!boolean(&mut session, &value));
    let replacement = session.eval("core/boolean").unwrap();
    let value = session
        .eval("(core/not (do (set! core/not (fn [x] false)) nil))")
        .unwrap();
    assert!(boolean(&mut session, &value));
    let value = session.eval("(core/not nil)").unwrap();
    assert!(!boolean(&mut session, &value));
    session.collect().unwrap();
    session.eval(CORE).unwrap();
    session.enter_namespace("app").unwrap();
    session.collect().unwrap();
    for source in [
        "(core/boolean 0)",
        "(core/not nil)",
        "(old-boolean 0)",
        "(old-not nil)",
    ] {
        let value = session.eval(source).unwrap();
        assert!(boolean(&mut session, &value), "{source}");
    }
    let arg = session.eval("0").unwrap();
    let value = session.invoke(&replacement, &[&arg]).unwrap();
    assert!(!boolean(&mut session, &value));
}
