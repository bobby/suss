use suss_cli::portable_session::Session;

#[path = "support/clojure_corrections.rs"]
mod clojure_corrections;

/// The observation of a runtime number, distinguishing a long from a double.
fn number_json(number: suss_cli::portable_session::RuntimeNumber) -> serde_json::Value {
    match number {
        suss_cli::portable_session::RuntimeNumber::Long(value) => serde_json::json!({"tag":"long", "value":value}),
        suss_cli::portable_session::RuntimeNumber::Double(value) => {
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
fn string_hash_match_independently_decoded_primary_observations_after_gc() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/string-hash-cases.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 96);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session
        .enter_namespace("suss-oracle.string-hash-cases")
        .unwrap();
    let mut ids = std::collections::BTreeSet::new();
    let mut failures = Vec::new();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        assert!(ids.insert(id));
        let source = case["source"].as_str().unwrap();
        if clojure_corrections::calls_imul(source) {
            match session.eval(source) {
                Err(suss_cli::portable_session::SessionError::Compile(error))
                    if error.message.contains("Unresolved Runtime name")
                        && error.message.contains("imul") => {}
                _ => failures.push(format!("{id}: {source}: expected imul to be unresolved")),
            }
            continue;
        }
        let value = match session.eval(source) {
            Ok(value) => value,
            Err(error) => {
                failures.push(format!("{id}: {source}: {error:?}"));
                continue;
            }
        };
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "f64" => {
                let number = session
                    .inspect(&value, |store, value| {
                        Ok(suss_cli::portable_session::runtime_number(store, &value)?.expect("Number layout"))
                    })
                    .unwrap();
                number_json(number)
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
            tag => panic!("unsupported String hash observation {tag}"),
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

fn string_number(session: &mut Session, source: &str) -> i64 {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |mut store, value| {
            match suss_cli::portable_session::runtime_number(&mut store, &value)? {
                Some(suss_cli::portable_session::RuntimeNumber::Long(value)) => Ok(value),
                other => panic!("long result {other:?}"),
            }
        })
        .unwrap()
}

#[test]
fn string_member_domain_errors_recover_after_gc() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    for source in [
        "(let [f (.-charCodeAt \"A\")] (f 0))",
        "(.charCodeAt \"A\" (fn [] 1))",
        "(let [owner nil] (.charCodeAt owner 0))",
        "(let [owner 123] (.charCodeAt owner 0))",
        "(set! (.-charCodeAt \"A\") (fn [x] 1))",
    ] {
        session.collect().unwrap();
        let result = session.eval(source);
        assert!(matches!(result, Err(SessionError::Language(_))), "{source}: {result:?}");
        assert_eq!(string_number(&mut session, "(.charCodeAt \"AB\" 1)"), 66);
    }
    session
        .eval("(def saved_char_method (.-charCodeAt \"A\"))")
        .unwrap();
    session.collect().unwrap();
    assert!(matches!(
        session.eval("(saved_char_method 0)"),
        Err(SessionError::Language(_))
    ));
    assert_eq!(
        string_number(&mut session, "(.charCodeAt \"\\ud800\" 0)"),
        55296
    );
}

#[test]
fn positive_predicate_compile_errors_are_located_atomic_and_phase_checked() {
    use suss_cli::portable_session::SessionError;
    use suss_compile::portable::{
        prepare_fragment,
        resolve::{Environment, Phase},
    };
    let mut session = Session::new().unwrap();
    for body in ["(pos?)", "(pos? 1 2)"] {
        let source = format!("(do (def positive_unpublished 9) {body})");
        let SessionError::Compile(error) = session.eval(&source).unwrap_err() else {
            panic!("{source}");
        };
        assert!(error.span.start < error.span.end && error.span.end <= source.len());
        assert!(session.eval("positive_unpublished").is_err());
    }
    session
        .eval("(ns string.alias (:require [cljs.core :as c]))")
        .unwrap();
    assert_eq!(string_number(&mut session, "(if (c/pos? 1) 7 9)"), 7);
    for phase in [Phase::Runtime, Phase::Macro] {
        let prepared =
            prepare_fragment("(cljs.core/pos? 1)", &Environment::default(), phase).unwrap();
        suss_compile::runtime_abi::verify_artifact(
            &prepared.wasm,
            &suss_compile::runtime_abi::Manifest::default(),
        )
        .unwrap();
    }
    session
        .eval("(ns string.shadow (:refer-clojure :exclude [pos?]))")
        .unwrap();
    assert!(session.eval("(pos? 1)").is_err());
    assert_eq!(
        string_number(&mut session, "(let [pos? (fn [x] 42)] (pos? 1))"),
        42
    );
}
