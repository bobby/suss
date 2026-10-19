//! Regression specification from eight executed retained-CLJS observations.
//! Pin c4295f303100bbf5afac449242d30bca1126f1a1, analyzer.cljc3630–3645,
//! EPL-1.0 upstream policy. Native test orchestration is original code.
//! These tests are NOT executed or claimed passing in the author packet.
use super::*;
use crate::portable::{hir, resolve::{Environment, Phase}};
use crate::portable_session::memo_leaf_test_support::Method;

// Source strings and results are bound to the eight-case reference probe, not
// extrapolated from numeric memos or equivalence of reconstructed metadata.
const CASES: &[(&str, &[&str], &str)] = &[
    ("fresh-type-no-meta", &["(deftype* watched [] nil nil)"],
        r#"[true false false false "function" "" nil nil false false false false "" "" false 0]"#),
    ("prior-shorthand-meta", &["(def ^number watched 17)", "(deftype* watched [] nil nil)"],
        r#"[true false false false "function" "" nil nil true true true false "number" "" true 0]"#),
    ("prior-doc-function", &["(def watched \"prior doc\" (fn* [] 17))", "(deftype* watched [] nil nil)"],
        r#"[true true true true "function" "number" "prior doc" true true true true false "" "" false 0]"#),
    ("current-nil-tag-false-doc", &["(def watched \"prior doc\" (fn* [] 17))", "(deftype* ^{:tag nil :doc false} watched [] nil nil)"],
        r#"[true true true true "" "number" false true true true true false "" "" false 0]"#),
    ("current-tag-doc", &["(def watched \"prior doc\" (fn* [] 17))", "(deftype* ^{:tag current-tag :doc \"current doc\"} watched [] nil nil)"],
        r#"[true true true true "current-tag" "number" "current doc" true true true true false "" "" false 0]"#),
    ("explicit-current-meta-map", &["(def ^number watched 17)", "(deftype* ^{:meta {:marker :current}} watched [] nil nil)"],
        r#"[true false false false "function" "" nil nil true true false true "" ":current" false 0]"#),
    ("explicit-current-meta-false", &["(def ^number watched 17)", "(deftype* ^{:meta false} watched [] nil nil)"],
        r#"[true false false false "function" "" nil nil true false false false "" "" false 0]"#),
    ("repeat-retained-explicit-meta", &["(def ^{:tag prior-tag :doc \"old\"} watched 17)", "(deftype* ^{:meta {:marker :first}} watched [] nil nil)", "(deftype* ^{:doc \"later\"} watched [field] nil nil)"],
        r#"[true false true false "function" "" "later" nil true true true false "" ":first" false 1]"#),
];

#[test]
fn actual_type_value_projection_preserves_shallow_meta_and_prior_function_policy_after_gc() {
    for (phase, mut session) in [
        (Phase::Runtime, Session::new_repl().unwrap()),
        (Phase::Macro, Session::new_macro().unwrap()),
    ] {
        let bridge = FormBridge::new(&mut session).unwrap();
        session.eval("(def leaf-original-method nil) (def type-value-events [])").unwrap();
        let original = Method::capture(&mut session, "Keyword").unwrap();
        original.bind_delegate(&mut session).unwrap();
        let project = session.eval(r#"(fn [values]
          (let [previous (nth values 0) current (nth values 1) raw-meta (nth values 2)
                m (get current :meta)
                key-for (fn [value label]
                  (reduce (fn [found key] (if (= (str key) label) key found)) nil (keys value)))
                outer-tag (key-for current ":tag")
                inner-tag (if (map? m) (key-for m ":tag") nil)]
            [(contains? current :tag) (contains? current :ret-tag)
             (contains? current :doc) (contains? current :fn-var)
             (str (get current :tag)) (str (get current :ret-tag))
             (get current :doc) (get current :fn-var) (contains? current :meta) (map? m)
             (and (map? m) (map? (get previous :meta)) (identical? m (get previous :meta)))
             (and (map? m) (map? raw-meta) (identical? m raw-meta))
             (if (map? m) (str (get m :tag)) "")
             (if (map? m) (str (get m :marker)) "")
             (and (not (nil? inner-tag)) (identical? outer-tag inner-tag))
             (get current :num-fields)]))"#).unwrap();
        session.eval(r#"(extend-type Keyword IHash (-hash [self]
            (let [memo (.-_hash self)]
              (do (set! type-value-events (conj type-value-events [self memo]))
                  (if (nil? memo) (leaf-original-method self) memo)))))"#).unwrap();
        let mut observations = Vec::new();
        for (label, sources, expected) in CASES {
            session.eval("(set! type-value-events [])").unwrap();
            let mut environment = Environment::default();
            let mut previous = None;
            let mut current = None;
            for source in *sources {
                previous = current.take();
                let forms = suss_reader::forms::read_forms(source).unwrap();
                let origin = portable::SourceOrigin::new(source, None);
                environment = hir::prepare_with_origin(&forms, 0..source.len(), &environment, phase,
                    &mut portable::NoExpansion, Some(&origin)).unwrap().1;
                let binding = environment.resolve(phase, &suss_reader::Symbol::new("watched"), 0..0).unwrap();
                current = Some((binding.global().clone(), environment.shared_definition_info(binding.global()).unwrap().clone()));
            }
            let (global, info) = current.unwrap();
            for prior_first in [false, true] {
                let values = {
                    let mut graph = AnalysisGraph::new(&bridge, &mut session);
                    let actual_first = if !prior_first { Some(graph.declaration(&global, Some(&info), 0).unwrap()) } else { None };
                    let prior = if let Some((prior_global, prior_info)) = &previous {
                        graph.declaration(prior_global, Some(prior_info), 0).unwrap()
                    } else { graph.scalar(Literal::Nil).unwrap() };
                    let actual = if let Some(actual) = actual_first { actual }
                        else { graph.declaration(&global, Some(&info), 0).unwrap() };
                    // The current raw meta reference comes from the actual
                    // private captured source-value owner, not an equality join
                    // against independently cloned final reader forms.
                    let Kind::List(current_input) = &info.definition_form.kind else { panic!("actual current type input"); };
                    let raw_metadata = hir::reader_metadata_view_metered(&current_input[1], &mut |_| Ok(())).unwrap();
                    let current_meta = raw_metadata.iter().find(|entry| matches!(&entry.key.as_ref().kind,
                        Kind::Keyword(key) if key.namespace.is_none() && key.name == "meta"));
                    let explicit = if let Some(current_meta) = current_meta {
                        let value = info.type_key_history.as_ref().and_then(|h| h.values(&global))
                            .and_then(|v| v.source_value("meta")).expect("actual current selected value owner");
                        // Independent native capture relationship only, not a
                        // production equality join or public guest-frame alias.
                        assert_eq!(value.form(), current_meta.value.as_ref());
                        Some(value)
                    } else { None };
                    let raw_meta = if let Some(source) = explicit {
                        graph.completed_source_value_recipe(&global, &info, source, 0).unwrap()
                    } else { graph.scalar(Literal::Nil).unwrap() };
                    let root = graph.vector(&[prior, actual, raw_meta]).unwrap();
                    graph.materialize(root).unwrap()
                };
                session.collect().unwrap();
                let result = session.invoke(&project, &[&values]).unwrap();
                let decoded = bridge.read(&mut session, &result, 0..0).unwrap();
                let expected = suss_reader::forms::read_forms(expected).unwrap().remove(0);
                observations.push((format!("{label}/prior_first={prior_first}"), decoded, expected));
            }
        }
        // Restore before evaluating deferred assertions, including on a
        // semantic failure. Every case exercised the live nil-aware receiver.
        original.restore(&mut session).unwrap();
        session.collect().unwrap();
        for (label, observed, expected) in observations {
            assert_eq!(observed.kind, expected.kind, "{phase:?}: {label}");
        }
    }
}

#[test]
fn current_type_value_owner_preserves_original_nested_source_metadata_and_rejects_equal_foreign_owner() {
    for (phase, mut session) in [(Phase::Runtime, Session::new_repl().unwrap()), (Phase::Macro, Session::new_macro().unwrap())] {
        let bridge = FormBridge::new(&mut session).unwrap();
        let first = "(deftype* ^{:meta {:marker nested-symbol :nested {:leaf another-symbol}}} watched [] nil nil)";
        let first_origin = portable::SourceOrigin::new(first, None);
        let forms = suss_reader::forms::read_forms(first).unwrap();
        let Kind::List(input) = &forms[0].kind else { unreachable!() };
        let metadata = hir::reader_metadata_view_metered(&input[1], &mut |_| Ok(())).unwrap();
        let original_value = metadata.iter().find(|entry| matches!(&entry.key.as_ref().kind,
            Kind::Keyword(key) if key.namespace.is_none() && key.name == "meta")).unwrap().value.as_ref();
        let expected_reader = first_origin.macro_form_data_metered(original_value, &mut |_| Ok(())).unwrap();
        let mut environment = hir::prepare_with_origin(&forms, 0..first.len(), &Environment::default(), phase,
            &mut portable::NoExpansion, Some(&first_origin)).unwrap().1;
        // A different final origin must not be applied to the earlier value.
        let second = "\n\n(deftype* watched [field] nil nil)";
        let second_origin = portable::SourceOrigin::new(second, None);
        let forms2 = suss_reader::forms::read_forms(second).unwrap();
        environment = hir::prepare_with_origin(&forms2, 0..second.len(), &environment, phase,
            &mut portable::NoExpansion, Some(&second_origin)).unwrap().1;
        let binding = environment.resolve(phase, &suss_reader::Symbol::new("watched"), 0..0).unwrap();
        let global = binding.global();
        let info = environment.shared_definition_info(global).unwrap();
        let value = info.type_key_history.as_ref().and_then(|h| h.values(global)).unwrap().source_value("meta").unwrap();
        assert_eq!(value.form(), original_value, "actual selected native source value, with all spans/prefixes");
        assert!(value.origin().unwrap().same_native_owner(&first_origin));
        assert!(!value.origin().unwrap().same_native_owner(&second_origin));
        let foreign = value.independent_test_copy();
        assert_eq!(foreign.form(), value.form());
        let actual = {
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            let before = graph.recipes.len();
            assert!(matches!(graph.completed_source_value_recipe(global, info, &foreign, 0), Err(SessionError::Host(_))));
            assert_eq!(graph.recipes.len(), before, "no recipe publication on foreign equal-copy owner");
            let root = graph.completed_source_value_recipe(global, info, value, 0).unwrap();
            graph.materialize(root).unwrap()
        };
        let expected = bridge.quote(&mut session, expected_reader).unwrap();
        session.collect().unwrap();
        let actual_data = bridge.read(&mut session, &actual, 0..0).unwrap();
        let expected_data = bridge.read(&mut session, &expected, 0..0).unwrap();
        assert_eq!(actual_data.kind, expected_data.kind);
        // Do not stop at comparing value data: reader metadata is observable.
        let meta_project = session.eval("(fn [m] [(meta (:marker m)) (meta (:leaf (:nested m)))])").unwrap();
        let a = session.invoke(&meta_project, &[&actual]).unwrap();
        let b = session.invoke(&meta_project, &[&expected]).unwrap();
        assert_eq!(bridge.read(&mut session, &a, 0..0).unwrap().kind,
            bridge.read(&mut session, &b, 0..0).unwrap().kind);
    }
}

#[test]
fn current_type_value_transport_preserves_typed_fuel_irq_and_retry_without_source_mutation() {
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let source = "(deftype* ^{:meta {:marker nested}} watched [] nil nil)";
    let forms = suss_reader::forms::read_forms(source).unwrap();
    let origin = portable::SourceOrigin::new(source, None);
    let environment = hir::prepare_with_origin(&forms, 0..source.len(), &Environment::default(), Phase::Macro,
        &mut portable::NoExpansion, Some(&origin)).unwrap().1;
    let binding = environment.resolve(Phase::Macro, &suss_reader::Symbol::new("watched"), 0..0).unwrap();
    let global = binding.global();
    let info = environment.shared_definition_info(global).unwrap();
    let value = info.type_key_history.as_ref().and_then(|h| h.values(global)).unwrap().source_value("meta").unwrap();
    let initial = value.form().clone();
    // This sampled Store-budget test intentionally measures the warm source
    // snapshot. Native capture has its separate finite diagnostic-cut test.
    origin.macro_form_data_metered(value.form(), &mut |_| Ok(())).unwrap();
    let total = {
        let request = session.operation_budget_scope().unwrap();
        let before = session.operation_fuel_remaining().unwrap();
        {
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            graph.completed_source_value_recipe(global, info, value, 0).unwrap();
        }
        let used = before - session.operation_fuel_remaining().unwrap();
        drop(request); used
    };
    assert!(total > 1, "the measured helper performs charged native work");
    // The helper's first debit is at least 22*8+12 units, before any
    // binding/cache read or recipe publication. Both 0 and positive 1 MUST
    // exhaust; higher cuts may legitimately complete at a warm boundary.
    for (allowance, must_exhaust) in [(0, true), (1, true),
        (total / 2, false), (total.saturating_sub(1), false), (total, false)] {
        session.set_operation_fuel(allowance);
        let request = session.operation_budget_scope().unwrap();
        {
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            let result = graph.completed_source_value_recipe(global, info, value, 0);
            if must_exhaust {
                let error = result.expect_err("zero/one cannot pay the helper's first native debit");
                assert!(matches!(error, SessionError::Trap(ref e)
                    if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "required cut {allowance}: {error:?}");
            } else if let Err(error) = result {
                assert!(matches!(error, SessionError::Trap(ref e)
                    if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{allowance}: {error:?}");
            }
        }
        drop(request);
        assert_eq!(value.form(), &initial);
        session.set_operation_fuel(portable::DEFAULT_OPERATION_FUEL);
        let request = session.operation_budget_scope().unwrap();
        {
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            graph.completed_source_value_recipe(global, info, value, 0).unwrap();
        }
        drop(request);
    }
    let request = session.operation_budget_scope().unwrap();
    session.interrupt_handle().interrupt();
    {
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let error = graph.completed_source_value_recipe(global, info, value, 0).err().expect("pending IRQ");
        assert!(error.is_interrupt());
    }
    drop(request);
    let request = session.operation_budget_scope().unwrap();
    {
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.completed_source_value_recipe(global, info, value, 0).unwrap();
    }
    drop(request);
    assert_eq!(value.form(), &initial);
}
