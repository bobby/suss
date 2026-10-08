//! Original FIFO/metadata/equality regressions shared with the pinned CLJS oracle.
//! Queue support must execute; missing constructors are failures, never skips.
use suss_cli::portable_session::Session;

#[test]
fn persistent_queues_match_pinned_fifo_and_value_contract_in_both_phases() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/queue-cases.json")).unwrap();
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 41);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session
            .eval(include_str!(
                "../../../tests/oracle/fixtures/queue-probe.sus"
            ))
            .unwrap();
        for case in corpus["cases"].as_array().unwrap() {
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
fn queue_versions_share_front_and_rear_storage_and_sole_handle_survives_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session
            .eval(include_str!(
                "../../../tests/oracle/fixtures/queue-probe.sus"
            ))
            .unwrap();
        let original = session.eval("q3").unwrap();
        let storage = session
            .eval("(array q3 qm (clone qm) (conj q3 4))")
            .unwrap();
        session.collect().unwrap();
        session
            .inspect(&storage, |mut store, value| {
                let source_array = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                assert_eq!(source_array.fields(&mut store)?.count(), 4);
                let array_fields = source_array
                    .field(&mut store, 1)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                assert_eq!(array_fields.len(&store)?, 1);
                let versions = array_fields
                    .get(&mut store, 0)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                assert_eq!(versions.len(&store)?, 4);
                let mut fronts = Vec::new();
                let mut rears = Vec::new();
                let mut descriptors = Vec::new();
                for (index, count) in [3.0_f64, 3.0, 3.0, 4.0].into_iter().enumerate() {
                    let queue = versions
                        .get(&mut store, index as u32)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_struct(&store)?
                        .unwrap();
                    assert_eq!(
                        queue.fields(&mut store)?.count(),
                        4,
                        "nominal object layout"
                    );
                    descriptors.push(
                        queue
                            .field(&mut store, 0)?
                            .unwrap_anyref()
                            .unwrap()
                            .as_eqref(&mut store)?
                            .unwrap(),
                    );
                    let fields = queue
                        .field(&mut store, 1)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    assert_eq!(fields.len(&store)?, 5, "full upstream queue field layout");
                    let number = fields
                        .get(&mut store, 1)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_struct(&store)?
                        .unwrap();
                    assert_eq!(number.field(&mut store, 0)?.unwrap_f64(), count);
                    fronts.push(
                        fields
                            .get(&mut store, 2)?
                            .unwrap_anyref()
                            .unwrap()
                            .as_eqref(&mut store)?
                            .unwrap(),
                    );
                    rears.push(
                        fields
                            .get(&mut store, 3)?
                            .unwrap_anyref()
                            .unwrap()
                            .as_eqref(&mut store)?
                            .unwrap(),
                    );
                }
                for index in 1..4 {
                    assert!(wasmtime::Rooted::ref_eq(
                        &store,
                        &descriptors[0],
                        &descriptors[index]
                    )?);
                    assert!(wasmtime::Rooted::ref_eq(
                        &store,
                        &fronts[0],
                        &fronts[index]
                    )?);
                }
                for index in 1..3 {
                    assert!(wasmtime::Rooted::ref_eq(&store, &rears[0], &rears[index])?);
                }
                assert!(!wasmtime::Rooted::ref_eq(&store, &rears[0], &rears[3])?);
                Ok(())
            })
            .unwrap();
        drop(storage);
        session.eval("(set! q0 nil) (set! q1 nil) (set! q3 nil) (set! qm nil) (set! qp nil) (set! qr nil) (set! qnf nil) (set! q65 nil) (set! q33 nil)").unwrap();
        session.collect().unwrap();
        let project = session.eval("(fn [q] (into [] q))").unwrap();
        let result = session.invoke(&project, &[&original]).unwrap();
        let bridge = suss_cli::portable_macro_data::FormBridge::new(&mut session).unwrap();
        let decoded = bridge.read(&mut session, &result, 0..0).unwrap();
        let suss_reader::forms::Kind::Vector(items) = decoded.kind else {
            panic!("queue projected vector")
        };
        assert_eq!(items.len(), 3);
        for (item, expected) in items.iter().zip([1.0_f64, 2.0, 3.0]) {
            let suss_reader::forms::Kind::Number(actual) = item.kind else {
                panic!("queue numeric item")
            };
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
    }
}

#[test]
fn queue_iterator_preserves_mutation_exhaustion_and_returned_error_contract() {
    use suss_cli::portable_session::{SessionError, SessionValue};
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session
            .eval(include_str!(
                "../../../tests/oracle/fixtures/queue-probe.sus"
            ))
            .unwrap();
        session.eval("(def qi (-iterator q3))").unwrap();
        let first_identity = session
            .eval("(identical? (.hasNext qi) (.-front q3))")
            .unwrap();
        session.collect().unwrap();
        assert_eq!(
            session
                .inspect(&first_identity, |store, value| Ok(value
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .unwrap()
                    .get_u32()))
                .unwrap(),
            4
        );
        for expected in [1.0_f64, 2.0, 3.0] {
            let available = session.eval("(boolean (.hasNext qi))").unwrap();
            session.collect().unwrap();
            assert_eq!(
                session
                    .inspect(&available, |store, value| Ok(value
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&store)?
                        .unwrap()
                        .get_u32()))
                    .unwrap(),
                4
            );
            let item = session.eval("(.next qi)").unwrap();
            session.collect().unwrap();
            let bits = session
                .inspect(&item, |mut store, value| {
                    let boxed = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    let fields = boxed.fields(&mut store)?.collect::<Vec<_>>();
                    let [wasmtime::Val::F64(bits)] = fields.as_slice() else {
                        panic!("iterator Number layout")
                    };
                    Ok(*bits)
                })
                .unwrap();
            assert_eq!(bits, expected.to_bits());
        }
        let exhausted = session.eval("(.hasNext qi)").unwrap();
        session.collect().unwrap();
        assert_eq!(
            session
                .inspect(&exhausted, |store, value| Ok(value
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .unwrap()
                    .get_u32()))
                .unwrap(),
            2
        );
        fn error(session: &mut Session, payload: &SessionValue, expected: &str) {
            session.collect().unwrap();
            let (descriptor, message) = session
                .inspect(payload, |mut store, value| {
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
            assert_eq!(message, expected.encode_utf16().collect::<Vec<_>>());
        }
        let SessionError::Language(payload) = session.eval("(.next qi)").unwrap_err() else {
            panic!("iterator exhaustion must throw")
        };
        error(&mut session, &payload, "No such element");
        // Upstream remove returns an Error object rather than throwing it.
        let returned = session.eval("(.remove qi)").unwrap();
        error(&mut session, &returned, "Unsupported operation");
        let old = session.eval("(= (seq q3) [1 2 3])").unwrap();
        assert_eq!(
            session
                .inspect(&old, |store, value| Ok(value
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .unwrap()
                    .get_u32()))
                .unwrap(),
            4
        );
    }
}
