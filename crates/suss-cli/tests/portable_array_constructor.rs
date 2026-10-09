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
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 22);
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
            "single-undefined-element",
            "holes-have-no-own-index",
            "explicit-undefined-has-own-index",
            "valid-sparse-length-above-resource-cap",
            "maximum-valid-sparse-length",
            "uint32-max-is-ordinary-property",
        ]
    );
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for case in corpus["cases"].as_array().unwrap() {
            let source = case["source"].as_str().unwrap();
            let value = session
                .eval(source)
                .unwrap_or_else(|error| panic!("Array case {} ({source}): {error}", case["id"]));
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

#[test]
fn array_own_method_identity_survives_public_property_reads_and_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let method = session.eval("(.-hasOwnProperty (array))").unwrap();
        session.collect().unwrap();
        let check = session.eval("(fn [saved] (let [a (array 7)] (and (identical? saved (.-hasOwnProperty a)) (.hasOwnProperty a 0) (.hasOwnProperty a \"length\") (not (.hasOwnProperty a \"00\")) (not (.hasOwnProperty a \"-0\")) (not (.hasOwnProperty a \"1e0\")) (not (.hasOwnProperty a)))))").unwrap();
        let value = session.invoke(&check, &[&method]).unwrap();
        session.collect().unwrap();
        session.inspect(&value, |store, value| {
            assert_eq!(value.unwrap_anyref().unwrap().as_i31(&store)?.unwrap().get_u32(), 4);
            Ok(())
        }).unwrap();
    }
}

#[test]
fn array_push_overflow_and_ordinary_properties_match_pinned_cases_in_both_phases() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!("../../../tests/oracle/array-push-properties-cases.json")).unwrap();
    assert_eq!(corpus["upstream"], "c4295f303100bbf5afac449242d30bca1126f1a1");
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    let ids: Vec<_> = cases.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["push-max-three-writes", "push-max-minus-one-three-writes", "push-to-max-succeeds", "push-max-empty-succeeds", "ordinary-numeric-properties", "ordinary-string-properties-and-index-spelling", "indexed-length-shrink-preserves-ordinary", "invalid-length-assignment-no-write"]);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for case in cases {
            let source = case["source"].as_str().unwrap();
            let value = session.eval(source).unwrap_or_else(|error| panic!("Array property case {}: {error}", case["id"]));
            session.collect().unwrap();
            let actual = session.inspect(&value, |store, value| {
                match value.unwrap_anyref().unwrap().as_i31(&store)?.expect("Boolean ABI").get_u32() {
                    2 => Ok(false), 4 => Ok(true), other => panic!("Unexpected sentinel {other}"),
                }
            }).unwrap();
            assert_eq!(serde_json::json!({"tag":"bool", "value":actual}), case["expected"], "{}", case["id"]);
        }
    }
}

#[test]
fn array_named_length_primitive_rhs_and_builtin_shadowing() {
    let corpus = array_public_property_cases();
    let cases: Vec<_> = corpus["cases"].as_array().unwrap()[..6].iter()
        .map(|case| case["source"].as_str().unwrap()).collect();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for source in &cases {
            let value = session.eval(source).unwrap_or_else(|error| panic!("{source}: {error}"));
            session.collect().unwrap();
            session.inspect(&value, |store, value| {
                assert_eq!(value.unwrap_anyref().unwrap().as_i31(&store)?.expect("Boolean ABI").get_u32(), 4, "{source}");
                Ok(())
            }).unwrap();
        }
    }
}

#[test]
fn make_array_preserves_holes_and_literal_nil_presence() {
    let corpus = array_public_property_cases();
    let cases: Vec<_> = corpus["cases"].as_array().unwrap()[6..].iter()
        .map(|case| case["source"].as_str().unwrap()).collect();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for source in &cases {
            let value = session.eval(source).unwrap_or_else(|error| panic!("{source}: {error}"));
            session.collect().unwrap();
            session.inspect(&value, |store, value| {
                assert_eq!(value.unwrap_anyref().unwrap().as_i31(&store)?.expect("Boolean ABI").get_u32(), 4, "{source}");
                Ok(())
            }).unwrap();
        }
    }
}

fn array_public_property_cases() -> serde_json::Value {
    let corpus: serde_json::Value = serde_json::from_str(include_str!("../../../tests/oracle/array-public-properties-cases.json")).unwrap();
    assert_eq!(corpus["upstream"], "c4295f303100bbf5afac449242d30bca1126f1a1");
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 11);
    let ids: Vec<_> = cases.iter().map(|case| case["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["named-length", "string-length-rhs", "boolean-true-length", "boolean-false-length", "nil-length", "shadow-push", "dynamic-make-holes", "first-class-make-holes", "literal-make-nil-own", "multi-make-leaf-holes", "dynamic-make-max-holes"]);
    for case in cases { assert_eq!(case["expected"], serde_json::json!({"tag":"bool","value":true})); }
    corpus
}

#[test]
fn make_array_dimension_validation_and_unreachable_leaves() {
    let cases = [
        "(let [n -1] (try (make-array nil n 2) false (catch :default e (= (.-message e) \"Invalid array length\"))))",
        "(let [n 1.5] (try (make-array nil n 2) false (catch :default e (= (.-message e) \"Invalid array length\"))))",
        "(try (make-array nil 2 -1) false (catch :default e (= (.-message e) \"Invalid array length\")))",
        "(= (alength (make-array nil 0 -1)) 0)",
        "(= (alength (make-array nil 0 -1 2)) 0)",
        "(let [a (make-array nil \"2\" 3)] (and (= (alength a) 1) (= (alength (aget a 0)) 3) (not (.hasOwnProperty (aget a 0) \"0\"))))",
        "(let [a (make-array nil 2 \"3\")] (and (= (alength a) 2) (= (alength (aget a 0)) 1) (= (aget a 0 0) \"3\")))",
        "(= (alength (make-array nil -1 2)) 0)",
        "(let [a (make-array nil 1.5 2)] (and (= (alength a) 2) (= (alength (aget a 0)) 2)))",
    ];
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for source in cases {
            let value = session.eval(source).unwrap_or_else(|error| panic!("{source}: {error}"));
            session.collect().unwrap();
            session.inspect(&value, |store, value| {
                assert_eq!(value.unwrap_anyref().unwrap().as_i31(&store)?.expect("Boolean ABI").get_u32(), 4, "{source}");
                Ok(())
            }).unwrap();
        }
    }
}
