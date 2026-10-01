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
fn hash_numeric_boundaries_match_independently_encoded_primary_observations() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/hash-numeric-boundaries-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 68);
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
            tag => panic!("unexpected numeric hashing observation tag {tag}"),
        };
        assert_eq!(actual, case["expected"], "{id}: {source}");
        session.collect().unwrap();
    }
}

#[test]
fn numeric_hash_boundaries_keep_effects_typed_errors_and_compile_atomicity() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session.eval("(def boundary-effects 0)").unwrap();
    for source in [
        "(suss.bootstrap/safe-integer-remainder (array 1) (do (set! boundary-effects 17) 3))",
        "(suss.bootstrap/safe-integer-remainder 1.5 3)",
        "(suss.bootstrap/safe-integer-remainder 9007199254740992 3)",
        "(suss.bootstrap/safe-integer-remainder ##NaN 3)",
        "(suss.bootstrap/safe-integer-remainder 3 ##Inf)",
        "(suss.bootstrap/safe-integer-remainder 3 -0.0)",
        "(suss.bootstrap/f64-floor (fn [] 1))",
        "(suss.bootstrap/f64-finite (array 1))",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Language(_))),
            "{source}"
        );
        session.collect().unwrap();
        assert_eq!(
            eval_number(&mut session, "boundary-effects"),
            17.0f64.to_bits()
        );
        assert_eq!(
            eval_number(
                &mut session,
                "(suss.bootstrap/safe-integer-remainder -2147483647 2147483647)"
            ),
            (-0.0f64).to_bits()
        );
    }
    assert_eq!(eval_number(&mut session, "(let [a (array 0)] (suss.bootstrap/safe-integer-remainder (do (aset a 0 17) 19) (do (aset a 0 (+ (aget a 0) 1)) 3)) (aget a 0))"), 18.0f64.to_bits());
    for (name, arity) in [
        ("f64-floor", 1),
        ("f64-finite", 1),
        ("f64-safe-integer", 1),
        ("safe-integer-remainder", 2),
    ] {
        for args in ["", "1 2 3"] {
            let source = format!("(def boundary-ghost 7) (suss.bootstrap/{name} {args})");
            let Err(SessionError::Compile(error)) = session.eval(&source) else {
                panic!("arity {name}/{arity}");
            };
            assert!(error.span.end > error.span.start);
            assert!(matches!(
                session.eval("boundary-ghost"),
                Err(SessionError::Compile(_))
            ));
        }
        assert!(matches!(
            session.eval(&format!("suss.bootstrap/{name}")),
            Err(SessionError::Compile(_))
        ));
    }
}

#[test]
fn numeric_hash_adapters_validate_in_both_compilation_phases() {
    use suss_compile::portable::{
        prepare_fragment,
        resolve::{Environment, Phase},
    };
    for phase in [Phase::Runtime, Phase::Macro] {
        for source in [
            "(suss.bootstrap/f64-floor -1.75)",
            "(suss.bootstrap/f64-finite ##NaN)",
            "(suss.bootstrap/f64-safe-integer 9007199254740991)",
            "(suss.bootstrap/safe-integer-remainder -9007199254740991 2147483647)",
        ] {
            let fragment = prepare_fragment(source, &Environment::default(), phase).unwrap();
            suss_compile::runtime_abi::verify_artifact(
                &fragment.wasm,
                &suss_compile::runtime_abi::Manifest::default(),
            )
            .unwrap();
        }
        for source in [
            "(suss.bootstrap/f64-floor)",
            "(suss.bootstrap/f64-finite 1 2)",
            "(suss.bootstrap/f64-safe-integer)",
            "(suss.bootstrap/safe-integer-remainder 1)",
        ] {
            let Err(error) = prepare_fragment(source, &Environment::default(), phase) else {
                panic!("invalid private adapter arity");
            };
            assert!(error.span.end > error.span.start);
        }
    }
}
