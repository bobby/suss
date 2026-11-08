use suss_cli::portable_session::{RuntimeNumber, Session, runtime_number};
use suss_compile::portable_session::sentinel_tag;
use wasmtime::{StoreContextMut, Val};

/// The long `value` holds; panics on any other value.
fn long(store: &mut StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<i64> {
    match runtime_number(store, value)? {
        Some(RuntimeNumber::Long(value)) => Ok(value),
        other => panic!("long result, got {other:?}"),
    }
}

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
fn named_properties_match_independently_decoded_primary_observations_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/named-property-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 64);
    let mut session = Session::new().unwrap();
    // Ported core reads other types' private fields, so it runs as core (#278).
    session
        .eval_privileged(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session
        .enter_namespace("suss-oracle.named-property-cases")
        .unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        assert!(ids.insert(id));
        // Clojure throws for a long divided by zero; a double divisor still
        // gives the NaN marker this case probes.
        let source = &case["source"].as_str().unwrap().replace("(/ 0 0)", "(/ 0.0 0)");
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
                            sentinel_tag(&mut store, &value)?.unwrap(),
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
                        let sentinel = sentinel_tag(&mut store, &value)?.unwrap();
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
        assert_eq!(actual, clojure_number(&case["expected"]), "{id}: {source}");
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
        "(.-1cache named-owner)",
        "(set! (.-1cache named-owner) 3)",
        "(set! (.-cache) 3)",
        "(set! (.-cache nil) 3)",
        "(do (def property-should-not-publish 17) (.-1cache named-owner))",
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
    // Canonical munging makes hyphenated spellings legitimate host names;
    // an absent closure property reads as canonical undefined, not an error.
    let absent = session
        .eval("(.-bad-name named-owner)")
        .unwrap();
    session.collect().unwrap();
    session
        .inspect(&absent, |mut store, value| {
            assert_eq!(
                sentinel_tag(&mut store, &value)?.expect("undefined ABI"),
                6,
                "absent hyphenated property must be canonical undefined after GC"
            );
            Ok(())
        })
        .unwrap();
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
                assert_eq!(long(&mut store, &value)?, 0);
                Ok(())
            })
            .unwrap();
    }
    let removed = session.eval("(aget named-array 0)").unwrap();
    session.collect().unwrap();
    session
        .inspect(&removed, |mut store, value| {
            assert_eq!(
                sentinel_tag(&mut store, &value)?.unwrap(),
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
            assert_eq!(long(&mut store, &value)?, 39);
            Ok(())
        })
        .unwrap();
}

#[test]
fn named_access_uses_canonical_storage_without_changing_raw_lexical_names() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session.eval("(defprotocol NamedReviewRead (named-review-read [owner])) (deftype NamedReviewReserved [null] NamedReviewRead (named-review-read [owner] null)) (def named-review-reserved (NamedReviewReserved. 7)) (deftype NamedReviewHyphen [some-field]) (def named-review-hyphen (NamedReviewHyphen. 9))").unwrap();
    assert!(matches!(session.eval("(set! (.-null named-review-reserved) 11)"),
        Err(SessionError::Language(_))), "extra instance properties remain unsupported");
    session.collect().unwrap();
    let missing = session.eval("(.-null named-review-reserved)").unwrap();
    session.collect().unwrap();
    session.inspect(&missing, |mut store, value| {
        assert_eq!(sentinel_tag(&mut store, &value)?.expect("undefined ABI"), 6,
            "raw absent property must be canonical undefined after GC");
        Ok(())
    }).unwrap();
    for (source, expected) in [
        ("(.-null$ named-review-reserved)", 7),
        ("(.-some_field named-review-hyphen)", 9),
    ] {
        let value = session.eval(source).unwrap();
        session.collect().unwrap();
        session.inspect(&value, |mut store, value| {
            assert_eq!(long(&mut store, &value)?, expected, "{source}");
            Ok(())
        }).unwrap();
    }
    // Fields not declared mutable are final, under their munged name too (#278).
    assert!(matches!(session.eval("(set! (.-null$ named-review-reserved) 11)"),
        Err(SessionError::Language(_))), "immutable field write");
    session.collect().unwrap();
    let value = session
        .eval("(named-review-read named-review-reserved)")
        .unwrap();
    session
        .inspect(&value, |mut store, value| {
            assert_eq!(long(&mut store, &value)?, 7);
            Ok(())
        })
        .unwrap();
}

#[test]
fn constructor_prototype_is_stable_and_callable_attributes_remain_explicit_errors() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session.eval("(deftype NamedHostBoundary [value]) (def named-host-object (NamedHostBoundary. 7)) (def named-host-function (fn [x] x))").unwrap();
    let stable_prototype = session
        .eval("(identical? (.-prototype NamedHostBoundary) (.-prototype NamedHostBoundary))")
        .unwrap();
    session.inspect(&stable_prototype, |mut store, value| {
        assert_eq!(sentinel_tag(&mut store, &value)?.unwrap(), 4);
        Ok(())
    }).unwrap();
    let constructor = session
        .eval("(identical? (.-constructor named-host-object) NamedHostBoundary)")
        .unwrap();
    session.inspect(&constructor, |mut store, value| {
        assert_eq!(sentinel_tag(&mut store, &value)?.unwrap(), 4);
        Ok(())
    }).unwrap();
    for source in [
        "(.-length named-host-function)",
        "(.-name named-host-function)",
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
    session.eval("(deftype NamedOwnHostSpelling [toString]) (def named-own-host-spelling (NamedOwnHostSpelling. 23))").unwrap();
    let value = session
        .eval("(.-toString named-own-host-spelling)")
        .unwrap();
    session
        .inspect(&value, |mut store, value| {
            assert_eq!(long(&mut store, &value)?, 23);
            Ok(())
        })
        .unwrap();
}
