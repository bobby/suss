//! Authored scalar comparator prerequisites; full compare is deliberately absent.
use suss_cli::portable_session::{Session, SessionError, SessionOptions, SessionValue};
use suss_compile::portable::resolve::Phase;
use wasmtime::Val;

fn boolean(session: &mut Session, source: &str, expected: bool) {
    let value = session.eval(source).unwrap();
    session.collect().unwrap();
    let actual = session
        .inspect(&value, |store, value| {
            Ok(value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32())
        })
        .unwrap();
    assert_eq!(actual, if expected { 4 } else { 2 }, "{source}");
}
fn number(session: &mut Session, value: &SessionValue, expected: f64) {
    session.collect().unwrap();
    let actual = session
        .inspect(value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("number layout")
            };
            Ok(*bits)
        })
        .unwrap();
    assert_eq!(actual, expected.to_bits());
}

fn expect_language_error(
    session: &mut Session,
    error: SessionError,
    expected_descriptor: i64,
    expected_message: &str,
    source: &str,
) {
    let SessionError::Language(payload) = error else {
        panic!("expected language payload for {source}: {error}")
    };
    session.collect().unwrap();
    let (descriptor_id, message) = session
        .inspect(&payload, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let descriptor = object
                .field(&mut store, 0)?
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap();
            let descriptor_id = descriptor.field(&mut store, 0)?.unwrap_i64();
            let text = object
                .field(&mut store, 1)?
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            let message = text
                .elems(&mut store)?
                .map(|unit| unit.unwrap_i32() as u16)
                .collect::<Vec<_>>();
            Ok((descriptor_id, message))
        })
        .unwrap();
    assert_eq!(descriptor_id, expected_descriptor, "{source}");
    assert_eq!(
        message,
        expected_message.encode_utf16().collect::<Vec<_>>(),
        "{source}"
    );
}

#[test]
fn private_predicates_and_scalar_comparison_preserve_live_publics_in_both_phases() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
        session
            .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
            .unwrap();
        session.enter_namespace("user").unwrap();
        session
            .eval("(def saved-number? cljs.core/number?) (def saved-string? cljs.core/string?)")
            .unwrap();
        session.enter_namespace("suss.core").unwrap();
        session.eval("(def number? (fn [x] false)) (def string? (fn [x] false)) (def > (fn [a b] false)) (def < (fn [a b] false))").unwrap();
        session
            .eval(include_str!("../../../runtime/comparator-foundations.sus"))
            .unwrap();
        session.enter_namespace("user").unwrap();
        session.eval("(def retained-number? (fn [x] (suss.bootstrap/number? x))) (def retained-string? (fn [x] (suss.bootstrap/string? x))) (def retained-scalar suss.core/default-compare-scalar) (def comparator-trace 0)").unwrap();
        session.collect().unwrap();
        for (source, expected) in [
            ("(cljs.core/number? 7)", false),
            ("(cljs.core/string? \"x\")", false),
            ("(let [gt cljs.core/>] (gt 2 1))", false),
            ("(let [lt cljs.core/<] (lt 1 2))", false),
            ("(saved-number? 7)", true),
            ("(saved-string? \"x\")", true),
            ("(retained-number? 7)", true),
            ("(retained-number? \"7\")", false),
            ("(retained-string? \"7\")", true),
            ("(retained-string? 7)", false),
            ("(suss.bootstrap/number? false)", false),
            ("(suss.bootstrap/string? nil)", false),
            ("(suss.bootstrap/number? ##NaN)", true),
            ("(suss.bootstrap/string? (array 1))", false),
        ] {
            boolean(&mut session, source, expected);
        }
        session.enter_namespace("suss.core").unwrap();
        session
            .eval("(def number? (fn [x] true)) (def string? (fn [x] true))")
            .unwrap();
        session.enter_namespace("user").unwrap();
        boolean(&mut session, "(cljs.core/number? \"x\")", true);
        boolean(&mut session, "(retained-number? \"x\")", false);
        boolean(&mut session, "(retained-string? 7)", false);
        for (source, expected) in [
            ("(retained-scalar 2 10)", -1.0),
            ("(retained-scalar \"2\" \"10\")", 1.0),
            ("(retained-scalar ##NaN 1)", 0.0),
            ("(retained-scalar -0.0 0.0)", 0.0),
            ("(retained-scalar false true)", -1.0),
            ("(retained-scalar nil false)", 0.0),
            ("(retained-scalar \"2\" 10)", -1.0),
            ("(retained-scalar \"\\ud800\\udc00\" \"\\ue000\")", -1.0),
        ] {
            let value = session.eval(source).unwrap();
            number(&mut session, &value, expected);
        }
        boolean(
            &mut session,
            "(retained-number? (do (set! comparator-trace (+ (* comparator-trace 10) 1)) 7))",
            true,
        );
        let value = session.eval("(retained-scalar (do (set! comparator-trace (+ (* comparator-trace 10) 2)) 1) (do (set! comparator-trace (+ (* comparator-trace 10) 3)) 2))").unwrap();
        number(&mut session, &value, -1.0);
        let value = session.eval("comparator-trace").unwrap();
        number(&mut session, &value, 123.0);
        let Err(SessionError::Language(payload)) =
            session.eval("(retained-scalar (throw 73) (do (set! comparator-trace 999) 2))")
        else {
            panic!("operand throw")
        };
        number(&mut session, &payload, 73.0);
        let value = session.eval("comparator-trace").unwrap();
        number(&mut session, &value, 123.0);
        for (source, expected) in [
            (
                "(retained-scalar (throw false) (do (set! comparator-trace 999) 2))",
                2,
            ),
            (
                "(retained-scalar (throw nil) (do (set! comparator-trace 999) 2))",
                0,
            ),
        ] {
            let Err(SessionError::Language(payload)) = session.eval(source) else {
                panic!("language throw: {source}")
            };
            session.collect().unwrap();
            let actual = session
                .inspect(&payload, |store, value| {
                    Ok(value
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&store)?
                        .unwrap()
                        .get_u32())
                })
                .unwrap();
            assert_eq!(actual, expected, "rooted payload: {source}");
            let value = session.eval("comparator-trace").unwrap();
            number(&mut session, &value, 123.0);
        }
        let source = "(retained-scalar (array 2) (array 10))";
        let error = session.eval(source).unwrap_err();
        expect_language_error(
            &mut session,
            error,
            7,
            "Scalar comparator requires scalar operands",
            source,
        );
        boolean(&mut session, "(retained-number? 7)", true);
    }
}

#[test]
fn staged_type_returns_retained_nominal_constructors_in_both_phases() {
    let patch: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/compatibility/patches/sorted-type.json"
    ))
    .unwrap();
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
        session
            .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
            .unwrap();
        session.enter_namespace("suss.core").unwrap();
        session
            .eval(patch["replacement"].as_str().unwrap())
            .unwrap();
        session.enter_namespace("user").unwrap();
        session.eval("(deftype ComparatorTypeProbe [value m] cljs.core/IMeta (-meta [this] m) cljs.core/IWithMeta (-with-meta [this metadata] (ComparatorTypeProbe. value metadata)) cljs.core/IComparable (-compare [this other] value)) (deftype OtherComparatorTypeProbe [value m]) (def retained-class ComparatorTypeProbe) (def retained-instance (ComparatorTypeProbe. 17 nil)) (def retained-metadata-instance (with-meta retained-instance 29)) (def retained-type cljs.core/type) (def type-trace 0)").unwrap();
        session.collect().unwrap();
        let original_class = session.eval("retained-class").unwrap();
        let constructor_root = session
            .inspect(&original_class, |mut store, value| {
                value.unwrap_anyref().unwrap().to_owned_rooted(&mut store)
            })
            .unwrap();
        for source in [
            "(retained-type retained-instance)",
            "(retained-type retained-metadata-instance)",
        ] {
            let value = session.eval(source).unwrap();
            session.collect().unwrap();
            assert!(
                session
                    .inspect(&value, |store, value| wasmtime::Rooted::ref_eq(
                        &store,
                        &constructor_root,
                        value.unwrap_anyref().unwrap()
                    ))
                    .unwrap(),
                "{phase:?}: {source}"
            );
        }
        let instance = session.eval("retained-instance").unwrap();
        session.collect().unwrap();
        session
            .inspect(&instance, |mut store, value| {
                let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let descriptor = object
                    .field(&mut store, 0)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&store)?
                    .unwrap();
                let metadata = descriptor
                    .field(&mut store, 3)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .unwrap()
                    .get_u32();
                assert_eq!(metadata, 0, "cache leaves descriptor metadata intact");
                let protocols = descriptor
                    .field(&mut store, 2)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                let table = protocols.elems(&mut store)?.collect::<Vec<_>>();
                assert!(!table.is_empty());
                assert_eq!(table.len() % 2, 0, "protocol table retains key/value pairs");
                Ok(())
            })
            .unwrap();
        boolean(
            &mut session,
            "(identical? (cljs.core/type retained-instance) retained-class)",
            true,
        );
        boolean(
            &mut session,
            "(identical? (suss.core/type (OtherComparatorTypeProbe. 17 nil)) retained-class)",
            false,
        );
        boolean(&mut session, "(nil? (retained-type nil))", true);
        boolean(
            &mut session,
            "(nil? (retained-type (retained-class)))",
            true,
        );
        session
            .eval("(def ComparatorTypeProbe OtherComparatorTypeProbe)")
            .unwrap();
        session.enter_namespace("suss.core").unwrap();
        session
            .eval("(def type (fn [x] nil)) (def nil? (fn [x] false))")
            .unwrap();
        session.enter_namespace("user").unwrap();
        boolean(
            &mut session,
            "(identical? (cljs.core/type retained-instance) nil)",
            true,
        );
        boolean(&mut session, "(identical? (retained-type nil) nil)", true);
        boolean(
            &mut session,
            "(identical? (retained-type retained-instance) retained-class)",
            true,
        );
        boolean(
            &mut session,
            "(identical? (retained-type (do (set! type-trace (inc type-trace)) retained-instance)) retained-class)",
            true,
        );
        let value = session.eval("type-trace").unwrap();
        number(&mut session, &value, 1.0);
        for source in [
            "(retained-type (array 1))",
            "(retained-type (js-obj))",
            "(retained-type (fn [x] x))",
        ] {
            let error = session.eval(source).unwrap_err();
            expect_language_error(&mut session, error, 7, "Invalid nominal operation", source);
            boolean(
                &mut session,
                "(identical? (retained-type retained-instance) retained-class)",
                true,
            );
        }
        for source in [
            "(retained-type)",
            "(retained-type retained-instance retained-instance)",
        ] {
            let error = session.eval(source).unwrap_err();
            expect_language_error(&mut session, error, 1, "Wrong arity", source);
            boolean(
                &mut session,
                "(identical? (retained-type retained-instance) retained-class)",
                true,
            );
        }
        for source in [
            "(retained-type retained-instance)",
            "(retained-type retained-metadata-instance)",
        ] {
            let value = session.eval(source).unwrap();
            session.collect().unwrap();
            assert!(
                session
                    .inspect(&value, |store, value| wasmtime::Rooted::ref_eq(
                        &store,
                        &constructor_root,
                        value.unwrap_anyref().unwrap()
                    ))
                    .unwrap(),
                "retained constructor after redefinition: {source}"
            );
        }
        for (source, expected) in [
            ("(cljs.core/-meta retained-metadata-instance)", 29.0),
            ("(cljs.core/-compare retained-instance nil)", 17.0),
            ("(cljs.core/-compare retained-metadata-instance nil)", 17.0),
        ] {
            let value = session.eval(source).unwrap();
            number(&mut session, &value, expected);
        }
        boolean(
            &mut session,
            "(satisfies? cljs.core/IComparable retained-metadata-instance)",
            true,
        );
        for (source, expected) in [
            ("(retained-type (throw false))", 2),
            ("(retained-type (throw nil))", 0),
        ] {
            let Err(SessionError::Language(payload)) = session.eval(source) else {
                panic!("language throw: {source}")
            };
            session.collect().unwrap();
            let actual = session
                .inspect(&payload, |store, value| {
                    Ok(value
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&store)?
                        .unwrap()
                        .get_u32())
                })
                .unwrap();
            assert_eq!(actual, expected, "rooted payload: {source}");
            boolean(
                &mut session,
                "(identical? (retained-type retained-instance) retained-class)",
                true,
            );
        }
        boolean(
            &mut session,
            "(identical? (retained-type (ex-info \"x\" nil)) suss.core/ExceptionInfo)",
            true,
        );
    }
}

#[test]
fn primitive_constructor_values_are_canonical_callable_and_rooted_in_both_phases() {
    let patch: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/compatibility/patches/sorted-type.json"
    ))
    .unwrap();
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
        session
            .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
            .unwrap();
        session.enter_namespace("suss.core").unwrap();
        session
            .eval(patch["replacement"].as_str().unwrap())
            .unwrap();
        session.enter_namespace("user").unwrap();
        session.eval("(def retained-type cljs.core/type) (def scalar-number-constructor (retained-type 7)) (def scalar-string-constructor (retained-type \"x\")) (def scalar-boolean-constructor (retained-type false)) (deftype ScalarTypeUndefinedProbe []) (def primitive-constructor-trace 0)").unwrap();
        for (original, aliases) in [
            (
                "scalar-number-constructor",
                ["(retained-type 8)", "(retained-type ##NaN)"],
            ),
            (
                "scalar-string-constructor",
                ["(retained-type \"\")", "(retained-type \"y\")"],
            ),
            (
                "scalar-boolean-constructor",
                ["(retained-type true)", "(retained-type false)"],
            ),
        ] {
            let original = session.eval(original).unwrap();
            let root = session
                .inspect(&original, |mut store, value| {
                    value.unwrap_anyref().unwrap().to_owned_rooted(&mut store)
                })
                .unwrap();
            for source in aliases {
                let returned = session.eval(source).unwrap();
                session.collect().unwrap();
                assert!(
                    session
                        .inspect(&returned, |store, value| wasmtime::Rooted::ref_eq(
                            &store,
                            &root,
                            value.unwrap_anyref().unwrap()
                        ))
                        .unwrap(),
                    "canonical primitive constructor: {source}"
                );
            }
        }
        boolean(
            &mut session,
            "(identical? scalar-number-constructor scalar-string-constructor)",
            false,
        );
        boolean(
            &mut session,
            "(identical? scalar-boolean-constructor scalar-number-constructor)",
            false,
        );
        boolean(
            &mut session,
            "(identical? scalar-boolean-constructor scalar-string-constructor)",
            false,
        );
        session.enter_namespace("suss.core").unwrap();
        session
            .eval(
                "(def type (fn [x] nil)) (def number? (fn [x] false)) (def string? (fn [x] false))",
            )
            .unwrap();
        session.enter_namespace("user").unwrap();
        boolean(&mut session, "(identical? (cljs.core/type 1) nil)", true);
        boolean(
            &mut session,
            "(identical? (retained-type 1) scalar-number-constructor)",
            true,
        );
        boolean(
            &mut session,
            "(identical? (retained-type \"x\") scalar-string-constructor)",
            true,
        );
        for (source, expected) in [
            ("(scalar-number-constructor)", 0.0),
            ("(scalar-number-constructor nil)", 0.0),
            ("(scalar-number-constructor true)", 1.0),
            ("(scalar-number-constructor \"16\")", 16.0),
            ("(scalar-number-constructor -0.0)", -0.0),
            (
                "(scalar-number-constructor (do (set! primitive-constructor-trace 1) 7) (do (set! primitive-constructor-trace (+ (* primitive-constructor-trace 10) 2)) 9))",
                7.0,
            ),
            ("primitive-constructor-trace", 12.0),
        ] {
            let value = session.eval(source).unwrap();
            number(&mut session, &value, expected);
        }
        for (source, expected) in [
            ("(scalar-string-constructor)", ""),
            ("(scalar-string-constructor nil)", "null"),
            (
                "(scalar-string-constructor (ScalarTypeUndefinedProbe))",
                "undefined",
            ),
            ("(scalar-string-constructor false)", "false"),
            ("(scalar-string-constructor -0.0)", "0"),
            (
                "(scalar-string-constructor \"\\ud800\\udc00\")",
                "\u{10000}",
            ),
        ] {
            let value = session.eval(source).unwrap();
            session.collect().unwrap();
            let units = session
                .inspect(&value, |mut store, value| {
                    let text = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                    Ok(text
                        .elems(&mut store)?
                        .map(|unit| unit.unwrap_i32() as u16)
                        .collect::<Vec<_>>())
                })
                .unwrap();
            assert_eq!(
                units,
                expected.encode_utf16().collect::<Vec<_>>(),
                "{source}"
            );
        }
        for (source, expected) in [
            ("(scalar-boolean-constructor)", false),
            ("(scalar-boolean-constructor nil)", false),
            ("(scalar-boolean-constructor false)", false),
            (
                "(scalar-boolean-constructor (ScalarTypeUndefinedProbe))",
                false,
            ),
            ("(scalar-boolean-constructor 0)", false),
            ("(scalar-boolean-constructor -0.0)", false),
            ("(scalar-boolean-constructor ##NaN)", false),
            ("(scalar-boolean-constructor \"\")", false),
            ("(scalar-boolean-constructor \"0\")", true),
            ("(scalar-boolean-constructor ##Inf)", true),
            ("(scalar-boolean-constructor (array))", true),
            (
                "(scalar-boolean-constructor (ScalarTypeUndefinedProbe.))",
                true,
            ),
        ] {
            boolean(&mut session, source, expected);
        }
        for source in [
            "(scalar-number-constructor (array 1))",
            "(scalar-string-constructor (array 1))",
        ] {
            let error = session.eval(source).unwrap_err();
            expect_language_error(
                &mut session,
                error,
                5,
                "Unsupported arithmetic object coercion",
                source,
            );
            boolean(
                &mut session,
                "(identical? (retained-type 1) scalar-number-constructor)",
                true,
            );
        }
    }
}
