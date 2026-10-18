//! One build owns all selected source exports. Private parents are never patched
//! until construction/reconstruction callbacks have ended. No guest-derived fact
//! supplies a source expectation or enlarges a certified child domain.
use super::*;
use crate::portable_macro_data::FormBridge;
use crate::portable_session::{
    SessionValue,
    construction_records::ConstructionRecords,
    deferred_factory::DeferredFactory,
    private_fixups::slot_log::{PrivateSlotLog, PrivateToken, SharedSlotLog},
};

struct Plan {
    layout: Vec<Vec<usize>>,
    order: Vec<usize>,
    expected: Vec<Option<Rc<Expectation>>>,
    exports: Vec<usize>,
    export_witness: Option<super::super::source_facts::LogicalSourceWitness>,
    root_alias: Option<usize>,
}

// Find the first native source boundary on each path. Order is the ordinary
// recipe dependency order; a repeated alias remains one identity-bearing root.
fn frontier(graph: &mut AnalysisGraph<'_>, root: Value) -> Result<Vec<usize>> {
    let mut found = Vec::new();
    let mut seen = Vec::new();
    reserve(graph.session, &mut seen, graph.recipes.len())?;
    graph.session.charge_host_work(graph.recipes.len() as u64)?;
    seen.resize(graph.recipes.len(), false);
    let mut stack = Vec::new();
    populate(graph.session, &mut stack, std::iter::once(root))?;
    while let Some(id) = stack.pop() {
        graph.session.charge_host_work(2)?;
        if let Some(boundary) = graph.source_roots.index(graph.session, id)? {
            graph.session.charge_host_work(found.len() as u64)?;
            if !found.contains(&boundary) {
                reserve(graph.session, &mut found, 1)?;
                found.push(boundary);
            }
            continue;
        }
        let old = seen
            .get_mut(id)
            .ok_or_else(|| invalid("Source frontier outside recipes"))?;
        if *old {
            continue;
        }
        *old = true;
        let recipe = graph.recipes[id]
            .as_ref()
            .ok_or_else(|| invalid("Missing source recipe"))?;
        let edges = dependencies(graph.session, recipe)?;
        populate(graph.session, &mut stack, edges.into_iter().rev())?;
    }
    Ok(found)
}

fn aliases(graph: &mut AnalysisGraph<'_>, children: &[usize]) -> Result<Vec<Value>> {
    aliases_in(graph.session, &graph.source_roots, children)
}
fn aliases_in(session: &mut Session, roots: &Captures, children: &[usize]) -> Result<Vec<Value>> {
    let mut result = Vec::new();
    reserve(session, &mut result, children.len())?;
    for child in children {
        session.charge_host_work(2)?;
        result.push(roots.roots[*child].alias);
    }
    Ok(result)
}

// Optional existing metadata trace only: distinguish conservative admission
// declines without constructing a guest value or treating a decline as success.
fn admission_decline<T>(
    graph: &AnalysisGraph<'_>,
    reason: &'static str,
    boundary: Option<usize>,
) -> Result<Option<T>> {
    if std::env::var_os("SUSS_METADATA_FUEL_TRACE").is_some() {
        eprintln!(
            "coherent-admission decline={reason} boundary={boundary:?} remaining={}",
            graph.session.operation_fuel_remaining()?
        );
    }
    Ok(None)
}

impl Plan {
    // No callbacks, records, or cache updates occur during admission. A decline
    // chooses the original whole-build route before any construction effects.
    fn admit(graph: &mut AnalysisGraph<'_>, root: Value) -> Result<Option<Self>> {
        admission_stage(graph, "entry", None);
        if !graph.capture_roots {
            return admission_decline(graph, "capture-disabled", None);
        }
        admission_stage(graph, "finish-recipes-before", None);
        graph.finish_recipes()?;
        admission_stage(graph, "finish-recipes-after", None);
        admission_stage(graph, "source-root-children-before", None);
        let layout = graph.source_root_children()?;
        admission_stage(graph, "source-root-children-after", None);
        admission_stage(graph, "frontier-before", None);
        let exports = frontier(graph, root)?;
        admission_stage(graph, "frontier-after", None);
        if exports.is_empty() {
            return admission_decline(graph, "empty-export-frontier", None);
        }
        admission_stage(graph, "order-before", None);
        let root_alias = graph.source_roots.index(graph.session, root)?;
        let mut state = Vec::new();
        reserve(graph.session, &mut state, layout.len())?;
        graph.session.charge_host_work(layout.len() as u64)?;
        state.resize(layout.len(), 0u8);
        let mut stack = Vec::new();
        populate(
            graph.session,
            &mut stack,
            exports.iter().rev().map(|id| (*id, false)),
        )?;
        let mut order = Vec::new();
        while let Some((id, exit)) = stack.pop() {
            graph.session.charge_host_work(2)?;
            if exit {
                state[id] = 2;
                reserve(graph.session, &mut order, 1)?;
                order.push(id);
            } else if state[id] != 2 {
                if state[id] == 1 {
                    return Err(invalid("Cyclic source boundary plan"));
                }
                state[id] = 1;
                populate(graph.session, &mut stack, std::iter::once((id, true)))?;
                populate(
                    graph.session,
                    &mut stack,
                    layout[id].iter().rev().map(|id| (*id, false)),
                )?;
            }
        }
        admission_stage(graph, "order-after", None);
        let mut expected: Vec<Option<Rc<Expectation>>> = Vec::new();
        reserve(graph.session, &mut expected, layout.len())?;
        graph.session.charge_host_work(layout.len() as u64)?;
        expected.resize_with(layout.len(), || None);
        let mut projection = super::super::metadata_expectations::shared::Builder::new(&graph.recipes);
        for id in &order {
            admission_stage(graph, "expectation-before", Some(*id));
            let native = graph.source_roots.roots[*id].native;
            let Some(value) = projection.project(graph.session, native)? else {
                return admission_decline(graph, "source-expectation", Some(*id));
            };
            let Some(logical) = super::super::source_facts::LogicalSourceWitness::from_shared(graph.session, value)? else {
                return admission_decline(graph, "source-expectation", Some(*id));
            };
            let witness = expectation_with_shared_witness(graph.session, &graph.source_roots, *id, &layout, logical)?;
            admission_stage(graph, "expectation-after", Some(*id));
            admission_stage(graph, "placement-before", Some(*id));
            let selected = aliases_in(graph.session, &graph.source_roots, &layout[*id])?;
            let native = graph.source_roots.roots[*id].native;
            if !super::super::private_placement::placement_in(graph.session, &graph.recipes, native, &selected, true)? {
                return admission_decline(graph, "boundary-private-placement", Some(*id));
            }
            admission_stage(graph, "placement-after", Some(*id));
            admission_stage(graph, "owner-check-before", Some(*id));
            // One owner cannot denote two different exports in this graph.
            // Otherwise publication order would choose an arbitrary candidate.
            for other in &order {
                graph.session.charge_host_work(1)?;
                if other == id {
                    break;
                }
                let previous = expected[*other].as_ref().unwrap();
                graph
                    .session
                    .charge_host_work(key_comparison_work(&previous.key, &witness.key))?;
                if previous.key == witness.key {
                    if std::env::var_os("SUSS_METADATA_FUEL_TRACE").is_some() {
                        let first = &graph.source_roots.roots[*other];
                        let second = &graph.source_roots.roots[*id];
                        eprintln!(
                            "coherent-owner-conflict kind={} first_boundary={} first_alias={} first_native={} first_children={} second_boundary={} second_alias={} second_native={} second_children={}",
                            first.owner.kind(),
                            other,
                            first.alias,
                            first.native,
                            layout[*other].len(),
                            id,
                            second.alias,
                            second.native,
                            layout[*id].len()
                        );
                    }
                    return admission_decline(graph, "duplicate-owner", Some(*id));
                }
            }
            expected[*id] = Some(witness);
            admission_stage(graph, "owner-check-after", Some(*id));
        }
        admission_stage(graph, "export-witness-before", None);
        let export_witness = if root_alias.is_some() {
            None
        } else {
            admission_stage(graph, "export-placement-before", None);
            let selected = aliases_in(graph.session, &graph.source_roots, &exports)?;
            if !super::super::private_placement::placement_in(graph.session, &graph.recipes, root, &selected, true)? {
                return admission_decline(graph, "export-private-placement", None);
            }
            admission_stage(graph, "export-placement-after", None);
            admission_stage(graph, "export-project-before", None);
            let Some(value) = projection.project(graph.session, root)?
            else {
                return admission_decline(graph, "export-projection", None);
            };
            admission_stage(graph, "export-project-after", None);
            admission_stage(graph, "export-logical-witness-before", None);
            let Some(witness) =
                super::super::source_facts::LogicalSourceWitness::from_shared(graph.session, value)?
            else {
                return admission_decline(graph, "export-logical-witness", None);
            };
            admission_stage(graph, "export-logical-witness-after", None);
            Some(witness)
        };
        admission_stage(graph, "export-witness-after", None);
        // Placement alone cannot justify bypassing an arbitrary live factory.
        // If shipped class authority is unavailable (including bootstrap setup)
        // or already changed, decline BEFORE construction and preserve the
        // original whole-build route. Later callbacks still require full final
        // class/prototype/source checks and can never trigger a fallback retry.
        admission_stage(graph, "schema-before", None);
        let preflight = graph.bridge.recording(graph.session)?;
        let records = preflight.into_records()?;
        if !records.matches_class_schemas(graph.session)? {
            return admission_decline(graph, "class-schema-authority", None);
        }
        admission_stage(graph, "schema-after", None);
        admission_stage(graph, "admitted", None);
        Ok(Some(Self {
            layout,
            order,
            expected,
            exports,
            export_witness,
            root_alias,
        }))
    }
    fn expectation(&self, id: usize) -> &Rc<Expectation> {
        self.expected[id]
            .as_ref()
            .expect("admitted source expectation")
    }
}

struct Binding {
    alias: Value,
    child: usize,
    token: PrivateToken,
}
struct Inputs {
    log: SharedSlotLog,
    bindings: Vec<Binding>,
    by_alias: Vec<Option<usize>>,
}
impl SourceSelection for Inputs {
    fn intercepts(&self, alias: Value) -> bool {
        self.by_alias.get(alias).is_some_and(Option::is_some)
    }
    fn selected(&self, session: &mut Session, alias: Value) -> Result<Option<&SessionValue>> {
        self.log.borrow().binding_owner(session)?;
        session.charge_host_work(2)?;
        let Some(Some(ordinal)) = self.by_alias.get(alias) else {
            return Ok(None);
        };
        let binding = &self.bindings[*ordinal];
        if binding.alias != alias
            || !binding.token.belongs_to(&self.log.borrow())
            || binding.token.id() != *ordinal
        {
            return Err(invalid("Foreign coherent source token"));
        }
        drop(self.log.borrow().token_handle(session, *ordinal)?);
        Ok(Some(binding.token.value()))
    }
}
struct Pending {
    value: SessionValue,
    records: ConstructionRecords,
    deferred: Vec<DeferredFactory>,
    inputs: Inputs,
    alias_root: bool,
}
enum State {
    Empty,
    Old(Rc<Certificate>),
    Fresh(Pending),
}
impl State {
    fn value(&self) -> Result<&SessionValue> {
        match self {
            Self::Old(certificate) => Ok(&certificate.value),
            Self::Fresh(pending) => Ok(&pending.value),
            Self::Empty => Err(invalid("Unbuilt coherent source dependency")),
        }
    }
}

fn construct(
    graph: &mut AnalysisGraph<'_>,
    native: Value,
    children: &[usize],
    states: &[State],
) -> Result<Pending> {
    let log = PrivateSlotLog::new(graph.session)?;
    let mut bindings = Vec::new();
    reserve(graph.session, &mut bindings, children.len())?;
    let mut by_alias = Vec::new();
    if !children.is_empty() {
        reserve(graph.session, &mut by_alias, graph.recipes.len())?;
        graph.session.charge_host_work(graph.recipes.len() as u64)?;
        by_alias.resize(graph.recipes.len(), None);
    }
    for (ordinal, child) in children.iter().enumerate() {
        graph.session.charge_host_work(2)?;
        drop(log.borrow_mut().token(graph.session, ordinal)?);
        let token = log.borrow().token_handle(graph.session, ordinal)?;
        let alias = graph.source_roots.roots[*child].alias;
        by_alias[alias] = Some(ordinal);
        bindings.push(Binding {
            alias,
            child: *child,
            token,
        });
    }
    let inputs = Inputs {
        log,
        bindings,
        by_alias,
    };
    let normal = graph.bridge.recording(graph.session)?;
    let private = normal.deferred_private_bridge(graph.session, &inputs.log)?;
    // Unselected reader occurrences use the normal recording bridge and retain
    // their callbacks. Selected children exist only as private native tokens;
    // admission excluded every hash/equality/base-inspection argument path.
    // A selected declaration field can itself alias a captured Callable/AST.
    // That is a genuine shared child root, not a private token to patch in place.
    // Bind its exact selected reference and later attach the complete child proof.
    let mut cursor = native;
    let mut direct_child = None;
    loop {
        graph.session.charge_host_work(2)?;
        if let Some(Some(ordinal)) = inputs.by_alias.get(cursor) {
            direct_child = Some(inputs.bindings[*ordinal].child);
            break;
        }
        match graph.recipes[cursor].as_ref() {
            Some(Recipe::Alias(next)) => cursor = *next,
            _ => break,
        }
    }
    let alias_root = direct_child.is_some();
    if alias_root && (children.len() != 1 || direct_child != children.first().copied()) {
        return Err(invalid("Ambiguous native coherent root alias"));
    }
    let value = match direct_child {
        Some(child) => states[child].value()?.clone(),
        None => graph.materialize_with_inputs(native, &private, None, Some((&inputs, &normal)))?,
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

fn same_children(
    graph: &mut AnalysisGraph<'_>,
    certificate: &Certificate,
    selected: &[usize],
    states: &[State],
) -> Result<bool> {
    graph.session.charge_host_work(1)?;
    let mut same = certificate.children.len() == selected.len();
    for (old, id) in certificate.children.iter().zip(selected) {
        graph.session.charge_host_work(1)?;
        // An alias may share a guest object with another genuine owner. Compare
        // CURRENT native keys first, then still perform the physical check.
        let current = graph.source_roots.roots[*id].owner.key(graph.session)?;
        graph
            .session
            .charge_host_work(key_comparison_work(&current, &old.source.key))?;
        same &= current == old.source.key;
        // Exact references, not guest equality/hash or current guest contents.
        same &= graph
            .session
            .values_identical(&old.value, states[*id].value()?)?;
    }
    Ok(same)
}

fn finish_pending(
    graph: &mut AnalysisGraph<'_>,
    mut pending: Pending,
    plan: &Plan,
    certificates: &[Option<FreshSourceProof>],
) -> Result<(SessionValue, ConstructionRecords, Vec<FreshSourceProof>, bool)> {
    let mut children = Vec::new();
    reserve(graph.session, &mut children, pending.inputs.bindings.len())?;
    // No guest code may run from this point through publication. Recheck source
    // and complete physical closure even if the candidate was checked earlier.
    for binding in &pending.inputs.bindings {
        graph.session.charge_host_work(1)?;
        let certificate = certificates[binding.child]
            .as_ref()
            .ok_or_else(|| invalid("Uncertified coherent child"))?;
        if !certificate.checked(graph.session, graph.bridge, plan.expectation(binding.child))? {
            return Err(invalid("Source child changed before final binding"));
        }
        children.push(certificate.retain(graph.session)?);
    }
    if pending.alias_root {
        graph.session.charge_host_work(3)?;
        if children.len() != 1 || pending.records.len() != 0 || !pending.deferred.is_empty() {
            return Err(invalid("Invalid coherent native alias lineage"));
        }
        // An alias is a reference selection, not mutable parent storage. Follow
        // the FINAL certified child if a later callback replaced an old child;
        // do not retain the obsolete reference chosen during initial planning.
        pending.value = children[0].value().clone();
    }
    let mut pairs = Vec::new();
    reserve(graph.session, &mut pairs, children.len())?;
    for (ordinal, (binding, child)) in pending.inputs.bindings.iter().zip(&children).enumerate() {
        graph.session.charge_host_work(3)?;
        let log = pending.inputs.log.borrow();
        if !binding.token.belongs_to(&log) || binding.token.id() != ordinal {
            return Err(invalid("Foreign final coherent source binding"));
        }
        drop(log.token_handle(graph.session, ordinal)?);
        pairs.push((binding.token.value(), child.value()));
    }
    pending
        .inputs
        .log
        .borrow()
        .apply(graph.session, &pending.value, &pairs)?;
    for child in &children {
        graph.session.charge_host_work(1)?;
        if let Some(certificate) = child.certificate() {
            // Only strict certificates grant persistent domain membership.
            // Uncached children are handled by the separate health-only input
            // check and never become CheckedChild or write-domain authority.
            graph.session.charge_host_work(2)?;
            let checked = CheckedChild { certificate: certificate.clone() };
            pending.records.attach_source_child(graph.session, &checked)?;
        }
    }
    for factory in pending.deferred {
        graph.session.charge_host_work(1)?;
        let (value, fact) = factory.finalize(graph.session, &pairs)?;
        pending.records.push(graph.session, value, fact)?;
    }
    // Pairs hold roots through all physical substitutions/fact finalizations.
    drop(pairs);
    Ok((pending.value, pending.records, children, pending.alias_root))
}

pub(in crate::portable_macro_graph) fn materialize_coherent(
    graph: &mut AnalysisGraph<'_>,
    root: Value,
    cache: &mut RootCache,
) -> Result<Option<SessionValue>> {
    let _request = graph.session.operation_budget_scope()?;
    let Some(plan) = Plan::admit(graph, root)? else {
        return Ok(None);
    };
    let mut states = Vec::new();
    reserve(graph.session, &mut states, plan.layout.len())?;
    graph.session.charge_host_work(plan.layout.len() as u64)?;
    states.resize_with(plan.layout.len(), || State::Empty);
    // Construct in the same first-occurrence dependency order. Only previously
    // retained, currently checked roots may skip their original construction.
    for id in &plan.order {
        let expected = plan.expectation(*id);
        admission_stage(graph, "candidate-lookup-before", Some(*id));
        let candidate = cache.lookup(graph.session, graph.bridge, expected)?;
        admission_stage(graph, "candidate-lookup-after", Some(*id));
        if let Some(child) = candidate {
            if same_children(graph, &child.certificate, &plan.layout[*id], &states)? {
                states[*id] = State::Old(child.certificate);
                continue;
            }
        }
        let native = graph.source_roots.roots[*id].native;
        admission_stage(graph, "construct-before", Some(*id));
        states[*id] = State::Fresh(construct(graph, native, &plan.layout[*id], &states)?);
        admission_stage(graph, "construct-after", Some(*id));
    }
    let export = if plan.root_alias.is_none() {
        admission_stage(graph, "export-construct-before", None);
        let pending = construct(graph, root, &plan.exports, &states)?;
        admission_stage(graph, "export-construct-after", None);
        Some(pending)
    } else {
        None
    };
    // Later callbacks may invalidate ANY earlier old root or shared prototype.
    // Old -> Fresh is the only transition that can run more construction. Each
    // owner transitions once. Fresh private parents keep tokens until the loop
    // ends; their physical edges can follow rebuilt children without rerunning
    // a constructor or repairing an exposed cached object.
    admission_stage(graph, "postcallback-recheck-before", None);
    loop {
        let mut rebuilt = false;
        for id in &plan.order {
            let valid = match &states[*id] {
                State::Old(old) => {
                    Certificate::checked(graph.session, graph.bridge, old, plan.expectation(*id))?
                        .is_some()
                        && same_children(graph, old, &plan.layout[*id], &states)?
                }
                State::Fresh(_) => continue,
                State::Empty => return Err(invalid("Missing coherent source root")),
            };
            if !valid {
                // No certification audit or cached validation survives callbacks.
                let native = graph.source_roots.roots[*id].native;
                states[*id] = State::Fresh(construct(graph, native, &plan.layout[*id], &states)?);
                rebuilt = true;
            }
        }
        if !rebuilt {
            break;
        }
    }
    admission_stage(graph, "postcallback-recheck-after", None);
    let mut certificates = Vec::new();
    reserve(graph.session, &mut certificates, plan.layout.len())?;
    graph.session.charge_host_work(plan.layout.len() as u64)?;
    certificates.resize_with(plan.layout.len(), || None);
    for id in &plan.order {
        admission_stage(graph, "finalize-before", Some(*id));
        let state = std::mem::replace(&mut states[*id], State::Empty);
        let certificate = match state {
            State::Old(certificate) => {
                Certificate::checked(
                    graph.session,
                    graph.bridge,
                    &certificate,
                    plan.expectation(*id),
                )?
                .ok_or_else(|| invalid("Retained source changed during finalization"))?;
                FreshSourceProof::certified(graph.session, certificate)?
            }
            State::Fresh(pending) => {
                let (value, records, children, alias_root) =
                    finish_pending(graph, pending, &plan, &certificates)?;
                admission_stage(graph, "finish-pending-after", Some(*id));
                graph.session.charge_host_work(2)?;
                FreshSourceProof::finish(graph.session, graph.bridge,
                    plan.expectation(*id).clone(), value, records, children, alias_root)?
            }
            State::Empty => return Err(invalid("Missing final coherent source root")),
        };
        certificates[*id] = Some(certificate);
        admission_stage(graph, "finalize-after", Some(*id));
    }
    let value = if let Some(export) = export {
        let (value, records, children, alias_root) =
            finish_pending(graph, export, &plan, &certificates)?;
        let logical = plan
            .export_witness
            .as_ref()
            .unwrap()
            .matches_logical_source(graph.session, graph.bridge, &value)?;
        // Do not short-circuit malformed/owner/fuel/IRQ errors after false.
        let (direct, strict) = records.fresh_direct_status(graph.session)?;
        graph.session.charge_host_work(children.len() as u64 + 1)?;
        let all_certified = children.iter().all(|child| child.certificate().is_some());
        let inputs = if alias_root {
            graph.session.charge_host_work(3)?;
            if children.len() != 1 || records.len() != 0 {
                false
            } else {
                graph
                    .session
                    .values_identical(&value, children[0].value())?
            }
        } else if all_certified {
            records.matches_recorded_inputs(graph.session, &value)?
        } else {
            records.matches_fresh_source_inputs(graph.session, &value, &children)?
        };
        let classes = records.matches_class_schemas(graph.session)?;
        let prototypes = records.matches_prototype_origins(graph.session)?;
        if !(logical && direct && inputs && classes && prototypes) {
            return Err(invalid(
                "Coherent export failed final source/storage checks",
            ));
        }
        if strict && all_certified { records.seal_source_domain(graph.session)?; }
        value
    } else {
        certificates[plan.root_alias.unwrap()]
            .as_ref()
            .unwrap()
            .value()
            .clone()
    };
    let mut updates = Vec::new();
    reserve(graph.session, &mut updates, plan.order.len())?;
    for id in &plan.order {
        graph.session.charge_host_work(1)?;
        let certificate = certificates[*id].as_ref().unwrap();
        if !certificate.checked(graph.session, graph.bridge, plan.expectation(*id))? {
            return Err(invalid("Source changed before coherent publication"));
        }
        graph.session.charge_host_work(1)?;
        if let Some(strict) = certificate.certificate() {
            graph.session.charge_host_work(2)?;
            updates.push(strict.clone());
        }
    }
    let next = cache.replacement(graph.session, &updates)?;
    // Infallible single publication after ALL fallible preparation. No callback
    // or additional debit follows the replacement; exposed old objects unchanged.
    cache.entries = next;
    Ok(Some(value))
}

#[cfg(test)]
#[path = "coherent_tests.rs"]
mod tests;
