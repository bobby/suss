use suss_cli::portable_session::{Session, SessionError, SessionValue};
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
fn native_protocols_dispatch_nil_undefined_and_scalar_kinds() {
    let mut session = Session::new().unwrap();
    session.eval("(defprotocol P (read [x]))").unwrap();
    for (kind, source, expected) in [
        ("nil", "nil", 10.0f64),
        ("number", "0", 20.0),
        ("boolean", "false", 30.0),
        ("string", "\"\"", 40.0),
        ("function", "(fn [] 1)", 50.0),
        ("object", "(ex-info \"m\" 7)", 60.0),
    ] {
        session
            .eval(&format!("(extend-type {kind} P (read [x] {expected}))"))
            .unwrap();
        assert_eq!(
            eval_number(&mut session, &format!("(read {source})")),
            expected.to_bits()
        );
        assert!(eval_bool(&mut session, &format!("(satisfies? P {source})")));
    }
    assert_eq!(
        eval_number(&mut session, "(read (ex-data (new ExceptionInfo \"m\")))"),
        10.0f64.to_bits()
    );
}

#[test]
fn native_protocols_choose_direct_then_specific_then_default_methods() {
    let mut session = Session::new().unwrap();
    session.eval("(defprotocol P (read [x])) (deftype Item [] P (read [x] 1)) (deftype Bare []) (def item (Item.)) (def bare (Bare.)) (extend-type default P (read [x] 9))").unwrap();
    assert_eq!(eval_number(&mut session, "(read item)"), 1.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "(read bare)"), 9.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "(read 7)"), 9.0f64.to_bits());
    session
        .eval("(extend-type object P (read [x] 8)) (extend-type number P (read [x] 2))")
        .unwrap();
    assert_eq!(eval_number(&mut session, "(read item)"), 1.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "(read bare)"), 8.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "(read 7)"), 2.0f64.to_bits());
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
    assert_eq!(number(&mut session, &value), 9.0f64.to_bits());
    session
        .eval("(extend-type number P (read ([x] 3) ([x y] 4)))")
        .unwrap();
    session.collect().unwrap();
    let value = session.invoke(&reader, &[&x]).unwrap();
    assert_eq!(number(&mut session, &value), 3.0f64.to_bits());
    let value = session.invoke(&reader, &[&x, &y]).unwrap();
    assert_eq!(number(&mut session, &value), 4.0f64.to_bits());
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
    assert_eq!(eval_number(&mut session, "count"), 2.0f64.to_bits());
    session
        .eval("(extend-type number P (read [x y] (+ x y)))")
        .unwrap();
    assert_eq!(eval_number(&mut session, "(read 3 4)"), 7.0f64.to_bits());
}

#[test]
fn captured_native_dispatcher_reads_current_method_table_after_replacement() {
    let mut session = Session::new().unwrap();
    session.eval("(defprotocol P (read [x])) (extend-type number P (read [x] 1)) (def old read) (def read (fn [x] 9))").unwrap();
    assert!(matches!(
        session.eval("(old 7)"),
        Err(SessionError::Language(_))
    ));
    assert_eq!(eval_number(&mut session, "(read 7)"), 9.0f64.to_bits());
    session
        .eval("(defprotocol P (read [x])) (extend-type number P (read [x] 2))")
        .unwrap();
    assert_eq!(eval_number(&mut session, "(old 7)"), 2.0f64.to_bits());
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
    assert_eq!(eval_number(&mut session, "(old item)"), 3.0f64.to_bits());
}

#[test]
fn native_extension_replaces_all_arities_and_recur_changes_receiver() {
    let mut session = Session::new().unwrap();
    session.eval("(defprotocol P (read [x] [x y])) (extend-type number P (read ([x] 1) ([x y] 2))) (def old read) (extend-type number P (read [x] (if (nil? x) 3 (recur nil))))").unwrap();
    assert_eq!(eval_number(&mut session, "(old 7)"), 3.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "(old 7 8)"), 3.0f64.to_bits());
    session
        .eval("(extend-type number P (read [x y] (undefined? y)))")
        .unwrap();
    assert!(eval_bool(&mut session, "(old 7)"));
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
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "bool" => serde_json::json!({"tag":"bool", "value":eval_bool(&mut session, source)}),
            "f64" => {
                serde_json::json!({"tag":"f64", "bits":format!("{:016x}",eval_number(&mut session, source))})
            }
            tag => panic!("unexpected tag {tag}"),
        };
        assert_eq!(actual, case["expected"], "{source}");
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
        .inspect(&result, |store, value| {
            Ok(value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32())
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
        9.0f64.to_bits()
    );
    assert!(eval_bool(&mut session, "(satisfies? P (Item.))"));
    let result = session.invoke(&function, &[&protocol, &value]).unwrap();
    assert_eq!(
        session
            .inspect(&result, |store, value| Ok(value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32()))
            .unwrap(),
        4
    );
}
