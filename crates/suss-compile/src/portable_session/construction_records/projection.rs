//! Closed views of ONE original factory ledger. No factory is reconstructed and
//! no watch is rebound to another domain. This is storage/construction evidence,
//! not source authority. In particular, copied fields do NOT retain their read
//! receiver in this physical closure. Source integration must separately prove
//! causal reads/occurrences and exact parent source edges before certification.
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
        Self::from_recorded_root_with_hash(session, ledger, root, u64::MAX)
    }
    fn from_recorded_root_with_hash(
        session: &mut Session,
        ledger: &Rc<ConstructionRecords>,
        root: &SessionValue,
        hash_mask: u64,
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
        let mut meter = InputIndex { session, hash_mask };
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
        meter.charge(2)?; // ledger Rc and managed root clones
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

#[cfg(test)]
mod tests {
    use super::super::super::certification_audit::CertificationAudit;
    use super::*;
    use crate::{portable::hir::Literal, portable_macro_data::FormBridge};

    fn matches(s: &mut Session, view: &FactoryProjection, root: &SessionValue) -> bool {
        let mut audit = CertificationAudit::new(s).unwrap();
        view.matches_factory_facts(s, root, &mut audit).unwrap()
    }
    #[test]
    fn factory_projection_preserves_shared_aliases_and_closes_exact_native_inputs_under_collisions()
    {
        let mut s = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut s).unwrap();
        let recording = bridge.recording(&mut s).unwrap();
        let _request = s.operation_budget_scope().unwrap();
        let leaf = recording.scalar(&mut s, &Literal::Number(17.0)).unwrap();
        let shared = recording.vector_values(&mut s, &[leaf]).unwrap();
        let a = recording
            .vector_values(&mut s, &[shared.clone(), shared.clone()])
            .unwrap();
        let b = recording.vector_values(&mut s, &[shared.clone()]).unwrap();
        let equal_b = recording.vector_values(&mut s, &[shared.clone()]).unwrap();
        let unrelated = recording.vector_values(&mut s, &[]).unwrap();
        let ledger = Rc::new(recording.into_records().unwrap());
        let a_view = FactoryProjection::from_recorded_root_with_hash(&mut s, &ledger, &a, 0)
            .unwrap()
            .unwrap();
        let b_view = FactoryProjection::from_recorded_root(&mut s, &ledger, &b)
            .unwrap()
            .unwrap();
        s.collect().unwrap();
        let inspect = s
            .eval("(fn [a b] [(identical? (nth a 0) (nth a 1)) (identical? (nth a 0) (nth b 0))])")
            .unwrap();
        let result = s.invoke(&inspect, &[&a, &b]).unwrap();
        assert_eq!(
            crate::portable_repl::display(&mut s, &result).unwrap(),
            "[true true]"
        );
        assert!(matches(&mut s, &a_view, &a));
        assert!(matches(&mut s, &b_view, &b));
        assert!(
            !matches(&mut s, &b_view, &equal_b),
            "a closed view binds the exact root, not equal contents"
        );
        let poison = s.eval("(fn [v] (set! (.-cnt v) 99))").unwrap();
        drop(s.invoke(&poison, &[&unrelated]).unwrap());
        assert!(!ledger.matches_direct_facts(&mut s).unwrap());
        assert!(
            matches(&mut s, &a_view, &a),
            "unrelated factory payload is not a root input"
        );
        assert!(matches(&mut s, &b_view, &b));
        drop(s.invoke(&poison, &[&shared]).unwrap());
        assert!(!matches(&mut s, &a_view, &a));
        assert!(!matches(&mut s, &b_view, &b));
    }
    #[test]
    fn factory_projection_rejects_unrecorded_and_open_roots_and_recovers_after_fuel_irq() {
        let mut s = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut s).unwrap();
        let recording = bridge.recording(&mut s).unwrap();
        let op = s.operation_budget_scope().unwrap();
        let leaf = recording.scalar(&mut s, &Literal::Number(17.0)).unwrap();
        let root = recording.vector_values(&mut s, &[leaf]).unwrap();
        let unknown = s.eval("[17]").unwrap();
        let ledger = Rc::new(recording.into_records().unwrap());
        assert!(
            FactoryProjection::from_recorded_root(&mut s, &ledger, &unknown)
                .unwrap()
                .is_none()
        );
        let open = bridge.recording(&mut s).unwrap();
        let open_root = open.vector_values(&mut s, &[unknown]).unwrap();
        let open_ledger = Rc::new(open.into_records().unwrap());
        assert!(
            FactoryProjection::from_recorded_root(&mut s, &open_ledger, &open_root)
                .unwrap()
                .is_none()
        );
        drop(op);
        let strong = Rc::strong_count(&ledger);
        s.set_operation_fuel(1);
        assert!(FactoryProjection::from_recorded_root(&mut s, &ledger, &root).is_err());
        assert_eq!(Rc::strong_count(&ledger), strong);
        s.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        let op = s.operation_budget_scope().unwrap();
        s.interrupt_handle().interrupt();
        assert!(
            FactoryProjection::from_recorded_root(&mut s, &ledger, &root)
                .err()
                .unwrap()
                .is_interrupt()
        );
        assert_eq!(Rc::strong_count(&ledger), strong);
        drop(op);
        let _op = s.operation_budget_scope().unwrap();
        let view = FactoryProjection::from_recorded_root(&mut s, &ledger, &root)
            .unwrap()
            .unwrap();
        assert!(matches(&mut s, &view, &root));
        let mut foreign = Session::new_macro().unwrap();
        assert!(matches!(
            FactoryProjection::from_recorded_root(&mut foreign, &ledger, &root),
            Err(SessionError::ForeignValue)
        ));
        let mut audit = CertificationAudit::new(&mut foreign).unwrap();
        assert!(matches!(
            view.matches_factory_facts(&mut foreign, &root, &mut audit),
            Err(SessionError::ForeignValue)
        ));
        assert_eq!(Rc::strong_count(&ledger), strong + 1);
    }
    #[test]
    fn physical_factory_projection_is_not_source_authority_for_copied_metadata_memo() {
        use crate::portable_macro_data::MetadataShape;
        let mut s = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut s).unwrap();
        let recording = bridge.recording(&mut s).unwrap();
        let _request = s.operation_budget_scope().unwrap();
        let leaf = recording.scalar(&mut s, &Literal::Number(17.0)).unwrap();
        let base = recording.vector_values(&mut s, &[leaf]).unwrap();
        let forged = recording.scalar(&mut s, &Literal::Number(991.0)).unwrap();
        // Recorded numeric input membership cannot authenticate a generic memo
        // write. In particular, copying it cannot turn it into source authority.
        let poison = s
            .eval("(fn [base value] (set! (.-__hash base) value))")
            .unwrap();
        drop(s.invoke(&poison, &[&base, &forged]).unwrap());
        let metadata = recording.map_values(&mut s, &[]).unwrap();
        let copied = recording
            .with_form_metadata(&mut s, MetadataShape::Vector, &base, &metadata, &[])
            .unwrap();
        let ledger = Rc::new(recording.into_records().unwrap());
        assert!(
            !ledger.matches_direct_facts(&mut s).unwrap(),
            "the original nil watch must reject the generic forged write"
        );
        let view = FactoryProjection::from_recorded_root(&mut s, &ledger, &copied)
            .unwrap()
            .unwrap();
        // This affirmative result documents the LIMITED factory-view contract;
        // it is the negative control for any future source integration. A bound
        // source certificate MUST also import the inspected base's causal proof.
        assert!(matches(&mut s, &view, &copied));
        assert!(
            ledger.records.iter().enumerate().any(|(id, record)| {
                let mut meter = InputIndex {
                    session: &mut s,
                    hash_mask: u64::MAX,
                };
                meter.same(&record.value, &base).unwrap() && !view.selected.contains(&id)
            }),
            "the inspected base is absent from the physical input closure"
        );
    }
    #[test]
    fn factory_projection_checks_complete_pristine_seed_snapshot() {
        let mut s = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut s).unwrap();
        let recording = bridge.recording(&mut s).unwrap();
        let _request = s.operation_budget_scope().unwrap();
        let seed = recording.map_values(&mut s, &[]).unwrap();
        let ledger = Rc::new(recording.into_records().unwrap());
        let view = FactoryProjection::from_recorded_root(&mut s, &ledger, &seed)
            .unwrap()
            .unwrap();
        assert!(matches(&mut s, &view, &seed));
        let poison = s.eval("(fn [m] (set! (.-__hash m) 991))").unwrap();
        drop(s.invoke(&poison, &[&seed]).unwrap());
        assert!(
            !matches(&mut s, &view, &seed),
            "seed membership cannot replace its complete original snapshot check"
        );
        // An isolated Session owns this mutation; no restoration is claimed.
    }
    #[test]
    fn factory_projection_every_insufficient_constructor_allowance_keeps_ledger_and_recovers() {
        let mut s = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut s).unwrap();
        let recording = bridge.recording(&mut s).unwrap();
        let op = s.operation_budget_scope().unwrap();
        let leaf = recording.scalar(&mut s, &Literal::Number(17.0)).unwrap();
        let root = recording.vector_values(&mut s, &[leaf]).unwrap();
        let ledger = Rc::new(recording.into_records().unwrap());
        drop(op);
        let op = s.operation_budget_scope().unwrap();
        let before = s.operation_fuel_remaining().unwrap();
        let measured = FactoryProjection::from_recorded_root_with_hash(&mut s, &ledger, &root, 0)
            .unwrap()
            .unwrap();
        let cost = before - s.operation_fuel_remaining().unwrap();
        let expected = measured.selected.clone();
        drop(measured);
        drop(op);
        assert!(cost > 0);
        let strong = Rc::strong_count(&ledger);
        let records = ledger.len();
        for allowance in 0..cost {
            s.set_operation_fuel(allowance);
            let result = FactoryProjection::from_recorded_root_with_hash(&mut s, &ledger, &root, 0);
            assert!(
                matches!(result, Err(SessionError::Trap(ref error))
                if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
                "allowance {allowance} of measured {cost}"
            );
            assert_eq!(Rc::strong_count(&ledger), strong, "allowance {allowance}");
            assert_eq!(ledger.len(), records);
        }
        s.set_operation_fuel(cost);
        let recovered = FactoryProjection::from_recorded_root_with_hash(&mut s, &ledger, &root, 0)
            .unwrap()
            .unwrap();
        assert_eq!(recovered.selected, expected);
        s.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        let _op = s.operation_budget_scope().unwrap();
        assert!(ledger.matches_direct_facts(&mut s).unwrap());
        assert!(matches(&mut s, &recovered, &root));
    }
    #[test]
    fn factory_projection_false_root_still_propagates_later_fuel_error() {
        let mut s = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut s).unwrap();
        let recording = bridge.recording(&mut s).unwrap();
        let op = s.operation_budget_scope().unwrap();
        let root = recording.scalar(&mut s, &Literal::Number(17.0)).unwrap();
        let other = recording.scalar(&mut s, &Literal::Number(99.0)).unwrap();
        let ledger = Rc::new(recording.into_records().unwrap());
        let view = FactoryProjection::from_recorded_root(&mut s, &ledger, &root)
            .unwrap()
            .unwrap();
        let mut audit = CertificationAudit::new(&mut s).unwrap();
        drop(op);
        // One unit completes the genuine root ref_eq (false). The later class
        // coverage charge must still run and propagate the typed exhaustion.
        s.set_operation_fuel(1);
        let result = view.matches_factory_facts(&mut s, &other, &mut audit);
        assert!(matches!(result, Err(SessionError::Trap(ref error))
            if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
        drop(audit);
        s.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        let _op = s.operation_budget_scope().unwrap();
        assert!(matches(&mut s, &view, &root));
        assert!(!matches(&mut s, &view, &other));
    }
}
