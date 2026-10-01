use suss_cli::portable_session::Session;
use wasmtime::Val;

#[test]
fn named_properties_match_independently_decoded_primary_observations_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/named-property-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 49);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session
        .enter_namespace("suss-oracle.named-property-cases")
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
            tag => panic!("unsupported control observation {tag}"),
        };
        assert_eq!(actual, case["expected"], "{id}: {source}");
        session.collect().unwrap();
    }
}

#[test]
fn malformed_or_unsupported_named_forms_preserve_session_and_recover() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval("(def named-owner (let [n 7] (fn [x] (+ n x)))) (set! (.-cache named-owner) 11)")
        .unwrap();
    for source in [
        "(.-cache)",
        "(.-cache named-owner 1)",
        "(.-cache nil)",
        "(.-bad-name named-owner)",
        "(set! (.-cache) 3)",
        "(set! (.-cache nil) 3)",
        "(do (def property-should-not-publish 17) (.-bad-name named-owner))",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Compile(_))),
            "{source}"
        );
        session.collect().unwrap();
    }
    assert!(matches!(
        session.eval("property-should-not-publish"),
        Err(SessionError::Compile(_))
    ));
    for source in ["(set! (.-newField (new NamedUnsupported)) 1)"] {
        // An unresolved constructor is a compile error, not a nil/property fallback.
        assert!(matches!(
            session.eval(source),
            Err(SessionError::Compile(_))
        ));
    }
    session
        .eval("(deftype NamedUnsupported [value]) (def named-instance (NamedUnsupported. 19))")
        .unwrap();
    for source in [
        "(set! (.-newField named-instance) 1)",
        "(set! (.-length (array 1)) 0)",
        "(set! (.-length \"x\") 0)",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Language(_))),
            "{source}"
        );
    }
    let result = session
        .eval("(+ (named-owner 2) (.-cache named-owner) (.-value named-instance))")
        .unwrap();
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
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Number layout")
            };
            assert_eq!(*bits, 39.0f64.to_bits());
            Ok(())
        })
        .unwrap();
}
