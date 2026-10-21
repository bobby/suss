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
mod factory_inputs;
pub(crate) use factory_inputs::FactoryOrigin;
// Temporary attribution of existing native predicates only. No guest reads.
#[cfg(test)]
fn memo_domain_trace(detail: std::fmt::Arguments<'_>) {
    if std::env::var_os("SUSS_MEMO_DOMAIN_TRACE").is_some() {
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr().lock(), "MEMO_DOMAIN test={:?} {detail}", std::thread::current().name());
    }
}
struct ComplexNode {
    value: SessionValue,
    snapshot: Option<StorageSnapshot>,
    seed: Option<Arc<CanonicalSeed>>,
    edges: Vec<SessionValue>,
    watch: Option<Weak<Watch>>,
}
// A scalar slot is admitted only after exact supplied-root direct validation.
// It owns the immutable factory witness; no domain/record/Watch backpointer.
// Each accessor's variant/read forwarding is prepaid by its caller (3 units).
enum Node {
    Scalar(Arc<super::ScalarInputs>),
    Complex(Box<ComplexNode>),
}
impl Node {
    fn value(&self) -> &SessionValue {
        match self { Self::Scalar(f) => f.receiver(), Self::Complex(n) => &n.value }
    }
    fn watch(&self) -> Option<&Weak<Watch>> {
        match self { Self::Scalar(_) => None, Self::Complex(n) => n.watch.as_ref() }
    }
    fn seed(&self) -> Option<&Arc<CanonicalSeed>> {
        match self { Self::Scalar(_) => None, Self::Complex(n) => n.seed.as_ref() }
    }
    fn snapshot(&self) -> Option<&StorageSnapshot> {
        match self { Self::Scalar(_) => None, Self::Complex(n) => n.snapshot.as_ref() }
    }
    fn edges(&self) -> &[SessionValue] {
        match self { Self::Scalar(_) => &[], Self::Complex(n) => n.edges.as_slice() }
    }
    // Initial original-storage eligibility ONLY. Current health and exact scope
    // still require the complete separate factory-component proof. Scalar
    // witnesses have only their exact root, no seed or outgoing reference edge.
    // The caller prepays 5 units for dispatch/option/eligibility controls.
    fn has_original_storage(&self) -> bool {
        match self {
            Self::Scalar(_) => true,
            Self::Complex(n) => n.snapshot.is_some() || n.seed.is_some(),
        }
    }
}
enum ChildDomain {
    Source { value: SessionValue, domain: Arc<WriteDomain> },
    // Offers are not membership. Only stage_factory_edges creates exact links.
    FactoryCandidate(FactoryOrigin),
    FactoryReference { value: SessionValue, origin: FactoryOrigin, owner: factory_inputs::FactoryPoint },
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
        let mut state = registry_lock(&self.state);
        session.charge_host_work((state.children.len() as u64).saturating_mul(3).saturating_add(2))?;
        // Candidate retirement/backing is prepaid during prepare_factory_inputs.
        // Used exact links remain; none owns a strong producer-domain Arc.
        state.children.retain(|child| !matches!(child, ChildDomain::FactoryCandidate(_)));
        state.sealed=true;
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
        session.charge_host_work(2)?; // new tagged representation initialization
        state.children.push(ChildDomain::Source {value:child.value().clone(),domain});
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
        let watch = match fact {
            Fact::Constructor(f) => f.memo_watch(),
            _ => None,
        };
        if let Some(watch) = &watch {
            watch.bind_source_domain(self);
        }
        let own = match fact {
            Fact::Scalar(f) => f.matches(session, value)?,
            Fact::Array(f) => f.matches(session, value)?,
            Fact::Constructor(f) => {
                f.matches(session, value)? && f.matches_prototype_origin(session)?
            }
            Fact::CanonicalSeed(f) => f.matches(session, value)?,
            Fact::ExternalSeed(_) => false,
        };
        if !own {
            self.invalidate();
        }
        session.charge_host_work(2)?; // scalar selection/branch before staging
        let node = if own && matches!(fact, Fact::Scalar(_)) {
            let Fact::Scalar(facts) = fact else { unreachable!() };
            session.charge_host_work((2 * std::mem::size_of::<Node>() + 6) as u64)?;
            Node::Scalar(Arc::clone(facts))
        } else {
        let upper = match fact {
            Fact::Constructor(f) => f.inputs().size_hint().1.unwrap_or(0),
            Fact::Array(f) => f.inputs().size_hint().1.unwrap_or(0),
            _ => 0,
        };
        session.charge_host_work(
            (upper as u64 + 4).saturating_mul((3 * std::mem::size_of::<SessionValue>()) as u64)
                + (2 * std::mem::size_of::<ComplexNode>() + std::mem::size_of::<Node>() + 4) as u64,
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
        let snapshot = if own && seed.is_none() {
            Some(session.capture_storage_cut_accounted(value, &cuts, self.handles.clone())?)
        } else {
            None
        };
        // Pay actual boxed payload allocation/move/retirement in the fee above.
        // Invalid scalar facts retain the ORIGINAL supplied value and no proof.
        Node::Complex(Box::new(ComplexNode {
            value: value.clone(), snapshot, seed, edges,
            watch: watch.as_ref().map(Arc::downgrade),
        }))
        };
        session.charge_host_work(3)?; // Node::edges variant/read forwarding
        let links = self.stage_factory_edges(session, node.edges())?;
        let mut state = registry_lock(&self.state);
        if state.sealed { return Err(wasmtime::Error::msg("Source domain is sealed").into()); }
        // Stage all capacity and link work before publishing either node or links.
        factory_inputs::reserve_factory(session, &mut state.children, links.len())?;
        let link_bytes = links.len().checked_mul(std::mem::size_of::<ChildDomain>())
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| SessionError::Host(wasmtime::Error::msg("Factory link publication overflow")))?;
        session.charge_host_work(link_bytes.checked_add(2).ok_or_else(||
            SessionError::Host(wasmtime::Error::msg("Factory link publication overflow")))?)?;
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
        }
        state.nodes.push(node);
        // Payload/root/weak-link publication and retirement were prepaid while staging.
        state.children.extend(links);
        Ok(())
    }
    pub(super) fn matches(
        self: &Arc<Self>, store: StoreContextMut<'_, ()>, runtime: wasmtime::Instance,
        identity: u64, irq: &AtomicBool, methods: &CanonicalMethods,
        prototype: &Authority, staged: Option<(&Watch, &Receipt)>,
        target: &Watch, receivers: &[Val],
    ) -> Result<bool> {
        self.matches_factory_closure(store, runtime, identity, irq, methods,
            prototype, staged, target, receivers)
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
        factory_layouts: &[Val],
    ) -> Result<bool> {
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
                Session::consume_host_work(store.as_context_mut(), 4, irq)?; // original loop + watch accessor
                if let Some(watch) = node.watch().and_then(Weak::upgrade) {
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
        #[cfg(test)]
        memo_domain_trace(format_args!("domain={self:p} target={target:p} stage=membership identity={identity} staged={} require_target={require_target} member={member}", staged.is_some()));
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
        // The caller derives these only from actual receiver Watch anchors in
        // this exact factory-ownership component, never a strict source child.
        factory_inputs::append_layouts(store.as_context_mut(), irq, &mut layouts, factory_layouts)?;
        // This callback-free pass ends before any class/prototype/storage proof
        // or write. Only repeated exact method inspections share a result.
        let canonical = super::memo_bridge::inspect_domain_methods(
            store.as_context_mut(), runtime, methods, irq, &layouts, &watches,
        )?;
        #[cfg(test)]
        memo_domain_trace(format_args!("domain={self:p} target={target:p} stage=method-pass watches={} valid={canonical}", watches.len()));
        let classes = methods.source_classes_in_context(InspectionContext {
            store: store.as_context_mut(),
            runtime,
            identity,
            interrupt: irq,
        })?;
        let parent = prototype.parent_matches_in_context(InspectionContext {
            store: store.as_context_mut(),
            runtime,
            identity,
            interrupt: irq,
        })?;
        let state = registry_lock(&self.state);
        let mut valid = state.valid && classes && parent && canonical;
        #[cfg(test)]
        memo_domain_trace(format_args!("domain={self:p} target={target:p} stage=header staged={} state_valid={} classes={classes} prototype_parent={parent} methods={canonical} nodes={} child_domains={}", staged.is_some(), state.valid, state.nodes.len(), state.children.len()));
        let mut scope = RootScope::new(store);
        for node in &state.nodes {
            Session::consume_host_work(scope.as_context_mut(), 1, irq)?;
            // Five bounded accessors plus scalar dispatch/owner controls;
            // prepaid before any root access, even on false/empty paths.
            Session::consume_host_work(scope.as_context_mut(), 17, irq)?;
            if let Node::Scalar(facts) = node { facts.check_owner(identity)?; }
            let root = Val::AnyRef(Some(node.value().value.to_rooted(&mut scope)));
            if let Node::Scalar(facts) = node {
                valid &= facts.matches_in_context(scope.as_context_mut(), root, identity, irq, false)?;
            } else if let Some(seed) = node.seed() {
                let seed_valid = seed.write_storage().matches_in_context(
                    scope.as_context_mut(),
                    root,
                    identity,
                    irq,
                )?;
                #[cfg(test)]
                memo_domain_trace(format_args!("domain={self:p} target={target:p} stage=seed node={node:p} valid={seed_valid}"));
                valid &= seed_valid;
            } else if let Some(snapshot) = node.snapshot() {
                let watch = node.watch().and_then(Weak::upgrade);
                let snapshot_valid = snapshot.matches_in_context_with_slots(
                    scope.as_context_mut(),
                    root,
                    identity,
                    irq,
                    |mut store, array, index, actual| {
                        let Some(watch) = &watch else {
                            #[cfg(test)]
                            memo_domain_trace(format_args!("domain={self:p} target={target:p} stage=memo-slot node={node:p} index={index} watch_present=false"));
                            return Ok(false);
                        };
                        let is_slot = watch.is_slot(store.as_context_mut(), &array, index, irq)?;
                        #[cfg(test)]
                        memo_domain_trace(format_args!("domain={self:p} target={target:p} stage=memo-slot node={node:p} watch={:p} index={index} exact_slot={is_slot}", watch.as_ref()));
                        if !is_slot {
                            return Ok(false);
                        }
                        if let Some((current, receipt)) = staged {
                            if std::ptr::eq(current, watch.as_ref()) {
                                let matches = watch.matches_receipt(store, actual, receipt, irq)?;
                                #[cfg(test)]
                                memo_domain_trace(format_args!("domain={self:p} target={target:p} stage=staged-memo node={node:p} watch={:p} index={index} matches={matches}", watch.as_ref()));
                                return Ok(matches);
                            }
                        }
                        let authenticated = watch.source_authenticated(store, actual, irq)?;
                        #[cfg(test)]
                        memo_domain_trace(format_args!("domain={self:p} target={target:p} stage=existing-memo node={node:p} watch={:p} index={index} authenticated={authenticated}", watch.as_ref()));
                        Ok(authenticated)
                    },
                )?;
                #[cfg(test)]
                memo_domain_trace(format_args!("domain={self:p} target={target:p} stage=snapshot node={node:p} valid={snapshot_valid}"));
                valid &= snapshot_valid;
            } else {
                #[cfg(test)]
                memo_domain_trace(format_args!("domain={self:p} target={target:p} stage=missing-proof node={node:p}"));
                valid = false;
            }
            for edge in node.edges() {
                Session::consume_host_work(scope.as_context_mut(), 1, irq)?;
                let edge = edge.value.to_rooted(&mut scope);
                let mut member = edge.as_i31(&scope)?.is_some();
                if member {
                    continue;
                }
                for other in &state.nodes {
                    Session::consume_host_work(scope.as_context_mut(), 1, irq)?;
                    Session::consume_host_work(scope.as_context_mut(), 8, irq)?; // value/seed accessors + scalar owner
                    if let Node::Scalar(facts) = other { facts.check_owner(identity)?; }
                    let other_root = other.value().value.to_rooted(&mut scope);
                    member |= Rooted::<AnyRef>::ref_eq(&scope, &edge, &other_root)?;
                    if member {
                        break;
                    }
                    if let Some(seed) = other.seed() {
                        member |= seed.write_storage().contains_reference_in_context(
                            scope.as_context_mut(),
                            Val::AnyRef(Some(edge)),
                            identity,
                            irq,
                        )?;
                        if member {
                            break;
                        }
                    }
                }
                if !member {
                    for child in &state.children {
                        Session::consume_host_work(scope.as_context_mut(),1,irq)?;
                        Session::consume_host_work(scope.as_context_mut(), 2, irq)?;
                        let value = match child {
                            ChildDomain::Source { value, .. } | ChildDomain::FactoryReference { value, .. } => value,
                            ChildDomain::FactoryCandidate(_) => continue,
                        };
                        let root=value.value.to_rooted(&mut scope);
                        if Rooted::<AnyRef>::ref_eq(&scope,&edge,&root)? {member=true;break;}
                    }
                }
                #[cfg(test)]
                memo_domain_trace(format_args!("domain={self:p} target={target:p} stage=original-input-edge node={node:p} member={member}"));
                valid &= member;
            }
        }
        #[cfg(test)]
        memo_domain_trace(format_args!("domain={self:p} target={target:p} stage=local-result valid={valid}"));
        Ok(valid)
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
            .find_map(|node| node.watch().and_then(Weak::upgrade)).unwrap();
        (records, domain, watch, seed, interior)
    }
    #[test]
    fn scalar_domain_mixed_growth_and_record_drop_orders_keep_only_scalar_owners() {
        eprintln!("typed-scalar-layout Node={} ComplexNode={} ScalarInputs={} Fact={}",
            std::mem::size_of::<Node>(), std::mem::size_of::<ComplexNode>(),
            std::mem::size_of::<super::super::ScalarInputs>(), std::mem::size_of::<Fact>());
        use crate::portable::resolve::Phase;
        for phase in [Phase::Runtime, Phase::Macro] {
            for records_first in [false, true] {
                let mut session = match phase { Phase::Runtime => Session::new_repl().unwrap(), Phase::Macro => Session::new_macro().unwrap() };
                let bridge = FormBridge::new(&mut session).unwrap().recording(&mut session).unwrap();
                // Each vector contributes genuine complex constructor/array
                // facts alongside a distinct immutable scalar factory owner.
                for index in 0..20 {
                    let value = bridge.scalar(&mut session, &Literal::Number(index as f64)).unwrap();
                    drop(bridge.vector_values(&mut session, &[value]).unwrap());
                }
                let records = bridge.into_records().unwrap();
                let domain = records.write_domain();
                let mut scalar_owners = Vec::new();
                let mut watch_owners = Vec::new();
                let (scalars, complex) = {
                    let state = registry_lock(&domain.state);
                    let mut scalars = 0; let mut complex = 0;
                    for node in &state.nodes {
                        match node {
                            Node::Scalar(facts) => {
                                scalars += 1;
                                scalar_owners.push(Arc::downgrade(facts));
                            }
                            Node::Complex(node) => {
                                complex += 1;
                                if let Some(watch) = &node.watch { watch_owners.push(watch.clone()); }
                            }
                        }
                    }
                    // Distinct mixed owners span multiple geometric growth
                    // boundaries; do not assume allocator capacity is exact.
                    assert!(state.nodes.len() > 32);
                    (scalars, complex)
                };
                // At least the twenty explicit inputs, plus the bridge's own
                // count/shift/nil scalar facts; do not depend on that encoding.
                assert!(scalars >= 20);
                assert!(complex >= 20);
                if phase == Phase::Macro { assert!(!watch_owners.is_empty()); }
                assert!(watch_owners.iter().all(|owner| owner.strong_count() > 0));
                session.collect().unwrap();
                assert!(records.matches_direct_facts(&mut session).unwrap());
                if records_first {
                    drop(records);
                    // Only native scalar facts are retained strongly by the
                    // domain. Constructor Watches must die with their facts.
                    assert!(watch_owners.iter().all(|owner| owner.strong_count() == 0));
                    session.collect().unwrap();
                    for weak in &scalar_owners {
                        let facts = weak.upgrade().expect("domain lost its scalar owner");
                        assert_eq!(Arc::strong_count(&facts), 2); // domain + this local
                        assert!(facts.matches(&mut session, facts.receiver()).unwrap());
                    }
                    drop(domain);
                } else {
                    // Records themselves own the domain. Releasing our domain
                    // reference first must keep both fact sets alive until the
                    // records' final owner is dropped; no artificial cycle.
                    drop(domain);
                    assert!(scalar_owners.iter().all(|owner| owner.strong_count() > 0));
                    assert!(watch_owners.iter().all(|owner| owner.strong_count() > 0));
                    assert!(records.matches_direct_facts(&mut session).unwrap());
                    drop(records);
                }
                session.collect().unwrap();
                assert!(scalar_owners.iter().all(|owner| owner.upgrade().is_none()));
                assert!(watch_owners.iter().all(|owner| owner.upgrade().is_none()));
            }
        }
    }
    #[test]
    fn scalar_domain_shares_only_valid_fact_and_preserves_wrong_supplied_root_and_late_error() {
        for phase in [crate::portable::resolve::Phase::Runtime, crate::portable::resolve::Phase::Macro] {
            let mut session = match phase { crate::portable::resolve::Phase::Runtime => Session::new_repl().unwrap(), _ => Session::new_macro().unwrap() };
            let macro_fixture = match phase {
                crate::portable::resolve::Phase::Macro => Some(membership_fixture(&mut session)),
                crate::portable::resolve::Phase::Runtime => {
                    // Runtime factories supply direct facts but do not enroll
                    // protected memo Watches. Establish that actual capability
                    // boundary instead of unwrapping/manufacturing a Watch.
                    let bridge = FormBridge::new(&mut session).unwrap().recording(&mut session).unwrap();
                    drop(bridge.identifier(&mut session, None, "runtime-no-memo-watch", true, None).unwrap());
                    let records = bridge.into_records().unwrap();
                    let runtime_domain = records.write_domain();
                    assert!(!registry_lock(&runtime_domain.state).nodes.is_empty());
                    assert!(registry_lock(&runtime_domain.state).nodes.iter().all(|node| node.watch().is_none()));
                    assert!(records.matches_direct_facts(&mut session).unwrap());
                    None
                }
            };
            // Exercise a minimal no-edge domain so an intentionally foreign
            // later test entry is never rooted by an earlier membership scan.
            let domain = WriteDomain::new();
            fn check(session: &mut Session, domain: &WriteDomain, target: &Watch) -> Result<bool> {
                let _request = session.operation_budget_scope()?;
                let methods = session.canonical_methods.clone().unwrap();
                let prototype = session.prototype_origin.clone().unwrap();
                let identity = session.identity; let runtime = session.runtime;
                let irq = session.host_work_interrupt();
                domain.matches_local(session.store.as_context_mut(), runtime, identity, &irq,
                    &methods, &prototype, None, target, &[], false, &[])
            }
            let (value, facts) = session.data_scalar_with_inputs(&Literal::Number(17.0)).unwrap();
            domain.record(&mut session, &value, &Fact::Scalar(facts.clone())).unwrap();
            {
                let state = registry_lock(&domain.state);
                let Some(Node::Scalar(retained)) = state.nodes.last() else { panic!("valid scalar did not specialize") };
                assert!(Arc::ptr_eq(retained, &facts));
            }
            let wrong = session.data_scalar(&Literal::Number(17.0)).unwrap();
            domain.record(&mut session, &wrong, &Fact::Scalar(facts.clone())).unwrap();
            {
                let state = registry_lock(&domain.state);
                assert!(!state.valid);
                let Some(Node::Complex(node)) = state.nodes.last() else { panic!("wrong root specialized") };
                assert!(node.snapshot.is_none());
                session.inspect(&wrong, |mut store, root| {
                    let retained = node.value.value.to_rooted(&mut store);
                    assert!(Rooted::<AnyRef>::ref_eq(&store, root.unwrap_anyref().unwrap(), &retained)?);
                    let original = facts.receiver().value.to_rooted(&mut store);
                    assert!(!Rooted::<AnyRef>::ref_eq(&store, &retained, &original)?);
                    Ok(())
                }).unwrap();
            }
            session.collect().unwrap();
            if let Some((_records, _fixture_domain, target, _seed, _interior)) = macro_fixture {
                assert!(!check(&mut session, &domain, &target).unwrap());
                // Test-only malformed later entry: an invalid Macro-transport
                // domain must still run later checks and propagate owner errors.
                let mut foreign = Session::new_macro().unwrap();
                let (_, foreign_facts) = foreign.data_scalar_with_inputs(&Literal::Nil).unwrap();
                registry_lock(&domain.state).nodes.push(Node::Scalar(foreign_facts));
                assert!(matches!(check(&mut session, &domain, &target), Err(SessionError::ForeignValue)));
            } else {
                assert_eq!(phase, crate::portable::resolve::Phase::Runtime);
                assert!(registry_lock(&domain.state).nodes.iter().all(|node| node.watch().is_none()));
                // This is explicit capability absence, not a passing Runtime
                // proof or a claim about the still-unexecuted sourcechild gate.
            }
        }
    }
    #[test]
    fn scalar_domain_publication_fuel_irq_sweep_preserves_owners_and_recovers() {
        let mut session = Session::new_macro().unwrap();
        let (value, facts) = session.data_scalar_with_inputs(&Literal::String(vec![0, 0xd800, 17])).unwrap();
        let cost = {
            let domain = WriteDomain::new();
            let request = session.operation_budget_scope().unwrap();
            let before = session.store.get_fuel().unwrap();
            domain.record(&mut session, &value, &Fact::Scalar(facts.clone())).unwrap();
            let cost = before - session.store.get_fuel().unwrap();
            drop(request); cost
        };
        let handles = session.stats().external_value_handles;
        let owners = Arc::strong_count(&facts);
        for allowance in 0..cost {
            let domain = WriteDomain::new();
            let request = session.operation_budget_scope().unwrap();
            session.store.set_fuel(allowance).unwrap();
            let error = domain.record(&mut session, &value, &Fact::Scalar(facts.clone())).unwrap_err();
            assert!(matches!(error, SessionError::Trap(ref e) if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "allowance={allowance}: {error:?}");
            assert!(registry_lock(&domain.state).nodes.is_empty());
            assert_eq!(Arc::strong_count(&facts), owners);
            assert_eq!(session.stats().external_value_handles, handles);
            drop(request);
            domain.record(&mut session, &value, &Fact::Scalar(facts.clone())).unwrap();
            assert_eq!(registry_lock(&domain.state).nodes.len(), 1);
        }
        let domain = WriteDomain::new();
        let request = session.operation_budget_scope().unwrap();
        session.interrupt_handle().interrupt();
        assert!(domain.record(&mut session, &value, &Fact::Scalar(facts.clone())).unwrap_err().is_interrupt());
        assert!(registry_lock(&domain.state).nodes.is_empty());
        drop(request);
        domain.record(&mut session, &value, &Fact::Scalar(facts.clone())).unwrap();
        assert_eq!(registry_lock(&domain.state).nodes.len(), 1);
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
        assert_eq!(session.options().fuel_per_operation, crate::portable::DEFAULT_OPERATION_FUEL);
        super::super::memo_leaf_test_support::eval_with_compilation_fuel(
            &mut session, "(extend-type PersistentVector IHash (-hash [_] (do (set! user/child-method-calls (+ user/child-method-calls 1)) 999)))").unwrap();
        assert_eq!(session.options().fuel_per_operation, crate::portable::DEFAULT_OPERATION_FUEL);
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
        assert_eq!(session.options().fuel_per_operation, crate::portable::DEFAULT_OPERATION_FUEL);
        super::super::memo_leaf_test_support::eval_with_compilation_fuel(
            &mut session, "(extend-type PersistentVector IHash (-hash [_] (do (set! user/child-method-calls (+ user/child-method-calls 1)) 999)))").unwrap();
        assert_eq!(session.options().fuel_per_operation, crate::portable::DEFAULT_OPERATION_FUEL);
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
