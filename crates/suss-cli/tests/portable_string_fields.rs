//! Full source adapters; authored, native UNCOMPILED/UNEXECUTED.
use suss_cli::portable_session::Session;
const IDS: &[&str] = &[
    "str-empty",
    "str-scalars",
    "str-many",
    "str-raw-number-return",
    "str-raw-nil-return",
    "str-conversion-order",
    "str-conversion-throw",
    "buffer-zero-nil-first",
    "buffer-second-nil-suppresses-rest",
    "buffer-append-empty",
    "buffer-append-self",
    "buffer-clear-set",
    "buffer-first-string-later-default-hints",
    "buffer-set-default-hint",
    "buffer-conversion-mutation-captured-left",
    "field-collision-constructor-read-write",
    "field-hyphen-callback",
    "object-variadic-receiver-rest",
    "field-borrowed-callback",
    "str-live-recursive-var",
    "str-immutable-nil-macro",
    "buffer-captured-to-array",
    "field-collision-protocol-read-write",
    "object-fixed-rest-overlap",
    "reserved-field-storage",
    "buffer-append-nil",
    "buffer-undefined-second-suppresses-rest",
    "buffer-set-missing",
    "buffer-utf16-length-and-ignored-effects",
];
// Host comparisons use decoded values and binary64 bits, never guest equality.
fn raw_helper_tag(form: &suss_reader::forms::Form) -> serde_json::Value {
    use serde_json::json;
    use suss_reader::forms::Kind;
    assert!(
        form.metadata.is_empty(),
        "unexpected raw observation metadata"
    );
    match &form.kind {
        Kind::String(value) => {
            json!({"tag":"string", "units":value.encode_utf16().collect::<Vec<_>>()})
        }
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
fn string_buffer_fields_and_str_match_raw_pin_in_both_phases_after_gc() {
    use suss_cli::portable_macro_data::FormBridge;
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/string-field-cases.json"
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
        IDS
    );
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session
            .eval("(def make-buffer bootstrap-string-buffer)")
            .unwrap();
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

#[test]
fn sole_host_callable_retains_buffer_and_captured_values_across_gc() {
    use suss_cli::portable_macro_data::FormBridge;
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let bridge = FormBridge::new(&mut session).unwrap();
        let callable = session.eval("(let [captured [17 23] b (bootstrap-string-buffer)] (fn [x] (.append b (get captured 1) x) (.toString b)))").unwrap();
        session.collect().unwrap();
        for expected in ["23!", "23!23!", "23!23!23!"] {
            let input = session.eval("\"!\"").unwrap();
            let result = session.invoke(&callable, &[&input]).unwrap();
            session.collect().unwrap();
            let decoded = bridge.read(&mut session, &result, 0..0).unwrap();
            assert_eq!(
                raw_helper_tag(&decoded),
                serde_json::json!({
                    "tag":"string", "units":expected.encode_utf16().collect::<Vec<_>>()
                })
            );
        }
    }
}
