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
    boolean(session, &value)
}
fn boolean(session: &mut Session, value: &SessionValue) -> bool {
    session
        .inspect(value, |store, value| {
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
fn reduction_contract_match_independently_encoded_primary_observations() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/reduction-cases.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 65);
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
            tag => panic!("unexpected reduction observation tag {tag}"),
        };
        assert_eq!(actual, case["expected"], "{id}: {source}");
        session.collect().unwrap();
    }
}

#[test]
fn reduced_values_and_saved_reducers_survive_gc_and_language_errors() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def retained-reduced (reduced (list 17 19))) (def retained-reducer reduce) (def reduction-error-trace 0)").unwrap();
    session.collect().unwrap();
    assert_eq!(
        eval_number(
            &mut session,
            "(retained-reducer + (deref retained-reduced))"
        ),
        36.0f64.to_bits()
    );
    for source in [
        "(reduce)",
        "(reduce +)",
        "(array-reduce (array 1) + 0 0 (do (set! reduction-error-trace 17) nil))",
        "(reduce (fn [a] a) 0 (list 1 2))",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Language(_))),
            "{source}"
        );
        session.collect().unwrap();
        assert_eq!(
            eval_number(
                &mut session,
                "(retained-reducer + (deref retained-reduced))"
            ),
            36.0f64.to_bits()
        );
    }
    assert_eq!(
        eval_number(&mut session, "reduction-error-trace"),
        17.0f64.to_bits()
    );
    let Err(SessionError::Language(value)) = session.eval("(reduce (fn [a b] (set! reduction-error-trace (+ (* reduction-error-trace 10) b)) (throw 73)) 0 (list 1 2 3))") else { panic!("typed reducer exception"); };
    session.collect().unwrap();
    assert_eq!(number(&mut session, &value), 73.0f64.to_bits());
    assert_eq!(
        eval_number(&mut session, "reduction-error-trace"),
        171.0f64.to_bits()
    );
    assert!(eval_bool(&mut session, "(reduced? retained-reduced)"));
    assert_eq!(
        eval_number(
            &mut session,
            "(retained-reducer + (deref retained-reduced))"
        ),
        36.0f64.to_bits()
    );
}

#[test]
fn reduced_stops_vector_callbacks_at_chunk_boundaries_in_both_phases() {
    use suss_cli::portable_session::SessionOptions;
    use suss_compile::portable::resolve::Phase;
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/reduction-boundary-cases.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 108);
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
        session.set_operation_fuel(100_000_000);
        session
            .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
            .unwrap();
        session.enter_namespace("user").unwrap();
        session
            .eval(include_str!(
                "../../../tests/oracle/fixtures/reduction-boundary-probe.sus"
            ))
            .unwrap();
        let mut ids = std::collections::BTreeSet::new();
        for case in cases {
            let id = case["id"].as_str().unwrap();
            assert!(ids.insert(id));
            let source = case["source"].as_str().unwrap();
            let value = session
                .eval(source)
                .unwrap_or_else(|error| panic!("{phase:?}, {id}: {error}"));
            session.collect().unwrap();
            let actual = match case["expected"]["tag"].as_str().unwrap() {
                "bool" => {
                    serde_json::json!({"tag":"bool", "value":boolean(&mut session, &value)})
                }
                "f64" => {
                    serde_json::json!({"tag":"f64", "bits":format!("{:016x}",number(&mut session, &value))})
                }
                tag => panic!("unexpected boundary observation tag {tag}"),
            };
            assert_eq!(actual, case["expected"], "{phase:?}, {id}: {source}");
        }
    }
}
