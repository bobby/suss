use suss_cli::portable_session::Session;

/// An exact runtime number: a long, or a double by its bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Number {
    Long(i64),
    Double(u64),
}
/// The expected number for `value`: integral values are longs, as in Clojure.
fn num(value: f64) -> Number {
    if value.is_finite()
        && value.fract() == 0.0
        && !(value == 0.0 && value.is_sign_negative())
        && value.abs() < 9_223_372_036_854_775_808.0
    {
        Number::Long(value as i64)
    } else {
        Number::Double(value.to_bits())
    }
}
/// The pinned ClojureScript observation with each integral `f64` as a Clojure
/// long, a distinction that corpus cannot record.
fn clojure_longs(expected: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match expected {
        Value::Object(fields) if fields.get("tag") == Some(&Value::from("f64")) => {
            let bits = u64::from_str_radix(fields["bits"].as_str().unwrap(), 16).unwrap();
            let value = f64::from_bits(bits);
            if value.is_finite()
                && value.fract() == 0.0
                && !(value == 0.0 && value.is_sign_negative())
                && value.abs() < 9_223_372_036_854_775_808.0
            {
                serde_json::json!({"tag":"long", "value":value as i64})
            } else {
                expected.clone()
            }
        }
        Value::Object(fields) => Value::Object(
            fields.iter().map(|(key, value)| (key.clone(), clojure_longs(value))).collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(clojure_longs).collect()),
        other => other.clone(),
    }
}
fn exact_number(store: impl wasmtime::AsContextMut, value: &wasmtime::Val) -> wasmtime::Result<Number> {
    Ok(
        match suss_compile::portable_session::runtime_number(store, value)?
            .expect("expected a runtime number")
        {
            suss_compile::portable_session::RuntimeNumber::Long(value) => Number::Long(value),
            suss_compile::portable_session::RuntimeNumber::Double(value) => {
                Number::Double(value.to_bits())
            }
        },
    )
}

/// Whether `actual` matches a pinned `f64` observation. The ClojureScript
/// corpus cannot distinguish longs: an integral observation is a Clojure long,
/// or an integral double when the source has a double operand.
fn matches_pinned(actual: Number, expected: &serde_json::Value, source: &str) -> bool {
    let bits = u64::from_str_radix(expected["bits"].as_str().unwrap(), 16).unwrap();
    match actual {
        Number::Long(_) => actual == num(f64::from_bits(bits)),
        Number::Double(actual) => {
            actual == bits
                && (num(f64::from_bits(bits)) == Number::Double(bits)
                    || source.contains("##")
                    || source.as_bytes().windows(3).any(|w| {
                        w[0].is_ascii_digit()
                            && (w[1] == b'.' || w[1] == b'e')
                            && (w[2].is_ascii_digit() || w[2] == b'-')
                    }))
        }
    }
}
/// The pinned observation when `actual` matches it, otherwise a distinct value.
fn number_observation(actual: Number, expected: &serde_json::Value, source: &str) -> serde_json::Value {
    if matches_pinned(actual, expected, source) {
        expected.clone()
    } else {
        serde_json::json!({ "tag": "number", "actual": format!("{actual:?}") })
    }
}

#[test]
fn caching_hash_match_independently_decoded_primary_observations_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/caching-hash-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 38);
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
        // Clojure governs: a long division by zero throws, so the NaN case
        // takes the double NaN.
        let source = &case["source"].as_str().unwrap().replace("(/ 0 0)", "##NaN");
        let value = session
            .eval(source)
            .unwrap_or_else(|error| panic!("{id}: {source}: {error:?}"));
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "f64" => {
                number_observation(
                    session
                        .inspect(&value, |mut store, value| exact_number(&mut store, &value))
                        .unwrap(),
                    &case["expected"],
                    &source,
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
            tag => panic!("unsupported Caching-hash observation {tag}"),
        };
        assert!(
            hash_observation_matches(id, &actual, &case["expected"]),
            "{id}: {source}: actual {actual}, expected {}",
            case["expected"]
        );
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
        let suss_cli::portable_session::SessionError::Compile(diagnostic) = error else {
            panic!("{source}: {error:?}");
        };
        assert!(
            diagnostic.span.start < diagnostic.span.end,
            "{source}: {diagnostic:?}"
        );
        assert!(
            diagnostic.span.end <= source.len(),
            "{source}: {diagnostic:?}"
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
                assert_eq!(exact_number(&mut store, &value)?, num(79.0));
                Ok(())
            })
            .unwrap();
    }
}

#[test]
fn hash_macro_aliases_exclusions_and_phase_are_resolved_explicitly() {
    use suss_cli::portable_session::SessionError;
    use suss_compile::portable::{
        prepare_fragment,
        resolve::{Environment, Phase},
    };
    let mut session = Session::new().unwrap();
    session
        .eval("(ns hash.alias (:require [cljs.core :as c]))")
        .unwrap();
    let value = session
        .eval("(def cached nil) (c/caching-hash 7 (fn [x] (* x 3)) cached)")
        .unwrap();
    session
        .inspect(&value, |mut store, value| {
            assert_eq!(exact_number(&mut store, &value)?, num(21.0));
            Ok(())
        })
        .unwrap();
    session
        .eval("(ns hash.excluded (:refer-clojure :exclude [caching-hash])) (def cached nil)")
        .unwrap();
    assert!(matches!(
        session.eval("(caching-hash 7 (fn [x] x) cached)"),
        Err(SessionError::Compile(_))
    ));
    session
        .eval("(cljs.core/caching-hash 7 (fn [x] x) cached)")
        .unwrap();
    for phase in [Phase::Runtime, Phase::Macro] {
        prepare_fragment(
            "(def cached nil) (cljs.core/caching-hash 7 (fn [x] x) cached)",
            &Environment::default(),
            phase,
        )
        .unwrap();
    }
}

// Arithmetic canonical NaN has an unspecified sign (accepted design evidence
// clarification). Keep the decoder's raw bits and allow only that case's sign;
// all payloads, signed zeros and other observations still compare exactly.
fn hash_observation_matches(
    id: &str,
    actual: &serde_json::Value,
    expected: &serde_json::Value,
) -> bool {
    if actual == expected {
        return true;
    }
    if id != "nan-is-cached" || actual["tag"] != "f64" || expected["tag"] != "f64" {
        return false;
    }
    let bits = |value: &serde_json::Value| {
        value["bits"]
            .as_str()
            .and_then(|s| u64::from_str_radix(s, 16).ok())
    };
    matches!((bits(actual), bits(expected)), (Some(a), Some(b)) if a & 0x7fffffffffffffff == 0x7ff8000000000000 && b & 0x7fffffffffffffff == 0x7ff8000000000000)
}

#[test]
fn canonical_nan_comparison_is_narrow_and_cache_storage_preserves_both_signs() {
    let observed = |bits: &str| serde_json::json!({"tag":"f64", "bits":bits});
    let positive = observed("7ff8000000000000");
    assert!(hash_observation_matches(
        "nan-is-cached",
        &observed("fff8000000000000"),
        &positive
    ));
    for bits in [
        "7ff8000000000001",
        "fff8000000000001",
        "7ff0000000000000",
        "0000000000000000",
        "8000000000000000",
    ] {
        assert!(!hash_observation_matches(
            "nan-is-cached",
            &observed(bits),
            &positive
        ));
    }
    assert!(!hash_observation_matches(
        "other-storage-case",
        &observed("fff8000000000000"),
        &positive
    ));
    assert!(!hash_observation_matches(
        "nan-is-cached",
        &observed("8000000000000000"),
        &observed("0000000000000000")
    ));
    let mut session = Session::new().unwrap();
    session.eval("(def cache nil)").unwrap();
    fn read_bits(session: &mut Session, value: &suss_cli::portable_session::SessionValue) -> u64 {
        session
            .inspect(value, |mut store, value| {
                let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                Ok(object
                    .fields(&mut store)?
                    .next()
                    .unwrap()
                    .unwrap_f64()
                    .to_bits())
            })
            .unwrap()
    }
    let mut signs = std::collections::BTreeSet::new();
    for source in ["##NaN", "(- ##NaN)"] {
        let before = session
            .eval(&format!("(set! cache {source}) cache"))
            .unwrap();
        let before_bits = read_bits(&mut session, &before);
        assert_eq!(before_bits & 0x7fffffffffffffff, 0x7ff8000000000000);
        signs.insert(before_bits);
        let hit = session
            .eval("(cljs.core/caching-hash (throw 31) (throw 37) cache)")
            .unwrap();
        assert_eq!(read_bits(&mut session, &hit), before_bits);
        session.collect().unwrap();
        let stored = session.eval("cache").unwrap();
        assert_eq!(read_bits(&mut session, &stored), before_bits);
    }
    assert_eq!(
        signs,
        [0x7ff8000000000000, 0xfff8000000000000]
            .into_iter()
            .collect()
    );
}

#[test]
fn caching_hash_invokes_callable_objects_and_skips_hit_effects_in_both_phases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/caching-hash-callable-case.json"
    ))
    .unwrap();
    let fixture = &fixture["cases"][0];
    let mut failures = Vec::new();
    for (phase, mut session) in [
        ("runtime", Session::new_repl().unwrap()),
        ("macro", Session::new_macro().unwrap()),
    ] {
        let bridge = suss_cli::portable_macro_data::FormBridge::new(&mut session).unwrap();
        match session.eval(fixture["source"].as_str().unwrap()) {
            Ok(value) => {
                session.collect().unwrap();
                let form = bridge.read(&mut session, &value, 0..0).unwrap();
                let suss_reader::forms::Kind::Vector(items) = &form.kind else {
                    panic!("Expected vector")
                };
                let items: Vec<_> = items.iter().map(|item| match &item.kind {
                    suss_reader::forms::Kind::Long(value) => {
                        serde_json::json!({"tag":"long", "value":value})
                    }
                    suss_reader::forms::Kind::Number(number) => {
                        serde_json::json!({"tag":"f64", "bits":format!("{:016x}", number.to_bits())})
                    }
                    _ => panic!("Expected number"),
                }).collect();
                let actual = serde_json::json!({"tag":"vector", "items":items});
                if actual != clojure_longs(&fixture["expected"]) {
                    failures.push(format!("{phase}: {actual}"));
                }
            }
            Err(error) => failures.push(format!("{phase}: {error}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
