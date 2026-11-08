use suss_cli::portable_session::{Session, SessionValue};

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
                    // The f64 primitives return doubles for non-long operands.
                    || source.contains("suss.bootstrap/f64-")
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
            match suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap()
            {
                2 => Ok(false),
                4 => Ok(true),
                other => panic!("Boolean {other}"),
            }
        })
        .unwrap()
}

#[test]
fn hash_numeric_boundaries_match_independently_encoded_primary_observations() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/hash-numeric-boundaries-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 75);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    let mut identities = std::collections::BTreeSet::new();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        assert!(identities.insert(id));
        let source = case["source"].as_str().unwrap();
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "bool" => serde_json::json!({"tag":"bool", "value":eval_bool(&mut session, source)}),
            "f64" => {
                let actual = eval_number(&mut session, source);
                // Clojure governs: a remainder of longs is a long, never the
                // pinned ClojureScript negative zero.
                let long_zero = [
                    "rem-59",
                    "review-rem-negative-divisor-zero",
                    "review-rem-negative-unit",
                ];
                if long_zero.contains(&id) {
                    assert_eq!(actual, Number::Long(0), "{id}: {source}");
                    case["expected"].clone()
                } else {
                    number_observation(actual, &case["expected"], source)
                }
            }
            tag => panic!("unexpected numeric hashing observation tag {tag}"),
        };
        assert_eq!(actual, case["expected"], "{id}: {source}");
        session.collect().unwrap();
    }
}

#[test]
fn numeric_hash_boundaries_keep_effects_typed_errors_and_compile_atomicity() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session.eval("(def boundary-effects 0)").unwrap();
    for source in [
        "(suss.bootstrap/safe-integer-remainder (array 1) (do (set! boundary-effects 17) 3))",
        "(suss.bootstrap/safe-integer-remainder 1.5 3)",
        "(suss.bootstrap/safe-integer-remainder 9007199254740992 3)",
        "(suss.bootstrap/safe-integer-remainder ##NaN 3)",
        "(suss.bootstrap/safe-integer-remainder 3 ##Inf)",
        "(suss.bootstrap/safe-integer-remainder 3 -0.0)",
        "(suss.bootstrap/f64-floor (fn [] 1))",
        "(suss.bootstrap/f64-finite (array 1))",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Language(_))),
            "{source}"
        );
        session.collect().unwrap();
        assert_eq!(
            eval_number(&mut session, "boundary-effects"),
            num(17.0)
        );
        assert_eq!(
            eval_number(
                &mut session,
                "(suss.bootstrap/safe-integer-remainder -2147483647 2147483647)"
            ),
            // Long operands give a long, as Clojure's rem: no negative zero.
            num(0.0)
        );
    }
    assert_eq!(eval_number(&mut session, "(let [a (array 0)] (suss.bootstrap/safe-integer-remainder (do (aset a 0 17) 19) (do (aset a 0 (+ (aget a 0) 1)) 3)) (aget a 0))"), num(18.0));
    for (name, arity) in [
        ("f64-floor", 1),
        ("f64-finite", 1),
        ("f64-safe-integer", 1),
        ("safe-integer-remainder", 2),
    ] {
        for args in ["", "1 2 3"] {
            let source = format!("(def boundary-ghost 7) (suss.bootstrap/{name} {args})");
            let Err(SessionError::Compile(error)) = session.eval(&source) else {
                panic!("arity {name}/{arity}");
            };
            assert!(error.span.end > error.span.start);
            assert!(matches!(
                session.eval("boundary-ghost"),
                Err(SessionError::Compile(_))
            ));
        }
        assert!(matches!(
            session.eval(&format!("suss.bootstrap/{name}")),
            Err(SessionError::Compile(_))
        ));
    }
}

#[test]
fn numeric_hash_adapters_validate_in_both_compilation_phases() {
    use suss_compile::portable::{
        prepare_fragment,
        resolve::{Environment, Phase},
    };
    for phase in [Phase::Runtime, Phase::Macro] {
        for source in [
            "(suss.bootstrap/f64-floor -1.75)",
            "(suss.bootstrap/f64-finite ##NaN)",
            "(suss.bootstrap/f64-safe-integer 9007199254740991)",
            "(suss.bootstrap/safe-integer-remainder -9007199254740991 2147483647)",
        ] {
            let fragment = prepare_fragment(source, &Environment::default(), phase).unwrap();
            suss_compile::runtime_abi::verify_artifact(
                &fragment.wasm,
                &suss_compile::runtime_abi::Manifest::default(),
            )
            .unwrap();
        }
        for source in [
            "(suss.bootstrap/f64-floor)",
            "(suss.bootstrap/f64-finite 1 2)",
            "(suss.bootstrap/f64-safe-integer)",
            "(suss.bootstrap/safe-integer-remainder 1)",
        ] {
            let Err(error) = prepare_fragment(source, &Environment::default(), phase) else {
                panic!("invalid private adapter arity");
            };
            assert!(error.span.end > error.span.start);
        }
    }
}

#[test]
fn numeric_hash_remainder_checks_both_operands_after_source_evaluation() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session.eval("(def review-effects 0)").unwrap();
    for source in [
        "(suss.bootstrap/safe-integer-remainder 17 1.5)",
        "(suss.bootstrap/safe-integer-remainder 17 9007199254740992)",
        "(suss.bootstrap/safe-integer-remainder 17 ##NaN)",
        "(suss.bootstrap/safe-integer-remainder 17 (array 1))",
        "(suss.bootstrap/safe-integer-remainder 17 0)",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Language(_))),
            "{source}"
        );
        session.collect().unwrap();
        assert_eq!(
            eval_number(
                &mut session,
                "(suss.bootstrap/safe-integer-remainder -17 -17)"
            ),
            // Long operands give a long, as Clojure's rem: no negative zero.
            num(0.0)
        );
    }
    assert!(matches!(session.eval("(suss.bootstrap/safe-integer-remainder (do (set! review-effects 1) 1.5) (do (set! review-effects (+ review-effects 10)) 3))"), Err(SessionError::Language(_))));
    assert_eq!(
        eval_number(&mut session, "review-effects"),
        num(11.0)
    );
    session.eval("(set! review-effects 0)").unwrap();
    assert!(matches!(
        session.eval(
            "(suss.bootstrap/safe-integer-remainder (throw 17) (do (set! review-effects 99) 3))"
        ),
        Err(SessionError::Language(_))
    ));
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "review-effects"),
        num(0.0)
    );
}
