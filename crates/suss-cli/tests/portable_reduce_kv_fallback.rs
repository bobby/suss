//! Bounded public reduce-kv fallback corpus through compiled source macros and
//! the independent ABI2 storage decoder. This is focused evidence for #244.
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;

use portable_decode::{Decoder, Observation};
use serde_json::{Value, json};
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};
use std::collections::BTreeSet;

const PIN: &str = "c4295f303100bbf5afac449242d30bca1126f1a1";
const IDS: [&str; 5] = [
    "public-fallback-selected-and-accumulates-real-map-entries",
    "public-fallback-empty-preserves-init-without-callback",
    "public-fallback-reduced-unwraps-and-stops-in-sequence-order",
    "public-fallback-throw-stops-effects-and-operands-evaluate-left-to-right",
    "public-fallback-normal-accumulation-exposes-ordered-callback-trace",
];
const CASES_TEXT: &str =
    include_str!("../../../tests/oracle/reduce-kv-fallback-cases.json");
const OBSERVATIONS_TEXT: &str =
    include_str!("../../../tests/oracle/reduce-kv-fallback-observations.json");
const FIXTURE: &str =
    include_str!("../../../tests/oracle/fixtures/reduce-kv-fallback-probe.sus");
const RECEIPT_TEXT: &str =
    include_str!("../../../tests/oracle/reduce-kv-fallback-receipt.json");
const PROVENANCE_TEXT: &str =
    include_str!("../../../docs/compatibility/reduce-kv-fallback-provenance.json");

fn raw(value: Observation) -> Value {
    fn string(units: Vec<u16>) -> Value {
        json!({"tag":"string", "units":units})
    }
    match value {
        Observation::Nil => json!({"tag":"nil"}),
        Observation::Bool(value) => json!({"tag":"bool", "value":value}),
        Observation::Number(bits) => json!({"tag":"f64", "bits":format!("{bits:016x}")}),
        Observation::String(units) => string(units),
        Observation::Keyword(namespace, name) => json!({
            "tag":"keyword",
            "namespace":namespace.map(string).unwrap_or(json!({"tag":"nil"})),
            "name":string(name)
        }),
        Observation::Symbol(namespace, name) => json!({
            "tag":"symbol",
            "namespace":namespace.map(string).unwrap_or(json!({"tag":"nil"})),
            "name":string(name)
        }),
        Observation::Vector(items) => json!({
            "tag":"vector", "items":items.into_iter().map(raw).collect::<Vec<_>>()
        }),
        Observation::List(items) => json!({
            "tag":"seq", "items":items.into_iter().map(raw).collect::<Vec<_>>()
        }),
        Observation::Map(entries) => json!({
            "tag":"map",
            "entries":entries.into_iter().map(|(key, value)| json!([raw(key), raw(value)])).collect::<Vec<_>>()
        }),
        Observation::Set(items) => json!({
            "tag":"set", "items":items.into_iter().map(raw).collect::<Vec<_>>()
        }),
        Observation::ExceptionInfo { message, data, cause } => json!({
            "tag":"exception-info",
            "message":raw(*message), "data":raw(*data), "cause":raw(*cause)
        }),
    }
}

fn exact_fields(value: &Value, expected: &[&str]) {
    let actual = value
        .as_object()
        .expect("expected JSON object")
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text).expect("checked-in reduce-kv evidence JSON parses")
}

#[test]
fn public_reduce_kv_fallback_matches_five_pinned_cases_in_both_phases_after_gc() {
    let cases: Value = parse(CASES_TEXT);
    let observations: Value = parse(OBSERVATIONS_TEXT);
    let receipt: Value = parse(RECEIPT_TEXT);
    let provenance: Value = parse(PROVENANCE_TEXT);

    assert_eq!(suss_compile::portable::bootstrap::sha256(RECEIPT_TEXT.as_bytes()),
        "bd2537e5a6b7011e232579f2a13688f8889c75cea8698d1ae555108775b38b0a");
    exact_fields(
        &receipt,
        &["schema", "upstream", "oracle_compile_exit", "oracle_node_exit", "sha256"],
    );
    assert_eq!(receipt["schema"], 1);
    assert_eq!(receipt["upstream"], PIN);
    assert_eq!(receipt["oracle_compile_exit"], 0);
    assert_eq!(receipt["oracle_node_exit"], 0);
    assert_eq!(suss_compile::portable::bootstrap::sha256(CASES_TEXT.as_bytes()),
        receipt["sha256"]["reduce-kv-fallback-cases.json"]);
    assert_eq!(suss_compile::portable::bootstrap::sha256(OBSERVATIONS_TEXT.as_bytes()),
        receipt["sha256"]["reduce-kv-fallback-observations.json"]);
    assert_eq!(suss_compile::portable::bootstrap::sha256(FIXTURE.as_bytes()),
        receipt["sha256"]["reduce-kv-fallback-probe.sus"]);
    assert_eq!(suss_compile::portable::bootstrap::sha256(PROVENANCE_TEXT.as_bytes()),
        receipt["sha256"]["reduce-kv-fallback-provenance.json"]);
    assert_eq!(provenance["contract"]["commit"], PIN);
    assert_eq!(cases["schema"], 1);
    assert_eq!(cases["upstream"], PIN);
    assert_eq!(observations["schema"], 2);
    assert_eq!(observations["upstream"], PIN);
    exact_fields(&cases, &["schema", "upstream", "cases"]);
    exact_fields(&observations, &["schema", "upstream", "cases"]);

    let cases = cases["cases"].as_array().expect("case array");
    let expected = observations["cases"].as_array().expect("observation array");
    assert_eq!(cases.len(), IDS.len());
    assert_eq!(expected.len(), IDS.len());
    for ((case, observation), id) in cases.iter().zip(expected).zip(IDS) {
        exact_fields(case, &["id", "source"]);
        exact_fields(observation, &["id", "status", "value", "effects"]);
        assert_eq!(case["id"], id);
        assert_eq!(observation["id"], id);
        assert_eq!(observation["status"], "value");
        assert_eq!(observation["effects"], json!([]));
        assert!(!case["source"].as_str().unwrap().trim().is_empty());
    }

    for (phase, mut session) in [
        ("runtime", Session::new_repl().unwrap()),
        ("macro", Session::new_macro().unwrap()),
    ] {
        let mut macros = CompiledMacros::new().unwrap();
        // Capture known core object layouts before fixture declarations.
        let mut decoder = Decoder::capture(&mut session, 65536).unwrap();
        session
            .eval_with_macros(FIXTURE, &mut macros)
            .unwrap_or_else(|error| {
                panic!(
                    "{phase}/fixture: {}",
                    suss_cli::portable_repl::error_display(&mut session, &error)
                )
            });
        session.collect().unwrap();
        decoder.refill(65536);

        for (case, expected) in cases.iter().zip(expected) {
            let id = case["id"].as_str().unwrap();
            let source = case["source"].as_str().unwrap();
            let result = session
                .eval_with_macros(source, &mut macros)
                .unwrap_or_else(|error| {
                    panic!(
                        "{phase}/{id}: {}",
                        suss_cli::portable_repl::error_display(&mut session, &error)
                    )
                });
            session.collect().unwrap();
            decoder.refill(65536);
            assert_eq!(
                raw(decoder.decode_session(&mut session, &result).unwrap()),
                expected["value"],
                "{phase}/{id}: raw ABI value including returned callback trace"
            );
        }
    }
}
