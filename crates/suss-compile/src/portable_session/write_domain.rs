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
struct Node {
    value: SessionValue,
    snapshot: Option<StorageSnapshot>,
    seed: Option<Arc<CanonicalSeed>>,
    edges: Vec<SessionValue>,
    watch: Option<Weak<Watch>>,
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
        let snapshot = if own && seed.is_none() {
            Some(session.capture_storage_cut_accounted(value, &cuts, self.handles.clone())?)
        } else {
            None
        };
        let links = self.stage_factory_edges(session, &edges)?;
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
        state.nodes.push(Node {
            value: value.clone(),
            snapshot,
            seed,
            edges,
            watch: watch.as_ref().map(Arc::downgrade),
        });
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
        let mut canonical = true;
        for watch in &watches {
            let method_valid = watch.domain_method(store.as_context_mut(), runtime, methods, irq, &layouts)?;
            #[cfg(test)]
            memo_domain_trace(format_args!("domain={self:p} target={target:p} stage=method watch={:p} valid={method_valid}", watch.as_ref()));
            canonical &= method_valid;
        }
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
            let root = Val::AnyRef(Some(node.value.value.to_rooted(&mut scope)));
            if let Some(seed) = &node.seed {
                let seed_valid = seed.write_storage().matches_in_context(
                    scope.as_context_mut(),
                    root,
                    identity,
                    irq,
                )?;
                #[cfg(test)]
                memo_domain_trace(format_args!("domain={self:p} target={target:p} stage=seed node={node:p} valid={seed_valid}"));
                valid &= seed_valid;
            } else if let Some(snapshot) = &node.snapshot {
                let watch = node.watch.as_ref().and_then(Weak::upgrade);
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
            for edge in &node.edges {
                Session::consume_host_work(scope.as_context_mut(), 1, irq)?;
                let edge = edge.value.to_rooted(&mut scope);
                let mut member = edge.as_i31(&scope)?.is_some();
                if member {
                    continue;
                }
                for other in &state.nodes {
                    Session::consume_host_work(scope.as_context_mut(), 1, irq)?;
                    let other_root = other.value.value.to_rooted(&mut scope);
                    member |= Rooted::<AnyRef>::ref_eq(&scope, &edge, &other_root)?;
                    if member {
                        break;
                    }
                    if let Some(seed) = &other.seed {
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
