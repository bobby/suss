use suss_cli::portable_session::Session;

fn boolean(session: &mut Session, source: &str) -> bool {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |store, value| {
            let bits = value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32();
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
    use wasmtime::Val;
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
                        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                        let fields = object.fields(&mut store)?.collect::<Vec<_>>();
                        let [Val::F64(bits)] = fields.as_slice() else {
                            panic!("Number layout")
                        };
                        serde_json::json!({"tag":"f64","bits":format!("{bits:016x}")})
                    }
                    "bool" => {
                        let boolean = match value
                            .unwrap_anyref()
                            .unwrap()
                            .as_i31(&store)?
                            .unwrap()
                            .get_u32()
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
    use wasmtime::Val;
    let Err(SessionError::Language(thrown)) = session.eval(&format!("[{entries}]")) else {
        panic!("next lookup uses changed method")
    };
    session
        .inspect(&thrown, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Number layout")
            };
            assert_eq!(*bits, 23f64.to_bits());
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
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
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
        "(and (= literal-effects 0) (= literal-owned 17))"
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
