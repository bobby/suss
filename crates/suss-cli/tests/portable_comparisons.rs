use suss_cli::portable_session::{Session, SessionError, SessionValue};
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
fn comparisons_match_independently_encoded_primary_corpus() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/comparison-cases.json")).unwrap();
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
    assert_eq!(ids.len(), 119);
}

#[test]
fn retained_comparison_captures_follow_accepted_design_with_explicit_primary_divergence() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/comparison-capture-divergences.json"
    ))
    .unwrap();
    let mut session = Session::new().unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        assert_eq!(case["expected-primary"]["tag"], "string");
        assert_eq!(
            eval_bool(&mut session, case["source"].as_str().unwrap()),
            case["expected-native"]["value"].as_bool().unwrap()
        );
        session.collect().unwrap();
    }
}

#[test]
fn comparison_macro_arity_erasure_and_lexical_shadowing_remain_distinct_from_values() {
    let mut session = Session::new().unwrap();
    for name in ["<", "<=", ">", ">=", "=="] {
        assert!(eval_bool(&mut session, &format!("({name} (throw 7))")));
        assert!(matches!(
            session.eval(&format!("({name})")),
            Err(SessionError::Compile(_))
        ));
        let original = session.eval(name).unwrap();
        session.collect().unwrap();
        assert!(matches!(
            session.invoke(&original, &[]),
            Err(SessionError::Language(_))
        ));
        assert_eq!(
            eval_number(
                &mut session,
                &format!("(let [{name} (fn [x] 77)] ({name} 1))")
            ),
            77.0f64.to_bits()
        );
        let argument = session.eval("##NaN").unwrap();
        let result = session.invoke(&original, &[&argument]).unwrap();
        let sentinel = session
            .inspect(&result, |store, value| {
                Ok(value
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .unwrap()
                    .get_u32())
            })
            .unwrap();
        assert_eq!(sentinel, 4);
    }
}
