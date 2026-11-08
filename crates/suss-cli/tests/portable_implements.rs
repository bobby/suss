use suss_cli::portable_session::{RuntimeNumber, Session, SessionValue};

fn number(session: &mut Session, value: &SessionValue) -> RuntimeNumber {
    session
        .inspect(value, |store, value| {
            Ok(suss_cli::portable_session::runtime_number(store, &value)?.expect("Number layout"))
        })
        .unwrap()
}
fn eval_number(session: &mut Session, source: &str) -> RuntimeNumber {
    let value = session.eval(source).unwrap();
    number(session, &value)
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
fn eval_bool(session: &mut Session, source: &str) -> bool {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |mut store, value| {
            match suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap()
            {
                2 => Ok(false),
                4 => Ok(true),
                other => panic!("Boolean {other}"),
            }
        })
        .unwrap()
}

#[test]
fn implements_matches_independently_encoded_primary_observations() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/implements-cases.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 33);
    let mut session = Session::new().unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        assert!(ids.insert(id));
        let source = case["source"].as_str().unwrap();
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "bool" => serde_json::json!({"tag":"bool", "value":eval_bool(&mut session, source)}),
            "f64" => {
                number_json(eval_number(&mut session, source))
            }
            tag => panic!("unexpected tag {tag}"),
        };
        assert_eq!(actual, clojure_number(&case["expected"]), "{id}: {source}");
        session.collect().unwrap();
    }
}

#[test]
fn implements_aliases_user_refers_exclusions_and_gc_preserve_direct_only_test() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("implementation_provider.sus"),
        "(ns implementation-provider) (def implements? (fn [p x] 77))",
    )
    .unwrap();
    let mut session = Session::with_options(suss_cli::portable_session::SessionOptions {
        source_paths: vec![root.path().into()],
        ..Default::default()
    })
    .unwrap();
    session.eval("(ns implementation-referrer (:require [cljs.core :as c] [implementation-provider :as p :refer [implements?]])) (defprotocol Probe (read [x])) (deftype Item [] Probe (read [x] 1)) (deftype Bare []) (def item (Item.)) (def direct (fn [x] (c/implements? Probe x))) (extend-type default Probe (read [x] 9))").unwrap();
    let direct = session.eval("direct").unwrap();
    let item = session.eval("item").unwrap();
    session.collect().unwrap();
    assert!(eval_bool(&mut session, "(implements? Probe item)"));
    assert_eq!(
        eval_number(&mut session, "(p/implements? Probe item)"),
        RuntimeNumber::Long(77)
    );
    assert!(eval_bool(&mut session, "(c/implements? Probe item)"));
    assert!(!eval_bool(&mut session, "(c/implements? Probe (Bare.))"));
    assert!(eval_bool(&mut session, "(satisfies? Probe (Bare.))"));
    session.eval("(def direct nil) (def item nil)").unwrap();
    session.collect().unwrap();
    let result = session.invoke(&direct, &[&item]).unwrap();
    assert!(
        session
            .inspect(&result, |mut store, value| Ok(suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap()
                == 4))
            .unwrap()
    );
    session
        .eval("(ns implementation-excluded (:refer-clojure :exclude [implements?]))")
        .unwrap();
    assert!(matches!(
        session.eval("(implements? nil nil)"),
        Err(suss_cli::portable_session::SessionError::Compile(_))
    ));
    assert!(!eval_bool(
        &mut session,
        "(cljs.core/implements? implementation-referrer/Probe nil)"
    ));
}

#[test]
fn implements_rejects_malformed_macro_calls_and_recovers_after_throw() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval("(defprotocol Probe (read [x])) (deftype Item [] Probe (read [x] 1))")
        .unwrap();
    for source in [
        "(implements?)",
        "(implements? Probe)",
        "(implements? Probe nil 1)",
        "(implements? (do Probe) nil)",
        "(implements? Missing nil)",
        "implements?",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Compile(_))),
            "{source}"
        );
    }
    assert!(matches!(
        session.eval("(implements? Probe (throw 7))"),
        Err(SessionError::Language(_))
    ));
    session.collect().unwrap();
    assert!(eval_bool(&mut session, "(implements? Probe (Item.))"));
}
