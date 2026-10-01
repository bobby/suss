use suss_cli::portable_session::Session;
use wasmtime::Val;

#[test]
fn string_cache_match_independently_decoded_primary_observations_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/string-cache-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 64);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session
        .enter_namespace("suss-oracle.string-cache-cases")
        .unwrap();
    let mut ids = std::collections::BTreeSet::new();
    let mut failures = Vec::new();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        assert!(ids.insert(id));
        let source = case["source"].as_str().unwrap();
        let value = match session.eval(source) {
            Ok(value) => value,
            Err(error) => {
                failures.push(format!("{id}: {source}: {error:?}"));
                continue;
            }
        };
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
            tag => panic!("unsupported String hash observation {tag}"),
        };
        if actual != case["expected"] {
            failures.push(format!(
                "{id}: {source}: actual {actual}, expected {}",
                case["expected"]
            ));
        }
        session.collect().unwrap();
    }
    assert!(
        failures.is_empty(),
        "{} failing observations:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn cache_number(session: &mut Session, source: &str) -> f64 {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |mut store, value| {
            store.gc(None)?;
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            Ok(object.field(&mut store, 0)?.unwrap_f64())
        })
        .unwrap()
}

#[test]
fn string_cache_aliases_factories_and_native_members_remain_live() {
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session
        .eval("(ns cache.alias (:require [cljs.core :as c]))")
        .unwrap();
    assert_eq!(cache_number(&mut session, "(c/hash-string \"A\")"), 65.0);
    assert_eq!(
        cache_number(
            &mut session,
            "(let [f c/js-obj o (f \"x\" 7)] (c/unchecked-get o \"x\"))"
        ),
        7.0
    );
    assert_eq!(
        cache_number(
            &mut session,
            "(let [f c/js-obj o (f (array \"x\" 9))] (c/unchecked-get o \"x\"))"
        ),
        9.0
    );
    assert_eq!(
        cache_number(
            &mut session,
            "(let [o (c/js-obj)] (if (identical? (.valueOf o) o) 1 0))"
        ),
        1.0
    );
    assert_eq!(
        cache_number(
            &mut session,
            "(let [o (c/js-obj)] (if (.hasOwnProperty (.-__proto__ o) \"__proto__\") 1 0))"
        ),
        1.0
    );
    assert_eq!(
        cache_number(
            &mut session,
            "(let [o (c/js-obj)] (set! (.-x o) 11) (.-x o))"
        ),
        11.0
    );
    assert_eq!(cache_number(&mut session, "(let [saved c/hash-string] (with-redefs [c/string-hash-cache (c/js-obj) c/string-hash-cache-count 0 c/hash-string* (fn [k] 13)] (saved \"new\")))"), 13.0);
    assert_eq!(
        cache_number(
            &mut session,
            "(let [unchecked-get (fn [o k] 17)] (unchecked-get nil nil))"
        ),
        17.0
    );
}

#[test]
fn string_cache_errors_and_macro_arity_preserve_effects_and_recover() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    for source in [
        "(do (def cache_unpublished 1) (unchecked-get nil))",
        "(do (def cache_unpublished 1) (unchecked-set nil \"x\"))",
    ] {
        let error = session.eval(source).unwrap_err();
        assert!(matches!(error, SessionError::Compile(_)), "{error:?}");
        assert!(session.eval("cache_unpublished").is_err());
    }
    session.eval("(def factory_effects 0)").unwrap();
    let error = session
        .eval("(let [f js-obj] (f (do (set! factory_effects 7) \"odd\")))")
        .unwrap_err();
    assert!(matches!(error, SessionError::Language(_)), "{error:?}");
    assert_eq!(cache_number(&mut session, "factory_effects"), 7.0);
    for source in [
        "(hash-string)",
        "(hash-string \"x\" \"y\")",
        "(add-to-string-hash-cache)",
        "(unchecked-get nil \"x\")",
    ] {
        let error = session.eval(source).unwrap_err();
        assert!(
            matches!(error, SessionError::Language(_)),
            "{source}: {error:?}"
        );
    }
    assert_eq!(cache_number(&mut session, "(hash-string \"AB\")"), 2081.0);
}
