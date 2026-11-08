use suss_cli::portable_session::RuntimeNumber;
use suss_cli::portable_session::Session;

#[path = "support/clojure_corrections.rs"]
mod clojure_corrections;

/// Recorded observations that coerced a non-number operand; Clojure rejects them.
const CORRECTIONS: &[clojure_corrections::Correction] = &[
    ("object-method-detached-loses-receiver", "((.-score object-probe) 3)", "+ requires number operands"),
    ("object-method-missing-argument", "", "+ requires number operands"),
];

/// The observation of a runtime number, distinguishing a long from a double.
fn number_json(number: RuntimeNumber) -> serde_json::Value {
    match number {
        RuntimeNumber::Long(value) => serde_json::json!({"tag":"long", "value":value}),
        RuntimeNumber::Double(value) => {
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
fn object_methods_match_independently_decoded_primary_observations_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/object-method-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 52);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session
        .enter_namespace("suss-oracle.object-method-cases")
        .unwrap();
    let mut ids = std::collections::BTreeSet::new();
    let mut corrected = 0;
    for case in cases {
        let case = &clojure_corrections::corrected(case, CORRECTIONS, &mut corrected);
        let id = case["id"].as_str().unwrap();
        assert!(ids.insert(id.to_owned()));
        let source = case["source"].as_str().unwrap();
        let value = session
            .eval(source)
            .unwrap_or_else(|error| panic!("{id}: {source}: {error:?}"));
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "f64" => {
                number_json(
                    session
                        .inspect(&value, |store, value| {
                            Ok(suss_cli::portable_session::runtime_number(store, &value)?
                                .expect("Number layout"))
                        })
                        .unwrap(),
                )
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
            tag => panic!("unsupported Object-method observation {tag}"),
        };
        assert_eq!(actual, clojure_number(&case["expected"]), "{id}: {source}");
        session.collect().unwrap();
    }
    assert_eq!(corrected, CORRECTIONS.len());
}

#[test]
fn malformed_object_methods_fail_atomically_and_session_recovers() {
    let mut session = Session::new().unwrap();
    // ClojureScript munges ordinary hyphens in Object method names; this is a
    // valid method declaration, not a malformed-method rejection case.
    session
        .eval("(deftype ObjectNamedMethod [] Object (bad-name [this] 83)) (def object-named-method (ObjectNamedMethod.))")
        .unwrap();
    let named_method = session.eval("(.bad-name object-named-method)").unwrap();
    session.collect().unwrap();
    session
        .inspect(&named_method, |store, value| {
            assert_eq!(
                suss_cli::portable_session::runtime_number(store, &value)?,
                Some(RuntimeNumber::Long(83))
            );
            Ok(())
        })
        .unwrap();
    for source in [
        "(do (def object-unpublished 3) (deftype ObjectInvalid [] Object (missing [] 1)))",
        "(deftype ObjectInvalid [] Object (missing [this] object-private-name))",
        "(do (def object-unpublished 3) (deftype ObjectInvalid [] Object (__proto__ [this] 1)))",
        "(do (def object-unpublished 3) (defprotocol ObjectProtoGuard (read-proto [this])) (deftype ObjectInvalid [__proto__] ObjectProtoGuard (read-proto [this] __proto__)))",
        "(deftype ObjectInvalid [] Object (missing [this & rest] 1))",
        "(deftype ObjectInvalid [] Object (missing [this n] (recur this n)))",
    ] {
        let error = session.eval(source).unwrap_err();
        assert!(
            matches!(error, suss_cli::portable_session::SessionError::Compile(_)),
            "{source}: {error:?}"
        );
        if let Some(start) = source.find("__proto__") {
            let suss_cli::portable_session::SessionError::Compile(diagnostic) = &error else {
                unreachable!()
            };
            assert_eq!(diagnostic.span, start..start + "__proto__".len());
            assert!(diagnostic.message.contains("prototype mutation"));
        }
        for name in ["object-unpublished", "ObjectInvalid", "->ObjectInvalid"] {
            assert!(
                session.eval(name).is_err(),
                "published {name} after {source}"
            );
        }
        session.collect().unwrap();
        let value = session.eval("79").unwrap();
        session
            .inspect(&value, |store, value| {
                assert_eq!(
                    suss_cli::portable_session::runtime_number(store, &value)?,
                    Some(RuntimeNumber::Long(79))
                );
                Ok(())
            })
            .unwrap();
    }
}
