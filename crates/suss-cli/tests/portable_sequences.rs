use suss_cli::portable_session::{RuntimeNumber, Session, SessionValue};

#[path = "support/clojure_corrections.rs"]
mod clojure_corrections;

/// Recorded observations that coerced a non-number operand; Clojure rejects them.
const CORRECTIONS: &[clojure_corrections::Correction] = &[
    ("equal-string-value", "", "+ requires number operands"),
];

fn number(session: &mut Session, value: &SessionValue) -> RuntimeNumber {
    session
        .inspect(value, |store, value| {
            Ok(suss_cli::portable_session::runtime_number(store, &value)?.expect("Number layout"))
        })
        .unwrap()
}
fn eval_number(session: &mut Session, source: &str) -> RuntimeNumber {
    let value = session.eval(source).unwrap();
    number(session, &value)
}

/// The observation of a runtime number, distinguishing a long from a double.
fn number_json(number: RuntimeNumber) -> serde_json::Value {
    match number {
        RuntimeNumber::Long(value) => serde_json::json!({"tag":"long", "value":value}),
        RuntimeNumber::Double(value) => {
            serde_json::json!({"tag":"f64", "bits":format!("{:016x}", value.to_bits())})
        }
    }
}
/// The ClojureScript oracle encodes every number as a double; JVM Clojure,
/// Suss's oracle, gives a long for these integral results.
fn clojure_number(expected: &serde_json::Value) -> serde_json::Value {
    if expected["tag"] == "f64" {
        let bits = u64::from_str_radix(expected["bits"].as_str().unwrap(), 16).unwrap();
        let value = f64::from_bits(bits);
        if value.is_finite() && value.fract() == 0.0 && bits != (-0.0f64).to_bits() {
            return serde_json::json!({"tag":"long", "value":value as i64});
        }
    }
    expected.clone()
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
fn sequence_foundations_match_independently_encoded_primary_observations() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/sequence-cases.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 137);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    let mut identities = std::collections::BTreeSet::new();
    let mut corrected = 0;
    for case in cases {
        let case = &clojure_corrections::corrected(case, CORRECTIONS, &mut corrected);
        let id = case["id"].as_str().unwrap();
        assert!(identities.insert(id.to_owned()));
        let source = case["source"].as_str().unwrap();
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "bool" => serde_json::json!({"tag":"bool", "value":eval_bool(&mut session, source)}),
            "f64" => {
                number_json(eval_number(&mut session, source))
            }
            tag => panic!("unexpected sequence observation tag {tag}"),
        };
        assert_eq!(actual, clojure_number(&case["expected"]), "{id}: {source}");
        session.collect().unwrap();
    }
    assert_eq!(corrected, CORRECTIONS.len());
}

#[test]
fn retained_concrete_sequence_types_execute_fields_tails_metadata_and_gc() {
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def retained-tail (cons 2 nil)) (def retained-head (cons 1 retained-tail)) (def retained-array (array 3 4)) (def retained-view (seq retained-array))").unwrap();
    session.collect().unwrap();
    for source in [
        "(instance? cljs.core/EmptyList ())",
        "(instance? cljs.core/List retained-tail)",
        "(instance? cljs.core/Cons retained-head)",
        "(instance? cljs.core/IndexedSeq retained-view)",
        "(identical? retained-tail (rest retained-head))",
        "(identical? retained-tail (next retained-head))",
        "(identical? () (rest retained-tail))",
        "(nil? (next retained-tail))",
        "(nil? (seq (rest retained-tail)))",
        "(identical? retained-view (seq retained-view))",
        "(identical? retained-tail (-with-meta retained-tail nil))",
        "(identical? retained-head (-with-meta retained-head nil))",
        "(identical? retained-view (-with-meta retained-view nil))",
        "(identical? () (rest (-with-meta () 17)))",
        "(identical? retained-head (seq retained-head))",
        "(identical? \"\\ud83d\" (first \"😀\"))",
        "(identical? \"\\ude00\" (first (next \"😀\")))",
    ] {
        assert!(eval_bool(&mut session, source), "{source}");
        session.collect().unwrap();
    }
    for (source, expected) in [
        ("(count retained-tail)", 1i64),
        ("(count retained-head)", 2),
        ("(first retained-head)", 1),
        ("(first (next retained-head))", 2),
        ("(count retained-view)", 2),
        ("(-nth retained-view 1)", 4),
        ("(-nth retained-view 9 77)", 77),
        ("(-meta (-with-meta retained-head 17))", 17),
        ("(-meta (-with-meta () 19))", 19),
        ("(first (-conj retained-tail 29))", 29),
        ("(first (cons 8 \"AB\"))", 8),
    ] {
        assert_eq!(
            eval_number(&mut session, source),
            RuntimeNumber::Long(expected),
            "{source}"
        );
        session.collect().unwrap();
    }
    session
        .eval("(aset retained-array 0 31) (aset retained-array 2 41)")
        .unwrap();
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(first retained-view)"),
        RuntimeNumber::Long(31)
    );
    assert_eq!(
        eval_number(&mut session, "(count retained-view)"),
        RuntimeNumber::Long(3)
    );
    assert!(eval_bool(&mut session, "(nil? (seq (array)))"));
    assert!(eval_bool(&mut session, "(nil? (seq \"\"))"));
    assert!(eval_bool(&mut session, "(false? (first (cons false nil)))"));
    assert!(eval_bool(&mut session, "(nil? (first (cons nil nil)))"));
}

#[test]
fn retained_source_errors_are_typed_and_recover_after_gc() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    for (source, message) in [
        ("(-pop ())", "Can't pop empty list"),
        ("(-nth (seq (array 1)) 9)", "Index out of bounds"),
    ] {
        let Err(SessionError::Language(value)) = session.eval(source) else {
            panic!("expected language exception: {source}");
        };
        session.collect().unwrap();
        session.enter_namespace("user").unwrap();
        // Decode the thrown ABI Error message directly, independently of printing/equality.
        let value = session
            .inspect(&value, |mut store, value| {
                let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let fields = object.fields(&mut store)?.collect::<Vec<_>>();
                assert_eq!(fields.len(), 5, "ABI Error layout");
                let field = fields[1];
                let units = field.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                Ok(units
                    .elems(&mut store)?
                    .map(|v| v.unwrap_i32() as u16)
                    .collect::<Vec<_>>())
            })
            .unwrap();
        assert_eq!(value, message.encode_utf16().collect::<Vec<_>>());
        assert_eq!(
            eval_number(&mut session, "(first (cons 17 nil))"),
            RuntimeNumber::Long(17)
        );
    }
    assert!(eval_bool(
        &mut session,
        "(identical? \"19\" (ex-message (suss.bootstrap/error 19)))"
    ));
}

#[test]
fn variadic_rest_values_survive_gc_and_arity_failures_preserve_effects() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def saved-rest ((fn [a & xs] xs) 17 nil false (fn [] 29))) (def variadic-floor (fn [a b & xs] (count xs))) (def variadic-trace 0)").unwrap();
    session.collect().unwrap();
    assert!(eval_bool(
        &mut session,
        "(instance? cljs.core/IndexedSeq saved-rest)"
    ));
    assert!(eval_bool(&mut session, "(nil? (first saved-rest))"));
    assert!(eval_bool(
        &mut session,
        "(false? (first (next saved-rest)))"
    ));
    assert_eq!(
        eval_number(&mut session, "(count saved-rest)"),
        RuntimeNumber::Long(3)
    );
    assert_eq!(
        eval_number(&mut session, "((first (next (next saved-rest))))"),
        RuntimeNumber::Long(29)
    );
    assert!(matches!(
        session.eval("(variadic-floor (do (set! variadic-trace 1) 7))"),
        Err(SessionError::Language(_))
    ));
    assert_eq!(
        eval_number(&mut session, "variadic-trace"),
        RuntimeNumber::Long(1)
    );
    for signature in [
        "(fn [a &] 1)",
        "(fn [a & xs b] 1)",
        "(fn ([& xs] 1) ([a & xs] 2))",
        "(fn ([a b] 1) ([a & xs] 2))",
    ] {
        assert!(
            matches!(
                session.eval(&format!("(def invalid-variadic {signature})")),
                Err(SessionError::Compile(_))
            ),
            "{signature}"
        );
        assert!(matches!(
            session.eval("invalid-variadic"),
            Err(SessionError::Compile(_))
        ));
    }
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(variadic-floor 1 2 3 4)"),
        RuntimeNumber::Long(2)
    );
}

#[test]
fn variadic_live_class_failures_are_typed_and_recover_after_gc() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def saved-class cljs.core/IndexedSeq) (def receiver (fn [& xs] xs)) (def argument-effect 0)").unwrap();
    for value in ["nil", "false", "7", "(fn [& xs] xs)"] {
        session
            .eval(&format!("(set! cljs.core/IndexedSeq {value})"))
            .unwrap();
        assert!(eval_bool(&mut session, "(nil? (receiver))"));
        assert!(
            matches!(
                session.eval("(receiver (do (set! argument-effect 17) 9))"),
                Err(SessionError::Language(_))
            ),
            "{value}"
        );
        session.collect().unwrap();
        assert_eq!(
            eval_number(&mut session, "argument-effect"),
            RuntimeNumber::Long(17)
        );
        session
            .eval("(set! cljs.core/IndexedSeq saved-class)")
            .unwrap();
        assert_eq!(
            eval_number(&mut session, "(first (receiver 29))"),
            RuntimeNumber::Long(29)
        );
    }
}

#[test]
fn retained_equality_survives_gc_and_wrong_arity_keeps_operand_effects() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def retained-equality =) (def equality-left (list (list 1 2) nil false)) (def equality-right (cons (seq (array 1 2)) (list nil false))) (def equality-arity-trace 0)").unwrap();
    session.collect().unwrap();
    assert!(eval_bool(
        &mut session,
        "(retained-equality equality-left equality-right)"
    ));
    for (source, trace) in [
        ("(retained-equality)", 0i64),
        ("(equiv-sequential (do (set! equality-arity-trace 17) equality-left))", 17),
        ("(equiv-sequential equality-left equality-right (do (set! equality-arity-trace 29) nil))", 29),
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Language(_))),
            "{source}"
        );
        session.collect().unwrap();
        assert_eq!(
            eval_number(&mut session, "equality-arity-trace"),
            RuntimeNumber::Long(trace),
            "{source}"
        );
        assert!(eval_bool(
            &mut session,
            "(retained-equality equality-left equality-right)"
        ));
    }
    assert_eq!(
        eval_number(&mut session, "equality-arity-trace"),
        RuntimeNumber::Long(29)
    );
    session.eval("(deftype ThrowEquiv [] IEquiv (-equiv [_ other] (do (set! equality-arity-trace 41) (throw 73))))").unwrap();
    let Err(SessionError::Language(value)) = session.eval("(= (list (ThrowEquiv.)) (list nil))")
    else {
        panic!("typed equality throw");
    };
    session.collect().unwrap();
    assert_eq!(number(&mut session, &value), RuntimeNumber::Long(73));
    assert_eq!(
        eval_number(&mut session, "equality-arity-trace"),
        RuntimeNumber::Long(41)
    );
    assert!(eval_bool(
        &mut session,
        "(retained-equality equality-left equality-right)"
    ));
}
