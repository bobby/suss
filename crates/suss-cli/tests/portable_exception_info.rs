//! Portable ExceptionInfo core behavior in the persistent shared runtime.
use suss_cli::portable_session::{Session, SessionError, SessionValue};

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
fn number(session: &mut Session, value: &SessionValue) -> Number {
    session
        .inspect(value, |mut store, value| {
            exact_number(&mut store, &value)
        })
        .unwrap()
}
fn eval(session: &mut Session, source: &str) -> Number {
    let value = session.eval(source).unwrap();
    number(session, &value)
}
#[test]
fn exception_info_keeps_message_data_cause_and_its_nominal_identity() {
    let mut s = Session::new().unwrap();
    s.eval("(def error (ex-info \"message\" 7 9))").unwrap();
    assert_eq!(
        eval(&mut s, "(+ (* (ex-data error) 10) (ex-cause error))"),
        num(79.0)
    );
    assert_eq!(
        eval(&mut s, "(if (instance? ExceptionInfo error) 1 0)"),
        num(1.0)
    );
    assert_eq!(
        eval(&mut s, "(if (ex-cause (ex-info \"message\" 7)) 1 0)"),
        num(0.0)
    );
    // The pinned constructor retains supplied values, rather than enforcing JVM types.
    assert_eq!(
        eval(&mut s, "(ex-message (ex-info 42 nil))"),
        num(42.0)
    );
    assert_eq!(
        eval(&mut s, "(if (ex-data (ex-info false false false)) 1 0)"),
        num(0.0)
    );
}
#[test]
fn exception_info_payload_and_closure_fields_survive_throw_frame_exit_and_gc() {
    let mut s = Session::new().unwrap();
    s.eval("(def ^:dynamic *value* 1)").unwrap();
    let error = s.eval("(try (binding [*value* 7] (throw (ex-info \"saved\" (let [x *value*] (fn [] x)) (fn [] *value*)))) (catch ExceptionInfo error error))").unwrap();
    s.collect().unwrap();
    let data_getter = s.eval("ex-data").unwrap();
    let cause_getter = s.eval("ex-cause").unwrap();
    let data = s.invoke(&data_getter, &[&error]).unwrap();
    let cause = s.invoke(&cause_getter, &[&error]).unwrap();
    s.collect().unwrap();
    let captured = s.invoke(&data, &[]).unwrap();
    assert_eq!(number(&mut s, &captured), num(7.0));
    let current = s.invoke(&cause, &[]).unwrap();
    assert_eq!(number(&mut s, &current), num(1.0));
}
#[test]
fn ordinary_values_and_user_objects_do_not_impersonate_exception_info() {
    let mut s = Session::new().unwrap();
    s.eval("(deftype Fake [message data cause])").unwrap();
    for source in [
        "nil",
        "false",
        "7",
        "\"message\"",
        "(fn [] 7)",
        "(Fake. 1 2 3)",
    ] {
        for getter in ["ex-data", "ex-message", "ex-cause"] {
            assert_eq!(
                eval(&mut s, &format!("(if ({getter} {source}) 1 0)")),
                num(0.0)
            );
        }
        assert_eq!(
            eval(
                &mut s,
                &format!("(if (instance? ExceptionInfo {source}) 1 0)")
            ),
            num(0.0)
        );
    }
}
#[test]
fn exception_info_functions_are_live_first_class_canonical_core_bindings() {
    let mut s = Session::new().unwrap();
    s.eval("(ns app (:require [cljs.core :as core])) (def maker core/ex-info) (def saved (maker \"message\" 7))").unwrap();
    assert_eq!(eval(&mut s, "(core/ex-data saved)"), num(7.0));
    assert_eq!(
        eval(
            &mut s,
            "(with-redefs [core/ex-data (fn [x] 42)] (core/ex-data saved))"
        ),
        num(42.0)
    );
    assert_eq!(eval(&mut s, "(suss.core/ex-data saved)"), num(7.0));
    assert_eq!(
        eval(&mut s, "(let [ex-data (fn [x] 9)] (ex-data saved))"),
        num(9.0)
    );
}
#[test]
fn exception_info_arity_errors_happen_after_ordered_arguments_and_recover() {
    let mut s = Session::new().unwrap();
    s.eval("(def maker ex-info) (def seen 0)").unwrap();
    let result = s.eval("(maker (do (set! seen 7) \"message\"))");
    assert!(matches!(result, Err(SessionError::Language(_))));
    assert_eq!(eval(&mut s, "seen"), num(7.0));
    assert_eq!(
        eval(&mut s, "(ex-data (ex-info \"next\" 42))"),
        num(42.0)
    );
}

#[test]
fn exception_info_distinguishes_false_nil_and_missing_constructor_fields() {
    let mut s = Session::new().unwrap();
    for (source, expected) in [
        ("(ex-data (ex-info \"message\" false))", 2),
        ("(ex-message (ex-info false nil))", 2),
        ("(ex-cause (ex-info \"message\" nil false))", 2),
        ("(ex-data (ex-info \"message\" nil))", 0),
        ("(ex-cause (ex-info \"message\" 7))", 0),
        ("(ex-cause (ExceptionInfo. \"message\" 7))", 6),
        ("(ex-data (ExceptionInfo. \"message\"))", 6),
    ] {
        let value = s.eval(source).unwrap();
        let sentinel = s
            .inspect(&value, |mut store, value| {
                Ok(suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap())
            })
            .unwrap();
        assert_eq!(sentinel, expected, "{source}");
    }
}

#[test]
fn exception_info_constructor_and_predicates_are_direct_linked_to_the_core_class() {
    // Core's own references to ExceptionInfo are direct-linked, as in Clojure:
    // redefining the public class var changes user call sites only.
    let mut s = Session::new().unwrap();
    s.eval("(deftype Alternate [message data cause]) (def original (ex-info \"original\" 7))")
        .unwrap();
    assert_eq!(
        eval(
            &mut s,
            "(with-redefs [ExceptionInfo Alternate] (if (ex-data original) 1 0))"
        ),
        num(1.0)
    );
    assert_eq!(
        eval(
            &mut s,
            "(with-redefs [ExceptionInfo Alternate] (ex-data (ex-info \"changed\" 42 9)))"
        ),
        num(42.0)
    );
    assert_eq!(eval(&mut s, "(ex-data original)"), num(7.0));
}

#[test]
fn exception_info_getters_ignore_user_types_when_the_class_var_is_redefined() {
    let mut s = Session::new().unwrap();
    s.eval("(deftype Reordered [data message cause]) (deftype Missing [message data])")
        .unwrap();
    for source in [
        "(with-redefs [ExceptionInfo Reordered] (if (ex-data (Reordered. 42 \"message\" 9)) 1 0))",
        "(with-redefs [ExceptionInfo Missing] (if (ex-cause (Missing. \"message\" 7)) 1 0))",
    ] {
        assert_eq!(eval(&mut s, source), num(0.0), "{source}");
    }
}

#[test]
fn ordinary_exception_info_calls_return_a_truthy_non_exception_realm_value() {
    let mut s = Session::new().unwrap();
    assert_eq!(
        eval(&mut s, "(if (ExceptionInfo \"message\" 7 nil) 1 0)"),
        num(1.0)
    );
    assert_eq!(
        eval(
            &mut s,
            "(if (instance? ExceptionInfo (ExceptionInfo \"message\" 7 nil)) 1 0)"
        ),
        num(0.0)
    );
    assert_eq!(
        eval(
            &mut s,
            "(if (ex-data (ExceptionInfo \"message\" 7 nil)) 1 0)"
        ),
        num(0.0)
    );
    let value = s.eval("(ExceptionInfo \"saved\" (fn [] 42) nil)").unwrap();
    s.collect().unwrap();
    assert_eq!(
        s.inspect(&value, |store, value| Ok(value
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)?
            .is_some()))
            .unwrap(),
        true
    );
}

#[test]
fn two_argument_ex_info_is_direct_linked_to_its_three_argument_arity() {
    let mut s = Session::new().unwrap();
    let saved = s.eval("ex-info").unwrap();
    let message = s.eval("\"message\"").unwrap();
    let data = s.eval("7").unwrap();
    let cause = s.eval("9").unwrap();
    s.eval("(ns suss.core) (def ex-info (fn [message data cause] (if cause 99 42)))")
        .unwrap();
    s.collect().unwrap();
    // Core's recursion is direct-linked, as in Clojure, so redefining the
    // var does not change either arity of the original function.
    let getter = s.eval("ex-data").unwrap();
    for args in [&[&message, &data][..], &[&message, &data, &cause][..]] {
        let error = s.invoke(&saved, args).unwrap();
        let result = s.invoke(&getter, &[&error]).unwrap();
        assert_eq!(number(&mut s, &result), num(7.0));
    }
}

#[test]
fn exception_info_source_arguments_run_once_before_construction() {
    let mut s = Session::new().unwrap();
    s.eval("(def seen 0)").unwrap();
    assert_eq!(
        eval(
            &mut s,
            "(ex-cause (ex-info (do (set! seen (+ (* seen 10) 1)) seen) (do (set! seen (+ (* seen 10) 2)) seen) (do (set! seen (+ (* seen 10) 3)) seen)))"
        ),
        num(123.0)
    );
    assert_eq!(eval(&mut s, "seen"), num(123.0));
}

#[test]
fn error_message_family_remains_original_when_info_class_is_replaced() {
    let mut s = Session::new().unwrap();
    s.eval("(def original (ex-info 42 7)) (deftype Other [message data cause])")
        .unwrap();
    assert_eq!(
        eval(
            &mut s,
            "(with-redefs [ExceptionInfo Other] (ex-message original))"
        ),
        num(42.0)
    );
    assert_eq!(
        eval(
            &mut s,
            "(with-redefs [ExceptionInfo Other] (if (ex-message (Other. 99 7 nil)) 1 0))"
        ),
        num(0.0)
    );
}
