use suss_cli::portable_session::{RuntimeNumber, Session, SessionValue};

fn number(session: &mut Session, value: &SessionValue) -> RuntimeNumber {
    session
        .inspect(value, |store, value| {
            Ok(suss_cli::portable_session::runtime_number(store, &value)?.expect("Number layout"))
        })
        .unwrap()
}
fn eval_number(session: &mut Session, source: &str) -> RuntimeNumber {
    let value = session.eval(source).unwrap();
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
fn iteration_contract_match_independently_encoded_primary_observations() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/iteration-cases.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 59);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    let mut identities = std::collections::BTreeSet::new();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        assert!(identities.insert(id));
        let source = case["source"].as_str().unwrap();
        let value = session
            .eval(source)
            .unwrap_or_else(|error| panic!("{id}: {source}: {error:?}"));
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "bool" => serde_json::json!({"tag":"bool", "value":boolean(&mut session, &value)}),
            "f64" => {
                number_json(number(&mut session, &value))
            }
            tag => panic!("unexpected iteration observation tag {tag}"),
        };
        assert_eq!(actual, clojure_number(&case["expected"]), "{id}: {source}");
        session.collect().unwrap();
    }
}

#[test]
fn retained_iterators_and_reverse_views_survive_gc_and_explicit_adapter_errors() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def iteration-array (array 17 19)) (def retained-iterator (-iterator (seq iteration-array))) (def retained-reverse (rseq (seq iteration-array)))").unwrap();
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(.next retained-iterator)"),
        RuntimeNumber::Long(17)
    );
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(.next retained-iterator)"),
        RuntimeNumber::Long(19)
    );
    assert_eq!(
        eval_number(&mut session, "(first retained-reverse)"),
        RuntimeNumber::Long(19)
    );
    let boundary: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/iteration-length-boundary.json"
    ))
    .unwrap();
    let cases = boundary["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0]["id"], "iterator-array-shrink");
    assert_eq!(
        cases[0]["expected"],
        serde_json::json!({"tag":"bool","value":false})
    );
    // Observe the retained iterator's live length against the pinned boundary.
    let shrunk = session.eval(cases[0]["source"].as_str().unwrap()).unwrap();
    session.collect().unwrap();
    assert!(!boolean(&mut session, &shrunk));
    for source in ["(rseq)"] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Language(_))),
            "{source}"
        );
        session.collect().unwrap();
        assert_eq!(
            eval_number(&mut session, "(count iteration-array)"),
            RuntimeNumber::Long(2)
        );
        assert_eq!(
            eval_number(&mut session, "(first retained-reverse)"),
            RuntimeNumber::Long(19)
        );
    }
    session
        .eval("(aset iteration-array 1 23) (aset iteration-array 2 29)")
        .unwrap();
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(first retained-reverse)"),
        RuntimeNumber::Long(23)
    );
    assert_eq!(
        eval_number(&mut session, "(.next retained-iterator)"),
        RuntimeNumber::Long(29)
    );
    assert_eq!(
        eval_number(&mut session, "(count retained-reverse)"),
        RuntimeNumber::Long(2)
    );
}
