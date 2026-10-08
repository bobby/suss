//! Original chunk-demand/Reduced/retry fixture; native execution pending.
use serde_json::{Value, json};
use suss_cli::{portable_macro_data::FormBridge, portable_session::Session};
use suss_reader::forms::{Form, Kind};

const FIXTURE: &str = include_str!("../../../tests/oracle/fixtures/chunk-effect-probe.sus");
const CASES: &str = include_str!("../../../tests/oracle/chunk-effect-cases.json");
const IDS: [&str; 9] = [
    "demand-31",
    "demand-32",
    "demand-33",
    "demand-65",
    "chunk-retry",
    "reduced-0",
    "reduced-31",
    "reduced-32",
    "concat-boundary",
];

// Decode actual canonical storage through the existing bounded FormBridge,
// without guest equality or printing. Unknown result kinds fail, never wildcard.
fn tagged(form: &Form) -> Value {
    assert!(form.metadata.is_empty(), "unexpected observation metadata");
    match &form.kind {
        Kind::Number(n) => json!({"tag":"f64", "bits":format!("{:016x}", n.to_bits())}),
        Kind::Bool(v) => json!({"tag":"bool", "value":v}),
        Kind::Vector(items) => {
            json!({"tag":"vector", "items":items.iter().map(tagged).collect::<Vec<_>>()})
        }
        other => panic!("unsupported actual observation kind: {other:?}"),
    }
}
fn observe(session: &mut Session, bridge: &FormBridge, source: &str) -> Value {
    let value = session
        .eval(source)
        .unwrap_or_else(|e| panic!("{source}: {e}"));
    session.collect().unwrap();
    tagged(&bridge.read(session, &value, 0..source.len()).unwrap())
}
fn number(n: f64) -> Value {
    json!({"tag":"f64","bits":format!("{:016x}",n.to_bits())})
}
fn vector(items: Vec<Value>) -> Value {
    json!({"tag":"vector","items":items})
}

#[test]
fn chunk_effects_match_complete_pinned_observations_in_both_phases_after_gc() {
    let corpus: Value = serde_json::from_str(CASES).unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), IDS.len());
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // Finite existing sequence stress policy; max input is 65 elements.
        // This allowance is authored, not measured/proven sufficient yet.
        session.set_operation_fuel(100_000_000);
        session.eval(FIXTURE).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        for (case, id) in cases.iter().zip(IDS) {
            assert_eq!(case["id"], id);
            let actual = observe(&mut session, &bridge, case["source"].as_str().unwrap());
            assert_eq!(
                actual,
                case["expected"],
                "phase={:?}, case={id}",
                session.phase()
            );
        }
    }
}

#[test]
fn chunk_demand_cache_and_retry_survive_gc_between_source_fragments() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session.eval(FIXTURE).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.eval("(chunk-effect-reset) (def retained-chunks (chunk-effect-producer (chunk-effect-input 65)))").unwrap();
        session.collect().unwrap();
        assert_eq!(
            observe(&mut session, &bridge, "(first retained-chunks)"),
            number(0.0)
        );
        assert_eq!(
            observe(&mut session, &bridge, "(nth retained-chunks 31)"),
            number(31.0)
        );
        assert_eq!(
            observe(&mut session, &bridge, "chunk-effect-attempts"),
            number(1.0)
        );
        assert_eq!(
            observe(&mut session, &bridge, "chunk-effect-trace"),
            vector((0..32).map(|n| number(n as f64)).collect())
        );
        session.eval("(set! chunk-effect-fail true)").unwrap();
        assert_eq!(
            observe(
                &mut session,
                &bridge,
                "(try (nth retained-chunks 32) (catch :default e e))"
            ),
            number(77.0)
        );
        assert_eq!(
            observe(&mut session, &bridge, "chunk-effect-attempts"),
            number(2.0)
        );
        assert_eq!(
            observe(&mut session, &bridge, "chunk-effect-trace"),
            vector((0..33).map(|n| number(n as f64)).collect())
        );
        session.eval("(set! chunk-effect-fail false)").unwrap();
        session.collect().unwrap();
        assert_eq!(
            observe(&mut session, &bridge, "(nth retained-chunks 32)"),
            number(32.0)
        );
        assert_eq!(
            observe(&mut session, &bridge, "(nth retained-chunks 33)"),
            number(33.0)
        );
        assert_eq!(
            observe(&mut session, &bridge, "chunk-effect-attempts"),
            number(3.0)
        );
        let expected = (0..32)
            .chain([32])
            .chain(32..64)
            .map(|n| number(n as f64))
            .collect();
        assert_eq!(
            observe(&mut session, &bridge, "chunk-effect-trace"),
            vector(expected)
        );
        // Earlier chunk remains cached after failed and successful tail attempts.
        assert_eq!(
            observe(&mut session, &bridge, "(nth retained-chunks 31)"),
            number(31.0)
        );
        assert_eq!(
            observe(&mut session, &bridge, "chunk-effect-attempts"),
            number(3.0)
        );
    }
}
