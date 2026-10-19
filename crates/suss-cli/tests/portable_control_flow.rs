use suss_cli::portable_session::Session;
use wasmtime::Val;

#[test]
fn control_macros_match_independently_decoded_primary_observations_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/control-flow-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 81);
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
        "(if-let)",
        "(if-let true 1)",
        "(if-let [] 1)",
        "(if-let [x] 1)",
        "(if-let [x 1 y 2] 1)",
        "(if-let [a/b 1] 1)",
        "(if-let [& 1] 1)",
        "(if-let [[x] 1] x)",
        "(if-let [iflet-missing 1] iflet-missing iflet-missing)",
        "(if-let [x true] 1 2 3)",
        "(loop [n 0] (if-let [x (recur 1)] 7 9))",
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
    let value = session.eval(&excessive).unwrap();
    session.inspect(&value, |store, value| {
        assert_eq!(value.unwrap_anyref().unwrap().as_i31(&store)?.unwrap().get_u32(), 4);
        Ok(())
    }).unwrap();
    session.eval("(def finite-and-effects 0)").unwrap();
    let ordered = (1..=257).map(|step| format!(
        "(do (if (== finite-and-effects {}) (set! finite-and-effects {step}) (throw 91)) true)",
        step - 1)).collect::<Vec<_>>().join(" ");
    let value = session.eval(&format!("(and {ordered})")).unwrap();
    session.inspect(&value, |store, value| {
        assert_eq!(value.unwrap_anyref().unwrap().as_i31(&store)?.unwrap().get_u32(), 4);
        Ok(())
    }).unwrap();
    let value = session.eval("finite-and-effects").unwrap();
    session.inspect(&value, |mut store, value| {
        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
        assert_eq!(object.field(&mut store, 0)?.unwrap_f64().to_bits(), 257.0f64.to_bits());
        Ok(())
    }).unwrap();
    assert!(matches!(
        session.eval("(def control-ghost 7) (when)"),
        Err(SessionError::Compile(_))
    ));
    assert!(matches!(
        session.eval("control-ghost"),
        Err(SessionError::Compile(_))
    ));
    assert!(matches!(
        session.eval("(def iflet-ghost 7) (if-let [x] 1)"),
        Err(SessionError::Compile(_))
    ));
    assert!(matches!(
        session.eval("iflet-ghost"),
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
    session.eval("(c/if-let [x 17] x 9)").unwrap();
    session
        .eval("(ns control.excluded (:refer-clojure :exclude [when if-let]))")
        .unwrap();
    assert!(matches!(
        session.eval("(when true 7)"),
        Err(SessionError::Compile(_))
    ));
    session.eval("(cljs.core/when true 7)").unwrap();
    assert!(matches!(
        session.eval("(if-let [x 17] x 9)"),
        Err(SessionError::Compile(_))
    ));
    session.eval("(cljs.core/if-let [x 17] x 9)").unwrap();
    for phase in [Phase::Runtime, Phase::Macro] {
        let environment = Environment::default();
        prepare_fragment(
            "(cljs.core/when-not false (cljs.core/if-let [x 7] (cljs.core/and true (cljs.core/or false x))))",
            &environment,
            phase,
        )
        .unwrap();
    }
}


// Execute the two direct control-tail recursion edges, not only their encodings.
// Minimal sessions avoid unrelated core provisioning; both artifact phases run.
fn assert_deep_control_number(session: &mut Session, value: &suss_cli::portable_session::SessionValue, expected: f64) {
    session.collect().unwrap();
    session.inspect(value, |mut store, value| {
        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
        let fields = object.fields(&mut store)?.collect::<Vec<_>>();
        assert!(matches!(fields.as_slice(), [Val::F64(bits)] if *bits == expected.to_bits()));
        Ok(())
    }).unwrap();
}

#[test]
fn deep_or_257_operands_execute_in_order_and_short_circuit_in_both_phases() {
    use suss_cli::portable_session::SessionOptions;
    use suss_compile::portable::resolve::Phase;
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
        assert_eq!(session.phase(), phase);
        let fuel = session.options().fuel_per_operation;
        for selected in [257, 1] {
            session.eval("(def deep-control-order 0)").unwrap();
            // The late case reaches every operand; the early case must not run
            // any later operand's effect, even though all257 are compiled.
            let operands = (1..=257).map(|step| format!(
                "(do (if (== deep-control-order {}) (set! deep-control-order {step}) (throw 91)) {})",
                step - 1, if step == selected { "42" } else { "false" },
            )).collect::<Vec<_>>().join(" ");
            let source = format!("(or {operands})");
            let value = session.eval(&source)
                .unwrap_or_else(|error| panic!("{phase:?}/selected{selected}: {error:?}"));
            assert_deep_control_number(&mut session, &value, 42.0);
            let order = session.eval("deep-control-order").unwrap();
            assert_deep_control_number(&mut session, &order, selected as f64);
            assert_eq!(session.options().fuel_per_operation, fuel);
        }
    }
}

#[test]
fn deep_cond_257_pairs_execute_tests_in_order_and_only_selected_arm_in_both_phases() {
    use suss_cli::portable_session::SessionOptions;
    use suss_compile::portable::resolve::Phase;
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
        assert_eq!(session.phase(), phase);
        let fuel = session.options().fuel_per_operation;
        for selected in [257, 1] {
            session.eval("(def deep-control-order 0) (def deep-control-arms 0)").unwrap();
            let pairs = (1..=257).map(|step| {
                let predicate = format!(
                    "(do (if (== deep-control-order {}) (set! deep-control-order {step}) (throw 91)) {})",
                    step - 1, if step == selected { "true" } else { "false" },
                );
                let arm = if step == selected {
                    "(do (set! deep-control-arms (+ deep-control-arms 1)) 42)"
                } else {
                    "(throw 92)"
                };
                format!("{predicate} {arm}")
            }).collect::<Vec<_>>().join(" ");
            let source = format!("(cond {pairs})");
            let value = session.eval(&source)
                .unwrap_or_else(|error| panic!("{phase:?}/selected{selected}: {error:?}"));
            assert_deep_control_number(&mut session, &value, 42.0);
            let order = session.eval("deep-control-order").unwrap();
            assert_deep_control_number(&mut session, &order, selected as f64);
            let arms = session.eval("deep-control-arms").unwrap();
            assert_deep_control_number(&mut session, &arms, 1.0);
            assert_eq!(session.options().fuel_per_operation, fuel);
        }
    }
}
