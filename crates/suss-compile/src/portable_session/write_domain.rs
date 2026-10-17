//! Factory-grounded payload snapshots for protected memo writes.
//! Captures occur before the fresh factory result can reach a guest callback.
use super::{
    Session, SessionError, SessionValue,
    canonical_methods::{CanonicalMethods, InspectionContext},
    canonical_seeds::CanonicalSeed,
    construction_records::Fact,
    memo_bridge::{Receipt, Watch},
    prototype_origin::Authority,
    registry_lock,
    storage_snapshot::StorageSnapshot,
};
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, AtomicUsize},
};
use wasmtime::{AnyRef, AsContextMut, RootScope, Rooted, StoreContextMut, Val};
type Result<T> = std::result::Result<T, SessionError>;
struct Node {
    value: SessionValue,
    snapshot: Option<StorageSnapshot>,
    seed: Option<Arc<CanonicalSeed>>,
    edges: Vec<SessionValue>,
    watch: Option<Weak<Watch>>,
}
struct ChildDomain {
    value: SessionValue,
    domain: Arc<WriteDomain>,
}
struct State {
    valid: bool,
    nodes: Vec<Node>,
    children: Vec<ChildDomain>,
    sealed: bool,
}
pub(crate) struct WriteDomain {
    state: Mutex<State>,
    handles: Arc<AtomicUsize>,
}
impl WriteDomain {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                valid: true,
                nodes: Vec::new(),
                children: Vec::new(),
                sealed: false,
            }),
            handles: Arc::new(AtomicUsize::new(0)),
        })
    }
    pub(crate) fn seal(&self, session: &mut Session) -> Result<()> {
        session.charge_host_work(1)?;
        registry_lock(&self.state).sealed=true;
        Ok(())
    }
    pub(crate) fn attach_child(
        &self, session: &mut Session,
        child: &crate::portable_macro_graph::source_roots::CheckedChild,
    )->Result<()> {
        session.check(child.value())?;
        session.charge_host_work(3)?;
        let domain=child.domain();
        if std::ptr::eq(self,domain.as_ref()) {return Err(wasmtime::Error::msg("Self child domain").into());}
        // The child link set is immutable before linking. Together with the
        // unsealed-parent requirement this prevents cycles by construction.
        if !registry_lock(&domain.state).sealed {return Err(wasmtime::Error::msg("Unsealed child domain").into());}
        let mut state=registry_lock(&self.state);
        if state.sealed {return Err(wasmtime::Error::msg("Sealed parent domain").into());}
        if state.children.len()==state.children.capacity() {
            let count=state.children.len().checked_mul(2).unwrap_or(usize::MAX).max(4);
            let bytes=count.checked_add(state.children.len()).and_then(|n|n.checked_mul(std::mem::size_of::<ChildDomain>()))
                .ok_or_else(||wasmtime::Error::msg("Child domain capacity overflow"))?;
            session.charge_host_work(bytes as u64)?;
            let extra=count-state.children.len();
            state.children.try_reserve_exact(extra).map_err(|_|wasmtime::Error::msg("Child domain allocation failed"))?;
        }
        session.charge_host_work(2)?;
        state.children.push(ChildDomain {value:child.value().clone(),domain});
        Ok(())
    }
    pub(super) fn invalidate(&self) {
        registry_lock(&self.state).valid = false;
    }
    pub(crate) fn record(
        self: &Arc<Self>,
        session: &mut Session,
        value: &SessionValue,
        fact: &Fact,
    ) -> Result<()> {
        let fuel_trace = std::env::var_os("SUSS_SOURCE_MATERIALIZE_FUEL_TRACE").is_some();
        let fact_kind = if fuel_trace { match fact {
            Fact::Scalar(_) => "Scalar", Fact::Array(_) => "Array",
            Fact::Constructor(_) => "Constructor", Fact::CanonicalSeed(_) => "Seed",
            Fact::ExternalSeed(_) => "ExternalSeed",
        }} else { "" };
        macro_rules! record_observed {
            ($phase:literal, $operation:expr, $summary:expr) => {{
                if fuel_trace {
                    eprintln!("source-domain-record domain={:p} fact={fact_kind} phase={} event=before remaining={:?}",
                        self.as_ref(), $phase, session.operation_fuel_remaining().ok());
                }
                let result = $operation;
                if fuel_trace {
                    eprintln!("source-domain-record domain={:p} fact={fact_kind} phase={} event=after result={:?} remaining={:?}",
                        self.as_ref(), $phase, result.as_ref().map($summary).map_err(|error| error.to_string()),
                        session.operation_fuel_remaining().ok());
                }
                result
            }};
        }
        macro_rules! record_skipped {
            ($phase:literal, $reason:literal) => {
                if fuel_trace {
                    eprintln!("source-domain-record domain={:p} fact={fact_kind} phase={} event=skipped reason={} remaining={:?}",
                        self.as_ref(), $phase, $reason, session.operation_fuel_remaining().ok());
                }
            };
        }
        let watch = match fact {
            Fact::Constructor(f) => f.memo_watch(),
            _ => None,
        };
        if let Some(watch) = &watch {
            watch.bind_source_domain(self);
        }
        let own = match fact {
            Fact::Scalar(f) => {
                let valid = record_observed!("direct-fact", f.matches(session, value), |valid: &bool| *valid)?;
                record_skipped!("prototype", "not-constructor");
                valid
            }
            Fact::Array(f) => {
                let valid = record_observed!("direct-fact", f.matches(session, value), |valid: &bool| *valid)?;
                record_skipped!("prototype", "not-constructor");
                valid
            }
            Fact::Constructor(f) => {
                let direct = record_observed!("direct-fact", f.matches(session, value), |valid: &bool| *valid)?;
                if direct {
                    record_observed!("prototype", f.matches_prototype_origin(session), |valid: &bool| *valid)?
                } else {
                    record_skipped!("prototype", "direct-fact-false");
                    false
                }
            }
            Fact::CanonicalSeed(f) => {
                let valid = record_observed!("direct-fact", f.matches(session, value), |valid: &bool| *valid)?;
                record_skipped!("prototype", "not-constructor");
                valid
            }
            Fact::ExternalSeed(_) => {
                record_skipped!("direct-fact", "external-unverified");
                record_skipped!("prototype", "not-constructor");
                false
            }
        };
        if !own {
            self.invalidate();
        }
        let (edges, cuts, seed) = record_observed!("edges-cuts", (|| -> Result<_> {
        let upper = match fact {
            Fact::Constructor(f) => f.inputs().size_hint().1.unwrap_or(0),
            Fact::Array(f) => f.inputs().size_hint().1.unwrap_or(0),
            _ => 0,
        };
        session.charge_host_work(
            (upper as u64 + 4).saturating_mul((3 * std::mem::size_of::<SessionValue>()) as u64)
                + std::mem::size_of::<Node>() as u64,
        )?;
        let mut edges = Vec::new();
        edges.try_reserve_exact(upper).map_err(|_| {
            SessionError::Host(wasmtime::Error::msg("Memo domain edges allocation failed"))
        })?;
        match fact {
            Fact::Constructor(f) => edges.extend(f.inputs().cloned()),
            Fact::Array(f) => edges.extend(f.inputs().cloned()),
            _ => {}
        }
        let cuts = match fact {
            Fact::Constructor(f) => f.write_cuts(),
            Fact::Array(f) => f.write_cuts(),
            _ => Vec::new(),
        };
        let seed = match fact {
            Fact::CanonicalSeed(seed) => Some(seed.clone()),
            _ => None,
        };
        Ok((edges, cuts, seed))
        })(), |_| ())?;
        let snapshot = if own && seed.is_none() {
            Some(record_observed!("snapshot-capture",
                session.capture_storage_cut_accounted(value, &cuts, self.handles.clone()), |_| ())?)
        } else {
            if !own { record_skipped!("snapshot-capture", "invalid-fact"); }
            else { record_skipped!("snapshot-capture", "seed-has-existing-witness"); }
            None
        };
        let mut state = registry_lock(&self.state);
        record_observed!("node-growth", (|| -> Result<()> {
        if state.sealed { return Err(wasmtime::Error::msg("Source domain is sealed").into()); }
        if state.nodes.len() == state.nodes.capacity() {
            let capacity = state
                .nodes
                .capacity()
                .checked_mul(2)
                .unwrap_or(usize::MAX)
                .max(4);
            let bytes = capacity
                .checked_add(state.nodes.len())
                .and_then(|n| n.checked_mul(std::mem::size_of::<Node>()))
                .ok_or_else(|| {
                    SessionError::Host(wasmtime::Error::msg("Memo domain capacity overflow"))
                })?;
            session.charge_host_work(bytes as u64 + 1)?;
            let extra = capacity - state.nodes.len();
            state.nodes.try_reserve_exact(extra).map_err(|_| {
                SessionError::Host(wasmtime::Error::msg("Memo domain allocation failed"))
            })?;
        } else {
            record_skipped!("node-growth-allocation", "capacity-available");
        }
        Ok(())
        })(), |_| ())?;
        record_observed!("node-publication", (|| -> Result<()> {
        state.nodes.push(Node {
            value: value.clone(),
            snapshot,
            seed,
            edges,
            watch: watch.as_ref().map(Arc::downgrade),
        });
        Ok(())
        })(), |_| ())
    }
    pub(super) fn matches(
        &self,
        mut store: StoreContextMut<'_, ()>,
        runtime: wasmtime::Instance,
        identity: u64,
        irq: &AtomicBool,
        methods: &CanonicalMethods,
        prototype: &Authority,
        staged: Option<(&Watch, &Receipt)>,
        target: &Watch,
        receivers: &[Val],
    ) -> Result<bool> {
        // Clone only typed immutable domain links under each brief lock. All
        // authority adapters and payload inspections run after those locks end.
        Session::consume_host_work(store.as_context_mut(),1,irq)?;
        if registry_lock(&self.state).children.is_empty() {
            return self.matches_local(store,runtime,identity,irq,methods,prototype,staged,target,receivers,true);
        }
        let mut pending: Vec<Arc<WriteDomain>>=Vec::new();
        let mut domains: Vec<Arc<WriteDomain>>=Vec::new();
        let mut seen: Vec<usize>=Vec::new();
        reserve_context(store.as_context_mut(),irq,&mut seen,1)?;
        seen.push(self as *const Self as usize);
        self.queue_children(store.as_context_mut(),irq,&mut pending)?;
        while let Some(current)=pending.pop() {
            Session::consume_host_work(store.as_context_mut(),3,irq)?;
            let pointer=Arc::as_ptr(&current) as usize;
            let mut duplicate=false;
            for old in &seen {Session::consume_host_work(store.as_context_mut(),1,irq)?;if *old==pointer {duplicate=true;break;}}
            if duplicate {continue;}
            if !registry_lock(&current.state).sealed {return Err(wasmtime::Error::msg("Unsealed certified domain dependency").into());}
            reserve_context(store.as_context_mut(),irq,&mut seen,1)?;
            seen.push(pointer);
            current.queue_children(store.as_context_mut(),irq,&mut pending)?;
            reserve_context(store.as_context_mut(),irq,&mut domains,1)?;
            domains.push(current);
        }
        let mut valid=self.matches_local(store.as_context_mut(),runtime,identity,irq,methods,prototype,staged,target,receivers,true)?;
        for domain in domains {
            Session::consume_host_work(store.as_context_mut(),1,irq)?;
            // A parent's staged receipt can never excuse a child's storage slot.
            valid &= domain.matches_local(store.as_context_mut(),runtime,identity,irq,methods,prototype,None,target,receivers,false)?;
        }
        Ok(valid)
    }
    fn queue_children(&self, mut store:StoreContextMut<'_,()>,irq:&AtomicBool,domains:&mut Vec<Arc<WriteDomain>>)->Result<()> {
        let state=registry_lock(&self.state);
        reserve_context(store.as_context_mut(),irq,domains,state.children.len())?;
        for child in &state.children {
            Session::consume_host_work(store.as_context_mut(),1,irq)?;
            domains.push(child.domain.clone());
        }
        Ok(())
    }
    fn matches_local(
        &self,
        mut store: StoreContextMut<'_, ()>,
        runtime: wasmtime::Instance,
        identity: u64,
        irq: &AtomicBool,
        methods: &CanonicalMethods,
        prototype: &Authority,
        staged: Option<(&Watch, &Receipt)>,
        target: &Watch,
        receivers: &[Val],
        require_target: bool,
    ) -> Result<bool> {
        let fuel_trace = std::env::var_os("SUSS_SOURCE_MATERIALIZE_FUEL_TRACE").is_some();
        macro_rules! domain_observed {
            ($label:literal, $operation:expr) => {{
                if fuel_trace {
                    eprintln!("source-domain domain={:p} staged={} phase={} event=before remaining={:?}",
                        self, staged.is_some(), $label, store.get_fuel().ok());
                }
                let result = $operation;
                if fuel_trace {
                    eprintln!("source-domain domain={:p} staged={} phase={} event=after result={:?} remaining={:?}",
                        self, staged.is_some(), $label,
                        result.as_ref().map_err(|error| error.to_string()), store.get_fuel().ok());
                }
                result
            }};
        }
        // Authority adapters may allocate. Run them before locking the domain;
        // all subsequent operations are native reads, without guest execution.
        let watches = {
            let state = registry_lock(&self.state);
            Session::consume_host_work(
                store.as_context_mut(),
                (state.nodes.len() as u64).saturating_mul(std::mem::size_of::<Arc<Watch>>() as u64)
                    + 1,
                irq,
            )?;
            let mut watches = Vec::new();
            watches.try_reserve_exact(state.nodes.len()).map_err(|_| {
                SessionError::Host(wasmtime::Error::msg("Memo domain method allocation failed"))
            })?;
            for node in &state.nodes {
                Session::consume_host_work(store.as_context_mut(), 1, irq)?;
                if let Some(watch) = node.watch.as_ref().and_then(Weak::upgrade) {
                    watches.push(watch);
                }
            }
            watches
        };
        // The target and every layout anchor must belong to this exact domain.
        // Actual dynamic IHash ancestry grants only method-layout inspection;
        // all payload/edge/schema/prototype and child-receipt checks stay below.
        let mut member = false;
        for watch in &watches {
            Session::consume_host_work(store.as_context_mut(), 1, irq)?;
            if std::ptr::eq(watch.as_ref(), target) { member = true; break; }
        }
        if require_target && !member { return Ok(false); }
        let bytes = receivers.len().checked_mul(std::mem::size_of::<Val>())
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| wasmtime::Error::msg("Memo layout context overflow"))?;
        Session::consume_host_work(store.as_context_mut(), bytes, irq)?;
        Session::consume_host_work(store.as_context_mut(), 1, irq)?;
        let mut layouts = Vec::new();
        layouts.try_reserve_exact(receivers.len())
            .map_err(|_| wasmtime::Error::msg("Memo layout context allocation failed"))?;
        for receiver in receivers {
            Session::consume_host_work(store.as_context_mut(), 1, irq)?;
            for watch in &watches {
                Session::consume_host_work(store.as_context_mut(), 1, irq)?;
                if let Some(descriptor) = watch.anchor(store.as_context_mut(), receiver, self, irq)? {
                    layouts.push(descriptor);
                    break;
                }
            }
        }
        let canonical = domain_observed!("canonical-loop",
            super::memo_bridge::inspect_domain_methods(store.as_context_mut(), runtime,
                methods, irq, &layouts, &watches).map_err(SessionError::from))?;
        let classes = domain_observed!("source-classes", methods.source_classes_in_context(InspectionContext {
            store: store.as_context_mut(),
            runtime,
            identity,
            interrupt: irq,
        }))?;
        let parent = domain_observed!("prototype", prototype.parent_matches_in_context(InspectionContext {
            store: store.as_context_mut(),
            runtime,
            identity,
            interrupt: irq,
        }))?;
        let state = registry_lock(&self.state);
        let mut edge_counts = if fuel_trace { Some([0_u64; 8]) } else { None };
        macro_rules! count_edge {
            ($index:expr) => {
                if let Some(counts) = edge_counts.as_mut() {
                    counts[$index] = counts[$index].saturating_add(1);
                }
            };
        }
        let result = domain_observed!("storage-and-edges", (|| -> Result<bool> {
        let mut valid = state.valid && classes && parent && canonical;
        let mut scope = RootScope::new(store.as_context_mut());
        for (ordinal, node) in state.nodes.iter().enumerate() {
            macro_rules! node_observed {
                ($phase:literal, $operation:expr) => {{
                    if fuel_trace {
                        // Presence flags only: no extra Weak upgrade or guest
                        // reference inspection is performed for diagnostics.
                        eprintln!("source-domain-node domain={:p} staged={} ordinal={ordinal} watch={} seed={} snapshot={} phase={} event=before remaining={:?}",
                            self, staged.is_some(), node.watch.is_some(), node.seed.is_some(), node.snapshot.is_some(),
                            $phase, scope.as_context_mut().get_fuel().ok());
                    }
                    let result = $operation;
                    if fuel_trace {
                        eprintln!("source-domain-node domain={:p} staged={} ordinal={ordinal} watch={} seed={} snapshot={} phase={} event=after result={:?} remaining={:?}",
                            self, staged.is_some(), node.watch.is_some(), node.seed.is_some(), node.snapshot.is_some(),
                            $phase, result.as_ref().map_err(|error| error.to_string()), scope.as_context_mut().get_fuel().ok());
                    }
                    result
                }};
            }
            Session::consume_host_work(scope.as_context_mut(), 1, irq)?;
            let root = Val::AnyRef(Some(node.value.value.to_rooted(&mut scope)));
            let storage_valid = node_observed!("snapshot", (|| -> Result<bool> {
            if let Some(seed) = &node.seed {
                Ok(seed.write_storage().matches_in_context(
                    scope.as_context_mut(),
                    root,
                    identity,
                    irq,
                )?)
            } else if let Some(snapshot) = &node.snapshot {
                let watch = node.watch.as_ref().and_then(Weak::upgrade);
                Ok(snapshot.matches_in_context_with_slots(
                    scope.as_context_mut(),
                    root,
                    identity,
                    irq,
                    |mut store, array, index, actual| {
                        let Some(watch) = &watch else {
                            return Ok(false);
                        };
                        if !watch.is_slot(store.as_context_mut(), &array, index, irq)? {
                            return Ok(false);
                        }
                        if let Some((current, receipt)) = staged {
                            if std::ptr::eq(current, watch.as_ref()) {
                                return watch
                                    .matches_receipt(store, actual, receipt, irq)
                                    .map_err(Into::into);
                            }
                        }
                        watch
                            .source_authenticated(store, actual, irq)
                            .map_err(Into::into)
                    },
                )?)
            } else {
                Ok(false)
            }
            })())?;
            valid &= storage_valid;
            node_observed!("edge-membership", (|| -> Result<()> {
            for edge in &node.edges {
                count_edge!(0);
                Session::consume_host_work(scope.as_context_mut(), 1, irq)?;
                let edge = edge.value.to_rooted(&mut scope);
                let mut member = edge.as_i31(&scope)?.is_some();
                if member {
                    count_edge!(1);
                    continue;
                }
                for other in &state.nodes {
                    Session::consume_host_work(scope.as_context_mut(), 1, irq)?;
                    let other_root = other.value.value.to_rooted(&mut scope);
                    count_edge!(2);
                    member |= Rooted::<AnyRef>::ref_eq(&scope, &edge, &other_root)?;
                    if member {
                        count_edge!(3);
                        break;
                    }
                }
                if !member {
                    for child in &state.children {
                        Session::consume_host_work(scope.as_context_mut(),1,irq)?;
                        let root=child.value.value.to_rooted(&mut scope);
                        count_edge!(4);
                        if Rooted::<AnyRef>::ref_eq(&scope,&edge,&root)? {count_edge!(5);member=true;break;}
                    }
                }
                // Exact recorded/child roots are sufficient membership proof.
                // Only an edge absent from ALL those roots needs a seed-interior
                // search. Complete seed snapshots still run for every seed in
                // the outer node loop, even after an earlier false result.
                if !member {
                    for other in &state.nodes {
                        Session::consume_host_work(scope.as_context_mut(), 1, irq)?;
                        count_edge!(6);
                        if let Some(seed) = &other.seed {
                            count_edge!(7);
                            member |= seed.write_storage().contains_reference_in_context(
                                scope.as_context_mut(),
                                Val::AnyRef(Some(edge)),
                                identity,
                                irq,
                            )?;
                            if member { break; }
                        }
                    }
                }
                valid &= member;
            }
            Ok(())
            })())?;
        }
        Ok(valid)
        })());
        if let Some(counts) = edge_counts {
            eprintln!("source-domain-edges domain={:p} staged={} ok={} edges={} immediate={} node_attempts={} node_hits={} child_attempts={} child_hits={} seed_candidates={} interior_calls={} saturated={}",
                self, staged.is_some(), result.is_ok(), counts[0], counts[1], counts[2], counts[3], counts[4], counts[5], counts[6], counts[7], counts.contains(&u64::MAX));
        }
        result
    }
}

fn reserve_context<T>(mut store:StoreContextMut<'_,()>,irq:&AtomicBool,values:&mut Vec<T>,extra:usize)->Result<()> {
    Session::consume_host_work(store.as_context_mut(),1,irq)?;
    if extra<=values.capacity()-values.len() {return Ok(());}
    let needed=values.len().checked_add(extra).ok_or_else(||wasmtime::Error::msg("Domain traversal capacity overflow"))?;
    let count=values.capacity().checked_mul(2).unwrap_or(needed).max(needed).max(4);
    let bytes=count.checked_add(values.len()).and_then(|n|n.checked_mul(std::mem::size_of::<T>()))
        .ok_or_else(||wasmtime::Error::msg("Domain traversal byte overflow"))?;
    Session::consume_host_work(store.as_context_mut(),bytes as u64,irq)?;
    values.try_reserve_exact(count-values.len()).map_err(|_|wasmtime::Error::msg("Domain traversal allocation failed"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{portable::hir::Literal, portable_macro_data::FormBridge};
    // Real canonical seed and watched factory inputs. Keep records alive: their
    // constructor facts own the Watches; the domain intentionally stores Weak.
    fn membership_fixture(session: &mut Session) -> (
        super::super::construction_records::ConstructionRecords,
        Arc<WriteDomain>, Arc<Watch>, SessionValue, SessionValue,
    ) {
        let bridge = FormBridge::new(session).unwrap().recording(session).unwrap();
        let seed = bridge.map_values(session, &[]).unwrap();
        let _keyword = bridge.identifier(session, None, "domain-membership", true, None).unwrap();
        let interior = session.data_fields(&seed).unwrap()[2].clone();
        let records = bridge.into_records().unwrap();
        let domain = records.write_domain();
        let watch = registry_lock(&domain.state).nodes.iter()
            .find_map(|node| node.watch.as_ref().and_then(Weak::upgrade)).unwrap();
        (records, domain, watch, seed, interior)
    }
    fn domain_valid(session: &mut Session, domain: &WriteDomain, target: &Watch) -> Result<bool> {
        let _request = session.operation_budget_scope()?;
        let methods = session.canonical_methods.clone().unwrap();
        let prototype = session.prototype_origin.clone().unwrap();
        let identity = session.identity;
        let runtime = session.runtime;
        let irq = session.host_work_interrupt();
        domain.matches(session.store.as_context_mut(), runtime, identity, &irq,
            &methods, &prototype, None, target, &[])
    }
    fn record_array_edge(session: &mut Session, domain: &Arc<WriteDomain>, edge: &SessionValue) -> SessionValue {
        let (value, fact) = session.data_array_with_inputs(&[edge]).unwrap();
        domain.record(session, &value, &Fact::Array(fact)).unwrap();
        value
    }
    #[test]
    fn direct_membership_and_seed_interior_preserve_exact_closed_domain_and_seed_checks() {
        let mut session = Session::new_macro().unwrap();
        let (_records, domain, target, seed, interior) = membership_fixture(&mut session);
        // This edge is only inside the independently captured canonical seed.
        // It has no separately recorded factory root in this domain.
        {
            let state = registry_lock(&domain.state);
            session.inspect(&interior, |mut store, value| {
                for node in &state.nodes {
                    let root = node.value.value.to_rooted(&mut store);
                    assert!(!Rooted::<AnyRef>::ref_eq(&store, value.unwrap_anyref().unwrap(), &root)?);
                }
                Ok(())
            }).unwrap();
        }
        let _seed_edge = record_array_edge(&mut session, &domain, &interior);
        // Record a genuine input AFTER its user, so membership must find a later
        // direct root without consulting the preceding seed's interior graph.
        let (number, number_fact) = session.data_scalar_with_inputs(&Literal::Number(17.0)).unwrap();
        let _direct_edge = record_array_edge(&mut session, &domain, &number);
        domain.record(&mut session, &number, &Fact::Scalar(number_fact)).unwrap();
        session.collect().unwrap();
        assert!(domain_valid(&mut session, &domain, &target).unwrap());
        let poison = session.eval("(fn [seed] (set! (.-__hash seed) 991))").unwrap();
        drop(session.invoke(&poison, &[&seed]).unwrap());
        assert!(!domain_valid(&mut session, &domain, &target).unwrap(),
            "a direct-root hit cannot bypass the full canonical seed snapshot");
    }
    #[test]
    fn membership_rejects_equal_content_missing_and_foreign_edges_and_recovers_from_fuel_irq() {
        let mut session = Session::new_macro().unwrap();
        let (_records, domain, target, _seed, _interior) = membership_fixture(&mut session);
        let (number, fact) = session.data_scalar_with_inputs(&Literal::Number(17.0)).unwrap();
        domain.record(&mut session, &number, &Fact::Scalar(fact)).unwrap();
        let _edge = record_array_edge(&mut session, &domain, &number);
        session.collect().unwrap();
        assert!(domain_valid(&mut session, &domain, &target).unwrap());
        session.with_operation_budget(|session| {
            let remaining = session.operation_fuel_remaining()?;
            session.charge_host_work(remaining)?;
            assert!(matches!(domain_valid(session, &domain, &target), Err(SessionError::Trap(_))));
            Ok(())
        }).unwrap();
        assert!(domain_valid(&mut session, &domain, &target).unwrap());
        session.with_operation_budget(|session| {
            session.interrupt_handle().interrupt();
            match domain_valid(session, &domain, &target) {
                Err(error) => assert!(error.is_interrupt()),
                Ok(_) => panic!("pending interrupt must not be hidden by direct membership"),
            }
            Ok(())
        }).unwrap();
        assert!(domain_valid(&mut session, &domain, &target).unwrap());
        let mut foreign = Session::new_macro().unwrap();
        assert!(matches!(foreign.data_array_with_inputs(&[&number]), Err(SessionError::ForeignValue)));
        assert!(domain_valid(&mut session, &domain, &target).unwrap());
        let equal = session.data_scalar(&Literal::Number(17.0)).unwrap();
        session.inspect(&equal, |mut store, value| {
            let original = number.value.to_rooted(&mut store);
            assert!(!Rooted::<AnyRef>::ref_eq(&store, value.unwrap_anyref().unwrap(), &original)?);
            Ok(())
        }).unwrap();
        let _unknown = record_array_edge(&mut session, &domain, &equal);
        assert!(!domain_valid(&mut session, &domain, &target).unwrap(),
            "equal payload cannot authorize a reference absent from roots and seed interiors");
    }
    fn method(session: &mut Session, descriptor: &SessionValue) -> SessionValue {
        let name = format!(
            "key-{}",
            "suss.core/IHash/-hash/1"
                .as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let handles = session.handles.clone();
        let key = super::super::canonical_methods::binding(
            session,
            "suss.internal.protocol-keys",
            &name,
            &handles,
        )
        .unwrap();
        let mut scope = RootScope::new(&mut session.store);
        let function = session
            .runtime
            .get_func(&mut scope, "protocol-method-get")
            .unwrap();
        let descriptor = Val::AnyRef(Some(descriptor.value.to_rooted(&mut scope)));
        let key = Val::AnyRef(Some(key.value.to_rooted(&mut scope)));
        super::super::call(
            &mut scope,
            session.runtime,
            function,
            &[descriptor, key],
            session.identity,
            &session.handles,
        )
        .unwrap()
    }
    #[test]
    fn source_write_domain_checks_approved_child_method_during_parent_write() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session)
            .unwrap()
            .recording(&mut session)
            .unwrap();
        let number = bridge.scalar(&mut session, &Literal::Number(17.0)).unwrap();
        let child = bridge.vector_values(&mut session, &[number]).unwrap();
        let key = bridge
            .identifier(&mut session, None, "items", true, None)
            .unwrap();
        let root = bridge
            .map_values(&mut session, &[(key, child.clone())])
            .unwrap();
        let records = bridge.into_records().unwrap();
        let hash = session.eval("(fn [x] (hash x))").unwrap();
        drop(session.invoke(&hash, &[&child]).unwrap());
        assert!(
            records.matches_direct_facts(&mut session).unwrap(),
            "first child receipt is valid"
        );
        let class = session.eval("suss.core/PersistentVector").unwrap();
        let descriptor = session.data_descriptor(&class, true).unwrap();
        let original = method(&mut session, &descriptor);
        session.eval("(def child-method-calls 0)").unwrap();
        session.enter_namespace("suss.core").unwrap();
        session.eval("(extend-type PersistentVector IHash (-hash [_] (do (set! user/child-method-calls (+ user/child-method-calls 1)) 999)))").unwrap();
        session.enter_namespace("user").unwrap();
        let changed = method(&mut session, &descriptor);
        drop(session.invoke(&hash, &[&root]).unwrap());
        let calls = session.eval("child-method-calls").unwrap();
        session
            .inspect(&calls, |mut store, value| {
                let count = value
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&store)?
                    .unwrap()
                    .field(&mut store, 0)?
                    .unwrap_f64();
                assert!(
                    count > 0.0,
                    "changed child IHash must actually run during parent hashing"
                );
                Ok(())
            })
            .unwrap();
        session
            .inspect(&descriptor, |mut store, value| {
                let descriptor = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let table = descriptor
                    .field(&mut store, 2)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                let changed = changed.value.to_rooted(&mut store);
                let mut count = 0;
                for index in (1..table.len(&store)?).step_by(2) {
                    if let Some(value) = table.get(&mut store, index)?.unwrap_anyref() {
                        if Rooted::<AnyRef>::ref_eq(&store, value, &changed)? {
                            let old = original.value.to_rooted(&mut store);
                            table.set(&mut store, index, Val::AnyRef(Some(old)))?;
                            count += 1;
                        }
                    }
                }
                assert_eq!(count, 1);
                Ok(())
            })
            .unwrap();
        session.collect().unwrap();
        let fields = session.data_fields(&root).unwrap();
        session
            .inspect(&fields[3], |store, value| {
                assert!(
                    value.unwrap_anyref().unwrap().as_struct(&store)?.is_some(),
                    "actual parent memo was stored"
                );
                Ok(())
            })
            .unwrap();
        assert!(
            !records.matches_direct_facts(&mut session).unwrap(),
            "child method restoration cannot retroactively authorize the parent memo"
        );
    }
    #[test]
    fn source_write_domain_checks_method_replaced_before_child_construction() {
        let mut session = Session::new_macro().unwrap();
        let class = session.eval("suss.core/PersistentVector").unwrap();
        let descriptor = session.data_descriptor(&class, true).unwrap();
        let original = method(&mut session, &descriptor);
        session.eval("(def child-method-calls 0)").unwrap();
        session.enter_namespace("suss.core").unwrap();
        session.eval("(extend-type PersistentVector IHash (-hash [_] (do (set! user/child-method-calls (+ user/child-method-calls 1)) 999)))").unwrap();
        session.enter_namespace("user").unwrap();
        let bridge = FormBridge::new(&mut session)
            .unwrap()
            .recording(&mut session)
            .unwrap();
        let number = bridge.scalar(&mut session, &Literal::Number(17.0)).unwrap();
        let child = bridge.vector_values(&mut session, &[number]).unwrap();
        let key = bridge
            .identifier(&mut session, None, "items", true, None)
            .unwrap();
        let root = bridge
            .map_values(&mut session, &[(key, child.clone())])
            .unwrap();
        let records = bridge.into_records().unwrap();
        let hash = session.eval("(fn [x] (hash x))").unwrap();
        let changed = method(&mut session, &descriptor);
        drop(session.invoke(&hash, &[&root]).unwrap());
        let calls = session.eval("child-method-calls").unwrap();
        session
            .inspect(&calls, |mut store, value| {
                let count = value
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&store)?
                    .unwrap()
                    .field(&mut store, 0)?
                    .unwrap_f64();
                assert!(
                    count > 0.0,
                    "changed child IHash must actually run during parent hashing"
                );
                Ok(())
            })
            .unwrap();
        session
            .inspect(&descriptor, |mut store, value| {
                let descriptor = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let table = descriptor
                    .field(&mut store, 2)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                let changed = changed.value.to_rooted(&mut store);
                let mut count = 0;
                for index in (1..table.len(&store)?).step_by(2) {
                    if let Some(value) = table.get(&mut store, index)?.unwrap_anyref() {
                        if Rooted::<AnyRef>::ref_eq(&store, value, &changed)? {
                            let old = original.value.to_rooted(&mut store);
                            table.set(&mut store, index, Val::AnyRef(Some(old)))?;
                            count += 1;
                        }
                    }
                }
                assert_eq!(count, 1);
                Ok(())
            })
            .unwrap();
        session.collect().unwrap();
        let fields = session.data_fields(&root).unwrap();
        session
            .inspect(&fields[3], |store, value| {
                assert!(
                    value.unwrap_anyref().unwrap().as_struct(&store)?.is_some(),
                    "actual parent memo was stored"
                );
                Ok(())
            })
            .unwrap();
        assert!(
            !records.matches_direct_facts(&mut session).unwrap(),
            "child method restoration cannot retroactively authorize the parent memo"
        );
    }
}
