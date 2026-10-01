use suss_cli::portable_session::Session;
use wasmtime::Val;

#[test]
fn type_method_scopes_match_independently_decoded_primary_observations_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/type-method-scope-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 14);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session
        .enter_namespace("suss-oracle.type-method-scope-cases")
        .unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        assert!(ids.insert(id));
        let source = case["source"].as_str().unwrap();
        let value = session
            .eval(source)
            .unwrap_or_else(|error| panic!("{id}: {source}: {error:?}"));
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "f64" => {
                let bits = session
                    .inspect(&value, |mut store, value| {
                        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                        let fields = object.fields(&mut store)?.collect::<Vec<_>>();
                        let [Val::F64(bits)] = fields.as_slice() else {
                            panic!("Number layout");
                        };
                        Ok(*bits)
                    })
                    .unwrap();
                serde_json::json!({"tag":"f64", "bits":format!("{bits:016x}")})
            }
            "nil" => {
                session
                    .inspect(&value, |store, value| {
                        assert_eq!(
                            value
                                .unwrap_anyref()
                                .unwrap()
                                .as_i31(&store)?
                                .unwrap()
                                .get_u32(),
                            0
                        );
                        Ok(())
                    })
                    .unwrap();
                serde_json::json!({"tag":"nil"})
            }
            "bool" => {
                let boolean = session
                    .inspect(&value, |store, value| {
                        let sentinel = value
                            .unwrap_anyref()
                            .unwrap()
                            .as_i31(&store)?
                            .unwrap()
                            .get_u32();
                        match sentinel {
                            2 => Ok(false),
                            4 => Ok(true),
                            n => panic!("Boolean {n}"),
                        }
                    })
                    .unwrap();
                serde_json::json!({"tag":"bool", "value":boolean})
            }
            "string" => {
                let units = session
                    .inspect(&value, |mut store, value| {
                        let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                        Ok(array
                            .elems(&mut store)?
                            .map(|v| v.unwrap_i32() as u16)
                            .collect::<Vec<_>>())
                    })
                    .unwrap();
                serde_json::json!({"tag":"string", "units":units})
            }
            tag => panic!("unsupported Type-method-scope observation {tag}"),
        };
        assert_eq!(actual, case["expected"], "{id}: {source}");
        session.collect().unwrap();
    }
}

#[test]
fn unresolved_type_method_capture_is_compile_atomic_and_session_recovers() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session.eval("(def scope-factor 11) (def scope-kept 17) (defprotocol ScopeTransaction (scope-transaction [this]))").unwrap();
    let source = "(do (def scope-unpublished 19) (let [scope-private 7] (deftype ScopeInvalid [] ScopeTransaction (scope-transaction [this] scope-private))))";
    let error = session.eval(source).unwrap_err();
    let SessionError::Compile(diagnostic) = error else {
        panic!("Expected a located compile error");
    };
    assert_eq!(&source[diagnostic.span], "scope-private");
    assert!(diagnostic
        .message
        .contains("Unresolved Runtime name scope-private"));
    for symbol in ["scope-unpublished", "ScopeInvalid", "->ScopeInvalid"] {
        assert!(
            matches!(session.eval(symbol), Err(SessionError::Compile(_))),
            "{symbol}"
        );
    }
    session.collect().unwrap();
    let result = session.eval("(let [scope-factor 7] (do (deftype ScopeRecovery [value] ScopeTransaction (scope-transaction [this] (* value scope-factor))) (+ scope-kept scope-factor (scope-transaction (ScopeRecovery. 5)))))").unwrap();
    session.collect().unwrap();
    session
        .inspect(&result, |mut store, value| {
            let fields = value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap()
                .fields(&mut store)?
                .collect::<Vec<_>>();
            assert!(matches!(fields.as_slice(), [Val::F64(bits)] if *bits == 79.0f64.to_bits()));
            Ok(())
        })
        .unwrap();
}
