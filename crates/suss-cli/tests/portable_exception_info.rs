//! Portable ExceptionInfo core behavior in the persistent shared runtime.
use suss_cli::portable_session::{Session, SessionError, SessionValue};
use wasmtime::Val;
fn number(session: &mut Session, value: &SessionValue) -> u64 {
    session
        .inspect(value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Number layout")
            };
            Ok(*bits)
        })
        .unwrap()
}
fn eval(session: &mut Session, source: &str) -> u64 {
    let value = session.eval(source).unwrap();
    number(session, &value)
}
#[test]
fn exception_info_keeps_message_data_cause_and_its_nominal_identity() {
    let mut s = Session::new().unwrap();
    s.eval("(def error (ex-info \"message\" 7 9))").unwrap();
    assert_eq!(
        eval(&mut s, "(+ (* (ex-data error) 10) (ex-cause error))"),
        79.0f64.to_bits()
    );
    assert_eq!(
        eval(&mut s, "(if (instance? ExceptionInfo error) 1 0)"),
        1.0f64.to_bits()
    );
    assert_eq!(
        eval(&mut s, "(if (ex-cause (ex-info \"message\" 7)) 1 0)"),
        0.0f64.to_bits()
    );
    // The pinned constructor retains supplied values, rather than enforcing JVM types.
    assert_eq!(
        eval(&mut s, "(ex-message (ex-info 42 nil))"),
        42.0f64.to_bits()
    );
    assert_eq!(
        eval(&mut s, "(if (ex-data (ex-info false false false)) 1 0)"),
        0.0f64.to_bits()
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
    assert_eq!(number(&mut s, &captured), 7.0f64.to_bits());
    let current = s.invoke(&cause, &[]).unwrap();
    assert_eq!(number(&mut s, &current), 1.0f64.to_bits());
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
                0.0f64.to_bits()
            );
        }
        assert_eq!(
            eval(
                &mut s,
                &format!("(if (instance? ExceptionInfo {source}) 1 0)")
            ),
            0.0f64.to_bits()
        );
    }
}
#[test]
fn exception_info_functions_are_live_first_class_canonical_core_bindings() {
    let mut s = Session::new().unwrap();
    s.eval("(ns app (:require [cljs.core :as core])) (def maker core/ex-info) (def saved (maker \"message\" 7))").unwrap();
    assert_eq!(eval(&mut s, "(core/ex-data saved)"), 7.0f64.to_bits());
    assert_eq!(
        eval(
            &mut s,
            "(with-redefs [core/ex-data (fn [x] 42)] (core/ex-data saved))"
        ),
        42.0f64.to_bits()
    );
    assert_eq!(eval(&mut s, "(suss.core/ex-data saved)"), 7.0f64.to_bits());
    assert_eq!(
        eval(&mut s, "(let [ex-data (fn [x] 9)] (ex-data saved))"),
        9.0f64.to_bits()
    );
}
#[test]
fn exception_info_arity_errors_happen_after_ordered_arguments_and_recover() {
    let mut s = Session::new().unwrap();
    s.eval("(def maker ex-info) (def seen 0)").unwrap();
    let result = s.eval("(maker (do (set! seen 7) \"message\"))");
    assert!(matches!(result, Err(SessionError::Language(_))));
    assert_eq!(eval(&mut s, "seen"), 7.0f64.to_bits());
    assert_eq!(
        eval(&mut s, "(ex-data (ex-info \"next\" 42))"),
        42.0f64.to_bits()
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
            .inspect(&value, |store, value| {
                Ok(value
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .unwrap()
                    .get_u32())
            })
            .unwrap();
        assert_eq!(sentinel, expected, "{source}");
    }
}

#[test]
fn exception_info_constructor_and_predicates_follow_the_current_class_cell() {
    let mut s = Session::new().unwrap();
    s.eval("(deftype Alternate [message data cause]) (def original (ex-info \"original\" 7))")
        .unwrap();
    assert_eq!(
        eval(
            &mut s,
            "(with-redefs [ExceptionInfo Alternate] (if (ex-data original) 1 0))"
        ),
        0.0f64.to_bits()
    );
    assert_eq!(
        eval(
            &mut s,
            "(with-redefs [ExceptionInfo Alternate] (ex-data (ex-info \"changed\" 42 9)))"
        ),
        42.0f64.to_bits()
    );
    assert_eq!(eval(&mut s, "(ex-data original)"), 7.0f64.to_bits());
}

#[test]
fn exception_info_getters_use_field_names_when_the_class_is_redefined() {
    let mut s = Session::new().unwrap();
    s.eval("(deftype Reordered [data message cause]) (deftype Missing [message data])")
        .unwrap();
    assert_eq!(
        eval(
            &mut s,
            "(with-redefs [ExceptionInfo Reordered] (ex-data (Reordered. 42 \"message\" 9)))"
        ),
        42.0f64.to_bits()
    );
    let bits = eval(
        &mut s,
        "(with-redefs [ExceptionInfo Missing] (+ (ex-cause (Missing. \"message\" 7)) 1))",
    );
    assert!(f64::from_bits(bits).is_nan());
}

#[test]
fn ordinary_exception_info_calls_return_a_truthy_non_exception_realm_value() {
    let mut s = Session::new().unwrap();
    assert_eq!(
        eval(&mut s, "(if (ExceptionInfo \"message\" 7 nil) 1 0)"),
        1.0f64.to_bits()
    );
    assert_eq!(
        eval(
            &mut s,
            "(if (instance? ExceptionInfo (ExceptionInfo \"message\" 7 nil)) 1 0)"
        ),
        0.0f64.to_bits()
    );
    assert_eq!(
        eval(
            &mut s,
            "(if (ex-data (ExceptionInfo \"message\" 7 nil)) 1 0)"
        ),
        0.0f64.to_bits()
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
