//! One invocation's private children plus strict direct KEY children.
//! Health-only children close a transient native input walk; they never grant
//! domain membership, memo authority, or a retained cache entry.
use super::reader_plan::{ChildMode, NativeReaderPlan};
use super::*;
use crate::portable_macro_data::FormBridge;
use crate::portable_session::{
    SessionValue,
    construction_records::ConstructionRecords,
    deferred_factory::DeferredFactory,
    private_fixups::slot_log::{PrivateToken, SharedSlotLog},
};

struct Binding {
    alias: Value,
    ordinal: usize,
    token: Option<PrivateToken>,
    proof: FreshSourceProof,
    expected: Rc<Expectation>,
    exposed: std::cell::Cell<bool>,
}
pub(super) struct Inputs {
    bindings: Vec<Binding>,
}
impl Inputs {
    pub(super) fn selected(
        &self,
        session: &mut Session,
        bridge: &FormBridge,
        log: &SharedSlotLog,
        alias: Value,
    ) -> Result<Option<&SessionValue>> {
        log.borrow().binding_owner(session)?;
        for binding in &self.bindings {
            session.charge_host_work(3)?;
            if binding.alias != alias {
                continue;
            }
            if let Some(token) = &binding.token {
                let slots = log.borrow();
                session.charge_host_work(3)?;
                if !token.belongs_to(&slots) || token.id() != binding.ordinal {
                    return Err(invalid("Foreign transient source token"));
                }
                drop(slots.token_handle(session, binding.ordinal)?);
                return Ok(Some(token.value()));
            }
            // Direct key exposure is STRICT. An Uncached value cannot become a
            // direct checked child merely because its current data is healthy.
            let Some(certificate) = binding.proof.certificate() else {
                return Err(invalid("Uncertifiable direct reader key"));
            };
            if Certificate::checked(session, bridge, certificate, &binding.expected)?.is_none() {
                return Err(invalid("Direct reader key changed at exposure"));
            }
            session.charge_host_work(1)?;
            binding.exposed.set(true);
            return Ok(Some(binding.proof.value()));
        }
        Ok(None)
    }
}
pub(super) struct Pending {
    value: SessionValue,
    records: ConstructionRecords,
    deferred: Vec<DeferredFactory>,
    inputs: SourceInputs,
    alias_root: bool,
}
impl AnalysisGraph<'_> {
    pub(super) fn construct_transient_reader_parent(
        &mut self,
        native: Value,
        id: usize,
        plan: &NativeReaderPlan,
        states: &[Option<FreshSourceProof>],
    ) -> Result<Pending> {
        let log =
            crate::portable_session::private_fixups::slot_log::PrivateSlotLog::new(self.session)?;
        let mut by_alias = Vec::new();
        reserve(self.session, &mut by_alias, self.recipes.len())?;
        self.session.charge_host_work(self.recipes.len() as u64)?;
        by_alias.resize(self.recipes.len(), None);
        let mut bindings = Vec::new();
        reserve(self.session, &mut bindings, plan.layout[id].len())?;
        for (ordinal, &child) in plan.layout[id].iter().enumerate() {
            self.session.charge_host_work(3)?;
            let expected = plan.expected[child]
                .as_ref()
                .ok_or_else(|| invalid("Missing transient child expectation"))?;
            let proof = states[child]
                .as_ref()
                .ok_or_else(|| invalid("Unbuilt transient child"))?;
            if !proof.checked(self.session, self.bridge, expected)? {
                return Err(invalid(
                    "Transient source child changed before construction",
                ));
            }
            let direct = plan.modes[child] == ChildMode::CheckedDirectKey;
            if direct && proof.certificate().is_none() {
                return Err(invalid(
                    "A direct KEY requires its strict checked source certificate",
                ));
            }
            let token = if direct {
                None
            } else {
                drop(log.borrow_mut().token(self.session, ordinal)?);
                Some(log.borrow().token_handle(self.session, ordinal)?)
            };
            self.session.charge_host_work(6)?;
            let alias = self.source_roots.roots[child].alias;
            let proof = proof.retain(self.session)?;
            by_alias[alias] = Some(ordinal);
            bindings.push(Binding {
                alias,
                ordinal,
                token,
                proof,
                expected: expected.clone(),
                exposed: std::cell::Cell::new(false),
            });
        }
        self.session.charge_host_work(3)?;
        let expected = plan.expected[id]
            .as_ref()
            .ok_or_else(|| invalid("Missing parent expectation"))?
            .clone();
        let inputs = SourceInputs {
            expected,
            log,
            inputs: Vec::new(),
            by_alias,
            transient: Some(Inputs { bindings }),
        };
        // Determine reference-only lineage from native Alias edges before any
        // callback. Do not construct a separate copy of an aliased child root.
        let mut cursor = native;
        let mut direct_child = None;
        for _ in 0..self.recipes.len() {
            self.session.charge_host_work(3)?;
            if let Some(Some(ordinal)) = inputs.by_alias.get(cursor) {
                direct_child = Some(*ordinal);
                break;
            }
            match self.recipes.get(cursor).and_then(Option::as_ref) {
                Some(Recipe::Alias(next)) => cursor = *next,
                Some(_) => break,
                None => return Err(invalid("Missing transient native parent recipe")),
            }
        }
        let normal = self.bridge.recording(self.session)?;
        let private = normal.deferred_private_bridge(self.session, &inputs.log)?;
        let alias_root = direct_child.is_some();
        let value = if let Some(ordinal) = direct_child {
            let bindings = &inputs.transient.as_ref().unwrap().bindings;
            self.session.charge_host_work(4)?;
            if bindings.len() != 1 || ordinal != 0 {
                return Err(invalid("Ambiguous transient root alias lineage"));
            }
            bindings[0].proof.value().clone()
        } else {
            // selected direct children taint their ancestors just like private
            // tokens: this forces deferred recording with zero placeholders too.
            self.materialize_with_inputs(native, &private, None, Some((&inputs, &normal)))?
        };
        drop(normal);
        let (records, deferred) = private.into_deferred_records()?;
        Ok(Pending {
            value,
            records,
            deferred,
            inputs,
            alias_root,
        })
    }
    pub(super) fn finish_transient_reader_parent(
        &mut self,
        mut pending: Pending,
    ) -> Result<FreshSourceProof> {
        let bindings = &pending.inputs.transient.as_ref().unwrap().bindings;
        let mut children = Vec::new();
        reserve(self.session, &mut children, bindings.len())?;
        for binding in bindings {
            self.session.charge_host_work(2)?;
            if !binding
                .proof
                .checked(self.session, self.bridge, &binding.expected)?
            {
                return Err(invalid(
                    "Transient child changed during construction callbacks",
                ));
            }
            if !pending.alias_root && binding.token.is_none() && !binding.exposed.get() {
                return Err(invalid("Unexposed admitted direct reader key"));
            }
            children.push(binding.proof.retain(self.session)?);
        }
        if pending.alias_root {
            self.session.charge_host_work(3)?;
            if children.len() != 1 || pending.records.len() != 0 || !pending.deferred.is_empty() {
                return Err(invalid("Invalid transient reference-only lineage"));
            }
            pending.value = children[0].value().clone();
        }
        let mut pairs = Vec::new();
        reserve(self.session, &mut pairs, bindings.len())?;
        for binding in bindings {
            self.session.charge_host_work(2)?;
            if let Some(token) = &binding.token {
                let log = pending.inputs.log.borrow();
                self.session.charge_host_work(3)?;
                if !token.belongs_to(&log) || token.id() != binding.ordinal {
                    return Err(invalid("Foreign final transient private binding"));
                }
                drop(log.token_handle(self.session, binding.ordinal)?);
                pairs.push((token.value(), binding.proof.value()));
            }
        }
        pending
            .inputs
            .log
            .borrow()
            .apply(self.session, &pending.value, &pairs)?;
        for child in &children {
            self.session.charge_host_work(1)?;
            if let Some(certificate) = child.certificate() {
                self.session.charge_host_work(2)?;
                let checked = CheckedChild {
                    certificate: certificate.clone(),
                };
                pending
                    .records
                    .attach_source_child(self.session, &checked)?;
            }
            // No domain accessor or CheckedChild conversion for Uncached data.
        }
        for factory in pending.deferred {
            self.session.charge_host_work(1)?;
            let (value, fact) = factory.finalize(self.session, &pairs)?;
            pending.records.push(self.session, value, fact)?;
        }
        drop(pairs);
        FreshSourceProof::finish(
            self.session,
            self.bridge,
            pending.inputs.expected,
            pending.value,
            pending.records,
            children,
            pending.alias_root,
        )
    }
}
