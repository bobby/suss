use suss_cli::portable_session::{Session, SessionValue};
use wasmtime::Val;

fn number(session: &mut Session, value: &SessionValue) -> u64 {
    session
        .inspect(value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Number layout");
            };
            Ok(*bits)
        })
        .unwrap()
}
fn eval_number(session: &mut Session, source: &str) -> u64 {
    let value = session.eval(source).unwrap();
    number(session, &value)
}

#[test]
fn identity_uids_remain_owned_stable_and_distinct_across_gc_fragments() {
    let mut session = Session::new().unwrap();
    session.eval("(def identity-f (fn [] 17))").unwrap();
    let first = eval_number(&mut session, "(suss.bootstrap/identity-uid identity-f)");
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(suss.bootstrap/identity-uid identity-f)"),
        first
    );
    assert_eq!(eval_number(&mut session, "(identity-f)"), 17.0f64.to_bits());
    session
        .eval("(deftype IdentityObject [x]) (def identity-o (IdentityObject. 19))")
        .unwrap();
    let second = eval_number(&mut session, "(suss.bootstrap/identity-uid identity-o)");
    assert_ne!(second, first);
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(suss.bootstrap/identity-uid identity-o)"),
        second
    );
    assert_eq!(
        eval_number(&mut session, "(.-x identity-o)"),
        19.0f64.to_bits()
    );
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
fn identity_hash_contract_matches_pinned_owned_value_relations() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/identity-hash-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 34);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def uid (fn [x] (suss.bootstrap/identity-uid x))) (def error (fn [message] (suss.bootstrap/error message)))").unwrap();
    let mut matching = 0;
    let mut boundaries = 0;
    for case in cases {
        if case["id"] == "metafn-explicit-invoke" {
            // Preserve the certified pin observation, but enforce the accepted
            // strict fixed-function arity contract on this exact source.
            assert_eq!(case["source"], "(let [w (with-meta (fn [] 17) false)] (uid w) (= (-invoke w) 17))");
            assert_eq!(case["expected"], serde_json::json!({"tag":"bool", "value":true}));
            assert_wrong_arity(&mut session, case["source"].as_str().unwrap());
            boundaries += 1;
            continue;
        }
        matching += 1;
        assert_eq!(
            serde_json::json!({"tag":"bool", "value":eval_bool(&mut session, case["source"].as_str().unwrap())}),
            case["expected"],
            "{}",
            case["id"]
        );
        session.collect().unwrap();
    }
    assert_eq!((matching, boundaries), (33, 1));
}

fn assert_wrong_arity(session: &mut Session, source: &str) {
    let Err(suss_cli::portable_session::SessionError::Language(value)) = session.eval(source) else {
        panic!("exact typed arity boundary: {source}");
    };
    session.collect().unwrap();
    let units = session.inspect(&value, |mut store, value| {
        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
        let fields = object.fields(&mut store)?.collect::<Vec<_>>();
        assert_eq!(fields.len(), 5, "ABI2 Error");
        let text = fields[1].unwrap_anyref().unwrap().as_array(&store)?.unwrap();
        Ok(text.elems(&mut store)?.map(|v| v.unwrap_i32() as u16).collect::<Vec<_>>())
    }).unwrap();
    assert_eq!(units, "Wrong arity".encode_utf16().collect::<Vec<_>>());
}

#[test]
fn identity_uid_errors_preserve_effects_and_compile_atomicity() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval("(def identity-effects 0) (def identity-survivor (fn [] 17))")
        .unwrap();
    let saved = eval_number(
        &mut session,
        "(suss.bootstrap/identity-uid identity-survivor)",
    );
    for source in ["nil", "false", "17", "\"text\""] {
        assert!(
            matches!(
                session.eval(&format!(
                    "(suss.bootstrap/identity-uid (do (set! identity-effects 19) {source}))"
                )),
                Err(SessionError::Language(_))
            ),
            "{source}"
        );
        session.collect().unwrap();
        assert_eq!(
            eval_number(&mut session, "identity-effects"),
            19.0f64.to_bits()
        );
        assert_eq!(
            eval_number(
                &mut session,
                "(suss.bootstrap/identity-uid identity-survivor)"
            ),
            saved
        );
        assert_eq!(
            eval_number(&mut session, "(identity-survivor)"),
            17.0f64.to_bits()
        );
    }
    for source in [
        "(suss.bootstrap/identity-uid)",
        "(suss.bootstrap/identity-uid 1 2)",
        "suss.bootstrap/identity-uid",
    ] {
        let Err(SessionError::Compile(error)) =
            session.eval(&format!("(def identity-ghost 7) {source}"))
        else {
            panic!("located private adapter guard");
        };
        assert!(error.span.end > error.span.start);
        assert!(matches!(
            session.eval("identity-ghost"),
            Err(SessionError::Compile(_))
        ));
    }
    use suss_compile::portable::{
        prepare_fragment,
        resolve::{Environment, Phase},
    };
    for phase in [Phase::Runtime, Phase::Macro] {
        let fragment = prepare_fragment(
            "(suss.bootstrap/identity-uid (fn [] 1))",
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
            "(suss.bootstrap/identity-uid)",
            &Environment::default(),
            phase
        )
        .is_err());
    }
}

#[test]
fn explicit_ifn_receiver_preserves_operand_effects_and_unrelated_protocols() {
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session
        .eval("(def invoke-effects 0) (def invoke-owner (with-meta (fn [target a] a) false))")
        .unwrap();
    let saved = eval_number(&mut session, "(suss.bootstrap/identity-uid invoke-owner)");
    assert_eq!(eval_number(&mut session, "(-invoke (do (set! invoke-effects 1) invoke-owner) (do (set! invoke-effects (+ invoke-effects 10)) 17))"), 17.0f64.to_bits());
    assert_eq!(
        eval_number(&mut session, "invoke-effects"),
        11.0f64.to_bits()
    );
    assert_wrong_arity(&mut session, "(-invoke invoke-owner 17 19)");
    session.eval("(set! invoke-effects 0)").unwrap();
    assert!(matches!(
        session.eval("(-invoke (throw 19) (do (set! invoke-effects 7) 17))"),
        Err(suss_cli::portable_session::SessionError::Language(_))
    ));
    assert_eq!(
        eval_number(&mut session, "invoke-effects"),
        0.0f64.to_bits()
    );
    session.collect().unwrap();
    assert_eq!(
        eval_number(&mut session, "(suss.bootstrap/identity-uid invoke-owner)"),
        saved
    );
    assert_eq!(
        eval_number(&mut session, "(-invoke invoke-owner 17)"),
        17.0f64.to_bits()
    );
    session.enter_namespace("ordinary-invoke").unwrap();
    session.eval("(defprotocol IFn (-invoke [this a])) (deftype OrdinaryInvoke [] IFn (-invoke [this a] a))").unwrap();
    assert_eq!(
        eval_number(&mut session, "(-invoke (OrdinaryInvoke.) 23)"),
        23.0f64.to_bits()
    );
}
