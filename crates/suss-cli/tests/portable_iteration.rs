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
fn boolean(session: &mut Session, value: &SessionValue) -> bool {
    session
        .inspect(value, |store, value| {
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
                serde_json::json!({"tag":"f64", "bits":format!("{:016x}",number(&mut session, &value))})
            }
            tag => panic!("unexpected iteration observation tag {tag}"),
        };
        assert_eq!(actual, case["expected"], "{id}: {source}");
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
        17.0f64.to_bits()
    );
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(.next retained-iterator)"),
        19.0f64.to_bits()
    );
    assert_eq!(
        eval_number(&mut session, "(first retained-reverse)"),
        19.0f64.to_bits()
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
    // The pin permits named .length writes; this portable named-property adapter
    // explicitly rejects them. This is a certified separate boundary, not a skip.
    for source in [
        cases[0]["source"].as_str().unwrap(),
        "(set! (.-length iteration-array) 0)",
        "(rseq)",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Language(_))),
            "{source}"
        );
        session.collect().unwrap();
        assert_eq!(
            eval_number(&mut session, "(count iteration-array)"),
            2.0f64.to_bits()
        );
        assert_eq!(
            eval_number(&mut session, "(first retained-reverse)"),
            19.0f64.to_bits()
        );
    }
    session
        .eval("(aset iteration-array 1 23) (aset iteration-array 2 29)")
        .unwrap();
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(first retained-reverse)"),
        23.0f64.to_bits()
    );
    assert_eq!(
        eval_number(&mut session, "(.next retained-iterator)"),
        29.0f64.to_bits()
    );
    assert_eq!(
        eval_number(&mut session, "(count retained-reverse)"),
        2.0f64.to_bits()
    );
}
