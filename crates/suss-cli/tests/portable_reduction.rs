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
fn reduction_contract_match_independently_encoded_primary_observations() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/reduction-cases.json")).unwrap();
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
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "bool" => serde_json::json!({"tag":"bool", "value":eval_bool(&mut session, source)}),
            "f64" => {
                serde_json::json!({"tag":"f64", "bits":format!("{:016x}",eval_number(&mut session, source))})
            }
            tag => panic!("unexpected reduction observation tag {tag}"),
        };
        assert_eq!(actual, case["expected"], "{id}: {source}");
        session.collect().unwrap();
    }
}
