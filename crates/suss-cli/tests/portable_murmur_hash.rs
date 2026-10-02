use suss_cli::portable_session::Session;
use wasmtime::Val;

#[test]
fn murmur_hash_match_independently_decoded_primary_observations_after_gc() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/murmur-hash-cases.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 73);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session
        .enter_namespace("suss-oracle.murmur-hash-cases")
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
            tag => panic!("unsupported Murmur hash observation {tag}"),
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

fn murmur_number(session: &mut Session, source: &str) -> f64 {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            Ok(object.field(&mut store, 0)?.unwrap_f64())
        })
        .unwrap()
}

#[test]
fn threading_invalid_bindings_and_arities_recover_without_publication() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    for body in [
        "(->)",
        "(as-> 1)",
        "(as-> 1 :x 2)",
        "(as-> 1 foo/x 2)",
        "(as-> 1 & 2)",
        "(-> 1 ())",
        "(zero?)",
        "(zero? 1 2)",
        "(as-> 1 x (recur x))",
    ] {
        let source = format!("(do (def murmur_unpublished 7) {body})");
        let SessionError::Compile(error) = session.eval(&source).unwrap_err() else {
            panic!("expected compile error: {source}");
        };
        assert!(error.span.start < error.span.end && error.span.end <= source.len());
        assert!(session.eval("murmur_unpublished").is_err());
        assert_eq!(murmur_number(&mut session, "(as-> 3 x (+ x 7))"), 10.0);
    }
}

#[test]
fn threading_aliases_exclusions_and_phase_are_explicit() {
    use suss_compile::portable::{
        prepare_fragment,
        resolve::{Environment, Phase},
    };
    let mut session = Session::new().unwrap();
    session
        .eval("(ns murmur.alias (:require [cljs.core :as c]))")
        .unwrap();
    assert_eq!(
        murmur_number(&mut session, "(c/-> 3 (imul 5) (bit-or 2))"),
        15.0
    );
    assert_eq!(
        murmur_number(&mut session, "(c/as-> 3 n (imul n 5) (bit-or n 2))"),
        15.0
    );
    session.eval("(ns murmur.excluded (:refer-clojure :exclude [-> as-> zero?])) (def -> (fn [x] (+ x 100))) (def as-> (fn [x y] (+ x y))) (def zero? (fn [x] (+ x 7)))").unwrap();
    assert_eq!(murmur_number(&mut session, "(-> 3)"), 103.0);
    assert_eq!(murmur_number(&mut session, "(as-> 3 4)"), 7.0);
    assert_eq!(murmur_number(&mut session, "(zero? 3)"), 10.0);
    assert_eq!(
        murmur_number(&mut session, "(cljs.core/-> 3 (imul 5))"),
        15.0
    );
    for phase in [Phase::Runtime, Phase::Macro] {
        for source in ["(-> 3 (bit-or 4))", "(as-> 3 n (bit-or n 4))", "(zero? 0)"] {
            let fragment = prepare_fragment(source, &Environment::default(), phase).unwrap();
            suss_compile::runtime_abi::verify_artifact(
                &fragment.wasm,
                &suss_compile::runtime_abi::Manifest::default(),
            )
            .unwrap();
        }
    }
}

#[test]
fn retained_murmur_functions_observe_live_globals_and_old_values_survive_gc() {
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session
        .eval("(def saved_murmur m3-mix-K1) (def saved_zero zero?)")
        .unwrap();
    session.collect().unwrap();
    assert_eq!(
        murmur_number(
            &mut session,
            "(with-redefs [cljs.core/imul (fn [x y] 71)] (saved_murmur 7))"
        ),
        71.0
    );
    let expected = murmur_number(&mut session, "(m3-mix-K1 7)");
    session
        .eval("(def cljs.core/m3-mix-K1 (fn [x] 99))")
        .unwrap();
    session.collect().unwrap();
    assert_eq!(murmur_number(&mut session, "(saved_murmur 7)"), expected);
    assert_eq!(murmur_number(&mut session, "(m3-mix-K1 7)"), 99.0);
    assert!(session.eval("(saved_murmur)").is_err());
    assert_eq!(murmur_number(&mut session, "(as-> 3 n (+ n 7))"), 10.0);
}

#[test]
fn threading_expansion_limits_are_located_atomic_and_recover() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    for body in [
        format!("(-> 1 {})", "(+ 1) ".repeat(256)),
        format!("(as-> 1 n {})", "(+ n 1) ".repeat(255)),
        // Each macro is within its own expansion budget, but their combined
        // expansion exceeds the guarded 64-level analyzer boundary.
        format!("(-> (-> 1 {}) {})", "(+ 1) ".repeat(40), "(+ 1) ".repeat(40)),
    ] {
        let source = format!("(do (def threading_limit_unpublished 7) {body})");
        let SessionError::Compile(error) = session.eval(&source).unwrap_err() else {
            panic!("expected located expansion limit");
        };
        assert!(error.message.contains("expansion limit"), "{error:?}");
        assert!(error.span.start < error.span.end && error.span.end <= source.len());
        assert!(session.eval("threading_limit_unpublished").is_err());
        assert_eq!(murmur_number(&mut session, "(as-> 7 n (+ n 3))"), 10.0);
    }
    // Sequential as-> keeps its argument boundary; modest threading executes.
    assert_eq!(
        murmur_number(&mut session, &format!("(-> 1 {})", "(+ 1) ".repeat(16))),
        17.0
    );
    assert_eq!(
        murmur_number(&mut session, &format!("(-> (-> 1 {}) {})", "(+ 1) ".repeat(16), "(+ 1) ".repeat(16))),
        33.0
    );
    assert_eq!(
        murmur_number(&mut session, &format!("{}1{}", "(+ 1 ".repeat(40), ")".repeat(40))),
        41.0
    );
    assert_eq!(
        murmur_number(&mut session, &format!("(as-> 1 n {})", "(+ n 1) ".repeat(254))),
        255.0
    );
}
