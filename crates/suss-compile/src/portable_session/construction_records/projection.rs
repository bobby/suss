//! Closed views of ONE original factory ledger. No factory is reconstructed and
//! no watch is rebound to another domain. This is storage/construction evidence,
//! not source authority: only the graph's bound-source path can certify a view.
use super::*;
use std::rc::Rc;

pub(crate) struct FactoryProjection {
    ledger: Rc<ConstructionRecords>,
    selected: Vec<usize>,
    root: SessionValue,
}
fn invalid(message: &'static str) -> SessionError {
    SessionError::Host(wasmtime::Error::msg(message))
}
fn reserve<T>(
    session: &mut Session,
    values: &mut Vec<T>,
    extra: usize,
) -> Result<(), SessionError> {
    session.charge_host_work(1)?;
    if extra <= values.capacity() - values.len() {
        return Ok(());
    }
    let needed = values
        .len()
        .checked_add(extra)
        .ok_or_else(|| invalid("Factory projection overflow"))?;
    let capacity = values
        .capacity()
        .checked_mul(2)
        .unwrap_or(needed)
        .max(needed);
    let bytes = capacity
        .checked_add(values.len())
        .and_then(|n| n.checked_mul(std::mem::size_of::<T>()))
        .filter(|n| *n <= isize::MAX as usize)
        .ok_or_else(|| invalid("Factory projection allocation overflow"))?;
    session.charge_host_work(bytes as u64)?;
    values
        .try_reserve_exact(capacity - values.len())
        .map_err(|_| invalid("Factory projection allocation failed"))?;
    Ok(())
}
impl FactoryProjection {
    pub(crate) fn from_recorded_root(
        session: &mut Session,
        ledger: &Rc<ConstructionRecords>,
        root: &SessionValue,
    ) -> Result<Option<Self>, SessionError> {
        let _request = session.operation_budget_scope()?;
        session.check(root)?;
        let mut selected = Vec::new();
        reserve(session, &mut selected, ledger.records.len())?;
        session.charge_host_work(ledger.records.len() as u64)?;
        selected.resize(ledger.records.len(), false);
        let mut pending = Vec::new();
        reserve(session, &mut pending, 1)?;
        pending.push(root);
        let mut meter = InputIndex {
            session,
            hash_mask: u64::MAX,
        };
        let mut heads = InputHeads::new(ledger.records.len(), &mut meter)?;
        let mut collisions = Vec::new();
        reserve(meter.session, &mut collisions, ledger.records.len())?;
        for (id, record) in ledger.records.iter().enumerate() {
            let hash = meter.hash(&record.value)?;
            let previous = heads.insert(hash, id, &mut meter)?;
            collisions.push(previous);
        }
        let mut valid = true;
        let mut first = true;
        // These are immutable NATIVE expected-input references, never edges read
        // from current guest storage. No guest allocation runs with heads live.
        while let Some(value) = pending.pop() {
            meter.charge(2)?;
            let hash = meter.hash(value)?;
            let mut cursor = heads.get(hash, &mut meter)?;
            let mut found = false;
            while let Some(id) = cursor {
                meter.charge(2)?;
                let record = &ledger.records[id];
                if meter.same(value, &record.value)? {
                    found = true;
                    if !selected[id] {
                        selected[id] = true;
                        match &record.fact {
                            Fact::Array(fact) => {
                                for input in fact.inputs() {
                                    reserve(meter.session, &mut pending, 1)?;
                                    meter.charge(1)?;
                                    pending.push(input);
                                }
                            }
                            Fact::Constructor(fact) => {
                                for input in fact.inputs() {
                                    reserve(meter.session, &mut pending, 1)?;
                                    meter.charge(1)?;
                                    pending.push(input);
                                }
                            }
                            Fact::ExternalSeed(_) => valid = false,
                            Fact::Scalar(_) | Fact::CanonicalSeed(_) => {}
                        }
                    }
                }
                cursor = collisions[id];
            }
            // The selected root must have its OWN recorded factory. Only edges
            // can terminate at an authenticated child or pristine initial seed.
            if !found && !first {
                for (id, record) in ledger.records.iter().enumerate() {
                    meter.charge(1)?;
                    if let Fact::CanonicalSeed(seed) = &record.fact {
                        if seed.contains_initial_reference(meter.session, value)? {
                            selected[id] = true;
                            found = true;
                            break;
                        }
                    }
                }
                if !found {
                    for child in &ledger.children {
                        meter.charge(1)?;
                        if meter.same(value, &child.value)? {
                            found = true;
                            break;
                        }
                    }
                }
            }
            valid &= found;
            first = false;
        }
        if !valid {
            return Ok(None);
        }
        drop(heads);
        let mut ids = Vec::new();
        reserve(meter.session, &mut ids, selected.len())?;
        for (id, chosen) in selected.into_iter().enumerate() {
            meter.charge(1)?;
            if chosen {
                ids.push(id);
            }
        }
        meter.charge(1)?;
        Ok(Some(Self {
            ledger: ledger.clone(),
            selected: ids,
            root: root.clone(),
        }))
    }
    pub(crate) fn write_domain(&self) -> std::sync::Arc<super::super::write_domain::WriteDomain> {
        self.ledger.write_domain()
    }
    pub(crate) fn seal_source_domain(&self, session: &mut Session) -> Result<(), SessionError> {
        self.ledger.seal_source_domain(session)
    }
    pub(crate) fn matches_factory_facts(
        &self,
        session: &mut Session,
        root: &SessionValue,
        audit: &mut super::super::certification_audit::CertificationAudit,
    ) -> Result<bool, SessionError> {
        let _request = session.operation_budget_scope()?;
        session.check(root)?;
        let mut meter = InputIndex {
            session,
            hash_mask: u64::MAX,
        };
        let mut valid = meter.same(root, &self.root)?;
        valid &= self
            .ledger
            .matches_class_schemas_audited(meter.session, audit)?;
        let session = &mut *meter.session;
        for id in &self.selected {
            session.charge_host_work(2)?;
            let record = &self.ledger.records[*id];
            session.charge_host_work(1)?;
            valid &= match &record.fact {
                Fact::Scalar(fact) => fact.matches(session, &record.value)?,
                Fact::Array(fact) => fact.matches(session, &record.value)?,
                Fact::Constructor(fact) => {
                    let direct = fact.matches(session, &record.value)?;
                    let origin = fact.matches_prototype_origin_audited(session, audit)?;
                    direct && origin
                }
                Fact::CanonicalSeed(fact) => fact.matches(session, &record.value)?,
                Fact::ExternalSeed(_) => false,
            };
        }
        // Closure is an immutable native proof built above, not a cached guest
        // validation result. Every selected current payload and pristine seed is
        // still checked. The owning source certificate separately validates ALL
        // typed external children after callbacks; this view cannot authorize one.
        session.charge_host_work(1)?;
        Ok(valid)
    }
}
