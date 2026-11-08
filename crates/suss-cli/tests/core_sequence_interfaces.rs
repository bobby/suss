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

const CORE: &str = include_str!("../../../runtime/core-import/suss/core.sus");

#[test]
fn imported_sequence_interfaces_execute_direct_methods_after_gc() {
    let mut session = Session::new().unwrap();
    session.eval(CORE).unwrap();
    session.enter_namespace("user").unwrap();
    assert!(!eval_bool(
        &mut session,
        "(implements? cljs.core/ISeqable nil)"
    ));
    session.eval("(deftype CoreProtocolProbe [value] cljs.core/ISeqable (-seq [this] this) cljs.core/ISeq (-first [this] value) (-rest [this] nil) cljs.core/INext (-next [this] nil) cljs.core/ASeq cljs.core/ISequential cljs.core/IList cljs.core/ICounted (-count [this] 1) cljs.core/ICollection (-conj [this x] (CoreProtocolProbe. x)) cljs.core/IEmptyableCollection (-empty [this] nil) cljs.core/IIndexed (-nth [this n] value) (-nth [this n not-found] value) cljs.core/IMeta (-meta [this] value) cljs.core/IWithMeta (-with-meta [this m] (CoreProtocolProbe. m)) cljs.core/IEquiv (-equiv [this other] (identical? this other)) cljs.core/IHash (-hash [this] value) cljs.core/ICloneable (-clone [this] (CoreProtocolProbe. value))) (def interface-probe (CoreProtocolProbe. 17))").unwrap();
    session.collect().unwrap();
    for protocol in [
        "ISeqable",
        "ISeq",
        "INext",
        "ASeq",
        "ISequential",
        "IList",
        "ICounted",
        "ICollection",
        "IEmptyableCollection",
        "IIndexed",
        "IMeta",
        "IWithMeta",
        "IEquiv",
        "IHash",
        "ICloneable",
    ] {
        assert!(
            eval_bool(
                &mut session,
                &format!("(implements? cljs.core/{protocol} interface-probe)")
            ),
            "{protocol}"
        );
    }
    for (source, expected) in [
        ("(cljs.core/-first interface-probe)", 17.0f64),
        ("(cljs.core/-count interface-probe)", 1.0),
        ("(cljs.core/-nth interface-probe 0)", 17.0),
        ("(cljs.core/-nth interface-probe 0 99)", 17.0),
        ("(cljs.core/-meta interface-probe)", 17.0),
        ("(cljs.core/-hash interface-probe)", 17.0),
        (
            "(cljs.core/-first (cljs.core/-clone interface-probe))",
            17.0,
        ),
        (
            "(cljs.core/-first (cljs.core/-conj interface-probe 8))",
            8.0,
        ),
        (
            "(cljs.core/-meta (cljs.core/-with-meta interface-probe 9))",
            9.0,
        ),
    ] {
        assert_eq!(
            eval_number(&mut session, source),
            num(expected),
            "{source}"
        );
        session.collect().unwrap();
    }
    assert!(eval_bool(
        &mut session,
        "(identical? interface-probe (cljs.core/-seq interface-probe))"
    ));
    assert!(eval_bool(
        &mut session,
        "(cljs.core/-equiv interface-probe interface-probe)"
    ));
    assert!(!eval_bool(
        &mut session,
        "(cljs.core/-equiv interface-probe (CoreProtocolProbe. 17))"
    ));
    for method in ["-rest", "-next", "-empty"] {
        assert!(eval_bool(
            &mut session,
            &format!("(nil? (cljs.core/{method} interface-probe))")
        ));
    }
}

#[test]
fn imported_interfaces_match_independently_encoded_primary_adapter_observations() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/core-interface-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 65);
    let mut session = Session::new().unwrap();
    session.eval(CORE).unwrap();
    session.enter_namespace("user").unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        assert!(ids.insert(id));
        let source = case["source"].as_str().unwrap();
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "bool" => serde_json::json!({"tag":"bool", "value":eval_bool(&mut session, source)}),
            "f64" => {
                number_observation(eval_number(&mut session, source), &case["expected"], source)
            }
            tag => panic!("unexpected interface tag {tag}"),
        };
        assert_eq!(actual, case["expected"], "{id}: {source}");
        session.collect().unwrap();
    }
}

#[test]
fn canonical_core_aliases_and_retained_methods_survive_source_reload() {
    let mut session = Session::new().unwrap();
    session.eval(CORE).unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(deftype ReloadIndexed [value] cljs.core/IIndexed (-nth [this n] (+ value n)) (-nth [this n missing] (+ value n missing))) (def retained-indexed (ReloadIndexed. 10)) (def retained-nth cljs.core/-nth)").unwrap();
    session.collect().unwrap();
    session.eval(CORE).unwrap();
    session.enter_namespace("user").unwrap();
    session.collect().unwrap();
    for source in [
        "(suss.core/-nth retained-indexed 2 40)",
        "(cljs.core/-nth retained-indexed 2 40)",
        "(retained-nth retained-indexed 2 40)",
    ] {
        assert_eq!(
            eval_number(&mut session, source),
            num(52.0),
            "{source}"
        );
    }
    assert!(eval_bool(
        &mut session,
        "(implements? suss.core/IIndexed retained-indexed)"
    ));
    assert!(eval_bool(
        &mut session,
        "(implements? cljs.core/IIndexed retained-indexed)"
    ));
    for source in [
        "(cljs.core/-nth)",
        "(cljs.core/-nth retained-indexed)",
        "(cljs.core/-nth retained-indexed 0 1 2)",
    ] {
        assert!(
            session.eval(source).is_err(),
            "invalid method arity accepted: {source}"
        );
    }
    assert_eq!(
        eval_number(&mut session, "(retained-nth retained-indexed 3)"),
        num(13.0)
    );
}

#[test]
fn imported_operation_methods_survive_reload_gc_and_invalid_arity_recovery() {
    let mut session = Session::new().unwrap();
    session.eval(CORE).unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(deftype ReloadOperations [value] cljs.core/IReduce (-reduce [this f] (f value value)) (-reduce [this f start] (f start value)) cljs.core/IStack (-peek [this] value) (-pop [this] this) cljs.core/IDrop (-drop [this n] this) cljs.core/IIterable (-iterator [this] this) cljs.core/IReversible (-rseq [this] this)) (def reload-operation (ReloadOperations. 7)) (def saved-reduce cljs.core/-reduce)").unwrap();
    session.collect().unwrap();
    session.eval(CORE).unwrap();
    session.enter_namespace("user").unwrap();
    session.collect().unwrap();
    for source in [
        "(saved-reduce reload-operation (fn [a b] (+ a b)) 3)",
        "(cljs.core/-reduce reload-operation (fn [a b] (+ a b)) 3)",
        "(suss.core/-reduce reload-operation (fn [a b] (+ a b)) 3)",
    ] {
        assert_eq!(
            eval_number(&mut session, source),
            num(10.0),
            "{source}"
        );
    }
    for source in [
        "(cljs.core/-reduce)",
        "(cljs.core/-reduce reload-operation)",
        "(cljs.core/-reduce reload-operation (fn [a b] a) 1 2)",
        "(cljs.core/-peek)",
        "(cljs.core/-peek reload-operation 1)",
        "(cljs.core/-drop reload-operation)",
        "(cljs.core/-rseq reload-operation 1)",
        "(cljs.core/-iterator)",
    ] {
        let error = session.eval(source).unwrap_err();
        assert!(
            matches!(error, SessionError::Language(_)),
            "{source}: {error}"
        );
    }
    assert_eq!(
        eval_number(
            &mut session,
            "(saved-reduce reload-operation (fn [a b] (+ a b)))"
        ),
        num(14.0)
    );
}

#[test]
fn malformed_operation_declarations_and_extensions_preserve_loaded_core() {
    let mut session = Session::new().unwrap();
    session.eval(CORE).unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(deftype ReviewStable [value] cljs.core/IReduce (-reduce [this f] (f value 2)) (-reduce [this f start] (f start value)) cljs.core/IStack (-peek [this] value) (-pop [this] this)) (def review-stable (ReviewStable. 9)) (def review-saved-reduce cljs.core/-reduce)").unwrap();
    for source in [
        "(defprotocol ReviewBad (bad-reduce [this f] [this start]))",
        "(defprotocol ReviewBad (bad-reduce []))",
        "(defprotocol ReviewBad (bad-reduce [this & xs]))",
        "(extend-type ReviewStable cljs.core/IReduce (-reduce [this f start extra] 99))",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Compile(_))),
            "{source}"
        );
        session.collect().unwrap();
        assert_eq!(
            eval_number(
                &mut session,
                "(review-saved-reduce review-stable (fn [a b] (+ a b)))"
            ),
            num(11.0)
        );
        assert_eq!(
            eval_number(
                &mut session,
                "(review-saved-reduce review-stable (fn [a b] (+ a b)) 40)"
            ),
            num(49.0)
        );
        assert_eq!(
            eval_number(&mut session, "(cljs.core/-peek review-stable)"),
            num(9.0)
        );
    }
    assert!(matches!(
        session.eval("ReviewBad"),
        Err(SessionError::Compile(_))
    ));
    assert!(matches!(
        session.eval("bad-reduce"),
        Err(SessionError::Compile(_))
    ));
    session.eval("(defprotocol ReviewBad (bad-reduce [this])) (extend-type ReviewStable ReviewBad (bad-reduce [this] 19))").unwrap();
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(bad-reduce review-stable)"),
        num(19.0)
    );
}

#[test]
fn empty_list_literal_reads_canonical_core_class_property_after_gc() {
    let mut session = Session::new().unwrap();
    assert!(matches!(
        session.eval("(def premature-empty ())"),
        Err(SessionError::Compile(_))
    ));
    assert!(matches!(
        session.eval("premature-empty"),
        Err(SessionError::Compile(_))
    ));
    // Original adapter fixture only: concrete upstream list types are not yet loaded.
    session.enter_namespace("suss.core").unwrap();
    session
        .eval("(deftype List []) (deftype EmptyList [value]) (set! (.-EMPTY List) (EmptyList. 17))")
        .unwrap();
    session.enter_namespace("user").unwrap();
    session
        .eval("(def saved-empty ()) (def read-empty (fn [] ())) (def List 123)")
        .unwrap();
    session.collect().unwrap();
    assert!(eval_bool(&mut session, "(identical? saved-empty ())"));
    assert!(eval_bool(
        &mut session,
        "(identical? saved-empty (let [List 99] ()))"
    ));
    assert!(eval_bool(
        &mut session,
        "(instance? cljs.core/EmptyList ())"
    ));
    assert_eq!(eval_number(&mut session, "(if () 1 2)"), num(1.0));
    session
        .eval("(set! (.-EMPTY cljs.core/List) (cljs.core/EmptyList. 29))")
        .unwrap();
    session.collect().unwrap();
    assert!(!eval_bool(&mut session, "(identical? saved-empty ())"));
    assert!(eval_bool(&mut session, "(identical? () (read-empty))"));
    assert_eq!(
        eval_number(&mut session, "(.-value (read-empty))"),
        num(29.0)
    );
    session.eval("(set! (.-EMPTY cljs.core/List) nil)").unwrap();
    assert!(eval_bool(&mut session, "(nil? ())"));
    assert!(eval_bool(&mut session, "(nil? (read-empty))"));
}
