//! Reusing native code must preserve every Store's execution and macro effects.
use suss_cli::{
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionOptions, SessionValue},
};

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
/// An expected number compares by `num`: integral values are longs.
impl PartialEq<f64> for Number {
    fn eq(&self, expected: &f64) -> bool {
        *self == num(*expected)
    }
}

fn read_number(session: &mut Session, value: &SessionValue) -> Number {
    session
        .inspect(value, |mut store, value| {
            exact_number(&mut store, &value)
        })
        .unwrap()
}
fn number(session: &mut Session, source: &str) -> Number {
    let value = session.eval(source).unwrap();
    read_number(session, &value)
}

#[test]
fn compiled_code_reuse_preserves_initializers_store_isolation_and_reset() {
    let mut first = Session::new().unwrap();
    let mut second = Session::new().unwrap();
    for session in [&mut first, &mut second] {
        session.eval("(def counter 0)").unwrap();
    }
    let increment = "(def counter (+ counter 1))";
    assert_eq!(number(&mut first, increment), 1.0);
    assert_eq!(number(&mut first, increment), 2.0);
    assert_eq!(number(&mut second, increment), 1.0);
    let once = "(defonce owned (do (def counter (+ counter 1)) counter))";
    first.eval(once).unwrap();
    first.eval(once).unwrap();
    assert_eq!(number(&mut first, "counter"), 3.0);
    assert_eq!(number(&mut second, "counter"), 1.0);
    first.collect().unwrap();
    first.reset().unwrap();
    assert!(first.eval("counter").is_err());
    first.eval("(def counter 0)").unwrap();
    assert_eq!(number(&mut first, increment), 1.0);
    assert_eq!(number(&mut second, "counter"), 1.0);
}

#[test]
fn identical_macro_output_still_executes_macro_effects_each_time() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("tools.sus"),
        "(ns tools) (def count (atom 0)) \
         (defmacro answer [] (do (swap! count inc) 42)) \
         (defmacro ticks [] @count)",
    )
    .unwrap();
    let mut runtime = Session::with_options(SessionOptions {
        source_paths: vec![project.path().to_owned()],
        ..Default::default()
    })
    .unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    runtime
        .eval_with_macros("(ns user (:require-macros [tools :as t]))", &mut macros)
        .unwrap();
    for expected in [1.0, 2.0, 3.0] {
        let value = runtime.eval_with_macros("(t/answer)", &mut macros).unwrap();
        assert_eq!(read_number(&mut runtime, &value), 42.0);
        runtime.collect().unwrap();
        let value = runtime.eval_with_macros("(t/ticks)", &mut macros).unwrap();
        assert_eq!(read_number(&mut runtime, &value), expected);
    }
    macros.define("(defmacro user-answer [] 41)").unwrap();
    for expected in [41.0, 43.0] {
        if expected == 43.0 {
            macros.define("(defmacro user-answer [] 43)").unwrap();
        }
        let value = runtime
            .eval_with_macros("(user-answer)", &mut macros)
            .unwrap();
        assert_eq!(read_number(&mut runtime, &value), expected);
    }
}
