use suss_cli::portable_session::Session;

#[path = "support/clojure_corrections.rs"]
mod clojure_corrections;

/// Clojure governs bit operations and casts (#336): bit operations take longs,
/// and `int` checks the range. These recorded observations throw in JVM
/// Clojure 1.12, with these messages.
const CORRECTIONS: &[clojure_corrections::Correction] = &[
    ("int-wrap", "", "integer overflow"),
    ("int-large", "", "integer overflow"),
    ("int-huge", "", "Value out of range for int: 1.0E100"),
    ("int-nil", "", "int requires a number operand"),
    ("int-true", "", "int requires a number operand"),
    ("int-string", "", "int requires a number operand"),
    ("int-utf16-invalid", "", "int requires a number operand"),
    ("shift-fraction", "", "bit operation not supported for: double"),
    ("first-class-int", "", "integer overflow"),
    ("undefined-int", "", "int requires a number operand"),
    ("capture-old-value", "", "integer overflow"),
    ("review-int-negative-max", "", "Value out of range for int: -1.7976931348623157E308"),
    ("review-int-positive-max", "", "Value out of range for int: 1.7976931348623157E308"),
    ("review-int-large-low-bits", "", "integer overflow"),
    ("review-int-negative-large-low-bits", "", "integer overflow"),
    ("review-int-prefix-string", "", "int requires a number operand"),
    ("review-shift-negative-fraction", "", "bit operation not supported for: double"),
];

/// An exact runtime number: a long, or a double by its bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Number {
    Long(i64),
    Double(u64),
}
/// The expected number for `value`: integral values are longs, as in Clojure.
fn num(value: f64) -> Number {
    if value.is_finite()
        && value.fract() == 0.0
        && !(value == 0.0 && value.is_sign_negative())
        && value.abs() < 9_223_372_036_854_775_808.0
    {
        Number::Long(value as i64)
    } else {
        Number::Double(value.to_bits())
    }
}
/// An expected number compares by `num`: integral values are longs.
impl PartialEq<f64> for Number {
    fn eq(&self, expected: &f64) -> bool {
        *self == num(*expected)
    }
}
fn exact_number(store: impl wasmtime::AsContextMut, value: &wasmtime::Val) -> wasmtime::Result<Number> {
    Ok(
        match suss_compile::portable_session::runtime_number(store, value)?
            .expect("expected a runtime number")
        {
            suss_compile::portable_session::RuntimeNumber::Long(value) => Number::Long(value),
            suss_compile::portable_session::RuntimeNumber::Double(value) => {
                Number::Double(value.to_bits())
            }
        },
    )
}

/// Whether `actual` matches a pinned `f64` observation. The ClojureScript
/// corpus cannot distinguish longs: an integral observation is a Clojure long,
/// or an integral double when the source has a double operand.
fn matches_pinned(actual: Number, expected: &serde_json::Value, source: &str) -> bool {
    let bits = u64::from_str_radix(expected["bits"].as_str().unwrap(), 16).unwrap();
    match actual {
        Number::Long(_) => actual == num(f64::from_bits(bits)),
        Number::Double(actual) => {
            actual == bits
                && (num(f64::from_bits(bits)) == Number::Double(bits)
                    || source.contains("##")
                    || source.as_bytes().windows(3).any(|w| {
                        w[0].is_ascii_digit()
                            && (w[1] == b'.' || w[1] == b'e')
                            && (w[2].is_ascii_digit() || w[2] == b'-')
                    }))
        }
    }
}
/// The pinned observation when `actual` matches it, otherwise a distinct value.
fn number_observation(actual: Number, expected: &serde_json::Value, source: &str) -> serde_json::Value {
    if matches_pinned(actual, expected, source) {
        expected.clone()
    } else {
        serde_json::json!({ "tag": "number", "actual": format!("{actual:?}") })
    }
}

#[test]
fn bitwise_hash_match_independently_decoded_primary_observations_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/bitwise-hash-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 70);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session
        .enter_namespace("suss-oracle.bitwise-hash-cases")
        .unwrap();
    let mut ids = std::collections::BTreeSet::new();
    let mut failures = Vec::new();
    let mut corrected = 0;
    for case in cases {
        let case = &clojure_corrections::corrected(case, CORRECTIONS, &mut corrected);
        let id = case["id"].as_str().unwrap();
        assert!(ids.insert(id.to_owned()));
        let source = case["source"].as_str().unwrap();
        // Clojure governs: dividing longs by zero throws rather than producing
        // the pinned ClojureScript NaN or infinity operand.
        let divides_by_zero = [
            "int-nan",
            "int-infinity",
            "shift-nan",
            "review-shift-infinity",
            "review-count-nan",
        ];
        if divides_by_zero.contains(&id) {
            if !matches!(
                session.eval(source),
                Err(suss_cli::portable_session::SessionError::Language(_))
            ) {
                failures.push(format!("{id}: {source}: expected a division by zero"));
            }
            session.collect().unwrap();
            continue;
        }
        // Bit operations operate on 64 bits (#336); these are JVM Clojure
        // 1.12's results where the pinned ClojureScript observation differs.
        // ClojureScript's `imul` is not public.
        let clojure_long = [
            ("and", 4294967297),
            ("or", 2147483651),
            ("not", -4294967297),
            ("clear-mask", -8589934593),
            ("flip-mask", i64::MIN),
            ("set-mask", 4294967296),
            ("left-mask", 8589934592),
            ("right-sign", 1073741824),
            ("unsigned", -1),
            ("unsigned-alias", -1),
            ("first-class-set", 2147483648),
        ];
        if clojure_corrections::calls_imul(source) {
            match session.eval(source) {
                Err(suss_cli::portable_session::SessionError::Compile(error))
                    if error.message.contains("Unresolved Runtime name")
                        && error.message.contains("imul") => {}
                _ => failures.push(format!("{id}: {source}: expected imul to be unresolved")),
            }
            continue;
        }
        let value = match session.eval(source) {
            Ok(value) => value,
            Err(error) => {
                failures.push(format!("{id}: {source}: {error:?}"));
                continue;
            }
        };
        if let Some((_, expected)) = clojure_long.iter().find(|(name, _)| *name == id) {
            let actual = session
                .inspect(&value, |mut store, value| exact_number(&mut store, &value))
                .unwrap();
            if actual != Number::Long(*expected) {
                failures.push(format!("{id}: {source}: actual {actual:?}, expected {expected}"));
            }
            session.collect().unwrap();
            continue;
        }
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "f64" => {
                number_observation(
                    session
                        .inspect(&value, |mut store, value| exact_number(&mut store, &value))
                        .unwrap(),
                    &case["expected"],
                    &source,
                )
            }
            "nil" => {
                session
                    .inspect(&value, |mut store, value| {
                        assert_eq!(
                            suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap(),
                            0
                        );
                        Ok(())
                    })
                    .unwrap();
                serde_json::json!({"tag":"nil"})
            }
            "bool" => {
                let boolean = session
                    .inspect(&value, |mut store, value| {
                        let sentinel = suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap();
                        match sentinel {
                            2 => Ok(false),
                            4 => Ok(true),
                            n => panic!("Boolean {n}"),
                        }
                    })
                    .unwrap();
                serde_json::json!({"tag":"bool", "value":boolean})
            }
            "string" => {
                let units = session
                    .inspect(&value, |mut store, value| {
                        let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                        Ok(array
                            .elems(&mut store)?
                            .map(|v| v.unwrap_i32() as u16)
                            .collect::<Vec<_>>())
                    })
                    .unwrap();
                serde_json::json!({"tag":"string", "units":units})
            }
            tag => panic!("unsupported Bitwise hash observation {tag}"),
        };
        if actual != case["expected"] {
            failures.push(format!(
                "{id}: {source}: actual {actual}, expected {}",
                case["expected"]
            ));
        }
        session.collect().unwrap();
    }
    assert_eq!(corrected, CORRECTIONS.len());
    assert!(
        failures.is_empty(),
        "{} failing observations:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn bitwise_number(session: &mut Session, source: &str) -> Number {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |mut store, value| exact_number(&mut store, &value))
        .unwrap()
}

#[test]
fn bitwise_bad_arities_fail_with_spans_and_no_partial_publication() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    for body in [
        "(int)",
        "(int 1 2)",
        "(bit-and)",
        "(bit-and 7)",
        "(bit-not 1 2)",
        "(unsigned-bit-shift-right 1)",
    ] {
        let source = format!("(do (def bitwise_unpublished 7) {body})");
        let SessionError::Compile(error) = session.eval(&source).unwrap_err() else {
            panic!("expected compile error: {source}");
        };
        assert!(error.span.start < error.span.end && error.span.end <= source.len());
        assert!(session.eval("bitwise_unpublished").is_err());
        assert_eq!(bitwise_number(&mut session, "(int -1)"), -1.0);
    }
    // The accepted arity contract diagnoses these separately from pinned JS
    // captured wrappers' off-arity fill/ignore behavior; no differential claim.
    for source in [
        "(let [f int] (f))",
        "(let [f int] (f 1 2))",
        "(let [f bit-and] (f 1))",
        "(imul 1)",
    ] {
        assert!(session.eval(source).is_err(), "{source}");
        assert_eq!(bitwise_number(&mut session, "(bit-xor 7 3)"), 4.0);
    }
}

#[test]
fn bitwise_macro_folds_preserve_throw_order_and_computed_calls_evaluate_all_args() {
    let mut session = Session::new().unwrap();
    session.eval("(def bitwise_trace 0)").unwrap();
    // Object coercion is explicitly unsupported. Nested macro pair conversion
    // fails before the third operand; computed function arguments precede it.
    for (source, expected) in [
        ("(bit-or (fn [] 0) 1 (do (set! bitwise_trace 7) 2))", 0.0),
        (
            "(let [f bit-or] (f (fn [] 0) 1 (do (set! bitwise_trace 7) 2)))",
            7.0,
        ),
    ] {
        session.eval("(set! bitwise_trace 0)").unwrap();
        assert!(session.eval(source).is_err());
        assert_eq!(bitwise_number(&mut session, "bitwise_trace"), expected);
        session.collect().unwrap();
        assert_eq!(bitwise_number(&mut session, "(bit-or 1 2 4)"), 7.0);
    }
}

#[test]
fn bitwise_alias_exclusion_and_both_compilation_phases_are_explicit() {
    use suss_compile::portable::{
        prepare_fragment,
        resolve::{Environment, Phase},
    };
    let mut session = Session::new().unwrap();
    session
        .eval("(ns bitwise.alias (:require [cljs.core :as c]))")
        .unwrap();
    assert_eq!(bitwise_number(&mut session, "(c/bit-or 1 2 4)"), 7.0);
    session.eval("(ns bitwise.excluded (:refer-clojure :exclude [bit-or])) (def bit-or (fn [x y] (+ x y)))").unwrap();
    assert_eq!(bitwise_number(&mut session, "(bit-or 1 1)"), 2.0);
    assert_eq!(bitwise_number(&mut session, "(cljs.core/bit-or 1 1)"), 1.0);
    for phase in [Phase::Runtime, Phase::Macro] {
        let fragment = prepare_fragment("(bit-or 1 2 4)", &Environment::default(), phase).unwrap();
        suss_compile::runtime_abi::verify_artifact(
            &fragment.wasm,
            &suss_compile::runtime_abi::Manifest::default(),
        )
        .unwrap();
    }
}

#[test]
fn captured_variadic_bitwise_body_keeps_core_reducer_under_user_redefinition() {
    // Shipped core is direct-linked (design decision 2026-10-25): the variadic
    // body reduces with core's own operator, unlike ClojureScript's live var read,
    // which the pinned expected-primary observations still record.
    let mut session = Session::new().unwrap();
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/bitwise-capture-observations.json"
    ))
    .unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        // Four pinned public wrapper errors remain separate observations, never
        // counted as equal public-source matches. The five body diagnostics use
        // development-only JS to inspect the retained implementation directly.
        let source = case["native-source"].as_str().unwrap();
        let actual = bitwise_number(&mut session, source);
        assert!(
            matches_pinned(actual, &case["expected-native"], source),
            "{}: {actual:?}",
            case["id"]
        );
        session.collect().unwrap();
    }
    assert_eq!(bitwise_number(&mut session, "(bit-or 1 2 4)"), 7.0);
}

#[test]
fn variadic_bitwise_reducers_ignore_user_redefinition_and_recover_after_gc() {
    let mut session = Session::new().unwrap();
    session.eval("(def bitwise_saved bit-or)").unwrap();
    session.collect().unwrap();
    // No tail arguments: the original pair succeeds without reading a reducer.
    assert_eq!(
        bitwise_number(
            &mut session,
            "(with-redefs [cljs.core/bit-or nil] (bitwise_saved 1 2))"
        ),
        3.0
    );
    // Direct linking: redefinition or binding of the public var does not reach
    // the variadic body's reducer.
    for replacement in ["nil", "(fn [x] x)", "(fn [x y] (throw 29))"] {
        let source =
            format!("(with-redefs [cljs.core/bit-or {replacement}] (bitwise_saved 1 2 4))");
        assert_eq!(bitwise_number(&mut session, &source), 7.0, "{source}");
        session.collect().unwrap();
        assert_eq!(bitwise_number(&mut session, "(bitwise_saved 1 2 4)"), 7.0);
    }
    assert_eq!(
        bitwise_number(
            &mut session,
            "(binding [cljs.core/bit-or (fn [x y] (+ x y 100))] (bitwise_saved 1 2 4))"
        ),
        7.0
    );
    session.collect().unwrap();
    assert_eq!(bitwise_number(&mut session, "(bitwise_saved 1 2 4)"), 7.0);
}
