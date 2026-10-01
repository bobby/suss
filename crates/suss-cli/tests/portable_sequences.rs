use suss_cli::portable_session::{Session, SessionValue};
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
fn eval_number(session: &mut Session, source: &str) -> u64 {
    let value = session.eval(source).unwrap();
    number(session, &value)
}
fn eval_bool(session: &mut Session, source: &str) -> bool {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |store, value| {
            match value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32()
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
                serde_json::json!({"tag":"f64", "bits":format!("{:016x}",eval_number(&mut session, source))})
            }
            tag => panic!("unexpected sequence observation tag {tag}"),
        };
        assert_eq!(actual, case["expected"], "{id}: {source}");
        session.collect().unwrap();
    }
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
        ("(count retained-tail)", 1.0f64),
        ("(count retained-head)", 2.0),
        ("(first retained-head)", 1.0),
        ("(first (next retained-head))", 2.0),
        ("(count retained-view)", 2.0),
        ("(-nth retained-view 1)", 4.0),
        ("(-nth retained-view 9 77)", 77.0),
        ("(-meta (-with-meta retained-head 17))", 17.0),
        ("(-meta (-with-meta () 19))", 19.0),
        ("(first (-conj retained-tail 29))", 29.0),
        ("(first (cons 8 \"AB\"))", 8.0),
    ] {
        assert_eq!(
            eval_number(&mut session, source),
            expected.to_bits(),
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
        31.0f64.to_bits()
    );
    assert_eq!(
        eval_number(&mut session, "(count retained-view)"),
        3.0f64.to_bits()
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
                assert_eq!(fields.len(), 4, "ABI Error layout");
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
            17.0f64.to_bits()
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
        3.0f64.to_bits()
    );
    assert_eq!(
        eval_number(&mut session, "((first (next (next saved-rest))))"),
        29.0f64.to_bits()
    );
    assert!(matches!(
        session.eval("(variadic-floor (do (set! variadic-trace 1) 7))"),
        Err(SessionError::Language(_))
    ));
    assert_eq!(
        eval_number(&mut session, "variadic-trace"),
        1.0f64.to_bits()
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
        2.0f64.to_bits()
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
            17.0f64.to_bits()
        );
        session
            .eval("(set! cljs.core/IndexedSeq saved-class)")
            .unwrap();
        assert_eq!(
            eval_number(&mut session, "(first (receiver 29))"),
            29.0f64.to_bits()
        );
    }
}
