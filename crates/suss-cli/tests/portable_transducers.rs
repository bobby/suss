//! Original public corpus through compiled source macros and independently decoded ABI storage.
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};
use serde_json::{Value, json};
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};

fn raw(value: Observation) -> Value {
    fn string(units: Vec<u16>) -> Value {
        json!({"tag":"string","units":units})
    }
    match value {
        Observation::Nil => json!({"tag":"nil"}),
        Observation::Bool(value) => json!({"tag":"bool","value":value}),
        Observation::Number(bits) => json!({"tag":"f64","bits":format!("{bits:016x}")}),
        Observation::String(units) => string(units),
        Observation::Keyword(namespace, name) => {
            json!({"tag":"keyword","namespace":namespace.map(string).unwrap_or(json!({"tag":"nil"})),"name":string(name)})
        }
        Observation::Vector(items) => {
            json!({"tag":"vector","items":items.into_iter().map(raw).collect::<Vec<_>>()})
        }
        Observation::List(items) => {
            json!({"tag":"seq","items":items.into_iter().map(raw).collect::<Vec<_>>()})
        }
        other => panic!("Unexpected corpus value: {other:?}"),
    }
}

#[test]
fn public_transducers_match_all_pinned_values_effects_and_throws_in_both_phases() {
    let cases: Value =
        serde_json::from_str(include_str!("../../../tests/oracle/transducer-cases.json")).unwrap();
    let expected_text = include_str!("../../../tests/oracle/transducer-observations.json");
    let expected: Value = serde_json::from_str(expected_text).unwrap();
    assert_eq!(
        suss_compile::portable::bootstrap::sha256(expected_text.as_bytes()),
        "daae4f454d29ebb2f785819fb1f23d2de985f8ec9ebc37636363a90f1b290e19"
    );
    let cases = cases["cases"].as_array().unwrap();
    let expected = expected["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 26);
    assert_eq!(expected.len(), 26);
    let ids = cases
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), 26);
    let fixture = include_str!("../../../tests/oracle/transducer-fixture.cljs");
    assert_eq!(
        suss_compile::portable::bootstrap::sha256(fixture.as_bytes()),
        "2c77196276ed17f6527f318c8e0e8473450e7d77b4abd2880e2a5556df6d44ea"
    );
    for (phase, mut session) in [
        ("runtime", Session::new_repl().unwrap()),
        ("macro", Session::new_macro().unwrap()),
    ] {
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 65536).unwrap();
        for (case, expected) in cases.iter().zip(expected) {
            let id = case["id"].as_str().unwrap();
            let source = case["source"].as_str().unwrap();
            assert_eq!(case["id"], expected["id"]);
            assert!(fixture.contains(&format!(
                "(transport/observation {id:?} (fn [trace] {source}))"
            )));
            // Original public input is unchanged. Numeric exceptions are retained
            // as values only by this observation wrapper; effects are read once.
            let source = format!(
                "(let [trace (atom []) result (try [true {source}] (catch :default error [false error]))] [result @trace])"
            );
            let result = session
                .eval_with_macros(&source, &mut macros)
                .unwrap_or_else(|error| {
                    panic!(
                        "{phase}/{id}: {}",
                        suss_cli::portable_repl::error_display(&mut session, &error)
                    )
                });
            session.collect().unwrap();
            decoder.refill(65536);
            let observed = raw(decoder.decode_session(&mut session, &result).unwrap());
            let result = &observed["items"][0]["items"];
            let status = if result[0]["value"] == json!(true) {
                "value"
            } else {
                "exception"
            };
            assert_eq!(status, expected["status"].as_str().unwrap(), "{phase}/{id}");
            assert_eq!(
                result[1],
                expected[if status == "value" { "value" } else { "thrown" }],
                "{phase}/{id}: raw value"
            );
            assert_eq!(
                observed["items"][1]["items"], expected["effects"],
                "{phase}/{id}: ordered effects"
            );
        }
    }
}

#[test]
fn public_lazy_seq_respects_core_resolution_aliases_exclusions_and_shadows() {
    use suss_cli::portable_session::SessionError;
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        for (source, expected) in [
            ("(first (lazy-seq (list 17)))", 17.0_f64),
            ("(first (cljs.core/lazy-seq (list 23)))", 23.0),
            (
                "(ns lazy.alias (:require [cljs.core :as c])) (first (c/lazy-seq (list 29)))",
                29.0,
            ),
            (
                "(ns lazy.renamed (:refer-clojure :rename {lazy-seq delayed})) (first (delayed (list 31)))",
                31.0,
            ),
            (
                "(ns lazy.lexical) (let [lazy-seq (fn [x] x)] (lazy-seq 37))",
                37.0,
            ),
            (
                "(ns lazy.owned) (def lazy-seq (fn [x] x)) (lazy-seq 41)",
                41.0,
            ),
        ] {
            let result = session
                .eval_with_macros(source, &mut macros)
                .unwrap_or_else(|error| {
                    panic!(
                        "{source}: {}",
                        suss_cli::portable_repl::error_display(&mut session, &error)
                    )
                });
            session.collect().unwrap();
            assert_eq!(
                Decoder::new(16)
                    .decode_session(&mut session, &result)
                    .unwrap(),
                Observation::Number(expected.to_bits()),
                "{source}"
            );
        }
        let error = session
            .eval_with_macros(
                "(ns lazy.value (:refer-clojure :rename {lazy-seq delayed})) delayed",
                &mut macros,
            )
            .unwrap_err();
        let SessionError::Compile(diagnostic) = error else {
            panic!("Macro rename must not synthesize an ordinary value")
        };
        assert!(
            diagnostic.message.contains("delayed") && diagnostic.message.contains("Unresolved"),
            "{}",
            diagnostic.message
        );
        let error = session
            .eval_with_macros(
                "(ns lazy.excluded (:refer-clojure :exclude [lazy-seq])) (lazy-seq (list 1))",
                &mut macros,
            )
            .unwrap_err();
        let SessionError::Compile(diagnostic) = error else {
            panic!("Expected unresolved macro diagnostic")
        };
        assert!(
            diagnostic.message.contains("lazy-seq") && diagnostic.message.contains("Unresolved"),
            "{}",
            diagnostic.message
        );
        let qualified=session.eval_with_macros("(ns lazy.excluded (:refer-clojure :exclude [lazy-seq])) (first (cljs.core/lazy-seq (list 47)))",&mut macros).unwrap();
        session.collect().unwrap();
        assert_eq!(
            Decoder::new(16)
                .decode_session(&mut session, &qualified)
                .unwrap(),
            Observation::Number(47.0_f64.to_bits())
        );
        let result = session
            .eval_with_macros(
                "(ns lazy.recovery) (first (lazy-seq (list 43)))",
                &mut macros,
            )
            .unwrap();
        session.collect().unwrap();
        assert_eq!(
            Decoder::new(16)
                .decode_session(&mut session, &result)
                .unwrap(),
            Observation::Number(43.0_f64.to_bits())
        );
    }
}

#[test]
fn public_array_dependencies_match_all_pinned_values_and_effects_in_both_phases() {
    let cases: Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/array-initializer-cases.json"
    ))
    .unwrap();
    let expected_text = include_str!("../../../tests/oracle/array-initializer-observations.json");
    let expected: Value = serde_json::from_str(expected_text).unwrap();
    assert_eq!(
        suss_compile::portable::bootstrap::sha256(expected_text.as_bytes()),
        "4736967df9e50f0a99d387be1d252135c80742e35b1528c436181a6f6537354d"
    );
    assert_eq!(
        cases["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    assert_eq!(expected["upstream"], cases["upstream"]);
    let cases = cases["cases"].as_array().unwrap();
    let expected = expected["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 19);
    assert_eq!(expected.len(), 19);
    let ids = cases
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), 19);
    let fixture = include_str!("../../../tests/oracle/array-initializer-fixture.cljs");
    assert_eq!(
        suss_compile::portable::bootstrap::sha256(fixture.as_bytes()),
        "cdca842f17c6b1cb27c3682c50764ac4e04238bef7ca78ba2f8eeae585d35348"
    );
    for (phase, mut session) in [
        ("runtime", Session::new_repl().unwrap()),
        ("macro", Session::new_macro().unwrap()),
    ] {
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 65536).unwrap();
        for (case, expected) in cases.iter().zip(expected) {
            let id = case["id"].as_str().unwrap();
            let source = case["source"].as_str().unwrap();
            assert_eq!(case["id"], expected["id"]);
            assert!(fixture.contains(&format!(
                "(transport/observation {id:?} (fn [trace] {source}))"
            )));
            // Original public input is unchanged. Numeric exceptions are retained
            // as values only by this observation wrapper; effects are read once.
            let source = format!(
                "(let [trace (atom []) result (try [true {source}] (catch :default error [false error]))] [result @trace])"
            );
            let result = session
                .eval_with_macros(&source, &mut macros)
                .unwrap_or_else(|error| {
                    panic!(
                        "{phase}/{id}: {}",
                        suss_cli::portable_repl::error_display(&mut session, &error)
                    )
                });
            session.collect().unwrap();
            decoder.refill(65536);
            let observed = raw(decoder.decode_session(&mut session, &result).unwrap());
            let result = &observed["items"][0]["items"];
            let status = if result[0]["value"] == json!(true) {
                "value"
            } else {
                "exception"
            };
            assert_eq!(status, expected["status"].as_str().unwrap(), "{phase}/{id}");
            assert_eq!(
                result[1],
                expected[if status == "value" { "value" } else { "thrown" }],
                "{phase}/{id}: raw value"
            );
            assert_eq!(
                observed["items"][1]["items"], expected["effects"],
                "{phase}/{id}: ordered effects"
            );
        }
    }
}

#[test]
fn owned_array_shift_preserves_holes_owner_and_single_evaluation_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let value = session.eval_with_macros(
            "(let [ctor (type (array)) a (ctor 4) alias a trace (atom []) ignored (aset a 0 nil) ignored (aset a 2 false) ignored (aset a 3 17) first (.shift (do (swap! trace conj 7) a))] [(identical? a alias) first (alength alias) (.hasOwnProperty alias 0) (.hasOwnProperty alias 1) (aget alias 1) (aget alias 2) (.hasOwnProperty alias 3) @trace])",
            &mut macros,
        ).unwrap();
        session.collect().unwrap();
        assert_eq!(
            Decoder::capture(&mut session, 256)
                .unwrap()
                .decode_session(&mut session, &value)
                .unwrap(),
            Observation::Vector(vec![
                Observation::Bool(true),
                Observation::Nil,
                Observation::Number(3.0_f64.to_bits()),
                Observation::Bool(false),
                Observation::Bool(true),
                Observation::Bool(false),
                Observation::Number(17.0_f64.to_bits()),
                Observation::Bool(false),
                Observation::Vector(vec![Observation::Number(7.0_f64.to_bits())]),
            ])
        );
        for source in [
            "(let [ctor (type (array)) a (ctor 1)] [(undefined? (.shift a)) (alength a) (.hasOwnProperty a 0)])",
            "(let [a (array)] [(undefined? (.shift a)) (alength a) (.hasOwnProperty a 0)])",
        ] {
            let value = session.eval_with_macros(source, &mut macros).unwrap();
            session.collect().unwrap();
            assert_eq!(
                Decoder::capture(&mut session, 128)
                    .unwrap()
                    .decode_session(&mut session, &value)
                    .unwrap(),
                Observation::Vector(vec![
                    Observation::Bool(true),
                    Observation::Number(0.0_f64.to_bits()),
                    Observation::Bool(false)
                ])
            );
        }
    }
}
