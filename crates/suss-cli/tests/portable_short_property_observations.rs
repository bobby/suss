//! Pinned CLJS value/effect observations, decoded independently after GC.
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};
use suss_cli::portable_session::Session;

fn tagged(value: Observation) -> serde_json::Value {
    match value {
        Observation::Nil => serde_json::json!({"tag":"nil"}),
        Observation::Bool(value) => serde_json::json!({"tag":"bool","value":value}),
        Observation::Number(bits) => serde_json::json!({"tag":"f64","bits":format!("{bits:016x}")}),
        Observation::Vector(items) => {
            serde_json::json!({"tag":"vector","items":items.into_iter().map(tagged).collect::<Vec<_>>()})
        }
        other => panic!("unsupported short-property observation: {other:?}"),
    }
}
#[test]
fn short_property_values_and_getter_effects_match_pinned_cljs_after_gc_in_both_phases() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/short-property-cases.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    let expected_ids = [
        "nominal-nil-false-value",
        "replaced-getter-ordered-owner-effects",
        "fallback-nil-false-getters-run-once",
        "fallback-thrown-nil-false-preserve-effects",
    ];
    assert_eq!(
        cases
            .iter()
            .map(|c| c["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        expected_ids
    );
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval("(deftype ShortLiteralFields [ns name])")
            .unwrap();
        let mut decoder = Decoder::capture(&mut session, 16_384).unwrap();
        let mut seen = std::collections::BTreeSet::new();
        for case in cases {
            let id = case["id"].as_str().unwrap();
            assert!(seen.insert(id));
            let value = session
                .eval(case["source"].as_str().unwrap())
                .unwrap_or_else(|error| panic!("{id}: {error:?}"));
            session.collect().unwrap();
            decoder.refill(16_384);
            let actual = tagged(
                decoder
                    .decode_session(&mut session, &value)
                    .unwrap_or_else(|error| panic!("{id}: {error:?}")),
            );
            assert_eq!(actual, case["expected"], "{id}");
        }
    }
}
