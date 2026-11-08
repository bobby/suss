use suss_cli::portable_session::Session;

/// The observation of a runtime number, distinguishing a long from a double.
fn number_json(number: suss_cli::portable_session::RuntimeNumber) -> serde_json::Value {
    match number {
        suss_cli::portable_session::RuntimeNumber::Long(value) => serde_json::json!({"tag":"long", "value":value}),
        suss_cli::portable_session::RuntimeNumber::Double(value) => {
            serde_json::json!({"tag":"f64", "bits":format!("{:016x}", value.to_bits())})
        }
    }
}
/// The ClojureScript oracle encodes every number as a double; JVM Clojure,
/// Suss's oracle, gives a long for these integral results.
fn clojure_number(expected: &serde_json::Value) -> serde_json::Value {
    if expected["tag"] == "f64" {
        let bits = u64::from_str_radix(expected["bits"].as_str().unwrap(), 16).unwrap();
        let value = f64::from_bits(bits);
        if value.is_finite() && value.fract() == 0.0 && bits != (-0.0f64).to_bits() {
            return serde_json::json!({"tag":"long", "value":value as i64});
        }
    }
    expected.clone()
}

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
                let number = session
                    .inspect(&value, |store, value| {
                        Ok(suss_cli::portable_session::runtime_number(store, &value)?.expect("Number layout"))
                    })
                    .unwrap();
                number_json(number)
            }
            "nil" => {
                session
                    .inspect(&value, |mut store, value| {
                        assert_eq!(
                            suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap(),
                            0
                        );
                        Ok(())
                    })
                    .unwrap();
                serde_json::json!({"tag":"nil"})
            }
            "bool" => {
                let boolean = session
                    .inspect(&value, |mut store, value| {
                        let sentinel = suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap();
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
        assert_eq!(actual, clojure_number(&case["expected"]), "{id}: {source}");
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
        .inspect(&result, |store, value| {
            assert_eq!(
                suss_cli::portable_session::runtime_number(store, &value)?,
                Some(suss_cli::portable_session::RuntimeNumber::Long(79))
            );
            Ok(())
        })
        .unwrap();
}
