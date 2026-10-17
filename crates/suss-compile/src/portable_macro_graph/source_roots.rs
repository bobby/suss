//! Owned native source identities and explicit retention boundaries.
//!
//! A capture is not a source/storage certificate. Its native recipe remains
//! separate from its materialization alias. No guest contents or equality enter
//! identity selection, and no child edge admits an external construction seed.
use super::{AnalysisGraph, Recipe, Result, Value, reuse::reserve};
use crate::portable::{
    hir,
    resolve::{DefinitionInfo, Global},
};
use crate::portable_session::{Session, SessionError};
use std::{rc::Rc, sync::Arc};

type Locals = Arc<std::collections::HashMap<String, Arc<hir::LocalBinding>>>;
type Fields = Arc<std::collections::HashMap<String, Arc<hir::FieldBinding>>>;

#[derive(Clone)]
pub(super) enum Owner {
    Namespace(Arc<hir::SourceNamespace>),
    Defs(Arc<hir::SourceNamespace>),
    Declaration(Global, Arc<DefinitionInfo>),
    // The ordinal refers to the FINAL selected native declaration entry, after
    // top-fn/forward-declaration/tag overlays; never a field-name inference.
    DeclarationField(Global, Arc<DefinitionInfo>, usize),
    Locals(Locals, Fields),
    Local(Arc<hir::LocalBinding>),
    Field(Arc<hir::FieldBinding>),
    Scope(Arc<hir::FunctionScope>),
    Callable(Arc<hir::SourceCallable>),
    Ast(Arc<hir::SourceAnalysis>),
}
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Key {
    Namespace(usize),
    Defs(usize),
    Declaration(Global, usize),
    DeclarationField(Global, usize, usize),
    Locals(usize, usize),
    Local(usize),
    Field(usize),
    Scope(usize),
    Callable(usize),
    Ast(usize),
}
impl Owner {
    fn key(&self, session: &mut Session) -> Result<Key> {
        session.charge_host_work(3)?;
        Ok(match self {
            Self::Namespace(v) => Key::Namespace(Arc::as_ptr(v) as usize),
            Self::Defs(v) => Key::Defs(Arc::as_ptr(v) as usize),
            Self::Declaration(g, v) | Self::DeclarationField(g, v, _) => {
                session.charge_host_work((g.namespace().len() + g.name().len()) as u64 + 1)?;
                match self {
                    Self::DeclarationField(_, _, slot) => {
                        Key::DeclarationField(g.clone(), Arc::as_ptr(v) as usize, *slot)
                    }
                    _ => Key::Declaration(g.clone(), Arc::as_ptr(v) as usize),
                }
            }
            Self::Locals(l, f) => Key::Locals(Arc::as_ptr(l) as usize, Arc::as_ptr(f) as usize),
            Self::Local(v) => Key::Local(Arc::as_ptr(v) as usize),
            Self::Field(v) => Key::Field(Arc::as_ptr(v) as usize),
            Self::Scope(v) => Key::Scope(Arc::as_ptr(v) as usize),
            Self::Callable(v) => Key::Callable(Arc::as_ptr(v) as usize),
            Self::Ast(v) => Key::Ast(Arc::as_ptr(v) as usize),
        })
    }
}
// Retention owns the source revision as part of each certificate. Pointer keys
// are indexes only; every lookup independently checks the current expectation.
#[derive(Default)]
pub(super) struct RootCache {
    entries: crate::portable::metered_map::TryOrderedMap<Key, Option<Rc<Certificate>>>,
}
struct KeyMeter<'a>(&'a mut Session);
impl crate::portable::metered_map::Context<Key> for KeyMeter<'_> {
    type Error = SessionError;
    fn charge(&mut self, units: u64) -> Result<()> {
        self.0.charge_host_work(units)
    }
    fn compare(&mut self, a: &Key, b: &Key) -> Result<std::cmp::Ordering> {
        self.0.charge_host_work(key_comparison_work(a, b))?;
        Ok(a.cmp(b))
    }
}
impl RootCache {
    fn lookup(
        &self,
        session: &mut Session,
        bridge: &crate::portable_macro_data::FormBridge,
        expected: &Rc<Expectation>,
    ) -> Result<Option<CheckedChild>> {
        let _request = session.operation_budget_scope()?;
        let key = expected.owner.key(session)?;
        session.charge_host_work(1)?;
        let candidate = self
            .entries
            .get(&key, &mut KeyMeter(session))?
            .and_then(|entry| entry.as_ref())
            .cloned();
        match candidate {
            Some(certificate) => Certificate::checked(session, bridge, &certificate, expected),
            None => Ok(None),
        }
    }
    // Called only in the callback-free final publication interval. Errors are
    // propagated, and an invalid entry explicitly invalidates its old cache slot.
    // Each inserted Some is already a COMPLETE independently checked certificate;
    // an allocation trap cannot expose a partly initialized certificate.
    fn publish(
        &mut self,
        session: &mut Session,
        bridge: &crate::portable_macro_data::FormBridge,
        expected: &Rc<Expectation>,
        certificate: &Rc<Certificate>,
    ) -> Result<bool> {
        let _request = session.operation_budget_scope()?;
        let checked = Certificate::checked(session, bridge, certificate, expected)?;
        let key = expected.owner.key(session)?;
        let valid = checked.is_some();
        session.charge_host_work(1)?;
        self.entries.insert(
            key,
            checked.map(|child| child.certificate),
            &mut KeyMeter(session),
        )?;
        Ok(valid)
    }
}
pub(super) struct Capture {
    pub(super) owner: Owner,
    pub(super) native: Value,
    pub(super) alias: Value,
}
#[derive(Default)]
pub(super) struct Captures {
    pub(super) roots: Vec<Capture>,
    // Graph-local aliases only. Persistent retention must use owned Owner keys.
    by_alias: Vec<Option<usize>>,
}
fn invalid(message: &'static str) -> SessionError {
    wasmtime::Error::msg(message).into()
}
impl Captures {
    pub(super) fn native(&self, session: &mut Session, alias: Value) -> Result<Value> {
        Ok(self
            .index(session, alias)?
            .map_or(alias, |id| self.roots[id].native))
    }
    pub(super) fn index(&self, session: &mut Session, alias: Value) -> Result<Option<usize>> {
        session.charge_host_work(2)?;
        Ok(self.by_alias.get(alias).copied().flatten())
    }
    pub(super) fn keys(&self, session: &mut Session) -> Result<Vec<Key>> {
        let mut keys = Vec::new();
        reserve(session, &mut keys, self.roots.len())?;
        for root in &self.roots {
            keys.push(root.owner.key(session)?);
        }
        Ok(keys)
    }
}
impl AnalysisGraph<'_> {
    pub(super) fn source_boundary(&mut self, owner: Owner, native: Value) -> Result<Value> {
        if !self.capture_roots {
            return Ok(native);
        }
        self.capture_source_root(owner, native)
    }
    pub(super) fn capture_source_root(&mut self, owner: Owner, native: Value) -> Result<Value> {
        // Native selection always precedes the alias. Callers inspecting a raw
        // declaration map must resolve native explicitly, never inspect an alias
        // as if it were a Recipe::Map.
        let alias = self.recipe(Recipe::Alias(native))?;
        reserve(self.session, &mut self.source_roots.roots, 1)?;
        let count = alias
            .checked_add(1)
            .ok_or_else(|| invalid("Source alias index overflow"))?;
        let extra = count.saturating_sub(self.source_roots.by_alias.len());
        reserve(self.session, &mut self.source_roots.by_alias, extra)?;
        self.session.charge_host_work(extra as u64 + 3)?;
        self.source_roots.by_alias.resize(count, None);
        let id = self.source_roots.roots.len();
        self.source_roots.roots.push(Capture {
            owner,
            native,
            alias,
        });
        self.source_roots.by_alias[alias] = Some(id);
        Ok(alias)
    }
    // Validate the FULL native graph, including dependencies that a retained
    // alias would otherwise hide. This is an iterative proof, not a cut-through
    // materializer traversal. Called before source-root interception is enabled.
    pub(super) fn source_root_children(&mut self) -> Result<Vec<Vec<usize>>> {
        self.finish_recipes()?;
        let count = self.recipes.len();
        let mut children = Vec::new();
        reserve(self.session, &mut children, self.source_roots.roots.len())?;
        let mut state = Vec::new();
        reserve(self.session, &mut state, count)?;
        self.session.charge_host_work(count as u64)?;
        state.resize(count, 0u8);
        let mut stack = Vec::new();
        for root in &self.source_roots.roots {
            reserve(self.session, &mut stack, 1)?;
            stack.push((root.native, false));
            while let Some((id, exit)) = stack.pop() {
                self.session.charge_host_work(3)?;
                let slot = state
                    .get_mut(id)
                    .ok_or_else(|| invalid("Unknown source recipe edge"))?;
                if exit {
                    *slot = 2;
                    continue;
                }
                if *slot == 2 {
                    continue;
                }
                if *slot == 1 {
                    return Err(invalid("Cyclic native source-root graph"));
                }
                *slot = 1;
                let recipe = self.recipes[id]
                    .as_ref()
                    .ok_or_else(|| invalid("Unfinished source-root recipe"))?;
                let deps = dependencies(self.session, recipe)?;
                reserve(self.session, &mut stack, deps.len() + 1)?;
                stack.push((id, true));
                stack.extend(deps.into_iter().rev().map(|id| (id, false)));
            }
        }
        // The complete recipe DAG is acyclic. Boundary discovery is separate:
        // descend only to the next explicit captured-root alias, keeping the
        // certificate graph compact instead of expanding shared descendants.
        for (root_id, root) in self.source_roots.roots.iter().enumerate() {
            let mut found = Vec::new();
            self.session.charge_host_work(count as u64)?;
            state.fill(0);
            reserve(self.session, &mut stack, 1)?;
            stack.push((root.native, false));
            while let Some((id, _)) = stack.pop() {
                self.session.charge_host_work(3)?;
                if let Some(child) = self.source_roots.index(self.session, id)? {
                    if child == root_id {
                        return Err(invalid("Self-referential source-root capture"));
                    }
                    // Prepay every possible comparison before the bounded scan.
                    self.session.charge_host_work(found.len() as u64)?;
                    if !found.contains(&child) {
                        reserve(self.session, &mut found, 1)?;
                        found.push(child);
                    }
                    continue;
                }
                if state[id] != 0 {
                    continue;
                }
                state[id] = 1;
                let deps = dependencies(self.session, self.recipes[id].as_ref().unwrap())?;
                reserve(self.session, &mut stack, deps.len())?;
                stack.extend(deps.into_iter().rev().map(|id| (id, false)));
            }
            children.push(found);
        }
        Ok(children)
    }
}
pub(super) fn dependencies(session: &mut Session, recipe: &Recipe) -> Result<Vec<Value>> {
    let count = match recipe {
        Recipe::Alias(_) => 1,
        Recipe::List(v)
        | Recipe::Vector(v)
        | Recipe::ReaderVector(v)
        | Recipe::Set(v)
        | Recipe::ReaderSet(v) => v.len(),
        Recipe::Map(v) | Recipe::ReaderMap(v) => v
            .len()
            .checked_mul(2)
            .ok_or_else(|| invalid("Source-root edge overflow"))?,
        Recipe::Metadata { set_items, .. } => set_items
            .len()
            .checked_add(2)
            .ok_or_else(|| invalid("Source-root edge overflow"))?,
        _ => 0,
    };
    let mut result = Vec::new();
    reserve(session, &mut result, count)?;
    session.charge_host_work(count as u64 + 1)?;
    match recipe {
        Recipe::Alias(id) => result.push(*id),
        Recipe::List(v)
        | Recipe::Vector(v)
        | Recipe::ReaderVector(v)
        | Recipe::Set(v)
        | Recipe::ReaderSet(v) => result.extend_from_slice(v),
        Recipe::Map(v) | Recipe::ReaderMap(v) => {
            for (k, v) in v {
                result.extend([*k, *v]);
            }
        }
        Recipe::Metadata {
            value,
            metadata,
            set_items,
            ..
        } => {
            result.push(*value);
            result.extend_from_slice(set_items);
            result.push(*metadata);
        }
        _ => {}
    }
    Ok(result)
}

// Independent source expectation. Its constructor consumes native recipes only;
// a later physical read can reject it but cannot refresh its contents.
pub(super) struct Expectation {
    owner: Owner,
    key: Key,
    witness: super::source_facts::LogicalSourceWitness,
    // Canonical boundary ordinal -> independently owned native source identity.
    // These are selected from recipes, never from the supplied guest children.
    children: Vec<Key>,
}
pub(crate) struct Certificate {
    source: Rc<Expectation>,
    value: crate::portable_session::SessionValue,
    records: crate::portable_session::construction_records::ConstructionRecords,
    children: Vec<Rc<Certificate>>,
}
// Only this module can mint a checked child; neither an arbitrary SessionValue
// nor an ExternalSeed can be converted into one. The expected current source is
// checked independently as well as the complete prior certificate.
pub(crate) struct CheckedChild {
    certificate: Rc<Certificate>,
}
impl CheckedChild {
    pub(crate) fn value(&self) -> &crate::portable_session::SessionValue {
        &self.certificate.value
    }
    pub(crate) fn domain(&self) -> Arc<crate::portable_session::write_domain::WriteDomain> {
        self.certificate.records.write_domain()
    }
}
// A private input is selected by a native recipe alias, not by comparing guest
// values. Each distinct boundary owns a fresh token; repeated occurrences of the
// same alias deliberately share that token. Only the materializer may inject it.
struct SourceInput {
    alias: Value,
    ordinal: usize,
    token: crate::portable_session::private_fixups::slot_log::PrivateToken,
    child: CheckedChild,
    expected: Rc<Expectation>,
}
pub(super) struct SourceInputs {
    expected: Rc<Expectation>,
    log: crate::portable_session::private_fixups::slot_log::SharedSlotLog,
    inputs: Vec<SourceInput>,
    by_alias: Vec<Option<usize>>,
}
// Only native recipe-directed materialization can construct this proof. A list
// of individually valid children cannot establish their physical parent slots.
struct BoundParent {
    source: Rc<Expectation>,
    value: crate::portable_session::SessionValue,
    records: crate::portable_session::construction_records::ConstructionRecords,
    children: Vec<CheckedChild>,
}
impl SourceInputs {
    pub(super) fn intercepts(&self, alias: Value) -> bool {
        self.by_alias.get(alias).is_some_and(Option::is_some)
    }
    pub(super) fn selected(
        &self,
        session: &mut Session,
        alias: Value,
    ) -> Result<Option<&crate::portable_session::SessionValue>> {
        self.log.borrow().binding_owner(session)?;
        session.charge_host_work(2)?;
        if let Some(Some(index)) = self.by_alias.get(alias) {
            let input = &self.inputs[*index];
            session.charge_host_work(3)?;
            if input.alias != alias
                || !input.token.belongs_to(&self.log.borrow())
                || input.token.id() != input.ordinal
            {
                return Err(invalid("Foreign source-edge token"));
            }
            return Ok(Some(input.token.value()));
        }
        Ok(None)
    }
}
impl AnalysisGraph<'_> {
    fn source_root_inputs(
        &mut self,
        id: usize,
        children: Vec<CheckedChild>,
    ) -> Result<Option<SourceInputs>> {
        let _request = self.session.operation_budget_scope()?;
        let layout = self.source_root_children()?;
        let Some(expected) = self.source_root_expectation_with_layout(id, &layout)? else {
            return Ok(None);
        };
        self.session.charge_host_work(2)?;
        if children.len() != expected.children.len() {
            return Ok(None);
        }
        // Fully recheck every child after callbacks before minting ANY token.
        let mut checked = Vec::new();
        reserve(self.session, &mut checked, children.len())?;
        let mut valid = true;
        let mut done = Vec::new();
        let mut audit =
            crate::portable_session::certification_audit::CertificationAudit::new(self.session)?;
        for (ordinal, child) in children.into_iter().enumerate() {
            self.session.charge_host_work(2)?;
            let child_expected = self
                .source_root_expectation_with_layout(layout[id][ordinal], &layout)?
                .ok_or_else(|| invalid("Unsupported source-edge expectation"))?;
            match Certificate::checked_into(
                self.session,
                self.bridge,
                &child.certificate,
                &child_expected,
                &mut done,
                &mut audit,
            )? {
                Some(child) => checked.push(Some((child, child_expected))),
                None => {
                    valid = false;
                    checked.push(None);
                }
            }
        }
        drop(audit);
        drop(done);
        if !valid {
            return Ok(None);
        }
        let mut aliases = Vec::new();
        reserve(self.session, &mut aliases, layout[id].len())?;
        for child in &layout[id] {
            self.session.charge_host_work(2)?;
            aliases.push(self.source_roots.roots[*child].alias);
        }
        let native = self.source_roots.roots[id].native;
        self.session.charge_host_work(aliases.len() as u64 + 1)?;
        if aliases.contains(&native) || !self.private_source_placement_for(native, &aliases)? {
            return Ok(None);
        }
        let mut by_alias = Vec::new();
        reserve(self.session, &mut by_alias, self.recipes.len())?;
        self.session.charge_host_work(self.recipes.len() as u64)?;
        by_alias.resize(self.recipes.len(), None);
        let log =
            crate::portable_session::private_fixups::slot_log::PrivateSlotLog::new(self.session)?;
        let mut inputs = Vec::new();
        reserve(self.session, &mut inputs, checked.len())?;
        for (ordinal, child) in checked.into_iter().enumerate() {
            let mut slots = log.borrow_mut();
            let _value = slots.token(self.session, ordinal)?;
            let token = slots.token_handle(self.session, ordinal)?;
            self.session.charge_host_work(3)?;
            let alias = self.source_roots.roots[layout[id][ordinal]].alias;
            by_alias[alias] = Some(ordinal);
            let (child, expected) = child.unwrap();
            inputs.push(SourceInput {
                alias,
                ordinal,
                token,
                child,
                expected,
            });
        }
        Ok(Some(SourceInputs {
            expected,
            log,
            inputs,
            by_alias,
        }))
    }
    fn materialize_source_parent(
        &mut self,
        id: usize,
        children: Vec<CheckedChild>,
    ) -> Result<Option<BoundParent>> {
        let _request = self.session.operation_budget_scope()?;
        let Some(inputs) = self.source_root_inputs(id, children)? else {
            return Ok(None);
        };
        let normal = self.bridge.recording(self.session)?;
        let private = normal.deferred_private_bridge(self.session, &inputs.log)?;
        self.session.charge_host_work(2)?;
        let root = self.source_roots.roots[id].native;
        // This is the only injection route: SourceInputs selects a fresh token
        // by the exact native boundary alias. Factories see no child target until
        // the callback-free finalization below. Repeated occurrences retain the
        // same alias binding while reader occurrence execution remains unchanged.
        let value = self.materialize_with_inputs(root, &private, None, Some((&inputs, &normal)))?;
        drop(normal);
        let (mut records, deferred) = private.into_deferred_records()?;
        let mut checked = Vec::new();
        reserve(self.session, &mut checked, inputs.inputs.len())?;
        let mut valid = true;
        // All ordinary construction callbacks have finished. Even children that
        // were checked before construction must now be independently rechecked.
        let batch = Certificate::checked_batch(
            self.session,
            self.bridge,
            inputs
                .inputs
                .iter()
                .map(|input| (&input.child.certificate, &input.expected)),
        )?;
        for child in batch {
            self.session.charge_host_work(2)?;
            valid &= child.is_some();
            checked.push(child);
        }
        // Class authority is not inferred from successful native construction.
        valid &= records.matches_class_schemas(self.session)?;
        if !valid {
            return Ok(None);
        }
        let mut pairs = Vec::new();
        reserve(self.session, &mut pairs, inputs.inputs.len())?;
        for input in &inputs.inputs {
            self.session.charge_host_work(3)?;
            let slots = inputs.log.borrow();
            if !input.token.belongs_to(&slots) || input.token.id() != input.ordinal {
                return Err(invalid("Foreign final source-edge binding"));
            }
            // Recheck the original native empty-array token fact before use.
            drop(slots.token_handle(self.session, input.ordinal)?);
            pairs.push((input.token.value(), input.child.value()));
        }
        inputs.log.borrow().apply(self.session, &value, &pairs)?;
        // Child domains must close the parent domain before any snapshot is
        // enrolled. Native fact inputs are finalized from the SAME typed pairs
        // used for physical substitution, never from the patched guest object.
        let mut children = Vec::new();
        reserve(self.session, &mut children, checked.len())?;
        for child in checked {
            let child = child.unwrap();
            records.attach_source_child(self.session, &child)?;
            children.push(child);
        }
        for factory in deferred {
            self.session.charge_host_work(1)?;
            let (value, fact) = factory.finalize(self.session, &pairs)?;
            records.push(self.session, value, fact)?;
        }
        Ok(Some(BoundParent {
            source: inputs.expected,
            value,
            records,
            children,
        }))
    }
    pub(super) fn source_root_expectation(&mut self, id: usize) -> Result<Option<Rc<Expectation>>> {
        let layout = self.source_root_children()?;
        self.source_root_expectation_with_layout(id, &layout)
    }
    // Invocation-local native plan only. Callers compute this layout after
    // finish_recipes and do not add captures/recipes while borrowing it. Guest
    // callbacks cannot mutate these owned native recipes. Witness construction
    // and every current Certificate check remain independent and fallible.
    fn source_root_expectation_with_layout(
        &mut self,
        id: usize,
        layout: &[Vec<usize>],
    ) -> Result<Option<Rc<Expectation>>> {
        self.session.charge_host_work(2)?;
        let mut children = Vec::new();
        let selected = layout
            .get(id)
            .ok_or_else(|| invalid("Unknown source-root capture"))?;
        reserve(self.session, &mut children, selected.len())?;
        for child in selected {
            self.session.charge_host_work(2)?;
            children.push(self.source_roots.roots[*child].owner.key(self.session)?);
        }
        let capture = self
            .source_roots
            .roots
            .get(id)
            .ok_or_else(|| invalid("Unknown source-root capture"))?;
        let Some(form) =
            super::metadata_expectations::project(self.session, &self.recipes, capture.native)?
        else {
            return Ok(None);
        };
        let Some(witness) =
            super::source_facts::LogicalSourceWitness::from_normalized_source(self.session, form)?
        else {
            return Ok(None);
        };
        let key = capture.owner.key(self.session)?;
        self.session.charge_host_work(
            (std::mem::size_of::<Expectation>() + 2 * std::mem::size_of::<usize>()) as u64 + 1,
        )?;
        Ok(Some(Rc::new(Expectation {
            owner: capture.owner.clone(),
            key,
            witness,
            children,
        })))
    }
}
impl Certificate {
    fn finish_bound(
        session: &mut Session,
        bridge: &crate::portable_macro_data::FormBridge,
        bound: BoundParent,
    ) -> Result<Option<Rc<Self>>> {
        Self::finish_complete(
            session,
            bridge,
            bound.source,
            bound.value,
            bound.records,
            bound.children,
            true,
        )
    }
    // Inventory-only construction is restricted to the negative/closure tests.
    // Production must supply the private native materialization lineage proof.
    #[cfg(test)]
    fn finish(
        session: &mut Session,
        bridge: &crate::portable_macro_data::FormBridge,
        source: Rc<Expectation>,
        value: crate::portable_session::SessionValue,
        records: crate::portable_session::construction_records::ConstructionRecords,
        children: Vec<CheckedChild>,
    ) -> Result<Option<Rc<Self>>> {
        Self::finish_complete(session, bridge, source, value, records, children, false)
    }
    fn finish_complete(
        session: &mut Session,
        bridge: &crate::portable_macro_data::FormBridge,
        source: Rc<Expectation>,
        value: crate::portable_session::SessionValue,
        mut records: crate::portable_session::construction_records::ConstructionRecords,
        children: Vec<CheckedChild>,
        attached: bool,
    ) -> Result<Option<Rc<Self>>> {
        let _request = session.operation_budget_scope()?;
        // Check coverage and source identity BEFORE attaching any domain. The
        // vector position is the canonical native boundary ordinal, not a
        // caller-selected field name or logical-equality match.
        session.charge_host_work(2)?;
        if source.children.len() != children.len() {
            return Ok(None);
        }
        let mut matches = true;
        for (expected, child) in source.children.iter().zip(&children) {
            session.charge_host_work(2)?;
            session.inspect(child.value(), |_, _| Ok(()))?;
            session
                .charge_host_work(key_comparison_work(expected, &child.certificate.source.key))?;
            matches &= *expected == child.certificate.source.key;
        }
        if !matches {
            return Ok(None);
        }
        let mut owned = Vec::new();
        reserve(session, &mut owned, children.len())?;
        for child in children {
            if !attached {
                records.attach_source_child(session, &child)?;
            }
            session.charge_host_work(1)?;
            owned.push(child.certificate);
        }
        session.charge_host_work(
            (std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>()) as u64,
        )?;
        let certificate = Rc::new(Self {
            source,
            value,
            records,
            children: owned,
        });
        if !Self::validate(session, bridge, &certificate)? {
            return Ok(None);
        }
        // Membership can no longer grow after the complete independent source
        // and storage checks. Guest payload remains mutable and is revalidated.
        certificate.records.seal_source_domain(session)?;
        Ok(Some(certificate))
    }
    pub(super) fn checked(
        session: &mut Session,
        bridge: &crate::portable_macro_data::FormBridge,
        certificate: &Rc<Self>,
        expected: &Rc<Expectation>,
    ) -> Result<Option<CheckedChild>> {
        let mut result =
            Self::checked_batch(session, bridge, std::iter::once((certificate, expected)))?;
        Ok(result.pop().unwrap())
    }
    // The caller must not invoke guest code during this entire batch. All state
    // is owned here and dropped before returning, including on errors.
    fn checked_batch<'a>(
        session: &mut Session,
        bridge: &crate::portable_macro_data::FormBridge,
        inputs: impl IntoIterator<Item = (&'a Rc<Self>, &'a Rc<Expectation>)>,
    ) -> Result<Vec<Option<CheckedChild>>> {
        let _request = session.operation_budget_scope()?;
        let mut done = Vec::new();
        let mut audit =
            crate::portable_session::certification_audit::CertificationAudit::new(session)?;
        let mut results = Vec::new();
        for (certificate, expected) in inputs {
            reserve(session, &mut results, 1)?;
            let result = Self::checked_into(
                session,
                bridge,
                certificate,
                expected,
                &mut done,
                &mut audit,
            )?;
            session.charge_host_work(1)?;
            results.push(result);
        }
        Ok(results)
    }
    fn checked_into(
        session: &mut Session,
        bridge: &crate::portable_macro_data::FormBridge,
        certificate: &Rc<Self>,
        expected: &Rc<Expectation>,
        done: &mut Vec<(Rc<Self>, bool)>,
        audit: &mut crate::portable_session::certification_audit::CertificationAudit,
    ) -> Result<Option<CheckedChild>> {
        session.charge_host_work(2)?;
        session.inspect(&certificate.value, |_, _| Ok(()))?;
        let owner = expected.owner.key(session)?;
        // Key comparison includes genuine owned revisions and field ordinals;
        // logical equivalence cannot substitute for a source-owner mismatch.
        session.charge_host_work(key_comparison_work(&owner, &certificate.source.key))?;
        let same = owner == certificate.source.key;
        let valid = Self::validate_into(session, bridge, certificate, done, audit)?;
        let current =
            expected
                .witness
                .matches_logical_source(session, bridge, &certificate.value)?;
        if !(same && valid && current) {
            return Ok(None);
        }
        session.charge_host_work(1)?;
        Ok(Some(CheckedChild {
            certificate: certificate.clone(),
        }))
    }
    fn validate(
        session: &mut Session,
        bridge: &crate::portable_macro_data::FormBridge,
        root: &Rc<Self>,
    ) -> Result<bool> {
        let _request = session.operation_budget_scope()?;
        let mut done = Vec::new();
        let mut audit =
            crate::portable_session::certification_audit::CertificationAudit::new(session)?;
        Self::validate_into(session, bridge, root, &mut done, &mut audit)
    }
    fn validate_into(
        session: &mut Session,
        bridge: &crate::portable_macro_data::FormBridge,
        root: &Rc<Self>,
        done: &mut Vec<(Rc<Self>, bool)>,
        audit: &mut crate::portable_session::certification_audit::CertificationAudit,
    ) -> Result<bool> {
        let mut stack = Vec::new();
        reserve(session, &mut stack, 1)?;
        session.charge_host_work(1)?;
        stack.push((root.clone(), false));
        while let Some((certificate, exit)) = stack.pop() {
            session.charge_host_work(3)?;
            // Ownership precedes any invocation-local validation reuse.
            session.inspect(&certificate.value, |_, _| Ok(()))?;
            let pointer = Rc::as_ptr(&certificate);
            let mut seen = false;
            for (old, _) in done.iter() {
                session.charge_host_work(1)?;
                if Rc::as_ptr(old) == pointer {
                    seen = true;
                    break;
                }
            }
            if seen {
                continue;
            }
            if !exit {
                reserve(session, &mut stack, certificate.children.len() + 1)?;
                session.charge_host_work(certificate.children.len() as u64 + 1)?;
                stack.push((certificate.clone(), true));
                for child in certificate.children.iter().rev() {
                    stack.push((child.clone(), false));
                }
                continue;
            }
            session.charge_host_work(2)?;
            let mut valid = certificate.source.children.len() == certificate.children.len();
            for (expected, child) in certificate
                .source
                .children
                .iter()
                .zip(&certificate.children)
            {
                session.charge_host_work(key_comparison_work(expected, &child.source.key))?;
                valid &= *expected == child.source.key;
            }
            for child in &certificate.children {
                session.charge_host_work(1)?;
                let child_ptr = Rc::as_ptr(child);
                let mut found = None;
                for (old, result) in done.iter() {
                    session.charge_host_work(1)?;
                    if Rc::as_ptr(old) == child_ptr {
                        found = Some(*result);
                        break;
                    }
                }
                valid &= found.ok_or_else(|| invalid("Unvalidated source child"))?;
            }
            // No short circuit suppresses later native authority/errors.
            valid &= certificate.source.witness.matches_logical_source(
                session,
                bridge,
                &certificate.value,
            )?;
            valid &= certificate.records.matches_direct_facts(session)?;
            valid &= certificate
                .records
                .matches_recorded_inputs(session, &certificate.value)?;
            valid &= certificate
                .records
                .matches_class_schemas_audited(session, audit)?;
            valid &= certificate
                .records
                .matches_prototype_origins_audited(session, audit)?;
            reserve(session, done, 1)?;
            session.charge_host_work(1)?;
            done.push((certificate.clone(), valid));
        }
        // A previously completed root need not be the last completed entry.
        for (pointer, result) in done.iter() {
            session.charge_host_work(1)?;
            if Rc::ptr_eq(pointer, root) {
                return Ok(*result);
            }
        }
        Err(invalid("Unvalidated source root"))
    }
}
fn key_comparison_work(a: &Key, b: &Key) -> u64 {
    let text = |key: &Key| match key {
        Key::Declaration(g, _) | Key::DeclarationField(g, _, _) => {
            g.namespace().len() + g.name().len()
        }
        _ => 0,
    };
    (text(a) + text(b)) as u64 + 4
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        portable::resolve::Phase, portable_macro_data::FormBridge, portable_session::SessionValue,
    };

    // Real analyzed declaration selection, not a caller-supplied owner/value
    // pair. The native final map determines both the source slot and recipe.
    fn declaration(
        graph: &mut AnalysisGraph<'_>,
        name: &str,
    ) -> (usize, usize, Vec<(Value, Value)>) {
        let snapshot = graph.session.compilation_snapshot();
        let source_phase = graph.session.phase();
        declaration_from_source(graph, &snapshot.environment, source_phase, name)
    }
    // Only immutable analyzed source facts cross this boundary. Guest values
    // always belong to the independent Macro transport store.
    fn declaration_from_source(
        graph: &mut AnalysisGraph<'_>,
        environment: &crate::portable::resolve::Environment,
        source_phase: Phase,
        name: &str,
    ) -> (usize, usize, Vec<(Value, Value)>) {
        let ns = hir::SourceNamespace::capture(environment, source_phase);
        let (global, revision) = ns
            .declarations
            .iter()
            .find(|(g, _)| g.name() == name)
            .unwrap();
        graph.capture_roots = true;
        let root = graph.declaration(global, Some(revision), 0).unwrap();
        graph.finish_recipes().unwrap();
        let id = graph
            .source_roots
            .index(graph.session, root)
            .unwrap()
            .unwrap();
        let Owner::Declaration(retained_global, retained_revision) =
            &graph.source_roots.roots[id].owner
        else {
            panic!("declaration capture must retain a declaration owner");
        };
        assert_eq!(retained_global.phase(), source_phase);
        assert_eq!(retained_global, global);
        assert!(
            Arc::ptr_eq(retained_revision, revision),
            "transport must retain the supplied native source revision, not its local duplicate"
        );
        let native = graph.source_roots.native(graph.session, root).unwrap();
        let entries = match graph.recipes[native].as_ref().unwrap() {
            Recipe::Map(entries) => entries.clone(),
            _ => panic!("native declaration must remain a map"),
        };
        let meta_key = graph.keyword("meta").unwrap();
        let meta = entries.iter().find(|(key, _)| *key == meta_key).unwrap().1;
        let meta_id = graph
            .source_roots
            .index(graph.session, meta)
            .unwrap()
            .unwrap();
        (id, meta_id, entries)
    }
    fn standalone(graph: &mut AnalysisGraph<'_>, id: usize) -> Rc<Certificate> {
        let layout = graph.source_root_children().unwrap();
        standalone_with_layout(graph, id, &layout)
    }
    fn standalone_with_layout(
        graph: &mut AnalysisGraph<'_>,
        id: usize,
        layout: &[Vec<usize>],
    ) -> Rc<Certificate> {
        let expected = graph
            .source_root_expectation_with_layout(id, layout)
            .unwrap()
            .unwrap();
        let root = graph.source_roots.roots[id].alias;
        let recording = graph.bridge.recording(graph.session).unwrap();
        let value = graph.materialize_using(root, &recording).unwrap();
        Certificate::finish(
            graph.session,
            graph.bridge,
            expected,
            value,
            recording.into_records().unwrap(),
            Vec::new(),
        )
        .unwrap()
        .unwrap()
    }
    fn parent(
        graph: &mut AnalysisGraph<'_>,
        id: usize,
        child_id: usize,
        entries: Vec<(Value, Value)>,
        child: &Rc<Certificate>,
    ) -> Rc<Certificate> {
        let layout = graph.source_root_children().unwrap();
        let mut supplied = Vec::new();
        let mut imported = Vec::new();
        for boundary in &layout[id] {
            let certificate = if *boundary == child_id {
                child.clone()
            } else {
                standalone_with_layout(graph, *boundary, &layout)
            };
            let expected = graph
                .source_root_expectation_with_layout(*boundary, &layout)
                .unwrap()
                .unwrap();
            supplied.push(
                Certificate::checked(graph.session, graph.bridge, &certificate, &expected)
                    .unwrap()
                    .unwrap(),
            );
            imported.push((graph.source_roots.roots[*boundary].alias, certificate));
        }
        let recording = graph.bridge.recording(graph.session).unwrap();
        let mut actual = Vec::new();
        for (key, value) in entries {
            let key = graph.materialize_using(key, &recording).unwrap();
            let value = if let Some((_, certificate)) =
                imported.iter().find(|(alias, _)| *alias == value)
            {
                certificate.value.clone()
            } else {
                graph.materialize_using(value, &recording).unwrap()
            };
            actual.push((key, value));
        }
        let value = recording.map_values(graph.session, &actual).unwrap();
        let records = recording.into_records().unwrap();
        assert!(
            !records
                .matches_recorded_inputs(graph.session, &value)
                .unwrap(),
            "an unrecorded child is not an arbitrary external seed"
        );
        let expected = graph
            .source_root_expectation_with_layout(id, &layout)
            .unwrap()
            .unwrap();
        Certificate::finish(
            graph.session,
            graph.bridge,
            expected,
            value,
            records,
            supplied,
        )
        .unwrap()
        .unwrap()
    }
    fn invoke(s: &mut Session, source: &str, value: &SessionValue) {
        let f = s.eval(source).unwrap();
        drop(s.invoke(&f, &[value]).unwrap());
    }
    #[test]
    fn checked_batch_rechecks_expected_source_and_releases_state_after_errors() {
        let mut session = Session::new_macro().unwrap();
        session
            .eval("(def ^{:audit {:x 17}} batch-a 1) (def ^{:audit {:x 17}} batch-b 1)")
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let (certificate, wrong) = {
            let _request = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            let (_, a, _) = declaration(&mut graph, "batch-a");
            let (_, b, _) = declaration(&mut graph, "batch-b");
            let certificate = standalone(&mut graph, a);
            let wrong = graph.source_root_expectation(b).unwrap().unwrap();
            (certificate, wrong)
        };
        session.collect().unwrap();
        // Repeating the same genuine root exercises a completed-root hit; its
        // independent expected owner must still be checked on every occurrence.
        let result = Certificate::checked_batch(
            &mut session,
            &bridge,
            [
                (&certificate, &certificate.source),
                (&certificate, &wrong),
                (&certificate, &certificate.source),
            ],
        )
        .unwrap();
        assert!(result[0].is_some());
        assert!(result[1].is_none());
        assert!(result[2].is_some());
        drop(result);
        // A later error is not hidden by the earlier false result. Inject the
        // interrupt between inputs, not before the initial source mismatch.
        let interrupt = session.interrupt_handle();
        let mut index = 0;
        let inputs = std::iter::from_fn(|| {
            index += 1;
            match index {
                1 => Some((&certificate, &wrong)),
                2 => {
                    interrupt.interrupt();
                    Some((&certificate, &certificate.source))
                }
                _ => None,
            }
        });
        match Certificate::checked_batch(&mut session, &bridge, inputs) {
            Err(error) => assert!(error.is_interrupt()),
            Ok(_) => panic!("later interrupt must propagate after false"),
        }
        assert!(
            Certificate::checked(&mut session, &bridge, &certificate, &certificate.source)
                .unwrap()
                .is_some()
        );
        // Exercise a later fuel failure with exactly the same internal batch
        // state, without replenishing any enclosing operation budget.
        session
            .with_operation_budget(|session| {
                let mut done = Vec::new();
                let mut audit =
                    crate::portable_session::certification_audit::CertificationAudit::new(session)?;
                assert!(
                    Certificate::checked_into(
                        session,
                        &bridge,
                        &certificate,
                        &wrong,
                        &mut done,
                        &mut audit
                    )?
                    .is_none()
                );
                let remaining = session.operation_fuel_remaining()?;
                session.charge_host_work(remaining)?;
                assert!(matches!(
                    Certificate::checked_into(
                        session,
                        &bridge,
                        &certificate,
                        &certificate.source,
                        &mut done,
                        &mut audit
                    ),
                    Err(SessionError::Trap(_))
                ));
                Ok(())
            })
            .unwrap();
        assert!(
            Certificate::checked(&mut session, &bridge, &certificate, &certificate.source)
                .unwrap()
                .is_some()
        );
        // A different store cannot use even a valid, live completed certificate.
        let mut foreign = Session::new_macro().unwrap();
        let foreign_bridge = FormBridge::new(&mut foreign).unwrap();
        assert!(matches!(
            Certificate::checked_batch(
                &mut foreign,
                &foreign_bridge,
                [(&certificate, &certificate.source)]
            ),
            Err(SessionError::ForeignValue)
        ));
        session.collect().unwrap();
        assert!(
            Certificate::checked(&mut session, &bridge, &certificate, &certificate.source)
                .unwrap()
                .is_some()
        );
    }
    #[test]
    fn runtime_keyword_warming_does_not_authenticate_source_factory_facts() {
        let mut session = Session::new_repl().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let hash = session.eval("(fn [key] (hash key))").unwrap();
        let _request = session.operation_budget_scope().unwrap();
        let recording = bridge.recording(&mut session).unwrap();
        let mut graph = AnalysisGraph::new(&recording, &mut session);
        // Reader keyword, not an analyzer-literal prehashed keyword.
        let form = suss_reader::forms::read_forms(":runtime-source-memo")
            .unwrap()
            .remove(0);
        let recipe = graph.form(&form, 0).unwrap();
        graph.finish_recipes().unwrap();
        let value = graph.materialize_using(recipe, &recording).unwrap();
        drop(graph);
        let records = recording.into_records().unwrap();
        assert!(records.matches_direct_facts(&mut session).unwrap());
        drop(session.invoke(&hash, &[&value]).unwrap());
        session.collect().unwrap();
        assert!(
            !records.matches_direct_facts(&mut session).unwrap(),
            "Runtime cache writes have no Macro protected-store receipt"
        );
    }
    #[test]
    fn source_parent_lineage_keeps_selected_identity_and_rechecks_after_real_hash_callback() {
        for source_phase in [Phase::Runtime, Phase::Macro] {
            let mut source = match source_phase {
                Phase::Runtime => Session::new_repl().unwrap(),
                Phase::Macro => Session::new_macro().unwrap(),
            };
            source
                .eval("(def ^{:audit {:x 17}} lineage-source 1)")
                .unwrap();
            let source_snapshot = source.compilation_snapshot();
            let mut session = Session::new_macro().unwrap();
            session
                .eval(
                    r#"
              (def ^{:audit {:x 17}} lineage-source 1)
              (def lineage-target nil)
              (def lineage-armed false)
              (def lineage-callbacks 0)
              (def lineage-original-hash hash)
            "#,
                )
                .unwrap();
            session.enter_namespace("suss.core").unwrap();
            session
                .eval(
                    r#"
              (def hash (fn [key]
                (do
                  (if user/lineage-armed
                    (do (set! user/lineage-armed false)
                        (set! user/lineage-callbacks (+ user/lineage-callbacks 1))
                        (set! (.-__hash user/lineage-target) 991))
                    nil)
                  (user/lineage-original-hash key))))
            "#,
                )
                .unwrap();
            session.enter_namespace("user").unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let arm = session
                .eval("(fn [target] (do (set! lineage-target target) (set! lineage-armed true)))")
                .unwrap();
            let inspect = session
                .eval("(fn [parent expected] (identical? (:meta parent) expected))")
                .unwrap();
            let _request = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            let (root, meta, entries) = declaration_from_source(
                &mut graph,
                &source_snapshot.environment,
                source_phase,
                "lineage-source",
            );
            assert!(
                entries.len() > 8,
                "the real parent factory must invoke captured public hash"
            );
            let layout = graph.source_root_children().unwrap();
            let mut certificates = Vec::new();
            for child in &layout[root] {
                certificates.push(standalone_with_layout(&mut graph, *child, &layout));
            }
            let meta_slot = layout[root].iter().position(|id| *id == meta).unwrap();
            let children = certificates
                .iter()
                .map(|certificate| {
                    Certificate::checked(
                        graph.session,
                        graph.bridge,
                        certificate,
                        &certificate.source,
                    )
                    .unwrap()
                    .unwrap()
                })
                .collect();
            let bound = graph
                .materialize_source_parent(root, children)
                .unwrap()
                .unwrap();
            graph.session.collect().unwrap();
            let observed = graph
                .session
                .invoke(&inspect, &[&bound.value, &certificates[meta_slot].value])
                .unwrap();
            assert_eq!(
                crate::portable_repl::display(graph.session, &observed).unwrap(),
                "true"
            );
            let parent = Certificate::finish_bound(graph.session, graph.bridge, bound)
                .unwrap()
                .unwrap();
            // The real parent and its exported metadata certificate share a
            // descendant. Complete validation must reuse only within this
            // callback-free interval; an independent expected witness still runs.
            {
                let mut done = Vec::new();
                let mut audit =
                    crate::portable_session::certification_audit::CertificationAudit::new(
                        graph.session,
                    )
                    .unwrap();
                assert!(
                    Certificate::checked_into(
                        graph.session,
                        graph.bridge,
                        &parent,
                        &parent.source,
                        &mut done,
                        &mut audit
                    )
                    .unwrap()
                    .is_some()
                );
                let completed = done.len();
                assert!(
                    Certificate::checked_into(
                        graph.session,
                        graph.bridge,
                        &certificates[meta_slot],
                        &certificates[meta_slot].source,
                        &mut done,
                        &mut audit
                    )
                    .unwrap()
                    .is_some()
                );
                assert_eq!(
                    done.len(),
                    completed,
                    "shared descendant was already completely validated"
                );
            }
            let mut cache = RootCache::default();
            let expected = graph.source_root_expectation(root).unwrap().unwrap();
            assert!(
                cache
                    .publish(graph.session, graph.bridge, &expected, &parent)
                    .unwrap()
            );
            graph.session.collect().unwrap();
            let hit = cache
                .lookup(graph.session, graph.bridge, &expected)
                .unwrap()
                .unwrap();
            assert!(Rc::ptr_eq(&hit.certificate, &parent));
            invoke(graph.session, "(fn [parent] (hash parent))", &parent.value);
            assert!(Certificate::validate(graph.session, graph.bridge, &parent).unwrap());
            drop(
                graph
                    .session
                    .invoke(&arm, &[&certificates[meta_slot].value])
                    .unwrap(),
            );
            let children = certificates
                .iter()
                .map(|certificate| {
                    Certificate::checked(
                        graph.session,
                        graph.bridge,
                        certificate,
                        &certificate.source,
                    )
                    .unwrap()
                    .unwrap()
                })
                .collect();
            assert!(
                graph
                    .materialize_source_parent(root, children)
                    .unwrap()
                    .is_none(),
                "a later real parent hash callback must deny stale child enrollment"
            );
            let observations = graph
                .session
                .eval("[lineage-callbacks lineage-armed]")
                .unwrap();
            assert_eq!(
                crate::portable_repl::display(graph.session, &observations).unwrap(),
                "[1 false]"
            );
            assert!(!Certificate::validate(graph.session, graph.bridge, &parent).unwrap());
            assert!(
                cache
                    .lookup(graph.session, graph.bridge, &expected)
                    .unwrap()
                    .is_none()
            );
            assert!(
                !cache
                    .publish(graph.session, graph.bridge, &expected, &parent)
                    .unwrap()
            );
            assert!(
                cache
                    .lookup(graph.session, graph.bridge, &expected)
                    .unwrap()
                    .is_none()
            );
        }
    }
    #[test]
    fn source_child_closure_enforces_source_and_domain_after_canonical_warming_and_poison() {
        let mut s = Session::new_macro().unwrap();
        s.eval("(def ^{:audit {:x 17}} child-domain-source 1)")
            .unwrap();
        let bridge = FormBridge::new(&mut s).unwrap();
        let _op = s.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut s);
        let (root, meta, entries) = declaration(&mut graph, "child-domain-source");
        let child = standalone(&mut graph, meta);
        let parent = parent(&mut graph, root, meta, entries, &child);
        graph.session.collect().unwrap();
        invoke(graph.session, "(fn [x] (hash x))", &parent.value);
        assert!(
            Certificate::validate(graph.session, graph.bridge, &parent).unwrap(),
            "final native facts and child domain must admit genuine memo warming"
        );
        invoke(
            graph.session,
            "(fn [m] (aset (.-arr (:audit m)) 1 99))",
            &child.value,
        );
        invoke(
            graph.session,
            "(fn [x] (do (set! (.-__hash x) nil) (hash x)))",
            &parent.value,
        );
        assert!(
            !Certificate::validate(graph.session, graph.bridge, &parent).unwrap(),
            "a poisoned certified child must invalidate its parent despite a protected hash attempt"
        );
        invoke(
            graph.session,
            "(fn [m] (aset (.-arr (:audit m)) 1 17))",
            &child.value,
        );
        // Equal numeric restoration cannot recreate the original boxed input.
        // A changed child remains invalid; no posthoc guest facts are captured.
        assert!(!Certificate::validate(graph.session, graph.bridge, &parent).unwrap());
    }
    #[test]
    fn source_child_certificates_reject_wrong_owner_equal_doc_and_forged_parent_memo() {
        for source_phase in [Phase::Runtime, Phase::Macro] {
            let mut source = match source_phase {
                Phase::Runtime => Session::new_repl().unwrap(),
                Phase::Macro => Session::new_macro().unwrap(),
            };
            source.eval("(def ^{:doc \"same\"} source-child-a 1) (def ^{:doc \"same\"} source-child-b 1)").unwrap();
            let source_snapshot = source.compilation_snapshot();
            let mut s = Session::new_macro().unwrap();
            s.eval(
                "(def ^{:doc \"same\"} source-child-a 1) (def ^{:doc \"same\"} source-child-b 1)",
            )
            .unwrap();
            let bridge = FormBridge::new(&mut s).unwrap();
            let _op = s.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut s);
            let (a, am, entries) = declaration_from_source(
                &mut graph,
                &source_snapshot.environment,
                source_phase,
                "source-child-a",
            );
            let (b, _, bentries) = declaration_from_source(
                &mut graph,
                &source_snapshot.environment,
                source_phase,
                "source-child-b",
            );
            let doc_key = graph.keyword("doc").unwrap();
            let a_doc = entries.iter().find(|(key, _)| *key == doc_key).unwrap().1;
            let b_doc = bentries.iter().find(|(key, _)| *key == doc_key).unwrap().1;
            let ad = graph
                .source_roots
                .index(graph.session, a_doc)
                .unwrap()
                .unwrap();
            let bd = graph
                .source_roots
                .index(graph.session, b_doc)
                .unwrap()
                .unwrap();
            let layout = graph.source_root_children().unwrap();
            let child_doc = standalone_with_layout(&mut graph, ad, &layout);
            let wrong = graph
                .source_root_expectation_with_layout(bd, &layout)
                .unwrap()
                .unwrap();
            assert!(
                wrong
                    .witness
                    .matches_logical_source(graph.session, graph.bridge, &child_doc.value)
                    .unwrap(),
                "logical equality is deliberately insufficient"
            );
            assert!(
                Certificate::checked(graph.session, graph.bridge, &child_doc, &wrong)
                    .unwrap()
                    .is_none()
            );
            // A child authenticated against ITS OWN source must still be
            // rejected at a different parent's native boundary ordinal.
            let wrong_doc = standalone_with_layout(&mut graph, bd, &layout);
            // Keep the genuine factory certificates, not validation booleans.
            // Every variant below performs complete current validation again.
            // This remains ONE unchanged operation budget for the entire test.
            let mut common = Vec::new();
            for boundary in &layout[a] {
                common.push(if *boundary == ad {
                    child_doc.clone()
                } else {
                    standalone_with_layout(&mut graph, *boundary, &layout)
                });
            }
            for variant in 0..5 {
                let selected = layout[a].iter().enumerate().map(|(slot, boundary)| {
                    let certificate = if *boundary == ad && variant == 0 {
                        &wrong_doc
                    } else {
                        &common[slot]
                    };
                    (certificate, &certificate.source)
                });
                let mut supplied: Vec<_> =
                    Certificate::checked_batch(graph.session, graph.bridge, selected)
                        .unwrap()
                        .into_iter()
                        .map(|child| child.unwrap())
                        .collect();
                match variant {
                    1 => {
                        supplied.pop().unwrap();
                    } // missing
                    2 => {
                        let own_source = wrong_doc.source.clone();
                        supplied.push(
                            Certificate::checked(
                                graph.session,
                                graph.bridge,
                                &wrong_doc,
                                &own_source,
                            )
                            .unwrap()
                            .unwrap(),
                        ); // unexpected
                    }
                    3 => {
                        assert!(supplied.len() > 1);
                        supplied.swap(0, 1); // correct owners, wrong ordinals
                    }
                    4 => {
                        assert!(supplied.len() > 1);
                        let duplicate = &supplied[0].certificate;
                        let own_source = duplicate.source.clone();
                        let checked = Certificate::checked(
                            graph.session,
                            graph.bridge,
                            duplicate,
                            &own_source,
                        )
                        .unwrap()
                        .unwrap();
                        supplied[1] = checked; // duplicate owner, missing another
                    }
                    _ => {}
                }
                let expected = graph
                    .source_root_expectation_with_layout(a, &layout)
                    .unwrap()
                    .unwrap();
                let recording = graph.bridge.recording(graph.session).unwrap();
                let alias = graph.source_roots.roots[a].alias;
                let value = graph.materialize_using(alias, &recording).unwrap();
                assert!(
                    Certificate::finish(
                        graph.session,
                        graph.bridge,
                        expected,
                        value,
                        recording.into_records().unwrap(),
                        supplied,
                    )
                    .unwrap()
                    .is_none(),
                    "invalid parent child coverage variant {variant}"
                );
            }
            // Physical lineage negative: inject only the CORRECT native alias
            // tokens, then deliberately patch the doc slot with the equal-valued
            // wrong-owner child. Correct child inventory cannot bless that slot.
            let mut supplied = Vec::new();
            for certificate in &common {
                let expected = certificate.source.clone();
                supplied.push(
                    Certificate::checked(graph.session, graph.bridge, certificate, &expected)
                        .unwrap()
                        .unwrap(),
                );
            }
            let inputs = graph.source_root_inputs(a, supplied).unwrap().unwrap();
            let normal = graph.bridge.recording(graph.session).unwrap();
            let private = normal
                .deferred_private_bridge(graph.session, &inputs.log)
                .unwrap();
            let native = graph.source_roots.roots[a].native;
            let value = graph
                .materialize_with_inputs(native, &private, None, Some((&inputs, &normal)))
                .unwrap();
            drop(normal);
            let (_, factories) = private.into_deferred_records().unwrap();
            let correct: Vec<_> = inputs
                .inputs
                .iter()
                .map(|input| (input.token.value(), input.child.value()))
                .collect();
            let mut wrong_pairs = correct.clone();
            let doc_slot = inputs
                .inputs
                .iter()
                .position(|input| input.alias == a_doc)
                .unwrap();
            wrong_pairs[doc_slot].1 = &wrong_doc.value;
            inputs
                .log
                .borrow()
                .apply(graph.session, &value, &wrong_pairs)
                .unwrap();
            let mut rejected = false;
            for factory in factories {
                match factory.finalize(graph.session, &correct) {
                    Ok(_) => {}
                    Err(error) => {
                        assert!(
                            error
                                .to_string()
                                .contains("Deferred factory does not match native bound inputs"),
                            "{error}"
                        );
                        rejected = true;
                        break;
                    }
                }
            }
            assert!(
                rejected,
                "valid inventory cannot authorize an equal-content wrong physical child"
            );
            let meta_slot = layout[a].iter().position(|id| *id == am).unwrap();
            let child_meta = common[meta_slot].clone();
            let parent = parent(&mut graph, a, am, entries, &child_meta);
            invoke(
                graph.session,
                "(fn [m] (set! (.-__hash m) 991))",
                &parent.value,
            );
            assert!(!Certificate::validate(graph.session, graph.bridge, &parent).unwrap());
            let _ = b;
        }
    }
}
