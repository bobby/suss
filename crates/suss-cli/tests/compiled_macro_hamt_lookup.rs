use suss_cli::portable_session::Session;

#[test]
fn compiled_macro_transient_hash_map_omitted_lookup_preserves_undefined() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session.eval("(def lookup-map (transient (assoc (PersistentHashMap. nil 0 nil false nil nil) 1 101)))").unwrap();
        let present = session.eval("(get lookup-map 1)").unwrap();
        session
            .inspect(&present, |mut store, value| {
                assert_eq!(
                    value
                        .unwrap_anyref()
                        .unwrap()
                        .as_struct(&store)
                        .unwrap()
                        .unwrap()
                        .fields(&mut store)
                        .unwrap()
                        .next()
                        .unwrap()
                        .unwrap_f64(),
                    101_f64
                );
                Ok(())
            })
            .unwrap();
        let missing = session.eval("(get lookup-map 2)").unwrap();
        session
            .inspect(&missing, |mut store, value| {
                assert_eq!(
                    value
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&mut store)
                        .unwrap()
                        .unwrap()
                        .get_u32(),
                    6
                );
                Ok(())
            })
            .unwrap();
    }
}

#[test]
fn compiled_macro_transient_hash_map_rejects_lookup_after_persistence() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session.eval("(def lookup-map (transient (assoc (PersistentHashMap. nil 0 nil false nil nil) 1 101))) (def captured-lookup (fn [k] (get lookup-map k))) (def saved-map (persistent! lookup-map)) (def seen (array))").unwrap();
        session.collect().unwrap();
        for source in [
            "(get lookup-map 1)",
            "(get lookup-map 2)",
            "(get lookup-map nil)",
            "(get lookup-map 1 909)",
            "(get lookup-map 2 909)",
            "(get lookup-map nil 909)",
            "(lookup-map 1)",
            "(lookup-map 2 909)",
            "(captured-lookup 1)",
            "(get lookup-map (do (.push seen 1) 1) (do (.push seen 2) 909))",
        ] {
            assert!(
                matches!(
                    session.eval(source),
                    Err(suss_cli::portable_session::SessionError::Language(_))
                ),
                "{source} must reject a persisted transient"
            );
        }
        let bridge = suss_cli::portable_macro_data::FormBridge::new(&mut session).unwrap();
        let value = session
            .eval("[(get saved-map 1) (alength seen) (aget seen 0) (aget seen 1)]")
            .unwrap();
        let decoded = bridge.read(&mut session, &value, 0..1).unwrap();
        let suss_reader::forms::Kind::Vector(items) = decoded.kind else {
            panic!("Vector result")
        };
        for (item, expected) in items.iter().zip([101.0, 2.0, 1.0, 2.0]) {
            assert!(matches!(item.kind, suss_reader::forms::Kind::Number(n) if n == expected));
        }
        assert_eq!(items.len(), 4);
    }
}

#[test]
fn compiled_macro_transient_hash_map_lookup_matches_pinned_active_observations() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/hamt-lookup-cases.json")).unwrap();
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 12);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let bridge = suss_cli::portable_macro_data::FormBridge::new(&mut session).unwrap();
        for case in corpus["cases"].as_array().unwrap() {
            let value = session.eval(case["source"].as_str().unwrap()).unwrap();
            session.collect().unwrap();
            let decoded = bridge.read(&mut session, &value, 0..1).unwrap();
            let actual = match decoded.kind {
                suss_reader::forms::Kind::Bool(value) => {
                    serde_json::json!({"tag":"bool", "value":value})
                }
                suss_reader::forms::Kind::Number(n) => {
                    serde_json::json!({"tag":"f64", "bits":format!("{:016x}", n.to_bits())})
                }
                other => panic!("Unexpected observation: {other:?}"),
            };
            assert_eq!(actual, case["expected"], "{}", case["id"]);
        }
    }
}
