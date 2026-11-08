use suss_cli::portable_session::{RuntimeNumber, Session, SessionValue};

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
fn session() -> Session {
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session
}

#[test]
fn source_apply_preserves_fixed_variadic_and_captured_functions_after_gc() {
    let mut session = session();
    session.eval("(def original (fn [x y] (+ x y))) (def captured original) (def variadic (fn [x & xs] (+ x (count xs))))").unwrap();
    assert_eq!(
        eval_number(&mut session, "(apply captured [17 19])"),
        RuntimeNumber::Long(36)
    );
    assert_eq!(
        eval_number(&mut session, "(apply variadic 10 [1 2 3])"),
        RuntimeNumber::Long(13)
    );
    session.eval("(def original (fn [x y] 99))").unwrap();
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(apply captured 20 [22])"),
        RuntimeNumber::Long(42)
    );
}

#[test]
fn source_apply_invokes_actual_metafn_and_collection_methods() {
    let mut session = session();
    session
        .eval("(def wrapped (with-meta (fn [x] (+ x 1)) {:doc 7}))")
        .unwrap();
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(apply wrapped [41])"),
        RuntimeNumber::Long(42)
    );
    assert_eq!(
        eval_number(&mut session, "(apply [17 19] [1])"),
        RuntimeNumber::Long(19)
    );
    assert_eq!(
        eval_number(&mut session, "(apply {:x 23} [:x])"),
        RuntimeNumber::Long(23)
    );
    assert_eq!(
        eval_number(&mut session, "(apply :x [{:x 29}])"),
        RuntimeNumber::Long(29)
    );
}

#[test]
fn source_apply_runs_operands_once_before_language_arity_failure() {
    let mut session = session();
    session
        .eval("(def effects 0) (def target (fn [x] x))")
        .unwrap();
    assert!(matches!(session.eval("(apply (do (set! effects 1) target) (do (set! effects (+ effects 10)) 7) (do (set! effects (+ effects 100)) [8]))"), Err(suss_cli::portable_session::SessionError::Language(_))));
    assert_eq!(eval_number(&mut session, "effects"), RuntimeNumber::Long(111));
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(apply target [42])"),
        RuntimeNumber::Long(42)
    );
}

#[test]
fn source_apply_high_arity_and_sequence_tail_are_preserved() {
    let mut session = session();
    let parameters = (0..25)
        .map(|i| format!("a{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let arguments = (0..25).map(|i| i.to_string()).collect::<Vec<_>>().join(" ");
    assert_eq!(
        eval_number(
            &mut session,
            &format!("(apply (fn [{parameters}] a24) [{arguments}])")
        ),
        RuntimeNumber::Long(24)
    );
    session.eval("(def seq-effects 0) (deftype ApplySeq [n] ISeqable (-seq [this] this) ISeq (-first [_] (do (set! seq-effects (+ seq-effects 10)) n)) (-rest [_] (do (set! seq-effects (+ seq-effects 100)) (ApplySeq. (+ n 1)))) INext (-next [_] (do (set! seq-effects (+ seq-effects 1)) (ApplySeq. (+ n 1)))))").unwrap();
    assert_eq!(
        eval_number(&mut session, "(apply (fn [x y & more] x) (ApplySeq. 0))"),
        RuntimeNumber::Long(0)
    );
    assert_eq!(eval_number(&mut session, "seq-effects"), RuntimeNumber::Long(124));
    session.collect().unwrap();
    assert_eq!(
        eval_number(
            &mut session,
            "(apply (fn [& more] (first more)) (ApplySeq. 99))"
        ),
        RuntimeNumber::Long(99)
    );
}

#[test]
fn source_apply_corpus_matches_in_both_phases_with_explicit_strict_arity_boundary() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let _bridge = suss_cli::portable_macro_data::FormBridge::new(&mut session).unwrap();
        session.enter_namespace("user").unwrap();
        session
            .eval(include_str!(
                "../../../tests/oracle/source-apply-fixture.sus"
            ))
            .unwrap();
        let corpus: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/oracle/source-apply-cases.json"
        ))
        .unwrap();
        let mut matching = 0;
        let mut boundaries = 0;
        for case in corpus["cases"].as_array().unwrap() {
            session.collect().unwrap();
            let source = case["source"].as_str().unwrap();
            let actual = eval_number(&mut session, source);
            // The ClojureScript primary encodes every number as a double; these
            // integral results are longs in JVM Clojure.
            let primary = RuntimeNumber::Long(
                f64::from_bits(
                    u64::from_str_radix(case["expected"]["bits"].as_str().unwrap(), 16)
                        .unwrap(),
                ) as i64,
            );
            if case["id"] == "strict-fixed-arity-boundary" {
                assert_eq!(primary, RuntimeNumber::Long(7));
                assert_eq!(
                    actual,
                    RuntimeNumber::Long(111),
                    "accepted design requires strict arity"
                );
                boundaries += 1;
            } else if case["id"] == "vector-wrong-arity" {
                assert_eq!(primary, RuntimeNumber::Long(1));
                assert_eq!(
                    actual,
                    RuntimeNumber::Long(47),
                    "accepted design requires strict vector arity"
                );
                boundaries += 1;
            } else {
                assert_eq!(actual, primary, "{}", case["id"]);
                matching += 1;
            }
        }
        assert_eq!((matching, boundaries), (20, 2));
    }
}
