//! Executing source exception regressions for the replacement ABI pipeline.
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
fn eval_number(session: &mut Session, source: &str) -> Number {
    let result = session.eval(source).unwrap();
    number(session, &result)
}
fn language(session: &mut Session, source: &str) -> SessionValue {
    let SessionError::Language(payload) = session.eval(source).unwrap_err() else {
        panic!("expected an exact language payload from {source}")
    };
    payload
}

#[test]
fn source_throw_keeps_exact_scalar_payloads_and_recovers_after_gc() {
    let mut session = Session::new().unwrap();
    for (source, expected) in [
        ("(throw nil)", 0),
        ("(throw false)", 2),
        ("(throw true)", 4),
    ] {
        let payload = language(&mut session, source);
        session.collect().unwrap();
        let actual = session
            .inspect(&payload, |mut store, value| {
                Ok(suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap())
            })
            .unwrap();
        assert_eq!(actual, expected);
    }
    let payload = language(&mut session, "(throw -0.0)");
    session.collect().unwrap();
    assert_eq!(number(&mut session, &payload), num(-0.0));
    let payload = language(&mut session, "(throw \"\\uD800\")");
    session.collect().unwrap();
    let units = session
        .inspect(&payload, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(array
                .elems(&mut store)?
                .map(|unit| unit.unwrap_i32() as u16)
                .collect::<Vec<_>>())
        })
        .unwrap();
    assert_eq!(units, [0xd800]);
    assert_eq!(eval_number(&mut session, "(+ 20 22)"), num(42.0));
}

#[test]
fn source_try_preserves_results_cleanup_order_and_superseding_exceptions() {
    let mut session = Session::new().unwrap();
    assert_eq!(
        eval_number(&mut session, "(try 42 (finally 7))"),
        num(42.0)
    );
    assert_eq!(
        eval_number(
            &mut session,
            "(try (throw 7) (catch :default error (+ error 35)) (finally 9))"
        ),
        num(42.0)
    );
    assert_eq!(
        eval_number(
            &mut session,
            "(try (try (throw 7) (catch :default error (throw (+ error 2))) (finally 11)) (catch :default error error))"
        ),
        num(9.0)
    );
    for source in [
        "(try (try (throw 7) (finally (throw 42))) (catch :default error error))",
        "(try (try (throw 7) (catch :default error (throw 9)) (finally (throw 42))) (catch :default error error))",
    ] {
        assert_eq!(eval_number(&mut session, source), num(42.0));
    }
    assert_eq!(
        eval_number(
            &mut session,
            "(do (def counter 0) (try (try (do (def counter (+ (* counter 10) 1)) (throw 7)) (catch :default error (def counter (+ (* counter 10) 2)) (throw error)) (finally (def counter (+ (* counter 10) 3)))) (catch :default error (def counter (+ (* counter 10) 4)))) counter)"
        ),
        num(1234.0)
    );
}

#[test]
fn source_typed_catches_use_descriptor_identity_and_preserve_payload_roots() {
    let mut session = Session::new().unwrap();
    session.eval("(deftype CatchA [value]) (deftype CatchB [value]) (def thrown (CatchB. 7)) (def alias CatchB)").unwrap();
    assert_eq!(
        eval_number(
            &mut session,
            "(try (throw thrown) (catch CatchA error 9) (catch alias error 42) (catch :default error 11))"
        ),
        num(42.0)
    );
    let payload = session
        .eval("(try (throw thrown) (catch :default error error))")
        .unwrap();
    let root = session
        .inspect(&payload, |store, value| {
            Ok(value.unwrap_anyref().unwrap().to_owned_rooted(store)?)
        })
        .unwrap();
    session.collect().unwrap();
    let original = session.eval("thrown").unwrap();
    assert!(session
        .inspect(&original, |store, value| wasmtime::Rooted::ref_eq(
            &store,
            &root,
            value.unwrap_anyref().unwrap()
        ))
        .unwrap());
    assert_eq!(
        eval_number(
            &mut session,
            "(try (throw nil) (catch CatchA error 7) (catch :default error 42))"
        ),
        num(42.0)
    );
}

#[test]
fn source_catch_bindings_shadow_restore_and_survive_closure_capture() {
    let mut session = Session::new().unwrap();
    assert_eq!(
        eval_number(
            &mut session,
            "(let [error 35] (+ (try (throw 7) (catch :default error error)) error))"
        ),
        num(42.0)
    );
    let captured = session
        .eval("(let [outside 35] (fn [] (try (throw 7) (catch :default error (+ outside error)))))")
        .unwrap();
    session.collect().unwrap();
    let result = session.invoke(&captured, &[]).unwrap();
    assert_eq!(number(&mut session, &result), num(42.0));
    let captured = session
        .eval("(try (throw 7) (catch :default error (fn [] error)))")
        .unwrap();
    session.collect().unwrap();
    let result = session.invoke(&captured, &[]).unwrap();
    assert_eq!(number(&mut session, &result), num(7.0));
}

#[test]
fn source_try_rejects_invalid_handlers_and_cross_region_recur_atomically() {
    let mut session = Session::new().unwrap();
    let before = session.stats().binding_cells;
    for source in [
        "(def leaked 7) (throw)",
        "(def leaked 7) (throw 1 2)",
        "(def leaked 7) (try 7 (catch :default))",
        "(def leaked 7) (try 7 (finally 9) 11)",
        "(def leaked 7) (try 7 (catch :default error 9) (catch :default again 11))",
        "(def leaked 7) (loop [x 0] (try (recur 1) (finally 7)))",
        "(def leaked 7) (loop [x 0] (try (throw 7) (catch :default error (recur 1))))",
    ] {
        let SessionError::Compile(diagnostic) = session.eval(source).unwrap_err() else {
            panic!("expected located rejection: {source}")
        };
        assert!(diagnostic.span.start < diagnostic.span.end && diagnostic.span.end <= source.len());
        assert_eq!(session.stats().binding_cells, before, "{source}");
    }
    assert!(matches!(
        session.eval("leaked"),
        Err(SessionError::Compile(_))
    ));
    assert_eq!(
        eval_number(
            &mut session,
            "(try (loop [again true value 0] (if again (recur false 42) value)) (finally 7))"
        ),
        num(42.0)
    );
}

#[test]
fn source_handlers_catch_runtime_errors_without_publishing_failed_initializers() {
    let mut session = Session::new().unwrap();
    session
        .eval("(def old 7) (def counter 0) (def wrong (fn [] 9))")
        .unwrap();
    assert_eq!(
        eval_number(
            &mut session,
            "(try (def old (do (def counter (+ counter 1)) (wrong 7))) (catch :default error 42) (finally (def counter (+ counter 10))))"
        ),
        num(42.0)
    );
    assert_eq!(eval_number(&mut session, "old"), num(7.0));
    assert_eq!(eval_number(&mut session, "counter"), num(11.0));
    assert_eq!(eval_number(&mut session, "(+ 20 22)"), num(42.0));
}

#[test]
fn source_throw_divergence_stops_later_operands_and_failed_publication() {
    let mut session = Session::new().unwrap();
    session
        .eval("(def counter 0) (def tick (fn [] (def counter (+ counter 1))))")
        .unwrap();
    for (source, result, effects) in [
        (
            "(try (+ (tick) (throw (tick)) (tick)) (catch :default error error) (finally (tick)))",
            2.0,
            3.0,
        ),
        (
            "(try ((do (tick) (throw 7)) (tick)) (catch :default error error))",
            7.0,
            1.0,
        ),
        (
            "(try (if (throw 7) (tick) (tick)) (catch :default error error))",
            7.0,
            0.0,
        ),
        (
            "(try (let [x (throw 7) y (tick)] y) (catch :default error error))",
            7.0,
            0.0,
        ),
        (
            "(try (loop [x (tick) y (throw 7)] y) (catch :default error error))",
            7.0,
            1.0,
        ),
        (
            "(try (loop [again true value 0] (if again (recur false (throw 7)) value)) (catch :default error error))",
            7.0,
            0.0,
        ),
        (
            "(try (throw (throw 7)) (catch :default error error))",
            7.0,
            0.0,
        ),
    ] {
        session.eval("(def counter 0)").unwrap();
        assert_eq!(
            eval_number(&mut session, source),
            num(result),
            "{source}"
        );
        assert_eq!(
            eval_number(&mut session, "counter"),
            num(effects),
            "{source}"
        );
    }
    session.eval("(def old 7) (defonce bound 11)").unwrap();
    assert_eq!(
        eval_number(
            &mut session,
            "(try (def old (do (tick) (throw 42))) (catch :default error error))"
        ),
        num(42.0)
    );
    assert_eq!(eval_number(&mut session, "old"), num(7.0));
    session.eval("(defonce bound (throw 42))").unwrap();
    assert_eq!(eval_number(&mut session, "bound"), num(11.0));
    assert_eq!(
        eval_number(
            &mut session,
            "(try (defonce unbound (throw 7)) (catch :default error error))"
        ),
        num(7.0)
    );
    session.eval("(defonce unbound 42)").unwrap();
    assert_eq!(eval_number(&mut session, "unbound"), num(42.0));
}

#[test]
fn unhandled_nil_and_closure_run_cleanup_without_replacing_payload() {
    let mut session = Session::new().unwrap();
    session.eval("(def counter 0)").unwrap();
    let nil = language(
        &mut session,
        "(try (throw nil) (finally (def counter (+ counter 1))))",
    );
    assert_eq!(eval_number(&mut session, "counter"), num(1.0));
    assert_eq!(
        session
            .inspect(&nil, |mut store, value| Ok(suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap()))
            .unwrap(),
        0
    );
    let closure = language(
        &mut session,
        "(let [captured 42] (try (throw (fn [] captured)) (finally (def counter (+ counter 1)))))",
    );
    session.collect().unwrap();
    let value = session.invoke(&closure, &[]).unwrap();
    assert_eq!(number(&mut session, &value), num(42.0));
    assert_eq!(eval_number(&mut session, "counter"), num(2.0));
}

#[test]
fn typed_catch_operands_are_lazy_ordered_and_short_circuit_after_matching() {
    let mut session = Session::new().unwrap();
    session
        .eval("(deftype First []) (deftype Second []) (def counter 0)")
        .unwrap();
    assert_eq!(
        eval_number(
            &mut session,
            "(try 42 (catch (do (def counter 99) First) error 7))"
        ),
        num(42.0)
    );
    assert_eq!(eval_number(&mut session, "counter"), num(0.0));
    assert_eq!(eval_number(&mut session, "(try (throw (Second.)) (catch (do (def counter (+ (* counter 10) 1)) First) error 7) (catch (do (def counter (+ (* counter 10) 2)) Second) error 42) (catch (do (def counter 99) First) error 9) (finally (def counter (+ (* counter 10) 3))))"), num(42.0));
    assert_eq!(eval_number(&mut session, "counter"), num(123.0));
    assert_eq!(eval_number(&mut session, "(try (try (throw 7) (catch (throw 42) error 9) (finally (def counter (+ counter 1)))) (catch :default error error))"), num(42.0));
    assert_eq!(eval_number(&mut session, "counter"), num(124.0));
}

#[test]
fn nested_exception_regions_capture_nominal_fields_and_loop_bindings() {
    let mut session = Session::new().unwrap();
    session.eval("(defprotocol ReadCaptured (read-captured [this])) (deftype Captured [value] ReadCaptured (read-captured [this] (try (throw value) (catch :default error (fn [] (+ error value))) (finally 7)))) (def object (Captured. 21))").unwrap();
    let closure = session.eval("(read-captured object)").unwrap();
    session.collect().unwrap();
    let value = session.invoke(&closure, &[]).unwrap();
    assert_eq!(number(&mut session, &value), num(42.0));
    assert_eq!(eval_number(&mut session, "(loop [again true value 7] (if again (recur false (try (throw (+ value 35)) (catch :default error error) (finally 9))) value))"), num(42.0));
}

#[test]
fn runtime_errors_read_options_without_a_presence_mask() {
    // A runtime-thrown error keeps nil in the field that holds an
    // Error-constructor presence mask; reading `cause` or wrapping it must
    // treat that as "no options" rather than trap (#335).
    let mut session = Session::new().unwrap();
    session.eval("(def arity (fn [a] a))").unwrap();
    let cause = session
        .eval("(try (arity 1 2) (catch :default e (.-cause e)))")
        .unwrap();
    session.collect().unwrap();
    let tag = session
        .inspect(&cause, |mut store, value| {
            suss_cli::portable_session::sentinel_tag(&mut store, &value)
        })
        .unwrap();
    assert_eq!(tag, Some(6), "undefined");
    session
        .eval("(try (arity 1 2) (catch :default e (js/Error. \"wrapped\" e)))")
        .unwrap();
}
