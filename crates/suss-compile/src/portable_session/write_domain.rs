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
struct State {
    valid: bool,
    nodes: Vec<Node>,
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
            }),
            handles: Arc::new(AtomicUsize::new(0)),
        })
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
        let mut state = registry_lock(&self.state);
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
        Ok(())
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
        layout_descriptor: Option<&Val>,
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
        let mut canonical = true;
        for watch in &watches {
            canonical &= watch.domain_method(store.as_context_mut(), runtime, methods, irq, layout_descriptor)?;
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
        let mut scope = RootScope::new(store);
        for node in &state.nodes {
            Session::consume_host_work(scope.as_context_mut(), 1, irq)?;
            let root = Val::AnyRef(Some(node.value.value.to_rooted(&mut scope)));
            if let Some(seed) = &node.seed {
                valid &= seed.write_storage().matches_in_context(
                    scope.as_context_mut(),
                    root,
                    identity,
                    irq,
                )?;
            } else if let Some(snapshot) = &node.snapshot {
                let watch = node.watch.as_ref().and_then(Weak::upgrade);
                valid &= snapshot.matches_in_context_with_slots(
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
                )?;
            } else {
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
                valid &= member;
            }
        }
        Ok(valid)
    }
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
