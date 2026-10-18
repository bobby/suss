use super::*;
use crate::portable::{
    hir::SourceNamespace,
    resolve::{DefinitionInfo, Global},
};
use std::sync::Arc;

type Revision = (Global, Arc<DefinitionInfo>);
fn owner(session: &Session, name: &str) -> Revision {
    let snapshot = session.compilation_snapshot();
    let namespace = SourceNamespace::capture(&snapshot.environment, session.phase());
    namespace
        .declarations
        .iter()
        .find(|(g, _)| g.name() == name)
        .map(|(g, info)| (g.clone(), info.clone()))
        .expect("actual analyzed revision")
}
fn get(session: &mut Session, value: &SessionValue, field: &str) -> SessionValue {
    let read = session
        .eval(&format!("(fn [value] (get value :{field}))"))
        .unwrap();
    session.invoke(&read, &[value]).unwrap()
}
fn build(
    session: &mut Session,
    bridge: &FormBridge,
    cache: &mut RootCache,
    owners: &[Revision],
) -> Result<SessionValue> {
    let _request = session.operation_budget_scope()?;
    let mut graph = AnalysisGraph::new(bridge, session);
    graph.capture_roots = true;
    let mut values = Vec::new();
    reserve(graph.session, &mut values, owners.len())?;
    for (global, revision) in owners {
        values.push(graph.declaration(global, Some(revision), 0)?);
    }
    let root = graph.recipe(Recipe::Vector(values))?;
    materialize_coherent(&mut graph, root, cache)?
        .ok_or_else(|| invalid("Test source must admit the coherent controller"))
}
fn nth(session: &mut Session, value: &SessionValue, index: usize) -> SessionValue {
    let read = session
        .eval(&format!("(fn [value] (nth value {index}))"))
        .unwrap();
    session.invoke(&read, &[value]).unwrap()
}
fn unchanged(cache: &RootCache, before: &[Rc<Certificate>]) -> bool {
    cache.entries.len() == before.len()
        && cache
            .entries
            .iter()
            .zip(before)
            .all(|(a, b)| Rc::ptr_eq(a, b))
}

#[test]
fn coherent_entrypoint_retains_raw_and_params_but_keeps_resolved_outer_fresh_after_gc() {
    let mut session = Session::new_macro().unwrap();
    session
        .eval("(def ^{:doc \"coherent-doc\"} coherent-callable (fn* ([x] x) ([x y] y)))")
        .unwrap();
    let (global, revision) = owner(&session, "coherent-callable");
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut values = super::super::super::reuse::DeclarationValues::default();
    let make = |session: &mut Session,
                cache: &mut super::super::super::reuse::DeclarationValues| {
        let _request = session.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::with_declaration_values(&bridge, session, cache);
        let raw = graph.declaration(&global, Some(&revision), 0).unwrap();
        let native = graph.source_roots.native(graph.session, raw).unwrap();
        let Recipe::Map(entries) = graph.recipes[native].as_ref().unwrap() else {
            panic!("raw map")
        };
        // Resolved info copies the exact selected child references into a fresh
        // outer map. This does not fabricate a second retained declaration owner.
        let info = graph.recipe(Recipe::Map(entries.clone())).unwrap();
        let root = graph.recipe(Recipe::Vector(vec![raw, info])).unwrap();
        graph.materialize(root).unwrap()
    };
    let first = make(&mut session, &mut values);
    assert!(
        !values.roots.entries.is_empty(),
        "production entrypoint must use source retention"
    );
    let raw = nth(&mut session, &first, 0);
    let info = nth(&mut session, &first, 1);
    let params = get(&mut session, &raw, "method-params");
    let meta = get(&mut session, &raw, "meta");
    for _ in 0..2 {
        session.collect().unwrap();
        let next = make(&mut session, &mut values);
        let next_raw = nth(&mut session, &next, 0);
        let next_info = nth(&mut session, &next, 1);
        let next_params = get(&mut session, &next_raw, "method-params");
        let info_params = get(&mut session, &next_info, "method-params");
        let next_meta = get(&mut session, &next_raw, "meta");
        assert!(session.values_identical(&raw, &next_raw).unwrap());
        assert!(session.values_identical(&params, &next_params).unwrap());
        assert!(session.values_identical(&params, &info_params).unwrap());
        assert!(session.values_identical(&meta, &next_meta).unwrap());
        assert!(!session.values_identical(&info, &next_info).unwrap());
        assert!(!session.values_identical(&next_raw, &next_info).unwrap());
    }
}

#[test]
fn coherent_later_reader_callback_rebinds_poisoned_old_raw_params_without_repair_or_replay() {
    let mut session = Session::new_macro().unwrap();
    session
        .eval(
            r#"
      (def ^{:doc "coherent-a"} coherent-a (fn* ([x] x) ([x y] y)))
      (def ^{:items #{101 102 103 104 105 106 107 108 109}} coherent-b 18)
      (def coherent-armed false)
      (def coherent-events [])
      (def coherent-old-raw nil)
      (def coherent-old-params nil)
      (def coherent-original-hash suss.core/hash)
    "#,
        )
        .unwrap();
    let a = owner(&session, "coherent-a");
    let b = owner(&session, "coherent-b");
    // Capture real delegated core hashing before bridge capture. The later
    // ReaderSet invokes this actual callback; no synthetic construction hook.
    session.enter_namespace("suss.core").unwrap();
    session
        .eval(
            r#"
      (def hash (fn [key]
        (do
          (if (and user/coherent-armed (= key 101))
            (do
              (set! user/coherent-armed false)
              (set! user/coherent-events (conj user/coherent-events :poison))
              (set! (.-__hash user/coherent-old-raw) 991)
              (aset (.-tail (first user/coherent-old-params)) 0 'poisoned-param))
            nil)
          (user/coherent-original-hash key))))
    "#,
        )
        .unwrap();
    session.enter_namespace("user").unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut cache = RootCache::default();
    let first = build(&mut session, &bridge, &mut cache, &[a.clone()]).unwrap();
    let old_raw = nth(&mut session, &first, 0);
    let old_params = get(&mut session, &old_raw, "method-params");
    let old_meta = get(&mut session, &old_raw, "meta");
    let arm = session
        .eval(
            r#"(fn [raw params]
      (do (set! coherent-old-raw raw) (set! coherent-old-params params)
          (set! coherent-events []) (set! coherent-armed true)))"#,
        )
        .unwrap();
    session.invoke(&arm, &[&old_raw, &old_params]).unwrap();
    let next = build(&mut session, &bridge, &mut cache, &[a, b]).unwrap();
    session.collect().unwrap();
    let new_raw = nth(&mut session, &next, 0);
    let new_params = get(&mut session, &new_raw, "method-params");
    let new_meta = get(&mut session, &new_raw, "meta");
    assert!(!session.values_identical(&old_raw, &new_raw).unwrap());
    assert!(!session.values_identical(&old_params, &new_params).unwrap());
    assert!(
        session.values_identical(&old_meta, &new_meta).unwrap(),
        "healthy independent child remains shared"
    );
    let observe = session
        .eval(
            r#"(fn [raw params]
      [(.-__hash coherent-old-raw) (first (first coherent-old-params))
       (first (first params)) (count coherent-events)])"#,
        )
        .unwrap();
    let observation = session.invoke(&observe, &[&new_raw, &new_params]).unwrap();
    let actual = bridge.read(&mut session, &observation, 0..1).unwrap();
    let expected = suss_reader::forms::read_forms("[991 poisoned-param x 1]")
        .unwrap()
        .remove(0);
    assert_eq!(
        super::super::super::form_key(&actual).unwrap(),
        super::super::super::form_key(&expected).unwrap(),
        "old poison survives; new source is original; later callback ran once"
    );
}

#[test]
fn coherent_genuine_revision_distinguishes_equal_source_and_rejects_foreign_and_reset_handles() {
    let mut session = Session::new_macro().unwrap();
    session
        .eval("(def ^{:doc \"revision\"} coherent-revision 17)")
        .unwrap();
    let original = owner(&session, "coherent-revision");
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut cache = RootCache::default();
    let first = build(&mut session, &bridge, &mut cache, &[original.clone()]).unwrap();
    let distinct = (original.0.clone(), Arc::new(original.1.as_ref().clone()));
    let second = build(&mut session, &bridge, &mut cache, &[distinct]).unwrap();
    let a = nth(&mut session, &first, 0);
    let b = nth(&mut session, &second, 0);
    assert!(
        !session.values_identical(&a, &b).unwrap(),
        "equal source is not the same revision"
    );
    let saved = cache.entries.clone();
    let mut foreign = Session::new_macro().unwrap();
    let foreign_bridge = FormBridge::new(&mut foreign).unwrap();
    assert!(
        build(
            &mut foreign,
            &foreign_bridge,
            &mut cache,
            &[original.clone()]
        )
        .is_err()
    );
    assert!(unchanged(&cache, &saved));
    session.reset().unwrap();
    let reset_bridge = FormBridge::new(&mut session).unwrap();
    assert!(build(&mut session, &reset_bridge, &mut cache, &[original]).is_err());
    assert!(unchanged(&cache, &saved));
    // The caller retires the session-owned controller on reset, exactly as for
    // captured bridge/metadata state; stale handles never migrate into new facts.
    cache = RootCache::default();
    session.eval("(def coherent-after-reset 31)").unwrap();
    let current = owner(&session, "coherent-after-reset");
    assert!(build(&mut session, &reset_bridge, &mut cache, &[current]).is_ok());
}

#[test]
fn coherent_fuel_and_irq_failures_publish_no_partial_cache_then_retry() {
    let mut session = Session::new_macro().unwrap();
    session
        .eval("(def ^{:audit {:x 17}} coherent-budget 17)")
        .unwrap();
    let revision = owner(&session, "coherent-budget");
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut cache = RootCache::default();
    let initial = build(&mut session, &bridge, &mut cache, &[revision.clone()]).unwrap();
    let saved = cache.entries.clone();
    session.set_operation_fuel(1);
    assert!(build(&mut session, &bridge, &mut cache, &[revision.clone()]).is_err());
    assert!(unchanged(&cache, &saved));
    session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
    let retry = build(&mut session, &bridge, &mut cache, &[revision.clone()]).unwrap();
    let old_raw = nth(&mut session, &initial, 0);
    let new_raw = nth(&mut session, &retry, 0);
    assert!(session.values_identical(&old_raw, &new_raw).unwrap());
    let saved = cache.entries.clone();
    let request = session.operation_budget_scope().unwrap();
    session.interrupt_handle().interrupt();
    let error = build(&mut session, &bridge, &mut cache, &[revision.clone()])
        .err()
        .unwrap();
    assert!(error.is_interrupt());
    assert!(unchanged(&cache, &saved));
    drop(request);
    assert!(build(&mut session, &bridge, &mut cache, &[revision]).is_ok());
}

#[test]
fn coherent_publication_preparation_is_atomic_at_every_fuel_irq_cut() {
    let mut session = Session::new_macro().unwrap();
    session.eval("(def coherent-publication 17)").unwrap();
    let revision = owner(&session, "coherent-publication");
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut cache = RootCache::default();
    build(&mut session, &bridge, &mut cache, &[revision]).unwrap();
    let updates = cache.entries.clone();
    // Count the exact replacement operation independently of guest construction.
    let work = session
        .with_operation_budget(|session| {
            let before = session.operation_fuel_remaining()?;
            drop(cache.replacement(session, &updates)?);
            Ok(before - session.operation_fuel_remaining()?)
        })
        .unwrap();
    for cut in 0..work {
        let saved = cache.entries.clone();
        session.set_operation_fuel(cut);
        assert!(
            session
                .with_operation_budget(|session| cache.replacement(session, &updates))
                .is_err()
        );
        assert!(unchanged(&cache, &saved));
    }
    session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
    let saved = cache.entries.clone();
    match session.with_operation_budget(|session| {
        session.interrupt_handle().interrupt();
        cache.replacement(session, &updates)
    }) {
        Err(error) => assert!(error.is_interrupt()),
        Ok(_) => panic!("publication preparation must observe interruption"),
    }
    assert!(unchanged(&cache, &saved));
    assert!(
        session
            .with_operation_budget(|session| cache.replacement(session, &updates))
            .is_ok()
    );
}

#[test]
fn coherent_later_callback_poisoned_fresh_authority_aborts_without_partial_publication() {
    let mut session = Session::new_macro().unwrap();
    session
        .eval(
            r#"
      (def ^{:doc "prior"} coherent-prior 17)
      (def ^{:doc "fresh-a"} coherent-fresh-a 18)
      (def ^{:items #{201 202 203 204 205 206 207 208 209}} coherent-fresh-b 19)
      (def coherent-prototype-armed false)
      (def coherent-prototype-events [])
      (def coherent-original-hash suss.core/hash)
    "#,
        )
        .unwrap();
    let prior = owner(&session, "coherent-prior");
    let a = owner(&session, "coherent-fresh-a");
    let b = owner(&session, "coherent-fresh-b");
    session.enter_namespace("suss.core").unwrap();
    session
        .eval(
            r#"
      (def hash (fn [key]
        (do
          (if (and user/coherent-prototype-armed (= key 201))
            (do
              (set! user/coherent-prototype-armed false)
              (set! user/coherent-prototype-events (conj user/coherent-prototype-events :poison))
              (aset (.-prototype PersistentArrayMap) "coherentAuthorityPoison" 99))
            nil)
          (user/coherent-original-hash key))))
    "#,
        )
        .unwrap();
    session.enter_namespace("user").unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut cache = RootCache::default();
    build(&mut session, &bridge, &mut cache, &[prior]).unwrap();
    let saved = cache.entries.clone();
    session
        .eval("(set! coherent-prototype-armed true)")
        .unwrap();
    assert!(
        build(&mut session, &bridge, &mut cache, &[a, b]).is_err(),
        "a callback-poisoned fresh value has no repair/retry/publication path"
    );
    assert!(unchanged(&cache, &saved));
    let events = session.eval("(count coherent-prototype-events)").unwrap();
    let actual = bridge.read(&mut session, &events, 0..1).unwrap();
    assert!(
        matches!(actual.kind, suss_reader::forms::Kind::Number(1.0)),
        "the failed fresh construction callback is not repeated"
    );
}

#[test]
fn coherent_native_alias_requires_exact_certified_child_not_equal_content_root() {
    let mut session = Session::new_macro().unwrap();
    session
        .eval("(def coherent-alias-fn (fn* ([x] x) ([x y] y)))")
        .unwrap();
    let revision = owner(&session, "coherent-alias-fn");
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut cache = RootCache::default();
    build(&mut session, &bridge, &mut cache, &[revision]).unwrap();
    let alias = cache
        .entries
        .iter()
        .find(|c| c.alias_root)
        .expect("selected source-function field is a native captured child alias")
        .clone();
    assert_eq!(alias.children.len(), 1);
    assert_eq!(
        alias.records.len(),
        0,
        "no fictitious constructor fact for an alias"
    );
    assert!(
        Certificate::checked(&mut session, &bridge, &alias, &alias.source)
            .unwrap()
            .is_some()
    );
    let copy = session.eval("(fn [value] (into {} value))").unwrap();
    let replacement = session.invoke(&copy, &[&alias.value]).unwrap();
    assert!(
        alias
            .source
            .witness
            .matches_logical_source(&mut session, &bridge, &replacement)
            .unwrap()
    );
    assert!(
        !session
            .values_identical(&replacement, &alias.children[0].value)
            .unwrap()
    );
    // Use the real alias's child domain and source witness; only the equal-data
    // replacement root is different. This must fail the exact reference proof.
    let recording = bridge.recording(&mut session).unwrap();
    let mut records = recording.into_records().unwrap();
    let child = Certificate::checked(
        &mut session,
        &bridge,
        &alias.children[0],
        &alias.children[0].source,
    )
    .unwrap()
    .unwrap();
    records.attach_source_child(&mut session, &child).unwrap();
    let forged = Rc::new(Certificate {
        source: alias.source.clone(),
        value: replacement,
        records,
        children: vec![child.certificate],
        alias_root: true,
    });
    assert!(
        Certificate::checked(&mut session, &bridge, &forged, &alias.source)
            .unwrap()
            .is_none()
    );
}

#[test]
fn coherent_changed_initial_class_authority_declines_before_any_construction_callback() {
    let mut session = Session::new_macro().unwrap();
    session
        .eval(
            r#"
      (def ^{:items #{401 402 403 404 405 406 407 408 409}} coherent-admission 17)
      (def coherent-admission-events [])
      (def coherent-original-hash suss.core/hash)
    "#,
        )
        .unwrap();
    let revision = owner(&session, "coherent-admission");
    session.enter_namespace("suss.core").unwrap();
    session
        .eval(
            r#"
      (def hash (fn [key]
        (do
          (if (= key 401)
            (set! user/coherent-admission-events (conj user/coherent-admission-events :constructed))
            nil)
          (user/coherent-original-hash key))))
    "#,
        )
        .unwrap();
    session.enter_namespace("user").unwrap();
    let constructor = session.eval("PersistentArrayMap").unwrap();
    let descriptor = session.data_descriptor(&constructor, true).unwrap();
    // Source class authority is explicitly schema-only, not prototype authority.
    // Change one actual schema slot while retaining valid UTF16 field values;
    // prototype-origin rejection is exercised by the separate later-callback test.
    session
        .inspect(&descriptor, |mut store, value| {
            let descriptor = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let schema = descriptor
                .field(&mut store, 1)?
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            let replacement = schema.get(&mut store, 1)?;
            schema.set(&mut store, 0, replacement)
        })
        .unwrap();

    let bridge = FormBridge::new(&mut session).unwrap();
    let mut cache = RootCache::default();
    {
        let _request = session.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.capture_roots = true;
        let root = graph
            .declaration(&revision.0, Some(&revision.1), 0)
            .unwrap();
        assert!(
            materialize_coherent(&mut graph, root, &mut cache)
                .unwrap()
                .is_none(),
            "known unsafe private factory authority declines before effects"
        );
    }
    assert!(cache.entries.is_empty());
    let count = session.eval("(count coherent-admission-events)").unwrap();
    let actual = bridge.read(&mut session, &count, 0..1).unwrap();
    assert!(matches!(actual.kind, suss_reader::forms::Kind::Number(0.0)));
}

#[test]
fn coherent_stack_population_prepays_every_fuel_irq_cut_with_reused_capacity() {
    let mut session = Session::new_macro().unwrap();
    let source: Vec<usize> = (0..17).collect();
    // Test fixtures deliberately allocate outside the measured operation. The
    // production helper must pay for all mapped reads/writes even with capacity.
    for retained in [false, true] {
        let fresh_stack = || {
            let mut stack = if retained {
                Vec::with_capacity(64)
            } else {
                Vec::new()
            };
            stack.push((99, false));
            stack
        };
        let work = session
            .with_operation_budget(|session| {
                let before = session.operation_fuel_remaining()?;
                let mut stack = fresh_stack();
                populate(
                    session,
                    &mut stack,
                    source.iter().rev().map(|id| (*id, false)),
                )?;
                Ok(before - session.operation_fuel_remaining()?)
            })
            .unwrap();
        for cut in 0..work {
            let mut stack = fresh_stack();
            let visited = std::cell::Cell::new(0);
            session.set_operation_fuel(cut);
            assert!(
                session
                    .with_operation_budget(|session| {
                        populate(
                            session,
                            &mut stack,
                            source.iter().rev().map(|id| {
                                visited.set(visited.get() + 1);
                                (*id, false)
                            }),
                        )
                    })
                    .is_err()
            );
            assert_eq!(visited.get(), 0, "no mapped entry before full prepayment");
            assert_eq!(stack, [(99, false)], "no partially populated stack");
        }
        session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        let mut stack = fresh_stack();
        let visited = std::cell::Cell::new(0);
        let error = session
            .with_operation_budget(|session| {
                session.interrupt_handle().interrupt();
                populate(
                    session,
                    &mut stack,
                    source.iter().rev().map(|id| {
                        visited.set(visited.get() + 1);
                        (*id, false)
                    }),
                )
            })
            .err()
            .expect("population must observe IRQ before work");
        assert!(error.is_interrupt());
        assert_eq!(visited.get(), 0);
        assert_eq!(stack, [(99, false)]);
        // Recovery preserves the exact reversed dependency order.
        session
            .with_operation_budget(|session| {
                populate(
                    session,
                    &mut stack,
                    source.iter().rev().map(|id| (*id, false)),
                )
            })
            .unwrap();
        assert_eq!(stack.len(), source.len() + 1);
        assert_eq!(stack[0], (99, false));
        for (entry, expected) in stack[1..].iter().zip(source.iter().rev()) {
            assert_eq!(*entry, (*expected, false));
        }
    }
}

#[test]
fn coherent_current_child_owner_ordinals_reject_equal_reference_swaps_missing_and_extra() {
    let mut session = Session::new_macro().unwrap();
    session
        .eval("(def coherent-key-order (fn* ([x] x) ([x y] y)))")
        .unwrap();
    let revision = owner(&session, "coherent-key-order");
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut cache = RootCache::default();
    build(&mut session, &bridge, &mut cache, &[revision.clone()]).unwrap();
    let parent = cache
        .entries
        .iter()
        .find(|c| matches!(c.source.key, Key::Declaration(..)))
        .expect("real raw declaration parent")
        .clone();
    // Genuine FINAL native field ordinals (e.g. two nil/Boolean fields), each
    // separately certified, may designate the very same immediate guest value.
    let mut pair = None;
    for (i, a) in parent.children.iter().enumerate() {
        for (j, b) in parent.children.iter().enumerate().skip(i + 1) {
            if a.source.key != b.source.key
                && matches!(a.source.key, Key::DeclarationField(..))
                && matches!(b.source.key, Key::DeclarationField(..))
                && session.values_identical(&a.value, &b.value).unwrap()
            {
                pair = Some((i, j));
                break;
            }
        }
        if pair.is_some() {
            break;
        }
    }
    let (a, b) = pair.expect("distinct genuine declaration-field owners share a nil/Boolean value");
    assert!(
        Certificate::checked(
            &mut session,
            &bridge,
            &parent.children[a],
            &parent.children[a].source
        )
        .unwrap()
        .is_some()
    );
    assert!(
        Certificate::checked(
            &mut session,
            &bridge,
            &parent.children[b],
            &parent.children[b].source
        )
        .unwrap()
        .is_some()
    );
    let _request = session.operation_budget_scope().unwrap();
    let mut graph = AnalysisGraph::new(&bridge, &mut session);
    graph.capture_roots = true;
    let root = graph
        .declaration(&revision.0, Some(&revision.1), 0)
        .unwrap();
    graph.finish_recipes().unwrap();
    let id = graph
        .source_roots
        .index(graph.session, root)
        .unwrap()
        .unwrap();
    for mode in 0..3 {
        let current = graph.source_root_expectation(id).unwrap().unwrap();
        let mut current = Rc::try_unwrap(current)
            .ok()
            .expect("fresh independent witness");
        assert!(
            same_child_keys(graph.session, &current.children, &parent.source.children).unwrap()
        );
        match mode {
            0 => current.children.swap(a, b),
            1 => {
                current.children.remove(a);
            }
            _ => {
                current.children.push(current.children[a].clone());
            }
        }
        // The parent and every retained child remain physically healthy. Only
        // the independently current native owner/ordinal inventory differs.
        assert!(Certificate::validate(graph.session, &bridge, &parent).unwrap());
        assert!(
            Certificate::checked(graph.session, &bridge, &parent, &Rc::new(current))
                .unwrap()
                .is_none()
        );
    }
    // same_children must independently reject the swapped owners even though
    // both selected rooted references are exactly equal.
    let layout = graph.source_root_children().unwrap();
    let mut states = Vec::new();
    states.resize_with(layout.len(), || State::Empty);
    for child in &parent.children {
        let child_id = graph
            .source_roots
            .roots
            .iter()
            .position(|capture| capture.owner.key(graph.session).unwrap() == child.source.key)
            .unwrap();
        states[child_id] = State::Old(child.clone());
    }
    let mut selected = layout[id].clone();
    selected.swap(a, b);
    assert!(!same_children(&mut graph, &parent, &selected, &states).unwrap());
    // Also exercise the native Alias composition itself, not only shared
    // immediates. Both the field alias and its callable child have complete,
    // separate certificates and exactly the same guest root.
    let alias = parent
        .children
        .iter()
        .find(|c| c.alias_root)
        .expect("real declaration field aliases its certified callable child");
    assert_eq!(alias.children.len(), 1);
    assert!(alias.source.key != alias.children[0].source.key);
    assert!(
        graph
            .session
            .values_identical(&alias.value, &alias.children[0].value)
            .unwrap()
    );
    let alias_id = graph
        .source_roots
        .roots
        .iter()
        .position(|capture| capture.owner.key(graph.session).unwrap() == alias.source.key)
        .unwrap();
    let current = graph.source_root_expectation(alias_id).unwrap().unwrap();
    let mut current = Rc::try_unwrap(current)
        .ok()
        .expect("independent alias witness");
    assert_eq!(current.children.len(), 1);
    current.children[0] = alias.source.key.clone();
    assert!(
        Certificate::checked(graph.session, &bridge, alias, &Rc::new(current))
            .unwrap()
            .is_none(),
        "physical alias equality cannot replace its current child owner"
    );
    assert!(!same_children(&mut graph, alias, &[alias_id], &states).unwrap());
}

#[test]
fn coherent_obsolete_revisions_retire_despite_cached_aliases_but_live_snapshots_survive() {
    let mut session = Session::new_macro().unwrap();
    session
        .eval("(def ^{:audit {:x 17}} coherent-retirement 17)")
        .unwrap();
    let original = owner(&session, "coherent-retirement");
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut cache = RootCache::default();
    let captured = Arc::new((*original.1).clone());
    let captured_id = Arc::as_ptr(&captured) as usize;
    let captured_weak = Arc::downgrade(&captured);
    let mut snapshot =
        SourceNamespace::capture(&session.compilation_snapshot().environment, session.phase());
    snapshot
        .declarations
        .insert(original.0.clone(), captured.clone());
    let snapshot = Arc::new(snapshot);
    build(
        &mut session,
        &bridge,
        &mut cache,
        &[(original.0.clone(), captured.clone())],
    )
    .unwrap();
    drop(captured);
    let mut expired = Vec::new();
    let mut outside_certificates = Vec::new();
    for _ in 0..4 {
        let revision = Arc::new((*original.1).clone());
        let weak = Arc::downgrade(&revision);
        build(
            &mut session,
            &bridge,
            &mut cache,
            &[(original.0.clone(), revision.clone())],
        )
        .unwrap();
        outside_certificates.extend(cache.entries.iter().cloned());
        drop(revision);
        assert!(
            weak.upgrade().is_none(),
            "cache/certificate/parent aliases must not own native revisions"
        );
        expired.push(weak);
    }
    let work = session
        .with_operation_budget(|session| {
            let before = session.operation_fuel_remaining()?;
            drop(cache.replacement(session, &[])?);
            Ok(before - session.operation_fuel_remaining()?)
        })
        .unwrap();
    for cut in 0..work {
        let saved = cache.entries.clone();
        session.set_operation_fuel(cut);
        assert!(
            session
                .with_operation_budget(|session| cache.replacement(session, &[]))
                .is_err()
        );
        assert!(unchanged(&cache, &saved));
    }
    session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
    let saved = cache.entries.clone();
    let error = session
        .with_operation_budget(|session| {
            session.interrupt_handle().interrupt();
            cache.replacement(session, &[])
        })
        .err()
        .expect("retirement must observe IRQ before commit");
    assert!(error.is_interrupt());
    assert!(unchanged(&cache, &saved));
    // This IRQ baseline is an intentional strong certificate owner. Release it
    // AFTER proving atomicity, BEFORE testing cache-only rooted-value retirement.
    drop(saved);
    cache.entries = session
        .with_operation_budget(|session| cache.replacement(session, &[]))
        .unwrap();
    assert!(
        cache.entries.iter().any(|c| matches!(c.source.key,
        Key::Declaration(_, pointer) if pointer == captured_id)),
        "a genuine captured SourceNamespace keeps the old declaration live"
    );
    for c in &cache.entries {
        assert!(
            session
                .with_operation_budget(|s| c.source.owner.live(s))
                .unwrap()
        );
    }
    assert!(expired.iter().all(|weak| weak.upgrade().is_none()));
    let guest_roots: Vec<_> = outside_certificates
        .iter()
        .filter(|c| {
            matches!(
                c.source.key,
                Key::Declaration(..) | Key::DeclarationField(..)
            )
        })
        .map(Rc::downgrade)
        .collect();
    drop(outside_certificates);
    drop(snapshot);
    cache.entries = session
        .with_operation_budget(|session| cache.replacement(session, &[]))
        .unwrap();
    assert!(captured_weak.upgrade().is_none());
    assert!(
        !cache.entries.iter().any(|c| matches!(
            c.source.key,
            Key::Declaration(..) | Key::DeclarationField(..)
        )),
        "all obsolete declaration/field revisions retire; genuinely live shared ASTs may remain"
    );
    assert!(
        guest_roots.iter().all(|weak| weak.upgrade().is_none()),
        "no cache/certificate child DAG keeps the retired rooted values alive"
    );
    session.collect().unwrap();
    // A still-live revision outside the latest request can be selected again.
    build(&mut session, &bridge, &mut cache, &[original]).unwrap();
}

#[test]
fn coherent_legacy_metadata_cache_snapshots_do_not_pin_native_revision_or_initializer_graph() {
    let mut session = Session::new_macro().unwrap();
    session
        .eval("(def ^{:audit {:x 17}} coherent-legacy-owner (fn [x] x))")
        .unwrap();
    let original = owner(&session, "coherent-legacy-owner");
    let mut source = (*original.1).clone();
    let initializer = source
        .initializer
        .as_ref()
        .expect("actual analyzed function initializer");
    source.initializer = Some(Arc::new((**initializer).clone()));
    let initializer = Arc::downgrade(source.initializer.as_ref().unwrap());
    let revision = Arc::new(source);
    let weak = Arc::downgrade(&revision);
    let bridge = FormBridge::new(&mut session).unwrap();
    let record = {
        let _request = session.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.declaration(&original.0, Some(&revision), 0).unwrap();
        graph.record_metadata(0).unwrap().unwrap()
    };
    let cached = session
        .with_operation_budget(|session| record.cache_snapshot(session))
        .unwrap();
    assert!(cached.source().externally_live());
    assert!(
        session
            .with_operation_budget(|session| cached.matches_recorded_source(session, &bridge))
            .unwrap()
    );
    drop(record);
    drop(revision);
    assert!(
        weak.upgrade().is_none(),
        "legacy cache source, witness and recordings retain no native revision"
    );
    assert!(!cached.source().externally_live());
    assert!(
        initializer.upgrade().is_none(),
        "cached witness/recordings must not retain the native initializer"
    );
}

#[test]
fn coherent_symbol_key_witnesses_cover_real_locals_defs_without_relaxing_duplicates_or_metadata() {
    use crate::portable::resolve::Phase;
    use crate::portable_macro_graph::source_facts::LogicalSourceWitness;
    use suss_reader::forms::{Form, Kind};
    let parse = |text: &str| {
        let mut forms = suss_reader::forms::read_forms(text).unwrap();
        assert_eq!(forms.len(), 1);
        forms.pop().unwrap()
    };
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = match phase {
            Phase::Runtime => Session::new_repl().unwrap(),
            Phase::Macro => Session::new_macro().unwrap(),
        };
        let bridge = FormBridge::new(&mut session).unwrap();
        // Pinned c429 Symbol IEquiv uses str only; metadata is ignored and
        // malformed decompositions can have the SAME FQN. Admission must not
        // confuse a tuple comparison with portable equality on those forms.
        let equalities = session
            .eval(
                r#"[(= (symbol "x") (with-meta (symbol "x") {:m 1}))
            (= (symbol "a" "x") (symbol "b" "x"))
            (= (new suss.core/Symbol nil "ns/x" "ns/x" nil nil) (symbol "ns" "x"))
            (= (new suss.core/Symbol "ns/x" "y" "ns/x/y" nil nil)
               (new suss.core/Symbol "ns" "x/y" "ns/x/y" nil nil))]"#,
            )
            .unwrap();
        assert_eq!(
            crate::portable_repl::display(&mut session, &equalities).unwrap(),
            "[true false true true]"
        );
        for (text, reordered, changed) in [
            ("{x 1 ns/x 2}", "{ns/x 2 x 1}", "{x 2 ns/x 1}"),
            ("{a/x 1 b/x 2}", "{b/x 2 a/x 1}", "{a/x 2 b/x 1}"),
            (
                "{^{:m 1} x 1 ns/x 2}",
                "{ns/x 2 ^{:m 1} x 1}",
                "{^{:m 2} x 1 ns/x 2}",
            ),
            ("#{x ns/x /}", "#{/ ns/x x}", "#{y ns/x /}"),
        ] {
            let witness = LogicalSourceWitness::from_source(&mut session, &parse(text))
                .unwrap()
                .expect("canonical symbol keys are supported native locals/defs data");
            let actual = session.eval(&format!("(quote {reordered})")).unwrap();
            assert!(
                witness
                    .matches_logical_source(&mut session, &bridge, &actual)
                    .unwrap()
            );
            let changed = session.eval(&format!("(quote {changed})")).unwrap();
            assert!(
                !witness
                    .matches_logical_source(&mut session, &bridge, &changed)
                    .unwrap()
            );
        }
        for text in [
            "{x 1 x 2}",
            "{x 1 ^{:m 1} x 2}",
            "#{x ^{:m 1} x}",
            "{[x] 1}",
            "#{[x]}",
            "{1 :numeric-map-key-still-unsupported}",
        ] {
            assert!(
                LogicalSourceWitness::from_source(&mut session, &parse(text))
                    .unwrap()
                    .is_none(),
                "duplicate or unsupported key must still decline: {text}"
            );
        }
        // Noncanonical decompositions can collide by FQN with canonical keys;
        // they must not become a second logical locator for the same Symbol.
        let key = Form {
            span: 0..1,
            metadata: vec![],
            kind: Kind::Symbol(suss_reader::Symbol::new("ns/x")),
        };
        let value = Form {
            span: 0..1,
            metadata: vec![],
            kind: Kind::Nil,
        };
        let ambiguous = Form {
            span: 0..1,
            metadata: vec![],
            kind: Kind::Map(vec![key, value]),
        };
        assert!(
            LogicalSourceWitness::from_source(&mut session, &ambiguous)
                .unwrap()
                .is_none()
        );
        // Further noncanonical constructor decompositions: ns containing a
        // slash, empty namespace and empty name never gain native key authority.
        for symbol in [
            suss_reader::Symbol::namespaced("ns/x", "y"),
            suss_reader::Symbol::namespaced("", "x"),
            suss_reader::Symbol::namespaced("ns", ""),
        ] {
            let key = Form {
                span: 0..1,
                metadata: vec![],
                kind: Kind::Symbol(symbol),
            };
            let value = Form {
                span: 0..1,
                metadata: vec![],
                kind: Kind::Nil,
            };
            let form = Form {
                span: 0..1,
                metadata: vec![],
                kind: Kind::Map(vec![key, value]),
            };
            assert!(
                LogicalSourceWitness::from_source(&mut session, &form)
                    .unwrap()
                    .is_none()
            );
        }
        let witness = LogicalSourceWitness::from_source(&mut session, &parse("{ns/x 1}"))
            .unwrap()
            .unwrap();
        for constructor in [
            r#"(new suss.core/Symbol nil "ns/x" "ns/x" nil nil)"#,
            r#"(new suss.core/Symbol "ns" "x" "forged" nil nil)"#,
            r#"(new suss.core/Symbol "ns/x" "y" "ns/x/y" nil nil)"#,
        ] {
            let malformed = session
                .eval(&format!("(array-map {constructor} 1)"))
                .unwrap();
            assert!(
                !witness
                    .matches_logical_source(&mut session, &bridge, &malformed)
                    .unwrap(),
                "malformed/noncanonical Symbol fields cannot satisfy the canonical witness: {constructor}"
            );
        }
        let canonical_key = Form {
            span: 0..1,
            metadata: vec![],
            kind: Kind::Symbol(suss_reader::Symbol::namespaced("ns", "x/y")),
        };
        let value = Form {
            span: 0..1,
            metadata: vec![],
            kind: Kind::Number(1.0),
        };
        let expected = Form {
            span: 0..1,
            metadata: vec![],
            kind: Kind::Map(vec![canonical_key, value]),
        };
        let same_fqn_witness = LogicalSourceWitness::from_source(&mut session, &expected)
            .unwrap()
            .unwrap();
        let same_fqn_wrong_split = session
            .eval(r#"(array-map (new suss.core/Symbol "ns/x" "y" "ns/x/y" nil nil) 1)"#)
            .unwrap();
        assert!(
            !same_fqn_witness
                .matches_logical_source(&mut session, &bridge, &same_fqn_wrong_split)
                .unwrap(),
            "same FQN cannot authorize a noncanonical namespace/name split"
        );
        let malformed = session
            .eval(r#"(array-map (new suss.core/Symbol nil 1 "x" nil nil) 1)"#)
            .unwrap();
        let error = witness
            .matches_logical_source(&mut session, &bridge, &malformed)
            .unwrap_err();
        assert!(
            error.to_string().contains("UTF16"),
            "structural decoder error must propagate: {error}"
        );
    }
}

// Preserve the genuine analyzed body-entry owners, rather than inventing a
// locals map from an equal guest value or a pruned source snapshot.
fn actual_body_environment(
    session: &Session,
    name: &str,
) -> crate::portable::hir::SourceMethodEnvironment {
    let (_, revision) = owner(session, name);
    let callable = revision
        .initializer
        .as_ref()
        .unwrap()
        .source
        .as_ref()
        .unwrap()
        .callable
        .as_ref()
        .unwrap();
    let source = callable.methods[0].body.source.as_ref().unwrap();
    crate::portable::hir::SourceMethodEnvironment {
        scope: source.scope.clone(),
        namespace_snapshot: source.namespace_snapshot.clone(),
        locals: source.locals.clone(),
        fields: source.fields.clone(),
        function_scopes: source.function_scopes.clone(),
        context: source.context,
    }
}

#[test]
fn coherent_locals_repeated_owner_requires_exact_native_map_and_preserves_outer_context() {
    use crate::portable::AnalysisContext;
    let mut session = Session::new_macro().unwrap();
    session.eval("(def coherent-context (fn [x] x))").unwrap();
    let env = actual_body_environment(&session, "coherent-context");
    assert!(!env.locals.is_empty());
    let bridge = FormBridge::new(&mut session).unwrap();
    let _request = session.operation_budget_scope().unwrap();
    let mut graph = AnalysisGraph::new(&bridge, &mut session);
    graph.capture_roots = true;
    let first = graph
        .environment(
            &env.namespace_snapshot,
            &env.scope,
            &env.locals,
            &env.fields,
            &env.function_scopes,
            AnalysisContext::Expression,
            0,
        )
        .unwrap();
    graph.finish_recipes().unwrap();
    let matching = |c: &Capture| {
        matches!(&c.owner, Owner::Locals(l, f)
        if Arc::ptr_eq(l, &env.locals) && Arc::ptr_eq(f, &env.fields))
    };
    let id = graph
        .source_roots
        .roots
        .iter()
        .position(|c| matching(c))
        .unwrap();
    let alias = graph.source_roots.roots[id].alias;
    let native = graph.source_roots.roots[id].native;
    assert!(matches!(graph.recipes[native], Some(Recipe::Map(_))));
    let before = graph.source_root_expectation(id).unwrap().unwrap();
    let second = graph
        .environment(
            &env.namespace_snapshot,
            &env.scope,
            &env.locals,
            &env.fields,
            &env.function_scopes,
            AnalysisContext::Statement,
            0,
        )
        .unwrap();
    graph.finish_recipes().unwrap();
    assert_ne!(
        first, second,
        "outer context remains part of each environment view"
    );
    assert_eq!(
        graph
            .source_roots
            .roots
            .iter()
            .filter(|c| matching(c))
            .count(),
        1
    );
    assert_eq!(graph.source_roots.roots[id].alias, alias);
    assert_eq!(graph.source_roots.roots[id].native, native);
    let after = graph.source_root_expectation(id).unwrap().unwrap();
    assert!(before.key == after.key);
    assert!(same_child_keys(graph.session, &before.children, &after.children).unwrap());
    // Fresh genuine map owners must remain separate even if map_values happened
    // to intern their exact native data to the same recipe node.
    for owner in [
        Owner::Locals(Arc::new((*env.locals).clone()), env.fields.clone()),
        Owner::Locals(env.locals.clone(), Arc::new((*env.fields).clone())),
    ] {
        let other = graph.capture_source_root(owner, native).unwrap();
        assert_ne!(alias, other, "BOTH genuine owner identities are required");
    }
    let root = graph.recipe(Recipe::Vector(vec![first, second])).unwrap();
    assert!(Plan::admit(&mut graph, root).unwrap().is_some());
}

#[test]
fn coherent_locals_conflicting_nodes_and_reader_occurrences_still_decline_whole_plan() {
    let mut session = Session::new_macro().unwrap();
    session.eval("(def coherent-conflict (fn [x] x))").unwrap();
    let env = actual_body_environment(&session, "coherent-conflict");
    let bridge = FormBridge::new(&mut session).unwrap();
    for reader in [false, true] {
        let _request = session.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.capture_roots = true;
        let view = graph
            .environment(
                &env.namespace_snapshot,
                &env.scope,
                &env.locals,
                &env.fields,
                &env.function_scopes,
                env.context,
                0,
            )
            .unwrap();
        graph.finish_recipes().unwrap();
        let id = graph
            .source_roots
            .roots
            .iter()
            .position(|c| {
                matches!(&c.owner,
            Owner::Locals(l, f) if Arc::ptr_eq(l, &env.locals) && Arc::ptr_eq(f, &env.fields))
            })
            .unwrap();
        let first = graph.source_roots.roots[id].alias;
        let native = graph.source_roots.roots[id].native;
        let Recipe::Map(entries) = graph.recipes[native].as_ref().unwrap() else {
            panic!("compiler locals map")
        };
        // Equal CONTENTS are deliberately insufficient; a different source/view
        // recipe node must retain the original duplicate-owner rejection.
        let other_native = graph
            .recipe(if reader {
                Recipe::ReaderMap(entries.clone())
            } else {
                Recipe::Map(entries.clone())
            })
            .unwrap();
        assert_ne!(native, other_native);
        let owner = || Owner::Locals(env.locals.clone(), env.fields.clone());
        let second = graph.capture_source_root(owner(), other_native).unwrap();
        assert_ne!(first, second);
        if reader {
            let third = graph.capture_source_root(owner(), other_native).unwrap();
            assert_ne!(
                second, third,
                "never coalesce even the exact repeated ReaderMap node"
            );
        }
        let root = graph
            .recipe(Recipe::Vector(vec![view, first, second]))
            .unwrap();
        assert!(
            Plan::admit(&mut graph, root).unwrap().is_none(),
            "conflicting exports decline before any construction; never last-wins"
        );
    }
}

#[test]
fn coherent_repeated_locals_views_preserve_reader_hash_effect_order_against_original_route() {
    use crate::portable::AnalysisContext;
    let mut session = Session::new_macro().unwrap();
    session
        .eval("(def coherent-effects (fn [a b c d e f g h i] a))")
        .unwrap();
    let env = actual_body_environment(&session, "coherent-effects");
    assert_eq!(env.locals.len(), 9);
    session.enter_namespace("suss.core").unwrap();
    session
        .eval(
            r#"
      (def coherent-view-events (array))
      (def coherent-view-all-events (array))
      (def coherent-view-original-hash hash)
      (def hash (fn [key]
        (do (aset coherent-view-all-events (alength coherent-view-all-events)
                  [(symbol? key) (keyword? key) (number? key) (string? key) (str key)])
            (if (number? key)
              (aset coherent-view-events (alength coherent-view-events) key) nil)
            (coherent-view-original-hash key))))
    "#,
        )
        .unwrap();
    session.enter_namespace("user").unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut previous = None;
    let mut previous_all = None;
    for coherent in [false, true] {
        session
            .eval("(do (set! suss.core/coherent-view-events (array)) (set! suss.core/coherent-view-all-events (array)))")
            .unwrap();
        let value = {
            let _request = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            graph.capture_roots = coherent;
            let first = graph
                .environment(
                    &env.namespace_snapshot,
                    &env.scope,
                    &env.locals,
                    &env.fields,
                    &env.function_scopes,
                    AnalysisContext::Expression,
                    0,
                )
                .unwrap();
            let mut items = Vec::new();
            for key in 101..=109 {
                items.push(graph.number(key).unwrap());
            }
            let reader = graph.recipe(Recipe::ReaderSet(items)).unwrap();
            let second = graph
                .environment(
                    &env.namespace_snapshot,
                    &env.scope,
                    &env.locals,
                    &env.fields,
                    &env.function_scopes,
                    AnalysisContext::Statement,
                    0,
                )
                .unwrap();
            // The SAME reader recipe appears twice; its ordinary occurrence
            // effects must not disappear with the locals alias coalescing.
            let root = graph
                .recipe(Recipe::Vector(vec![first, reader, second, reader]))
                .unwrap();
            if coherent {
                materialize_coherent(&mut graph, root, &mut RootCache::default())
                    .unwrap()
                    .expect("coherent source view must admit")
            } else {
                graph.materialize_using(root, &bridge).unwrap()
            }
        };
        let events = session
            .eval("(seq suss.core/coherent-view-events)")
            .unwrap();
        let form = bridge.read(&mut session, &events, 0..1).unwrap();
        let expected = "(101 102 103 104 105 106 107 108 109 101 102 103 104 105 106 107 108 109)";
        assert_eq!(
            crate::portable_repl::display(&mut session, &events).unwrap(),
            expected
        );
        if let Some(old) = &previous {
            assert_eq!(old, &form, "actual callback values/count/order unchanged");
        }
        previous = Some(form);
        let all_events = session
            .eval("(seq suss.core/coherent-view-all-events)")
            .unwrap();
        let all_form = bridge.read(&mut session, &all_events, 0..1).unwrap();
        if let Some(old) = &previous_all {
            assert_eq!(
                old, &all_form,
                "ALL delegated public-hash callback key types/values/count/order preserved"
            );
        }
        previous_all = Some(all_form);
        let inspect = session
            .eval(
                r#"(fn [v]
          [(identical? (:locals (nth v 0)) (:locals (nth v 2)))
           (:context (nth v 0)) (:context (nth v 2))
           (= (nth v 1) #{101 102 103 104 105 106 107 108 109})
           (= (nth v 3) #{101 102 103 104 105 106 107 108 109})])"#,
            )
            .unwrap();
        let observed = session.invoke(&inspect, &[&value]).unwrap();
        assert_eq!(
            crate::portable_repl::display(&mut session, &observed).unwrap(),
            "[true :expr :statement true true]"
        );
    }
}

#[test]
fn coherent_reader_before_every_source_alias_preserves_original_effect_order() {
    use crate::portable::AnalysisContext;
    let mut session = Session::new_macro().unwrap();
    session
        .eval("(def coherent-effects (fn [a b c d e f g h i] a))")
        .unwrap();
    let env = actual_body_environment(&session, "coherent-effects");
    assert_eq!(env.locals.len(), 9);
    session.enter_namespace("suss.core").unwrap();
    session
        .eval(
            r#"
      (def coherent-view-events (array))
      (def coherent-view-all-events (array))
      (def coherent-view-original-hash hash)
      (def hash (fn [key]
        (do (aset coherent-view-all-events (alength coherent-view-all-events)
                  [(symbol? key) (keyword? key) (number? key) (string? key) (str key)])
            (if (number? key)
              (aset coherent-view-events (alength coherent-view-events) key) nil)
            (coherent-view-original-hash key))))
    "#,
        )
        .unwrap();
    session.enter_namespace("user").unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut previous = None;
    let mut previous_all = None;
    for coherent in [false, true] {
        session
            .eval("(do (set! suss.core/coherent-view-events (array)) (set! suss.core/coherent-view-all-events (array)))")
            .unwrap();
        let value = {
            let _request = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            graph.capture_roots = coherent;
            let first = graph
                .environment(
                    &env.namespace_snapshot,
                    &env.scope,
                    &env.locals,
                    &env.fields,
                    &env.function_scopes,
                    AnalysisContext::Expression,
                    0,
                )
                .unwrap();
            let mut items = Vec::new();
            for key in 101..=109 {
                items.push(graph.number(key).unwrap());
            }
            let reader = graph.recipe(Recipe::ReaderSet(items)).unwrap();
            let second = graph
                .environment(
                    &env.namespace_snapshot,
                    &env.scope,
                    &env.locals,
                    &env.fields,
                    &env.function_scopes,
                    AnalysisContext::Statement,
                    0,
                )
                .unwrap();
            // Reader comes BEFORE every source alias. The same reader recipe
            // occurs again later; neither ordering nor occurrence may be lost.
            let root = graph
                .recipe(Recipe::Vector(vec![reader, first, second, reader]))
                .unwrap();
            if coherent {
                materialize_coherent(&mut graph, root, &mut RootCache::default())
                    .unwrap()
                    .expect("coherent source view must admit")
            } else {
                graph.materialize_using(root, &bridge).unwrap()
            }
        };
        let events = session
            .eval("(seq suss.core/coherent-view-events)")
            .unwrap();
        let form = bridge.read(&mut session, &events, 0..1).unwrap();
        let expected = "(101 102 103 104 105 106 107 108 109 101 102 103 104 105 106 107 108 109)";
        assert_eq!(
            crate::portable_repl::display(&mut session, &events).unwrap(),
            expected
        );
        if let Some(old) = &previous {
            assert_eq!(old, &form, "actual callback values/count/order unchanged");
        }
        previous = Some(form);
        let all_events = session
            .eval("(seq suss.core/coherent-view-all-events)")
            .unwrap();
        let all_form = bridge.read(&mut session, &all_events, 0..1).unwrap();
        if let Some(old) = &previous_all {
            assert_eq!(
                old, &all_form,
                "ALL delegated public-hash callback key types/values/count/order preserved"
            );
        }
        previous_all = Some(all_form);
        let inspect = session
            .eval(
                r#"(fn [v]
          [(identical? (:locals (nth v 1)) (:locals (nth v 2)))
           (:context (nth v 1)) (:context (nth v 2))
           (= (nth v 1) #{101 102 103 104 105 106 107 108 109})
           (= (nth v 3) #{101 102 103 104 105 106 107 108 109})])"#,
            )
            .unwrap();
        let observed = session.invoke(&inspect, &[&value]).unwrap();
        assert_eq!(
            crate::portable_repl::display(&mut session, &observed).unwrap(),
            "[true :expr :statement true true]"
        );
    }
}
