//! Canonical Array constructor prerequisites; missing support is a failure.
use suss_cli::portable_session::Session;

#[test]
fn array_constructor_preserves_identity_arity_length_and_holes_in_both_phases() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/array-constructor-cases.json"
    ))
    .unwrap();
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 17);
    let ids: Vec<_> = corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "canonical-array-type",
            "zero-arity",
            "single-numeric-length-holes",
            "single-false-element",
            "single-nil-element",
            "multiple-elements",
            "single-string-element",
            "negative-zero-length",
            "invalid-length-negative",
            "invalid-length-fractional",
            "invalid-length-nan",
            "invalid-length-infinite",
            "invalid-length-overflow",
            "multiple-argument-effect-order",
            "single-array-element-identity",
            "single-object-element-identity",
            "single-undefined-element"
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
fn retained_array_constructor_is_callable_after_gc_in_separate_fragment() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let constructor = session.eval("(type (array))").unwrap();
        session.collect().unwrap();
        let invoke = session.eval("(fn [ctor] (let [a (ctor 17 false)] (and (identical? ctor (type a)) (= (alength a) 2) (= (aget a 0) 17) (false? (aget a 1)))))").unwrap();
        let value = session.invoke(&invoke, &[&constructor]).unwrap();
        session.collect().unwrap();
        session
            .inspect(&value, |store, value| {
                let raw = value
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .expect("Boolean ABI")
                    .get_u32();
                assert_eq!(raw, 4, "retained constructor identity and call result");
                Ok(())
            })
            .unwrap();
    }
}
