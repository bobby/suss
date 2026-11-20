use suss_cli::portable_session::RuntimeNumber;
use suss_cli::portable_session::Session;

/// The observation of a runtime number, distinguishing a long from a double.
fn number_json(number: RuntimeNumber) -> serde_json::Value {
    match number {
        RuntimeNumber::Long(value) => serde_json::json!({"tag":"long", "value":value}),
        RuntimeNumber::Double(value) => {
            serde_json::json!({"tag":"f64", "bits":format!("{:016x}", value.to_bits())})
        }
    }
}
/// The ClojureScript oracle encodes every number as a double; JVM Clojure,
/// Suss's oracle, gives a long for these integral results.
fn clojure_number(expected: &serde_json::Value) -> serde_json::Value {
    if expected["tag"] == "f64" {
        let bits = u64::from_str_radix(expected["bits"].as_str().unwrap(), 16).unwrap();
        let value = f64::from_bits(bits);
        if value.is_finite() && value.fract() == 0.0 && bits != (-0.0f64).to_bits() {
            return serde_json::json!({"tag":"long", "value":value as i64});
        }
    }
    expected.clone()
}

#[test]
fn numeric_hash_match_independently_decoded_primary_observations_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/numeric-hash-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 72);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session
        .enter_namespace("suss-oracle.numeric-hash-cases")
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
                number_json(
                    session
                        .inspect(&value, |store, value| {
                            Ok(suss_cli::portable_session::runtime_number(store, &value)?
                                .expect("Number layout"))
                        })
                        .unwrap(),
                )
            }
            "nil" => {
                session
                    .inspect(&value, |mut store, value| {
                        assert_eq!(
                            suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap(),
                            0
                        );
                        Ok(())
                    })
                    .unwrap();
                serde_json::json!({"tag":"nil"})
            }
            "bool" => {
                let boolean = session
                    .inspect(&value, |mut store, value| {
                        let sentinel = suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap();
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
            tag => panic!("unsupported Numeric hash observation {tag}"),
        };
        if actual != clojure_number(&case["expected"]) {
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

fn numeric_hash_number(session: &mut Session, source: &str) -> f64 {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |store, value| {
            // Every hash result here is integral: a long.
            match suss_cli::portable_session::runtime_number(store, &value)? {
                Some(RuntimeNumber::Long(value)) => Ok(value as f64),
                other => panic!("long result, got {other:?}"),
            }
        })
        .unwrap()
}

#[test]
fn numeric_hash_errors_are_located_atomic_or_typed_and_recover_after_gc() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    for body in [
        "(suss.bootstrap/f64-coerce)",
        "(suss.bootstrap/f64-word0 1 2)",
    ] {
        let source = format!("(do (def numeric_hash_unpublished 7) {body})");
        let SessionError::Compile(error) = session.eval(&source).unwrap_err() else {
            panic!("{source}");
        };
        assert!(error.span.start < error.span.end && error.span.end <= source.len());
        assert!(session.eval("numeric_hash_unpublished").is_err());
    }
    for source in [
        "(hash-double)", "(hash-double 1 2)", "(hash-combine 1)", "(hash-combine 1 2 3)",
        "(hash-double (fn [] 1))",
        "(suss.bootstrap/f64-word0 nil)",
        "(suss.bootstrap/f64-word4 \"1\")",
        "(let [f hash-double] (f))",
        "(let [f hash-combine] (f 1))",
    ] {
        session.collect().unwrap();
        assert!(
            matches!(session.eval(source), Err(SessionError::Language(_))),
            "{source}"
        );
        assert_eq!(
            numeric_hash_number(&mut session, "(hash-double 1.5)"),
            63551.0
        );
    }
}

#[test]
fn numeric_hash_aliases_and_private_adapters_validate_in_both_phases() {
    use suss_compile::portable::{
        prepare_fragment,
        resolve::{Environment, Phase},
    };
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session
        .eval("(ns numeric.alias (:require [cljs.core :as c]))")
        .unwrap();
    assert_eq!(
        numeric_hash_number(&mut session, "(c/hash-double 1.5)"),
        63551.0
    );
    assert_eq!(
        numeric_hash_number(
            &mut session,
            "(let [hash-double (fn [x] 7)] (hash-double 1.5))"
        ),
        7.0
    );
    for phase in [Phase::Runtime, Phase::Macro] {
        for source in [
            "(suss.bootstrap/f64-coerce nil)",
            "(suss.bootstrap/f64-word0 1.5)",
            "(suss.bootstrap/f64-word4 -0.0)",
        ] {
            let fragment = prepare_fragment(source, &Environment::default(), phase).unwrap();
            suss_compile::runtime_abi::verify_artifact(
                &fragment.wasm,
                &suss_compile::runtime_abi::Manifest::default(),
            )
            .unwrap();
        }
    }
}
