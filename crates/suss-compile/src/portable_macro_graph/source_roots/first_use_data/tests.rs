//! Unexecuted positive/negative obligations for the first-use data route.
use super::*;
use crate::portable::{
    hir,
    resolve::{Environment, Phase},
};
use crate::portable_session::memo_leaf_test_support::Method;
pub(super) fn source() -> (Environment, Global, Arc<DefinitionInfo>) {
    let text = "(def ^{:order.meta/k0 0 :order.meta/k1 1 :order.meta/k2 2 :order.meta/k3 3 :order.meta/k4 4 :order.meta/k5 5 :order.meta/k6 6 :order.meta/k7 7 :order.meta/k8 8 :order.meta/k9 9} watched 17)";
    let forms = suss_reader::forms::read_forms(text).unwrap();
    let environment = hir::prepare(&forms, 0..text.len(), &Environment::default(), Phase::Macro)
        .unwrap()
        .1;
    let binding = environment
        .resolve(Phase::Macro, &suss_reader::Symbol::new("watched"), 0..0)
        .unwrap();
    let global = binding.global().clone();
    let info = environment.shared_definition_info(&global).unwrap().clone();
    (environment, global, info)
}
pub(super) fn build(
    session: &mut Session,
    bridge: &FormBridge,
    cache: &mut RootCache,
    global: &Global,
    info: &Arc<DefinitionInfo>,
    inline_first: bool,
) -> Result<SessionValue> {
    let owner = info.completed_values.as_ref().unwrap();
    let mut graph = AnalysisGraph::new(bridge, session);
    graph.capture_roots = true;
    graph.direct_reader_roots = true;
    let inline = suss_reader::forms::read_forms("{:order.inline/k0 0 :order.inline/k1 1 :order.inline/k2 2 :order.inline/k3 3 :order.inline/k4 4 :order.inline/k5 5 :order.inline/k6 6 :order.inline/k7 7 :order.inline/k8 8 :order.inline/k9 9}").unwrap().remove(0);
    let inline = graph.form(&inline, 0)?;
    let meta = graph.completed_metadata_map(global, owner, info, 0)?;
    let root = graph.recipe(Recipe::Vector(if inline_first {
        vec![inline, meta]
    } else {
        vec![meta, inline]
    }))?;
    graph
        .materialize_reader_controller(root, cache)?
        .ok_or_else(|| invalid("Required first-use fixture declined"))
}
pub(super) fn metadata(
    session: &mut Session,
    root: &SessionValue,
    inline_first: bool,
) -> SessionValue {
    let project = session
        .eval(if inline_first {
            "(fn [v] (nth v 1))"
        } else {
            "(fn [v] (nth v 0))"
        })
        .unwrap();
    session.invoke(&project, &[root]).unwrap()
}
fn save_key(session: &mut Session, meta: &SessionValue) -> SessionValue {
    session.eval("(def source-evidence-saved-key nil) (def source-evidence-effects 0) (def leaf-original-method nil)").unwrap();
    let find = session.eval("(fn [m] (reduce (fn [out key] (if (and (= (.-ns key) \"order.meta\") (= (.-name key) \"k0\")) key out)) nil (keys m)))").unwrap();
    let key = session.invoke(&find, &[meta]).unwrap();
    let save = session
        .eval("(fn [key] (set! source-evidence-saved-key key))")
        .unwrap();
    session.invoke(&save, &[&key]).unwrap();
    key
}
fn install_poison(session: &mut Session) -> Method {
    let original = Method::capture(session, "Keyword").unwrap();
    original.bind_delegate(session).unwrap();
    session
        .eval(
            r#"(extend-type Keyword IHash (-hash [self]
      (do (if (and (= (.-ns self) "order.inline") (= (.-name self) "k0"))
        (do (set! source-evidence-effects (inc source-evidence-effects))
            (set! (.-name source-evidence-saved-key) "poisoned")) nil)
        (let [memo (.-_hash self)] (if (nil? memo) (leaf-original-method self) memo)))))"#,
        )
        .unwrap();
    original
}
#[test]
fn earlier_inline_callback_invalidates_old_private_candidate_before_first_use_and_freshens_once() {
    let (_environment, global, info) = source();
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut cache = RootCache::default();
    let before = build(&mut session, &bridge, &mut cache, &global, &info, false).unwrap();
    let old_meta = metadata(&mut session, &before, false);
    let old_key = save_key(&mut session, &old_meta);
    let original = install_poison(&mut session);
    // The inline map completes BEFORE metadata's first-use cache check. Its
    // actual callback poisons only the OLD object. Native rematerialization of
    // metadata is the first construction of that source in this request.
    let result = build(&mut session, &bridge, &mut cache, &global, &info, true);
    original.restore(&mut session).unwrap();
    let after = result.unwrap();
    let current_meta = metadata(&mut session, &after, true);
    assert!(!session.values_identical(&old_meta, &current_meta).unwrap());
    let inspect = session.eval("(fn [old m] [(pos? source-evidence-effects) (= (.-name old) \"poisoned\") (count m) (every? (fn [key] (not= (.-name key) \"poisoned\")) (keys m))])").unwrap();
    session.collect().unwrap();
    let row = session
        .invoke(&inspect, &[&old_key, &current_meta])
        .unwrap();
    assert_eq!(
        crate::portable_repl::display(&mut session, &row).unwrap(),
        "[true true 10 true]"
    );
    assert!(
        cache.entries.is_empty(),
        "DATA identity never becomes strict source authority"
    );
}
#[test]
fn later_callback_key_poison_preserves_effect_and_entire_old_cache_then_exact_restoration_recovers()
{
    let (_environment, global, info) = source();
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut cache = RootCache::default();
    let before = build(&mut session, &bridge, &mut cache, &global, &info, false).unwrap();
    let old_meta = metadata(&mut session, &before, false);
    let key = save_key(&mut session, &old_meta);
    let get_name = session.eval("(fn [key] (.-name key))").unwrap();
    let name = session.invoke(&get_name, &[&key]).unwrap();
    let prior: Vec<_> = cache.data.iter().map(Rc::as_ptr).collect();
    assert!(!prior.is_empty());
    let original = install_poison(&mut session);
    // Reverse placement: old metadata has already entered the root's values
    // before the later inline hash callback changes its actual selected key.
    let result = build(&mut session, &bridge, &mut cache, &global, &info, false);
    original.restore(&mut session).unwrap();
    assert!(
        result.is_err(),
        "post-selection source corruption must not publish a repaired value"
    );
    assert_eq!(cache.data.iter().map(Rc::as_ptr).collect::<Vec<_>>(), prior);
    let inspect = session
        .eval("(fn [key] [(pos? source-evidence-effects) (= (.-name key) \"poisoned\")])")
        .unwrap();
    let row = session.invoke(&inspect, &[&key]).unwrap();
    assert_eq!(
        crate::portable_repl::display(&mut session, &row).unwrap(),
        "[true true]",
        "an early setup/fuel failure cannot substitute for an actual poison callback"
    );
    let restore = session
        .eval("(fn [key name] (set! (.-name key) name))")
        .unwrap();
    session.invoke(&restore, &[&key, &name]).unwrap();
    session.collect().unwrap();
    let recovered = build(&mut session, &bridge, &mut cache, &global, &info, false).unwrap();
    let recovered_meta = metadata(&mut session, &recovered, false);
    assert!(
        session
            .values_identical(&old_meta, &recovered_meta)
            .unwrap()
    );
}
#[test]
fn healthy_nil_aware_method_transition_keeps_warm_data_identity_without_strict_capabilities() {
    let (_environment, global, info) = source();
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut cache = RootCache::default();
    let first = build(&mut session, &bridge, &mut cache, &global, &info, false).unwrap();
    let old_meta = metadata(&mut session, &first, false);
    session.eval("(def leaf-original-method nil)").unwrap();
    let original = Method::capture(&mut session, "Keyword").unwrap();
    original.bind_delegate(&mut session).unwrap();
    session.eval("(extend-type Keyword IHash (-hash [self] (let [memo (.-_hash self)] (if (nil? memo) (leaf-original-method self) memo))))").unwrap();
    session.collect().unwrap();
    let warm = build(&mut session, &bridge, &mut cache, &global, &info, false);
    original.restore(&mut session).unwrap();
    let warm = warm.unwrap();
    let current = metadata(&mut session, &warm, false);
    assert!(session.values_identical(&old_meta, &current).unwrap());
    assert!(cache.entries.is_empty());
    assert!(!cache.data.is_empty());
}
#[test]
fn source_identity_cache_rejects_foreign_store_without_new_constructor_replay_or_cache_change() {
    let (_environment, global, info) = source();
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut cache = RootCache::default();
    build(&mut session, &bridge, &mut cache, &global, &info, false).unwrap();
    let prior: Vec<_> = cache.data.iter().map(Rc::as_ptr).collect();
    let mut foreign = Session::new_macro().unwrap();
    let foreign_bridge = FormBridge::new(&mut foreign).unwrap();
    assert!(
        build(
            &mut foreign,
            &foreign_bridge,
            &mut cache,
            &global,
            &info,
            false
        )
        .is_err()
    );
    assert_eq!(cache.data.iter().map(Rc::as_ptr).collect::<Vec<_>>(), prior);
    build(&mut session, &bridge, &mut cache, &global, &info, false).unwrap();
}

#[test]
fn active_request_interrupt_leaves_identity_cache_unpublished_and_retry_reuses_original() {
    let (_environment, global, info) = source();
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut cache = RootCache::default();
    let first = build(&mut session, &bridge, &mut cache, &global, &info, false).unwrap();
    let first_meta = metadata(&mut session, &first, false);
    let before: Vec<_> = cache.data.iter().map(Rc::as_ptr).collect();
    assert!(!before.is_empty());
    // Signal only after entering the actual cumulative request. This proves
    // entry cancellation and cache atomicity, not every internal IRQ boundary.
    let operation = session.operation_budget_scope().unwrap();
    session.interrupt_handle().interrupt();
    let error = build(&mut session, &bridge, &mut cache, &global, &info, false).unwrap_err();
    assert!(error.is_interrupt());
    assert_eq!(
        cache.data.iter().map(Rc::as_ptr).collect::<Vec<_>>(),
        before
    );
    assert!(cache.entries.is_empty());
    drop(operation);
    let retry = build(&mut session, &bridge, &mut cache, &global, &info, false).unwrap();
    let retry_meta = metadata(&mut session, &retry, false);
    assert!(session.values_identical(&first_meta, &retry_meta).unwrap());
}

#[test]
fn selected_reader_value_poison_after_parent_exposure_preserves_old_cache_and_exact_restore_identity() {
    // This is a genuine completed source occurrence, not an arbitrary native
    // seed or content-equal object promoted into source/key authority.
    for (phase, mut session) in [(Phase::Runtime, Session::new_repl().unwrap()),
        (Phase::Macro, Session::new_macro().unwrap())] {
        let text = "(def ^number watched 17)";
        let forms = suss_reader::forms::read_forms(text).unwrap();
        let origin = crate::portable::SourceOrigin::new(text, None);
        let environment = hir::prepare_with_origin(&forms, 0..text.len(), &Environment::default(), phase,
            &mut crate::portable::NoExpansion, Some(&origin)).unwrap().1;
        let binding = environment.resolve(phase, &suss_reader::Symbol::new("watched"), 0..0).unwrap();
        let global = binding.global();
        let info = environment.shared_definition_info(global).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let mut cache = RootCache::default();
        let make = |session: &mut Session, cache: &mut RootCache| -> Result<SessionValue> {
            let mut graph = AnalysisGraph::new(&bridge, session);
            graph.capture_roots = true;
            graph.direct_reader_roots = true;
            let meta = graph.completed_metadata_map(global, info.completed_values.as_ref().unwrap(), info, 0)?;
            let pairs = graph.completed_pairs(meta)?;
            let value = graph.declaration_field_value(&pairs, "tag")?.expect("source tag");
            let inline = suss_reader::forms::read_forms("{:order.inline/k0 0 :order.inline/k1 1 :order.inline/k2 2 :order.inline/k3 3 :order.inline/k4 4 :order.inline/k5 5 :order.inline/k6 6 :order.inline/k7 7 :order.inline/k8 8 :order.inline/k9 9}").unwrap().remove(0);
            let inline = graph.form(&inline, 0)?;
            let root = graph.vector(&[meta, value, inline])?;
            graph.materialize_reader_controller(root, cache)?
                .ok_or_else(|| invalid("Healthy selected-value source declined"))
        };
        let before = make(&mut session, &mut cache).unwrap();
        let get = session.eval("(fn [v] (nth v 1))").unwrap();
        let old_value = session.invoke(&get, &[&before]).unwrap();
        session.eval("(def saved-reader-value nil) (def reader-value-effects 0) (def leaf-original-method nil)").unwrap();
        let save = session.eval("(fn [v] (set! saved-reader-value v))").unwrap();
        session.invoke(&save, &[&old_value]).unwrap();
        let get_name = session.eval("(fn [v] (.-name v))").unwrap();
        let original_name = session.invoke(&get_name, &[&old_value]).unwrap();
        let prior: Vec<_> = cache.data.iter().map(Rc::as_ptr).collect();
        assert!(!prior.is_empty());
        let original = Method::capture(&mut session, "Keyword").unwrap();
        original.bind_delegate(&mut session).unwrap();
        session.eval(r#"(extend-type Keyword IHash (-hash [self]
          (do (if (and (= (.-ns self) "order.inline") (= (.-name self) "k0"))
                (do (set! reader-value-effects (inc reader-value-effects))
                    (set! (.-name saved-reader-value) "poisoned-value")) nil)
              (let [memo (.-_hash self)] (if (nil? memo) (leaf-original-method self) memo)))))"#).unwrap();
        let failed = make(&mut session, &mut cache);
        original.restore(&mut session).unwrap();
        assert!(matches!(failed, Err(SessionError::Host(_))),
            "actual postcallback DATA validation must reject; a fuel/setup error is not this witness");
        assert_eq!(cache.data.iter().map(Rc::as_ptr).collect::<Vec<_>>(), prior,
            "no partial source-value cache replacement");
        let check = session.eval("(fn [v] [(pos? reader-value-effects) (= (.-name v) \"poisoned-value\")])").unwrap();
        let row = session.invoke(&check, &[&old_value]).unwrap();
        assert_eq!(crate::portable_repl::display(&mut session, &row).unwrap(), "[true true]",
            "the actual late callback and old-object effect must both occur");
        let restore = session.eval("(fn [v name] (set! (.-name v) name))").unwrap();
        session.invoke(&restore, &[&old_value, &original_name]).unwrap();
        session.collect().unwrap();
        let recovered = make(&mut session, &mut cache).unwrap();
        let recovered_value = session.invoke(&get, &[&recovered]).unwrap();
        assert!(session.values_identical(&old_value, &recovered_value).unwrap(),
            "exact caller restoration recovers the original retained source value");
        assert!(cache.entries.is_empty(), "DATA values never mint strict source capabilities");
    }
}


#[test]
fn uncaptured_native_aliases_preserve_original_factory_evidence_and_exact_object_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let bridge = FormBridge::new(&mut session).unwrap();
        let mut cache = RootCache::default();
        let result = {
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            graph.capture_roots = true;
            graph.direct_reader_roots = true;
            let original = graph.symbol("alias-evidence").unwrap();
            let first = graph.recipe(Recipe::Alias(original)).unwrap();
            let second = graph.recipe(Recipe::Alias(first)).unwrap();
            let outer = graph.vector(&[original, first, second]).unwrap();
            graph.materialize_reader_controller(outer, &mut cache).unwrap()
                .expect("healthy transparent alias DATA must not decline")
        };
        session.collect().unwrap();
        let check = session.eval("(fn [v] (and (symbol? (nth v 0)) (identical? (nth v 0) (nth v 1)) (identical? (nth v 1) (nth v 2))))").unwrap();
        let passed = session.invoke(&check, &[&result]).unwrap();
        assert!(matches!(bridge.read(&mut session, &passed, 0..0).unwrap().kind, suss_reader::forms::Kind::Bool(true)));
        assert!(cache.data.is_empty(), "a request-only owner must not become persistent authority");
    }
}
