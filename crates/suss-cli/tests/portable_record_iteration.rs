//! Iterator prerequisites only; these tests do not establish nominal records.
use suss_cli::portable_session::Session;

#[test]
fn record_iterator_matches_pinned_order_and_effects_in_both_phases() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/record-iteration-cases.json"
    ))
    .unwrap();
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 14);
    let ids: Vec<_> = corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "not-empty-nil",
            "not-empty-empty-vector",
            "not-empty-identity",
            "declared-field-order",
            "short-circuit-extension",
            "extension-availability-delegation",
            "extension-next-delegation",
            "lookup-throw-advances-index",
            "missing-field-lookup-nil",
            "returned-remove-error",
            "not-empty-argument-once",
            "not-empty-live-seq-once",
            "record-iterator-live-nth",
            "record-iterator-live-lookup"
        ]
    );
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session
            .eval(include_str!(
                "../../../tests/oracle/fixtures/record-iteration-probe.sus"
            ))
            .unwrap();
        for case in corpus["cases"].as_array().unwrap() {
            // Adapt the reference host Error predicate to the canonical portable
            // descriptor predicate. Every iterator operation and assertion remains.
            let source = case["source"].as_str().unwrap().replace(
                "(instance? js/Error result)",
                "(suss.bootstrap/error? result)",
            );
            let value = session.eval(&source).unwrap();
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
                        _ => panic!("Unexpected Boolean sentinel {raw}"),
                    })
                })
                .unwrap();
            assert_eq!(
                serde_json::json!({"tag":"bool","value":actual}),
                case["expected"],
                "{}",
                case["id"]
            );
        }
    }
}

#[test]
fn record_iterator_survives_with_only_host_handle_and_mutates_across_fragments() {
    use suss_cli::portable_macro_data::FormBridge;
    use suss_reader::forms::Kind;
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session
            .eval(include_str!(
                "../../../tests/oracle/fixtures/record-iteration-probe.sus"
            ))
            .unwrap();
        // No guest global owns this iterator or its record/fields/extension object.
        let iterator = session
            .eval("(RecordIter. 0 {:a 17 :b 23} 2 [:a :b] (EmptyRecordExtension.))")
            .unwrap();
        session.collect().unwrap();
        let advance = session.eval("(fn [iter] (val (.next iter)))").unwrap();
        for expected in [17.0_f64, 23.0] {
            let result = session.invoke(&advance, &[&iterator]).unwrap();
            session.collect().unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let decoded = bridge.read(&mut session, &result, 0..0).unwrap();
            let Kind::Number(actual) = decoded.kind else {
                panic!("numeric record field")
            };
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
        let exhausted = session.eval("(fn [iter] (.hasNext iter))").unwrap();
        let result = session.invoke(&exhausted, &[&iterator]).unwrap();
        session.collect().unwrap();
        session
            .inspect(&result, |store, value| {
                let raw = value
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .expect("Boolean ABI")
                    .get_u32();
                assert_eq!(
                    raw, 2,
                    "iterator exhausted after both independent invocations"
                );
                Ok(())
            })
            .unwrap();
    }
}

#[test]
fn record_iterator_remove_returns_independently_decoded_error() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session
            .eval(include_str!(
                "../../../tests/oracle/fixtures/record-iteration-probe.sus"
            ))
            .unwrap();
        // Successful evaluation is essential: remove returns this object.
        let payload = session
            .eval("(.remove (RecordIter. 0 {} 0 [] (EmptyRecordExtension.)))")
            .unwrap();
        session.collect().unwrap();
        let (descriptor, message) = session
            .inspect(&payload, |mut store, value| {
                let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let class = object
                    .field(&mut store, 0)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&store)?
                    .unwrap();
                let descriptor = class.field(&mut store, 0)?.unwrap_i64();
                let text = object
                    .field(&mut store, 1)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                let units = text
                    .elems(&mut store)?
                    .map(|v| v.unwrap_i32() as u16)
                    .collect::<Vec<_>>();
                Ok((descriptor, units))
            })
            .unwrap();
        assert_eq!(descriptor, 7);
        assert_eq!(
            message,
            "Unsupported operation".encode_utf16().collect::<Vec<_>>()
        );
    }
}
