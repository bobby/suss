//! Native owner/path DATA tests. No strict key/domain or callback authority.
use super::*;
use crate::portable::resolve::{Environment, Global, Phase};
use crate::portable_macro_data::FormBridge;
use crate::portable_session::SessionValue;

fn prepare(source: &str, phase: Phase) -> (Global, Info) {
    let forms = suss_reader::forms::resolve_conditionals(suss_reader::forms::read_forms(source).unwrap()).unwrap();
    let origin = portable::SourceOrigin::new(source, Some("nested-key.cljs".into()));
    let environment = hir::prepare_with_origin(&forms, 0..source.len(), &Environment::default(), phase,
        &mut portable::NoExpansion, Some(&origin)).unwrap().1;
    let binding = environment.resolve(phase, &suss_reader::Symbol::new("subject"), 0..0).unwrap();
    let global = binding.global().clone();
    (global.clone(), environment.shared_definition_info(&global).unwrap().clone())
}
fn row(session: &mut Session, bridge: &FormBridge, cache: &mut source_roots::RootCache,
    global: &Global, info: &Info, parent_first: bool) -> SessionValue {
    let _request = session.operation_budget_scope().unwrap();
    let owner = info.completed_values.as_ref().unwrap();
    let mut graph = AnalysisGraph::new(bridge, session);
    graph.capture_roots = true;
    graph.direct_reader_roots = true;
    let metadata = graph.completed_reader(owner).unwrap();
    let index = graph.completed_property_index(&metadata, "top-fn").unwrap().unwrap();
    let (parent, key) = if parent_first {
        let parent = graph.completed_reader_property_value(global, owner, info, index, 0).unwrap();
        let key = graph.completed_nested_property_key(global, owner, info, "top-fn", "variadic?").unwrap();
        (parent, key)
    } else {
        let key = graph.completed_nested_property_key(global, owner, info, "top-fn", "variadic?").unwrap();
        let parent = graph.completed_reader_property_value(global, owner, info, index, 0).unwrap();
        (parent, key)
    };
    let root = graph.vector(&[parent, key]).unwrap();
    assert!(graph.admit_reader_controller(root).unwrap().is_none(), "nested DATA keys are not strict source capabilities");
    graph.materialize_reader_controller(root, cache).unwrap().expect("healthy nested key DATA")
}

#[test]
fn nested_reader_key_warm_parent_and_rebuilt_poisoned_sibling_keep_exact_healthy_key() {
    let source = "(def ^{:top-fn {:variadic? false :max-fixed-arity 3 :payload \"steady\"}} subject (fn* [x] x))";
    for (phase, mut session) in [(Phase::Runtime, Session::new_repl().unwrap()),
        (Phase::Macro, Session::new_macro().unwrap())] {
        let (global, info) = prepare(source, phase);
        let bridge = FormBridge::new(&mut session).unwrap();
        let check = session.eval("(fn [v] (identical? (first (keys (nth v 0))) (nth v 1)))").unwrap();
        let first = session.eval("(fn [v] (nth v 0))").unwrap();
        let key = session.eval("(fn [v] (nth v 1))").unwrap();
        let payload = session.eval("(fn [v] (:payload (nth v 0)))").unwrap();
        let mut cache = source_roots::RootCache::default();
        let a = row(&mut session, &bridge, &mut cache, &global, &info, false);
        let old_parent = session.invoke(&first, &[&a]).unwrap();
        let old_key = session.invoke(&key, &[&a]).unwrap();
        session.collect().unwrap();
        let b = row(&mut session, &bridge, &mut cache, &global, &info, true);
        let b_parent = session.invoke(&first, &[&b]).unwrap();
        let b_key = session.invoke(&key, &[&b]).unwrap();
        assert!(session.values_identical(&old_parent, &b_parent).unwrap());
        assert!(session.values_identical(&old_key, &b_key).unwrap());
        let ok = session.invoke(&check, &[&b]).unwrap();
        assert_eq!(bridge.read(&mut session, &ok, 0..0).unwrap().kind, Kind::Bool(true));
        // Poison an unrelated original child between requests: never repair it
        // or recapture expectations. Rebuild only the invalid parent and reuse
        // its still-healthy exact source-key occurrence through original evidence.
        let bad = session.invoke(&payload, &[&b]).unwrap();
        session.inspect(&bad, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            array.set(&mut store, 0, wasmtime::Val::I32('X' as i32))
        }).unwrap();
        session.collect().unwrap();
        let c = row(&mut session, &bridge, &mut cache, &global, &info, true);
        let c_parent = session.invoke(&first, &[&c]).unwrap();
        let c_key = session.invoke(&key, &[&c]).unwrap();
        assert!(!session.values_identical(&old_parent, &c_parent).unwrap());
        assert!(session.values_identical(&old_key, &c_key).unwrap());
        let restored = session.invoke(&payload, &[&c]).unwrap();
        assert_eq!(bridge.read(&mut session, &restored, 0..0).unwrap().kind,
            Kind::String("steady".encode_utf16().collect()));
        assert_eq!(bridge.read(&mut session, &bad, 0..0).unwrap().kind,
            Kind::String("Xteady".encode_utf16().collect()), "old exposed storage is never repaired");
        let ok = session.invoke(&check, &[&c]).unwrap();
        assert_eq!(bridge.read(&mut session, &ok, 0..0).unwrap().kind, Kind::Bool(true));
    }
}

#[test]
fn nested_reader_keys_reject_foreign_owner_changed_holder_and_bad_coordinates() {
    let source = "(def ^{:top-fn {:variadic? false :variadic? true :payload 17}} subject (fn* [x] x))";
    for (phase, mut session) in [(Phase::Runtime, Session::new_repl().unwrap()),
        (Phase::Macro, Session::new_macro().unwrap())] {
        let (global, info) = prepare(source, phase);
        let (_, foreign) = prepare(source, phase);
        let owner = info.completed_values.as_ref().unwrap();
        let foreign_owner = foreign.completed_values.as_ref().unwrap();
        let mut changed = info.as_ref().clone();
        changed.declaration.metadata.push(suss_reader::forms::read_forms("{:doc changed}").unwrap().remove(0));
        let changed = Arc::new(changed);
        let bridge = FormBridge::new(&mut session).unwrap();
        let _request = session.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.capture_roots = true;
        graph.direct_reader_roots = true;
        let metadata = graph.completed_reader(owner).unwrap();
        let index = graph.completed_property_index(&metadata, "top-fn").unwrap().unwrap();
        for (candidate, holder, ordinal, pair) in [(foreign_owner, &info, index, 0),
            (owner, &changed, index, 0), (owner, &info, usize::MAX, 0),
            (owner, &info, index, usize::MAX), (owner, &info, index, 1)] {
            let before = graph.source_roots.roots.len();
            assert!(matches!(graph.capture_completed_nested_reader_key(&global, candidate, holder, ordinal, pair), Err(SessionError::Host(_))));
            assert_eq!(graph.source_roots.roots.len(), before);
        }
        // Literal first-key precedence means an unused nested source key is NOT
        // captured, even if the supplied source owner would have been foreign.
        let mut keys = RetainedFieldKeys::new(&mut graph).unwrap();
        keys.remember_named(&mut graph, AnalyzerLiteralKeyword("name")).unwrap();
        let before = graph.source_roots.roots.len();
        keys.remember_completed_property_key(&mut graph, "name", &global, foreign_owner, &info, "top-fn").unwrap();
        assert_eq!(graph.source_roots.roots.len(), before);
        let key = graph.capture_completed_nested_reader_key(&global, owner, &info, index, 0).unwrap();
        assert!(graph.admit_reader_controller(key).unwrap().is_none(), "standalone nested DATA key cannot mint strict authority");
    }
}

#[test]
fn nested_reader_entry_vector_uses_same_actual_key_and_active_irq_stays_typed() {
    let source = "(def ^{:top-fn [:variadic? false]} subject (fn* [x] x))";
    for (phase, mut session) in [(Phase::Runtime, Session::new_repl().unwrap()),
        (Phase::Macro, Session::new_macro().unwrap())] {
        let (global, info) = prepare(source, phase);
        let bridge = FormBridge::new(&mut session).unwrap();
        let owner = info.completed_values.as_ref().unwrap();
        let mut cache = source_roots::RootCache::default();
        for parent_first in [false, true] {
            let result = row(&mut session, &bridge, &mut cache, &global, &info, parent_first);
            let check = session.eval("(fn [v] (identical? (nth (nth v 0) 0) (nth v 1)))").unwrap();
            let ok = session.invoke(&check, &[&result]).unwrap();
            assert_eq!(bridge.read(&mut session, &ok, 0..0).unwrap().kind, Kind::Bool(true));
        }
        let budget = session.operation_budget_scope().unwrap();
        session.interrupt_handle().interrupt();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.capture_roots = true; graph.direct_reader_roots = true;
        assert!(graph.capture_completed_nested_reader_key(&global, owner, &info, 0, 0).unwrap_err().is_interrupt());
        assert!(graph.source_roots.roots.is_empty());
        drop(graph); drop(budget);
        // Normal request retry. No refill inside the interrupted request.
        let _request = session.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.capture_roots = true; graph.direct_reader_roots = true;
        let metadata = graph.completed_reader(owner).unwrap();
        let index = graph.completed_property_index(&metadata, "top-fn").unwrap().unwrap();
        graph.capture_completed_nested_reader_key(&global, owner, &info, index, 0).unwrap();
    }
}

#[test]
fn nested_reader_normalized_first_property_key_last_value_and_empty_map_stays_empty() {
    for (phase, mut session) in [(Phase::Runtime, Session::new_repl().unwrap()),
        (Phase::Macro, Session::new_macro().unwrap())] {
        let bridge = FormBridge::new(&mut session).unwrap();
        // Two actual metadata associations: first retained PROPERTY key is
        // distinct from the winning VALUE occurrence. The selected nested map
        // itself has unique keys, within the existing unambiguous witness guard.
        let (global, info) = prepare("(def ^{:top-fn {:variadic? true}} ^{:top-fn {:variadic? false}} subject (fn* [x] x))", phase);
        let entries = hir::reader_metadata_view_metered(&info.declaration, &mut |_| Ok(())).unwrap();
        let entry = entries.iter().find(|entry| matches!(&entry.key.as_ref().kind,
            Kind::Keyword(key) if key.namespace.is_none() && key.name == "top-fn")).unwrap();
        assert_ne!(entry.key_position(), entry.value_position(),
            "the normalized association must retain the earlier KEY and later VALUE independently");
        assert!(matches!(&entry.value.as_ref().kind, Kind::Map(items)
            if items.len() == 2 && items[1].kind == Kind::Bool(true)),
            "outer association is the actual selected last writer, with one unique nested key");
        let mut cache = source_roots::RootCache::default();
        let actual = row(&mut session, &bridge, &mut cache, &global, &info, false);
        let check = session.eval("(fn [v] (and (= (:variadic? (nth v 0)) true) (identical? (first (keys (nth v 0))) (nth v 1))))").unwrap();
        let ok = session.invoke(&check, &[&actual]).unwrap();
        assert_eq!(bridge.read(&mut session, &ok, 0..0).unwrap().kind, Kind::Bool(true));
        let (global, info) = prepare("(def ^{:top-fn {}} subject (fn* [x] x))", phase);
        let _request = session.operation_budget_scope().unwrap();
        let owner = info.completed_values.as_ref().unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.capture_roots = true; graph.direct_reader_roots = true;
        let metadata = graph.completed_reader(owner).unwrap();
        let index = graph.completed_property_index(&metadata, "top-fn").unwrap().unwrap();
        let parent = graph.completed_reader_property_value(&global, owner, &info, index, 0).unwrap();
        let root = graph.vector(&[parent]).unwrap();
        let actual = graph.materialize_reader_controller(root, &mut source_roots::RootCache::default()).unwrap().unwrap();
        let empty = session.eval("(fn [v] (empty? (nth v 0)))").unwrap();
        let ok = session.invoke(&empty, &[&actual]).unwrap();
        assert_eq!(bridge.read(&mut session, &ok, 0..0).unwrap().kind, Kind::Bool(true));
    }
}


#[test]
fn nested_reader_key_zero_fuel_fails_before_publication_and_new_request_retries() {
    for (phase, mut session) in [(Phase::Runtime, Session::new_repl().unwrap()),
        (Phase::Macro, Session::new_macro().unwrap())] {
        let (global, info) = prepare("(def ^{:top-fn {:variadic? false}} subject (fn* [x] x))", phase);
        let bridge = FormBridge::new(&mut session).unwrap();
        let owner = info.completed_values.as_ref().unwrap();
        session.set_operation_fuel(0);
        let request = session.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.capture_roots = true; graph.direct_reader_roots = true;
        let error = graph.capture_completed_nested_reader_key(&global, owner, &info, 0, 0).unwrap_err();
        assert!(matches!(error, SessionError::Trap(ref error)
            if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
        assert!(graph.source_roots.roots.is_empty());
        drop(graph); drop(request);
        session.set_operation_fuel(portable::DEFAULT_OPERATION_FUEL);
        let _request = session.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.capture_roots = true; graph.direct_reader_roots = true;
        let metadata = graph.completed_reader(owner).unwrap();
        let index = graph.completed_property_index(&metadata, "top-fn").unwrap().unwrap();
        graph.capture_completed_nested_reader_key(&global, owner, &info, index, 0).unwrap();
    }
}
