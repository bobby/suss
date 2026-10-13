//! Whole sorted prerequisites; authored only, bootstrap/native execution pending.
use suss_cli::portable_session::{Session, SessionError, SessionOptions, SessionValue};
use suss_compile::portable::resolve::Phase;
use wasmtime::Val;
const IDS: &[&str] = &[
    "symbol-identity",
    "symbol-no-namespace",
    "symbol-left-namespace-absent",
    "symbol-right-namespace-absent",
    "symbol-namespace-before-name",
    "symbol-namespace-greater",
    "symbol-namespace-tie-name",
    "symbol-empty-name",
    "symbol-utf16",
    "symbol-no-public-comparator",
    "symbol-identity-skips-fields",
    "symbol-repeated-conversion-order",
    "symbol-conversion-throw-stops-right",
    "keyword-identity",
    "keyword-no-namespace",
    "keyword-left-namespace-absent",
    "keyword-right-namespace-absent",
    "keyword-namespace-before-name",
    "keyword-namespace-greater",
    "keyword-namespace-tie-name",
    "keyword-empty-name",
    "keyword-utf16",
    "keyword-no-public-comparator",
    "keyword-identity-skips-fields",
    "keyword-repeated-conversion-order",
    "keyword-conversion-throw-stops-right",
    "ci-empty-no-init",
    "ci-single-seed-no-call",
    "ci-no-init-ordered",
    "ci-empty-init-identity",
    "ci-init-ordered",
    "ci-reduced-no-init",
    "ci-reduced-init-false",
    "ci-reduced-init-nil",
    "ci-original-reducer-throw",
    "ci-live-reduced-predicate",
    "ci-count-before-nth-throw",
    "ci-live-deref",
    "ci-seed-false",
    "ci-seed-nil",
    "ci-typed-count-ignores-public-rebind",
    "ci-typed-nth-ignores-public-rebind",
    "symbol-namespace-difference-skips-name",
    "symbol-namespace-tie-conversion-before-name",
    "keyword-namespace-difference-skips-name",
    "keyword-namespace-tie-conversion-before-name",
    "ci-immutable-macro-paths",
];
fn setup(phase: Phase) -> Session {
    let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
    session.set_operation_fuel(100_000_000);
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session
        .eval(include_str!(
            "../../../tests/oracle/fixtures/sorted-prerequisite-probe.sus"
        ))
        .unwrap();
    session.collect().unwrap();
    session
}
fn number_bits(session: &mut Session, value: &SessionValue) -> u64 {
    session
        .inspect(value, |mut store, value| {
            let number = value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .expect("Number");
            let fields = number.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("boxed Number layout")
            };
            Ok(*bits)
        })
        .unwrap()
}
#[test]
fn complete_sorted_prerequisites_match_ordered_pinned_value_and_effect_cases_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/sorted-prerequisite-cases.json"
    ))
    .unwrap();
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), IDS.len());
    for (case, id) in cases.iter().zip(IDS) {
        assert_eq!(case["id"], *id);
        assert_eq!(
            case["expected"],
            serde_json::json!({"tag":"bool","value":true})
        );
    }
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = setup(phase);
        for case in cases {
            let source = case["source"].as_str().unwrap();
            let value = session
                .eval(source)
                .unwrap_or_else(|e| panic!("{}: {source}: {e}", case["id"]));
            session.collect().unwrap();
            session
                .inspect(&value, |store, value| {
                    let raw = value
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&store)?
                        .expect("Boolean ABI")
                        .get_u32();
                    assert_eq!(raw, 4, "{}", case["id"]);
                    Ok(())
                })
                .unwrap();
        }
    }
}
#[test]
fn sorted_prerequisite_returns_and_original_throw_payload_are_independently_decoded() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = setup(phase);
        for (source, expected) in [
            (
                "(ci-reduce [-0.0] (fn [_ _] (throw 73)))",
                (-0.0_f64).to_bits(),
            ),
            ("(ci-reduce [] (fn [] 17))", 17.0_f64.to_bits()),
            (
                "(ci-reduce [1 2] (fn [_ _] (reduced -0.0)))",
                (-0.0_f64).to_bits(),
            ),
            (
                "(ci-reduce [1] (fn [_ _] (reduced ##Inf)) 0)",
                f64::INFINITY.to_bits(),
            ),
            (
                "(compare-symbols (symbol \"n\" \"a\") (symbol \"n\" \"b\"))",
                (-1.0_f64).to_bits(),
            ),
            (
                "(compare-keywords (keyword \"n\" \"b\") (keyword \"n\" \"a\"))",
                1.0_f64.to_bits(),
            ),
        ] {
            let value = session.eval(source).unwrap();
            session.collect().unwrap();
            assert_eq!(number_bits(&mut session, &value), expected, "{source}");
        }
        let value = session
            .eval("(ci-reduce [1] (fn [_ _] (reduced ##NaN)) 0)")
            .unwrap();
        session.collect().unwrap();
        assert!(f64::from_bits(number_bits(&mut session, &value)).is_nan());
        session.eval("(def sorted-effect 0)").unwrap();
        let Err(SessionError::Language(payload)) = session.eval(
            "(ci-reduce [1 2] (fn [_ _] (set! sorted-effect (+ sorted-effect 1)) (throw 73)) 0)",
        ) else {
            panic!("original reducer throw required")
        };
        session.collect().unwrap();
        assert_eq!(number_bits(&mut session, &payload), 73.0_f64.to_bits());
        let trace = session.eval("sorted-effect").unwrap();
        session.collect().unwrap();
        assert_eq!(number_bits(&mut session, &trace), 1.0_f64.to_bits());
        let recovery = session.eval("(ci-reduce [17 23] + 0)").unwrap();
        session.collect().unwrap();
        assert_eq!(number_bits(&mut session, &recovery), 40.0_f64.to_bits());
    }
}
