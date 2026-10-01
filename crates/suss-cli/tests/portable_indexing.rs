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
fn indexing_contract_match_independently_encoded_primary_observations() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/indexing-cases.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 56);
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
            tag => panic!("unexpected indexing observation tag {tag}"),
        };
        assert_eq!(actual, case["expected"], "{id}: {source}");
        session.collect().unwrap();
    }
}

#[test]
fn indexing_errors_preserve_messages_effects_and_gc_recovery() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session
        .eval("(def saved-nth nth) (def indexing-values (list 17 19)) (def indexing-effects 0)")
        .unwrap();
    session.collect().unwrap();
    for (source, message) in [
        ("(nth nil \"0\")", "Index argument to nth must be a number"),
        (
            "(nth nil \"0\" 17)",
            "Index argument to nth must be a number.",
        ),
        ("(nth (list 1) 9)", "Index out of bounds"),
        ("(nth (array 1) -1)", "Index out of bounds"),
        ("(nth \"A\" ##NaN)", "Index out of bounds"),
        ("(nth (seq (array 1)) 9)", "Index out of bounds"),
    ] {
        let Err(SessionError::Language(value)) = session.eval(source) else {
            panic!("typed indexing error: {source}");
        };
        session.collect().unwrap();
        let units = session
            .inspect(&value, |mut store, value| {
                let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let fields = object.fields(&mut store)?.collect::<Vec<_>>();
                assert_eq!(fields.len(), 4, "ABI Error");
                let array = fields[1]
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                Ok(array
                    .elems(&mut store)?
                    .map(|v| v.unwrap_i32() as u16)
                    .collect::<Vec<_>>())
            })
            .unwrap();
        assert_eq!(units, message.encode_utf16().collect::<Vec<_>>());
        assert_eq!(
            eval_number(&mut session, "(saved-nth indexing-values 1)"),
            19.0f64.to_bits()
        );
    }
    assert!(matches!(
        session.eval("(saved-nth (do (set! indexing-effects 17) indexing-values))"),
        Err(SessionError::Language(_))
    ));
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "indexing-effects"),
        17.0f64.to_bits()
    );
    assert_eq!(
        eval_number(&mut session, "(.lastIndexOf indexing-values 17)"),
        0.0f64.to_bits()
    );
    for source in [
        "(neg?)",
        "(neg? 1 2)",
        "(inc)",
        "(inc 1 2)",
        "(dec)",
        "(dec 1 2)",
    ] {
        let Err(SessionError::Compile(error)) = session.eval(source) else {
            panic!("located macro arity: {source}");
        };
        assert!(error.span.end > error.span.start);
        assert!(error.message.contains("one operand"));
    }
    assert!(matches!(
        session.eval("(def indexing-ghost 7) (neg? 1 2)"),
        Err(SessionError::Compile(_))
    ));
    assert!(matches!(
        session.eval("indexing-ghost"),
        Err(SessionError::Compile(_))
    ));
    assert_eq!(
        eval_number(&mut session, "(saved-nth indexing-values 0)"),
        17.0f64.to_bits()
    );
}
