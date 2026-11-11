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

fn assert_language_error(session: &mut Session, source: &str, message: &str) {
    let Err(SessionError::Language(payload)) = session.eval(source) else {
        panic!("Expected language error for {source}");
    };
    session.collect().unwrap();
    let units = session
        .inspect(&payload, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            // A language error (ABI2 Error layout) or an ExceptionInfo object.
            let text_value = if fields.len() == 5 {
                fields[1]
            } else {
                assert_eq!(fields.len(), 4, "ABI2 Object layout");
                let data = fields[1]
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                assert_eq!(
                    data.len(&store)?,
                    3,
                    "ExceptionInfo message/data/cause fields"
                );
                data.get(&mut store, 0)?
            };
            let text = text_value
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            Ok(text
                .elems(&mut store)?
                .map(|v| v.unwrap_i32() as u16)
                .collect::<Vec<_>>())
        })
        .unwrap();
    assert_eq!(
        units,
        message.encode_utf16().collect::<Vec<_>>(),
        "{source}"
    );
}

#[test]
fn persistent_session_atoms_and_old_contents_remain_rooted_across_fragments_and_gc() {
    let mut session = Session::new_repl().unwrap();
    session.eval("(def a (atom (fn [] 17))) (def read-old (let [old a] (fn [] @old))) (def mutate-old (let [old a] (fn [x] (reset! old x))))").unwrap();
    let old = session.eval("@a").unwrap();
    let read_old = session.eval("read-old").unwrap();
    session.eval("(def a (atom (fn [] 23)))").unwrap();
    for _ in 0..32 {
        session.eval("(fn [x] x)").unwrap();
    }
    session.collect().unwrap();
    let old_result = session.invoke(&old, &[]).unwrap();
    assert_eq!(number(&mut session, &old_result), num(17.0));
    let contents = session.invoke(&read_old, &[]).unwrap();
    let result = session.invoke(&contents, &[]).unwrap();
    assert_eq!(number(&mut session, &result), num(17.0));
    session.eval("(mutate-old (fn [] 31))").unwrap();
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "((read-old))"), num(31.0));
    assert_eq!(eval_number(&mut session, "(@a)"), num(23.0));
    let result = session.invoke(&old, &[]).unwrap();
    assert_eq!(number(&mut session, &result), num(17.0));
}

#[test]
fn persistent_session_atom_operations_preserve_order_and_errors_before_mutation() {
    let mut session = Session::new_repl().unwrap();
    session.eval("(def a (atom 17)) (def trace 0)").unwrap();
    assert_eq!(
        eval_number(
            &mut session,
            "(swap! (do (set! trace (+ (* trace 10) 1)) a) (do (set! trace (+ (* trace 10) 2)) (fn [v x y] (+ v x y))) (do (set! trace (+ (* trace 10) 3)) 2) (do (set! trace (+ (* trace 10) 4)) 3))"
        ),
        num(22.0)
    );
    assert_eq!(eval_number(&mut session, "trace"), num(1234.0));
    let Err(SessionError::Language(thrown)) =
        session.eval("(swap! a (fn [x] (do (set! trace 9) (throw 31))))")
    else {
        panic!("Expected callback throw");
    };
    assert_eq!(number(&mut session, &thrown), num(31.0));
    assert_eq!(eval_number(&mut session, "@a"), num(22.0));
    assert_eq!(eval_number(&mut session, "trace"), num(9.0));
    // Validators and watches are implemented (#30); see suss-compile's
    // core_state tests. A rejected value leaves the state unchanged.
    session.eval("(set-validator! a (fn [v] (< v 50)))").unwrap();
    assert_language_error(&mut session, "(reset! a 99)", "Invalid reference state");
    assert_eq!(eval_number(&mut session, "@a"), num(22.0));
    session.eval("(set-validator! a nil)").unwrap();
    assert_language_error(&mut session, "(swap! a (fn [x] x) 1 2 3)", "Wrong number of args (4) passed to: fn");
    assert_language_error(&mut session, "(atom 1 2)", "No value supplied for key: 2");
    // Non-atom references go through IReset and ISwap, which nil lacks.
    assert_language_error(&mut session, "(reset! nil 99)", "Invalid nominal operation");
    assert_language_error(&mut session, "(swap! nil (fn [x] x))", "Invalid nominal operation");
    assert_eq!(eval_number(&mut session, "@a"), num(22.0));
}

#[test]
fn session_lifecycle_atom_core_is_reprovisioned_on_reset_and_failed_reset_preserves_state() {
    let mut session = Session::new_repl().unwrap();
    session.eval("(def a (atom 17))").unwrap();
    let old = session.eval("(fn [] @a)").unwrap();
    let before = session.stats();
    session.set_operation_fuel(1);
    assert!(matches!(session.reset(), Err(SessionError::Trap(_))));
    assert_eq!(session.stats(), before);
    session.set_operation_fuel(10_000_000);
    assert_eq!(eval_number(&mut session, "@a"), num(17.0));
    session.reset().unwrap();
    assert!(matches!(
        session.invoke(&old, &[]),
        Err(SessionError::ForeignValue)
    ));
    assert_eq!(session.stats().external_value_handles, 0);
    assert_eq!(session.stats().resident_fragments, 1);
    assert_eq!(eval_number(&mut session, "@(atom 23)"), num(23.0));
}

#[test]
fn persistent_session_atoms_match_independent_pinned_observations() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/atom-cases.json")).unwrap();
    let mut session = Session::new_repl().unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for case in corpus["cases"].as_array().unwrap() {
        assert!(ids.insert(case["id"].as_str().unwrap()));
        let source = case["source"].as_str().unwrap();
        let result = session
            .eval(source)
            .unwrap_or_else(|e| panic!("{source}: {e}"));
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "bool" => session
                .inspect(&result, |mut store, value| {
                    let sentinel = suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap();
                    let boolean = match sentinel {
                        2 => false,
                        4 => true,
                        other => panic!("Boolean {other}"),
                    };
                    Ok(serde_json::json!({"tag":"bool", "value":boolean}))
                })
                .unwrap(),
            "f64" => {
                number_observation(number(&mut session, &result), &case["expected"], source)
            }
            tag => panic!("unexpected tag {tag}"),
        };
        assert_eq!(actual, case["expected"], "{source}");
        session.collect().unwrap();
    }
    assert_eq!(ids.len(), 28);
}
