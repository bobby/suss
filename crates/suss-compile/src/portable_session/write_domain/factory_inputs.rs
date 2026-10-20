//! Private original-factory provenance. A token names an original recorded root,
//! not current health, source ownership, a CheckedChild, or a memo receipt.
use super::*;

pub(crate) struct FactoryOrigin {
    root: SessionValue,
    domain: Weak<WriteDomain>,
    point: FactoryPoint,
}
// Exact factory output, or a downward original snapshot subroot. A member
// retains its Node/terminal scope across every producer generation; it never
// carries permission to revisit the enclosing seed or another ledger sibling.
pub(super) struct FactoryPoint {
    domain: Weak<WriteDomain>,
    node: usize,
    member: Option<super::super::canonical_seeds::SeedMember>,
}
impl FactoryPoint {
    fn retained(&self, session: &mut Session) -> Result<Self> {
        session.charge_host_work((2 * std::mem::size_of::<Self>() + 7) as u64)?;
        let member = match &self.member {
            Some(member) => Some(member.retained(session)?),
            None => None,
        };
        Ok(Self {
            domain: self.domain.clone(),
            node: self.node,
            member,
        })
    }
}

// Immediate edges already satisfy construction membership in matches_local.
// Classify without guest calls, current-storage capture or an operation refill.
fn factory_immediate(session: &mut Session, value: &SessionValue) -> Result<bool> {
    session.check(value)?;
    session.charge_host_work(8)?; // owner/kind controls, scoped root and retirement
    let mut scope = RootScope::new(&mut session.store);
    Ok(value.value.to_rooted(&mut scope).as_i31(&scope)?.is_some())
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
// prepare_factory_inputs installs only candidates into an empty children vector.
// attach_child/record only append Source/FactoryReference; seal removes candidates.
// Thus the candidate pool is exactly an initial prefix, never a later suffix.
// This optimization applies ONLY to selecting offers, not membership traversal.
fn factory_candidate_prefix_len(session: &mut Session, children: &[ChildDomain]) -> Result<usize> {
    session.charge_host_work((
        2 * std::mem::size_of::<std::slice::Iter<'_, ChildDomain>>()
        + 2 * std::mem::size_of::<usize>() + 8
    ) as u64)?; // iterator/count initialization, forwarding and retirement
    let mut count = 0;
    let mut children = children.iter();
    loop {
        session.charge_host_work(3)?; // next/tag, including first noncandidate/None
        match children.next() {
            Some(ChildDomain::FactoryCandidate(_)) => {
                session.charge_host_work(2)?; // increment/control before work
                count += 1; // bounded by the existing slice length
            }
            _ => return Ok(count),
        }
    }
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
                session.charge_host_work((2 * std::mem::size_of::<FactoryPoint>() + 6) as u64)?;
                return Ok(Some(FactoryOrigin {
                    root: root.clone(),
                    domain: Arc::downgrade(self),
                    point: FactoryPoint {
                        domain: Arc::downgrade(self),
                        node: index,
                        member: None,
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
            // A restricted snapshot scope is processed BEFORE output-node
            // deduplication: it cannot mark or expand its containing factory
            // node. The snapshot walker separately deduplicates only original
            // downward Nodes; opaque/immediate members stay terminal.
            session.charge_host_work(4)?;
            if let Some(member) = &point.member {
                let seed = {
                    let state = registry_lock(&domain.state);
                    if !state.sealed { return Ok(None); }
                    session.charge_host_work((2 * std::mem::size_of::<Arc<CanonicalSeed>>() + 4) as u64)?;
                    state.nodes.get(point.node).and_then(|node| node.seed.clone())
                };
                let Some(seed) = seed else { return Ok(None); };
                if let Some(descendant) = seed.original_member(session, value, Some(member))? {
                    let mut selected = point;
                    selected.member = Some(descendant);
                    return Ok(Some(selected));
                }
                continue;
            }
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
            // Compare the borrowed exact output first. A hit needs neither
            // owned root/seed/edge staging nor expansion scratch.
            let seed = {
                session.charge_host_work(8)?; // lock/sealed/index/eligibility reads
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
                session.charge_host_work(3)?; // borrowed root and match controls
                if session.values_identical(value, &node.value)? {
                    return Ok(Some(point));
                }
                session.charge_host_work(2)?;
                match &node.seed {
                    Some(seed) => {
                        session.charge_host_work(
                            (2 * std::mem::size_of::<Arc<CanonicalSeed>>() + 4) as u64,
                        )?;
                        Some(seed.clone())
                    }
                    None => None,
                }
            };
            if let Some(seed) = seed {
                if let Some(scope) = seed.original_member(session, value, None)? {
                    session.charge_host_work(3)?;
                    let mut selected = point;
                    selected.member = Some(scope);
                    return Ok(Some(selected));
                }
            }
            // Exact-root and seed-member misses alone require captured edges.
            // The sealed topology is immutable; no lock spans recursion or a
            // seed-subtree query. Copy only this node's original edge list.
            let edges = {
                session.charge_host_work(
                    (2 * std::mem::size_of::<Vec<SessionValue>>() + 8) as u64,
                )?;
                let state = registry_lock(&domain.state);
                if !state.sealed {
                    return Ok(None);
                }
                let Some(node) = state.nodes.get(point.node) else {
                    return Ok(None);
                };
                let mut edges = Vec::new();
                reserve_factory(session, &mut edges, node.edges.len())?;
                for edge in &node.edges {
                    session.charge_host_work(
                        (2 * std::mem::size_of::<SessionValue>() + 3) as u64,
                    )?;
                    edges.push(edge.clone());
                }
                edges
            };
            for edge in edges {
                session.charge_host_work(3)?;
                if factory_immediate(session, &edge)? {
                    continue;
                }
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
        session.charge_host_work(10)?; // lock/sealed/scan initialization controls
        let source = {
            let state = registry_lock(&self.state);
            if !state.sealed {
                return Ok(None);
            }
            // Preserve exact local-output priority. Native equality is the
            // existing rooted ref_eq helper, with no guest call or recapture.
            for (index, node) in state.nodes.iter().enumerate() {
                session.charge_host_work(8)?; // iterator/index/eligibility controls
                if node.snapshot.is_none() && node.seed.is_none() {
                    continue;
                }
                session.charge_host_work(3)?; // original exact-comparison controls
                if session.values_identical(value, &node.value)? {
                    session.charge_host_work(
                        (2 * std::mem::size_of::<FactoryPoint>() + 6) as u64,
                    )?;
                    return Ok(Some(FactoryPoint {
                        domain: Arc::downgrade(self),
                        node: index,
                        member: None,
                    }));
                }
            }
            // Preserve exact linked-reference priority and its complete scoped
            // member. Retain ONLY the selected owner; unused links need no
            // SessionValue/Weak/seed-member copies or temporary inventory.
            for child in &state.children {
                session.charge_host_work(4)?;
                if let ChildDomain::FactoryReference { value: root, owner, .. } = child {
                    session.charge_host_work(3)?;
                    if session.values_identical(value, root)? {
                        return Ok(Some(owner.retained(session)?));
                    }
                }
            }
            // Strict source fallback only after both prior searches miss.
            // Prepay/stage these actual roots and Arcs, then drop the parent
            // lock before consulting another domain (including possible self).
            session.charge_host_work(
                (2 * std::mem::size_of::<Vec<(SessionValue, Arc<WriteDomain>)>>() + 10) as u64,
            )?;
            let mut source = Vec::new();
            for child in &state.children {
                session.charge_host_work(4)?;
                if let ChildDomain::Source { value, domain } = child {
                    reserve_factory(session, &mut source, 1)?;
                    session.charge_host_work(
                        (2 * (std::mem::size_of::<SessionValue>()
                            + std::mem::size_of::<Arc<WriteDomain>>()) + 6) as u64,
                    )?;
                    source.push((value.clone(), domain.clone()));
                }
            }
            source
        };
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
        session.charge_host_work(1)?;
        if edges.is_empty() {
            return Ok(Vec::new());
        }
        let mut candidates = Vec::new();
        {
            let state = registry_lock(&self.state);
            let count = factory_candidate_prefix_len(session, &state.children)?;
            reserve_factory(session, &mut candidates, count)?;
            session.charge_host_work(2)?; // bounded prefix slice/iterator setup
            for child in &state.children[..count] {
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
            if factory_immediate(session, edge)? {
                continue;
            }
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

// Test-only baseline is the complete preimage staging function, name-only
// changed. It retains every historical fee/allocation and identical membership
// queries, so the measured comparison is not an estimate or mirrored formula.
#[cfg(test)]
impl WriteDomain {
    fn stage_factory_edges_full_scan_reference(
        &self,
        session: &mut Session,
        edges: &[SessionValue],
    ) -> Result<Vec<ChildDomain>> {
        session.charge_host_work((4 * std::mem::size_of::<Vec<FactoryOrigin>>() + 10) as u64)?;
        session.charge_host_work(1)?;
        if edges.is_empty() {
            return Ok(Vec::new());
        }
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
            if factory_immediate(session, edge)? {
                continue;
            }
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
    fn original_factory_seed_member_links_cannot_reexport_the_container_across_generations() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let recording = bridge.recording(&mut session).unwrap();
        let value = recording.map_values(&mut session, &[]).unwrap();
        let seed = finished(&mut session, recording, value);
        assert_eq!(seed.records.canonical_seed_fact_count_for_test(), 1);
        let meta = metadata(&mut session, &bridge);
        let base = symbol(&mut session, &bridge);
        let mut copies = Vec::new();
        // First copy has an explicit seed-root input. Its copied backing is
        // genuinely shared heap storage, not an equal-content replacement.
        // The second copy receives only the first copy plus the metadata.
        for _ in 0..2 {
            let parent = copies.last().unwrap_or(&seed);
            let origins = [origin(&mut session, parent), origin(&mut session, &meta)];
            let recording = bridge.recording(&mut session).unwrap();
            recording.bind_original_factory_inputs(&mut session, &origins).unwrap();
            let value = recording.with_form_metadata(&mut session,
                MetadataShape::Map { large: false }, &parent.value, &meta.value, &[]).unwrap();
            copies.push(finished(&mut session, recording, value));
        }
        let seed_fields = session.data_fields(&seed.value).unwrap();
        for copy in &copies {
            let fields = session.data_fields(&copy.value).unwrap();
            assert!(session.values_identical(&fields[2], &seed_fields[2]).unwrap(),
                "actual original backing must be shared across the real factory generations");
            let inherited = [origin(&mut session, &base), origin(&mut session, copy)];
            // Neither copy has a constructor edge to the original seed object.
            // Its exact backing link must not re-export that containing object.
            let wrong = wrap(&mut session, &bridge, &base, &seed, &inherited);
            hash(&mut session, &wrong.value);
            assert!(!watch(&wrong).matches(&mut session).unwrap(),
                "a terminal backing member cannot become whole-seed authority");
            // The metadata map IS an original downward input of each copy.
            let legitimate = wrap(&mut session, &bridge, &base, &meta, &inherited);
            hash(&mut session, &legitimate.value);
            session.collect().unwrap();
            assert!(watch(&legitimate).matches(&mut session).unwrap(),
                "restricting seed interiors must preserve actual downward input closure");
            // Explicitly offering the original seed still admits that exact
            // object. No mutation or initially-nil manufacture is involved.
            let explicit = [origin(&mut session, &base), origin(&mut session, copy),
                origin(&mut session, &seed)];
            let legitimate = wrap(&mut session, &bridge, &base, &seed, &explicit);
            hash(&mut session, &legitimate.value);
            assert!(watch(&legitimate).matches(&mut session).unwrap(),
                "the explicit seed-root positive must stay valid");
        }
    }
    #[test]
    fn original_factory_seed_node_descendants_survive_links_without_ancestor_or_sibling_authority() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let recording = bridge.recording(&mut session).unwrap();
        // Genuine private EMPTY_SET contains the original EMPTY_MAP Node;
        // that map in turn contains its original backing array Node.
        let value = recording.set_values(&mut session, &[]).unwrap();
        let seed = finished(&mut session, recording, value);
        assert_eq!(seed.records.canonical_seed_fact_count_for_test(), 1);
        let seed_fields = session.data_fields(&seed.value).unwrap();
        let inner_map = seed_fields[1].clone();
        let inner_fields = session.data_fields(&inner_map).unwrap();
        let meta = metadata(&mut session, &bridge);
        let mut copies = Vec::new();
        for _ in 0..2 {
            let parent = copies.last().unwrap_or(&seed);
            let inputs = [origin(&mut session, parent)];
            let recording = bridge.recording(&mut session).unwrap();
            recording.bind_original_factory_inputs(&mut session, &inputs).unwrap();
            let key = recording.scalar(&mut session,
                &Literal::String("original-member".encode_utf16().collect())).unwrap();
            // The ordinary array-map factory captures inner_map as an actual
            // argument. Its seed-member Node scope must survive this link and
            // the next generation, rather than becoming either terminal-only
            // or permission for the entire EMPTY_SET snapshot.
            let value = recording.map_values(&mut session, &[(key, inner_map.clone())]).unwrap();
            copies.push(finished(&mut session, recording, value));
        }
        for copy in &copies {
            let inputs = [origin(&mut session, copy), origin(&mut session, &meta)];
            let recording = bridge.recording(&mut session).unwrap();
            recording.bind_original_factory_inputs(&mut session, &inputs).unwrap();
            // This supported metadata factory needs the backing BELOW the
            // admitted inner-map Node, not just that Node's exact identity.
            let value = recording.with_form_metadata(&mut session,
                MetadataShape::Map { large: false }, &inner_map, &meta.value, &[]).unwrap();
            let descendant = finished(&mut session, recording, value);
            let fields = session.data_fields(&descendant.value).unwrap();
            assert!(session.values_identical(&fields[2], &inner_fields[2]).unwrap());
            session.collect().unwrap();
            assert!(complete_domain(&mut session, &descendant),
                "genuine original Node descendants must remain selectable through both generations");
            // The parent seed object itself was not an argument of either map.
            // Store it as an ordinary map VALUE, preserving genuine factory
            // semantics rather than putting a set in a metadata-map position.
            let recording = bridge.recording(&mut session).unwrap();
            let original_inputs = [origin(&mut session, copy)];
            recording.bind_original_factory_inputs(&mut session, &original_inputs).unwrap();
            let key = recording.scalar(&mut session,
                &Literal::String("not-an-ancestor".encode_utf16().collect())).unwrap();
            let value = recording.map_values(&mut session, &[(key, seed.value.clone())]).unwrap();
            let ancestor = finished(&mut session, recording, value);
            assert!(!complete_domain(&mut session, &ancestor),
                "original downward membership must not recover its containing seed");
            // A separately allocated equal-content map is neither a descendant
            // nor a permitted seed sibling, even while all owners remain live.
            let other = metadata(&mut session, &bridge);
            assert!(!session.values_identical(&other.value, &meta.value).unwrap());
            let recording = bridge.recording(&mut session).unwrap();
            let inputs = [origin(&mut session, copy)];
            recording.bind_original_factory_inputs(&mut session, &inputs).unwrap();
            let value = recording.with_form_metadata(&mut session,
                MetadataShape::Map { large: false }, &inner_map, &other.value, &[]).unwrap();
            let unrelated = finished(&mut session, recording, value);
            assert!(!complete_domain(&mut session, &unrelated),
                "an unrelated same-content source object cannot enter the original subtree");
        }
    }
    fn complete_domain(session: &mut Session, owner: &Owner) -> bool {
        let domain = owner.records.write_domain();
        let watch = watch(owner);
        let methods = session.canonical_methods.clone().unwrap();
        let prototype = session.prototype_origin.clone().unwrap();
        let irq = session.host_work_interrupt();
        let runtime = session.runtime;
        let identity = session.identity;
        domain.matches(session.store.as_context_mut(), runtime, identity, &irq,
            &methods, &prototype, None, &watch, &[]).unwrap()
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
    // Real factories install exact candidate offers and subsequently append
    // FactoryReference links. No source certificate or synthetic seed is made.
    fn prefix_recording(
        session: &mut Session, bridge: &FormBridge,
    ) -> (Owner, Owner, Owner) {
        let base = symbol(session, bridge);
        let meta = metadata(session, bridge);
        let origins = [origin(session, &base), origin(session, &meta)];
        let recording = bridge.recording(session).unwrap();
        recording.bind_original_factory_inputs(session, &origins).unwrap();
        let mut last = None;
        for _ in 0..3 {
            last = Some(recording.with_form_metadata(
                session, MetadataShape::Symbol, &base.value, &meta.value, &[],
            ).unwrap());
        }
        let owner = Owner {
            value: last.unwrap(),
            records: recording.into_records().unwrap(),
        };
        (base, meta, owner)
    }

    #[test]
    fn factory_candidate_prefix_preserves_real_origins_with_reference_suffix_and_zero_candidates() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let (base, meta, owner) = prefix_recording(&mut session, &bridge);
        let domain = owner.records.write_domain();
        let operation = session.operation_budget_scope().unwrap();
        {
            let state = registry_lock(&domain.state);
            assert!(!state.sealed);
            assert!(state.children.len() > 2);
            assert_eq!(factory_candidate_prefix_len(&mut session, &state.children).unwrap(), 2);
            for (child, expected) in state.children[..2].iter().zip([&base.value, &meta.value]) {
                let ChildDomain::FactoryCandidate(origin) = child else { panic!("missing original offer") };
                assert!(session.values_identical(&origin.root, expected).unwrap());
            }
            assert!(state.children[2..].iter().all(|child|
                matches!(child, ChildDomain::FactoryReference { .. })));
        }
        // Stage the genuine original metadata edge through unchanged rooted
        // membership. Offers must neither disappear nor broaden to a ledger.
        let links = domain.stage_factory_edges(&mut session, std::slice::from_ref(&meta.value)).unwrap();
        assert_eq!(links.len(), 1);
        let ChildDomain::FactoryReference { value, owner: selected, .. } = &links[0]
            else { panic!("missing proved edge") };
        assert!(session.values_identical(value, &meta.value).unwrap());
        assert!(selected.member.is_none());
        assert!(Arc::ptr_eq(&selected.domain.upgrade().unwrap(), &meta.records.write_domain()));
        drop(links);
        // Sealing genuinely removes the offers but leaves proved references.
        owner.records.seal_source_domain(&mut session).unwrap();
        {
            let state = registry_lock(&domain.state);
            assert!(!state.children.is_empty());
            assert_eq!(factory_candidate_prefix_len(&mut session, &state.children).unwrap(), 0);
        }
        assert!(domain.stage_factory_edges(&mut session, std::slice::from_ref(&meta.value)).unwrap().is_empty());
        let empty = WriteDomain::new();
        assert!(empty.stage_factory_edges(&mut session, std::slice::from_ref(&meta.value)).unwrap().is_empty());
        hash(&mut session, &owner.value);
        assert!(watch(&owner).matches(&mut session).unwrap());
        drop(operation);
    }

    #[test]
    fn factory_candidate_prefix_staging_fuel_irq_failures_leave_domain_unchanged_and_retry() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let (_base, meta, owner) = prefix_recording(&mut session, &bridge);
        let domain = owner.records.write_domain();
        let before = {
            let state = registry_lock(&domain.state);
            (state.nodes.len(), state.children.len(), state.sealed, state.valid)
        };
        let used = {
            let operation = session.operation_budget_scope().unwrap();
            let start = session.operation_fuel_remaining().unwrap();
            let links = domain.stage_factory_edges(&mut session, std::slice::from_ref(&meta.value)).unwrap();
            assert_eq!(links.len(), 1);
            let used = start - session.operation_fuel_remaining().unwrap();
            drop(links);
            drop(operation);
            used
        };
        assert!(used > 2);
        for (fuel, irq) in [(0, false), (1, false), (used / 2, false), (used, true)] {
            let operation = session.operation_budget_scope().unwrap();
            session.store.set_fuel(fuel).unwrap();
            if irq { session.host_work_interrupt().store(true, std::sync::atomic::Ordering::SeqCst); }
            let error = match domain.stage_factory_edges(&mut session, std::slice::from_ref(&meta.value)) {
                Err(error) => error,
                Ok(_) => panic!("staging unexpectedly passed a failing cut"),
            };
            if irq {
                assert!(error.is_interrupt(), "{error:?}");
            } else {
                assert!(matches!(error, SessionError::Trap(ref trap)
                    if trap.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
            }
            assert_eq!(session.operation_fuel_remaining().unwrap(), 0);
            drop(operation);
            {
                let state = registry_lock(&domain.state);
                assert_eq!((state.nodes.len(), state.children.len(), state.sealed, state.valid), before);
            }
            // A genuine fresh operation, same domain and inputs; no explicit
            // refill inside the failed scope or partial link publication.
            let operation = session.operation_budget_scope().unwrap();
            let links = domain.stage_factory_edges(&mut session, std::slice::from_ref(&meta.value)).unwrap();
            assert_eq!(links.len(), 1);
            drop(links);
            drop(operation);
        }
        owner.records.seal_source_domain(&mut session).unwrap();
        hash(&mut session, &owner.value);
        assert!(watch(&owner).matches(&mut session).unwrap());
    }

    #[test]
    fn factory_candidate_prefix_actual_charges_match_old_membership_and_reduce_large_suffix_work() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let base = symbol(&mut session, &bridge);
        let meta = metadata(&mut session, &bridge);
        let origins = [origin(&mut session, &base), origin(&mut session, &meta)];
        let recording = bridge.recording(&mut session).unwrap();
        recording.bind_original_factory_inputs(&mut session, &origins).unwrap();
        let mut last = None;
        // Genuine completed factories append their actual proved references.
        // No dummy Source/FactoryReference link or authority is fabricated.
        for _ in 0..64 {
            last = Some(recording.with_form_metadata(
                &mut session, MetadataShape::Symbol, &base.value, &meta.value, &[],
            ).unwrap());
        }
        let owner = Owner { value: last.unwrap(), records: recording.into_records().unwrap() };
        let domain = owner.records.write_domain();
        let state_before = {
            let operation = session.operation_budget_scope().unwrap();
            let state = registry_lock(&domain.state);
            assert_eq!(factory_candidate_prefix_len(&mut session, &state.children).unwrap(), 2);
            assert!(state.children.len() >= 66, "expected a genuinely large reference suffix");
            for (child, expected) in state.children[..2].iter().zip([&base.value, &meta.value]) {
                let ChildDomain::FactoryCandidate(origin) = child else { panic!("missing original offer") };
                assert!(session.values_identical(&origin.root, expected).unwrap());
            }
            assert!(state.children[2..].iter().all(|child|
                matches!(child, ChildDomain::FactoryReference { .. })));
            let counts = (state.nodes.len(), state.children.len(), state.sealed, state.valid);
            drop(state);
            drop(operation);
            counts
        };
        fn measured(
            session: &mut Session, domain: &WriteDomain, edge: &SessionValue, reference: bool,
        ) -> (Vec<ChildDomain>, u64) {
            let operation = session.operation_budget_scope().unwrap();
            let start = session.operation_fuel_remaining().unwrap();
            let links = if reference {
                domain.stage_factory_edges_full_scan_reference(session, std::slice::from_ref(edge))
            } else {
                domain.stage_factory_edges(session, std::slice::from_ref(edge))
            }.unwrap();
            let used = start - session.operation_fuel_remaining().unwrap();
            drop(operation);
            (links, used)
        }
        session.collect().unwrap();
        // Same exact domain/prefix/edge, in both orders; never publish either
        // staging result between measurements or infer a cold/warm cache gain.
        for reference_first in [true, false] {
            let (old, new) = if reference_first {
                let old = measured(&mut session, &domain, &meta.value, true);
                let new = measured(&mut session, &domain, &meta.value, false);
                (old, new)
            } else {
                let new = measured(&mut session, &domain, &meta.value, false);
                let old = measured(&mut session, &domain, &meta.value, true);
                (old, new)
            };
            assert_eq!(old.0.len(), 1);
            assert_eq!(new.0.len(), 1);
            for links in [&old.0, &new.0] {
                let ChildDomain::FactoryReference { value, origin, owner: selected } = &links[0]
                    else { panic!("missing exact proved metadata link") };
                assert!(session.values_identical(value, &meta.value).unwrap());
                assert!(session.values_identical(&origin.root, &meta.value).unwrap());
                assert!(selected.member.is_none());
                assert!(Arc::ptr_eq(&selected.domain.upgrade().unwrap(), &meta.records.write_domain()));
            }
            let (ChildDomain::FactoryReference { owner: old_owner, .. },
                 ChildDomain::FactoryReference { owner: new_owner, .. }) = (&old.0[0], &new.0[0])
                else { panic!("expected two exact references") };
            assert_eq!(old_owner.node, new_owner.node);
            assert!(std::sync::Weak::ptr_eq(&old_owner.domain, &new_owner.domain));
            eprintln!("factory-candidate-prefix reference_first={reference_first} candidates=2 suffix={} old={} new={}",
                state_before.1 - 2, old.1, new.1);
            assert!(new.1 < old.1, "large-suffix actual staging work should decrease");
            let state = registry_lock(&domain.state);
            assert_eq!((state.nodes.len(), state.children.len(), state.sealed, state.valid), state_before);
        }
        owner.records.seal_source_domain(&mut session).unwrap();
        hash(&mut session, &owner.value);
        assert!(watch(&owner).matches(&mut session).unwrap());
    }

    // Test-only integration entrypoint: the source_roots test must obtain its
    // CheckedChild through full Certificate finish/checked on real source. This
    // method cannot construct a certificate or an unchecked Source link.
    impl WriteDomain {
        pub(crate) fn test_candidate_prefix_with_checked_source(
            session: &mut Session,
            bridge: &FormBridge,
            child: &crate::portable_macro_graph::source_roots::CheckedChild,
        ) {
            let (_base, meta, owner) = prefix_recording(session, bridge);
            let domain = owner.records.write_domain();
            let operation = session.operation_budget_scope().unwrap();
            // Genuine source enrollment uses the production attachment guards.
            // The existing recording already has candidate + reference suffix.
            domain.attach_child(session, child).unwrap();
            {
                let state = registry_lock(&domain.state);
                assert_eq!(factory_candidate_prefix_len(session, &state.children).unwrap(), 2);
                assert!(state.children[2..].iter().any(|link|
                    matches!(link, ChildDomain::FactoryReference { .. })));
                let mut sources = 0;
                for link in &state.children[2..] {
                    if let ChildDomain::Source { value, domain } = link {
                        sources += 1;
                        assert!(session.values_identical(value, child.value()).unwrap());
                        assert!(Arc::ptr_eq(domain, &child.domain()));
                    }
                }
                assert_eq!(sources, 1);
            }
            let old = domain.stage_factory_edges_full_scan_reference(session, std::slice::from_ref(&meta.value)).unwrap();
            let new = domain.stage_factory_edges(session, std::slice::from_ref(&meta.value)).unwrap();
            assert_eq!(old.len(), 1);
            assert_eq!(new.len(), 1);
            for links in [&old, &new] {
                let ChildDomain::FactoryReference { value, owner: selected, .. } = &links[0]
                    else { panic!("lost original metadata edge") };
                assert!(session.values_identical(value, &meta.value).unwrap());
                assert!(selected.member.is_none());
                assert!(Arc::ptr_eq(&selected.domain.upgrade().unwrap(), &meta.records.write_domain()));
            }
            drop((old, new));
            // Separate unsealed Source-only domain: no offer pool ever existed,
            // so Source must never be mistaken for a FactoryCandidate.
            let source_only = WriteDomain::new();
            source_only.attach_child(session, child).unwrap();
            {
                let state = registry_lock(&source_only.state);
                assert!(!state.sealed);
                assert!(state.nodes.is_empty());
                assert_eq!(state.children.len(), 1);
                assert!(matches!(state.children[0], ChildDomain::Source { .. }));
                assert_eq!(factory_candidate_prefix_len(session, &state.children).unwrap(), 0);
            }
            assert!(source_only.stage_factory_edges_full_scan_reference(
                session, std::slice::from_ref(child.value()),
            ).unwrap().is_empty());
            assert!(source_only.stage_factory_edges(
                session, std::slice::from_ref(child.value()),
            ).unwrap().is_empty());
            owner.records.seal_source_domain(session).unwrap();
            {
                let state = registry_lock(&domain.state);
                assert_eq!(factory_candidate_prefix_len(session, &state.children).unwrap(), 0);
                assert!(state.children.iter().any(|link| matches!(link, ChildDomain::Source { .. })));
                assert!(state.children.iter().any(|link| matches!(link, ChildDomain::FactoryReference { .. })));
            }
            hash(session, &owner.value);
            assert!(watch(&owner).matches(session).unwrap());
            drop(operation);
        }
    }

}
