use suss_cli::portable_session::Session;
use wasmtime::Val;

#[test]
fn named_properties_match_independently_decoded_primary_observations_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/named-property-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 64);
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
        "(.-1cache named-owner)",
        "(set! (.-1cache named-owner) 3)",
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
        "(set! (.-length \"x\") 0)",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Language(_))),
            "{source}"
        );
    }
    session.eval("(def named-array (array 7 8))").unwrap();
    for source in ["(set! (.-length named-array) 0)", "(.-length named-array)"] {
        let value = session.eval(source).unwrap();
        session.collect().unwrap();
        session
            .inspect(&value, |mut store, value| {
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
                assert_eq!(*bits, 0.0_f64.to_bits());
                Ok(())
            })
            .unwrap();
    }
    let removed = session.eval("(aget named-array 0)").unwrap();
    session.collect().unwrap();
    session
        .inspect(&removed, |store, value| {
            assert_eq!(
                value
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .unwrap()
                    .get_u32(),
                6
            );
            Ok(())
        })
        .unwrap();
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

#[test]
fn named_access_rejects_munged_schemas_without_breaking_lexical_fields() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session.eval("(defprotocol NamedReviewRead (named-review-read [owner])) (deftype NamedReviewReserved [null] NamedReviewRead (named-review-read [owner] null)) (def named-review-reserved (NamedReviewReserved. 7)) (deftype NamedReviewHyphen [some-field]) (def named-review-hyphen (NamedReviewHyphen. 9))").unwrap();
    for source in [
        "(.-null named-review-reserved)",
        "(.-null$ named-review-reserved)",
        "(set! (.-null named-review-reserved) 11)",
        "(set! (.-null$ named-review-reserved) 11)",
        "(.-some_field named-review-hyphen)",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Language(_))),
            "{source}"
        );
        session.collect().unwrap();
    }
    let value = session
        .eval("(named-review-read named-review-reserved)")
        .unwrap();
    session
        .inspect(&value, |mut store, value| {
            let fields = value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap()
                .fields(&mut store)?
                .collect::<Vec<_>>();
            assert!(matches!(fields.as_slice(), [Val::F64(bits)] if *bits == 7.0f64.to_bits()));
            Ok(())
        })
        .unwrap();
}

#[test]
fn unfinished_prototype_and_callable_attributes_are_explicit_errors() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session.eval("(deftype NamedHostBoundary [value]) (def named-host-object (NamedHostBoundary. 7)) (def named-host-function (fn [x] x))").unwrap();
    for source in [
        "(.-prototype NamedHostBoundary)",
        "(.-length named-host-function)",
        "(.-name named-host-function)",
        "(.-constructor named-host-object)",
        "(.-__proto__ named-host-object)",
        "(.-bind named-host-function)",
        "(set! (.-length named-host-function) 3)",
        "(set! (.-prototype NamedHostBoundary) named-host-object)",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Language(_))),
            "{source}"
        );
        session.collect().unwrap();
    }
    // Declared own fields still take precedence over an inherited method name.
    session.eval("(deftype NamedOwnHostSpelling [toString]) (def named-own-host-spelling (NamedOwnHostSpelling. 19)) (set! (.-toString named-own-host-spelling) 23)").unwrap();
    let value = session
        .eval("(.-toString named-own-host-spelling)")
        .unwrap();
    session
        .inspect(&value, |mut store, value| {
            let fields = value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap()
                .fields(&mut store)?
                .collect::<Vec<_>>();
            assert!(matches!(fields.as_slice(), [Val::F64(bits)] if *bits == 23.0f64.to_bits()));
            Ok(())
        })
        .unwrap();
}
