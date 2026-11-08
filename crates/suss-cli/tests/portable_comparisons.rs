use suss_cli::portable_session::{Session, SessionError, SessionValue};
use suss_compile::portable_session::sentinel_tag;

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

fn number(session: &mut Session, value: &SessionValue) -> Number {
    session
        .inspect(value, |mut store, value| {
            exact_number(&mut store, &value)
        })
        .unwrap()
}
fn eval_number(session: &mut Session, source: &str) -> Number {
    let value = session.eval(source).unwrap();
    number(session, &value)
}
fn eval_bool(session: &mut Session, source: &str) -> bool {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |mut store, value| {
            match sentinel_tag(&mut store, &value)?.unwrap()
            {
                2 => Ok(false),
                4 => Ok(true),
                other => panic!("Boolean {other}"),
            }
        })
        .unwrap()
}

#[test]
fn comparisons_match_independently_encoded_primary_corpus() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/comparison-cases.json")).unwrap();
    let mut session = Session::new().unwrap();
    let mut ids = std::collections::BTreeSet::new();
    let mut corrected = 0;
    let mut evaluated = 0;
    let cases = corpus["cases"].as_array().unwrap();
    let recorded = |id: &str| {
        &cases
            .iter()
            .find(|case| case["id"] == id)
            .unwrap_or_else(|| panic!("no case {id}"))["expected"]
    };
    for case in cases {
        let id = case["id"].as_str().unwrap();
        assert!(ids.insert(id));
        let source = case["source"].as_str().unwrap();
        // Clojure governs (#293, #294): an inline comparison evaluates each
        // operand once, as its first-class call does, so the recorded macro
        // erasure and repeated middle operands give way to the call's result.
        let expected = match clojure_evaluates(id) {
            Some(call) => {
                evaluated += 1;
                recorded(call)
            }
            None => &case["expected"],
        };
        // Clojure governs: the recorded primary coerced non-number operands.
        if let Some(operator) = clojure_rejects(id) {
            corrected += 1;
            assert_eq!(
                thrown(&mut session, source),
                format!("{operator} requires number operands"),
                "{source}"
            );
            session.collect().unwrap();
            continue;
        }
        let actual = match expected["tag"].as_str().unwrap() {
            "bool" => serde_json::json!({"tag":"bool", "value":eval_bool(&mut session, source)}),
            "f64" => {
                number_observation(eval_number(&mut session, source), expected, source)
            }
            tag => panic!("unexpected tag {tag}"),
        };
        assert_eq!(&actual, expected, "{source}");
        session.collect().unwrap();
    }
    assert_eq!(ids.len(), 123);
    assert_eq!(corrected, 49);
    assert_eq!(evaluated, 12);
}

/// The first-class call case whose recorded result an inline comparison case
/// must match: unary operands are evaluated, middle operands once, and
/// operands after a false pair still run.
fn clojure_evaluates(id: &str) -> Option<&'static str> {
    Some(match id {
        "lt-macro-unary" => "lt-runtime-unary",
        "le-macro-unary" => "le-runtime-unary",
        "gt-macro-unary" => "gt-runtime-unary",
        "ge-macro-unary" => "ge-runtime-unary",
        "eq-unary-macro-erases" => "eq-unary-runtime-effects",
        "lt-macro-chain" => "lt-runtime-chain",
        "le-macro-chain" => "le-runtime-chain",
        "gt-macro-chain" => "gt-runtime-chain",
        "ge-macro-chain" => "ge-runtime-chain",
        "eq-chain-macro" => "eq-chain-runtime",
        "false-chain-macro" => "false-chain-runtime",
        "macro-false-skips-throw" => "runtime-false-evaluates-throw",
        _ => return None,
    })
}

/// Cases whose recorded primary result coerced a non-number operand, with
/// the operator Clojure rejects it in.
fn clojure_rejects(id: &str) -> Option<&'static str> {
    const NON_NUMBERS: [&str; 10] = [
        "nil",
        "false",
        "true",
        "strings",
        "mixed-string",
        "bad-string",
        "empty-string",
        "astral-order",
        "surrogate-order",
        "undefined",
    ];
    for (prefix, operator) in [("lt-", "<"), ("le-", "<="), ("gt-", ">"), ("ge-", ">=")] {
        if id.strip_prefix(prefix).is_some_and(|rest| NON_NUMBERS.contains(&rest)) {
            return Some(operator);
        }
    }
    [
        "macro-eq-strings",
        "macro-eq-mixed",
        "macro-eq-boolean-number",
        "macro-eq-false-number",
        "macro-eq-nil-false",
        "macro-eq-nil-undefined",
        "macro-eq-nil-nil",
        "strict-object-identity",
        "strict-object-distinct",
    ]
    .contains(&id)
    .then_some("==")
}
/// The message of the language error `source` throws; the session has no
/// core, so the error is caught and its message read from the error struct.
fn thrown(session: &mut Session, source: &str) -> String {
    let error = session
        .eval(&format!("(try {source} (catch :default e e))"))
        .unwrap();
    session
        .inspect(&error, |mut store, value| {
            let error = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let message = error.field(&mut store, 1)?;
            let message = message.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(String::from_utf16(
                &message
                    .elems(&mut store)?
                    .map(|unit| unit.unwrap_i32() as u16)
                    .collect::<Vec<_>>(),
            )
            .unwrap())
        })
        .unwrap()
}

#[test]
fn retained_comparison_captures_follow_accepted_design_with_explicit_primary_divergence() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/comparison-capture-divergences.json"
    ))
    .unwrap();
    let mut session = Session::new().unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        assert_eq!(case["expected-primary"]["tag"], "string");
        assert_eq!(
            eval_bool(&mut session, case["source"].as_str().unwrap()),
            case["expected-native"]["value"].as_bool().unwrap()
        );
        session.collect().unwrap();
    }
}

#[test]
fn comparison_macro_arity_unary_evaluation_and_lexical_shadowing_remain_distinct_from_values() {
    let mut session = Session::new().unwrap();
    for name in ["<", "<=", ">", ">=", "=="] {
        // A unary comparison evaluates its operand, so its throw escapes (#294).
        assert!(matches!(
            session.eval(&format!("({name} (throw 7))")),
            Err(SessionError::Language(_))
        ));
        assert!(eval_bool(&mut session, &format!("({name} 7)")));
        assert!(matches!(
            session.eval(&format!("({name})")),
            Err(SessionError::Compile(_))
        ));
        let original = session.eval(name).unwrap();
        session.collect().unwrap();
        assert!(matches!(
            session.invoke(&original, &[]),
            Err(SessionError::Language(_))
        ));
        assert_eq!(
            eval_number(
                &mut session,
                &format!("(let [{name} (fn [x] 77)] ({name} 1))")
            ),
            num(77.0)
        );
        let argument = session.eval("##NaN").unwrap();
        let result = session.invoke(&original, &[&argument]).unwrap();
        let sentinel = session
            .inspect(&result, |mut store, value| {
                Ok(sentinel_tag(&mut store, &value)?.unwrap())
            })
            .unwrap();
        assert_eq!(sentinel, 4);
    }
}

#[test]
fn comparison_macro_and_runtime_names_preserve_aliases_user_shadowing_and_gc() {
    let mut session = Session::new().unwrap();
    // Explicit refers conflict with an own declaration under the accepted
    // namespace contract. Auto-referred core values permit user shadowing.
    assert!(matches!(
        session.eval(
            "(ns comparison-conflict (:require [cljs.core :refer [<]])) (def < (fn [x y] 77))"
        ),
        Err(SessionError::Compile(_))
    ));
    session
        .eval("(ns comparison-consumer (:require [cljs.core :as c])) (def retained <)")
        .unwrap();
    session.eval("(def < (fn [x y] 77))").unwrap();
    assert_eq!(eval_number(&mut session, "(< 1 2)"), num(77.0));
    assert!(eval_bool(&mut session, "(c/< 1 2)"));
    assert!(eval_bool(&mut session, "(let [f retained] (f 1 2))"));
    let function = session.eval("retained").unwrap();
    session.eval("(def retained nil)").unwrap();
    session.collect().unwrap();
    let left = session.eval("1").unwrap();
    let right = session.eval("2").unwrap();
    let value = session.invoke(&function, &[&left, &right]).unwrap();
    let sentinel = session
        .inspect(&value, |mut store, value| {
            Ok(sentinel_tag(&mut store, &value)?.unwrap())
        })
        .unwrap();
    assert_eq!(sentinel, 4);
    session
        .eval("(ns comparison-excluded (:refer-clojure :exclude [<]))")
        .unwrap();
    assert!(matches!(
        session.eval("(< 1 2)"),
        Err(SessionError::Compile(_))
    ));
    assert!(eval_bool(&mut session, "(cljs.core/< 1 2)"));
}

#[test]
fn comparison_utf16_prefix_loop_has_fuel_recovery_and_typed_coercion_errors() {
    let mut session = Session::new().unwrap();
    assert_eq!(
        thrown(&mut session, "(< \"a\" \"b\")"),
        "< requires number operands"
    );
    // Shipped core keeps the coercing comparison that owns the UTF-16 loop.
    let function = session.eval_privileged("(fn [a b] (< a b))").unwrap();
    let text = "😀".repeat(4096);
    let left = session.eval(&format!("\"{text}\"")).unwrap();
    let right = session.eval(&format!("\"{text}x\"")).unwrap();
    session.collect().unwrap();
    session.set_operation_fuel(5000);
    let SessionError::Trap(error) = session.invoke(&function, &[&left, &right]).unwrap_err() else {
        panic!("expected UTF-16 comparison fuel exhaustion")
    };
    assert_eq!(
        error.downcast_ref::<wasmtime::Trap>(),
        Some(&wasmtime::Trap::OutOfFuel)
    );
    session.collect().unwrap();
    session.set_operation_fuel(1_000_000);
    let answer = session.invoke(&function, &[&left, &right]).unwrap();
    let sentinel = session
        .inspect(&answer, |mut store, value| {
            Ok(sentinel_tag(&mut store, &value)?.unwrap())
        })
        .unwrap();
    assert_eq!(sentinel, 4);
    session.eval("(deftype ComparisonBad [])").unwrap();
    assert!(matches!(
        session.eval("(< (ComparisonBad.) 2)"),
        Err(SessionError::Language(_))
    ));
    assert!(eval_bool(&mut session, "(< 1 2)"));
}

#[test]
fn large_comparisons_execute_runtime_alias_and_direct_form_without_truncating_arguments() {
    let mut session = Session::new().unwrap();
    let args = (0..300)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(eval_bool(&mut session, &format!("(let [f <] (f {args}))")));
    assert!(eval_bool(&mut session, &format!("(< {args})")));
    // The final pair must participate too; a truncated prefix would be true.
    let late_equal = (0..299).chain(std::iter::once(298))
        .map(|n| n.to_string()).collect::<Vec<_>>().join(" ");
    assert!(!eval_bool(&mut session, &format!("(< {late_equal})")));
    assert!(eval_bool(&mut session, "(< 1 2)"));
}

#[test]
fn explicit_user_refers_preserve_pinned_core_macros_but_aliases_call_user_vars() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("comparison_provider.sus"), "(ns comparison-provider) (def < (fn [x y] 77)) (def <= (fn [x y] 78)) (def > (fn [x y] 79)) (def >= (fn [x y] 80)) (def == (fn [x y] 81))").unwrap();
    let mut session = Session::with_options(suss_cli::portable_session::SessionOptions {
        source_paths: vec![root.path().into()],
        ..Default::default()
    })
    .unwrap();
    session
        .eval(
            "(ns comparison-referrer (:require [comparison-provider :as p :refer [< <= > >= ==]]))",
        )
        .unwrap();
    session.collect().unwrap();
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/comparison-refers.json")).unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "bool" => serde_json::json!({"tag":"bool", "value":eval_bool(&mut session, source)}),
            "f64" => {
                number_observation(eval_number(&mut session, source), &case["expected"], source)
            }
            tag => panic!("unexpected referral tag {tag}"),
        };
        assert_eq!(actual, case["expected"], "{source}");
    }
    assert!(eval_bool(&mut session, "(cljs.core/< 1 2)"));
}
