use suss_cli::{portable_macro_data::FormBridge, portable_session::Session};
use suss_reader::forms::{Form, Kind};

fn tagged(form: &Form) -> serde_json::Value {
    match &form.kind {
        Kind::Bool(value) => serde_json::json!({"tag":"bool", "value":value}),
        Kind::Nil => serde_json::json!({"tag":"nil"}),
        Kind::Long(n) => serde_json::json!({"tag":"long", "value":n}),
        Kind::Number(n) => serde_json::json!({"tag":"f64", "bits":format!("{:016x}", n.to_bits())}),
        Kind::String(units) => serde_json::json!({"tag":"string", "units":units}),
        Kind::Symbol(symbol) => {
            serde_json::json!({"tag":"symbol", "namespace":symbol.namespace.as_ref().map(|name| serde_json::json!({"tag":"string", "units":name.encode_utf16().collect::<Vec<_>>()})).unwrap_or_else(|| serde_json::json!({"tag":"nil"})), "name":{"tag":"string", "units":symbol.name.encode_utf16().collect::<Vec<_>>()}})
        }
        Kind::Vector(items) => {
            serde_json::json!({"tag":"vector", "items":items.iter().map(tagged).collect::<Vec<_>>()})
        }
        Kind::List(items) => {
            serde_json::json!({"tag":"seq", "items":items.iter().map(tagged).collect::<Vec<_>>()})
        }
        other => panic!("Unexpected sequence observation: {other:?}"),
    }
}
/// The pinned ClojureScript observation with each integral `f64` as a Clojure
/// long, a distinction that corpus cannot record.
fn clojure_longs(expected: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match expected {
        Value::Object(fields) if fields.get("tag") == Some(&Value::from("f64")) => {
            let bits = u64::from_str_radix(fields["bits"].as_str().unwrap(), 16).unwrap();
            let value = f64::from_bits(bits);
            if value.is_finite()
                && value.fract() == 0.0
                && !(value == 0.0 && value.is_sign_negative())
                && value.abs() < 9_223_372_036_854_775_808.0
            {
                serde_json::json!({"tag":"long", "value":value as i64})
            } else {
                expected.clone()
            }
        }
        Value::Object(fields) => Value::Object(
            fields.iter().map(|(key, value)| (key.clone(), clojure_longs(value))).collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(clojure_longs).collect()),
        other => other.clone(),
    }
}

/// JVM Clojure 1.12's observation of a hash-ordered case of `corpus`, which
/// replaces its pinned ClojureScript order (#336).
fn clojure_order(corpus: &str, id: &str) -> Option<serde_json::Value> {
    let observations: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/clojure-hash-order-observations.json"
    ))
    .unwrap();
    observations["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["corpus"] == corpus && case["id"] == id)
        .map(|case| case["expected"].clone())
}

#[test]
fn compiled_macro_sets_match_pinned_source_observations_in_both_phases_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/set-data-cases.json"
    ))
    .unwrap();
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 39);
    let mut failures = Vec::new();
    for (phase, mut session) in [
        ("Runtime", Session::new_repl().unwrap()),
        ("Macro", Session::new_macro().unwrap()),
    ] {
        session.set_operation_fuel(100_000_000);
        let bridge = FormBridge::new(&mut session).unwrap();
        for case in corpus["cases"].as_array().unwrap() {
            let source = case["source"].as_str().unwrap();
            let value = match session.eval(source) {
                Ok(value) => value,
                Err(error) => {
                    failures.push(format!("{phase} {} execution: {error:?}", case["id"]));
                    continue;
                }
            };
            session.collect().unwrap();
            match bridge.read(&mut session, &value, 70..80) {
                Ok(form) => {
                    assert_eq!(form.span, 70..80);
                    let actual = tagged(&form);
                    let expected = clojure_order("set-data-cases.json", case["id"].as_str().unwrap())
                        .unwrap_or_else(|| clojure_longs(&case["expected"]));
                    if actual != expected {
                        failures.push(format!(
                            "{phase} {}: actual {actual}, expected {expected}",
                            case["id"]
                        ));
                    }
                }
                Err(error) => failures.push(format!("{phase} {} transport: {error:?}", case["id"])),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
