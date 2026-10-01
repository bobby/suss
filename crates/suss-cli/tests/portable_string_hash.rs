use suss_cli::portable_session::Session;
use wasmtime::Val;

#[test]
fn string_hash_match_independently_decoded_primary_observations_after_gc() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/string-hash-cases.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 82);
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
            tag => panic!("unsupported String hash observation {tag}"),
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

fn string_number(session: &mut Session, source: &str) -> f64 {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            Ok(object.field(&mut store, 0)?.unwrap_f64())
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
        assert_eq!(string_number(&mut session, "(.charCodeAt \"AB\" 1)"), 66.0);
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
        55296.0
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
    assert_eq!(string_number(&mut session, "(if (c/pos? 1) 7 9)"), 7.0);
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
        42.0
    );
}
