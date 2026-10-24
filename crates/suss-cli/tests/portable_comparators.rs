//! Complete comparator source draft: both phases and independent scalar decoding.
//! Authored only; bootstrap regeneration and all native execution remain pending.
#[path = "support/eval_with_compilation_fuel.rs"]
mod semantic_compilation;
use suss_cli::portable_session::{Session, SessionError, SessionOptions, SessionValue};
use suss_compile::portable::resolve::Phase;
use wasmtime::Val;
const CORE: &str = include_str!("../../../runtime/core-import/suss/core.sus");
const PROBE: &str = include_str!("../../../tests/oracle/fixtures/comparator-probe.sus");
const IDS: &[&str] = &[
    "identity-object",
    "nil-order",
    "distinct-nil-undefined",
    "number-nan",
    "number-infinity",
    "numeric-order",
    "utf16-order",
    "boolean-order",
    "arrays-lexical",
    "array-holes-nil-undefined",
    "array-nested-cycle",
    "comparable-return-not-normalized",
    "comparable-identity-no-call",
    "comparable-original-false",
    "numeric-before-comparable",
    "array-reconvert-both-relations",
    "array-first-relation-only",
    "array-conversion-changes-between-tests",
    "array-override-valueOf",
    "array-type-constructor-nil",
    "array-type-constructor-false",
    "array-type-constructor-mismatch",
    "live-type-mixed",
    "live-array-predicate",
    "public-primitive-predicate-rebind",
    "wrapper-number-unchanged",
    "wrapper-truthy-empty",
    "wrapper-false-nil-undefined",
    "wrapper-reverse-zero-truthy",
    "wrapper-reverse-nan-truthy",
    "wrapper-reverse-effects",
    "wrapper-number-once",
    "wrapper-first-throw-no-reverse",
    "wrapper-reverse-throw",
    "wrapper-live-compare",
    "wrapper-equality-changes-compare",
    "indexed-length-first",
    "indexed-four-arity",
    "indexed-stops-first-difference",
    "indexed-live-compare",
    "format-toString-object-fallback",
    "format-noncallable-toString",
    "format-order",
    "format-throw-stops-second",
    "format-live-str",
    "str-nil-single-variadic-rebind",
    "str-second-lookup-after-first",
    "str-nonprimitive-both-throw",
    "format-live-str-results-convert-after-calls",
];
fn setup(phase: Phase) -> Session {
    let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
    session.set_operation_fuel(100_000_000);
    semantic_compilation::eval(&mut session, CORE).unwrap();
    session.enter_namespace("user").unwrap();
    session.eval(PROBE).unwrap();
    session.collect().unwrap();
    session
}
fn bits(session: &mut Session, value: &SessionValue) -> u64 {
    session
        .inspect(value, |mut store, value| {
            let object = value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .expect("Number");
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Number layout")
            };
            Ok(*bits)
        })
        .unwrap()
}
#[test]
fn complete_comparator_source_candidates_and_policy_in_both_phases_after_gc() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/comparator-cases.json")).unwrap();
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), IDS.len());
    for (case, expected_id) in cases.iter().zip(IDS) {
        assert_eq!(case["id"].as_str().unwrap(), *expected_id);
        assert_eq!(
            case["expected"],
            serde_json::json!({"tag":"bool","value":true})
        );
    }
    let policy: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/comparator-policy-cases.json"
    ))
    .unwrap();
    assert_eq!(policy["cases"].as_array().unwrap().len(), 2);
    assert_eq!(policy["cases"][0]["id"], "compare-wrong-arity");
    assert_eq!(policy["cases"][1]["id"], "wrapper-wrong-arity");
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = setup(phase);
        for case in cases.iter().chain(policy["cases"].as_array().unwrap()) {
            let source = case["source"].as_str().unwrap();
            let value = session
                .eval(source)
                .unwrap_or_else(|e| panic!("{}: {source}: {e}", case["id"]));
            session.collect().unwrap();
            session
                .inspect(&value, |store, value| {
                    assert_eq!(
                        value
                            .unwrap_anyref()
                            .unwrap()
                            .as_i31(&store)?
                            .expect("Boolean")
                            .get_u32(),
                        4,
                        "{}",
                        case["id"]
                    );
                    Ok(())
                })
                .unwrap();
        }
    }
}
#[test]
fn comparator_numeric_return_bits_errors_and_retained_capture_after_gc() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = setup(phase);
        for (source, expected) in [
            (
                "((fn->comparator (fn [_ _] -0.0)) 2 3)",
                (-0.0_f64).to_bits(),
            ),
            (
                "((fn->comparator (fn [_ _] ##Inf)) 2 3)",
                f64::INFINITY.to_bits(),
            ),
            (
                "((fn->comparator (fn [_ _] ##-Inf)) 2 3)",
                f64::NEG_INFINITY.to_bits(),
            ),
        ] {
            let value = session.eval(source).unwrap();
            session.collect().unwrap();
            assert_eq!(bits(&mut session, &value), expected, "{source}");
        }
        let value = session
            .eval("((fn->comparator (fn [_ _] ##NaN)) 2 3)")
            .unwrap();
        session.collect().unwrap();
        assert!(f64::from_bits(bits(&mut session, &value)).is_nan());
        for (source, message) in [
            ("(compare 1 \"x\")", "Cannot compare 1 to x"),
            ("(compare false 0)", "Cannot compare false to 0"),
            ("(compare (array 1 2) \"x\")", "Cannot compare 1,2 to x"),
        ] {
            let Err(SessionError::Language(payload)) = session.eval(source) else {
                panic!("language Error required: {source}")
            };
            session.collect().unwrap();
            session
                .inspect(&payload, |mut store, value| {
                    let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    let fields = object.fields(&mut store)?.collect::<Vec<_>>();
                    assert_eq!(fields.len(), 5, "Exception layout");
                    let descriptor = fields[0]
                        .unwrap_anyref()
                        .unwrap()
                        .as_struct(&store)?
                        .unwrap();
                    assert_eq!(
                        descriptor.field(&mut store, 0)?.unwrap_i64(),
                        7,
                        "Error descriptor"
                    );
                    let text = fields[1]
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    let units = text
                        .elems(&mut store)?
                        .map(|v| v.unwrap_i32() as u16)
                        .collect::<Vec<_>>();
                    assert_eq!(units, message.encode_utf16().collect::<Vec<_>>());
                    Ok(())
                })
                .unwrap();
            let value = session.eval("(compare 2 3)").unwrap();
            assert_eq!(bits(&mut session, &value), (-1.0_f64).to_bits());
        }
        session.eval("(def captured-cmp (let [v [17 23]] (fn->comparator (fn [x y] (- (+ (nth v 1) x) y)))))").unwrap();
        session.collect().unwrap();
        session.eval("(def saved-cmp captured-cmp)").unwrap();
        session.eval("(def captured-cmp nil)").unwrap();
        session.collect().unwrap();
        let value = session.eval("(saved-cmp 2 3)").unwrap();
        session.collect().unwrap();
        assert_eq!(bits(&mut session, &value), 22.0_f64.to_bits());
    }
}
