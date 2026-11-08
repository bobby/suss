use suss_cli::portable_session::{RuntimeNumber, Session, SessionValue};

#[path = "support/clojure_corrections.rs"]
mod clojure_corrections;

/// Recorded observations that coerced a non-number operand; Clojure rejects them.
const CORRECTIONS: &[clojure_corrections::Correction] = &[
    ("reduce-string-units", "", "+ requires number operands"),
];

fn number(session: &mut Session, value: &SessionValue) -> RuntimeNumber {
    session
        .inspect(value, |store, value| {
            Ok(suss_cli::portable_session::runtime_number(store, &value)?.expect("Number layout"))
        })
        .unwrap()
}
fn eval_number(session: &mut Session, source: &str) -> RuntimeNumber {
    let value = session.eval(source).unwrap_or_else(|error| panic!("{source}: {error}"));
    number(session, &value)
}

/// The observation of a runtime number, distinguishing a long from a double.
fn number_json(number: RuntimeNumber) -> serde_json::Value {
    match number {
        RuntimeNumber::Long(value) => serde_json::json!({"tag":"long", "value":value}),
        RuntimeNumber::Double(value) => {
            serde_json::json!({"tag":"f64", "bits":format!("{:016x}", value.to_bits())})
        }
    }
}
/// The ClojureScript oracle encodes every number as a double; JVM Clojure,
/// Suss's oracle, gives a long for these integral results.
fn clojure_number(expected: &serde_json::Value) -> serde_json::Value {
    if expected["tag"] == "f64" {
        let bits = u64::from_str_radix(expected["bits"].as_str().unwrap(), 16).unwrap();
        let value = f64::from_bits(bits);
        if value.is_finite() && value.fract() == 0.0 && bits != (-0.0f64).to_bits() {
            return serde_json::json!({"tag":"long", "value":value as i64});
        }
    }
    expected.clone()
}
fn eval_bool(session: &mut Session, source: &str) -> bool {
    let value = session.eval(source).unwrap_or_else(|error| panic!("{source}: {error}"));
    boolean(session, &value)
}
fn boolean(session: &mut Session, value: &SessionValue) -> bool {
    session
        .inspect(value, |mut store, value| {
            match suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap()
            {
                2 => Ok(false),
                4 => Ok(true),
                other => panic!("Boolean {other}"),
            }
        })
        .unwrap()
}

#[test]
fn reduction_contract_match_independently_encoded_primary_observations() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/reduction-cases.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 63);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    let mut identities = std::collections::BTreeSet::new();
    let mut corrected = 0;
    for case in cases {
        let case = &clojure_corrections::corrected(case, CORRECTIONS, &mut corrected);
        let id = case["id"].as_str().unwrap();
        assert!(identities.insert(id.to_owned()));
        let source = case["source"].as_str().unwrap();
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "bool" => serde_json::json!({"tag":"bool", "value":eval_bool(&mut session, source)}),
            "f64" => {
                number_json(eval_number(&mut session, source))
            }
            tag => panic!("unexpected reduction observation tag {tag}"),
        };
        assert_eq!(actual, clojure_number(&case["expected"]), "{id}: {source}");
        session.collect().unwrap();
    }
    assert_eq!(corrected, CORRECTIONS.len());
}

#[test]
fn reduced_values_and_saved_reducers_survive_gc_and_language_errors() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def retained-reduced (reduced (list 17 19))) (def retained-reducer reduce) (def reduction-error-trace 0)").unwrap();
    session.collect().unwrap();
    assert_eq!(
        eval_number(
            &mut session,
            "(retained-reducer + (deref retained-reduced))"
        ),
        RuntimeNumber::Long(36)
    );
    for source in [
        "(reduce)",
        "(reduce +)",
        "(array-reduce (array 1) + 0 0 (do (set! reduction-error-trace 17) nil))",
        "(reduce (fn [a] a) 0 (list 1 2))",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Language(_))),
            "{source}"
        );
        session.collect().unwrap();
        assert_eq!(
            eval_number(
                &mut session,
                "(retained-reducer + (deref retained-reduced))"
            ),
            RuntimeNumber::Long(36)
        );
    }
    assert_eq!(
        eval_number(&mut session, "reduction-error-trace"),
        RuntimeNumber::Long(17)
    );
    let Err(SessionError::Language(value)) = session.eval("(reduce (fn [a b] (set! reduction-error-trace (+ (* reduction-error-trace 10) b)) (throw 73)) 0 (list 1 2 3))") else { panic!("typed reducer exception"); };
    session.collect().unwrap();
    assert_eq!(number(&mut session, &value), RuntimeNumber::Long(73));
    assert_eq!(
        eval_number(&mut session, "reduction-error-trace"),
        RuntimeNumber::Long(171)
    );
    assert!(eval_bool(&mut session, "(reduced? retained-reduced)"));
    assert_eq!(
        eval_number(
            &mut session,
            "(retained-reducer + (deref retained-reduced))"
        ),
        RuntimeNumber::Long(36)
    );
}

#[test]
fn reduced_stops_vector_callbacks_at_chunk_boundaries_in_both_phases() {
    use suss_cli::portable_session::SessionOptions;
    use suss_compile::portable::resolve::Phase;
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/reduction-boundary-cases.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 108);
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
        session.set_operation_fuel(100_000_000);
        session
            .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
            .unwrap();
        session.enter_namespace("user").unwrap();
        session
            .eval(include_str!(
                "../../../tests/oracle/fixtures/reduction-boundary-probe.sus"
            ))
            .unwrap();
        let mut ids = std::collections::BTreeSet::new();
        for case in cases {
            let id = case["id"].as_str().unwrap();
            assert!(ids.insert(id));
            let source = case["source"].as_str().unwrap();
            let value = session
                .eval(source)
                .unwrap_or_else(|error| panic!("{phase:?}, {id}: {error}"));
            session.collect().unwrap();
            let actual = match case["expected"]["tag"].as_str().unwrap() {
                "bool" => {
                    serde_json::json!({"tag":"bool", "value":boolean(&mut session, &value)})
                }
                "f64" => {
                    number_json(number(&mut session, &value))
                }
                tag => panic!("unexpected boundary observation tag {tag}"),
            };
            assert_eq!(actual, clojure_number(&case["expected"]), "{phase:?}, {id}: {source}");
        }
    }
}
