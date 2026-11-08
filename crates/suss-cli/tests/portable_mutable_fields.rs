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
fn eval_bool(session: &mut Session, source: &str) -> bool {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |mut store, value| {
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
fn mutable_fields_match_independently_encoded_primary_corpus() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/mutable-field-cases.json"
    ))
    .unwrap();
    let mut session = Session::new().unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for case in corpus["cases"].as_array().unwrap() {
        assert!(ids.insert(case["id"].as_str().unwrap()));
        let source = case["source"].as_str().unwrap();
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "bool" => serde_json::json!({"tag":"bool", "value":eval_bool(&mut session, source)}),
            "f64" => {
                number_json(eval_number(&mut session, source))
            }
            tag => panic!("unexpected tag {tag}"),
        };
        assert_eq!(actual, clojure_number(&case["expected"]), "{source}");
        session.collect().unwrap();
    }
    assert_eq!(ids.len(), 32);
}

#[test]
fn retained_mutable_field_setter_keeps_owner_alive_without_global_roots() {
    let mut session = Session::new().unwrap();
    session.eval("(defprotocol P (reader [this]) (setter [this])) (deftype T [^:mutable x] P (reader [this] (fn [] x)) (setter [this] (fn [v] (set! x v)))) (def owner (T. 1))").unwrap();
    let reader = session.eval("(reader owner)").unwrap();
    let setter = session.eval("(setter owner)").unwrap();
    session
        .eval("(def owner nil) (def T nil) (def ->T nil) (def reader nil) (def setter nil)")
        .unwrap();
    session.collect().unwrap();
    let replacement = session.eval("42").unwrap();
    let result = session.invoke(&setter, &[&replacement]).unwrap();
    assert_eq!(number(&mut session, &result), RuntimeNumber::Long(42));
    session.collect().unwrap();
    let result = session.invoke(&reader, &[]).unwrap();
    assert_eq!(number(&mut session, &result), RuntimeNumber::Long(42));
}
