use suss_cli::portable_session::{Session, SessionError, SessionValue};
use wasmtime::Val;

fn number(session: &mut Session, value: &SessionValue) -> u64 {
    session
        .inspect(value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Expected exact Number layout");
            };
            Ok(*bits)
        })
        .unwrap()
}
fn eval_number(session: &mut Session, source: &str) -> u64 {
    let value = session.eval(source).unwrap();
    number(session, &value)
}

fn assert_language_error(session: &mut Session, source: &str, message: &str) {
    let Err(SessionError::Language(payload)) = session.eval(source) else {
        panic!("Expected language error for {source}");
    };
    session.collect().unwrap();
    let units = session
        .inspect(&payload, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let text_value = if message == "Wrong arity" {
                assert_eq!(fields.len(), 5, "ABI2 Error layout");
                fields[1]
            } else {
                assert_eq!(fields.len(), 4, "ABI2 Object layout");
                let data = fields[1]
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                assert_eq!(
                    data.len(&store)?,
                    3,
                    "ExceptionInfo message/data/cause fields"
                );
                data.get(&mut store, 0)?
            };
            let text = text_value
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            Ok(text
                .elems(&mut store)?
                .map(|v| v.unwrap_i32() as u16)
                .collect::<Vec<_>>())
        })
        .unwrap();
    assert_eq!(
        units,
        message.encode_utf16().collect::<Vec<_>>(),
        "{source}"
    );
}

#[test]
fn persistent_session_atoms_and_old_contents_remain_rooted_across_fragments_and_gc() {
    let mut session = Session::new_repl().unwrap();
    session.eval("(def a (atom (fn [] 17))) (def read-old (let [old a] (fn [] @old))) (def mutate-old (let [old a] (fn [x] (reset! old x))))").unwrap();
    let old = session.eval("@a").unwrap();
    let read_old = session.eval("read-old").unwrap();
    session.eval("(def a (atom (fn [] 23)))").unwrap();
    for _ in 0..32 {
        session.eval("(fn [x] x)").unwrap();
    }
    session.collect().unwrap();
    let old_result = session.invoke(&old, &[]).unwrap();
    assert_eq!(number(&mut session, &old_result), 17f64.to_bits());
    let contents = session.invoke(&read_old, &[]).unwrap();
    let result = session.invoke(&contents, &[]).unwrap();
    assert_eq!(number(&mut session, &result), 17f64.to_bits());
    session.eval("(mutate-old (fn [] 31))").unwrap();
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "((read-old))"), 31f64.to_bits());
    assert_eq!(eval_number(&mut session, "(@a)"), 23f64.to_bits());
    let result = session.invoke(&old, &[]).unwrap();
    assert_eq!(number(&mut session, &result), 17f64.to_bits());
}

#[test]
fn persistent_session_atom_operations_preserve_order_and_errors_before_mutation() {
    let mut session = Session::new_repl().unwrap();
    session.eval("(def a (atom 17)) (def trace 0)").unwrap();
    assert_eq!(
        eval_number(
            &mut session,
            "(swap! (do (set! trace (+ (* trace 10) 1)) a) (do (set! trace (+ (* trace 10) 2)) (fn [v x y] (+ v x y))) (do (set! trace (+ (* trace 10) 3)) 2) (do (set! trace (+ (* trace 10) 4)) 3))"
        ),
        22f64.to_bits()
    );
    assert_eq!(eval_number(&mut session, "trace"), 1234f64.to_bits());
    let Err(SessionError::Language(thrown)) =
        session.eval("(swap! a (fn [x] (do (set! trace 9) (throw 31))))")
    else {
        panic!("Expected callback throw");
    };
    assert_eq!(number(&mut session, &thrown), 31f64.to_bits());
    assert_eq!(eval_number(&mut session, "@a"), 22f64.to_bits());
    assert_eq!(eval_number(&mut session, "trace"), 9f64.to_bits());
    session
        .eval("(set! (.-validator a) (fn [_] true))")
        .unwrap();
    assert_language_error(
        &mut session,
        "(reset! a 99)",
        "Bootstrap atom validators/watches are not implemented",
    );
    assert_eq!(eval_number(&mut session, "@a"), 22f64.to_bits());
    session
        .eval("(set! (.-validator a) nil) (set! (.-watches a) (array))")
        .unwrap();
    assert_language_error(
        &mut session,
        "(reset! a 99)",
        "Bootstrap atom validators/watches are not implemented",
    );
    assert_eq!(eval_number(&mut session, "@a"), 22f64.to_bits());
    assert_language_error(&mut session, "(swap! a (fn [x] x) 1 2 3)", "Wrong arity");
    assert_language_error(&mut session, "(atom 1 2)", "Wrong arity");
    assert_language_error(
        &mut session,
        "(reset! nil 99)",
        "Bootstrap reset! requires an Atom",
    );
    assert_language_error(
        &mut session,
        "(swap! nil (fn [x] x))",
        "Bootstrap swap! requires an Atom",
    );
    assert_eq!(eval_number(&mut session, "@a"), 22f64.to_bits());
}

#[test]
fn session_lifecycle_atom_core_is_reprovisioned_on_reset_and_failed_reset_preserves_state() {
    let mut session = Session::new_repl().unwrap();
    session.eval("(def a (atom 17))").unwrap();
    let old = session.eval("(fn [] @a)").unwrap();
    let before = session.stats();
    session.set_operation_fuel(1);
    assert!(matches!(session.reset(), Err(SessionError::Trap(_))));
    assert_eq!(session.stats(), before);
    session.set_operation_fuel(10_000_000);
    assert_eq!(eval_number(&mut session, "@a"), 17f64.to_bits());
    session.reset().unwrap();
    assert!(matches!(
        session.invoke(&old, &[]),
        Err(SessionError::ForeignValue)
    ));
    assert_eq!(session.stats().external_value_handles, 0);
    assert_eq!(session.stats().resident_fragments, 1);
    assert_eq!(eval_number(&mut session, "@(atom 23)"), 23f64.to_bits());
}

#[test]
fn persistent_session_atoms_match_independent_pinned_observations() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/atom-cases.json")).unwrap();
    let mut session = Session::new_repl().unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for case in corpus["cases"].as_array().unwrap() {
        assert!(ids.insert(case["id"].as_str().unwrap()));
        let source = case["source"].as_str().unwrap();
        let result = session
            .eval(source)
            .unwrap_or_else(|e| panic!("{source}: {e}"));
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "bool" => session
                .inspect(&result, |store, value| {
                    let sentinel = value
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&store)?
                        .unwrap()
                        .get_u32();
                    let boolean = match sentinel {
                        2 => false,
                        4 => true,
                        other => panic!("Boolean {other}"),
                    };
                    Ok(serde_json::json!({"tag":"bool", "value":boolean}))
                })
                .unwrap(),
            "f64" => {
                serde_json::json!({"tag":"f64", "bits":format!("{:016x}",number(&mut session, &result))})
            }
            tag => panic!("unexpected tag {tag}"),
        };
        assert_eq!(actual, case["expected"], "{source}");
        session.collect().unwrap();
    }
    assert_eq!(ids.len(), 28);
}
