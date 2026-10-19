use suss_cli::portable_session::{Session, SessionValue};
use wasmtime::Val;

fn number(session: &mut Session, value: &SessionValue) -> u64 {
    session
        .inspect(value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Number layout");
            };
            Ok(*bits)
        })
        .unwrap()
}

#[test]
fn scalar_case_evaluates_selector_once_and_only_selected_result() {
    let mut session = Session::new().unwrap();
    session.eval("(def case-effects 0)").unwrap();
    let value = session.eval("(case (do (set! case-effects (+ case-effects 1)) 2) (1 2) 17 (do (set! case-effects 99) 19))").unwrap();
    assert_eq!(number(&mut session, &value), 17.0f64.to_bits());
    session.collect().unwrap();
    let value = session.eval("case-effects").unwrap();
    assert_eq!(number(&mut session, &value), 1.0f64.to_bits());
}

#[test]
fn scalar_case_matches_independently_decoded_pinned_observations() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/case-cases.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 40);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    for case in cases {
        let value = session
            .eval(case["source"].as_str().unwrap())
            .unwrap_or_else(|error| panic!("{}: {error:?}", case["id"]));
        assert_eq!(
            serde_json::json!({"tag":"f64", "bits":format!("{:016x}",number(&mut session, &value))}),
            case["expected"],
            "{}",
            case["id"]
        );
        session.collect().unwrap();
    }
}

#[test]
fn empty_case_group_has_exact_located_boundary_without_publication() {
    use suss_cli::portable_session::SessionError;
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/case-reference-boundaries.json"
    ))
    .unwrap();
    let case = &corpus["cases"][0];
    let mut session = Session::new().unwrap();
    let Err(SessionError::Compile(error)) = session.eval(&format!(
        "(def case-ghost 17) {}",
        case["source"].as_str().unwrap()
    )) else {
        panic!("exact compile boundary");
    };
    assert_eq!(error.message, case["native"]["message"].as_str().unwrap());
    assert!(error.span.end > error.span.start);
    assert!(matches!(
        session.eval("case-ghost"),
        Err(SessionError::Compile(_))
    ));
}

#[test]
fn case_errors_and_namespace_guards_keep_effects_and_recover_after_gc() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def case-effects 0)").unwrap();
    assert!(matches!(
        session.eval("(case (throw 17) 1 (set! case-effects 1) (set! case-effects 2))"),
        Err(SessionError::Language(_))
    ));
    session.collect().unwrap();
    let value = session.eval("case-effects").unwrap();
    assert_eq!(number(&mut session, &value), 0.0f64.to_bits());
    for source in [
        "(case)",
        "(case 1 1 17)",
        "(case 1 1 17 1 19 23)",
        "(case 1 (1 1) 17 19)",
        "(case 1 x 17 19)",
        "case",
    ] {
        let Err(SessionError::Compile(error)) =
            session.eval(&format!("(def case-ghost 7) {source}"))
        else {
            panic!("located case guard: {source}");
        };
        assert!(error.span.end > error.span.start);
        assert!(matches!(
            session.eval("case-ghost"),
            Err(SessionError::Compile(_))
        ));
    }
    let constants = (0..257)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let value = session.eval(&format!("(case 1 ({constants}) 17 19)")).unwrap();
    assert_eq!(number(&mut session, &value), 17.0f64.to_bits());
    // Also reach the final constant, with selector/selected-arm effects once.
    session.eval("(def finite-case-selector 0) (def finite-case-arm 0)").unwrap();
    let value = session.eval(&format!(
        "(case (do (set! finite-case-selector (+ finite-case-selector 1)) 256) ({constants}) (do (set! finite-case-arm (+ finite-case-arm 1)) 17) (throw 91))"
    )).unwrap();
    assert_eq!(number(&mut session, &value), 17.0f64.to_bits());
    for source in ["finite-case-selector", "finite-case-arm"] {
        let value = session.eval(source).unwrap();
        assert_eq!(number(&mut session, &value), 1.0f64.to_bits());
    }
    session
        .eval("(ns case.alias (:require [cljs.core :as c]))")
        .unwrap();
    let value = session.eval("(c/case 1 1 17 19)").unwrap();
    assert_eq!(number(&mut session, &value), 17.0f64.to_bits());
    session
        .eval("(ns case.excluded (:refer-clojure :exclude [case]))")
        .unwrap();
    assert!(matches!(
        session.eval("(case 1 1 17 19)"),
        Err(SessionError::Compile(_))
    ));
    let value = session.eval("(cljs.core/case 1 1 17 19)").unwrap();
    assert_eq!(number(&mut session, &value), 17.0f64.to_bits());
}

#[test]
fn generic_case_throw_restores_equality_and_bounds_are_located() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def case-review-effects 0)").unwrap();
    assert!(matches!(session.eval("(with-redefs [cljs.core/= (fn [a b] (throw 17))] (case true false (set! case-review-effects 1) true (set! case-review-effects 2) (set! case-review-effects 3)))"), Err(SessionError::Language(_))));
    session.collect().unwrap();
    let value = session.eval("case-review-effects").unwrap();
    assert_eq!(number(&mut session, &value), 0.0f64.to_bits());
    let value = session.eval("(case true false 19 true 17 23)").unwrap();
    assert_eq!(number(&mut session, &value), 17.0f64.to_bits());
    for (source, message) in [
        ("(case 0 (0 -0.0) 17 19)", "Duplicate case test constant"),
    ] {
        let Err(SessionError::Compile(error)) =
            session.eval(&format!("(def case-review-ghost 7) {source}"))
        else {
            panic!("case boundary");
        };
        assert_eq!(error.message, message);
        assert!(error.span.end > error.span.start);
        assert!(matches!(
            session.eval("case-review-ghost"),
            Err(SessionError::Compile(_))
        ));
    }
    // The same formerly rejected nine-constant source is legitimate finite
    // generic dispatch; selector8 reaches the explicit default19.
    let value = session.eval("(case 8 (false nil 0 1 2 3 4 5 6) 17 19)").unwrap();
    assert_eq!(number(&mut session, &value), 19.0f64.to_bits());
    let closure = session
        .eval("(let [x 17] (case true false (fn [] 19) true (fn [] x) (fn [] 23)))")
        .unwrap();
    session.collect().unwrap();
    let value = session.invoke(&closure, &[]).unwrap();
    assert_eq!(number(&mut session, &value), 17.0f64.to_bits());
}

#[test]
fn keyword_case_groups_follow_pinned_dispatch_and_preserve_selector_effects() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def keyword-case-effects 0)").unwrap();
        let value = session.eval("(case (do (set! keyword-case-effects (inc keyword-case-effects)) :ns/b) (:a :ns/b) 17 :b 19 23)").unwrap();
        assert_eq!(number(&mut session, &value), 17.0f64.to_bits());
        session.collect().unwrap();
        let value = session.eval("keyword-case-effects").unwrap();
        assert_eq!(number(&mut session, &value), 1.0f64.to_bits());
        for selector in ["nil", "false", "17", "\"ns/b\"", "'ns/b"] {
            let value = session
                .eval(&format!("(case {selector} (:a :ns/b) 17 :b 19 23)"))
                .unwrap();
            assert_eq!(number(&mut session, &value), 23.0f64.to_bits());
        }
        let value = session
            .eval(
                "(with-redefs [suss.core/= (fn [a b] (throw :bad))] (case :ns/b (:a :ns/b) 17 23))",
            )
            .unwrap();
        assert_eq!(number(&mut session, &value), 17.0f64.to_bits());
        let value = session
            .eval("(with-redefs [suss.core/keyword? (fn [x] false)] (case :ns/b (:a :ns/b) 17 23))")
            .unwrap();
        assert_eq!(number(&mut session, &value), 23.0f64.to_bits());
        let value = session.eval("(with-redefs [suss.core/keyword? (fn [x] (throw :bad))] (case (do (set! keyword-case-effects 7) :a) 23))").unwrap();
        assert_eq!(number(&mut session, &value), 23.0f64.to_bits());
        assert!(session.eval("(case :a (:a :b) 17 :a 19 23)").is_err());
        let values = (0..9)
            .map(|i| format!(":k{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let value = session
            .eval(&format!("(case :k8 ({values}) 17 23)"))
            .unwrap();
        assert_eq!(number(&mut session, &value), 17.0f64.to_bits());
    }
}

#[path = "support/compilation_fuel.rs"]
mod compilation_fuel;

#[test]
fn generic_case_many_mixed_constants_keep_order_selector_once_and_duplicate_guards() {
    use suss_cli::{portable_macros::CompiledMacros, portable_macro_data::FormBridge,
                   portable_session::SessionError};
    use suss_reader::forms::{Kind, read_forms};
    let expected = read_forms("[false nil 0 1 2 :hit \"seven\" true :late :other 3 \"tail\"]")
        .unwrap().remove(0);
    let Kind::Vector(expected) = &expected.kind else { unreachable!() };
    for mut caller in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let caller_fuel = caller.options().fuel_per_operation;
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
        caller.eval("(def large-case-original-eq suss.core/=) (def large-case-order []) (def large-case-selector 0) (def large-case-arms 0)").unwrap();
        let bridge = FormBridge::new(&mut caller).unwrap();
        for (selector, result, comparisons, arm) in [
            ("false", 17.0_f64, 1, 1.0), ("nil", 17.0, 2, 1.0),
            ("2", 17.0, 5, 1.0), ("\"seven\"", 17.0, 7, 1.0),
            ("true", 17.0, 8, 1.0), (":late", 17.0, 9, 1.0),
            (":other", 19.0, 10, 2.0), ("\"tail\"", 21.0, 12, 3.0),
            (":missing", 23.0, 12, 4.0),
        ] {
            caller.eval("(set! large-case-order []) (set! large-case-selector 0) (set! large-case-arms 0)").unwrap();
            let source = format!(r#"
              (with-redefs [suss.core/=
                (fn [left right]
                  (do (set! large-case-order (conj large-case-order [left right]))
                      (large-case-original-eq left right)))]
                (case (do (set! large-case-selector (inc large-case-selector)) {selector})
                  (false nil 0 1 2 :hit "seven" true :late)
                    (do (set! large-case-arms (+ (* large-case-arms 10) 1)) 17)
                  :other
                    (do (set! large-case-arms (+ (* large-case-arms 10) 2)) 19)
                  (3 "tail")
                    (do (set! large-case-arms (+ (* large-case-arms 10) 3)) 21)
                  (do (set! large-case-arms (+ (* large-case-arms 10) 4)) 23)))
            "#);
            let value = caller.eval_with_macros(&source, &mut macros).unwrap();
            assert_eq!(number(&mut caller, &value), result.to_bits());
            caller.collect().unwrap();
            let snapshot = caller.eval("[large-case-selector large-case-arms large-case-order]").unwrap();
            let snapshot = bridge.read(&mut caller, &snapshot, 0..1).unwrap();
            let Kind::Vector(fields) = &snapshot.kind else { panic!("actual observations") };
            assert_eq!(fields[0].kind, Kind::Number(1.0));
            assert_eq!(fields[1].kind, Kind::Number(arm));
            let Kind::Vector(order) = &fields[2].kind else { panic!("actual order") };
            assert_eq!(order.len(), comparisons);
            let selected = read_forms(selector).unwrap().remove(0);
            for (pair, expected) in order.iter().zip(expected.iter()) {
                let Kind::Vector(pair) = &pair.kind else { panic!("comparison pair") };
                assert_eq!(pair.len(), 2);
                assert_eq!(pair[0].kind, expected.kind, "constant must be first, source order");
                assert_eq!(pair[1].kind, selected.kind, "selector must be second");
            }
        }
        for duplicate in [
            "(case :late (false nil 0 1 2 :hit \"seven\" true :late) 17 :late 19 23)",
            "(case :missing (false nil 0 1 2 :hit \"seven\" true :late) 17 nil 19 23)",
            "(case 0 (false nil 0 1 2 :hit \"seven\" true -0.0) 17 23)",
        ] {
            let source = format!("(def large-case-ghost 99) {duplicate}");
            let before = caller.stats();
            let SessionError::Compile(error) = caller.eval_with_macros(&source, &mut macros).unwrap_err()
                else { panic!("located duplicate error") };
            assert_eq!(error.message, "Duplicate case test constant");
            assert!(error.span.start < error.span.end && error.span.end <= source.len());
            assert_eq!(caller.stats(), before);
            assert!(caller.eval("large-case-ghost").unwrap_err().to_string().contains("Unresolved"));
        }
        assert_eq!(caller.options().fuel_per_operation, caller_fuel);
    }
}

#[test]
fn generic_case_late_equality_throw_retains_prefix_and_restores_live_binding() {
    use suss_cli::{portable_macros::CompiledMacros, portable_macro_data::FormBridge,
                   portable_session::SessionError};
    use suss_reader::forms::{Kind, read_forms};
    for mut caller in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let caller_fuel = caller.options().fuel_per_operation;
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
        caller.eval("(def late-case-original-eq suss.core/=) (def late-case-order []) (def late-case-selector 0) (def late-case-arms 0)").unwrap();
        let source = r#"
          (with-redefs [suss.core/=
            (fn [left right]
              (do (set! late-case-order (conj late-case-order left))
                  (if (late-case-original-eq left :late) (throw 91)
                      (late-case-original-eq left right))))]
            (case (do (set! late-case-selector (inc late-case-selector)) :missing)
              (false nil 0 1 2 :hit "seven" true :late)
                (do (set! late-case-arms 1) 17)
              :other (do (set! late-case-arms 2) 19)
              (do (set! late-case-arms 3) 23)))
        "#;
        let SessionError::Language(payload) = caller.eval_with_macros(source, &mut macros).unwrap_err()
            else { panic!("actual late equality callback must throw") };
        caller.collect().unwrap();
        let bridge = FormBridge::new(&mut caller).unwrap();
        assert_eq!(bridge.read(&mut caller, &payload, 0..1).unwrap().kind, Kind::Number(91.0));
        let snapshot = caller.eval("[late-case-selector late-case-arms late-case-order (identical? late-case-original-eq suss.core/=)]").unwrap();
        let snapshot = bridge.read(&mut caller, &snapshot, 0..1).unwrap();
        let Kind::Vector(fields) = &snapshot.kind else { panic!("actual observations") };
        assert_eq!(fields[0].kind, Kind::Number(1.0));
        assert_eq!(fields[1].kind, Kind::Number(0.0));
        assert_eq!(fields[3].kind, Kind::Bool(true));
        let expected = read_forms("[false nil 0 1 2 :hit \"seven\" true :late]").unwrap().remove(0);
        // Span/reader metadata are not part of comparison effects; retain all values/order.
        let Kind::Vector(order) = &fields[2].kind else { panic!("actual prefix") };
        let Kind::Vector(expected) = &expected.kind else { unreachable!() };
        assert_eq!(order.len(), expected.len());
        for (actual, expected) in order.iter().zip(expected.iter()) { assert_eq!(actual.kind, expected.kind); }
        let value = caller.eval_with_macros("(case :late (false nil 0 1 2 :hit \"seven\" true :late) 42 0)", &mut macros).unwrap();
        assert_eq!(number(&mut caller, &value), 42.0f64.to_bits());
        assert_eq!(caller.options().fuel_per_operation, caller_fuel);
    }
}
