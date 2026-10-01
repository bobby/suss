use suss_cli::portable_session::Session;
use wasmtime::Val;

#[test]
fn numeric_hash_match_independently_decoded_primary_observations_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/numeric-hash-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 52);
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
            tag => panic!("unsupported Numeric hash observation {tag}"),
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

fn numeric_hash_number(session: &mut Session, source: &str) -> f64 {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            Ok(object.field(&mut store, 0)?.unwrap_f64())
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
