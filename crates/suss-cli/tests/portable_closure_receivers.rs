use suss_cli::portable_session::{Session, SessionError};

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
fn number(session: &mut Session, source: &str) -> Number {
    let value = session.eval(source).unwrap_or_else(|error| {
        panic!(
            "{source}: {}",
            suss_cli::portable_repl::error_display(session, &error)
        )
    });
    session.collect().unwrap();
    session
        .inspect(&value, |mut store, value| {
            exact_number(&mut store, &value)
        })
        .unwrap()
}

#[test]
fn ordinary_call_and_apply_bind_the_requested_receiver_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def receiver-probe (fn [] (this-as receiver receiver))) (def receiver-owner (array 42))").unwrap();
        session.collect().unwrap();
        for source in [
            "(if (identical? receiver-owner (.call receiver-probe receiver-owner)) 1 0)",
            "(if (identical? receiver-owner (.apply receiver-probe receiver-owner (array))) 1 0)",
            "(if (false? (.call receiver-probe false)) 1 0)",
            "(if (nil? (.apply receiver-probe nil (array))) 1 0)",
        ] {
            assert_eq!(number(&mut session, source), 1.0, "{source}");
        }
    }
}

#[test]
fn ordinary_receivers_restore_after_nested_calls_throws_and_invalid_apply() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval(
                r#"
          (def receiver-inner (fn [] (this-as receiver receiver)))
          (def receiver-throw (fn [] (throw 73)))
          (def receiver-outer
            (fn [other]
              (this-as before
                (let [inner (.call receiver-inner other)]
                  (and (identical? inner other)
                       (this-as after (identical? before after)))))))
          (def receiver-catch
            (fn [other]
              (this-as before
                (try (.apply receiver-throw other (array))
                  (catch :default e
                    (and (= e 73)
                         (this-as after (identical? before after))))))))
          (def receiver-invalid-apply
            (fn [other]
              (this-as before
                (try (do (.apply receiver-inner other 42) false)
                  (catch :default e
                    (this-as after (identical? before after)))))))
          (def receiver-owner (array 42)) (def receiver-other (array 99))
        "#,
            )
            .unwrap();
        session.collect().unwrap();
        for source in [
            "(if (.call receiver-outer receiver-owner receiver-other) 1 0)",
            "(if (.apply receiver-catch receiver-owner (array receiver-other)) 1 0)",
            "(if (.call receiver-invalid-apply receiver-owner receiver-other) 1 0)",
        ] {
            assert_eq!(number(&mut session, source), 1.0, "{source}");
        }
    }
}

#[test]
fn receiver_adapters_keep_ordinary_closure_arity_strict() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def receiver-one-arg (fn [x] x))").unwrap();
        session.collect().unwrap();
        for source in [
            "(.call receiver-one-arg nil)",
            "(.call receiver-one-arg nil 1 2)",
            "(.apply receiver-one-arg nil (array))",
            "(.apply receiver-one-arg nil (array 1 2))",
        ] {
            assert!(
                matches!(session.eval(source), Err(SessionError::Language(_))),
                "{source}"
            );
        }
        assert_eq!(
            number(&mut session, "(.call receiver-one-arg nil 17)"),
            17.0
        );
    }
}
