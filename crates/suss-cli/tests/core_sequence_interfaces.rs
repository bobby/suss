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
            expected.to_bits(),
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
    assert_eq!(cases.len(), 60);
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
                serde_json::json!({"tag":"f64", "bits":format!("{:016x}",eval_number(&mut session, source))})
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
            52.0f64.to_bits(),
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
        13.0f64.to_bits()
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
            10.0f64.to_bits(),
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
        14.0f64.to_bits()
    );
}
