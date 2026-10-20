//! Private original-factory provenance. A token names an original recorded root,
//! not current health, source ownership, a CheckedChild, or a memo receipt.
use super::*;

pub(crate) struct FactoryOrigin {
    root: SessionValue,
    domain: Weak<WriteDomain>,
    point: FactoryPoint,
}
// Exact original factory/seed node. The index is immutable after sealing;
// dependencies are native weak/index references, never current guest snapshots.
pub(super) struct FactoryPoint {
    domain: Weak<WriteDomain>,
    node: usize,
}
impl FactoryPoint {
    fn retained(&self, session: &mut Session) -> Result<Self> {
        session.charge_host_work((2 * std::mem::size_of::<Self>() + 4) as u64)?;
        Ok(Self {
            domain: self.domain.clone(),
            node: self.node,
        })
    }
}

fn invalid_factory(message: &'static str) -> SessionError {
    SessionError::Host(wasmtime::Error::msg(message))
}
pub(super) fn reserve_factory<T>(
    session: &mut Session,
    values: &mut Vec<T>,
    extra: usize,
) -> Result<()> {
    session.charge_host_work(8)?; // extent/reserve/retirement planning
    let needed = values
        .len()
        .checked_add(extra)
        .ok_or_else(|| invalid_factory("Factory provenance extent overflow"))?;
    if needed <= values.capacity() {
        return Ok(());
    }
    let next = values
        .capacity()
        .checked_mul(2)
        .unwrap_or(needed)
        .max(needed);
    let bytes = next
        .checked_mul(2)
        .and_then(|n| n.checked_add(values.len()))
        .and_then(|n| n.checked_mul(std::mem::size_of::<T>()))
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| invalid_factory("Factory provenance backing overflow"))?;
    session.charge_host_work(
        bytes
            .checked_add(4)
            .ok_or_else(|| invalid_factory("Factory provenance backing overflow"))?,
    )?;
    values
        .try_reserve_exact(next - values.len())
        .map_err(|_| invalid_factory("Factory provenance allocation failed"))
}
impl FactoryOrigin {
    fn retained(&self, session: &mut Session) -> Result<Self> {
        session.charge_host_work((2 * std::mem::size_of::<Self>() + 6) as u64)?;
        session.check(&self.root)?;
        Ok(Self {
            root: self.root.clone(),
            domain: self.domain.clone(),
            point: self.point.retained(session)?,
        })
    }
    fn live(&self, session: &mut Session) -> Result<Option<Arc<WriteDomain>>> {
        session.charge_host_work(4)?; // Weak upgrade, Arc drop, flags/branch
        session.check(&self.root)?;
        Ok(self.domain.upgrade())
    }
}
impl WriteDomain {
    // Called only on a completed native recording. Root identity is checked
    // against original output facts; no guest contents are recaptured here.
    pub(crate) fn original_factory_origin(
        self: &Arc<Self>,
        session: &mut Session,
        root: &SessionValue,
    ) -> Result<Option<FactoryOrigin>> {
        session.charge_host_work(4)?;
        session.check(root)?;
        let state = registry_lock(&self.state);
        if !state.sealed {
            return Ok(None);
        }
        for (index, node) in state.nodes.iter().enumerate() {
            session.charge_host_work(5)?;
            if (node.snapshot.is_some() || node.seed.is_some())
                && session.values_identical(root, &node.value)?
            {
                session.charge_host_work((2 * std::mem::size_of::<FactoryOrigin>() + 6) as u64)?;
                session.charge_host_work((2 * std::mem::size_of::<FactoryPoint>() + 4) as u64)?;
                return Ok(Some(FactoryOrigin {
                    root: root.clone(),
                    domain: Arc::downgrade(self),
                    point: FactoryPoint {
                        domain: Arc::downgrade(self),
                        node: index,
                    },
                }));
            }
        }
        Ok(None)
    }
    // Pool entries are only proposed origins from actual recipe inputs. They
    // are NOT edge membership and matches() ignores them. All checks/staging
    // finish before this one topology publication, before any parent factory.
    pub(crate) fn prepare_factory_inputs(
        &self,
        session: &mut Session,
        inputs: &[FactoryOrigin],
    ) -> Result<()> {
        session.charge_host_work((2 * std::mem::size_of::<Vec<ChildDomain>>() + 16) as u64)?;
        {
            let state = registry_lock(&self.state);
            if state.sealed || !state.nodes.is_empty() || !state.children.is_empty() {
                return Err(invalid_factory(
                    "Factory provenance requires an empty unexposed recording",
                ));
            }
        }
        let mut staged = Vec::new();
        reserve_factory(session, &mut staged, inputs.len())?;
        for input in inputs {
            session.charge_host_work(4)?;
            let Some(domain) = input.live(session)? else {
                return Err(invalid_factory(
                    "Original factory producer retired during attachment",
                ));
            };
            if std::ptr::eq(self, domain.as_ref()) || !registry_lock(&domain.state).sealed {
                return Err(invalid_factory(
                    "Original factory input must be a sealed predecessor",
                ));
            }
            session.charge_host_work(2)?;
            session.charge_host_work(std::mem::size_of::<ChildDomain>() as u64 + 3)?;
            staged.push(ChildDomain::FactoryCandidate(input.retained(session)?));
        }
        let mut state = registry_lock(&self.state);
        if state.sealed || !state.nodes.is_empty() || !state.children.is_empty() {
            return Err(invalid_factory(
                "Factory provenance recording changed during attachment",
            ));
        }
        // No callback or fallible work follows the first publication.
        state.children = staged;
        Ok(())
    }
    // Walk from the offered input's EXACT original output through its captured
    // edges. Merely being another original output in that ledger is insufficient.
    fn origin_contains(
        session: &mut Session,
        origin: &FactoryOrigin,
        value: &SessionValue,
    ) -> Result<Option<FactoryPoint>> {
        session.charge_host_work((4 * std::mem::size_of::<Vec<FactoryPoint>>() + 10) as u64)?;
        session.check(value)?;
        if origin.live(session)?.is_none() {
            return Ok(None);
        }
        let mut pending = Vec::new();
        let mut seen = Vec::new();
        reserve_factory(session, &mut pending, 1)?;
        pending.push(origin.point.retained(session)?);
        while let Some(point) = pending.pop() {
            session.charge_host_work(6)?; // pop, Weak upgrade/drop and identity controls
            let Some(domain) = point.domain.upgrade() else {
                return Ok(None);
            };
            let key = (Arc::as_ptr(&domain) as usize, point.node);
            let mut duplicate = false;
            for old in &seen {
                session.charge_host_work(3)?;
                if *old == key {
                    duplicate = true;
                    break;
                }
            }
            if duplicate {
                continue;
            }
            reserve_factory(session, &mut seen, 1)?;
            session.charge_host_work(3)?;
            seen.push(key);
            let (root, seed, edges) = {
                let state = registry_lock(&domain.state);
                if !state.sealed {
                    return Ok(None);
                }
                let Some(node) = state.nodes.get(point.node) else {
                    return Ok(None);
                };
                if node.snapshot.is_none() && node.seed.is_none() {
                    continue;
                }
                session.charge_host_work(
                    (2 * std::mem::size_of::<SessionValue>()
                        + 2 * std::mem::size_of::<Arc<CanonicalSeed>>()
                        + 8) as u64,
                )?;
                let mut edges = Vec::new();
                reserve_factory(session, &mut edges, node.edges.len())?;
                for edge in &node.edges {
                    session
                        .charge_host_work((2 * std::mem::size_of::<SessionValue>() + 3) as u64)?;
                    edges.push(edge.clone());
                }
                (node.value.clone(), node.seed.clone(), edges)
            };
            if session.values_identical(value, &root)? {
                return Ok(Some(point));
            }
            if let Some(seed) = seed {
                if seed.contains_initial_reference(session, value)? {
                    return Ok(Some(point));
                }
            }
            for edge in edges {
                session.charge_host_work(3)?;
                if let Some(owner) = domain.original_edge_owner(session, &edge)? {
                    reserve_factory(session, &mut pending, 1)?;
                    session.charge_host_work(2)?;
                    pending.push(owner);
                }
            }
        }
        Ok(None)
    }
    // Resolve only this captured edge: own original factory output/seed, a
    // previously proved exact factory edge, or the original strict child root.
    // No ledger-wide output inventory is returned to the caller.
    fn original_edge_owner(
        self: &Arc<Self>,
        session: &mut Session,
        value: &SessionValue,
    ) -> Result<Option<FactoryPoint>> {
        session.charge_host_work((6 * std::mem::size_of::<Vec<FactoryPoint>>() + 10) as u64)?;
        let mut originals = Vec::new();
        let mut linked = Vec::new();
        let mut source = Vec::new();
        {
            let state = registry_lock(&self.state);
            if !state.sealed {
                return Ok(None);
            }
            reserve_factory(session, &mut originals, state.nodes.len())?;
            for (index, node) in state.nodes.iter().enumerate() {
                session.charge_host_work(
                    (2 * std::mem::size_of::<SessionValue>()
                        + 2 * std::mem::size_of::<Arc<CanonicalSeed>>()
                        + 8) as u64,
                )?;
                if node.snapshot.is_some() || node.seed.is_some() {
                    originals.push((index, node.value.clone(), node.seed.clone()));
                }
            }
            reserve_factory(session, &mut linked, state.children.len())?;
            reserve_factory(session, &mut source, state.children.len())?;
            for child in &state.children {
                session.charge_host_work(4)?;
                match child {
                    ChildDomain::FactoryReference { value, owner, .. } => {
                        session.charge_host_work(
                            (2 * std::mem::size_of::<SessionValue>() + 3) as u64,
                        )?;
                        linked.push((value.clone(), owner.retained(session)?));
                    }
                    ChildDomain::Source { value, domain } => {
                        session.charge_host_work(
                            (2 * std::mem::size_of::<SessionValue>() + 6) as u64,
                        )?;
                        source.push((value.clone(), domain.clone()));
                    }
                    ChildDomain::FactoryCandidate(_) => {}
                }
            }
        }
        for (index, root, seed) in originals {
            session.charge_host_work(3)?;
            if session.values_identical(value, &root)?
                || match seed {
                    Some(seed) => seed.contains_initial_reference(session, value)?,
                    None => false,
                }
            {
                session.charge_host_work((2 * std::mem::size_of::<FactoryPoint>() + 4) as u64)?;
                return Ok(Some(FactoryPoint {
                    domain: Arc::downgrade(self),
                    node: index,
                }));
            }
        }
        for (root, owner) in linked {
            session.charge_host_work(3)?;
            if session.values_identical(value, &root)? {
                return Ok(Some(owner));
            }
        }
        for (root, domain) in source {
            session.charge_host_work(3)?;
            if session.values_identical(value, &root)? {
                if let Some(origin) = domain.original_factory_origin(session, &root)? {
                    return Ok(Some(origin.point));
                }
            }
        }
        Ok(None)
    }
    // Stage a link for EACH actual original ctor/array edge. Unmatched edges
    // stay unmatched; no whitelist or ordinary construction failure is added.
    pub(super) fn stage_factory_edges(
        &self,
        session: &mut Session,
        edges: &[SessionValue],
    ) -> Result<Vec<ChildDomain>> {
        session.charge_host_work((4 * std::mem::size_of::<Vec<FactoryOrigin>>() + 10) as u64)?;
        let mut candidates = Vec::new();
        {
            let state = registry_lock(&self.state);
            reserve_factory(session, &mut candidates, state.children.len())?;
            for child in &state.children {
                session.charge_host_work(3)?;
                if let ChildDomain::FactoryCandidate(origin) = child {
                    candidates.push(origin.retained(session)?);
                }
            }
        }
        let mut links = Vec::new();
        if candidates.is_empty() {
            return Ok(links);
        }
        reserve_factory(session, &mut links, edges.len())?;
        for edge in edges {
            session.charge_host_work(3)?;
            // Already owned local inputs retain their original membership rule;
            // do not search unrelated predecessor outputs for them.
            let local = {
                let state = registry_lock(&self.state);
                let mut local = false;
                for node in &state.nodes {
                    session.charge_host_work(3)?;
                    if (node.snapshot.is_some() || node.seed.is_some())
                        && session.values_identical(edge, &node.value)?
                    {
                        local = true;
                        break;
                    }
                }
                local
            };
            if local {
                continue;
            }
            for origin in &candidates {
                session.charge_host_work(2)?;
                if let Some(owner) = Self::origin_contains(session, origin, edge)? {
                    session
                        .charge_host_work((2 * std::mem::size_of::<SessionValue>() + 4) as u64)?;
                    session.charge_host_work(std::mem::size_of::<ChildDomain>() as u64 + 3)?;
                    links.push(ChildDomain::FactoryReference {
                        value: edge.clone(),
                        origin: origin.retained(session)?,
                        owner,
                    });
                    break;
                }
            }
        }
        Ok(links)
    }
}

// Native proof scratch: allocation, relocation and backing retirement are paid
// before reserve. No borrowed Val here outlives the enclosing Wasmtime scope.
fn reserve_store<T>(
    mut store: StoreContextMut<'_, ()>,
    irq: &AtomicBool,
    values: &mut Vec<T>,
    extra: usize,
) -> Result<()> {
    Session::consume_host_work(store.as_context_mut(), 8, irq)?;
    let needed = values
        .len()
        .checked_add(extra)
        .ok_or_else(|| invalid_factory("Factory proof extent overflow"))?;
    if needed <= values.capacity() {
        return Ok(());
    }
    let next = values
        .capacity()
        .checked_mul(2)
        .unwrap_or(needed)
        .max(needed);
    let bytes = next
        .checked_mul(2)
        .and_then(|n| n.checked_add(values.len()))
        .and_then(|n| n.checked_mul(std::mem::size_of::<T>()))
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| invalid_factory("Factory proof backing overflow"))?;
    Session::consume_host_work(store.as_context_mut(), bytes, irq)?;
    values
        .try_reserve_exact(next - values.len())
        .map_err(|_| invalid_factory("Factory proof allocation failed"))
}
pub(super) fn append_layouts(
    mut store: StoreContextMut<'_, ()>,
    irq: &AtomicBool,
    destination: &mut Vec<Val>,
    values: &[Val],
) -> Result<()> {
    reserve_store(store.as_context_mut(), irq, destination, values.len())?;
    Session::consume_host_work(
        store.as_context_mut(),
        (values.len() as u64)
            .checked_mul(std::mem::size_of::<Val>() as u64 + 2)
            .ok_or_else(|| invalid_factory("Factory layout publication overflow"))?,
        irq,
    )?;
    destination.extend_from_slice(values);
    Ok(())
}
struct FactoryGroup {
    root: Arc<WriteDomain>,
    inputs: Vec<Arc<WriteDomain>>,
}
impl FactoryOrigin {
    fn live_store(
        &self,
        mut store: StoreContextMut<'_, ()>,
        identity: u64,
        irq: &AtomicBool,
    ) -> Result<Option<Arc<WriteDomain>>> {
        Session::consume_host_work(store.as_context_mut(), 5, irq)?;
        if self.root.identity != identity {
            return Err(invalid_factory("Foreign original factory input"));
        }
        Ok(self.domain.upgrade())
    }
}
impl WriteDomain {
    // Collect ONLY exact factory-input owner links. Source CheckedChild links
    // are separate strict groups and never inherit another group's layout proof.
    fn factory_component(
        self: &Arc<Self>,
        mut store: StoreContextMut<'_, ()>,
        identity: u64,
        irq: &AtomicBool,
    ) -> Result<(Vec<Arc<WriteDomain>>, bool)> {
        Session::consume_host_work(
            store.as_context_mut(),
            (6 * std::mem::size_of::<Vec<Arc<WriteDomain>>>() + 8) as u64,
            irq,
        )?;
        let mut pending = Vec::new();
        let mut output = Vec::new();
        let mut seen = Vec::new();
        reserve_store(store.as_context_mut(), irq, &mut seen, 1)?;
        Session::consume_host_work(store.as_context_mut(), 2, irq)?;
        seen.push(Arc::as_ptr(self) as usize);
        let mut complete =
            self.queue_factory_inputs(store.as_context_mut(), identity, irq, &mut pending)?;
        while let Some(current) = pending.pop() {
            Session::consume_host_work(store.as_context_mut(), 4, irq)?;
            let pointer = Arc::as_ptr(&current) as usize;
            let mut duplicate = false;
            for old in &seen {
                Session::consume_host_work(store.as_context_mut(), 2, irq)?;
                if *old == pointer {
                    duplicate = true;
                    break;
                }
            }
            if duplicate {
                continue;
            }
            if !registry_lock(&current.state).sealed {
                return Err(invalid_factory("Unsealed original factory producer"));
            }
            reserve_store(store.as_context_mut(), irq, &mut seen, 1)?;
            Session::consume_host_work(store.as_context_mut(), 2, irq)?;
            seen.push(pointer);
            complete &= current.queue_factory_inputs(
                store.as_context_mut(),
                identity,
                irq,
                &mut pending,
            )?;
            reserve_store(store.as_context_mut(), irq, &mut output, 1)?;
            Session::consume_host_work(store.as_context_mut(), 2, irq)?;
            output.push(current);
        }
        Ok((output, complete))
    }
    fn queue_factory_inputs(
        &self,
        mut store: StoreContextMut<'_, ()>,
        identity: u64,
        irq: &AtomicBool,
        pending: &mut Vec<Arc<WriteDomain>>,
    ) -> Result<bool> {
        let state = registry_lock(&self.state);
        reserve_store(store.as_context_mut(), irq, pending, state.children.len())?;
        let mut complete = true;
        for child in &state.children {
            Session::consume_host_work(store.as_context_mut(), 3, irq)?;
            if let ChildDomain::FactoryReference { origin, .. } = child {
                match origin.live_store(store.as_context_mut(), identity, irq)? {
                    Some(domain) => pending.push(domain),
                    None => complete = false,
                }
            }
        }
        Ok(complete)
    }
    fn queue_strict_children(
        &self,
        mut store: StoreContextMut<'_, ()>,
        irq: &AtomicBool,
        pending: &mut Vec<Arc<WriteDomain>>,
    ) -> Result<()> {
        let state = registry_lock(&self.state);
        reserve_store(store.as_context_mut(), irq, pending, state.children.len())?;
        for child in &state.children {
            Session::consume_host_work(store.as_context_mut(), 3, irq)?;
            if let ChildDomain::Source { domain, .. } = child {
                Session::consume_host_work(store.as_context_mut(), 2, irq)?;
                pending.push(domain.clone());
            }
        }
        Ok(())
    }
    // Every descriptor is grounded by the existing exact original receiver,
    // source-domain binding, descriptor/storage/slot checks in Watch::anchor.
    // This broadens no descriptor/class allowlist. It composes the SAME private
    // factory ownership component which one recording could originally own.
    fn factory_anchors(
        &self,
        mut store: StoreContextMut<'_, ()>,
        irq: &AtomicBool,
        receivers: &[Val],
        output: &mut Vec<Val>,
    ) -> Result<()> {
        let watches = {
            let state = registry_lock(&self.state);
            let mut watches = Vec::new();
            reserve_store(store.as_context_mut(), irq, &mut watches, state.nodes.len())?;
            for node in &state.nodes {
                Session::consume_host_work(store.as_context_mut(), 5, irq)?;
                if let Some(watch) = node.watch.as_ref().and_then(Weak::upgrade) {
                    watches.push(watch);
                }
            }
            watches
        };
        reserve_store(store.as_context_mut(), irq, output, receivers.len())?;
        for receiver in receivers {
            Session::consume_host_work(store.as_context_mut(), 1, irq)?;
            for watch in &watches {
                Session::consume_host_work(store.as_context_mut(), 1, irq)?;
                if let Some(descriptor) =
                    watch.anchor(store.as_context_mut(), receiver, self, irq)?
                {
                    Session::consume_host_work(
                        store.as_context_mut(),
                        std::mem::size_of::<Val>() as u64 + 2,
                        irq,
                    )?;
                    output.push(descriptor);
                    break;
                }
            }
        }
        Ok(())
    }
    pub(super) fn matches_factory_closure(
        self: &Arc<Self>,
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
        // Headers, init/move and retirement before acquiring any new scratch.
        Session::consume_host_work(
            store.as_context_mut(),
            (4 * std::mem::size_of::<Vec<FactoryGroup>>() + 12) as u64,
            irq,
        )?;
        let mut pending = Vec::new();
        let mut groups = Vec::new();
        let mut seen = Vec::new();
        reserve_store(store.as_context_mut(), irq, &mut pending, 1)?;
        Session::consume_host_work(store.as_context_mut(), 2, irq)?;
        pending.push(self.clone());
        let mut complete = true;
        while let Some(root) = pending.pop() {
            Session::consume_host_work(store.as_context_mut(), 4, irq)?;
            let pointer = Arc::as_ptr(&root) as usize;
            let mut duplicate = false;
            for old in &seen {
                Session::consume_host_work(store.as_context_mut(), 2, irq)?;
                if *old == pointer {
                    duplicate = true;
                    break;
                }
            }
            if duplicate {
                continue;
            }
            if !Arc::ptr_eq(&root, self) && !registry_lock(&root.state).sealed {
                return Err(invalid_factory("Unsealed certified domain dependency"));
            }
            reserve_store(store.as_context_mut(), irq, &mut seen, 1)?;
            Session::consume_host_work(store.as_context_mut(), 2, irq)?;
            seen.push(pointer);
            let (inputs, available) =
                root.factory_component(store.as_context_mut(), identity, irq)?;
            complete &= available;
            root.queue_strict_children(store.as_context_mut(), irq, &mut pending)?;
            for input in &inputs {
                Session::consume_host_work(store.as_context_mut(), 1, irq)?;
                input.queue_strict_children(store.as_context_mut(), irq, &mut pending)?;
            }
            reserve_store(store.as_context_mut(), irq, &mut groups, 1)?;
            Session::consume_host_work(store.as_context_mut(), 3, irq)?;
            groups.push(FactoryGroup { root, inputs });
        }
        let mut valid = complete;
        for group in groups {
            Session::consume_host_work(store.as_context_mut(), 3, irq)?;
            Session::consume_host_work(
                store.as_context_mut(),
                (2 * std::mem::size_of::<Vec<Val>>() + 4) as u64,
                irq,
            )?;
            let mut layouts = Vec::new();
            if !group.inputs.is_empty() {
                group
                    .root
                    .factory_anchors(store.as_context_mut(), irq, receivers, &mut layouts)?;
                for input in &group.inputs {
                    Session::consume_host_work(store.as_context_mut(), 1, irq)?;
                    input.factory_anchors(store.as_context_mut(), irq, receivers, &mut layouts)?;
                }
            }
            let is_target = Arc::ptr_eq(&group.root, self);
            valid &= group.root.matches_local(
                store.as_context_mut(),
                runtime,
                identity,
                irq,
                methods,
                prototype,
                if is_target { staged } else { None },
                target,
                receivers,
                is_target,
                &layouts,
            )?;
            for input in group.inputs {
                Session::consume_host_work(store.as_context_mut(), 1, irq)?;
                // Parent staged receipt never excuses ANY predecessor slot.
                valid &= input.matches_local(
                    store.as_context_mut(),
                    runtime,
                    identity,
                    irq,
                    methods,
                    prototype,
                    None,
                    target,
                    receivers,
                    false,
                    &layouts,
                )?;
            }
        }
        Ok(valid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::hir::Literal;
    use crate::portable_macro_data::{FormBridge, MetadataShape};
    use crate::portable_session::construction_records::ConstructionRecords;
    struct Owner {
        value: SessionValue,
        records: ConstructionRecords,
    }
    fn finished(session: &mut Session, recording: FormBridge, value: SessionValue) -> Owner {
        let records = recording.into_records().unwrap();
        records.seal_source_domain(session).unwrap();
        Owner { value, records }
    }
    fn symbol(session: &mut Session, bridge: &FormBridge) -> Owner {
        let recording = bridge.recording(session).unwrap();
        let value = recording
            .identifier(session, None, "factory-origin-warm", false, None)
            .unwrap();
        finished(session, recording, value)
    }
    fn metadata(session: &mut Session, bridge: &FormBridge) -> Owner {
        let recording = bridge.recording(session).unwrap();
        let form = suss_reader::forms::read_forms("{:marker :kept}")
            .unwrap()
            .remove(0);
        let value = recording.quote_ref(session, &form).unwrap();
        finished(session, recording, value)
    }
    fn origin(session: &mut Session, owner: &Owner) -> FactoryOrigin {
        owner
            .records
            .original_factory_origin(session, &owner.value)
            .unwrap()
            .unwrap()
    }
    fn wrap(
        session: &mut Session,
        bridge: &FormBridge,
        base: &Owner,
        meta: &Owner,
        origins: &[FactoryOrigin],
    ) -> Owner {
        let recording = bridge.recording(session).unwrap();
        recording
            .bind_original_factory_inputs(session, origins)
            .unwrap();
        let value = recording
            .with_form_metadata(
                session,
                MetadataShape::Symbol,
                &base.value,
                &meta.value,
                &[],
            )
            .unwrap();
        finished(session, recording, value)
    }
    fn watch(owner: &Owner) -> Arc<Watch> {
        let domain = owner.records.write_domain();
        let state = registry_lock(&domain.state);
        state
            .nodes
            .iter()
            .find_map(|node| node.watch.as_ref().and_then(Weak::upgrade))
            .unwrap()
    }
    fn hash(session: &mut Session, value: &SessionValue) -> SessionValue {
        let hash = session.eval("hash").unwrap();
        session.invoke(&hash, &[value]).unwrap()
    }
    #[test]
    fn original_factory_inputs_close_copied_symbol_fields_through_live_custom_hash_and_gc() {
        use crate::portable_session::memo_leaf_test_support::Method;
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let base = symbol(&mut session, &bridge);
        let meta = metadata(&mut session, &bridge);
        let origins = [origin(&mut session, &base), origin(&mut session, &meta)];
        let copy = wrap(&mut session, &bridge, &base, &meta, &origins);
        assert!(!session.values_identical(&base.value, &copy.value).unwrap());
        hash(&mut session, &copy.value);
        let watch = watch(&copy);
        assert!(
            watch.matches(&mut session).unwrap(),
            "canonical warming must have full original input closure"
        );
        session.eval("(def leaf-original-method nil)").unwrap();
        let original = Method::capture(&mut session, "Symbol").unwrap();
        session
            .eval("(def factory-origin-hash-calls 0) (extend-type Symbol IHash (-hash [self] (do (set! factory-origin-hash-calls (inc factory-origin-hash-calls)) (set! (.-_hash self) 91) 91)))")
            .unwrap();
        let answer = hash(&mut session, &copy.value);
        assert_eq!(
            crate::portable_repl::display(&mut session, &answer).unwrap(),
            "91"
        );
        let calls = session.eval("factory-origin-hash-calls").unwrap();
        assert_eq!(
            crate::portable_repl::display(&mut session, &calls).unwrap(),
            "1"
        );
        let fields = session.data_fields(&copy.value).unwrap();
        assert!(session.values_identical(&fields[4], &meta.value).unwrap());
        session.collect().unwrap();
        assert!(
            watch.matches(&mut session).unwrap(),
            "real scoped replacement must retain original receiver/closure authority"
        );
        original.restore(&mut session).unwrap();
        assert!(watch.matches(&mut session).unwrap());
    }
    #[test]
    fn original_factory_inputs_reject_equal_but_foreign_references_and_missing_metadata_edge() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let base = symbol(&mut session, &bridge);
        let equal = symbol(&mut session, &bridge);
        let meta = metadata(&mut session, &bridge);
        assert!(!session.values_identical(&base.value, &equal.value).unwrap());
        let wrong = [origin(&mut session, &equal), origin(&mut session, &meta)];
        let copy = wrap(&mut session, &bridge, &base, &meta, &wrong);
        hash(&mut session, &copy.value);
        assert!(
            !watch(&copy).matches(&mut session).unwrap(),
            "equal text is not original input ownership"
        );
        let missing = [origin(&mut session, &base)];
        let copy = wrap(&mut session, &bridge, &base, &meta, &missing);
        hash(&mut session, &copy.value);
        assert!(
            !watch(&copy).matches(&mut session).unwrap(),
            "unowned metadata is still an unowned original edge"
        );
    }
    #[test]
    fn original_factory_inputs_cannot_borrow_unused_sibling_outputs_from_the_same_recording() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let recording = bridge.recording(&mut session).unwrap();
        let form = suss_reader::forms::read_forms("{:marker :kept}")
            .unwrap()
            .remove(0);
        // Genuine factory output, but NOT an original input of the Symbol root.
        let unused = recording.quote_ref(&mut session, &form).unwrap();
        let value = recording
            .identifier(&mut session, None, "factory-origin-warm", false, None)
            .unwrap();
        let base = finished(&mut session, recording, value);
        let only_base = [origin(&mut session, &base)];
        let recording = bridge.recording(&mut session).unwrap();
        recording
            .bind_original_factory_inputs(&mut session, &only_base)
            .unwrap();
        let value = recording
            .with_form_metadata(
                &mut session,
                MetadataShape::Symbol,
                &base.value,
                &unused,
                &[],
            )
            .unwrap();
        let copy = finished(&mut session, recording, value);
        hash(&mut session, &copy.value);
        assert!(
            !watch(&copy).matches(&mut session).unwrap(),
            "recording-wide membership must not substitute for the actual root dependency closure"
        );
        // Positive control: offer that exact map as a distinct actual input.
        let actual_inputs = [
            origin(&mut session, &base),
            base.records
                .original_factory_origin(&mut session, &unused)
                .unwrap()
                .unwrap(),
        ];
        let recording = bridge.recording(&mut session).unwrap();
        recording
            .bind_original_factory_inputs(&mut session, &actual_inputs)
            .unwrap();
        let value = recording
            .with_form_metadata(
                &mut session,
                MetadataShape::Symbol,
                &base.value,
                &unused,
                &[],
            )
            .unwrap();
        let copy = finished(&mut session, recording, value);
        hash(&mut session, &copy.value);
        assert!(watch(&copy).matches(&mut session).unwrap());
    }
    #[test]
    fn original_factory_inputs_cannot_borrow_initial_seed_storage_from_an_unrelated_root() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let recording = bridge.recording(&mut session).unwrap();
        // A genuine private canonical seed in the SAME original recording,
        // but not reachable from the offered Symbol's captured arguments.
        let empty_map = recording.map_values(&mut session, &[]).unwrap();
        let value = recording
            .identifier(&mut session, None, "factory-origin-warm", false, None)
            .unwrap();
        let base = finished(&mut session, recording, value);
        assert!(base.records.canonical_seed_fact_count_for_test() > 0);
        let meta = metadata(&mut session, &bridge);
        for offer_seed in [false, true] {
            let mut origins = vec![origin(&mut session, &base), origin(&mut session, &meta)];
            if offer_seed {
                origins.push(
                    base.records
                        .original_factory_origin(&mut session, &empty_map)
                        .unwrap()
                        .expect("actual canonical seed output"),
                );
            }
            let recording = bridge.recording(&mut session).unwrap();
            recording
                .bind_original_factory_inputs(&mut session, &origins)
                .unwrap();
            // The real metadata constructor copies the seed's ORIGINAL backing
            // reference. No test-created Fact or current-storage recapture.
            let value = recording
                .with_form_metadata(
                    &mut session,
                    MetadataShape::Map { large: false },
                    &empty_map,
                    &meta.value,
                    &[],
                )
                .unwrap();
            let copy = finished(&mut session, recording, value);
            session.collect().unwrap();
            let fields = session.data_fields(&copy.value).unwrap();
            assert!(session.values_identical(&fields[0], &meta.value).unwrap());
            // Validate the complete original domains, rather than warming a
            // preinitialized seed hash or manufacturing an initially-nil slot.
            let domain = copy.records.write_domain();
            let watch = watch(&copy);
            let methods = session.canonical_methods.clone().unwrap();
            let prototype = session.prototype_origin.clone().unwrap();
            let irq = session.host_work_interrupt();
            let runtime = session.runtime;
            let identity = session.identity;
            let valid = domain
                .matches(
                    session.store.as_context_mut(),
                    runtime,
                    identity,
                    &irq,
                    &methods,
                    &prototype,
                    None,
                    &watch,
                    &[],
                )
                .unwrap();
            assert_eq!(
                valid, offer_seed,
                "only the exact offered seed root grants its initial backing membership"
            );
        }
    }
    #[test]
    fn original_factory_inputs_reject_mutated_original_producer_without_recapturing_it() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let base = symbol(&mut session, &bridge);
        let meta = metadata(&mut session, &bridge);
        let origins = [origin(&mut session, &base), origin(&mut session, &meta)];
        let change = session
            .eval("(fn [sym] (set! (.-name sym) \"after-poison\"))")
            .unwrap();
        session.invoke(&change, &[&base.value]).unwrap();
        let fields = session.data_fields(&base.value).unwrap();
        assert_eq!(
            crate::portable_repl::display(&mut session, &fields[1]).unwrap(),
            "\"after-poison\""
        );
        let copy = wrap(&mut session, &bridge, &base, &meta, &origins);
        hash(&mut session, &copy.value);
        assert!(!watch(&copy).matches(&mut session).unwrap());
    }
    #[test]
    fn original_factory_attachment_fuel_irq_or_foreign_failure_publishes_no_partial_links_and_retries()
     {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let base = symbol(&mut session, &bridge);
        let meta = metadata(&mut session, &bridge);
        let origins = [origin(&mut session, &base), origin(&mut session, &meta)];
        for irq in [false, true] {
            let recording = bridge.recording(&mut session).unwrap();
            let op = session.operation_budget_scope().unwrap();
            if irq {
                session.interrupt_handle().interrupt();
            } else {
                session.store.set_fuel(0).unwrap();
            }
            let error = recording
                .bind_original_factory_inputs(&mut session, &origins)
                .unwrap_err();
            if irq {
                assert!(error.is_interrupt(), "{error:?}");
            } else {
                assert!(
                    matches!(error, SessionError::Trap(ref e) if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
                    "{error:?}"
                );
            }
            drop(op);
            // Genuine next ordinary scope, same recording/options/inputs. A
            // partially installed pool would make this second bind fail.
            let op = session.operation_budget_scope().unwrap();
            recording
                .bind_original_factory_inputs(&mut session, &origins)
                .unwrap();
            let value = recording
                .with_form_metadata(
                    &mut session,
                    MetadataShape::Symbol,
                    &base.value,
                    &meta.value,
                    &[],
                )
                .unwrap();
            let owner = finished(&mut session, recording, value);
            hash(&mut session, &owner.value);
            assert!(watch(&owner).matches(&mut session).unwrap());
            drop(op);
        }
        let mut other = Session::new_macro().unwrap();
        let other_bridge = FormBridge::new(&mut other).unwrap();
        let foreign = symbol(&mut other, &other_bridge);
        let foreign_origins = [origin(&mut session, &base), origin(&mut other, &foreign)];
        let recording = bridge.recording(&mut session).unwrap();
        assert!(
            recording
                .bind_original_factory_inputs(&mut session, &foreign_origins)
                .is_err()
        );
        recording
            .bind_original_factory_inputs(&mut session, &origins)
            .unwrap();
        let value = recording
            .with_form_metadata(
                &mut session,
                MetadataShape::Symbol,
                &base.value,
                &meta.value,
                &[],
            )
            .unwrap();
        let owner = finished(&mut session, recording, value);
        hash(&mut session, &owner.value);
        assert!(watch(&owner).matches(&mut session).unwrap());
    }
    #[test]
    fn original_factory_input_domains_are_weak_and_retire_without_recursive_ownership() {
        let mut session = Session::new_macro().unwrap();
        // New lifetime fixture only; shipped/default/exhaustion budgets unchanged.
        session.set_operation_fuel(100_000_000_000);
        let op = session.operation_budget_scope().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let recording = bridge.recording(&mut session).unwrap();
        let value = recording
            .scalar(&mut session, &Literal::Number(17.0))
            .unwrap();
        let mut owners = vec![finished(&mut session, recording, value)];
        for _ in 0..512 {
            let previous = owners.last().unwrap();
            let origins = [origin(&mut session, previous)];
            let recording = bridge.recording(&mut session).unwrap();
            recording
                .bind_original_factory_inputs(&mut session, &origins)
                .unwrap();
            let value = recording
                .vector_values(&mut session, &[previous.value.clone()])
                .unwrap();
            owners.push(finished(&mut session, recording, value));
        }
        let prior = Arc::downgrade(&owners[owners.len() - 2].records.write_domain());
        let last = owners.pop().unwrap();
        drop(owners); // same child-first lifetime ordering as Native::Drop
        assert!(
            prior.upgrade().is_none(),
            "parent factory links must not retain producer domain Arcs"
        );
        let domain = last.records.write_domain();
        let watch = watch(&last);
        let methods = session.canonical_methods.clone().unwrap();
        let prototype = session.prototype_origin.clone().unwrap();
        let irq = session.host_work_interrupt();
        let runtime = session.runtime;
        let identity = session.identity;
        assert!(
            !domain
                .matches(
                    session.store.as_context_mut(),
                    runtime,
                    identity,
                    &irq,
                    &methods,
                    &prototype,
                    None,
                    &watch,
                    &[]
                )
                .unwrap(),
            "retired original factory owners cannot be replaced by rooted guest values"
        );
        drop(last);
        drop(op);
    }
}
