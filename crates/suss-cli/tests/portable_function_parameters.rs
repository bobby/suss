//! Authored native parameter regressions: UNCOMPILED; bootstrap remains stale.
use suss_cli::portable_session::Session;
const CASE_IDS: &[&str] = &[
    "vector-missing",
    "vector-nested",
    "vector-rest-empty",
    "vector-rest-nested",
    "vector-as",
    "variadic-rest-pattern",
    "map-keys",
    "map-qualified-keys",
    "map-syms",
    "map-strs",
    "map-nested",
    "map-as",
    "map-default-eager",
    "map-default-nil-false",
    "map-seq-pairs",
    "map-single-map",
    "map-empty-seq",
    "map-trailing-map",
    "recur-vector",
    "recur-map",
    "multi-arity-pattern",
    "argument-evaluation-once",
    "map-large-order",
    "map-shorthand-promotion-order",
    "map-dissoc-keeps-hash-order",
    "default-throw-and-recovery",
    "live-nth-order",
    "live-get-arities",
    "live-rest-next-before-nesting",
    "recur-defaults-repeat",
    "pinned-identifier-hash-collision",
    "symbol-collision-insertion-with-removal",
    "symbol-collision-reversed-with-removal",
    "nested-vector-collision-promotion-removal",
    "nested-map-pattern-key-hash-order",
    "array-node-promoted-shorthand-order",
    "default-present-still-throws",
    "captured-defaults-and-repeat",
    "vector-string-repeated-local",
];

fn raw_parameter_tag(form: &suss_reader::forms::Form) -> serde_json::Value {
    use serde_json::json;
    use suss_reader::forms::Kind;
    assert!(
        form.metadata.is_empty(),
        "unexpected raw observation metadata"
    );
    match &form.kind {
        Kind::Nil => json!({"tag":"nil"}),
        Kind::Bool(value) => json!({"tag":"bool", "value":value}),
        Kind::String(units) => json!({"tag":"string", "units":units}),
        Kind::Number(value) => json!({"tag":"f64", "bits":format!("{:016x}", value.to_bits())}),
        Kind::Vector(items) => {
            json!({"tag":"vector", "items":items.iter().map(raw_parameter_tag).collect::<Vec<_>>()})
        }
        Kind::Map(items) => {
            assert_eq!(items.len() % 2, 0, "complete decoded map entries");
            json!({"tag":"map", "entries":items.chunks_exact(2).map(|entry|
                vec![raw_parameter_tag(&entry[0]), raw_parameter_tag(&entry[1])]).collect::<Vec<_>>()})
        }
        Kind::Keyword(key) => json!({"tag":"keyword",
            "namespace":match &key.namespace {
                None => json!({"tag":"nil"}),
                Some(name) => json!({"tag":"string", "units":name.encode_utf16().collect::<Vec<_>>()}),
            },
            "name":{"tag":"string", "units":key.name.encode_utf16().collect::<Vec<_>>()}}),
        other => panic!("unsupported raw helper observation: {other:?}"),
    }
}

#[test]
fn function_parameters_match_raw_pinned_values_and_effect_traces_in_both_phases_after_gc() {
    use suss_cli::portable_macro_data::FormBridge;
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/function-parameter-cases.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(
        cases
            .iter()
            .map(|case| case["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        CASE_IDS
    );
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let bridge = FormBridge::new(&mut session).unwrap();
        for case in cases {
            let source = case["source"].as_str().unwrap();
            let result = session.eval(source).unwrap();
            session.collect().unwrap();
            let decoded = bridge.read(&mut session, &result, 0..source.len()).unwrap();
            assert_eq!(
                raw_parameter_tag(&decoded),
                case["expected"],
                "{}",
                case["id"]
            );
        }
    }
}

#[test]
fn destructured_callable_sole_host_handle_retains_default_captures_after_gc() {
    use suss_cli::portable_macro_data::FormBridge;
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let callable = session.eval("(let [captured [17 23]] (fn [[x] {:keys [y] :or {y (nth captured 1)}}] [x y (nth captured 0)]))").unwrap();
        session.collect().unwrap();
        for (map, expected) in [
            ("{}", serde_json::json!([31, 23, 17])),
            ("{:y false}", serde_json::json!([31, false, 17])),
            ("{:y nil}", serde_json::json!([31, null, 17])),
        ] {
            let vector = session.eval("[31]").unwrap();
            let map = session.eval(map).unwrap();
            session.collect().unwrap();
            let result = session.invoke(&callable, &[&vector, &map]).unwrap();
            session.collect().unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let decoded = bridge.read(&mut session, &result, 0..0).unwrap();
            let suss_reader::forms::Kind::Vector(items) = decoded.kind else {
                panic!("source vector result")
            };
            assert_eq!(items.len(), 3);
            for (actual, expected) in items.iter().zip(expected.as_array().unwrap()) {
                match &actual.kind {
                    suss_reader::forms::Kind::Number(value) => {
                        assert_eq!(value.to_bits(), expected.as_f64().unwrap().to_bits())
                    }
                    suss_reader::forms::Kind::Bool(value) => {
                        assert_eq!(*value, expected.as_bool().unwrap())
                    }
                    suss_reader::forms::Kind::Nil => assert!(expected.is_null()),
                    other => panic!("unexpected independently decoded value: {other:?}"),
                }
            }
        }
    }
}
