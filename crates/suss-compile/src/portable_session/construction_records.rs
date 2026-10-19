//! Construction records collected only by an explicitly recording FormBridge.
//! These audit direct facts; they are not a complete source/storage certificate.
use super::{
    ConstructorInputs, ScalarInputs, Session, SessionError, SessionValue, SourceArrayInputs,
};
use crate::portable::metered_map::Context;
use std::{cmp::Ordering, collections::hash_map::DefaultHasher, hash::Hasher};
use wasmtime::{AnyRef, RootScope, Rooted};

mod projection;
pub(crate) use projection::FactoryProjection;

// Invocation-local native identity-hash heads. No guest allocation or adapter
// call may occur while this table is live. Exact GC references decide membership.
#[derive(Default)]
struct InputHeads {
    slots: Vec<Option<(u64, usize)>>,
}
impl InputHeads {
    fn new(count: usize, meter: &mut InputIndex<'_>) -> Result<Self, SessionError> {
        if count == 0 { return Ok(Self::default()); }
        let capacity = count.checked_mul(2).and_then(usize::checked_next_power_of_two)
            .map(|n| n.max(4)).ok_or_else(|| SessionError::Host(wasmtime::Error::msg("Input table capacity overflow")))?;
        let bytes = capacity.checked_mul(std::mem::size_of::<Option<(u64, usize)>>())
            .filter(|n| *n <= isize::MAX as usize)
            .ok_or_else(|| SessionError::Host(wasmtime::Error::msg("Input table allocation overflow")))?;
        meter.charge(bytes as u64)?;
        let mut slots = Vec::new();
        slots.try_reserve_exact(capacity).map_err(|_| SessionError::Host(wasmtime::Error::msg("Input table allocation failed")))?;
        for _ in 0..capacity {
            meter.charge(1)?;
            slots.push(None);
        }
        Ok(Self { slots })
    }
    fn position(&self, hash: u64, meter: &mut InputIndex<'_>) -> Result<Option<usize>, SessionError> {
        if self.slots.is_empty() { meter.charge(1)?; return Ok(None); }
        meter.charge(3)?; // length, mask and initial placement arithmetic
        let mask = self.slots.len() - 1;
        let mut index = hash as usize & mask;
        for _ in 0..self.slots.len() {
            // Slot read, full hash comparison and bounded wrap arithmetic.
            meter.charge(4)?;
            match self.slots[index] {
                None => return Ok(Some(index)),
                Some((key, _)) if key == hash => return Ok(Some(index)),
                _ => index = (index + 1) & mask,
            }
        }
        Err(SessionError::Host(wasmtime::Error::msg("Input table probe bound exceeded")))
    }
    fn get(&self, hash: u64, meter: &mut InputIndex<'_>) -> Result<Option<usize>, SessionError> {
        let Some(index) = self.position(hash, meter)? else { return Ok(None); };
        meter.charge(2)?; // selected-slot reread and head extraction
        Ok(self.slots[index].map(|(_, head)| head))
    }
    fn insert(&mut self, hash: u64, head: usize, meter: &mut InputIndex<'_>) -> Result<Option<usize>, SessionError> {
        let index = self.position(hash, meter)?.ok_or_else(|| SessionError::Host(wasmtime::Error::msg("Empty input table insertion")))?;
        meter.charge(2)?;
        Ok(self.slots[index].replace((hash, head)).map(|(_, prior)| prior))
    }
}

struct InputIndex<'a> {
    session: &'a mut Session,
    hash_mask: u64,
}
impl Context<u64> for InputIndex<'_> {
    type Error = SessionError;
    fn charge(&mut self, units: u64) -> Result<(), SessionError> {
        self.session.charge_host_work(units)
    }
    fn compare(&mut self, a: &u64, b: &u64) -> Result<Ordering, SessionError> {
        self.charge(1)?;
        Ok(a.cmp(b))
    }
}
impl InputIndex<'_> {
    fn hash(&mut self, value: &SessionValue) -> Result<u64, SessionError> {
        self.session.check(value)?;
        self.charge(1)?;
        let mut scope = RootScope::new(&mut self.session.store);
        let value = value.value.to_rooted(&mut scope);
        let mut hash = DefaultHasher::new();
        value.ref_hash(&scope, &mut hash)?;
        Ok(hash.finish() & self.hash_mask)
    }
    fn same(&mut self, a: &SessionValue, b: &SessionValue) -> Result<bool, SessionError> {
        self.session.check(a)?;
        self.session.check(b)?;
        self.charge(1)?;
        let mut scope = RootScope::new(&mut self.session.store);
        let a = a.value.to_rooted(&mut scope);
        let b = b.value.to_rooted(&mut scope);
        Ok(Rooted::<AnyRef>::ref_eq(&scope, &a, &b)?)
    }
    fn contains_explicit<R: AsRef<Record>>(
        &mut self,
        index: &InputHeads,
        collisions: &[Option<usize>],
        records: &[R],
        value: &SessionValue,
    ) -> Result<bool, SessionError> {
        let hash = self.hash(value)?;
        let mut cursor = index.get(hash, self)?;
        while let Some(id) = cursor {
            if self.same(value, &records[id].as_ref().value)? {
                return Ok(true);
            }
            cursor = collisions[id];
        }
        Ok(false)
    }
    fn contains<R: AsRef<Record>>(
        &mut self,
        index: &InputHeads,
        collisions: &[Option<usize>],
        records: &[R],
        value: &SessionValue,
    ) -> Result<bool, SessionError> {
        if self.contains_explicit(index, collisions, records, value)? {
            return Ok(true);
        }
        // Copied fields of an EMPTY seed are covered by its pristine complete
        // storage witness, even when they were not bridge-created allocations.
        // Membership alone is not validation: direct seed checks remain required.
        for record in records {
            self.charge(1)?;
            if let Fact::CanonicalSeed(seed) = &record.as_ref().fact {
                if seed.contains_initial_reference(self.session, value)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

pub(crate) enum Fact {
    Scalar(ScalarInputs),
    Array(SourceArrayInputs),
    Constructor(ConstructorInputs),
    CanonicalSeed(std::sync::Arc<super::canonical_seeds::CanonicalSeed>),
    ExternalSeed(&'static str),
}
pub(crate) struct Record {
    pub value: SessionValue,
    pub fact: Fact,
}
impl AsRef<Record> for Record {
    fn as_ref(&self) -> &Record { self }
}
pub(crate) struct BridgeClassInput {
    pub name: &'static str,
    pub constructor: Option<SessionValue>,
    pub descriptor: SessionValue,
}
// Bounded helper for the transient, non-authoritative child-input buffer.
fn record_error(message: &'static str) -> SessionError {
    SessionError::Host(wasmtime::Error::msg(message))
}
fn record_bytes<T>(count: usize) -> Result<u64, SessionError> {
    count.checked_mul(std::mem::size_of::<T>())
        .filter(|bytes| *bytes <= isize::MAX as usize)
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or_else(|| record_error("Transient input allocation overflow"))
}
struct ChildBinding { value: SessionValue }
pub(crate) struct ConstructionRecords {
    domain: std::sync::Arc<super::write_domain::WriteDomain>,
    children: Vec<ChildBinding>,
    records: Vec<Record>,
    bridge_classes: Vec<BridgeClassInput>,
    class_prechecks_valid: bool,
}
impl ConstructionRecords {
    pub(crate) fn with_bridge_classes(bridge_classes: Vec<BridgeClassInput>) -> Self {
        Self {
            domain: super::write_domain::WriteDomain::new(),
            children: Vec::new(),
            records: Vec::new(),
            bridge_classes,
            class_prechecks_valid: true,
        }
    }
    // Eligibility only, used immediately after a callback-free recording fork.
    // This never substitutes for the later current class/prototype validators.
    pub(crate) fn initial_class_precheck(&self, session: &mut Session) -> Result<bool, SessionError> {
        session.charge_host_work(1)?;
        Ok(self.class_prechecks_valid)
    }
    pub(crate) fn note_class_precheck(&mut self, valid: bool) {
        self.class_prechecks_valid &= valid;
        if !valid {self.domain.invalidate();}
    }
    pub(crate) fn matches_class_schemas(
        &self, session: &mut Session,
    ) -> Result<bool, SessionError> {
        self.check_class_schemas(session, None)
    }
    pub(crate) fn matches_class_schemas_audited(
        &self, session: &mut Session, audit: &mut super::certification_audit::CertificationAudit,
    ) -> Result<bool, SessionError> {
        self.check_class_schemas(session, Some(audit))
    }
    fn check_class_schemas(
        &self, session: &mut Session, mut audit: Option<&mut super::certification_audit::CertificationAudit>,
    ) -> Result<bool, SessionError> {
        let _request = session.operation_budget_scope()?;
        let expected = crate::portable_macro_data::source_class_names().len() + 1;
        let mut valid = self.class_prechecks_valid && self.bridge_classes.len() == expected;
        let mut seen = Vec::new();
        session.charge_host_work(expected as u64 + 1)?;
        seen.try_reserve_exact(expected).map_err(|_| {
            SessionError::Host(wasmtime::Error::msg("Class coverage allocation failed"))
        })?;
        for _ in 0..expected {
            session.charge_host_work(1)?;
            seen.push(false);
        }
        let mut expected_names = crate::portable_macro_data::source_class_names()
            .chain(std::iter::once("SourceArray")).enumerate();
        for input in &self.bridge_classes {
            session.charge_host_work(1)?;
            let mut position = None;
            if let Some((index, name)) = expected_names.next() {
                session.charge_host_work(input.name.len() as u64 + name.len() as u64 + 1)?;
                if input.name == name { position = Some(index); }
            }
            if position.is_none() {
                for (index, name) in crate::portable_macro_data::source_class_names()
                    .chain(std::iter::once("SourceArray"))
                    .enumerate()
                {
                    session.charge_host_work(input.name.len() as u64 + name.len() as u64 + 1)?;
                    if input.name == name {
                        position = Some(index);
                        break;
                    }
                }
            }
            if let Some(index) = position {
                valid &= !seen[index];
                seen[index] = true;
            } else {
                valid = false;
            }
            // Do not suppress a later budget/interrupt error after a false audit.
            valid &= match audit.as_deref_mut() {
                Some(audit) => audit.class(session, input)?,
                None => session.initial_source_class_matches(
                    input.name, input.constructor.as_ref(), &input.descriptor,
                )?,
            };
        }
        for present in seen {
            session.charge_host_work(1)?;
            valid &= present;
        }
        Ok(valid)
    }
    pub(crate) fn matches_prototype_origins(&self, session: &mut Session) -> Result<bool, SessionError> {
        self.check_prototype_origins(session, None)
    }
    pub(crate) fn matches_prototype_origins_audited(
        &self, session: &mut Session, audit: &mut super::certification_audit::CertificationAudit,
    ) -> Result<bool, SessionError> {
        self.check_prototype_origins(session, Some(audit))
    }
    fn check_prototype_origins(
        &self, session: &mut Session, mut audit: Option<&mut super::certification_audit::CertificationAudit>,
    ) -> Result<bool, SessionError> {
        let _request = session.operation_budget_scope()?;
        let mut valid = true;
        for record in &self.records {
            session.charge_host_work(1)?;
            if let Fact::Constructor(inputs) = &record.fact {
                valid &= match audit.as_deref_mut() {
                    Some(audit) => inputs.matches_prototype_origin_audited(session, audit)?,
                    None => inputs.matches_prototype_origin(session)?,
                };
            }
        }
        Ok(valid)
    }
    // This closes only recorded constructor/array input edges, not all physical
    // storage. Descriptor state, seeds and memo transitions need separate proof.
    // A query over ORIGINAL immutable ledger values/private initial seed
    // membership only. Caller must separately establish this ledger's complete
    // current DATA health and relevance; no receipt/child/domain is returned.
    pub(crate) fn contains_original_data_reference(
        &self, session: &mut Session, value: &SessionValue,
    ) -> Result<bool, SessionError> {
        session.charge_host_work((2 * std::mem::size_of::<InputIndex<'_>>() + 6) as u64)?;
        let mut meter = InputIndex { session, hash_mask: u64::MAX };
        for record in &self.records {
            meter.charge(3)?; // record read, kind/loop and eventual scan control
            if meter.same(value, &record.value)? { return Ok(true); }
            if let Fact::CanonicalSeed(seed) = &record.fact {
                meter.charge(1)?;
                if seed.contains_initial_reference(meter.session, value)? { return Ok(true); }
            }
        }
        Ok(false)
    }
    pub(crate) fn matches_recorded_inputs(
        &self,
        session: &mut Session,
        root: &SessionValue,
    ) -> Result<bool, SessionError> {
        self.check_recorded_inputs(session, root, u64::MAX)
    }
    // DATA-only use of complete original native factory observations. This
    // returns a boolean, never a domain/receipt/CheckedChild. The only caller is
    // the first-use source evidence walk, which recursively checks every input
    // and binds it to the actual native recipe dependency before this call.
    pub(crate) fn matches_current_native_inputs(
        &self, session: &mut Session, root: &SessionValue, inputs: &[&SessionValue],
    ) -> Result<bool, SessionError> {
        self.matches_native_inputs(session, root, inputs, None)
    }
    // Only an unforgeable, call-local original DATA evidence borrow can extend
    // missing EDGE membership. The root still requires a local explicit record.
    pub(crate) fn matches_current_native_inputs_with_descendants(
        &self, session: &mut Session, root: &SessionValue, inputs: &[&SessionValue],
        descendants: &mut crate::portable_macro_graph::source_roots::OriginalDataInputs<'_>,
    ) -> Result<bool, SessionError> {
        session.charge_host_work(2)?; // exact current-ledger borrow binding
        if descendants.belongs_to(self) {
            self.matches_native_inputs(session, root, inputs, Some(descendants))
        } else {
            // A mismatched frontier grants nothing; retain ordinary full checks.
            self.matches_native_inputs(session, root, inputs, None)
        }
    }
    fn matches_native_inputs(
        &self, session: &mut Session, root: &SessionValue, inputs: &[&SessionValue],
        descendants: Option<&mut crate::portable_macro_graph::source_roots::OriginalDataInputs<'_>>,
    ) -> Result<bool, SessionError> {
        session.charge_host_work(2)?; // new optional edge-policy dispatch
        let count = inputs.len();
        session.charge_host_work(record_bytes::<ChildBinding>(count)?
            .checked_add((count as u64).checked_mul(4)
                .ok_or_else(|| record_error("Native data input control overflow"))?)
            .ok_or_else(|| record_error("Native data input prepayment overflow"))?)?;
        let mut children = Vec::new();
        children.try_reserve_exact(count).map_err(|_| record_error("Native data input allocation failed"))?;
        for value in inputs {
            session.check(value)?;
            children.push(ChildBinding { value: (*value).clone() });
        }
        Self::check_selected_inputs(session, root, u64::MAX, &self.records, &children, descendants)
    }
    fn check_recorded_inputs(
        &self,
        session: &mut Session,
        root: &SessionValue,
        hash_mask: u64,
    ) -> Result<bool, SessionError> {
        Self::check_selected_inputs(session, root, hash_mask, &self.records, &self.children, None)
    }
    fn check_selected_inputs<R: AsRef<Record>>(
        session: &mut Session, root: &SessionValue, hash_mask: u64,
        records: &[R], children: &[ChildBinding],
        descendants: Option<&mut crate::portable_macro_graph::source_roots::OriginalDataInputs<'_>>,
    ) -> Result<bool, SessionError> {
        let _request = session.operation_budget_scope()?;
        let mut meter = InputIndex { session, hash_mask };
        let tracing = std::env::var_os("SUSS_INPUT_INDEX_TRACE").is_some();
        let trace = |meter: &InputIndex<'_>, stage: &str| {
            if tracing { eprintln!("input-index records={} stage={stage} fuel={:?}",
                records.len(), meter.session.operation_fuel_remaining()); }
        };
        trace(&meter, "start");
        let mut index = InputHeads::new(records.len(), &mut meter)?;
        let bytes = records
            .len()
            .checked_mul(std::mem::size_of::<Option<usize>>())
            .ok_or_else(|| SessionError::Host(wasmtime::Error::msg("Input index overflow")))?;
        meter.charge(bytes as u64)?;
        let mut collisions = Vec::new();
        collisions
            .try_reserve_exact(records.len())
            .map_err(|_| {
                SessionError::Host(wasmtime::Error::msg("Input index allocation failed"))
            })?;
        trace(&meter, "reserved");
        // No guest execution or guest allocation occurs while this ephemeral
        // identity-hash index is live. Collision chains always use ref_eq.
        for (id, record) in records.iter().enumerate() {
            let hash = meter.hash(&record.as_ref().value)?;
            // insert returns the previous head from the same charged probe.
            // Collision capacity was reserved above, so publication needs no
            // fallible work between updating the head and recording its link.
            let prior = index.insert(hash, id, &mut meter)?;
            collisions.push(prior);
            if tracing && (id + 1).is_multiple_of(64) {
                eprintln!("input-index indexed={} records={} fuel={:?}", id + 1,
                    records.len(), meter.session.operation_fuel_remaining());
            }
        }
        trace(&meter, "built");
        let mut valid = meter.contains_explicit(&index, &collisions, records, root)?;
        #[cfg(test)]
        let diagnostic_root = valid;
        trace(&meter, "root");
        let edges = Self::check_selected_edges(&mut meter, &index, &collisions, records, children, descendants);
        trace(&meter, "edges-return");
        if tracing { eprintln!("input-index edge-result={:?}", edges.as_ref().map_err(|e| e.to_string())); }
        let edges = edges?;
        valid &= edges;
        #[cfg(test)]
        if !valid {
            eprintln!("RETAINED_INPUT_CLOSURE test={:?} records={} children={} root={} edges={}",
                std::thread::current().name(), records.len(), children.len(), diagnostic_root, edges);
        }
        Ok(valid)
    }
    fn check_input_edges(
        &self,
        meter: &mut InputIndex<'_>,
        index: &InputHeads,
        collisions: &[Option<usize>],
    ) -> Result<bool, SessionError> {
        Self::check_selected_edges(meter, index, collisions, &self.records, &self.children, None)
    }
    fn check_selected_edges<R: AsRef<Record>>(
        meter: &mut InputIndex<'_>, index: &InputHeads, collisions: &[Option<usize>],
        records: &[R], children: &[ChildBinding],
        mut descendants: Option<&mut crate::portable_macro_graph::source_roots::OriginalDataInputs<'_>>,
    ) -> Result<bool, SessionError> {
        let mut valid = true;
        for record in records {
            meter.charge(1)?;
            let record = record.as_ref();
            match &record.fact {
                Fact::Array(fact) => {
                    for value in fact.inputs() {
                        let mut member=meter.contains(index, collisions, records, value)?;
                        if !member {
                            for child in children {
                                meter.charge(1)?;
                                if meter.same(value,&child.value)? {member=true;break;}
                            }
                        }
                        meter.charge(2)?; // added edge-policy/missing-edge branch
                        if !member {
                            if let Some(original) = descendants.as_deref_mut() {
                                member = original.contains_original(meter.session, value)?;
                            }
                        }
                        valid &= member;
                    }
                }
                Fact::Constructor(fact) => {
                    for value in fact.inputs() {
                        let mut member=meter.contains(index, collisions, records, value)?;
                        if !member {
                            for child in children {
                                meter.charge(1)?;
                                if meter.same(value,&child.value)? {member=true;break;}
                            }
                        }
                        meter.charge(2)?; // added edge-policy/missing-edge branch
                        if !member {
                            if let Some(original) = descendants.as_deref_mut() {
                                member = original.contains_original(meter.session, value)?;
                            }
                        }
                        valid &= member;
                    }
                }
                Fact::ExternalSeed(_) => valid = false,
                Fact::Scalar(_) => {}
                Fact::CanonicalSeed(_) => {}
            }
        }
        Ok(valid)
    }
    pub(crate) fn attach_source_child(
        &mut self, session:&mut Session,
        child:&crate::portable_macro_graph::source_roots::CheckedChild,
    )->Result<(),SessionError> {
        session.check(child.value())?;
        if self.children.len()==self.children.capacity() {
            let count=self.children.len().checked_mul(2).unwrap_or(usize::MAX).max(4);
            let bytes=count.checked_add(self.children.len()).and_then(|n|n.checked_mul(std::mem::size_of::<ChildBinding>()))
                .ok_or_else(||SessionError::Host(wasmtime::Error::msg("Source-child capacity overflow")))?;
            session.charge_host_work(bytes as u64)?;
            self.children.try_reserve_exact(count-self.children.len())
                .map_err(|_|SessionError::Host(wasmtime::Error::msg("Source-child allocation failed")))?;
        }
        // A failed domain attachment publishes no closure membership.
        session.charge_host_work(1)?;
        self.domain.attach_child(session,child)?;
        self.children.push(ChildBinding {value:child.value().clone()});
        Ok(())
    }
    pub(crate) fn seal_source_domain(&self, session:&mut Session)->Result<(),SessionError> {
        self.domain.seal(session)
    }
    pub(crate) fn write_domain(&self)->std::sync::Arc<super::write_domain::WriteDomain> {self.domain.clone()}
    pub(crate) fn push(&mut self,session:&mut Session,value:SessionValue,fact:Fact)->Result<(),SessionError> {
        self.domain.record(session,&value,&fact)?;
        self.push_prepared(session,value,fact)
    }
    pub(crate) fn push_prepared(
        &mut self,
        session: &mut Session,
        value: SessionValue,
        fact: Fact,
    ) -> Result<(), SessionError> {
        session.charge_host_work(1)?;
        if self.records.len() == self.records.capacity() {
            let count = self
                .records
                .capacity()
                .checked_mul(2)
                .ok_or_else(|| {
                    SessionError::Host(wasmtime::Error::msg(
                        "Construction record capacity overflow",
                    ))
                })?
                .max(4);
            let bytes = count
                .checked_add(self.records.len())
                .and_then(|n| n.checked_mul(std::mem::size_of::<Record>()))
                .ok_or_else(|| {
                    SessionError::Host(wasmtime::Error::msg(
                        "Construction record allocation overflow",
                    ))
                })?;
            session.charge_host_work(bytes as u64)?;
            self.records
                .try_reserve_exact(count - self.records.len())
                .map_err(|_| {
                    SessionError::Host(wasmtime::Error::msg(
                        "Construction record allocation failed",
                    ))
                })?;
        }
        self.records.push(Record { value, fact });
        Ok(())
    }
    pub(crate) fn len(&self) -> usize {
        self.records.len()
    }
    #[cfg(test)]
    pub(crate) fn canonical_seed_fact_count_for_test(&self) -> usize {
        self.records.iter().filter(|record| matches!(&record.fact, Fact::CanonicalSeed(_))).count()
    }
    pub(crate) fn external_seeds(&self, session: &mut Session) -> Result<usize, SessionError> {
        let mut count = 0;
        for record in &self.records {
            session.charge_host_work(1)?;
            if let Fact::ExternalSeed(name) = &record.fact {
                session.charge_host_work(name.len() as u64)?;
                count += 1;
            }
        }
        Ok(count)
    }
    pub(crate) fn fresh_direct_status(&self, session: &mut Session)
        -> Result<(bool, bool), SessionError> {
        let _request = session.operation_budget_scope()?;
        let mut source = true;
        let mut certified = true;
        for record in &self.records {
            session.charge_host_work(1)?;
            let (data, reusable) = match &record.fact {
                Fact::Constructor(fact) => fact.fresh_direct_status(session, &record.value)?,
                Fact::Scalar(fact) => { let valid = fact.matches(session, &record.value)?; (valid, valid) }
                Fact::Array(fact) => { let valid = fact.matches(session, &record.value)?; (valid, valid) }
                Fact::CanonicalSeed(fact) => fact.fresh_direct_status(session, &record.value)?,
                Fact::ExternalSeed(_) => (false, false),
            };
            session.charge_host_work(2)?;
            source &= data;
            certified &= reusable;
        }
        Ok((source, certified))
    }
    pub(crate) fn matches_fresh_source_inputs(
        &self, session: &mut Session, root: &SessionValue,
        children: &[crate::portable_macro_graph::source_roots::FreshSourceProof],
    ) -> Result<bool, SessionError> {
        let _request = session.operation_budget_scope()?;
        let count = self.children.len().checked_add(children.len())
            .ok_or_else(|| record_error("Fresh input coverage overflow"))?;
        let bytes = record_bytes::<ChildBinding>(count)?;
        let controls = u64::try_from(count).ok().and_then(|n| n.checked_mul(3))
            .ok_or_else(|| record_error("Fresh input control overflow"))?;
        session.charge_host_work(bytes.checked_add(controls)
            .ok_or_else(|| record_error("Fresh input allocation overflow"))?)?;
        let mut selected = Vec::new();
        selected.try_reserve_exact(count)
            .map_err(|_| record_error("Fresh input allocation failed"))?;
        for child in &self.children { selected.push(ChildBinding { value: child.value.clone() }); }
        for child in children {
            session.check(child.value())?;
            selected.push(ChildBinding { value: child.value().clone() });
        }
        Self::check_selected_inputs(session, root, u64::MAX, &self.records, &selected, None)
    }
    pub(crate) fn matches_direct_facts(&self, session: &mut Session) -> Result<bool, SessionError> {
        let _request = session.operation_budget_scope()?;
        let mut valid = true;
        for record in &self.records {
            session.charge_host_work(1)?;
            // Never short-circuit later inspection/budget/interrupt failures.
            valid &= match &record.fact {
                Fact::Scalar(fact) => fact.matches(session, &record.value)?,
                Fact::Array(fact) => fact.matches(session, &record.value)?,
                Fact::Constructor(fact) => fact.matches(session, &record.value)?,
                Fact::CanonicalSeed(fact) => fact.matches(session, &record.value)?,
                Fact::ExternalSeed(_) => false,
            };
        }
        Ok(valid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn construction_records_table_clustered_hashes_and_empty_overflow() {
        let mut session = Session::new().unwrap();
        let _request = session.operation_budget_scope().unwrap();
        let mut meter = InputIndex { session: &mut session, hash_mask: u64::MAX };
        let empty = InputHeads::new(0, &mut meter).unwrap();
        assert_eq!(empty.get(0, &mut meter).unwrap(), None);
        assert!(InputHeads::new(usize::MAX, &mut meter).is_err());
        for count in [1, 3, 4, 5, 17] {
            let mut table = InputHeads::new(count, &mut meter).unwrap();
            let stride = table.slots.len() as u64;
            // Distinct full hashes deliberately share one placement bucket.
            for id in 0..count {
                assert_eq!(table.insert(stride * id as u64 + stride - 1, id, &mut meter).unwrap(), None);
            }
            for id in 0..count {
                assert_eq!(table.get(stride * id as u64 + stride - 1, &mut meter).unwrap(), Some(id));
            }
            assert_eq!(table.get(stride * count as u64 + stride - 1, &mut meter).unwrap(), None);
            assert_eq!(table.insert(stride - 1, count, &mut meter).unwrap(), Some(0));
            assert_eq!(table.get(stride - 1, &mut meter).unwrap(), Some(count));
        }
    }

    #[test]
    fn construction_records_table_probe_failures_leave_heads_unchanged() {
        let mut session = Session::new().unwrap();
        let _request = session.operation_budget_scope().unwrap();
        let mut meter = InputIndex { session: &mut session, hash_mask: u64::MAX };
        let mut table = InputHeads::new(4, &mut meter).unwrap();
        for (id, hash) in [0, 8, 16].into_iter().enumerate() {
            table.insert(hash, id, &mut meter).unwrap();
        }
        // Fourth distinct collision and replacement of the third occupied slot.
        for hash in [24, 16] {
            meter.session.store.set_fuel(1_000_000).unwrap();
            let original = table.slots.clone();
            let before = meter.session.store.get_fuel().unwrap();
            table.insert(hash, 99, &mut meter).unwrap();
            let cost = before - meter.session.store.get_fuel().unwrap();
            table.slots.clone_from(&original);
            for allowance in 0..cost {
                meter.session.store.set_fuel(allowance).unwrap();
                let error = table.insert(hash, 99, &mut meter).unwrap_err();
                assert!(matches!(error, SessionError::Trap(ref e) if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{error:?}");
                assert_eq!(table.slots, original);
            }
            meter.session.store.set_fuel(1_000_000).unwrap();
            assert_eq!(table.get(16, &mut meter).unwrap(), Some(2));
            meter.session.interrupt_handle().interrupt();
            assert!(table.insert(hash, 99, &mut meter).unwrap_err().is_interrupt());
            assert_eq!(table.slots, original);
        }
    }

    #[test]
    fn construction_records_missing_root_does_not_skip_later_edge_budget_errors() {
        let mut session = Session::new().unwrap();
        let root = session
            .data_scalar(&crate::portable::hir::Literal::Number(17.0))
            .unwrap();
        let mut records = ConstructionRecords::default();
        records
            .push(&mut session, root.clone(), Fact::ExternalSeed("unverified"))
            .unwrap();
        let index = InputHeads::default();
        for interrupted in [false, true] {
            let request = session.operation_budget_scope().unwrap();
            let mut meter = InputIndex {
                session: &mut session,
                hash_mask: u64::MAX,
            };
            assert!(
                !meter
                    .contains_explicit(&index, &[None], &records.records, &root)
                    .unwrap()
            );
            if interrupted {
                meter.session.interrupt_handle().interrupt();
            } else {
                meter.session.store.set_fuel(0).unwrap();
            }
            let error = records
                .check_input_edges(&mut meter, &index, &[None])
                .unwrap_err();
            if interrupted {
                assert!(error.is_interrupt(), "{error:?}");
            } else {
                assert!(
                    matches!(error, SessionError::Trap(ref e) if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
                    "{error:?}"
                );
            }
            drop(request);
        }
        assert!(
            !records
                .matches_recorded_inputs(&mut session, &root)
                .unwrap()
        );
    }
    #[test]
    fn construction_records_follow_normalized_constructor_fields_not_unused_arguments() {
        for phase in [
            crate::portable::resolve::Phase::Runtime,
            crate::portable::resolve::Phase::Macro,
        ] {
            let mut session = match phase {
                crate::portable::resolve::Phase::Runtime => Session::new_repl().unwrap(),
                crate::portable::resolve::Phase::Macro => Session::new_macro().unwrap(),
            };
            session.eval("(deftype RecordedInputProbe [a b])").unwrap();
            let class = session.eval("RecordedInputProbe").unwrap();
            let unused = session
                .data_scalar(&crate::portable::hir::Literal::Number(7.0))
                .unwrap();
            let (child, fact) = session
                .data_scalar_with_inputs(&crate::portable::hir::Literal::Number(11.0))
                .unwrap();
            let descriptor = session.data_descriptor(&class, true).unwrap();
            session
                .inspect(&descriptor, |mut store, value| {
                    let descriptor = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    let schema = descriptor
                        .field(&mut store, 1)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    let name = schema.get(&mut store, 0)?;
                    schema.set(&mut store, 1, name)?;
                    Ok(())
                })
                .unwrap();
            let (parent, inputs) = session
                .data_construct_with_inputs(&class, &[&unused, &child, &unused])
                .unwrap();
            let mut records = ConstructionRecords::default();
            records
                .push(
                    &mut session,
                    parent.clone(),
                    Fact::Constructor(inputs.unwrap()),
                )
                .unwrap();
            assert!(
                !records
                    .matches_recorded_inputs(&mut session, &parent)
                    .unwrap()
            );
            // Last duplicate name wins; neither overwritten nor extra arguments
            // are retained inputs. Padding is the immutable Undefined sentinel.
            records
                .push(&mut session, child, Fact::Scalar(fact))
                .unwrap();
            session.collect().unwrap();
            assert!(
                records
                    .matches_recorded_inputs(&mut session, &parent)
                    .unwrap()
            );
            assert!(records.matches_direct_facts(&mut session).unwrap());
            let (padded, inputs) = session
                .data_construct_with_inputs(&class, &[&unused])
                .unwrap();
            let mut padded_records = ConstructionRecords::default();
            padded_records
                .push(
                    &mut session,
                    padded.clone(),
                    Fact::Constructor(inputs.unwrap()),
                )
                .unwrap();
            assert!(
                padded_records
                    .matches_recorded_inputs(&mut session, &padded)
                    .unwrap()
            );
            assert!(padded_records.matches_direct_facts(&mut session).unwrap());
            session.reset().unwrap();
            assert!(matches!(
                records.matches_recorded_inputs(&mut session, &parent),
                Err(SessionError::ForeignValue)
            ));
        }
    }
    #[test]
    fn construction_records_require_independent_child_facts_and_exact_root() {
        let mut session = Session::new().unwrap();
        let (child, fact) = session
            .data_scalar_with_inputs(&crate::portable::hir::Literal::String(vec![17]))
            .unwrap();
        let (parent, array) = session.data_array_with_inputs(&[&child, &child]).unwrap();
        let mut records = ConstructionRecords::default();
        records
            .push(&mut session, parent.clone(), Fact::Array(array))
            .unwrap();
        session.collect().unwrap();
        assert!(records.matches_direct_facts(&mut session).unwrap());
        assert!(
            !records
                .matches_recorded_inputs(&mut session, &parent)
                .unwrap()
        );
        records
            .push(&mut session, child, Fact::Scalar(fact))
            .unwrap();
        session.collect().unwrap();
        assert!(
            records
                .matches_recorded_inputs(&mut session, &parent)
                .unwrap()
        );
        let other = session
            .data_scalar(&crate::portable::hir::Literal::String(vec![17]))
            .unwrap();
        assert!(
            !records
                .matches_recorded_inputs(&mut session, &other)
                .unwrap()
        );
        // Force distinct references into one bucket: hash equality must never
        // substitute for GC-reference equality, including after collection.
        assert!(
            records
                .check_recorded_inputs(&mut session, &parent, 0)
                .unwrap()
        );
        assert!(
            !records
                .check_recorded_inputs(&mut session, &other, 0)
                .unwrap()
        );
        for interrupted in [false, true] {
            let request = session.operation_budget_scope().unwrap();
            if interrupted {
                session.interrupt_handle().interrupt();
            } else {
                session.store.set_fuel(1).unwrap();
            }
            let error = records
                .matches_recorded_inputs(&mut session, &parent)
                .unwrap_err();
            if interrupted {
                assert!(error.is_interrupt(), "{error:?}");
            } else {
                assert!(
                    matches!(error, SessionError::Trap(ref e) if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
                    "{error:?}"
                );
            }
            drop(request);
            assert!(
                records
                    .matches_recorded_inputs(&mut session, &parent)
                    .unwrap()
            );
        }
        let mut foreign = Session::new().unwrap();
        let foreign_root = foreign
            .data_scalar(&crate::portable::hir::Literal::Nil)
            .unwrap();
        assert!(matches!(
            records.matches_recorded_inputs(&mut session, &foreign_root),
            Err(SessionError::ForeignValue)
        ));
        // A present child record is insufficient if its direct facts changed.
        // The low-level test below changes the UTF-16 storage directly; it does
        // not depend on source string indexing adapters or guest equality.
        let child = match &records.records[1].fact {
            Fact::Scalar(_) => &records.records[1].value,
            _ => unreachable!(),
        };
        session
            .inspect(child, |mut store, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                array.set(&mut store, 0, wasmtime::Val::I32(99))?;
                Ok(())
            })
            .unwrap();
        assert!(
            records
                .matches_recorded_inputs(&mut session, &parent)
                .unwrap()
        );
        assert!(!records.matches_direct_facts(&mut session).unwrap());
    }
    #[test]
    fn construction_records_false_seed_does_not_hide_later_fuel_error() {
        let mut session = Session::new().unwrap();
        let (value, fact) = session
            .data_scalar_with_inputs(&crate::portable::hir::Literal::Nil)
            .unwrap();
        let mut records = ConstructionRecords::default();
        records
            .push(
                &mut session,
                value.clone(),
                Fact::ExternalSeed("unverified"),
            )
            .unwrap();
        records
            .push(&mut session, value, Fact::Scalar(fact))
            .unwrap();
        let request = session.operation_budget_scope().unwrap();
        session.store.set_fuel(1).unwrap();
        let error = records.matches_direct_facts(&mut session).unwrap_err();
        assert!(
            matches!(error,SessionError::Trap(ref error) if error.downcast_ref::<wasmtime::Trap>()==Some(&wasmtime::Trap::OutOfFuel)),
            "{error:?}"
        );
        drop(request);
        assert!(!records.matches_direct_facts(&mut session).unwrap());
    }
}

impl Default for ConstructionRecords {
 fn default()->Self {Self {domain:super::write_domain::WriteDomain::new(),children:Vec::new(),records:Vec::new(),bridge_classes:Vec::new(),class_prechecks_valid:false}}
}

// Eligibility is only the existence of private provisioning witnesses, not a
// mutable class/method verdict. Every selected value still gets full current
// source/input/class/prototype validation. Bootstrap cannot mint this authority.
impl Session {
    pub(crate) fn source_controller_ready(&mut self) -> Result<bool, SessionError> {
        self.charge_host_work(6)?;
        Ok(self.bootstrap_core && self.canonical_methods.is_some() && self.canonical_seeds.is_some())
    }
}


#[cfg(test)]
mod controller_provisioning_tests {
    use super::*;

    #[test]
    fn source_controller_requires_installed_core_and_both_original_private_registries() {
        // A minimal Store has neither shipped provisioning nor private witnesses.
        let mut idle = Session::new().unwrap();
        assert!(!idle.source_controller_ready().unwrap());
        for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
            // Only actual normal provisioning may establish this state. Never
            // synthesize a flag, registry, canonical binding or source receipt.
            assert!(session.source_controller_ready().unwrap());
            let methods = session.canonical_methods.take().unwrap();
            assert!(!session.source_controller_ready().unwrap());
            session.canonical_methods = Some(methods);
            assert!(session.source_controller_ready().unwrap());
            let seeds = session.canonical_seeds.take().unwrap();
            assert!(!session.source_controller_ready().unwrap());
            session.canonical_seeds = Some(seeds);
            assert!(session.source_controller_ready().unwrap());
        }
    }
}
