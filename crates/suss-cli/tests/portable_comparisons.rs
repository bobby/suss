use suss_cli::portable_session::{Session, SessionError, SessionValue};
use wasmtime::Val;

fn number(session: &mut Session, value: &SessionValue) -> u64 {
    session
        .inspect(value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Number layout")
            };
            Ok(*bits)
        })
        .unwrap()
}
fn eval_number(session: &mut Session, source: &str) -> u64 {
    let value = session.eval(source).unwrap();
    number(session, &value)
}
fn eval_bool(session: &mut Session, source: &str) -> bool {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |store, value| {
            match value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32()
            {
                2 => Ok(false),
                4 => Ok(true),
                other => panic!("Boolean {other}"),
            }
        })
        .unwrap()
}

#[test]
fn comparisons_match_independently_encoded_primary_corpus() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/comparison-cases.json")).unwrap();
    let mut session = Session::new().unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for case in corpus["cases"].as_array().unwrap() {
        assert!(ids.insert(case["id"].as_str().unwrap()));
        let source = case["source"].as_str().unwrap();
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "bool" => serde_json::json!({"tag":"bool", "value":eval_bool(&mut session, source)}),
            "f64" => {
                serde_json::json!({"tag":"f64", "bits":format!("{:016x}",eval_number(&mut session, source))})
            }
            tag => panic!("unexpected tag {tag}"),
        };
        assert_eq!(actual, case["expected"], "{source}");
        session.collect().unwrap();
    }
    assert_eq!(ids.len(), 123);
}

#[test]
fn retained_comparison_captures_follow_accepted_design_with_explicit_primary_divergence() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/comparison-capture-divergences.json"
    ))
    .unwrap();
    let mut session = Session::new().unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        assert_eq!(case["expected-primary"]["tag"], "string");
        assert_eq!(
            eval_bool(&mut session, case["source"].as_str().unwrap()),
            case["expected-native"]["value"].as_bool().unwrap()
        );
        session.collect().unwrap();
    }
}

#[test]
fn comparison_macro_arity_erasure_and_lexical_shadowing_remain_distinct_from_values() {
    let mut session = Session::new().unwrap();
    for name in ["<", "<=", ">", ">=", "=="] {
        assert!(eval_bool(&mut session, &format!("({name} (throw 7))")));
        assert!(matches!(
            session.eval(&format!("({name})")),
            Err(SessionError::Compile(_))
        ));
        let original = session.eval(name).unwrap();
        session.collect().unwrap();
        assert!(matches!(
            session.invoke(&original, &[]),
            Err(SessionError::Language(_))
        ));
        assert_eq!(
            eval_number(
                &mut session,
                &format!("(let [{name} (fn [x] 77)] ({name} 1))")
            ),
            77.0f64.to_bits()
        );
        let argument = session.eval("##NaN").unwrap();
        let result = session.invoke(&original, &[&argument]).unwrap();
        let sentinel = session
            .inspect(&result, |store, value| {
                Ok(value
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .unwrap()
                    .get_u32())
            })
            .unwrap();
        assert_eq!(sentinel, 4);
    }
}

#[test]
fn comparison_macro_and_runtime_names_preserve_aliases_user_shadowing_and_gc() {
    let mut session = Session::new().unwrap();
    // Explicit refers conflict with an own declaration under the accepted
    // namespace contract. Auto-referred core values permit user shadowing.
    assert!(matches!(
        session.eval(
            "(ns comparison-conflict (:require [cljs.core :refer [<]])) (def < (fn [x y] 77))"
        ),
        Err(SessionError::Compile(_))
    ));
    session
        .eval("(ns comparison-consumer (:require [cljs.core :as c])) (def retained <)")
        .unwrap();
    session.eval("(def < (fn [x y] 77))").unwrap();
    assert_eq!(eval_number(&mut session, "(< 1 2)"), 77.0f64.to_bits());
    assert!(eval_bool(&mut session, "(c/< 1 2)"));
    assert!(eval_bool(&mut session, "(let [f retained] (f 1 2))"));
    let function = session.eval("retained").unwrap();
    session.eval("(def retained nil)").unwrap();
    session.collect().unwrap();
    let left = session.eval("1").unwrap();
    let right = session.eval("2").unwrap();
    let value = session.invoke(&function, &[&left, &right]).unwrap();
    let sentinel = session
        .inspect(&value, |store, value| {
            Ok(value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32())
        })
        .unwrap();
    assert_eq!(sentinel, 4);
    session
        .eval("(ns comparison-excluded (:refer-clojure :exclude [<]))")
        .unwrap();
    assert!(matches!(
        session.eval("(< 1 2)"),
        Err(SessionError::Compile(_))
    ));
    assert!(eval_bool(&mut session, "(cljs.core/< 1 2)"));
}

#[test]
fn comparison_utf16_prefix_loop_has_fuel_recovery_and_typed_coercion_errors() {
    let mut session = Session::new().unwrap();
    let function = session.eval("<").unwrap();
    let text = "😀".repeat(4096);
    let left = session.eval(&format!("\"{text}\"")).unwrap();
    let right = session.eval(&format!("\"{text}x\"")).unwrap();
    session.collect().unwrap();
    session.set_operation_fuel(5000);
    let SessionError::Trap(error) = session.invoke(&function, &[&left, &right]).unwrap_err() else {
        panic!("expected UTF-16 comparison fuel exhaustion")
    };
    assert_eq!(
        error.downcast_ref::<wasmtime::Trap>(),
        Some(&wasmtime::Trap::OutOfFuel)
    );
    session.collect().unwrap();
    session.set_operation_fuel(1_000_000);
    let answer = session.invoke(&function, &[&left, &right]).unwrap();
    let sentinel = session
        .inspect(&answer, |store, value| {
            Ok(value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32())
        })
        .unwrap();
    assert_eq!(sentinel, 4);
    session.eval("(deftype ComparisonBad [])").unwrap();
    assert!(matches!(
        session.eval("(< (ComparisonBad.) 2)"),
        Err(SessionError::Language(_))
    ));
    assert!(eval_bool(&mut session, "(< 1 2)"));
}

#[test]
fn runtime_comparison_large_arity_uses_bounded_callback_code_without_macro_expansion() {
    let mut session = Session::new().unwrap();
    let args = (0..300)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(eval_bool(&mut session, &format!("(let [f <] (f {args}))")));
    assert!(matches!(
        session.eval(&format!("(< {args})")),
        Err(SessionError::Compile(_))
    ));
    assert!(eval_bool(&mut session, "(< 1 2)"));
}

#[test]
fn explicit_user_refers_preserve_pinned_core_macros_but_aliases_call_user_vars() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("comparison_provider.sus"), "(ns comparison-provider) (def < (fn [x y] 77)) (def <= (fn [x y] 78)) (def > (fn [x y] 79)) (def >= (fn [x y] 80)) (def == (fn [x y] 81))").unwrap();
    let mut session = Session::with_options(suss_cli::portable_session::SessionOptions {
        source_paths: vec![root.path().into()],
        ..Default::default()
    })
    .unwrap();
    session
        .eval(
            "(ns comparison-referrer (:require [comparison-provider :as p :refer [< <= > >= ==]]))",
        )
        .unwrap();
    session.collect().unwrap();
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/comparison-refers.json")).unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "bool" => serde_json::json!({"tag":"bool", "value":eval_bool(&mut session, source)}),
            "f64" => {
                serde_json::json!({"tag":"f64", "bits":format!("{:016x}",eval_number(&mut session, source))})
            }
            tag => panic!("unexpected referral tag {tag}"),
        };
        assert_eq!(actual, case["expected"], "{source}");
    }
    assert!(eval_bool(&mut session, "(cljs.core/< 1 2)"));
}
