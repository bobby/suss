//! Complete reify prerequisite regression; missing macro support is a failure.
use suss_cli::portable_session::Session;

#[test]
fn reify_preserves_capture_metadata_and_nominal_identity_in_both_phases() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/reify-prerequisite-cases.json"
    ))
    .unwrap();
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 7);
    let ids: Vec<_> = corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "captured-local",
            "with-meta-preserves-capture",
            "reader-meta-transfer",
            "same-site-nominal-class",
            "different-sites-distinct-types",
            "protocol-dispatch",
            "method-head-recur"
        ]
    );
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for case in corpus["cases"].as_array().unwrap() {
            let source = case["source"].as_str().unwrap();
            let value = session.eval(source).unwrap();
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
fn reified_object_retains_capture_with_only_host_handle_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let object = session
            .eval("(let [captured [17 23]] (reify Object (value [_] (nth captured 1))))")
            .unwrap();
        session.collect().unwrap();
        let projection = session.eval("(fn [object] (.value object))").unwrap();
        let result = session.invoke(&projection, &[&object]).unwrap();
        session.collect().unwrap();
        let bridge = suss_cli::portable_macro_data::FormBridge::new(&mut session).unwrap();
        let decoded = bridge.read(&mut session, &result, 0..0).unwrap();
        let suss_reader::forms::Kind::Number(actual) = decoded.kind else {
            panic!("captured vector field")
        };
        assert_eq!(actual.to_bits(), 23.0_f64.to_bits());
    }
}
