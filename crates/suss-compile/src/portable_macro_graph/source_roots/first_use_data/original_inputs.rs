//! Missing DATA input edges may refer only to original, already healthy
//! descendants of THIS native node. No guest fields are read into expectations.
//! This borrow lives for one callback-free check; never retained in any cache.
use super::*;

fn reserve_original_refs<T>(session: &mut Session, values: &mut Vec<T>, extra: usize) -> Result<()> {
    session.charge_host_work(6)?; // extent/capacity/retirement planning controls
    let needed = values.len().checked_add(extra)
        .ok_or_else(|| invalid("Original DATA reference extent overflow"))?;
    if needed > values.capacity() {
        let next = values.capacity().checked_mul(2).unwrap_or(needed).max(needed);
        let retirement = next.checked_mul(std::mem::size_of::<T>())
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| invalid("Original DATA reference retirement overflow"))?;
        session.charge_host_work(retirement)?;
    }
    // Existing generic prepayment covers requested backing and relocation;
    // above additionally reserves retirement of each acquired backing.
    reserve(session, values, extra)
}
fn append_original_refs<T>(session: &mut Session, values: &mut Vec<T>, entries: impl ExactSizeIterator<Item=T>) -> Result<()> {
    session.charge_host_work(4)?;
    let count = entries.len();
    let writes = u64::try_from(count).ok().and_then(|n| n.checked_mul(3))
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| invalid("Original DATA reference writes overflow"))?;
    reserve_original_refs(session, values, count)?;
    session.charge_host_work(writes)?;
    values.extend(entries);
    Ok(())
}

pub(crate) struct OriginalDataInputs<'a> {
    inputs: &'a [Evidence],
    current: &'a ConstructionRecords,
    checked: &'a mut crate::portable::metered_map::TryHashMap<(u8, usize), bool>,
    ledgers: Option<Vec<&'a ConstructionRecords>>,
    unavailable: bool,
}
impl<'a> OriginalDataInputs<'a> {
    // Private outside first_use_data; callers cannot supply arbitrary ledgers,
    // claim a healthy bit, or create strict/memo/write authority.
    pub(super) fn new(
        session: &mut Session, inputs: &'a [Evidence],
        checked: &'a mut crate::portable::metered_map::TryHashMap<(u8, usize), bool>, current: &'a ConstructionRecords,
    ) -> Result<Self> {
        session.charge_host_work((2 * std::mem::size_of::<Self>() + 8) as u64)?;
        Ok(Self { inputs, current, checked, ledgers: None, unavailable: false })
    }
    // Only narrows which native parent may use this original-input borrow.
    // The caller prepays this exact pointer comparison; it proves no health.
    pub(crate) fn belongs_to(&self, current: &ConstructionRecords) -> bool {
        std::ptr::eq(self.current, current)
    }
    fn prepare(&mut self, session: &mut Session) -> Result<bool> {
        session.charge_host_work(4)?;
        if self.unavailable { return Ok(false); }
        if self.ledgers.is_some() { return Ok(true); }
        // Prepay all transient headers/init/move/retirement before acquisition.
        session.charge_host_work((
            2 * std::mem::size_of::<Vec<&Evidence>>()
            + 2 * std::mem::size_of::<crate::portable::metered_map::TryHashMap<(u8, usize), bool>>()
            + 2 * std::mem::size_of::<Vec<&ConstructionRecords>>() + 8
        ) as u64)?;
        let mut pending: Vec<&'a Evidence> = Vec::new();
        let mut seen = TryOrderedMap::<(u8, usize), bool>::default();
        let mut ledgers: Vec<&'a ConstructionRecords> = Vec::new();
        let inputs: &'a [Evidence] = self.inputs;
        append_original_refs(session, &mut pending, inputs.iter().rev())?;
        while let Some(evidence) = pending.pop() {
            session.charge_host_work(4)?; // pop, identity, branch and loop controls
            let key = evidence.identity();
            // Reachability alone is insufficient. False or unchecked nodes may
            // not lend original references, even if a sibling was audited.
            if self.checked.get(&key, &mut Meter(session))?.copied() != Some(true) {
                session.charge_host_work(1)?;
                self.unavailable = true;
                return Ok(false);
            }
            if seen.get(&key, &mut Meter(session))?.is_some() { continue; }
            seen.insert(key, true, &mut Meter(session))?;
            match evidence {
                Evidence::Native(node) => {
                    reserve_original_refs(session, &mut ledgers, 1)?;
                    session.charge_host_work(3)?; // original borrow/write/retirement
                    ledgers.push(&node.records);
                    append_original_refs(session, &mut pending, node.inputs.iter().rev())?;
                }
                Evidence::Source(root) => {
                    append_original_refs(session, &mut pending, std::iter::once(root.actual()))?;
                }
            }
        }
        session.charge_host_work(4)?; // publish borrowed local inventory, no roots
        self.ledgers = Some(ledgers);
        Ok(true)
    }
    pub(crate) fn contains_original(
        &mut self, session: &mut Session, value: &SessionValue,
    ) -> Result<bool> {
        if !self.prepare(session)? { return Ok(false); }
        // Linear scans avoid allocating another owner/reference table. They run
        // only for edges missing from local records and direct child roots.
        for ledger in self.ledgers.as_ref().unwrap() {
            session.charge_host_work(3)?;
            if ledger.contains_original_data_reference(session, value)? { return Ok(true); }
        }
        Ok(false)
    }
}
