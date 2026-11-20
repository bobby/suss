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
                    || source.as_bytes().windows(3).any(|w| {
                        w[0].is_ascii_digit()
                            && (w[1] == b'.' || w[1] == b'e')
                            && (w[2].is_ascii_digit() || w[2] == b'-')
                    }))
        }
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
fn session() -> Session {
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session
}

#[test]
fn retained_apply_sequences_preserve_spread_and_all_list_star_arities() {
    let mut session = session();
    for source in ["(if (nil? (spread nil)) 1 0)", "(if (= (spread (list [1 2])) '(1 2)) 1 0)", "(if (= (spread (list 1 2 [3 4])) '(1 2 3 4)) 1 0)", "(if (= (list* [1 2]) '(1 2)) 1 0)", "(if (= (list* 1 [2 3]) '(1 2 3)) 1 0)", "(if (= (list* 1 2 [3 4]) '(1 2 3 4)) 1 0)", "(if (= (list* 1 2 3 [4 5]) '(1 2 3 4 5)) 1 0)", "(if (= (list* 1 2 3 4 5 6 [7 8]) '(1 2 3 4 5 6 7 8)) 1 0)"] {
        assert_eq!(eval_number(&mut session, source), num(1.0), "{source}");
    }
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "(count (list* 1 2 3 4 [5 6]))"), num(6.0));
}

#[test]
fn retained_bounded_count_keeps_counted_fast_path_and_limits_seq_effects() {
    let mut session = session();
    session.eval("(def steps 0) (deftype CountProbe [remaining] ISeqable (-seq [this] (if (> remaining 0) this nil)) ISeq (-first [_] remaining) (-rest [_] (do (set! steps (+ steps 1)) (CountProbe. (- remaining 1)))) INext (-next [_] (do (set! steps (+ steps 1)) (if (> remaining 1) (CountProbe. (- remaining 1)) nil))))").unwrap();
    assert_eq!(eval_number(&mut session, "(bounded-count 1 [1 2 3 4])"), num(4.0));
    assert_eq!(eval_number(&mut session, "(bounded-count 2 (CountProbe. 100))"), num(2.0));
    assert_eq!(eval_number(&mut session, "steps"), num(2.0));
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "(bounded-count 0 (CountProbe. 100))"), num(0.0));
    assert_eq!(eval_number(&mut session, "steps"), num(2.0));
    assert_eq!(eval_number(&mut session, "(bounded-count 10 (CountProbe. 3))"), num(3.0));
    assert_eq!(eval_number(&mut session, "steps"), num(5.0));
}

#[test]
fn private_definitions_retain_callable_qualified_access() {
    let mut session = session();
    session.enter_namespace("apply.private").unwrap();
    session.eval("(def ^:private hidden (fn [x] (+ x 1)))").unwrap();
    assert_eq!(eval_number(&mut session, "(hidden 41)"), num(42.0));
    session.enter_namespace("user").unwrap();
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "(apply.private/hidden 42)"), num(43.0));
}

#[test]
fn retained_apply_sequence_corpus_executes_all_primary_numeric_observations() {
    let mut session = session();
    session.enter_namespace("apply.private").unwrap();
    session.eval("(def ^:private hidden (fn [x] (+ x 1)))").unwrap();
    session.enter_namespace("user").unwrap();
    let corpus: serde_json::Value = serde_json::from_str(include_str!("../../../tests/oracle/apply-sequences-cases.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 14);
    for case in cases {
        session.collect().unwrap();
        let source = case["source"].as_str().unwrap().replace("private-defs/hidden", "apply.private/hidden");
        let actual = eval_number(&mut session, &source);
        assert!(matches_pinned(actual, &case["expected"], &source), "{}: {actual:?}", case["id"]);
    }
}
