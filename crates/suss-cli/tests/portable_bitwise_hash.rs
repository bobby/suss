use suss_cli::portable_session::Session;
use wasmtime::Val;

#[test]
fn bitwise_hash_match_independently_decoded_primary_observations_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/bitwise-hash-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 54);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session
        .enter_namespace("suss-oracle.bitwise-hash-cases")
        .unwrap();
    let mut ids = std::collections::BTreeSet::new();
    let mut failures = Vec::new();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        assert!(ids.insert(id));
        let source = case["source"].as_str().unwrap();
        let value = match session.eval(source) {
            Ok(value) => value,
            Err(error) => {
                failures.push(format!("{id}: {source}: {error:?}"));
                continue;
            }
        };
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "f64" => {
                let bits = session
                    .inspect(&value, |mut store, value| {
                        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                        let fields = object.fields(&mut store)?.collect::<Vec<_>>();
                        let [Val::F64(bits)] = fields.as_slice() else {
                            panic!("Number layout");
                        };
                        Ok(*bits)
                    })
                    .unwrap();
                serde_json::json!({"tag":"f64", "bits":format!("{bits:016x}")})
            }
            "nil" => {
                session
                    .inspect(&value, |store, value| {
                        assert_eq!(
                            value
                                .unwrap_anyref()
                                .unwrap()
                                .as_i31(&store)?
                                .unwrap()
                                .get_u32(),
                            0
                        );
                        Ok(())
                    })
                    .unwrap();
                serde_json::json!({"tag":"nil"})
            }
            "bool" => {
                let boolean = session
                    .inspect(&value, |store, value| {
                        let sentinel = value
                            .unwrap_anyref()
                            .unwrap()
                            .as_i31(&store)?
                            .unwrap()
                            .get_u32();
                        match sentinel {
                            2 => Ok(false),
                            4 => Ok(true),
                            n => panic!("Boolean {n}"),
                        }
                    })
                    .unwrap();
                serde_json::json!({"tag":"bool", "value":boolean})
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
            tag => panic!("unsupported Bitwise hash observation {tag}"),
        };
        if actual != case["expected"] {
            failures.push(format!(
                "{id}: {source}: actual {actual}, expected {}",
                case["expected"]
            ));
        }
        session.collect().unwrap();
    }
    assert!(
        failures.is_empty(),
        "{} failing observations:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn bitwise_number(session: &mut Session, source: &str) -> f64 {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            Ok(object.field(&mut store, 0)?.unwrap_f64())
        })
        .unwrap()
}

#[test]
fn bitwise_bad_arities_fail_with_spans_and_no_partial_publication() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    for body in [
        "(int)",
        "(int 1 2)",
        "(bit-and)",
        "(bit-and 7)",
        "(bit-not 1 2)",
        "(unsigned-bit-shift-right 1)",
    ] {
        let source = format!("(do (def bitwise_unpublished 7) {body})");
        let SessionError::Compile(error) = session.eval(&source).unwrap_err() else {
            panic!("expected compile error: {source}");
        };
        assert!(error.span.start < error.span.end && error.span.end <= source.len());
        assert!(session.eval("bitwise_unpublished").is_err());
        assert_eq!(bitwise_number(&mut session, "(int 4294967295)"), -1.0);
    }
    // The accepted arity contract diagnoses these separately from pinned JS
    // captured wrappers' off-arity fill/ignore behavior; no differential claim.
    for source in [
        "(let [f int] (f))",
        "(let [f int] (f 1 2))",
        "(let [f bit-and] (f 1))",
        "(imul 1)",
    ] {
        assert!(session.eval(source).is_err(), "{source}");
        assert_eq!(bitwise_number(&mut session, "(imul 7 3)"), 21.0);
    }
}

#[test]
fn bitwise_macro_folds_preserve_throw_order_and_computed_calls_evaluate_all_args() {
    let mut session = Session::new().unwrap();
    session.eval("(def bitwise_trace 0)").unwrap();
    // Object coercion is explicitly unsupported. Nested macro pair conversion
    // fails before the third operand; computed function arguments precede it.
    for (source, expected) in [
        ("(bit-or (fn [] 0) 1 (do (set! bitwise_trace 7) 2))", 0.0),
        (
            "(let [f bit-or] (f (fn [] 0) 1 (do (set! bitwise_trace 7) 2)))",
            7.0,
        ),
    ] {
        session.eval("(set! bitwise_trace 0)").unwrap();
        assert!(session.eval(source).is_err());
        assert_eq!(bitwise_number(&mut session, "bitwise_trace"), expected);
        session.collect().unwrap();
        assert_eq!(bitwise_number(&mut session, "(bit-or 1 2 4)"), 7.0);
    }
}

#[test]
fn bitwise_alias_exclusion_and_both_compilation_phases_are_explicit() {
    use suss_compile::portable::{
        prepare_fragment,
        resolve::{Environment, Phase},
    };
    let mut session = Session::new().unwrap();
    session
        .eval("(ns bitwise.alias (:require [cljs.core :as c]))")
        .unwrap();
    assert_eq!(bitwise_number(&mut session, "(c/bit-or 1 2 4)"), 7.0);
    session.eval("(ns bitwise.excluded (:refer-clojure :exclude [bit-or])) (def bit-or (fn [x y] (+ x y)))").unwrap();
    assert_eq!(bitwise_number(&mut session, "(bit-or 1 1)"), 2.0);
    assert_eq!(bitwise_number(&mut session, "(cljs.core/bit-or 1 1)"), 1.0);
    for phase in [Phase::Runtime, Phase::Macro] {
        let fragment = prepare_fragment("(bit-or 1 2 4)", &Environment::default(), phase).unwrap();
        suss_compile::runtime_abi::verify_artifact(
            &fragment.wasm,
            &suss_compile::runtime_abi::Manifest::default(),
        )
        .unwrap();
    }
}
