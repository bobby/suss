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
        Kind::Number(v) => json!({"tag":"f64","bits":format!("{:016x}", v.to_bits())}),
        Kind::Vector(xs) => json!({"tag":"vector","items":xs.iter().map(raw).collect::<Vec<_>>()}),
        other => panic!("Unexpected independently decoded raw value: {other:?}"),
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
                row["expected"],
                "{} postGC original value/throw and numeric effect trace",
                row["id"]
            );
            let counter = session.eval("char-effects").unwrap();
            session.collect().unwrap();
            assert_eq!(
                raw(&bridge.read(&mut session, &counter, 0..0).unwrap()),
                row["expected"]["items"][1],
                "{} independent postGC counter",
                row["id"]
            );
            let recovery = session.eval("(quot 29 5)").unwrap();
            session.collect().unwrap();
            assert_eq!(
                raw(&bridge.read(&mut session, &recovery, 0..0).unwrap()),
                json!({"tag":"f64","bits":"4014000000000000"})
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
