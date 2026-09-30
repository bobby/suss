use suss_cli::portable_session::{Session, SessionValue};
use wasmtime::Val;

fn number(session: &mut Session, value: &SessionValue) -> u64 {
    session
        .inspect(value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Number layout")
            };
            Ok(*bits)
        })
        .unwrap()
}
fn eval_number(session: &mut Session, source: &str) -> u64 {
    let value = session.eval(source).unwrap();
    number(session, &value)
}
fn eval_bool(session: &mut Session, source: &str) -> bool {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |store, value| {
            match value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32()
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
                serde_json::json!({"tag":"f64", "bits":format!("{:016x}",eval_number(&mut session, source))})
            }
            tag => panic!("unexpected tag {tag}"),
        };
        assert_eq!(actual, case["expected"], "{source}");
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
    assert_eq!(number(&mut session, &result), 42.0f64.to_bits());
    session.collect().unwrap();
    let result = session.invoke(&reader, &[]).unwrap();
    assert_eq!(number(&mut session, &result), 42.0f64.to_bits());
}
