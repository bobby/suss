use suss_cli::portable_session::Session;

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
fn default_hash_matches_pinned_identity_and_root_relations() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/default-hash-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 21);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def rootobj (fn [] (suss.core/root-obj))) (def uid (fn [x] (suss.bootstrap/identity-uid x))) (def error (fn [message] (suss.bootstrap/error message)))").unwrap();
    for case in cases {
        assert_eq!(
            serde_json::json!({"tag":"bool", "value":eval_bool(&mut session, case["source"].as_str().unwrap())}),
            case["expected"],
            "{}",
            case["id"]
        );
        session.collect().unwrap();
    }
}

#[test]
fn default_hash_errors_preserve_effects_and_private_adapter_is_checked() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session
        .eval("(def default-effect false) (def default-survivor (fn [] 17))")
        .unwrap();
    for source in ["nil", "false", "17", "\"text\""] {
        assert!(
            matches!(
                session.eval(&format!("(-hash (do (set! default-effect true) {source}))")),
                Err(SessionError::Language(_))
            ),
            "{source}"
        );
        session.collect().unwrap();
        assert!(eval_bool(&mut session, "default-effect"));
        assert!(eval_bool(
            &mut session,
            "(= (-hash default-survivor) (suss.bootstrap/identity-uid default-survivor))"
        ));
    }
    for source in [
        "(suss.bootstrap/object-default-prototype nil)",
        "suss.bootstrap/object-default-prototype",
    ] {
        let Err(SessionError::Compile(error)) =
            session.eval(&format!("(def default-ghost 7) {source}"))
        else {
            panic!("private prototype adapter must fail before publication");
        };
        assert!(error.span.end > error.span.start);
        assert!(matches!(
            session.eval("default-ghost"),
            Err(SessionError::Compile(_))
        ));
    }
    use suss_compile::portable::{
        prepare_fragment,
        resolve::{Environment, Phase},
    };
    for phase in [Phase::Runtime, Phase::Macro] {
        let fragment = prepare_fragment(
            "(suss.bootstrap/object-default-prototype)",
            &Environment::default(),
            phase,
        )
        .unwrap();
        suss_compile::runtime_abi::verify_artifact(
            &fragment.wasm,
            &suss_compile::runtime_abi::Manifest::default(),
        )
        .unwrap();
        assert!(prepare_fragment(
            "(suss.bootstrap/object-default-prototype nil)",
            &Environment::default(),
            phase
        )
        .is_err());
    }
}
