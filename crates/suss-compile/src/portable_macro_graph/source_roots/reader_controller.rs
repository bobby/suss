//! Whole-build source controller: strict retained authority and separate
//! request-local complete DATA health, with no conversion of DATA to authority.
//! One invocation owns all selected roots, one dependency context, one final
//! closure, and one retained-cache replacement. No error-triggered replay.
use super::reader_plan::{ChildMode, NativeReaderPlan};
use super::*;

fn expectation(plan: &NativeReaderPlan, id: usize) -> Result<&Rc<Expectation>> {
    plan.expected
        .get(id)
        .and_then(Option::as_ref)
        .ok_or_else(|| invalid("Missing admitted reader source expectation"))
}
fn selected_children(
    graph: &mut AnalysisGraph<'_>,
    plan: &NativeReaderPlan,
    id: usize,
    states: &[Option<Rc<Certificate>>],
) -> Result<Vec<CheckedChild>> {
    let mut children = Vec::new();
    reserve(graph.session, &mut children, plan.layout[id].len())?;
    for &child in &plan.layout[id] {
        graph.session.charge_host_work(2)?;
        let certificate = states[child]
            .as_ref()
            .ok_or_else(|| invalid("Unbuilt reader source child"))?;
        let checked = Certificate::checked(
            graph.session,
            graph.bridge,
            certificate,
            expectation(plan, child)?,
        )?
        .ok_or_else(|| invalid("Reader source child changed before parent selection"))?;
        children.push(checked);
    }
    Ok(children)
}
fn same_children(
    graph: &mut AnalysisGraph<'_>,
    plan: &NativeReaderPlan,
    id: usize,
    candidate: &Certificate,
    states: &[Option<Rc<Certificate>>],
) -> Result<bool> {
    graph.session.charge_host_work(2)?;
    let mut valid = candidate.children.len() == plan.layout[id].len();
    for (old, &child) in candidate.children.iter().zip(&plan.layout[id]) {
        graph.session.charge_host_work(2)?;
        let current = states[child]
            .as_ref()
            .ok_or_else(|| invalid("Missing selected reader source child"))?;
        let expected = expectation(plan, child)?;
        graph
            .session
            .charge_host_work(key_comparison_work(&old.source.key, &expected.key))?;
        valid &= old.source.key == expected.key;
        valid &= graph.session.values_identical(&old.value, &current.value)?;
    }
    Ok(valid)
}
// The reference-only path is selected from native recipes, not observed guest
// equality. It is separate from constructor snapshots and never rewrites a child.
fn alias_child(
    graph: &mut AnalysisGraph<'_>,
    plan: &NativeReaderPlan,
    id: usize,
) -> Result<Option<usize>> {
    let mut cursor = graph.source_roots.roots[id].native;
    for _ in 0..graph.recipes.len() {
        graph
            .session
            .charge_host_work(plan.layout[id].len() as u64 + 3)?;
        let mut found = None;
        for &child in &plan.layout[id] {
            if graph.source_roots.roots[child].alias == cursor {
                found = Some(child);
                break;
            }
        }
        if let Some(child) = found {
            if plan.layout[id].len() != 1 {
                return Err(invalid("Ambiguous native reader root alias"));
            }
            return Ok(Some(child));
        }
        match graph.recipes.get(cursor).and_then(Option::as_ref) {
            Some(Recipe::Alias(next)) => cursor = *next,
            Some(_) => return Ok(None),
            None => return Err(invalid("Missing reader root native recipe")),
        }
    }
    Err(invalid("Cyclic reader root native alias"))
}

impl AnalysisGraph<'_> {
    // A bare outer envelope has no persistent source owner. Its request-local
    // native owner closes the SAME whole-build source proof but is never cached.
    // Existing captured root aliases do not acquire an extra identity/barrier.
    pub(super) fn reader_request_export(&mut self, root: Value) -> Result<Value> {
        let mut cursor = root;
        for _ in 0..self.recipes.len() {
            self.session.charge_host_work(3)?;
            if self.source_roots.index(self.session, cursor)?.is_some() {
                return Ok(cursor);
            }
            match self.recipes.get(cursor).and_then(Option::as_ref) {
                Some(Recipe::Alias(next)) => cursor = *next,
                Some(_) => {
                    self.session
                        .charge_host_work((2 * std::mem::size_of::<usize>() + 6) as u64)?;
                    let owner = Arc::new(());
                    return self.capture_source_root(Owner::Request(owner), root);
                }
                None => return Err(invalid("Unknown reader request export")),
            }
        }
        Err(invalid("Cyclic reader request export"))
    }
    /// Return None only on native strategy decline BEFORE guest construction.
    /// Once construction starts every error propagates. No legacy interleaving,
    /// exposed-key repair, repeated callback, or partial retained publication.
    pub(in crate::portable_macro_graph) fn materialize_strict_reader_controller(
        &mut self,
        root: Value,
        cache: &mut RootCache,
    ) -> Result<Option<crate::portable_session::SessionValue>> {
        let _request = self.session.operation_budget_scope()?;
        self.session.charge_host_work(2)?;
        if !self.capture_roots || !self.direct_reader_roots {
            return Ok(None);
        }
        if !self.session.source_controller_ready()? {
            return Ok(None);
        }
        // Unsupported current factory layouts choose the ordinary whole-build
        // route BEFORE callbacks, never a retry after effects. The preflight
        // does not substitute for any later current class/prototype validation.
        let preflight = self.bridge.recording(self.session)?.into_records()?;
        if !preflight.initial_class_precheck(self.session)? {
            return Ok(None);
        }
        drop(preflight);
        self.finish_recipes()?;
        let root = self.reader_request_export(root)?;
        let Some(plan) = self.admit_reader_controller(root)? else {
            return Ok(None);
        };
        let root_id = self
            .source_roots
            .index(self.session, root)?
            .ok_or_else(|| invalid("Reader request lacks an export boundary"))?;
        let mut states: Vec<Option<Rc<Certificate>>> = Vec::new();
        reserve(self.session, &mut states, plan.layout.len())?;
        self.session.charge_host_work(plan.layout.len() as u64)?;
        states.resize_with(plan.layout.len(), || None);
        let mut exposed = Vec::new();
        reserve(self.session, &mut exposed, plan.layout.len())?;
        self.session.charge_host_work(plan.layout.len() as u64)?;
        exposed.resize(plan.layout.len(), false);
        for &id in &plan.order {
            let expected = expectation(&plan, id)?;
            // No request-local placeholder is ever retained as a cache key.
            self.session.charge_host_work(1)?;
            let candidate = if matches!(expected.key, Key::Request(_)) {
                None
            } else {
                cache.lookup(self.session, self.bridge, expected)?
            };
            if let Some(checked) = candidate {
                if same_children(self, &plan, id, &checked.certificate, &states)? {
                    self.session.charge_host_work(1)?;
                    states[id] = Some(checked.certificate);
                    continue;
                }
            }
            let children = selected_children(self, &plan, id, &states)?;
            // Conservative exposure before construction: every admitted native
            // child is reachable. Keys are never replaced after this point.
            for &child in &plan.layout[id] {
                self.session.charge_host_work(2)?;
                if plan.modes[child] == ChildMode::CheckedDirectKey {
                    exposed[child] = true;
                }
            }
            let certificate = if let Some(child) = alias_child(self, &plan, id)? {
                self.session.charge_host_work(2)?;
                let checked = children
                    .into_iter()
                    .next()
                    .ok_or_else(|| invalid("Missing native alias child"))?;
                if checked.certificate.source.key != expectation(&plan, child)?.key {
                    return Err(invalid("Wrong native alias child owner"));
                }
                Certificate::finish_native_alias(
                    self.session,
                    self.bridge,
                    expected.clone(),
                    checked,
                )?
            } else {
                let bound = self
                    .materialize_mixed_source_parent(id, children)?
                    .ok_or_else(|| invalid("Admitted reader parent became unsupported"))?;
                Certificate::finish_bound(self.session, self.bridge, bound)?
            }
            .ok_or_else(|| invalid("Fresh reader source requires separate data-health proof"))?;
            self.session.charge_host_work(1)?;
            states[id] = Some(certificate);
        }
        // Every construction callback is over. This stage deliberately FAILS
        // invalid postcallback children; it cannot replay an exposed key or
        // rebuild an already exposed parent. The later health-mode integration
        // may add only proven unexposed private-value transitions.
        let mut updates = Vec::new();
        reserve(self.session, &mut updates, plan.order.len())?;
        for &id in &plan.order {
            self.session.charge_host_work(2)?;
            let certificate = states[id]
                .as_ref()
                .ok_or_else(|| invalid("Missing final reader proof"))?;
            let valid = Certificate::checked(
                self.session,
                self.bridge,
                certificate,
                expectation(&plan, id)?,
            )?
            .is_some();
            let same = same_children(self, &plan, id, certificate, &states)?;
            if !(valid && same) {
                return Err(invalid(if exposed[id] {
                    "Exposed direct reader key changed before publication"
                } else {
                    "Reader source changed before whole-build publication"
                }));
            }
            self.session.charge_host_work(2)?;
            if !matches!(certificate.source.key, Key::Request(_)) {
                updates.push(certificate.clone());
            }
        }
        self.session.charge_host_work(2)?;
        let value = states[root_id].as_ref().unwrap().value.clone();
        let replacement = cache.replacement(self.session, &updates)?;
        // No callback or fallible debit follows this single commit. Existing
        // partial construction effects are never rolled back or repeated.
        cache.entries = replacement;
        Ok(Some(value))
    }
    /// Complete build-local health controller. Only Certified proofs contribute
    /// persistent updates; Uncached data cannot be enrolled as a child domain.
    pub(in crate::portable_macro_graph) fn materialize_eager_reader_controller(
        &mut self,
        root: Value,
        cache: &mut RootCache,
    ) -> Result<Option<crate::portable_session::SessionValue>> {
        let _request = self.session.operation_budget_scope()?;
        self.session.charge_host_work(2)?;
        if !self.capture_roots || !self.direct_reader_roots {
            return Ok(None);
        }
        if !self.session.source_controller_ready()? {
            return Ok(None);
        }
        // Unsupported current factory layouts choose the ordinary whole-build
        // route BEFORE callbacks, never a retry after effects. The preflight
        // does not substitute for any later current class/prototype validation.
        let preflight = self.bridge.recording(self.session)?.into_records()?;
        if !preflight.initial_class_precheck(self.session)? {
            return Ok(None);
        }
        drop(preflight);
        self.finish_recipes()?;
        let root = self.reader_request_export(root)?;
        let Some(plan) = self.admit_reader_controller(root)? else {
            return Ok(None);
        };
        let root_id = self
            .source_roots
            .index(self.session, root)?
            .ok_or_else(|| invalid("Missing whole reader export boundary"))?;
        let mut states: Vec<Option<FreshSourceProof>> = Vec::new();
        reserve(self.session, &mut states, plan.layout.len())?;
        self.session.charge_host_work(plan.layout.len() as u64)?;
        states.resize_with(plan.layout.len(), || None);
        let mut exposed = Vec::new();
        reserve(self.session, &mut exposed, plan.layout.len())?;
        self.session.charge_host_work(plan.layout.len() as u64)?;
        exposed.resize(plan.layout.len(), false);
        for &id in &plan.order {
            let expected = expectation(&plan, id)?;
            self.session.charge_host_work(1)?;
            let candidate = if matches!(expected.key, Key::Request(_)) {
                None
            } else {
                cache.lookup(self.session, self.bridge, expected)?
            };
            if let Some(candidate) = candidate {
                self.session.charge_host_work(1)?;
                let mut same = candidate.certificate.children.len() == plan.layout[id].len();
                for (old, &child) in candidate.certificate.children.iter().zip(&plan.layout[id]) {
                    self.session.charge_host_work(2)?;
                    let selected = states[child]
                        .as_ref()
                        .ok_or_else(|| invalid("Unbuilt whole reader child"))?;
                    self.session.charge_host_work(key_comparison_work(
                        &old.source.key,
                        &selected.source().key,
                    ))?;
                    same &= old.source.key == selected.source().key;
                    same &= self
                        .session
                        .values_identical(&old.value, selected.value())?;
                }
                if same {
                    self.session.charge_host_work(1)?;
                    states[id] = Some(FreshSourceProof::certified(
                        self.session,
                        candidate.certificate,
                    )?);
                    continue;
                }
            }
            for &child in &plan.layout[id] {
                self.session.charge_host_work(2)?;
                if plan.modes[child] == ChildMode::CheckedDirectKey {
                    exposed[child] = true;
                }
            }
            let native = self.source_roots.roots[id].native;
            let pending = self.construct_transient_reader_parent(native, id, &plan, &states)?;
            let proof = self.finish_transient_reader_parent(pending)?;
            self.session.charge_host_work(1)?;
            states[id] = Some(proof);
        }
        // ALL construction callbacks are complete. Every parent and every child
        // is rechecked, including already-finished earlier roots. Health-only
        // failures are never converted to fresh snapshots or strict authority.
        let mut updates = Vec::new();
        reserve(self.session, &mut updates, plan.order.len())?;
        for &id in &plan.order {
            self.session.charge_host_work(2)?;
            let proof = states[id]
                .as_ref()
                .ok_or_else(|| invalid("Missing final whole reader proof"))?;
            if !proof.checked(self.session, self.bridge, expectation(&plan, id)?)? {
                return Err(invalid(if exposed[id] {
                    "Exposed direct reader key invalidated by a later callback"
                } else {
                    "Reader source invalidated before whole-build publication"
                }));
            }
            self.session.charge_host_work(1)?;
            if let Some(strict) = proof.certificate() {
                self.session.charge_host_work(2)?;
                if !matches!(strict.source.key, Key::Request(_)) {
                    updates.push(strict.clone());
                }
            }
        }
        self.session.charge_host_work(2)?;
        let value = states[root_id].as_ref().unwrap().value().clone();
        let replacement = cache.replacement(self.session, &updates)?;
        cache.entries = replacement; // single infallible cache commit, last action
        Ok(Some(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::{
        hir,
        resolve::{Environment, Phase},
    };
    use crate::portable_macro_data::FormBridge;
    fn environment(previous: &Environment, input: &str, phase: Phase) -> Environment {
        let forms = suss_reader::forms::read_forms(input).unwrap();
        hir::prepare(&forms, 0..input.len(), previous, phase)
            .unwrap()
            .1
    }
    fn declaration(environment: &Environment, phase: Phase) -> (Global, Arc<DefinitionInfo>) {
        let binding = environment
            .resolve(phase, &suss_reader::Symbol::new("watched"), 0..0)
            .unwrap();
        let global = binding.global().clone();
        let info = environment.shared_definition_info(&global).unwrap().clone();
        (global, info)
    }
    fn build(
        session: &mut Session,
        bridge: &FormBridge,
        cache: &mut RootCache,
        environment: &Environment,
        phase: Phase,
    ) -> crate::portable_session::SessionValue {
        let (global, info) = declaration(environment, phase);
        let mut graph = AnalysisGraph::new(bridge, session);
        graph.capture_roots = true;
        graph.direct_reader_roots = true;
        let root = graph.declaration(&global, Some(&info), 0).unwrap();
        graph
            .materialize_reader_controller(root, cache)
            .unwrap_or_else(|error| panic!(
                "reader controller build phase={phase:?} strict_cache={} data_cache={} remaining={:?}: {error:?}",
                cache.entries.len(), cache.data.len(), graph.session.operation_fuel_remaining()))
            .expect("reader controller fixture must admit its declaration")
    }
    #[test]
    fn whole_build_preserves_cold_key_alias_and_warm_original_meta_across_type_revisions() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let first = environment(&Environment::default(), "(def ^number watched 17)", phase);
            let mut session = match phase {
                Phase::Runtime => Session::new_repl().unwrap(),
                Phase::Macro => Session::new_macro().unwrap(),
            };
            let bridge = FormBridge::new(&mut session).unwrap();
            let project = session.eval("(fn [a b] (let [key (fn [m] (reduce (fn [found k] (if (= (str k) \":tag\") k found)) nil (keys m)))] [(identical? (key a) (key (get a :meta))) (identical? (get a :meta) (get b :meta)) (identical? (key (get a :meta)) (key (get b :meta)))]))").unwrap();
            let mut cache = RootCache::default();
            let cold = build(&mut session, &bridge, &mut cache, &first, phase);
            session.collect().unwrap();
            let warm = build(&mut session, &bridge, &mut cache, &first, phase);
            let value = session.invoke(&project, &[&cold, &warm]).unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &value).unwrap(),
                "[true true true]"
            );
            let later = environment(&first, "(deftype* watched [] nil nil)", phase);
            let latest = environment(&later, "(deftype* watched [field] nil nil)", phase);
            for current in [&later, &latest] {
                session.collect().unwrap();
                let actual = build(&mut session, &bridge, &mut cache, current, phase);
                let value = session.invoke(&project, &[&cold, &actual]).unwrap();
                assert_eq!(
                    crate::portable_repl::display(&mut session, &value).unwrap(),
                    "[true true true]"
                );
            }
        }
    }
    #[test]
    fn whole_controller_retains_original_explicit_nested_meta_after_new_holder_revision() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let first = environment(
                &Environment::default(),
                "(deftype* ^{:meta {:marker nested-symbol :nested {:leaf another-symbol}}} watched [] nil nil)",
                phase,
            );
            let later = environment(&first, "(deftype* watched [field] nil nil)", phase);
            let mut session = match phase {
                Phase::Runtime => Session::new_repl().unwrap(),
                Phase::Macro => Session::new_macro().unwrap(),
            };
            let bridge = FormBridge::new(&mut session).unwrap();
            let project = session.eval("(fn [a b] [(identical? (get a :meta) (get b :meta)) (identical? (get (get a :meta) :nested) (get (get b :meta) :nested)) (identical? (get (get (get a :meta) :nested) :leaf) (get (get (get b :meta) :nested) :leaf))])").unwrap();
            let mut cache = RootCache::default();
            let cold = build(&mut session, &bridge, &mut cache, &first, phase);
            session.collect().unwrap();
            let warm = build(&mut session, &bridge, &mut cache, &first, phase);
            let current = build(&mut session, &bridge, &mut cache, &later, phase);
            for other in [&warm, &current] {
                let observed = session.invoke(&project, &[&cold, other]).unwrap();
                assert_eq!(
                    crate::portable_repl::display(&mut session, &observed).unwrap(),
                    "[true true true]"
                );
            }
        }
    }
    #[test]
    fn only_exact_locals_owners_and_native_map_share_a_capture() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.capture_roots = true;
        graph.direct_reader_roots = true;
        let locals = Arc::new(std::collections::HashMap::new());
        let fields = Arc::new(std::collections::HashMap::new());
        let original = graph.recipe(Recipe::Map(vec![])).unwrap();
        let first = graph
            .capture_source_root(Owner::Locals(locals.clone(), fields.clone()), original)
            .unwrap();
        let shared = graph
            .capture_source_root(Owner::Locals(locals.clone(), fields.clone()), original)
            .unwrap();
        assert_eq!(first, shared);
        let equal_map = graph.recipe(Recipe::Map(vec![])).unwrap();
        let distinct = graph
            .capture_source_root(Owner::Locals(locals.clone(), fields.clone()), equal_map)
            .unwrap();
        assert_ne!(
            first, distinct,
            "equal contents cannot merge native source nodes"
        );
        let other_locals = Arc::new(std::collections::HashMap::new());
        let foreign = graph
            .capture_source_root(Owner::Locals(other_locals, fields), original)
            .unwrap();
        assert_ne!(
            first, foreign,
            "both original owner allocations are required"
        );
    }
    #[test]
    fn whole_controller_failure_keeps_the_entire_previous_retained_forest_unchanged() {
        let environment = environment(
            &Environment::default(),
            "(def ^number watched 17)",
            Phase::Macro,
        );
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let mut cache = RootCache::default();
        let first = crate::portable_session::memo_leaf_test_support::with_compilation_data_fuel(
            &mut session, |session| Ok(build(session, &bridge, &mut cache, &environment, Phase::Macro)),
        ).unwrap();
        let previous: Vec<_> = cache.entries.iter().map(Rc::as_ptr).collect();
        assert!(
            !cache.data.is_empty(),
            "a warm source-data forest must exist before the failure"
        );
        let previous_data: Vec<_> = cache.data.iter().map(Rc::as_ptr).collect();
        let (global, holder) = declaration(&environment, Phase::Macro);
        // Warm-cache failure occurs before construction, within the actual one
        // cumulative request; it cannot publish a prefix of retained updates.
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.capture_roots = true;
        graph.direct_reader_roots = true;
        let root = graph.declaration(&global, Some(&holder), 0).unwrap();
        graph.session.set_operation_fuel(1);
        let op = graph.session.operation_budget_scope().unwrap();
        let error = graph
            .materialize_reader_controller(root, &mut cache)
            .unwrap_err();
        assert!(matches!(error, SessionError::Trap(ref error)
            if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
        assert_eq!(
            cache.entries.iter().map(Rc::as_ptr).collect::<Vec<_>>(),
            previous
        );
        assert_eq!(
            cache.data.iter().map(Rc::as_ptr).collect::<Vec<_>>(),
            previous_data
        );
        drop(op);
        drop(graph);
        session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        let next = build(
            &mut session,
            &bridge,
            &mut cache,
            &environment,
            Phase::Macro,
        );
        assert!(session.values_identical(&first, &next).unwrap());
    }
}

#[cfg(test)]
mod health_tests {
    use super::*;
    use crate::portable::{
        hir,
        resolve::Phase,
    };
    use crate::portable_macro_data::FormBridge;
    #[test]
    fn uncached_numeric_key_data_never_mints_direct_key_or_retained_authority() {
        // Preserve the original explicit-map program as the cold case. The
        // reader's shorthand supplies the separate protected literal origin.
        for (text, born_literal) in [
            ("(def ^{:tag number} watched 17)", false),
            ("(def ^number watched 17)", true),
        ] {
            let mut source_session = Session::new_repl().unwrap();
            source_session.eval(text).unwrap();
            let snapshot = source_session.compilation_snapshot();
            let namespace = hir::SourceNamespace::capture(&snapshot.environment, Phase::Runtime);
            let (global, info) = namespace.declarations.iter()
                .find(|(global, _)| global.name() == "watched").unwrap();
            let mut session = Session::new_macro().unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let _operation = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            graph.capture_roots = true;
            graph.direct_reader_roots = true;
            graph.declaration(global, Some(info), 0).unwrap();
            graph.finish_recipes().unwrap();
            let owner = info.completed_values.as_ref().unwrap();
            let metadata = graph.completed_reader(owner).unwrap();
            let selected = metadata.pairs.chunks_exact(2).enumerate().find(|(index, pair)| {
                // Name selects the authored input, never its origin. Origin is
                // the existing native normalization enum, independently checked.
                matches!(&pair[0].kind, suss_reader::forms::Kind::Keyword(name)
                    if name.namespace.is_none() && name.name == "tag")
                    && metadata.origins[*index] == if born_literal {
                        hir::MetadataKeyOrigin::ReaderShorthandTag
                    } else {
                        hir::MetadataKeyOrigin::SourceSyntax
                    }
            }).map(|(index, _)| index)
                .expect("both actual normalized provenance cases must exist");
            let captured = graph.capture_completed_reader_key(
                global, owner, info, metadata.key_positions[selected],
            ).unwrap();
            let key = graph.source_roots.index(graph.session, captured).unwrap().unwrap();
            let mut native = graph.source_roots.roots[key].native;
            let mut provenance_checked = false;
            for _ in 0..graph.recipes.len() {
                match graph.recipes[native].as_ref().unwrap() {
                    Recipe::Alias(child) => native = *child,
                    Recipe::Identifier { keyword: true, literal: Some(_), .. } if born_literal => {
                        provenance_checked = true;
                        break;
                    }
                    Recipe::Form(form) if !born_literal => {
                        assert!(matches!(form.kind, suss_reader::forms::Kind::Keyword(_)));
                        provenance_checked = true;
                        break;
                    }
                    _ => panic!("normalized origin must retain its actual native constructor path"),
                }
            }
            assert!(provenance_checked, "native provenance walk must terminate");
            let alias = graph.source_roots.roots[key].alias;
            let scalar = graph.recipe(Recipe::Scalar(crate::portable::hir::Literal::Number(17.0))).unwrap();
            let request_map = graph.recipe(Recipe::ReaderMap(vec![(alias, scalar)])).unwrap();
            let root = graph.reader_request_export(request_map).unwrap();
            // This separate native Request has ALL of its children. It does
            // not filter a full declaration's children or forge a source owner.
            assert!(graph.admit_reader_controller(root).unwrap().is_some());
            let plan = graph.admit_first_use_data_controller(root).unwrap().unwrap();
            assert_eq!(plan.modes[key], ChildMode::CheckedDirectKey);
            assert!(plan.layout[key].is_empty());
            let parent = graph.source_roots.index(graph.session, root).unwrap().unwrap();
            assert_eq!(plan.layout[parent], vec![key]);
            assert!(plan.expected[key].is_some());
            let recording = graph.bridge.recording(graph.session).unwrap();
            let value = graph.materialize_using(alias, &recording).unwrap();
            let mut fields = graph.session.data_fields(&value).unwrap();
            assert_eq!(fields.len(), 4, "actual Keyword schema");
            let initial = fields.remove(3);
            let initial_form = graph.bridge.read(graph.session, &initial, 0..1).unwrap();
            if born_literal {
                assert!(matches!(initial_form.kind, suss_reader::forms::Kind::Number(_)),
                    "compiler-literal memo must actually start numeric");
            } else {
                assert!(matches!(initial_form.kind, suss_reader::forms::Kind::Nil),
                    "the reader-key positive must actually start nil");
            }
            let poison = graph.session.eval("(fn [key] (set! (.-_hash key) 991))").unwrap();
            graph.session.invoke(&poison, &[&value]).unwrap();
            graph.session.collect().unwrap();
            let replacement = graph.session.data_fields(&value).unwrap().remove(3);
            assert!(!graph.session.values_identical(&initial, &replacement).unwrap(),
                "the generic write must actually replace the original memo");
            let finished = FreshSourceProof::finish(
                graph.session, graph.bridge, plan.expected[key].as_ref().unwrap().clone(),
                value, recording.into_records().unwrap(), vec![], false,
            );
            if born_literal {
                let error = match finished {
                    Err(error) => error,
                    Ok(_) => panic!("born-numeric literals cannot gain originally-nil DATA eligibility"),
                };
                assert!(matches!(error, crate::portable_session::SessionError::Host(ref error)
                    if error.to_string() == "Fresh coherent source failed non-memo certification"));
                assert!(RootCache::default().entries.is_empty());
                continue;
            }
            let proof = finished.unwrap();
            assert!(proof.certificate().is_none(), "numeric DATA cannot authorize a forged memo");
            assert!(proof.checked(graph.session, graph.bridge, plan.expected[key].as_ref().unwrap()).unwrap());
            let mut states = Vec::new();
            states.resize_with(plan.layout.len(), || None);
            states[key] = Some(proof);
            let error = match graph.construct_transient_reader_parent(
                graph.source_roots.roots[parent].native, parent, &plan, &states,
            ) {
                Err(error) => error,
                Ok(_) => panic!("health-only KEY must not enter strict direct exposure"),
            };
            assert!(error.to_string().contains("strict checked source certificate"), "{error}");
            let cache = RootCache::default();
            assert_eq!(cache.entries.len(), 0);
        }
    }
}

#[cfg(test)]
#[path = "reader_controller/source_order_tests.rs"]
mod source_order_tests;
