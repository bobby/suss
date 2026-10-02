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
fn source_apply_preserves_fixed_variadic_and_captured_functions_after_gc() {
    let mut session = session();
    session.eval("(def original (fn [x y] (+ x y))) (def captured original) (def variadic (fn [x & xs] (+ x (count xs))))").unwrap();
    assert_eq!(
        eval_number(&mut session, "(apply captured [17 19])"),
        36.0f64.to_bits()
    );
    assert_eq!(
        eval_number(&mut session, "(apply variadic 10 [1 2 3])"),
        13.0f64.to_bits()
    );
    session.eval("(def original (fn [x y] 99))").unwrap();
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(apply captured 20 [22])"),
        42.0f64.to_bits()
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
        42.0f64.to_bits()
    );
    assert_eq!(
        eval_number(&mut session, "(apply [17 19] [1])"),
        19.0f64.to_bits()
    );
    assert_eq!(
        eval_number(&mut session, "(apply {:x 23} [:x])"),
        23.0f64.to_bits()
    );
    assert_eq!(
        eval_number(&mut session, "(apply :x [{:x 29}])"),
        29.0f64.to_bits()
    );
}

#[test]
fn source_apply_runs_operands_once_before_language_arity_failure() {
    let mut session = session();
    session
        .eval("(def effects 0) (def target (fn [x] x))")
        .unwrap();
    assert!(matches!(session.eval("(apply (do (set! effects 1) target) (do (set! effects (+ effects 10)) 7) (do (set! effects (+ effects 100)) [8]))"), Err(suss_cli::portable_session::SessionError::Language(_))));
    assert_eq!(eval_number(&mut session, "effects"), 111.0f64.to_bits());
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(apply target [42])"),
        42.0f64.to_bits()
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
        24.0f64.to_bits()
    );
    session.eval("(def seq-effects 0) (deftype ApplySeq [n] ISeqable (-seq [this] this) ISeq (-first [_] (do (set! seq-effects (+ seq-effects 10)) n)) (-rest [_] (do (set! seq-effects (+ seq-effects 100)) (ApplySeq. (+ n 1)))) INext (-next [_] (do (set! seq-effects (+ seq-effects 1)) (ApplySeq. (+ n 1)))))").unwrap();
    assert_eq!(
        eval_number(&mut session, "(apply (fn [x y & more] x) (ApplySeq. 0))"),
        0.0f64.to_bits()
    );
    assert_eq!(eval_number(&mut session, "seq-effects"), 124.0f64.to_bits());
    session.collect().unwrap();
    assert_eq!(
        eval_number(
            &mut session,
            "(apply (fn [& more] (first more)) (ApplySeq. 99))"
        ),
        99.0f64.to_bits()
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
            let primary =
                u64::from_str_radix(case["expected"]["bits"].as_str().unwrap(), 16).unwrap();
            if case["id"] == "strict-fixed-arity-boundary" {
                assert_eq!(primary, 7.0f64.to_bits());
                assert_eq!(
                    actual,
                    111.0f64.to_bits(),
                    "accepted design requires strict arity"
                );
                boundaries += 1;
            } else if case["id"] == "vector-wrong-arity" {
                assert_eq!(primary, 1.0f64.to_bits());
                assert_eq!(
                    actual,
                    47.0f64.to_bits(),
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
