use suss_cli::portable_session::Session;
use wasmtime::Val;

#[test]
fn control_macros_match_independently_decoded_primary_observations_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/control-flow-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 59);
    let mut session = Session::new().unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        assert!(ids.insert(id));
        let source = case["source"].as_str().unwrap();
        let value = session
            .eval(source)
            .unwrap_or_else(|error| panic!("{id}: {source}: {error:?}"));
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
            tag => panic!("unsupported control observation {tag}"),
        };
        assert_eq!(actual, case["expected"], "{id}: {source}");
        session.collect().unwrap();
    }
}

#[test]
fn malformed_control_macros_have_located_errors_and_do_not_publish_definitions() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    for source in [
        "(when)",
        "(when-not)",
        "(if-not true)",
        "(if-not true 1 2 3)",
        "(cond true)",
        "(and (recur 1) 7)",
        "(or (recur 1) 7)",
    ] {
        let error = session.eval(source).unwrap_err();
        let SessionError::Compile(error) = error else {
            panic!("expected located diagnostic: {error:?}");
        };
        assert!(error.span.end > error.span.start, "{source}: {error:?}");
    }
    let excessive = format!(
        "(and {})",
        std::iter::repeat_n("true", 257)
            .collect::<Vec<_>>()
            .join(" ")
    );
    let SessionError::Compile(error) = session.eval(&excessive).unwrap_err() else {
        panic!("expected expansion guard");
    };
    assert!(error.message.contains("expansion limit"));
    assert!(matches!(
        session.eval("(def control-ghost 7) (when)"),
        Err(SessionError::Compile(_))
    ));
    assert!(matches!(
        session.eval("control-ghost"),
        Err(SessionError::Compile(_))
    ));
    let value = session.eval("(cond false 1 true 7)").unwrap();
    session
        .inspect(&value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            assert!(matches!(fields.as_slice(), [Val::F64(bits)] if *bits == 7.0f64.to_bits()));
            Ok(())
        })
        .unwrap();
}

#[test]
fn control_macro_aliases_exclusions_and_macro_phase_are_resolved_explicitly() {
    use suss_cli::portable_session::SessionError;
    use suss_compile::portable::{
        prepare_fragment,
        resolve::{Environment, Phase},
    };
    let mut session = Session::new().unwrap();
    session
        .eval("(ns control.alias (:require [cljs.core :as c]))")
        .unwrap();
    let value = session
        .eval("(c/when true (c/cond false 1 true (c/or false (c/and true 7))))")
        .unwrap();
    session
        .inspect(&value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            assert!(matches!(fields.as_slice(), [Val::F64(bits)] if *bits == 7.0f64.to_bits()));
            Ok(())
        })
        .unwrap();
    session
        .eval("(ns control.excluded (:refer-clojure :exclude [when]))")
        .unwrap();
    assert!(matches!(
        session.eval("(when true 7)"),
        Err(SessionError::Compile(_))
    ));
    session.eval("(cljs.core/when true 7)").unwrap();
    for phase in [Phase::Runtime, Phase::Macro] {
        let environment = Environment::default();
        prepare_fragment(
            "(cljs.core/when-not false (cljs.core/and true (cljs.core/or false 7)))",
            &environment,
            phase,
        )
        .unwrap();
    }
}
