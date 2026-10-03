use suss_cli::{portable_macro_data::FormBridge, portable_session::Session};
use suss_reader::forms::{Form, Kind};

fn tagged(form: &Form) -> serde_json::Value {
    match &form.kind {
        Kind::Bool(value) => serde_json::json!({"tag":"bool", "value":value}),
        Kind::Nil => serde_json::json!({"tag":"nil"}),
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
                    if actual != case["expected"] {
                        failures.push(format!(
                            "{phase} {}: actual {actual}, expected {}",
                            case["id"], case["expected"]
                        ));
                    }
                }
                Err(error) => failures.push(format!("{phase} {} transport: {error:?}", case["id"])),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
