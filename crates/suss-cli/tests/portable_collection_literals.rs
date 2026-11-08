use suss_cli::portable_session::Session;

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

fn boolean(session: &mut Session, source: &str) -> bool {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |mut store, value| {
            let bits = suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap();
            Ok(match bits {
                2 => false,
                4 => true,
                other => panic!("Boolean layout {other}"),
            })
        })
        .unwrap()
}

#[test]
fn vector_literal_normalizes_entries_once_before_constructor_invocation() {
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    // Original development-only constructor fixture for the compiler boundary.
    // This is not a production PersistentVector implementation or M4 acceptance.
    session.enter_namespace("suss.core").unwrap();
    session.eval("(deftype PersistentVector [meta cnt shift root tail __hash]) (set! (.-EMPTY_NODE PersistentVector) (js-obj))").unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def literal-order 0)").unwrap();
    session.eval("(def literal-result [(do (set! literal-order (+ (* literal-order 10) 1)) 17) (do (set! literal-order (+ (* literal-order 10) 2)) 19)])").unwrap();
    session.collect().unwrap();
    assert!(boolean(&mut session,"(and (= literal-order 12) (= (.-cnt literal-result) 2) (= (aget (.-tail literal-result) 0) 17) (= (aget (.-tail literal-result) 1) 19))"));
}

#[test]
fn collection_literals_match_independently_decoded_primary_values() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/collection-literal-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 19);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.enter_namespace("suss.core").unwrap();
    session.eval("(deftype PersistentVector []) (deftype PersistentArrayMap []) (deftype PersistentHashMap []) (deftype PersistentHashSet [])").unwrap();
    session.enter_namespace("user").unwrap();
    session
        .eval(include_str!(
            "../../../tests/oracle/collection-literal-fixture.cljc"
        ))
        .unwrap();
    for case in cases {
        let value = session
            .eval(case["source"].as_str().unwrap())
            .unwrap_or_else(|error| panic!("{}: {error:?}", case["id"]));
        let observed = session
            .inspect(&value, |mut store, value| {
                Ok(match case["expected"]["tag"].as_str().unwrap() {
                    "f64" => {
                        number_observation(
                        exact_number(&mut store, &value)?,
                        &case["expected"],
                        case["source"].as_str().unwrap(),
                    )
                    }
                    "bool" => {
                        let boolean = match suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap()
                        {
                            2 => false,
                            4 => true,
                            other => panic!("Boolean {other}"),
                        };
                        serde_json::json!({"tag":"bool","value":boolean})
                    }
                    tag => panic!("Unexpected corpus tag {tag}"),
                })
            })
            .unwrap();
        assert_eq!(observed, case["expected"], "{}", case["id"]);
        session.collect().unwrap();
    }
}

#[test]
fn large_vector_method_is_captured_before_entry_property_mutation() {
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("suss.core").unwrap();
    session.eval("(deftype PersistentVector [meta cnt shift root tail __hash]) (set! (.-fromArray PersistentVector) (fn [xs no-clone] (PersistentVector. nil (alength xs) 5 no-clone xs nil)))").unwrap();
    session.enter_namespace("user").unwrap();
    let entries = std::iter::once(
        "(do (set! (.-fromArray suss.core/PersistentVector) (fn [_ _] (throw 23))) 17)".to_owned(),
    )
    .chain((1..32).map(|i| i.to_string()))
    .collect::<Vec<_>>()
    .join(" ");
    session
        .eval(&format!("(def literal-large [{entries}])"))
        .unwrap();
    session.collect().unwrap();
    assert!(boolean(&mut session,"(and (= (.-cnt literal-large) 32) (true? (.-root literal-large)) (= (aget (.-tail literal-large) 0) 17) (= (aget (.-tail literal-large) 31) 31))"));
    use suss_cli::portable_session::SessionError;
    let Err(SessionError::Language(thrown)) = session.eval(&format!("[{entries}]")) else {
        panic!("next lookup uses changed method")
    };
    session
        .inspect(&thrown, |mut store, value| {
            assert_eq!(exact_number(&mut store, &value)?, num(23.0));
            Ok(())
        })
        .unwrap();
}

#[test]
fn small_vector_captures_constructor_and_empty_node_before_entry_redefinition() {
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("suss.core").unwrap();
    session.eval("(deftype PersistentVector [meta cnt shift root tail __hash]) (set! (.-EMPTY_NODE PersistentVector) 17)").unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def literal-original-vector suss.core/PersistentVector) (deftype LiteralReplacement [meta cnt shift root tail __hash]) (set! (.-EMPTY_NODE LiteralReplacement) 23)").unwrap();
    session.eval("(def literal-before [(do (set! suss.core/PersistentVector LiteralReplacement) (set! (.-EMPTY_NODE literal-original-vector) 19) 31)])").unwrap();
    session.collect().unwrap();
    assert!(boolean(
        &mut session,
        "(and (instance? literal-original-vector literal-before) (= (.-root literal-before) 17) (= (aget (.-tail literal-before) 0) 31))"
    ));
    session.eval("(def literal-after [37])").unwrap();
    session.collect().unwrap();
    assert!(boolean(
        &mut session,
        "(and (instance? LiteralReplacement literal-after) (= (.-root literal-after) 23) (= (aget (.-tail literal-after) 0) 37))"
    ));
}

#[test]
fn missing_literal_classes_fail_before_initializer_effects_or_publication() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session.enter_namespace("user").unwrap();
    session
        .eval("(def literal-effects 0) (def literal-owned 17)")
        .unwrap();
    let source = "(def literal-owned [(do (set! literal-effects 1) 19)])";
    let Err(SessionError::Compile(error)) = session.eval(source) else {
        panic!("missing constructor diagnostic")
    };
    assert_eq!(&source[error.span], "[(do (set! literal-effects 1) 19)]");
    assert!(error.message.contains("suss.core/PersistentVector"));
    session.collect().unwrap();
    assert!(boolean(
        &mut session,
        "(and (== literal-effects 0) (== literal-owned 17))"
    ));
}

#[test]
fn thrown_map_and_set_entries_stop_later_effects_and_construction() {
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("suss.core").unwrap();
    session.eval("(deftype PersistentHashMap []) (deftype PersistentHashSet []) (set! (.-fromArrays PersistentHashMap) (fn [_ _] (throw 99))) (set! (.-createAsIfByAssoc PersistentHashSet) (fn [_] (throw 99)))").unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def literal-effects 0)").unwrap();
    let mut entries = vec![
        "(do (set! literal-effects 1) 17) (do (set! literal-effects 12) 19)".to_owned(),
        "(do (set! literal-effects 123) 23) (throw 29)".to_owned(),
    ];
    entries.extend((2..9).map(|i| format!("(do (set! literal-effects 999) {i}) {i}")));
    assert!(boolean(
        &mut session,
        &format!(
            "(try {{{}}} (catch :default e (= e 29)))",
            entries.join(" ")
        )
    ));
    session.collect().unwrap();
    assert!(boolean(&mut session, "(= literal-effects 123)"));
    assert!(boolean(
        &mut session,
        "(try #{(throw 31) (do (set! literal-effects 999) 37)} (catch :default e (= e 31)))"
    ));
    session.collect().unwrap();
    assert!(boolean(&mut session, "(= literal-effects 123)"));
    assert!(boolean(&mut session, "(= (+ 17 19) 36)"));
}
