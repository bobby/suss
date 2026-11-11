use suss_cli::portable_session::{RuntimeNumber, Session, SessionError, SessionValue};
use suss_compile::portable_session::sentinel_tag;

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
            match sentinel_tag(&mut store, &value)?.unwrap()
            {
                2 => Ok(false),
                4 => Ok(true),
                other => panic!("Boolean {other}"),
            }
        })
        .unwrap()
}

#[test]
fn native_protocols_dispatch_nil_undefined_and_scalar_kinds() {
    let mut session = Session::new().unwrap();
    session.eval("(defprotocol P (read [x]))").unwrap();
    for (kind, source, expected) in [
        ("nil", "nil", 10i64),
        ("number", "0", 20),
        ("boolean", "false", 30),
        ("string", "\"\"", 40),
        ("function", "(fn [] 1)", 50),
        ("object", "(ex-info \"m\" 7)", 60),
    ] {
        session
            .eval(&format!("(extend-type {kind} P (read [x] {expected}))"))
            .unwrap();
        assert_eq!(
            eval_number(&mut session, &format!("(read {source})")),
            RuntimeNumber::Long(expected)
        );
        assert!(eval_bool(&mut session, &format!("(satisfies? P {source})")));
    }
    assert_eq!(
        eval_number(&mut session, "(read (ex-data (new ExceptionInfo \"m\")))"),
        RuntimeNumber::Long(10)
    );
}

#[test]
fn native_protocols_choose_direct_then_specific_then_default_methods() {
    let mut session = Session::new().unwrap();
    session.eval("(defprotocol P (read [x])) (deftype Item [] P (read [x] 1)) (deftype Bare []) (def item (Item.)) (def bare (Bare.)) (extend-type default P (read [x] 9))").unwrap();
    assert_eq!(eval_number(&mut session, "(read item)"), RuntimeNumber::Long(1));
    assert_eq!(eval_number(&mut session, "(read bare)"), RuntimeNumber::Long(9));
    assert_eq!(eval_number(&mut session, "(read 7)"), RuntimeNumber::Long(9));
    session
        .eval("(extend-type object P (read [x] 8)) (extend-type number P (read [x] 2))")
        .unwrap();
    assert_eq!(eval_number(&mut session, "(read item)"), RuntimeNumber::Long(1));
    assert_eq!(eval_number(&mut session, "(read bare)"), RuntimeNumber::Long(8));
    assert_eq!(eval_number(&mut session, "(read 7)"), RuntimeNumber::Long(2));
    assert!(eval_bool(&mut session, "(satisfies? P false)"));
}

#[test]
fn native_protocol_extensions_remain_live_for_old_dispatchers_after_gc() {
    let mut session = Session::new().unwrap();
    session.eval("(defprotocol P (read [x] [x y])) (extend-type number P (read ([x] 1) ([x y] (+ x y))))").unwrap();
    let reader = session.eval("read").unwrap();
    let x = session.eval("7").unwrap();
    let y = session.eval("2").unwrap();
    session.collect().unwrap();
    let value = session.invoke(&reader, &[&x, &y]).unwrap();
    assert_eq!(number(&mut session, &value), RuntimeNumber::Long(9));
    session
        .eval("(extend-type number P (read ([x] 3) ([x y] 4)))")
        .unwrap();
    session.collect().unwrap();
    let value = session.invoke(&reader, &[&x]).unwrap();
    assert_eq!(number(&mut session, &value), RuntimeNumber::Long(3));
    let value = session.invoke(&reader, &[&x, &y]).unwrap();
    assert_eq!(number(&mut session, &value), RuntimeNumber::Long(4));
}

#[test]
fn native_protocol_membership_uses_marker_and_replaced_protocol_value() {
    let mut session = Session::new().unwrap();
    session.eval("(defprotocol P) (extend-type nil P)").unwrap();
    assert!(eval_bool(&mut session, "(satisfies? P nil)"));
    assert!(!eval_bool(&mut session, "(satisfies? P false)"));
    session
        .eval("(defprotocol Q) (extend-type number Q) (def P Q)")
        .unwrap();
    assert!(eval_bool(&mut session, "(satisfies? P 7)"));
    // Native membership reads P's current value; an old P nil marker does not
    // satisfy the replacement Q's native table.
    assert!(!eval_bool(&mut session, "(satisfies? P nil)"));
}

#[test]
fn missing_native_method_preserves_argument_effects_and_language_recovery() {
    let mut session = Session::new().unwrap();
    session
        .eval(
            "(defprotocol P (read [x y])) (def count 0) (def next (fn [] (def count (+ count 1))))",
        )
        .unwrap();
    assert!(matches!(
        session.eval("(read (next) (next))"),
        Err(SessionError::Language(_))
    ));
    assert_eq!(eval_number(&mut session, "count"), RuntimeNumber::Long(2));
    session
        .eval("(extend-type number P (read [x y] (+ x y)))")
        .unwrap();
    assert_eq!(eval_number(&mut session, "(read 3 4)"), RuntimeNumber::Long(7));
}

/// Native tables belong to the method wrapper its defprotocol created, not to
/// the public var's current value (#279). As with Clojure protocol fns, which
/// dispatch through their protocol, a captured method keeps dispatching after
/// the var is redefined; the pinned ClojureScript observation errored instead.
#[test]
fn captured_native_dispatcher_reads_current_method_table_after_replacement() {
    let mut session = Session::new().unwrap();
    session.eval("(defprotocol P (read [x])) (extend-type number P (read [x] 1)) (def old read) (def read (fn [x] 9))").unwrap();
    assert_eq!(eval_number(&mut session, "(old 7)"), RuntimeNumber::Long(1));
    assert_eq!(eval_number(&mut session, "(read 7)"), RuntimeNumber::Long(9));
    session
        .eval("(defprotocol P (read [x])) (extend-type number P (read [x] 2))")
        .unwrap();
    assert_eq!(eval_number(&mut session, "(old 7)"), RuntimeNumber::Long(2));
}

#[test]
fn native_protocol_redeclaration_resets_tables_but_direct_object_methods_survive() {
    let mut session = Session::new().unwrap();
    session.eval("(defprotocol P (read [x])) (deftype Item [] P (read [x] 3)) (def item (Item.)) (extend-type number P (read [x] 1)) (def old read) (defprotocol P (read [x]))").unwrap();
    assert!(!eval_bool(&mut session, "(satisfies? P 7)"));
    assert!(eval_bool(&mut session, "(satisfies? P item)"));
    assert!(matches!(
        session.eval("(old 7)"),
        Err(SessionError::Language(_))
    ));
    assert_eq!(eval_number(&mut session, "(old item)"), RuntimeNumber::Long(3));
}

#[test]
fn native_extension_replaces_all_arities_and_recur_changes_receiver() {
    let mut session = Session::new().unwrap();
    session.eval("(defprotocol P (read [x] [x y])) (extend-type number P (read ([x] 1) ([x y] 2))) (def old read) (extend-type number P (read [x] (if (nil? x) 3 (recur nil))))").unwrap();
    assert_eq!(eval_number(&mut session, "(old 7)"), RuntimeNumber::Long(3));
    // The replacement has only one arity: Clojure's arity error, not a
    // dropped operand (ADR-0004), and likewise no undefined parameter.
    assert!(matches!(session.eval("(old 7 8)"), Err(SessionError::Language(_))));
    session
        .eval("(extend-type number P (read [x y] (undefined? y)))")
        .unwrap();
    assert!(matches!(session.eval("(old 7)"), Err(SessionError::Language(_))));
    assert!(eval_bool(&mut session, "(old 7 8)") == false);
}

#[test]
fn native_protocols_match_independently_encoded_primary_corpus() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/native-protocol-cases.json"
    ))
    .unwrap();
    let mut session = Session::new().unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for case in corpus["cases"].as_array().unwrap() {
        assert!(ids.insert(case["id"].as_str().unwrap()));
        let source = case["source"].as_str().unwrap();
        if case["id"] == "captured-live-method-table" {
            // The pinned ClojureScript dispatcher read the redefined public
            // var and threw; Suss keeps the protocol's own table (#279).
            assert_eq!(eval_number(&mut session, source), RuntimeNumber::Long(1));
            continue;
        }
        // The pinned ClojureScript call convention dropped surplus operands
        // and filled missing ones with undefined. Under ADR-0004 a native
        // implementation is called with exactly its operands, so both throw
        // Clojure's arity error (#353).
        if case["id"] == "native-arity-replacement" {
            assert!(eval_bool(&mut session, source), "{source}");
            continue;
        }
        if case["id"] == "native-missing-parameter" {
            let message = session
                .eval(&format!("(try {source} (catch :default e (ex-message e)))"))
                .unwrap();
            let units = session
                .inspect(&message, |mut store, value| {
                    let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                    Ok(array.elems(&mut store)?.map(|unit| unit.unwrap_i32() as u16).collect::<Vec<_>>())
                })
                .unwrap();
            let message = String::from_utf16(&units).unwrap();
            assert!(message.starts_with("Wrong number of args (1) passed to: "), "{message}");
            continue;
        }
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "bool" => serde_json::json!({"tag":"bool", "value":eval_bool(&mut session, source)}),
            "f64" => {
                number_json(eval_number(&mut session, source))
            }
            tag => panic!("unexpected tag {tag}"),
        };
        assert_eq!(actual, clojure_number(&case["expected"]), "{source}");
        session.collect().unwrap();
    }
    assert_eq!(ids.len(), 29);
}

#[test]
fn native_satisfies_function_is_first_class_and_macro_fallback_observes_redefinition() {
    let mut session = Session::new().unwrap();
    session
        .eval("(defprotocol P) (extend-type number P) (deftype Item [] P)")
        .unwrap();
    let function = session.eval("native-satisfies?").unwrap();
    let protocol = session.eval("P").unwrap();
    let value = session.eval("7").unwrap();
    session.collect().unwrap();
    let result = session.invoke(&function, &[&protocol, &value]).unwrap();
    let truth = session
        .inspect(&result, |mut store, value| {
            Ok(sentinel_tag(&mut store, &value)?.unwrap())
        })
        .unwrap();
    assert_eq!(truth, 4);
    assert!(matches!(
        session.invoke(&function, &[&protocol]),
        Err(SessionError::Language(_))
    ));
    assert!(matches!(
        session.invoke(&function, &[&protocol, &value, &value]),
        Err(SessionError::Language(_))
    ));
    session.enter_namespace("suss.core").unwrap();
    session
        .eval("(def native-satisfies? (fn [p x] 9))")
        .unwrap();
    session.enter_namespace("user").unwrap();
    assert_eq!(
        eval_number(&mut session, "(satisfies? P 7)"),
        RuntimeNumber::Long(9)
    );
    assert!(eval_bool(&mut session, "(satisfies? P (Item.))"));
    let result = session.invoke(&function, &[&protocol, &value]).unwrap();
    assert_eq!(
        session
            .inspect(&result, |mut store, value| Ok(sentinel_tag(&mut store, &value)?.unwrap()))
            .unwrap(),
        4
    );
}

#[test]
fn qualified_native_extension_updates_captured_dispatcher_across_namespaces_and_gc() {
    let mut session = Session::new().unwrap();
    session.enter_namespace("protocol.owner").unwrap();
    session
        .eval("(defprotocol P (read [x])) (extend-type number P (read [x] 1))")
        .unwrap();
    session.enter_namespace("protocol.client").unwrap();
    session.eval("(def captured protocol.owner/read)").unwrap();
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "(captured 7)"), RuntimeNumber::Long(1));
    session
        .eval("(extend-type number protocol.owner/P (read [x] 2))")
        .unwrap();
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "(captured 7)"), RuntimeNumber::Long(2));
    assert!(eval_bool(&mut session, "(satisfies? protocol.owner/P 7)"));
    session.enter_namespace("protocol.owner").unwrap();
    session.eval("(def read (fn [x] 9))").unwrap();
    session.enter_namespace("protocol.client").unwrap();
    // The captured wrapper keeps the protocol's own table (#279).
    assert_eq!(eval_number(&mut session, "(captured 7)"), RuntimeNumber::Long(2));
    session
        .eval("(extend-type number protocol.owner/P (read [x] 3))")
        .unwrap();
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "(captured 7)"), RuntimeNumber::Long(3));
    assert_eq!(
        eval_number(&mut session, "(protocol.owner/read 7)"),
        RuntimeNumber::Long(9)
    );
}
