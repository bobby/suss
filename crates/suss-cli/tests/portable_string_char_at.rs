//! Genuine String.charAt member / complete get regressions; both phases after GC.
use serde_json::{Value, json};
use suss_cli::{portable_macro_data::FormBridge, portable_session::Session};
use suss_reader::forms::{Form, Kind};
fn raw(form: &Form) -> Value {
    assert!(form.metadata.is_empty());
    match &form.kind {
        Kind::Nil => json!({"tag":"nil"}),
        Kind::Bool(v) => json!({"tag":"bool","value":v}),
        Kind::String(units) => json!({"tag":"string","units":units}),
        Kind::Long(v) => json!({"tag":"long","value":v}),
        Kind::Number(v) => json!({"tag":"f64","bits":format!("{:016x}", v.to_bits())}),
        Kind::Vector(xs) => json!({"tag":"vector","items":xs.iter().map(raw).collect::<Vec<_>>()}),
        other => panic!("Unexpected independently decoded raw value: {other:?}"),
    }
}
/// The ClojureScript oracle encodes every number as a double; JVM Clojure,
/// Suss's oracle, gives a long for these integral results.
fn clojure_value(expected: &Value) -> Value {
    match expected {
        Value::Object(fields) if fields.get("tag").and_then(Value::as_str) == Some("f64") => {
            let bits = u64::from_str_radix(fields["bits"].as_str().unwrap(), 16).unwrap();
            let value = f64::from_bits(bits);
            if value.is_finite() && value.fract() == 0.0 && bits != (-0.0f64).to_bits() {
                json!({"tag":"long", "value":value as i64})
            } else {
                expected.clone()
            }
        }
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), clojure_value(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(clojure_value).collect()),
        other => other.clone(),
    }
}
#[test]
fn genuine_char_at_and_complete_get_match_raw_pin_after_gc_and_recover() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/string-char-at-cases.json"
    ))
    .unwrap();
    let rows = corpus["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 74);
    let mut ids = std::collections::BTreeSet::new();
    for row in rows {
        assert!(ids.insert(row["id"].as_str().unwrap()));
    }
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session.eval("(def char-effects 0)").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        for row in rows {
            session.eval("(set! char-effects 0)").unwrap();
            let source = row["source"].as_str().unwrap();
            let result = session
                .eval(source)
                .unwrap_or_else(|error| panic!("{}: {error:?}", row["id"]));
            session.collect().unwrap();
            let actual = bridge.read(&mut session, &result, 0..source.len()).unwrap();
            assert_eq!(
                raw(&actual),
                clojure_value(&row["expected"]),
                "{} postGC original value/throw and numeric effect trace",
                row["id"]
            );
            let counter = session.eval("char-effects").unwrap();
            session.collect().unwrap();
            assert_eq!(
                raw(&bridge.read(&mut session, &counter, 0..0).unwrap()),
                clojure_value(&row["expected"]["items"][1]),
                "{} independent postGC counter",
                row["id"]
            );
            let recovery = session.eval("(quot 29 5)").unwrap();
            session.collect().unwrap();
            assert_eq!(
                raw(&bridge.read(&mut session, &recovery, 0..0).unwrap()),
                json!({"tag":"long","value":5})
            );
        }
    }
}
#[test]
fn char_at_public_method_identity_is_canonical_shared_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let first = session.eval("(.-charAt \"old\")").unwrap();
        let reference = session
            .inspect(&first, |mut store, value| {
                value.unwrap_anyref().unwrap().to_owned_rooted(&mut store)
            })
            .unwrap();
        session.collect().unwrap();
        let second = session.eval("(.-charAt \"new\")").unwrap();
        session.collect().unwrap();
        assert!(
            session
                .inspect(&second, |store, value| wasmtime::Rooted::ref_eq(
                    &store,
                    &reference,
                    value.unwrap_anyref().unwrap()
                ))
                .unwrap()
        );
    }
}
