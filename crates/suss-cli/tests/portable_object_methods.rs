use suss_cli::portable_session::Session;
use wasmtime::Val;

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
            tag => panic!("unsupported Object-method observation {tag}"),
        };
        assert_eq!(actual, case["expected"], "{id}: {source}");
        session.collect().unwrap();
    }
}

#[test]
fn malformed_object_methods_fail_atomically_and_session_recovers() {
    let mut session = Session::new().unwrap();
    for source in [
        "(do (def object-unpublished 3) (deftype ObjectInvalid [] Object (missing [] 1)))",
        "(deftype ObjectInvalid [] Object (missing [this] object-private-name))",
        "(deftype ObjectInvalid [] Object (bad-name [this] 1))",
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
            .inspect(&value, |mut store, value| {
                let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                assert_eq!(
                    object.fields(&mut store)?.next().unwrap().unwrap_f64(),
                    79.0
                );
                Ok(())
            })
            .unwrap();
    }
}
