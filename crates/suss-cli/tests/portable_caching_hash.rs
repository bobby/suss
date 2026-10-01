use suss_cli::portable_session::Session;
use wasmtime::Val;

#[test]
fn caching_hash_match_independently_decoded_primary_observations_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/caching-hash-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 24);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session
        .enter_namespace("suss-oracle.caching-hash-cases")
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
            tag => panic!("unsupported Caching-hash observation {tag}"),
        };
        assert_eq!(actual, case["expected"], "{id}: {source}");
        session.collect().unwrap();
    }
}

#[test]
fn malformed_cache_expansions_fail_atomically_and_session_recovers() {
    let mut session = Session::new().unwrap();
    for body in [
        "(cljs.core/caching-hash 1 (fn [x] x))",
        "(cljs.core/caching-hash 1 (fn [x] x) 3)",
        "(cljs.core/caching-hash 1 (fn [x] x) cache-private)",
        "(let [cache-local nil] (cljs.core/caching-hash 1 (fn [x] x) cache-local))",
        "(deftype CacheInvalid [cached] Object (read [this] (cljs.core/caching-hash this (fn [x] 7) cached)))",
        "(deftype CacheInvalid [^:mutable cached] Object (read [this cached] (cljs.core/caching-hash this (fn [x] 7) cached)))",
    ] {
        let source = format!("(do (def cache-unpublished 3) {body})");
        let error = session.eval(&source).unwrap_err();
        assert!(
            matches!(error, suss_cli::portable_session::SessionError::Compile(_)),
            "{source}: {error:?}"
        );
        for name in ["cache-unpublished", "CacheInvalid", "->CacheInvalid"] {
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
