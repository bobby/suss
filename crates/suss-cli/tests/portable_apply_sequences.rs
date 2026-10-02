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
        assert_eq!(eval_number(&mut session, source), 1.0f64.to_bits(), "{source}");
    }
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "(count (list* 1 2 3 4 [5 6]))"), 6.0f64.to_bits());
}

#[test]
fn retained_bounded_count_keeps_counted_fast_path_and_limits_seq_effects() {
    let mut session = session();
    session.eval("(def steps 0) (deftype CountProbe [remaining] ISeqable (-seq [this] (if (> remaining 0) this nil)) ISeq (-first [_] remaining) (-rest [_] (do (set! steps (+ steps 1)) (CountProbe. (- remaining 1)))) INext (-next [_] (do (set! steps (+ steps 1)) (if (> remaining 1) (CountProbe. (- remaining 1)) nil))))").unwrap();
    assert_eq!(eval_number(&mut session, "(bounded-count 1 [1 2 3 4])"), 4.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "(bounded-count 2 (CountProbe. 100))"), 2.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "steps"), 2.0f64.to_bits());
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "(bounded-count 0 (CountProbe. 100))"), 0.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "steps"), 2.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "(bounded-count 10 (CountProbe. 3))"), 3.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "steps"), 5.0f64.to_bits());
}

#[test]
fn private_definitions_retain_callable_qualified_access() {
    let mut session = session();
    session.enter_namespace("apply.private").unwrap();
    session.eval("(def ^:private hidden (fn [x] (+ x 1)))").unwrap();
    assert_eq!(eval_number(&mut session, "(hidden 41)"), 42.0f64.to_bits());
    session.enter_namespace("user").unwrap();
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "(apply.private/hidden 42)"), 43.0f64.to_bits());
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
        let expected = u64::from_str_radix(case["expected"]["bits"].as_str().unwrap(), 16).unwrap();
        assert_eq!(eval_number(&mut session, &source), expected, "{}", case["id"]);
    }
}
