//! Source-owned metadata reuse; private parent values are fixed only after callbacks.
use super::{
    AnalysisGraph, Result, Value, construction::RecordedMetadata,
    metadata_expectations::MetadataExpectation,
};
use crate::{
    portable::{
        metered_map::{Context, TryOrderedMap},
        resolve::Global,
    },
    portable_session::{Session, SessionError, SessionValue},
};
use std::{cmp::Ordering, rc::Rc, sync::Arc};

type Key = (Global, usize);
struct Meter<'a>(&'a mut Session);
impl Context<Key> for Meter<'_> {
    type Error = SessionError;
    fn charge(&mut self, n: u64) -> Result<()> {
        self.0.charge_host_work(n)
    }
    fn compare(&mut self, a: &Key, b: &Key) -> Result<Ordering> {
        self.charge(
            (a.0.namespace().len() + a.0.name().len() + b.0.namespace().len() + b.0.name().len())
                as u64
                + 1,
        )?;
        Ok(a.0.cmp(&b.0).then(a.1.cmp(&b.1)))
    }
}
#[derive(Default)]
pub(crate) struct DeclarationValues {
    // The source controller is staged separately from legacy metadata reuse;
    // it must wholly own metadata exports when activated, never interleave them.
    pub(super) roots: super::source_roots::RootCache,
    metadata: TryOrderedMap<Key, Option<Rc<RecordedMetadata>>>,
}
pub(super) fn reserve<T>(session: &mut Session, values: &mut Vec<T>, extra: usize) -> Result<()> {
    session.charge_host_work(1)?;
    if extra > values.capacity() - values.len() {
        let needed = values
            .len()
            .checked_add(extra)
            .ok_or_else(|| invalid("Metadata reuse allocation overflow"))?;
        let cap = values
            .capacity()
            .checked_mul(2)
            .unwrap_or(needed)
            .max(needed);
        let bytes = cap
            .checked_add(values.len())
            .and_then(|n| n.checked_mul(std::mem::size_of::<T>()))
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| invalid("Metadata reuse allocation overflow"))?;
        session.charge_host_work(bytes)?;
        values
            .try_reserve_exact(cap - values.len())
            .map_err(|_| invalid("Metadata reuse allocation failed"))?;
    }
    Ok(())
}
fn invalid(s: &'static str) -> SessionError {
    wasmtime::Error::msg(s).into()
}
fn fresh(graph: &mut AnalysisGraph<'_>, index: usize) -> Result<Option<Rc<RecordedMetadata>>> {
    let record = graph.record_metadata(index)?;
    if record.is_some() {
        graph.session.charge_host_work(
            (std::mem::size_of::<RecordedMetadata>() + 2 * std::mem::size_of::<usize>()) as u64,
        )?;
    }
    Ok(record.map(Rc::new))
}
struct Pending {
    alias: Value,
    index: usize,
    key: Key,
    record: Option<Rc<RecordedMetadata>>,
    current: Option<Rc<MetadataExpectation>>,
    value: SessionValue,
    placeholder: Option<SessionValue>,
    old: bool,
}
pub(super) struct Build<'a> {
    cache: &'a mut DeclarationValues,
    sources: Vec<Option<usize>>,
    pending: Vec<Pending>,
    cuts: Vec<SessionValue>,
    slots: crate::portable_session::private_fixups::slot_log::SharedSlotLog,
    logged_parent_route: bool,
}
impl<'a> Build<'a> {
    fn new(graph: &mut AnalysisGraph<'_>, cache: &'a mut DeclarationValues) -> Result<Self> {
        cache
            .metadata
            .retain(&mut Meter(graph.session), |_, record| {
                record
                    .as_ref()
                    .is_some_and(|record| Arc::strong_count(record.source().revision()) > 1)
            })?;
        let mut sources = Vec::new();
        reserve(graph.session, &mut sources, graph.recipes.len())?;
        graph.session.charge_host_work(graph.recipes.len() as u64)?;
        sources.resize(graph.recipes.len(), None);
        for (index, source) in graph.metadata_sources.iter().enumerate() {
            graph.session.charge_host_work(1)?;
            sources[source.recipe] = Some(index);
        }
        let slots=crate::portable_session::private_fixups::slot_log::PrivateSlotLog::new(graph.session)?;
        Ok(Self {
            slots,
            logged_parent_route: false,
            cache,
            sources,
            pending: Vec::new(),
            cuts: Vec::new(),
        })
    }
    // Only this constructor establishes the shared-log parent route. Manual
    // controller callers retain the walker unless they explicitly use this bridge.
    fn logged_parent_bridge(&mut self, graph: &mut AnalysisGraph<'_>) -> Result<crate::portable_macro_data::FormBridge> {
        let bridge = graph.bridge.private_parent_bridge_with_slots(graph.session, &self.slots)?;
        graph.session.charge_host_work(1)?;
        self.logged_parent_route = true;
        Ok(bridge)
    }
    // An immediate untainted input of a tainted private factory parent cannot
    // contain this build's fresh, unexposed placeholder. Keep it opaque at fixup.
    pub(super) fn cut(&mut self, session: &mut Session, value: &SessionValue) -> Result<()> {
        reserve(session, &mut self.cuts, 1)?;
        self.cuts.push(value.clone());
        Ok(())
    }
    pub(super) fn note_slot_route(&mut self,session:&mut Session,covered:bool)->Result<()> {
        session.charge_host_work(1)?;
        if !covered {self.slots.borrow_mut().unsupported();}Ok(())
    }
    pub(super) fn last_is_private(&self, id: Value) -> bool {
        self.pending
            .last()
            .is_some_and(|p| p.alias == id && p.placeholder.is_some())
    }
    pub(super) fn intercepts(&self, id: Value) -> bool {
        self.sources[id].is_some()
    }
    pub(super) fn produce(
        &mut self,
        graph: &mut AnalysisGraph<'_>,
        alias: Value,
    ) -> Result<SessionValue> {
        let index = self.sources[alias].ok_or_else(|| invalid("Missing metadata reuse source"))?;
        let source = &graph.metadata_sources[index];
        graph.session.charge_host_work(
            (source.global.namespace().len() + source.global.name().len()) as u64 + 1,
        )?;
        let key = (
            source.global.clone(),
            Arc::as_ptr(&source.revision) as usize,
        );
        let current = graph.metadata_expectation(index)?;
        let candidate = self
            .cache
            .metadata
            .get(&key, &mut Meter(graph.session))?
            .and_then(Option::as_ref)
            .cloned();
        let mut old = false;
        let record = if let (Some(record), Some(expected)) = (candidate, current.as_ref()) {
            if record.matches_recorded_source(graph.session, graph.bridge)?
                && expected.matches_logical_source(graph.session, graph.bridge, &record.value)?
            {
                old = true;
                Some(record)
            } else {
                fresh(graph, index)?
            }
        } else {
            fresh(graph, index)?
        };
        let value = match &record {
            Some(record) => record.value.clone(),
            None => graph.materialize_using(alias, graph.bridge)?,
        };
        let placeholder = if old {
            Some(self.slots.borrow_mut().token(graph.session,index)?)
        } else {
            None
        };
        let result = placeholder.as_ref().unwrap_or(&value).clone();
        reserve(graph.session, &mut self.pending, 1)?;
        self.pending.push(Pending {
            alias,
            index,
            key,
            record,
            current,
            value,
            placeholder,
            old,
        });
        Ok(result)
    }
    fn valid(graph: &mut AnalysisGraph<'_>, pending: &Pending) -> Result<bool> {
        Self::valid_using(graph, pending, None)
    }
    fn valid_using(
        graph: &mut AnalysisGraph<'_>, pending: &Pending,
        audit: Option<&mut crate::portable_session::certification_audit::CertificationAudit>,
    ) -> Result<bool> {
        let Some(record) = &pending.record else {
            return Ok(false);
        };
        let valid = match audit {
            Some(audit) => record.matches_recorded_source_audited(graph.session, graph.bridge, audit)?,
            None => record.matches_recorded_source(graph.session, graph.bridge)?,
        };
        let current = match &pending.current {
            Some(expected) => {
                graph.session.charge_host_work(1)?;
                // Shared immutable expectation identity proves this is the same
                // logical predicate already checked by matches_recorded_source.
                // It never substitutes for live guest/storage/authority checks.
                record.has_source(expected)
                    || expected.matches_logical_source(graph.session, graph.bridge, &record.value)?
            }
            None => false,
        };
        Ok(valid && current)
    }
    fn finish(
        mut self,
        graph: &mut AnalysisGraph<'_>,
        root: Value,
        mut value: SessionValue,
    ) -> Result<SessionValue> {
        let tracing = std::env::var_os("SUSS_METADATA_FUEL_TRACE").is_some();
        let trace = |session: &Session, stage: &str| -> Result<()> {
            if tracing {
                eprintln!("metadata-finish {stage}: {}", session.operation_fuel_remaining()?);
            }
            Ok(())
        };
        trace(graph.session, "start")?;
        // Only Old -> Fresh transitions can invoke more construction callbacks.
        // Each owner transitions at most once; a poisoned fresh value is never
        // retained and is not repaired behind a guest's back.
        loop {
            let mut rebuilt = false;
            // valid_using observes finite data and fixed authority adapters only.
            // Share queries until the next potentially guest-calling construction.
            let mut audit = None;
            for pending in &mut self.pending {
                graph.session.charge_host_work(1)?;
                if !pending.old { continue; }
                if audit.is_none() {
                    audit = Some(crate::portable_session::certification_audit::CertificationAudit::new(graph.session)?);
                }
                if !Self::valid_using(graph, pending, audit.as_mut())? {
                    // Drop BEFORE fresh(), including its None/fallback route.
                    // Neither successful nor failed construction may retain queries.
                    drop(audit.take());
                    pending.record = fresh(graph, pending.index)?;
                    pending.value = match &pending.record {
                        Some(record) => record.value.clone(),
                        None => graph.materialize_using(pending.alias, graph.bridge)?,
                    };
                    pending.old = false;
                    rebuilt = true;
                }
            }
            if !rebuilt {
                break;
            }
        }
        trace(graph.session, "rechecked")?;
        let mut certified = Vec::new();
        reserve(graph.session, &mut certified, self.pending.len())?;
        // No construction callbacks can occur in this certification interval.
        let mut audit = crate::portable_session::certification_audit::CertificationAudit::new(graph.session)?;
        for pending in &self.pending {
            graph.session.charge_host_work(1)?;
            // The final complete pass rebuilt nothing: every Old record was
            // validated after the last construction callback. Native-only fresh
            // certification cannot invalidate that invocation-local result.
            certified.push(if pending.old { true } else { Self::valid_using(graph, pending, Some(&mut audit))? });
        }
        drop(audit);
        trace(graph.session, "certified")?;
        let mut pairs = Vec::new();
        reserve(graph.session, &mut pairs, self.pending.len())?;
        for pending in &self.pending {
            graph.session.charge_host_work(1)?;
            if let Some(placeholder) = &pending.placeholder {
                if pending.alias == root {
                    // An intercepted root has no traversed dependencies: exactly
                    // one pending/token exists, and no slot application follows.
                    graph.session.charge_host_work(1)?;
                    if self.pending.len()!=1 {return Err(invalid("Private root has unrelated pending bindings"));}
                    value = pending.value.clone();
                } else {
                    pairs.push((placeholder, &pending.value));
                }
            }
        }
        trace(graph.session, "pairs")?;
        if !pairs.is_empty() {
            if self.logged_parent_route && self.slots.borrow().complete() {
                self.slots.borrow().apply(graph.session,&value,&pairs)?;
            } else {
                graph.session.fixup_private_placeholders_with_cuts(&value, &pairs, &self.cuts)?;
            }
        }
        trace(graph.session, "fixed")?;
        // No guest callback occurs between certification, fixups and publication.
        // Every entry is an independently complete certificate; budget failure
        // cannot publish a partially initialized record or compilation binding.
        for (pending, valid) in self.pending.into_iter().zip(certified) {
            self.cache.metadata.insert(
                pending.key,
                if valid { pending.record } else { None },
                &mut Meter(graph.session),
            )?;
        }
        trace(graph.session, "published")?;
        Ok(value)
    }
}
fn trace_graph(graph: &AnalysisGraph<'_>, stage: &str) -> Result<()> {
    if std::env::var_os("SUSS_METADATA_FUEL_TRACE").is_some() {
        eprintln!("metadata-graph {stage} recipes={} sources={}: {}", graph.recipes.len(), graph.metadata_sources.len(), graph.session.operation_fuel_remaining()?);
    }
    Ok(())
}
impl AnalysisGraph<'_> {
    pub(super) fn materialize_reusing(
        &mut self,
        root: Value,
        cache: &mut DeclarationValues,
    ) -> Result<SessionValue> {
        trace_graph(self, "start")?;
        self.session.charge_host_work(1)?;
        if self.direct_reader_roots {
            match self.materialize_reader_controller(root, &mut cache.roots)? {
                Some(value) => return Ok(value),
                None => {
                    // No construction callbacks occurred in the declined path.
                    // One ordinary whole-build context preserves reader aliases.
                    let value = self.materialize_using(root, self.bridge)?;
                    cache.roots.retire_obsolete(self.session)?;
                    return Ok(value);
                }
            }
        }
        self.finish_recipes()?;
        trace_graph(self, "recipes")?;
        let placed = !self.metadata_sources.is_empty() && self.private_metadata_placement(root)?;
        trace_graph(self, "placement")?;
        if !placed {
            return self.materialize_using(root, self.bridge);
        }
        let mut build = Build::new(self, cache)?;
        let bridge = build.logged_parent_bridge(self)?;
        trace_graph(self, "bridge")?;
        trace_graph(self, "build")?;
        let value = self.materialize_controlled(root, &bridge, Some(&mut build))?;
        trace_graph(self, "materialized")?;
        build.finish(self, root, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        portable::{
            hir::{Literal, SourceNamespace},
            resolve::Phase,
        },
        portable_macro_data::FormBridge,
    };
    use wasmtime::{AnyRef, Rooted, Val};
    // Uses that module's existing super::*, SourceNamespace, Phase, and FormBridge.
    // No production hook or substituted validation is required.
    #[test]
    fn fresh_certification_shares_only_the_exact_graph_expectation() {
        let mut session = Session::new_macro().unwrap();
        session.eval("(def ^{:doc \"same expectation\"} expectation-target 17)").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let snapshot = session.compilation_snapshot();
        let namespace = SourceNamespace::capture(&snapshot.environment, Phase::Macro);
        let (global, revision) = namespace.declarations.iter()
            .find(|(g, _)| g.name() == "expectation-target").unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.declaration(global, Some(revision), 0).unwrap();
        let alias = graph.metadata_sources[0].recipe;
        let first = graph.metadata_expectation(0).unwrap().unwrap();
        assert!(Rc::ptr_eq(&first, &graph.metadata_expectation(0).unwrap().unwrap()));
        // Same owner and recipe, independently captured: identity must differ.
        super::super::metadata_expectations::capture(graph.session,
            &mut graph.metadata_sources, global, revision, alias).unwrap();
        let separate = graph.metadata_expectation(1).unwrap().unwrap();
        assert!(!Rc::ptr_eq(&first, &separate));
        let mut cache = DeclarationValues::default();
        let mut build = Build::new(&mut graph, &mut cache).unwrap();
        build.produce(&mut graph, alias).unwrap();
        let pending = &mut build.pending[0];
        let record = pending.record.as_ref().unwrap().clone();
        assert!(record.has_source(pending.current.as_ref().unwrap()));
        let operation = graph.session.operation_budget_scope().unwrap();
        let before = graph.session.operation_fuel_remaining().unwrap();
        assert!(record.matches_recorded_source(graph.session, graph.bridge).unwrap());
        let full_cost = before - graph.session.operation_fuel_remaining().unwrap();
        drop(operation);
        graph.session.set_operation_fuel(full_cost + 64);
        let operation = graph.session.operation_budget_scope().unwrap();
        assert!(Build::valid(&mut graph, pending).unwrap(),
            "fresh certification must fit one complete validation plus pointer check");
        drop(operation);
        // Distinct expectations still require the second logical comparison.
        pending.current = Some(first);
        let operation = graph.session.operation_budget_scope().unwrap();
        let error = Build::valid(&mut graph, pending).unwrap_err();
        assert!(matches!(error, SessionError::Trap(ref error)
            if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{error:?}");
        drop(operation);
        graph.session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        assert!(Build::valid(&mut graph, pending).unwrap());
        // Same owner/revision, genuinely different native recipe: no shortcut.
        let other = graph.recipe(super::super::Recipe::Scalar(Literal::Number(998.0))).unwrap();
        super::super::metadata_expectations::capture(graph.session,
            &mut graph.metadata_sources, global, revision, other).unwrap();
        pending.current = graph.metadata_expectation(2).unwrap();
        assert!(!record.has_source(pending.current.as_ref().unwrap()));
        assert!(!Build::valid(&mut graph, pending).unwrap());
    }
    #[test]
    fn reuse_later_rebuild_rechecks_earlier_old_and_does_not_retain_poisoned_fresh() {
        rebuild_callback_routes(false);
    }
    #[test]
    fn reuse_logged_parent_rebuild_rechecks_earlier_old_and_poisoned_fresh() {
        rebuild_callback_routes(true);
    }
    #[test]
    fn reuse_incomplete_logged_parent_falls_back_to_walker_with_callback_outputs_preserved() {
        rebuild_callback_routes_with_incomplete(true, true);
    }
    fn rebuild_callback_routes(logged: bool) {
        rebuild_callback_routes_with_incomplete(logged, false);
    }
    fn rebuild_callback_routes_with_incomplete(logged: bool, incomplete: bool) {
        use super::super::Recipe;

        for poison_prototype in [false, true] {
            let mut session = Session::new_macro().unwrap();
            session.eval(if poison_prototype {
                "(def rebuild-poison-prototype true)"
            } else {
                "(def rebuild-poison-prototype false)"
            }).unwrap();
            session.eval(r#"
              (def ^{:doc "A" :items #{301 302 303 304 305 306 307 308 309}} rebuild-a 17)
              (def ^{:items #{101 102 103 104 105 106 107 108 109}} rebuild-b 18)
              (def ^{:doc "C"} rebuild-c 19)
              (def ^{:items #{201 202 203 204 205 206 207 208 209}} rebuild-d 20)
              (def rebuild-track-a false)
              (def rebuild-track-d false)
              (def rebuild-armed false)
              (def rebuild-events [])
              (def rebuild-old-a nil)
              (def rebuild-fresh-c nil)
              (def rebuild-original-hash suss.core/hash)
            "#).unwrap();
            // Install before bridge capture: B's real ReaderSet construction invokes
            // this captured hash function. Initial priming/production runs unarmed.
            session.enter_namespace("suss.core").unwrap();
            session.eval(r#"
              (def hash
                (fn [key]
                  (do
                    (if (and user/rebuild-armed (= key 101))
                      (do
                        (set! user/rebuild-armed false)
                        (set! user/rebuild-events (conj user/rebuild-events :b-rebuild))
                        (if user/rebuild-poison-prototype
                          (do
                            (set! user/rebuild-track-a true)
                            (set! user/rebuild-track-d true)
                            (aset (.-prototype PersistentArrayMap) "rebuildAuthorityPoison" 99))
                          (do
                            (set! (.-__hash user/rebuild-old-a) 991)
                            (set! (.-__hash user/rebuild-fresh-c) 992))))
                      nil)
                    (if (and user/rebuild-track-a (= key 301))
                      (do
                        (set! user/rebuild-track-a false)
                        (set! user/rebuild-events (conj user/rebuild-events :a-rebuild)))
                      nil)
                    (if (and user/rebuild-track-d (= key 201))
                      (do
                        (set! user/rebuild-track-d false)
                        (set! user/rebuild-events (conj user/rebuild-events :d-rebuild)))
                      nil)
                    (user/rebuild-original-hash key))))
            "#).unwrap();
            session.enter_namespace("user").unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let snapshot = session.compilation_snapshot();
            let namespace = SourceNamespace::capture(&snapshot.environment, Phase::Macro);
            // Own real immutable definition revisions for the complete test lifetime.
            let owners = ["rebuild-a", "rebuild-b", "rebuild-c", "rebuild-d"].map(|name| {
                let (global, revision) = namespace.declarations.iter()
                    .find(|(global, _)| global.name() == name).unwrap();
                (global.clone(), revision.clone())
            });
            let mut cache = DeclarationValues::default();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            for (global, revision) in &owners {
                graph.declaration(global, Some(revision), 0).unwrap();
            }
            graph.finish_recipes().unwrap();
            let indices = owners.each_ref().map(|(global, revision)| {
                graph.metadata_sources.iter().position(|source| {
                    &source.global == global && Arc::ptr_eq(&source.revision, revision)
                }).unwrap()
            });
            let aliases = indices.map(|index| graph.metadata_sources[index].recipe);
            let root = graph.recipe(Recipe::ReaderVector(aliases.to_vec())).unwrap();
            let keys = owners.each_ref().map(|(global, revision)| {
                (global.clone(), Arc::as_ptr(revision) as usize)
            });

            // Prime A/B/D with real production recordings; C must start fresh.
            for index in [indices[0], indices[1], indices[3]] {
                let record = fresh(&mut graph, index).unwrap().unwrap();
                assert!(record.matches_recorded_source(graph.session, graph.bridge).unwrap());
                let source = &graph.metadata_sources[index];
                let key = (source.global.clone(), Arc::as_ptr(&source.revision) as usize);
                cache.metadata.insert(key, Some(record), &mut Meter(graph.session)).unwrap();
            }
            let mut build = Build::new(&mut graph, &mut cache).unwrap();
            let a_token = build.produce(&mut graph, aliases[0]).unwrap();
            let b_token = build.produce(&mut graph, aliases[1]).unwrap();
            let c_value = build.produce(&mut graph, aliases[2]).unwrap();
            let d_token = build.produce(&mut graph, aliases[3]).unwrap();
            assert_eq!(build.pending.len(), 4);
            assert!(build.pending[3].old);
            assert!(build.pending[0].old && build.pending[1].old);
            assert!(!build.pending[2].old);
            assert!(build.pending[0].placeholder.is_some());
            assert!(build.pending[1].placeholder.is_some());
            assert!(build.pending[2].placeholder.is_none());
            let old_a = build.pending[0].value.clone();
            let old_b = build.pending[1].value.clone();
            let fresh_c = build.pending[2].record.as_ref().unwrap().clone();
            // Genuine private parent, with the exact tokens produced above. Direct
            // controller access fixes pending order A/B/C/D without fabricating facts.
            let private = if logged {
                build.logged_parent_bridge(&mut graph).unwrap()
            } else {
                bridge.private_parent_bridge(graph.session).unwrap()
            };
            assert_eq!(build.logged_parent_route, logged);
            if incomplete {
                assert!(build.logged_parent_route);
                build.note_slot_route(graph.session, false).unwrap();
                assert!(!build.slots.borrow().complete());
            }
            let parent = private.vector_values(graph.session, &[a_token, b_token, c_value, d_token]).unwrap();

            let arm = graph.session.eval(r#"
              (fn [a b c]
                (do
                  (set! rebuild-old-a a)
                  (set! rebuild-fresh-c c)
                  (set! (.-__hash b) 990)
                  (set! rebuild-armed true)))
            "#).unwrap();
            drop(graph.session.invoke(&arm, &[&old_a, &old_b, &fresh_c.value]).unwrap());
            // B's logical source remains intact; only its unauthenticated memo forces
            // finish() to call real fresh(), whose set callback poisons A and fresh C.
            assert!(build.pending[1].current.as_ref().unwrap()
                .matches_logical_source(graph.session, graph.bridge, &old_b).unwrap());
            assert!(Build::valid(&mut graph, &build.pending[0]).unwrap());
            assert!(!Build::valid(&mut graph, &build.pending[1]).unwrap());
            assert!(Build::valid(&mut graph, &build.pending[2]).unwrap());
            let value = build.finish(&mut graph, root, parent).unwrap();

            if poison_prototype {
                // B poisons shared map prototype authority after A's audited check.
                // D must rebuild in THIS pass, before A rebuilds in the next pass.
                // Keeping the audit across B gives [:b-rebuild :a-rebuild :d-rebuild]
                // (or stale retention), so a next-pass-only reset cannot satisfy this.
                for key in &keys {
                    assert!(matches!(cache.metadata.get(key, &mut Meter(graph.session)).unwrap(), Some(None)),
                        "callback-poisoned prototype must prevent retaining any A/B/C/D certificate");
                }
                assert!(!fresh_c.matches_recorded_source(graph.session, graph.bridge).unwrap());
                let check = graph.session.eval(r#"
                  (fn [v old-a old-b fresh-c]
                    [(= rebuild-events [:b-rebuild :d-rebuild :a-rebuild])
                     (not rebuild-armed)
                     (not (identical? (nth v 0) old-a))
                     (not (identical? (nth v 1) old-b))
                     (identical? (nth v 2) fresh-c)
                     (nil? (.-__hash old-a))
                     (= (.-rebuildAuthorityPoison (.-prototype suss.core/PersistentArrayMap)) 99)])
                "#).unwrap();
                let result = graph.session.invoke(&check, &[&value, &old_a, &old_b, &fresh_c.value])
                    .unwrap_or_else(|error| panic!("{}", crate::portable_repl::error_display(graph.session, &error)));
                assert_eq!(crate::portable_repl::display(graph.session, &result).unwrap(),
                    "[true true true true true true true]");
                continue;
            }

            let a_record = cache.metadata.get(&keys[0], &mut Meter(graph.session))
                .unwrap().unwrap().as_ref().unwrap().clone();
            let b_record = cache.metadata.get(&keys[1], &mut Meter(graph.session))
                .unwrap().unwrap().as_ref().unwrap().clone();
            assert!(matches!(cache.metadata.get(&keys[2], &mut Meter(graph.session)).unwrap(), Some(None)),
                "poisoned fresh C must not be retained");
            let d_record = cache.metadata.get(&keys[3], &mut Meter(graph.session))
                .unwrap().unwrap().as_ref().unwrap().clone();
            assert!(d_record.matches_recorded_source(graph.session, graph.bridge).unwrap());
            assert!(a_record.matches_recorded_source(graph.session, graph.bridge).unwrap());
            assert!(b_record.matches_recorded_source(graph.session, graph.bridge).unwrap());
            assert!(!fresh_c.matches_recorded_source(graph.session, graph.bridge).unwrap());
            graph.session.collect().unwrap();
            let check = graph.session.eval(r#"
              (fn [v old-a old-b fresh-c cached-a cached-b]
                [(= rebuild-events [:b-rebuild])
                 (not rebuild-armed)
                 (= (.-__hash old-a) 991)
                 (= (.-__hash old-b) 990)
                 (= (.-__hash fresh-c) 992)
                 (not (identical? (nth v 0) old-a))
                 (not (identical? (nth v 1) old-b))
                 (identical? (nth v 2) fresh-c)
                 (identical? (nth v 0) cached-a)
                 (identical? (nth v 1) cached-b)
                 (nil? (.-__hash (nth v 0)))
                 (nil? (.-__hash (nth v 1)))
                 (= (:doc (nth v 0)) "A")
                 (= (:items (nth v 1)) #{101 102 103 104 105 106 107 108 109})
                 (= (:doc (nth v 2)) "C")])
            "#).unwrap();
            let result = graph.session.invoke(&check, &[
                &value, &old_a, &old_b, &fresh_c.value, &a_record.value, &b_record.value,
            ]).unwrap();
            assert_eq!(crate::portable_repl::display(graph.session, &result).unwrap(),
                "[true true true true true true true true true true true true true true true]");
        }
    }
    #[test]
    fn reuse_frontier_preserves_repeated_occurrences_cached_aliases_and_metadata_clones() {
        use super::super::Recipe;
        use crate::portable_macro_data::MetadataShape;
        let mut session = Session::new_macro().unwrap();
        session.eval("(def ^{:doc \"frontier\"} frontier-target 17)").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let snapshot = session.compilation_snapshot();
        let namespace = SourceNamespace::capture(&snapshot.environment, Phase::Macro);
        let (global, revision) = namespace.declarations.iter().find(|(g, _)| g.name()=="frontier-target").unwrap();
        let mut cache = DeclarationValues::default();
        let saved = {
            let mut graph = AnalysisGraph::with_declaration_values(&bridge, &mut session, &mut cache);
            graph.declaration(global, Some(revision), 0).unwrap();
            let root = graph.metadata_sources[0].recipe;
            graph.materialize(root).unwrap()
        };
        session.collect().unwrap();
        let value = {
            let mut graph = AnalysisGraph::with_declaration_values(&bridge, &mut session, &mut cache);
            graph.declaration(global, Some(revision), 0).unwrap();
            let source = graph.metadata_sources[0].recipe;
            let alias = graph.recipe(Recipe::Alias(source)).unwrap();
            let m = graph.recipe(Recipe::Identifier{namespace:None,name:"m".into(),keyword:true,literal:None}).unwrap();
            let branch_key = graph.recipe(Recipe::Identifier{namespace:None,name:"branch".into(),keyword:true,literal:None}).unwrap();
            let x = graph.recipe(Recipe::Identifier{namespace:None,name:"x".into(),keyword:true,literal:None}).unwrap();
            let scalar = graph.recipe(Recipe::Scalar(Literal::Number(17.0))).unwrap();
            let branch = graph.recipe(Recipe::ReaderMap(vec![(x,scalar)])).unwrap();
            let occurrence = graph.recipe(Recipe::ReaderMap(vec![(m,alias),(branch_key,branch)])).unwrap();
            let cloned = graph.recipe(Recipe::Metadata{shape:MetadataShape::Map{large:false},value:occurrence,metadata:alias,set_items:vec![]}).unwrap();
            let root = graph.recipe(Recipe::ReaderVector(vec![alias,cloned,cloned,occurrence])).unwrap();
            graph.materialize(root).unwrap()
        };
        session.collect().unwrap();
        let check = session.eval("(fn [v saved] (let [a (nth v 1) b (nth v 2) c (nth v 3)] [(identical? (nth v 0) saved) (identical? (:m a) saved) (identical? (:m b) saved) (identical? (:m c) saved) (identical? (meta a) saved) (identical? (meta b) saved) (not (identical? a b)) (not (identical? (:branch a) (:branch b))) (= (:x (:branch a)) 17) (= (:x (:branch b)) 17)]))").unwrap();
        let result = session.invoke(&check, &[&value,&saved]).unwrap();
        assert_eq!(crate::portable_repl::display(&mut session,&result).unwrap(),"[true true true true true true true true true true]");
    }
    #[test]
    fn reuse_cold_untainted_parent_preserves_special_constructor_callback() {
        let mut session = Session::new_macro().unwrap();
        session
            .eval("(def cold-route-calls 0) (def ^{:doc \"cold\"} cold-route-target 17)")
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let snapshot = session.compilation_snapshot();
        let namespace = SourceNamespace::capture(&snapshot.environment, Phase::Macro);
        let (global, revision) = namespace
            .declarations
            .iter()
            .find(|(g, _)| g.name() == "cold-route-target")
            .unwrap();
        let error = session.eval("js/Error").unwrap();
        let descriptor = session.data_descriptor(&error, true).unwrap();
        let replacement=session.eval("(fn [meta first rest count hash] (do (set! cold-route-calls (+ cold-route-calls 1)) first))").unwrap();
        session
            .inspect(&descriptor, |mut store, value| {
                let descriptor = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let cache = descriptor
                    .field(&mut store, 2)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                let original = error.rooted(&mut store);
                let mut changed = 0;
                for index in (1..cache.len(&store)?).step_by(2) {
                    if let Some(entry) = cache.get(&mut store, index)?.unwrap_anyref() {
                        if Rooted::<AnyRef>::ref_eq(&store, entry, &original)? {
                            let replacement = replacement.rooted(&mut store);
                            cache.set(&mut store, index, Val::AnyRef(Some(replacement)))?;
                            changed += 1;
                        }
                    }
                }
                assert_eq!(changed, 1);
                Ok(())
            })
            .unwrap();
        let list = session.eval("suss.core/List").unwrap();
        session
            .inspect(&list, |mut store, value| {
                let closure = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let owner = closure
                    .field(&mut store, 0)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&store)?
                    .unwrap();
                let fields = owner
                    .field(&mut store, 1)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                let descriptor = descriptor.rooted(&mut store);
                fields.set(&mut store, 0, Val::AnyRef(Some(descriptor)))
            })
            .unwrap();
        let mut values = DeclarationValues::default();
        let value = {
            let mut graph =
                AnalysisGraph::with_declaration_values(&bridge, &mut session, &mut values);
            graph.declaration(global, Some(revision), 0).unwrap();
            let scalar = graph
                .recipe(super::super::Recipe::Scalar(Literal::Number(17.0)))
                .unwrap();
            let root = graph
                .recipe(super::super::Recipe::List(vec![scalar]))
                .unwrap();
            assert!(!graph.metadata_sources.is_empty());
            graph.materialize(root).unwrap()
        };
        session
            .inspect(&value, |mut store, value| {
                assert_eq!(
                    value
                        .unwrap_anyref()
                        .unwrap()
                        .as_struct(&store)?
                        .unwrap()
                        .field(&mut store, 0)?
                        .unwrap_f64(),
                    17.0
                );
                Ok(())
            })
            .unwrap();
        let calls = session.eval("cold-route-calls").unwrap();
        assert_eq!(
            crate::portable_repl::display(&mut session, &calls).unwrap(),
            "1"
        );
        assert_eq!(values.metadata.len(), 0);
    }
}
