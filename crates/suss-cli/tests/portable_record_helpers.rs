//! Complete source helper group; authored UNCOMPILED/UNEXECUTED until bootstrap
//! regeneration and the exclusive Cargo lane. This does not certify reify.
use suss_cli::portable_session::Session;

const CASE_IDS: &[&str] = &[
    "vary-meta-args-0",
    "vary-meta-args-1",
    "vary-meta-args-2",
    "vary-meta-args-3",
    "vary-meta-args-4",
    "vary-meta-args-5",
    "vary-meta-effect-once",
    "fnil-defaults-1-args-1",
    "fnil-defaults-1-args-2",
    "fnil-defaults-1-args-3",
    "fnil-defaults-1-args-5",
    "fnil-defaults-2-args-2",
    "fnil-defaults-2-args-3",
    "fnil-defaults-2-args-5",
    "fnil-defaults-3-args-2",
    "fnil-defaults-3-args-3",
    "fnil-defaults-3-args-5",
    "fnil-false-values",
    "fnil-argument-order-and-once",
    "fnil-immutable-nil-macro",
    "update-in-args-0",
    "update-in-args-1",
    "update-in-args-2",
    "update-in-args-3",
    "update-in-args-4",
    "update-in-missing-path",
    "update-in-empty-path",
    "update-in-throw-order",
    "group-by-empty",
    "group-by-nil-false-keys",
    "group-by-callback-order-once",
    "group-by-transient-promotion",
    "group-by-callback-throw-order",
];

#[test]
fn complete_record_helpers_match_closed_pinned_cases_in_both_phases_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/record-helper-cases.json"
    ))
    .unwrap();
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    let ids = cases
        .iter()
        .map(|case| case["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(ids, CASE_IDS);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for case in cases {
            assert_eq!(
                case["expected"],
                serde_json::json!({"tag":"bool", "value":true})
            );
            let value = session.eval(case["source"].as_str().unwrap()).unwrap();
            session.collect().unwrap();
            let actual = session
                .inspect(&value, |store, value| {
                    let raw = value
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&store)?
                        .expect("Boolean ABI")
                        .get_u32();
                    Ok(match raw {
                        2 => false,
                        4 => true,
                        _ => panic!("unexpected Boolean sentinel {raw}"),
                    })
                })
                .unwrap();
            assert_eq!(
                serde_json::json!({"tag":"bool", "value":actual}),
                case["expected"],
                "{}",
                case["id"]
            );
        }
    }
}

#[test]
fn fnil_sole_host_handle_keeps_callback_captures_and_defaults_across_gc() {
    use suss_cli::portable_macro_data::FormBridge;
    use suss_reader::forms::Kind;
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let callable = session.eval("(let [captured [17 23]] (fnil (fn [a & more] [(get captured 1) a (count more)]) false))").unwrap();
        session.collect().unwrap();
        let nil = session.eval("nil").unwrap();
        let nine = session.eval("9").unwrap();
        for extra in [false, true, false] {
            let args = if extra { vec![&nil, &nine] } else { vec![&nil] };
            let result = session.invoke(&callable, &args).unwrap();
            session.collect().unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let decoded = bridge.read(&mut session, &result, 0..0).unwrap();
            let Kind::Vector(items) = decoded.kind else {
                panic!("source vector result")
            };
            assert_eq!(items.len(), 3);
            let Kind::Number(first) = &items[0].kind else {
                panic!("captured payload")
            };
            assert_eq!(first.to_bits(), 23.0_f64.to_bits());
            assert!(
                matches!(&items[1].kind, Kind::Bool(false)),
                "false default is retained"
            );
            let Kind::Number(count) = &items[2].kind else {
                panic!("variadic count")
            };
            let expected_count = if extra { 1.0_f64 } else { 0.0_f64 };
            assert_eq!(count.to_bits(), expected_count.to_bits());
        }
    }
}

// Host comparisons use decoded values and binary64 bits, never guest equality.
fn raw_helper_tag(form: &suss_reader::forms::Form) -> serde_json::Value {
    use serde_json::json;
    use suss_reader::forms::Kind;
    assert!(
        form.metadata.is_empty(),
        "unexpected raw observation metadata"
    );
    match &form.kind {
        Kind::Nil => json!({"tag":"nil"}),
        Kind::Bool(value) => json!({"tag":"bool", "value":value}),
        Kind::Number(value) => json!({"tag":"f64", "bits":format!("{:016x}", value.to_bits())}),
        Kind::Vector(items) => {
            json!({"tag":"vector", "items":items.iter().map(raw_helper_tag).collect::<Vec<_>>()})
        }
        Kind::Map(items) => {
            assert_eq!(items.len() % 2, 0, "complete decoded map entries");
            json!({"tag":"map", "entries":items.chunks_exact(2).map(|entry|
                vec![raw_helper_tag(&entry[0]), raw_helper_tag(&entry[1])]).collect::<Vec<_>>()})
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
fn record_helper_raw_values_and_traces_match_pinned_bits_in_both_phases_after_gc() {
    use suss_cli::portable_macro_data::FormBridge;
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/record-helper-raw-cases.json"
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
            assert_eq!(raw_helper_tag(&decoded), case["expected"], "{}", case["id"]);
        }
    }
}
