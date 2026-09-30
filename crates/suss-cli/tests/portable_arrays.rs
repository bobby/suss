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

fn check_array_cases(range: std::ops::Range<usize>) {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/array-cases.json")).unwrap();
    let mut session = Session::new().unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for case in &corpus["cases"].as_array().unwrap()[range.clone()] {
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
    assert_eq!(ids.len(), range.len());
}

#[test]
fn arrays_match_independently_encoded_primary_corpus() {
    check_array_cases(0..40);
}
#[test]
fn array_scalar_storage_identity_and_missing_slots() {
    check_array_cases(0..11);
}
#[test]
fn array_mutation_growth_and_shallow_clone() {
    check_array_cases(11..20);
}
#[test]
fn array_make_literal_dynamic_and_nested_dimensions() {
    check_array_cases(20..29);
}
#[test]
fn array_first_class_functions_and_native_protocols() {
    check_array_cases(29..36);
}
#[test]
fn array_argument_order_and_retained_function_values() {
    check_array_cases(36..40);
}
