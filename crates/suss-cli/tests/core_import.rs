//! Execute the generated, provenance-checked upstream bootstrap form.
use suss_cli::portable_session::{Session, SessionError, SessionValue};
use wasmtime::Rooted;

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

const CORE: &str = include_str!("../../../runtime/core-import/suss/core.sus");

fn loaded() -> Session {
    let mut session = Session::new().unwrap();
    assert!(matches!(
        session.eval("(identity 7)"),
        Err(SessionError::Compile(_))
    ));
    session.eval_privileged(CORE).unwrap();
    session.enter_namespace("user").unwrap();
    session
}

fn number(session: &mut Session, value: &SessionValue) -> Number {
    session
        .inspect(value, |mut store, value| {
            exact_number(&mut store, &value)
        })
        .unwrap()
}

#[test]
fn imported_identity_preserves_exact_values_and_object_identity_across_gc() {
    let mut session = loaded();
    session.eval("(deftype Item [value])").unwrap();
    let identity = session.eval("identity").unwrap();
    for source in [
        "nil",
        "false",
        "true",
        "-0.0",
        "##NaN",
        "##Inf",
        "\"\\uD800😀\"",
        "(fn [] 42)",
        "(Item. 7)",
        "(ex-info \"saved\" 9)",
    ] {
        let original = session.eval(source).unwrap();
        let root = session
            .inspect(&original, |store, value| {
                // ABI v3 nil is the null reference: there is nothing to root.
                value.unwrap_anyref().map(|value| value.to_owned_rooted(store)).transpose()
            })
            .unwrap();
        session.collect().unwrap();
        let returned = session.invoke(&identity, &[&original]).unwrap();
        session.collect().unwrap();
        assert!(
            session
                .inspect(&returned, |store, value| match (&root, value.unwrap_anyref()) {
                    (None, None) => Ok(true),
                    (Some(root), Some(value)) => Rooted::ref_eq(&store, root, value),
                    _ => Ok(false),
                })
                .unwrap(),
            "{source}"
        );
    }
}

#[test]
fn imported_core_aliases_and_captured_function_observe_live_redefinition() {
    let mut session = loaded();
    let original = session.eval("identity").unwrap();
    let root = session
        .inspect(&original, |store, value| {
            value.unwrap_anyref().unwrap().to_owned_rooted(store)
        })
        .unwrap();
    for source in [
        "cljs.core/identity",
        "suss.core/identity",
        "(let [f identity] f)",
    ] {
        let alias = session.eval(source).unwrap();
        assert!(
            session
                .inspect(&alias, |store, value| Rooted::ref_eq(
                    &store,
                    &root,
                    value.unwrap_anyref().unwrap()
                ))
                .unwrap()
        );
    }
    session
        .eval("(ns app (:require [cljs.core :as core])) (def call (fn [x] (core/identity x)))")
        .unwrap();
    session.enter_namespace("suss.core").unwrap();
    session.eval("(def identity (fn [x] 9))").unwrap();
    session.enter_namespace("app").unwrap();
    session.collect().unwrap();
    let value = session.eval("(call 7)").unwrap();
    assert_eq!(number(&mut session, &value), num(9.0));
    let arg = session.eval("7").unwrap();
    let value = session.invoke(&original, &[&arg]).unwrap();
    assert_eq!(number(&mut session, &value), num(7.0));
}

#[test]
fn imported_identity_evaluates_arguments_once_and_recovers_from_wrong_arity() {
    let mut session = loaded();
    session
        .eval("(def count 0) (def next (fn [] (def count (+ count 1))))")
        .unwrap();
    let value = session.eval("(identity (next))").unwrap();
    assert_eq!(number(&mut session, &value), num(1.0));
    assert!(matches!(
        session.eval("(identity (next) (next))"),
        Err(SessionError::Language(_))
    ));
    let value = session.eval("count").unwrap();
    assert_eq!(number(&mut session, &value), num(3.0));
    assert!(matches!(
        session.eval("(identity)"),
        Err(SessionError::Language(_))
    ));
    let value = session.eval("(identity 42)").unwrap();
    assert_eq!(number(&mut session, &value), num(42.0));
}

#[test]
fn imported_core_matches_independently_decoded_pinned_scalar_corpus() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/core-import-cases.json")).unwrap();
    let mut session = loaded();
    let mut ids = std::collections::BTreeSet::new();
    // Clojure governs (#286): inc and dec reject non-number operands, which the
    // pinned ClojureScript observations coerce.
    let throwing = [
        "inc-nil", "inc-false", "inc-true", "inc-numeric-string", "inc-empty-string",
        "inc-utf16-string", "inc-undefined", "inc-nonnumeric-string", "dec-nil", "dec-false",
        "dec-true", "dec-numeric-string", "dec-empty-string", "dec-undefined",
        "dec-nonnumeric-string",
    ];
    for case in corpus["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        assert!(ids.insert(id));
        let source = case["source"].as_str().unwrap();
        if throwing.contains(&id) {
            assert!(
                matches!(session.eval(source), Err(SessionError::Language(_))),
                "{id}: {source}"
            );
            continue;
        }
        let value = session.eval(source).unwrap();
        session.collect().unwrap();
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "f64" => {
                let actual = number(&mut session, &value);
                // Clojure governs: longs are exact past 2^53, where the pinned
                // ClojureScript doubles round.
                let exact = [
                    ("inc-rounding", 9_007_199_254_740_993),
                    ("inc-negative-rounding", -9_007_199_254_740_991),
                    ("dec-rounding", 9_007_199_254_740_991),
                    ("dec-negative-rounding", -9_007_199_254_740_993),
                ];
                match exact.iter().find(|(exact, _)| *exact == id) {
                    Some((_, long)) => {
                        assert_eq!(actual, Number::Long(*long), "{source}");
                        case["expected"].clone()
                    }
                    None => number_observation(actual, &case["expected"], source),
                }
            }
            "nil" | "bool" => {
                let sentinel = session
                    .inspect(&value, |mut store, value| {
                        Ok(suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap())
                    })
                    .unwrap();
                match sentinel {
                    0 => serde_json::json!({"tag":"nil"}),
                    2 => serde_json::json!({"tag":"bool", "value":false}),
                    4 => serde_json::json!({"tag":"bool", "value":true}),
                    _ => panic!("unexpected sentinel {sentinel}"),
                }
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
            tag => panic!("unsupported expected tag {tag}"),
        };
        assert_eq!(actual, case["expected"], "{source}");
    }
    assert_eq!(ids.len(), 117);
}

fn boolean(session: &mut Session, value: &SessionValue) -> bool {
    session
        .inspect(value, |mut store, value| {
            match suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap()
            {
                2 => Ok(false),
                4 => Ok(true),
                sentinel => panic!("unexpected Boolean {sentinel}"),
            }
        })
        .unwrap()
}

#[test]
fn imported_boolean_ports_use_cljs_truthiness_for_primitives_and_objects() {
    let mut session = loaded();
    session.eval("(deftype Item [value])").unwrap();
    for (source, truth) in [
        ("nil", false),
        ("false", false),
        ("true", true),
        ("0", true),
        ("-0.0", true),
        ("##NaN", true),
        ("##Inf", true),
        ("\"\"", true),
        ("\"\\uD800😀\"", true),
        ("(fn [] 42)", true),
        ("(Item. 7)", true),
        ("(ex-info \"m\" 7)", true),
        ("(ex-data (new ExceptionInfo \"m\"))", false),
        ("(ExceptionInfo)", true),
    ] {
        for (name, expected) in [("boolean", truth), ("not", !truth)] {
            let value = session.eval(&format!("({name} {source})")).unwrap();
            session.collect().unwrap();
            assert_eq!(boolean(&mut session, &value), expected, "{name}: {source}");
        }
    }
}

#[test]
fn imported_boolean_ports_keep_original_behavior_and_canonical_live_bindings() {
    let mut session = loaded();
    let old_not = session.eval("not").unwrap();
    let old_boolean = session.eval("boolean").unwrap();
    for name in ["not", "boolean"] {
        let original = session.eval(name).unwrap();
        let root = session
            .inspect(&original, |store, value| {
                value.unwrap_anyref().unwrap().to_owned_rooted(store)
            })
            .unwrap();
        let alias = session.eval(&format!("cljs.core/{name}")).unwrap();
        assert!(
            session
                .inspect(&alias, |store, value| Rooted::ref_eq(
                    &store,
                    &root,
                    value.unwrap_anyref().unwrap()
                ))
                .unwrap()
        );
    }
    session.eval("(def read (fn [x] (not x)))").unwrap();
    session.enter_namespace("suss.core").unwrap();
    session
        .eval("(def nil? (fn [x] false)) (def false? (fn [x] false))")
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.collect().unwrap();
    let value = session.eval("(not nil)").unwrap();
    assert!(boolean(&mut session, &value));
    let value = session.eval("(boolean false)").unwrap();
    assert!(!boolean(&mut session, &value));
    session.enter_namespace("suss.core").unwrap();
    session
        .eval("(def not (fn [x] false)) (def boolean (fn [x] false))")
        .unwrap();
    session.enter_namespace("user").unwrap();
    let value = session.eval("(read nil)").unwrap();
    assert!(!boolean(&mut session, &value));
    let nil = session.eval("nil").unwrap();
    let truthy = session.eval("0").unwrap();
    session.collect().unwrap();
    let value = session.invoke(&old_not, &[&nil]).unwrap();
    assert!(boolean(&mut session, &value));
    let value = session.invoke(&old_boolean, &[&truthy]).unwrap();
    assert!(boolean(&mut session, &value));
}

#[test]
fn imported_boolean_port_arguments_run_once_before_arity_errors() {
    let mut session = loaded();
    session
        .eval("(def count 0) (def next (fn [] (def count (+ count 1))))")
        .unwrap();
    for name in ["not", "boolean"] {
        let value = session.eval(&format!("({name} (next))")).unwrap();
        assert_eq!(boolean(&mut session, &value), name == "boolean");
        assert!(matches!(
            session.eval(&format!("({name} (next) (next))")),
            Err(SessionError::Language(_))
        ));
        assert!(matches!(
            session.eval(&format!("({name})")),
            Err(SessionError::Language(_))
        ));
    }
    let count = session.eval("count").unwrap();
    assert_eq!(number(&mut session, &count), num(6.0));
    let value = session.eval("(not nil)").unwrap();
    assert!(boolean(&mut session, &value));
}

#[test]
fn imported_boolean_callee_is_captured_before_argument_rebinding_and_core_reload() {
    let mut session = loaded();
    session
        .eval("(ns app (:require [cljs.core :as core])) (def old-not core/not) (def old-boolean core/boolean)")
        .unwrap();
    session.collect().unwrap();
    // The call's callee read precedes an argument which replaces that same cell.
    let value = session
        .eval("(core/boolean (do (set! core/boolean (fn [x] false)) 0))")
        .unwrap();
    assert!(boolean(&mut session, &value));
    let value = session.eval("(core/boolean 0)").unwrap();
    assert!(!boolean(&mut session, &value));
    let replacement = session.eval("core/boolean").unwrap();
    let value = session
        .eval("(core/not (do (set! core/not (fn [x] false)) nil))")
        .unwrap();
    assert!(boolean(&mut session, &value));
    let value = session.eval("(core/not nil)").unwrap();
    assert!(!boolean(&mut session, &value));
    session.collect().unwrap();
    session.eval_privileged(CORE).unwrap();
    session.enter_namespace("app").unwrap();
    session.collect().unwrap();
    for source in [
        "(core/boolean 0)",
        "(core/not nil)",
        "(old-boolean 0)",
        "(old-not nil)",
    ] {
        let value = session.eval(source).unwrap();
        assert!(boolean(&mut session, &value), "{source}");
    }
    let arg = session.eval("0").unwrap();
    let value = session.invoke(&replacement, &[&arg]).unwrap();
    assert!(!boolean(&mut session, &value));
}

#[test]
fn imported_some_distinguishes_nil_from_false_and_survives_gc() {
    let mut session = loaded();
    let function = session.eval("some?").unwrap();
    for (source, expected) in [
        ("nil", false),
        ("false", true),
        ("0", true),
        ("##NaN", true),
        ("\"\"", true),
        ("\"\\uD800😀\"", true),
        ("(fn [] 42)", true),
        ("(ex-info \"m\" 7)", true),
        ("(ex-data (new ExceptionInfo \"m\"))", false),
    ] {
        let argument = session.eval(source).unwrap();
        session.collect().unwrap();
        let value = session.invoke(&function, &[&argument]).unwrap();
        session.collect().unwrap();
        assert_eq!(boolean(&mut session, &value), expected, "{source}");
    }
}

#[test]
fn imported_some_ignores_redefined_predicate_and_not_vars() {
    let mut session = loaded();
    let function = session.eval("some?").unwrap();
    let nil = session.eval("nil").unwrap();
    let falsity = session.eval("false").unwrap();
    session.enter_namespace("suss.core").unwrap();
    session.eval("(def nil? (fn [x] false))").unwrap();
    session.collect().unwrap();
    let value = session.invoke(&function, &[&nil]).unwrap();
    assert!(!boolean(&mut session, &value));
    let value = session.invoke(&function, &[&falsity]).unwrap();
    assert!(boolean(&mut session, &value));
    session.eval("(def not identity)").unwrap();
    session.collect().unwrap();
    let value = session.invoke(&function, &[&nil]).unwrap();
    assert!(!boolean(&mut session, &value));
    let value = session.invoke(&function, &[&falsity]).unwrap();
    assert!(boolean(&mut session, &value));
    session.eval("(def not (fn [x] 7))").unwrap();
    let value = session.invoke(&function, &[&nil]).unwrap();
    assert!(!boolean(&mut session, &value));
}

#[test]
fn imported_some_evaluates_arguments_once_before_arity_error() {
    let mut session = loaded();
    session
        .eval("(def count 0) (def next (fn [] (def count (+ count 1))))")
        .unwrap();
    let value = session.eval("(some? (next))").unwrap();
    assert!(boolean(&mut session, &value));
    assert!(matches!(
        session.eval("(some? (next) (next))"),
        Err(SessionError::Language(_))
    ));
    assert!(matches!(
        session.eval("(some?)"),
        Err(SessionError::Language(_))
    ));
    let count = session.eval("count").unwrap();
    assert_eq!(number(&mut session, &count), num(3.0));
    let value = session.eval("(some? false)").unwrap();
    assert!(boolean(&mut session, &value));
}

#[test]
fn bootstrap_nil_test_evaluates_once_and_propagates_exceptions() {
    let mut session = loaded();
    session
        .eval("(def count 0) (def next (fn [] (def count (+ count 1)) nil))")
        .unwrap();
    let value = session.eval("(suss.bootstrap/nil? (next))").unwrap();
    assert!(boolean(&mut session, &value));
    let count = session.eval("count").unwrap();
    assert_eq!(number(&mut session, &count), num(1.0));
    let value = session
        .eval("(try (suss.bootstrap/nil? (throw 7)) (catch :default e e))")
        .unwrap();
    assert_eq!(number(&mut session, &value), num(7.0));
    for source in [
        "(suss.bootstrap/nil?)",
        "(suss.bootstrap/nil? nil false)",
        "suss.bootstrap/nil?",
        "(loop [x nil] (suss.bootstrap/nil? (recur x)))",
        "(ns app (:require [cljs.core :as suss.bootstrap]))",
        "(ns suss.bootstrap) (def nil? (fn [x] false))",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Compile(_))),
            "{source}"
        );
    }
    let value = session.eval("(some? false)").unwrap();
    assert!(boolean(&mut session, &value));
}

#[test]
fn bootstrap_nil_test_captures_values_across_fragments_and_loop_edges() {
    let mut session = loaded();
    let factory = session
        .eval("(fn [x] (fn [] (suss.bootstrap/nil? x)))")
        .unwrap();
    for (source, expected) in [
        ("nil", true),
        ("false", false),
        ("(fn [] 42)", false),
        ("(ex-data (new ExceptionInfo \"m\"))", true),
    ] {
        let argument = session.eval(source).unwrap();
        let captured = session.invoke(&factory, &[&argument]).unwrap();
        session.collect().unwrap();
        session.eval("(def unrelated 7)").unwrap();
        let result = session.invoke(&captured, &[]).unwrap();
        assert_eq!(boolean(&mut session, &result), expected, "{source}");
    }
    // The operand is a join value produced by parallel recur replacements;
    // neither the stale initial nil nor the unselected throwing branch may win.
    let result = session
        .eval("(suss.bootstrap/nil? (loop [again true x nil] (if again (recur false false) (if again (throw 7) x))))")
        .unwrap();
    assert!(!boolean(&mut session, &result));
}

#[test]
fn imported_inc_dec_preserve_primitives_captured_arithmetic_and_live_bindings() {
    let mut session = loaded();
    for (name, primitive, expected) in [("inc", "+", 3.0f64), ("dec", "-", 1.0f64)] {
        let original = session.eval(name).unwrap();
        let argument = session.eval("2").unwrap();
        for alias in [format!("cljs.core/{name}"), format!("suss.core/{name}")] {
            let returned = session.eval(&format!("(let [f {alias}] (f 2))")).unwrap();
            assert_eq!(number(&mut session, &returned), num(expected));
        }
        session.enter_namespace("suss.core").unwrap();
        session
            .eval(&format!(
                "(def {primitive} (fn [x y] 90)) (def {name} (fn [x] 80))"
            ))
            .unwrap();
        session.enter_namespace("user").unwrap();
        session.collect().unwrap();
        let returned = session.invoke(&original, &[&argument]).unwrap();
        assert_eq!(number(&mut session, &returned), num(expected));
        let returned = session.eval(&format!("(let [f {name}] (f 2))")).unwrap();
        assert_eq!(number(&mut session, &returned), num(80.0));
    }
}

#[test]
fn imported_inc_dec_evaluate_arguments_before_typed_arity_failure_and_recover() {
    let mut session = loaded();
    session
        .eval("(def effects 0) (def tick (fn [] (def effects (+ effects 1))))")
        .unwrap();
    for name in ["inc", "dec"] {
        let before = session.eval("effects").unwrap();
        let Number::Long(before) = number(&mut session, &before) else {
            panic!("effects counter is a long")
        };
        assert!(matches!(
            session.eval(&format!("(let [f {name}] (f (tick) (tick)))")),
            Err(SessionError::Language(_))
        ));
        let after = session.eval("effects").unwrap();
        assert_eq!(number(&mut session, &after), Number::Long(before + 2));
        assert!(matches!(
            session.eval(&format!("(let [f {name}] (f))")),
            Err(SessionError::Language(_))
        ));
        let value = session.eval(&format!("(let [f {name}] (f 4))")).unwrap();
        assert_eq!(
            number(&mut session, &value),
            num(if name == "inc" { 5.0 } else { 3.0 })
        );
    }
}

#[test]
fn imported_inc_dec_reject_objects_and_recover() {
    let mut session = loaded();
    session.eval("(deftype O [])").unwrap();
    for name in ["inc", "dec"] {
        // Clojure (#286): arrays, nominal objects and functions are not numbers.
        for argument in ["(array 1)", "(O.)", "(fn [] 1)"] {
            assert!(
                matches!(
                    session.eval(&format!("(let [f {name}] (f {argument}))")),
                    Err(SessionError::Language(_))
                ),
                "{name}: {argument}"
            );
            let value = session.eval(&format!("(let [f {name}] (f 3))")).unwrap();
            assert_eq!(
                number(&mut session, &value),
                num(if name == "inc" { 4.0 } else { 2.0 })
            );
        }
    }
}

#[test]
fn imported_inc_dec_alias_calls_capture_callee_before_mutating_operand() {
    let mut session = loaded();
    session
        .eval("(ns review (:require [cljs.core :as core]))")
        .unwrap();
    for (name, expected) in [("inc", 3.0f64), ("dec", 1.0f64)] {
        let source = format!(
            "(with-redefs [core/{name} core/{name}] ((if true core/{name} core/identity) (do (set! core/{name} (fn [x] 90)) 2)))"
        );
        let value = session.eval(&source).unwrap();
        assert_eq!(number(&mut session, &value), num(expected));
        session.collect().unwrap();
        let value = session
            .eval(&format!("(let [f core/{name}] (f 2))"))
            .unwrap();
        assert_eq!(number(&mut session, &value), num(expected));
        let source = format!(
            "(do (def effects 0) (try (let [f core/{name}] (f (do (def effects (+ effects 1)) (throw 7)))) (catch :default e nil) (finally (def effects (+ effects 10)))) effects)"
        );
        let value = session.eval(&source).unwrap();
        assert_eq!(number(&mut session, &value), num(11.0));
    }
}
