//! Full record compiler graph runtime dependencies; authored UNCOMPILED/UNEXECUTED.
use serde_json::{Value, json};
use suss_cli::{portable_macro_data::FormBridge, portable_session::Session};
use suss_reader::forms::{Form, Kind};
const IDS: &[&str] = &[
    "comp-zero",
    "comp-one",
    "comp-2-args-0",
    "comp-2-args-1",
    "comp-2-args-2",
    "comp-2-args-3",
    "comp-2-args-5",
    "comp-3-args-0",
    "comp-3-args-1",
    "comp-3-args-2",
    "comp-3-args-3",
    "comp-3-args-5",
    "comp-5-args-0",
    "comp-5-args-1",
    "comp-5-args-2",
    "comp-5-args-3",
    "comp-5-args-5",
    "partial-0-args-0",
    "partial-0-args-1",
    "partial-0-args-2",
    "partial-0-args-3",
    "partial-0-args-5",
    "partial-1-args-0",
    "partial-1-args-1",
    "partial-1-args-2",
    "partial-1-args-3",
    "partial-1-args-5",
    "partial-2-args-0",
    "partial-2-args-1",
    "partial-2-args-2",
    "partial-2-args-3",
    "partial-2-args-5",
    "partial-3-args-0",
    "partial-3-args-1",
    "partial-3-args-2",
    "partial-3-args-3",
    "partial-3-args-5",
    "partial-5-args-0",
    "partial-5-args-1",
    "partial-5-args-2",
    "partial-5-args-3",
    "partial-5-args-5",
    "juxt-1-args-0",
    "juxt-1-args-1",
    "juxt-1-args-2",
    "juxt-1-args-3",
    "juxt-1-args-5",
    "juxt-2-args-0",
    "juxt-2-args-1",
    "juxt-2-args-2",
    "juxt-2-args-3",
    "juxt-2-args-5",
    "juxt-3-args-0",
    "juxt-3-args-1",
    "juxt-3-args-2",
    "juxt-3-args-3",
    "juxt-3-args-5",
    "juxt-5-args-0",
    "juxt-5-args-1",
    "juxt-5-args-2",
    "juxt-5-args-3",
    "juxt-5-args-5",
    "merge-zero",
    "merge-all-nil",
    "merge-false-nil-values",
    "merge-with-zero",
    "merge-with-order",
    "merge-with-absence",
    "update-args-0",
    "update-args-1",
    "update-args-2",
    "update-args-3",
    "update-args-4",
    "update-missing",
    "update-throw-order",
    "select-keys-meta-presence",
    "select-keys-pinned-sentinel",
    "zipmap-empty",
    "zipmap-shortest",
    "zipmap-duplicate",
    "zipmap-promotion",
    "gensym-both-arities-counter",
    "gensym-counter-reuse",
    "gensym-immutable-nil-guard",
];
fn tagged(form: &Form) -> Value {
    assert!(form.metadata.is_empty(), "unexpected observation metadata");
    match &form.kind {
        Kind::Nil => json!({"tag":"nil"}),
        Kind::Bool(value) => json!({"tag":"bool", "value":value}),
        Kind::Number(value) => json!({"tag":"f64", "bits":format!("{:016x}",value.to_bits())}),
        Kind::String(units) => json!({"tag":"string", "units":units}),
        Kind::Vector(items) => {
            json!({"tag":"vector", "items":items.iter().map(tagged).collect::<Vec<_>>()})
        }
        other => panic!("unsupported decoded observation {other:?}"),
    }
}
#[test]
fn whole_record_graph_runtime_functions_match_raw_pinned_values_and_traces_after_gc() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/record-graph-runtime-cases.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(
        cases
            .iter()
            .map(|case| case["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        IDS
    );
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // New authored finite allowance, not yet measured/certified.
        session.set_operation_fuel(100_000_000);
        let bridge = FormBridge::new(&mut session).unwrap();
        for case in cases {
            let source = case["source"].as_str().unwrap();
            let value = session.eval(source).unwrap();
            session.collect().unwrap();
            let form = bridge.read(&mut session, &value, 0..source.len()).unwrap();
            assert_eq!(tagged(&form), case["expected"], "{}", case["id"]);
        }
    }
}
#[test]
fn composition_partial_and_juxt_retain_sole_host_callback_captures_across_invocations() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let bridge = FormBridge::new(&mut session).unwrap();
        for (source, expected) in [
            (
                "(let [captured [17 23]] (comp (fn [v] [(get captured 1) v]) vector))",
                json!({"tag":"vector","items":[{"tag":"f64","bits":"4037000000000000"},{"tag":"vector","items":[{"tag":"bool","value":false}]}]}),
            ),
            (
                "(let [captured [17 23]] (partial (fn [captured x] [(get captured 1) x]) captured))",
                json!({"tag":"vector","items":[{"tag":"f64","bits":"4037000000000000"},{"tag":"bool","value":false}]}),
            ),
            (
                "(let [captured [17 23]] (juxt (fn [x] (get captured 1)) (fn [x] x)))",
                json!({"tag":"vector","items":[{"tag":"f64","bits":"4037000000000000"},{"tag":"bool","value":false}]}),
            ),
        ] {
            let callable = session.eval(source).unwrap();
            session.collect().unwrap();
            for _ in 0..3 {
                let input = session.eval("false").unwrap();
                let value = session.invoke(&callable, &[&input]).unwrap();
                session.collect().unwrap();
                assert_eq!(
                    tagged(&bridge.read(&mut session, &value, 0..0).unwrap()),
                    expected
                );
            }
        }
    }
}
