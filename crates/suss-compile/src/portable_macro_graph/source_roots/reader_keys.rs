//! Exact native occurrence boundaries and checked direct reader keys.
//!
//! Persistent identity uses the ORIGINAL completed source (or original explicit
//! value owner), never the current type revision. Current holder binding is
//! independently rechecked BEFORE every capture. Certificates keep weak leases;
//! neither guest equality nor a same-spelling keyword mints a source owner.
use super::*;
use crate::portable_macro_graph::preserve_source_error;
use crate::portable_macro_graph::{reuse, MetadataShape};
use crate::portable::hir::MetadataKeyOrigin;
use crate::portable::resolve::{CompletedDeclarationSource, TypeSourceValue};
use suss_reader::forms::{Form, Kind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ReaderKeyPath {
    prefix: usize,
    pair: usize,
    origin: u8,
}
// Separate DATA occurrence type: never a direct-key selector and never a
// CheckedDirectKey/Certificate capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ReaderValuePath {
    prefix: usize,
    pair: usize,
}
// Direct child KEY of one winning reader property VALUE. DATA identity only;
// never a CheckedDirectKey, memo slot, or persistent source-domain capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ReaderNestedKeyPath {
    value: ReaderValuePath,
    pair: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ReaderSelector {
    Key(ReaderKeyPath),
    Value(ReaderValuePath),
    NestedKey(ReaderNestedKeyPath),
    Metadata,
    ExplicitValue,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ReaderSourceIndex {
    Ordinary(usize),
    Completed(usize),
    Explicit(usize),
}
#[derive(Clone)]
pub(super) enum ReaderSourceOwner {
    Ordinary(Arc<DefinitionInfo>),
    Completed(Arc<CompletedDeclarationSource>),
    Explicit(Arc<TypeSourceValue>),
}
#[derive(Clone)]
pub(super) enum ReaderSourceLease {
    Ordinary(Weak<DefinitionInfo>),
    Completed(Weak<CompletedDeclarationSource>),
    Explicit(Weak<TypeSourceValue>),
}
impl ReaderSourceOwner {
    pub(super) fn index(&self) -> ReaderSourceIndex {
        match self {
            Self::Ordinary(v) => ReaderSourceIndex::Ordinary(Arc::as_ptr(v) as usize),
            Self::Completed(v) => ReaderSourceIndex::Completed(Arc::as_ptr(v) as usize),
            Self::Explicit(v) => ReaderSourceIndex::Explicit(Arc::as_ptr(v) as usize),
        }
    }
    pub(super) fn lease(&self, session: &mut Session) -> Result<ReaderSourceLease> {
        session.charge_host_work(4)?; // variant, downgrade, retain/release
        Ok(match self {
            Self::Ordinary(v) => ReaderSourceLease::Ordinary(Arc::downgrade(v)),
            Self::Completed(v) => ReaderSourceLease::Completed(Arc::downgrade(v)),
            Self::Explicit(v) => ReaderSourceLease::Explicit(Arc::downgrade(v)),
        })
    }
}
impl ReaderSourceLease {
    pub(super) fn exists(&self) -> bool {
        match self {
            Self::Ordinary(v) => v.strong_count() != 0,
            Self::Completed(v) => v.strong_count() != 0,
            Self::Explicit(v) => v.strong_count() != 0,
        }
    }
}

/// This type has no arbitrary-value constructor. It carries a STRICT checked
/// child and the independently selected native expectation. Health-only proofs
/// must remain a distinct controller mode and can never manufacture this type.
pub(in crate::portable_macro_graph) struct CheckedDirectKey {
    child: CheckedChild,
    expected: Rc<Expectation>,
    exposed: bool,
}
impl CheckedDirectKey {
    fn recheck(
        &self,
        session: &mut Session,
        bridge: &crate::portable_macro_data::FormBridge,
    ) -> Result<bool> {
        session.charge_host_work(2)?; // transient checked child retain/release
        Ok(
            Certificate::checked(session, bridge, &self.child.certificate, &self.expected)?
                .is_some(),
        )
    }
    pub(in crate::portable_macro_graph) fn select(
        &mut self,
        session: &mut Session,
        bridge: &crate::portable_macro_data::FormBridge,
    ) -> Result<Option<crate::portable_session::SessionValue>> {
        let _request = session.operation_budget_scope()?;
        session.charge_host_work(2)?;
        if !self.recheck(session, bridge)? {
            if self.exposed {
                return Err(invalid("Direct reader key changed after exposure"));
            }
            return Ok(None); // only UNEXPOSED selection may decline
        }
        session.charge_host_work(3)?; // rooted value retain/release and state write
        self.exposed = true;
        Ok(Some(self.child.value().clone()))
    }
    pub(in crate::portable_macro_graph) fn check_after_callbacks(
        &self,
        session: &mut Session,
        bridge: &crate::portable_macro_data::FormBridge,
    ) -> Result<()> {
        let _request = session.operation_budget_scope()?;
        session.charge_host_work(2)?;
        if !self.exposed || !self.recheck(session, bridge)? {
            return Err(invalid("Direct reader key failed final callback closure"));
        }
        Ok(())
    }
}

impl AnalysisGraph<'_> {
    /// Mint a stable source-root alias AFTER native holder/selector validation.
    /// Private callers below obtain `native` from the original reader/value
    /// projection, not from a guest read or an arbitrary external seed.
    fn capture_reader_boundary(
        &mut self,
        global: &Global,
        owner: ReaderSourceOwner,
        selector: ReaderSelector,
        native: Value,
    ) -> Result<Value> {
        self.session.charge_host_work(4)?;
        if !self.capture_roots {
            return Ok(native);
        }
        let identity = owner.index();
        for capture in &self.source_roots.roots {
            self.session.charge_host_work(4)?;
            if let Owner::ReaderSource(old_global, old_owner, old_selector) = &capture.owner {
                self.session.charge_host_work(
                    (old_global.namespace().len()
                        + old_global.name().len()
                        + global.namespace().len()
                        + global.name().len()
                        + 12) as u64,
                )?;
                if *old_global == *global
                    && old_owner.index() == identity
                    && *old_selector == selector
                {
                    // Exact source occurrence already captured in this Graph.
                    // Its original native recipe remains the authority source.
                    return Ok(capture.alias);
                }
            }
        }
        self.session.charge_host_work(
            (2 * std::mem::size_of::<Global>()
                + 2 * (global.namespace().len() + global.name().len())
                + 8) as u64,
        )?;
        self.capture_source_root(Owner::ReaderSource(global.clone(), owner, selector), native)
    }

    pub(in crate::portable_macro_graph) fn capture_completed_reader_key(
        &mut self,
        global: &Global,
        owner: &Arc<CompletedDeclarationSource>,
        holder: &Arc<DefinitionInfo>,
        position: (usize, usize),
    ) -> Result<Value> {
        // Checks actual Global, exact held completed owner, full ordinary/type
        // current-input binding, including original SourceOrigin correspondence.
        self.completed_owner_binding(global, owner, holder)?;
        let metadata = self.completed_reader(owner)?;
        self.session.charge_host_work(2)?;
        self.capture_normalized_reader_key(
            global,
            ReaderSourceOwner::Completed(owner.clone()),
            &metadata,
            position,
        )
    }
    fn capture_normalized_reader_key(
        &mut self,
        global: &Global,
        owner: ReaderSourceOwner,
        metadata: &hir::ReaderMetadata,
        position: (usize, usize),
    ) -> Result<Value> {
        self.session.charge_host_work(6)?;
        if metadata.pairs.len() % 2 != 0
            || metadata.pairs.len() / 2 != metadata.origins.len()
            || metadata.key_positions.len() != metadata.origins.len()
        {
            return Err(invalid("Reader key normalized occurrence extent mismatch"));
        }
        let mut selected = None;
        for (index, candidate) in metadata.key_positions.iter().enumerate() {
            self.session.charge_host_work(5)?;
            if *candidate == position {
                if selected.is_some() {
                    return Err(invalid("Ambiguous retained reader key coordinate"));
                }
                selected = Some(index);
            }
        }
        let index = selected
            .ok_or_else(|| invalid("Requested key is not a retained normalized occurrence"))?;
        self.session.charge_host_work(8)?;
        let origin = metadata.origins[index];
        let path = ReaderKeyPath {
            prefix: position.0,
            pair: position.1,
            origin: match origin {
                MetadataKeyOrigin::SourceSyntax => 0,
                MetadataKeyOrigin::ReaderShorthandTag => 1,
            },
        };
        // All native source key kinds use the existing provenance-preserving
        // constructor, not a keyword spelling shortcut. Private placement and
        // eventual controller admission still decide supported direct-key use.
        let native = self.reader_key_recipe(&metadata.pairs[index * 2], origin)?;
        self.capture_reader_boundary(global, owner, ReaderSelector::Key(path), native)
    }
    // A DATA occurrence is selected from the genuine owner's actual source.
    // The winning VALUE position differs from the retained KEY position when a
    // later prefix associates a replacement value. Neither label nor equality
    // joins two independently produced values.
    pub(in crate::portable_macro_graph) fn capture_completed_reader_value(
        &mut self, global: &Global, owner: &Arc<CompletedDeclarationSource>,
        holder: &Arc<DefinitionInfo>, index: usize, depth: usize,
    ) -> Result<Value> {
        self.completed_owner_binding(global, owner, holder)?;
        let declaration = self.completed_reader_source(owner)?;
        let entries = preserve_source_error(declaration.span.clone(),
            &mut |n| self.session.charge_host_work(n),
            |meter| hir::reader_metadata_view_metered(&declaration, meter))?;
        self.session.charge_host_work(8)?;
        let entry = entries.get(index)
            .ok_or_else(|| invalid("Unknown selected reader value ordinal"))?;
        let position = entry.value_position();
        for (other, candidate) in entries.iter().enumerate() {
            self.session.charge_host_work(4)?;
            if other != index && candidate.value_position() == position {
                return Err(invalid("Ambiguous retained reader value coordinate"));
            }
        }
        // Build the original full value first, retaining every old guard/debit.
        // Decorate actual direct-key occurrences before publishing the parent.
        let native = self.reader_value_with_nested_keys(global, owner, holder, index,
            entry.value.as_ref(), depth)?;
        let native = self.recipe(Recipe::Alias(native))?;
        self.session.charge_host_work(6)?; // selector and actual owner retain/release
        let selector = ReaderValuePath { prefix: position.0, pair: position.1 };
        self.capture_reader_boundary(global, ReaderSourceOwner::Completed(owner.clone()),
            ReaderSelector::Value(selector), native)
    }
    // Metadata/count/index arguments are only native selectors. The actual KEY
    // and winning VALUE coordinate always come from this checked owner.
    pub(in crate::portable_macro_graph) fn capture_completed_nested_reader_key(
        &mut self, global: &Global, owner: &Arc<CompletedDeclarationSource>,
        holder: &Arc<DefinitionInfo>, index: usize, pair: usize,
    ) -> Result<Value> {
        self.completed_owner_binding(global, owner, holder)?;
        self.charge(4)?;
        if !self.capture_roots || !self.direct_reader_roots {
            return Err(invalid("Nested reader keys require DATA controller mode"));
        }
        let declaration = self.completed_reader_source(owner)?;
        let entries = preserve_source_error(declaration.span.clone(),
            &mut |n| self.session.charge_host_work(n),
            |meter| hir::reader_metadata_view_metered(&declaration, meter))?;
        self.charge(8)?;
        let entry = entries.get(index).ok_or_else(|| invalid("Unknown nested reader property"))?;
        let position = entry.value_position();
        for (other, candidate) in entries.iter().enumerate() {
            self.charge(4)?;
            if other != index && candidate.value_position() == position {
                return Err(invalid("Ambiguous nested reader value coordinate"));
            }
        }
        let items = match &entry.value.as_ref().kind {
            Kind::Map(items) if items.len() % 2 == 0 => items.as_slice(),
            Kind::Vector(items) if items.len() == 2 => items.as_slice(),
            _ => return Err(invalid("Nested key requires actual reader map or entry")),
        };
        if !self.first_reader_keyword_pair(items, pair)? {
            return Err(invalid("Nested key is not the first actual keyword occurrence"));
        }
        // Checked extent in first_reader_keyword_pair, no externally supplied Form.
        self.charge(8)?;
        let native = self.form(&items[pair * 2], 0)?;
        let native = self.recipe(Recipe::Alias(native))?;
        self.charge(8)?; // selector init, owner clone and retirement
        self.capture_reader_boundary(global, ReaderSourceOwner::Completed(owner.clone()),
            ReaderSelector::NestedKey(ReaderNestedKeyPath {
                value: ReaderValuePath { prefix: position.0, pair: position.1 }, pair,
            }), native)
    }
    fn first_reader_keyword_pair(&mut self, items: &[Form], pair: usize) -> Result<bool> {
        self.charge(8)?;
        if items.len() % 2 != 0 || pair >= items.len() / 2 { return Ok(false); }
        let Kind::Keyword(key) = &items[pair * 2].kind else { return Ok(false) };
        let mut earlier = 0;
        loop {
            self.charge(6)?; // next/probe and indexed read, including final probe
            if earlier >= pair { break; }
            if let Kind::Keyword(other) = &items[earlier * 2].kind {
                self.charge(key.name.len().saturating_add(other.name.len())
                    .saturating_add(key.namespace.as_ref().map_or(0, String::len))
                    .saturating_add(other.namespace.as_ref().map_or(0, String::len)).saturating_add(8))?;
                // Shipped Keyword IEquiv compares fqn, not its namespace/name
                // partition. Stream the exact constructor fqn with no allocation.
                self.charge(key.name.len().saturating_add(other.name.len())
                    .saturating_add(key.namespace.as_ref().map_or(0, String::len))
                    .saturating_add(other.namespace.as_ref().map_or(0, String::len))
                    .saturating_add(4).saturating_mul(2).saturating_add(16))?;
                let left = key.namespace.as_deref().unwrap_or("").bytes()
                    .chain(key.namespace.as_ref().map(|_| b'/')).chain(key.name.bytes());
                let right = other.namespace.as_deref().unwrap_or("").bytes()
                    .chain(other.namespace.as_ref().map(|_| b'/')).chain(other.name.bytes());
                if left.eq(right) { return Ok(false); }
            }
            self.charge(1)?;
            earlier += 1;
        }
        Ok(true)
    }
    fn reader_value_with_nested_keys(&mut self, global: &Global,
        owner: &Arc<CompletedDeclarationSource>, holder: &Arc<DefinitionInfo>,
        index: usize, form: &Form, depth: usize,
    ) -> Result<Value> {
        let native = self.form(form, depth)?;
        self.charge(6)?;
        let items = match &form.kind {
            Kind::Map(items) if items.len() % 2 == 0 => items.as_slice(),
            Kind::Vector(items) if items.len() == 2 => items.as_slice(),
            _ => return Ok(native),
        };
        // Recipe provenance is the immediately preceding original form build,
        // not an external ID or equality-joined guest/source projection.
        self.charge(std::mem::size_of::<Option<(MetadataShape, Value, Vec<Value>)>>() * 2 + 12)?;
        let (base, wrapper) = match self.recipes.get(native).and_then(Option::as_ref) {
            Some(Recipe::Metadata { set_items, .. }) => {
                self.charge(set_items.len().saturating_mul(std::mem::size_of::<Value>() * 2 + 4)
                    .saturating_add(std::mem::size_of::<Vec<Value>>() * 2 + 8))?;
                let Some(Recipe::Metadata { shape, value, metadata, set_items }) = self.recipes[native].as_ref() else { unreachable!() };
                (*value, Some((*shape, *metadata, set_items.clone())))
            },
            _ => (native, None),
        };
        self.charge(std::mem::size_of::<Vec<Value>>() * 2 + 8)?;
        let mut children = Vec::new();
        // Copy only original direct children, preserving their order/values.
        reuse::reserve(self.session, &mut children, items.len())?;
        match self.recipes.get(base).and_then(Option::as_ref) {
            Some(Recipe::ReaderMap(entries)) if matches!(&form.kind, Kind::Map(_)) && entries.len() == items.len() / 2 => {
                self.charge(items.len().saturating_mul(4))?;
                let Some(Recipe::ReaderMap(entries)) = self.recipes[base].as_ref() else { unreachable!() };
                for (key, value) in entries { children.push(*key); children.push(*value); }
            },
            Some(Recipe::ReaderVector(values)) if matches!(&form.kind, Kind::Vector(_)) && values.len() == 2 => {
                self.charge(8)?;
                let Some(Recipe::ReaderVector(values)) = self.recipes[base].as_ref() else { unreachable!() };
                children.extend_from_slice(values);
            },
            _ => return Err(invalid("Reader value recipe does not match its native constructor")),
        }
        let mut pair = 0;
        loop {
            self.charge(6)?; // loop/extent probe before indexed selection
            if pair >= items.len() / 2 { break; }
            if self.first_reader_keyword_pair(items, pair)? {
                let key = self.capture_completed_nested_reader_key(global, owner, holder, index, pair)?;
                self.charge(4)?;
                children[pair * 2] = key;
            }
            self.charge(1)?;
            pair += 1;
        }
        let base = if matches!(&form.kind, Kind::Map(_)) {
            self.charge(std::mem::size_of::<Vec<(Value, Value)>>() * 2 + 8)?;
            let mut entries = Vec::new();
            reuse::reserve(self.session, &mut entries, children.len() / 2)?;
            self.charge(children.len().saturating_mul(2).saturating_add(4))?;
            for pair in children.chunks_exact(2) { self.charge(4)?; entries.push((pair[0], pair[1])); }
            self.recipe(Recipe::ReaderMap(entries))?
        } else { self.recipe(Recipe::ReaderVector(children))? };
        if let Some((shape, metadata, set_items)) = wrapper {
            self.charge(8)?;
            self.recipe(Recipe::Metadata { shape, value: base, metadata, set_items })
        } else { Ok(base) }
    }
    pub(in crate::portable_macro_graph) fn capture_completed_metadata(
        &mut self,
        global: &Global,
        owner: &Arc<CompletedDeclarationSource>,
        holder: &Arc<DefinitionInfo>,
        native: Value,
    ) -> Result<Value> {
        self.completed_owner_binding(global, owner, holder)?;
        self.session.charge_host_work(2)?; // completed owner retain/release
        self.capture_reader_boundary(
            global,
            ReaderSourceOwner::Completed(owner.clone()),
            ReaderSelector::Metadata,
            native,
        )
    }
    pub(in crate::portable_macro_graph) fn capture_explicit_source_value(
        &mut self,
        global: &Global,
        owner: &Arc<TypeSourceValue>,
        holder: &Arc<DefinitionInfo>,
        native: Value,
    ) -> Result<Value> {
        // Exact original explicit value owner, not current type Info. Its full
        // current plan and membership were checked immediately by the producer;
        // recheck here too, so this API cannot bypass that private invariant.
        self.check_explicit_source_binding(global, holder, owner)?;
        self.session.charge_host_work(2)?;
        self.capture_reader_boundary(
            global,
            ReaderSourceOwner::Explicit(owner.clone()),
            ReaderSelector::ExplicitValue,
            native,
        )
    }
    pub(in crate::portable_macro_graph) fn capture_ordinary_reader_key(
        &mut self,
        source: &hir::SourceNamespace,
        global: &Global,
        holder: &Arc<DefinitionInfo>,
        position: (usize, usize),
    ) -> Result<Value> {
        self.session.charge_host_work(4)?;
        if !self.capture_roots {
            return Err(invalid("Reader key capture requires captured source"));
        }
        let mut found = false;
        for (candidate, info) in &source.declarations {
            self.session.charge_host_work(
                (candidate.namespace().len()
                    + candidate.name().len()
                    + global.namespace().len()
                    + global.name().len()
                    + 6) as u64,
            )?;
            if candidate == global {
                found = Arc::ptr_eq(info, holder);
                break;
            }
        }
        if !found {
            return Err(invalid(
                "Reader key holder is not the captured Global binding",
            ));
        }
        if let Some(owner) = &holder.completed_values {
            return self.capture_completed_reader_key(global, owner, holder, position);
        }
        self.session.charge_host_work(8)?;
        let ordinary = holder.type_fields.is_none()
            && matches!(&holder.definition_form.kind,
            suss_reader::forms::Kind::List(items) if matches!(items.first().map(|head| &head.kind),
                Some(suss_reader::forms::Kind::Symbol(head)) if head.namespace.is_none()
                    && matches!(head.name.as_str(), "def" | "defonce")));
        if !ordinary {
            return Err(invalid("Uncompleted type reader key has no original owner"));
        }
        let metadata = crate::portable_macro_data::normalized_reader_metadata_view(
            self.session,
            &holder.declaration,
        )?;
        self.session.charge_host_work(2)?;
        self.capture_normalized_reader_key(
            global,
            ReaderSourceOwner::Ordinary(holder.clone()),
            &metadata,
            position,
        )
    }
    pub(in crate::portable_macro_graph) fn checked_direct_reader_key(
        &mut self,
        alias: Value,
        certificate: &Rc<Certificate>,
    ) -> Result<Option<CheckedDirectKey>> {
        let id = self
            .source_roots
            .index(self.session, alias)?
            .ok_or_else(|| invalid("Unknown direct reader key capture"))?;
        self.session.charge_host_work(2)?;
        if !matches!(
            self.source_roots.roots[id].owner,
            Owner::ReaderSource(_, _, ReaderSelector::Key(_))
        ) {
            return Err(invalid(
                "Direct key requires an actual reader key occurrence",
            ));
        }
        let Some(expected) = self.source_root_expectation(id)? else {
            return Ok(None);
        };
        let Some(child) = Certificate::checked(self.session, self.bridge, certificate, &expected)?
        else {
            return Ok(None);
        };
        self.session
            .charge_host_work((2 * std::mem::size_of::<CheckedDirectKey>() + 6) as u64)?;
        Ok(Some(CheckedDirectKey {
            child,
            expected,
            exposed: false,
        }))
    }
}

/// A build-local inventory. Selection returns ONLY exact native aliases; final
/// closure rechecks every exposed key. There is deliberately no cache commit,
/// arbitrary-value insertion, source repair, or guest callback in this type.
/// The controller must hold it until its last callback and validate it before
/// publishing ANY parent/root cache entry. It is not yet wired into legacy Build.
pub(in crate::portable_macro_graph) struct DirectKeyBatch {
    keys: Vec<(Value, CheckedDirectKey)>,
}
impl DirectKeyBatch {
    pub(in crate::portable_macro_graph) fn new() -> Self {
        Self { keys: Vec::new() }
    }
    pub(in crate::portable_macro_graph) fn select(
        &mut self,
        graph: &mut AnalysisGraph<'_>,
        alias: Value,
        certificate: &Rc<Certificate>,
    ) -> Result<Option<crate::portable_session::SessionValue>> {
        graph.session.charge_host_work(2)?;
        for (known, direct) in &mut self.keys {
            graph.session.charge_host_work(2)?;
            if *known == alias {
                // Same capture, same exact selected certificate. Never swap an
                // exposed child to an equal-content or later incoming value.
                graph.session.charge_host_work(2)?;
                if !Rc::ptr_eq(&direct.child.certificate, certificate) {
                    return Err(invalid("Direct key certificate changed within one build"));
                }
                return direct.select(graph.session, graph.bridge);
            }
        }
        let Some(mut direct) = graph.checked_direct_reader_key(alias, certificate)? else {
            return Ok(None);
        };
        // All fallible inventory preparation precedes exposure.
        reserve(graph.session, &mut self.keys, 1)?;
        graph.session.charge_host_work(3)?;
        let Some(value) = direct.select(graph.session, graph.bridge)? else {
            return Ok(None);
        };
        self.keys.push((alias, direct));
        Ok(Some(value))
    }
    pub(in crate::portable_macro_graph) fn check_after_callbacks(
        &self,
        session: &mut Session,
        bridge: &crate::portable_macro_data::FormBridge,
    ) -> Result<()> {
        let _request = session.operation_budget_scope()?;
        for (_, direct) in &self.keys {
            session.charge_host_work(1)?;
            direct.check_after_callbacks(session, bridge)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::{
        hir,
        resolve::{Environment, Phase},
    };
    use crate::portable_macro_data::FormBridge;
    use crate::portable_session::SessionValue;

    fn add(environment: &Environment, text: &str, phase: Phase) -> Environment {
        let forms =
            suss_reader::forms::resolve_conditionals(suss_reader::forms::read_forms(text).unwrap())
                .unwrap();
        let origin = crate::portable::SourceOrigin::new(text, None);
        hir::prepare_with_origin(
            &forms,
            0..text.len(),
            environment,
            phase,
            &mut crate::portable::NoExpansion,
            Some(&origin),
        )
        .unwrap()
        .1
    }
    fn binding(
        environment: &Environment,
        phase: Phase,
        name: &str,
    ) -> (Global, Arc<DefinitionInfo>) {
        let binding = environment
            .resolve(phase, &suss_reader::Symbol::new(name), 0..0)
            .unwrap();
        let global = binding.global().clone();
        let info = environment.shared_definition_info(&global).unwrap().clone();
        (global, info)
    }
    fn certificate(graph: &mut AnalysisGraph<'_>, alias: Value) -> Rc<Certificate> {
        let id = graph
            .source_roots
            .index(graph.session, alias)
            .unwrap()
            .unwrap();
        let expected = graph.source_root_expectation(id).unwrap().unwrap();
        let recording = graph.bridge.recording(graph.session).unwrap();
        let value = graph.materialize_using(alias, &recording).unwrap();
        Certificate::finish(
            graph.session,
            graph.bridge,
            expected,
            value,
            recording.into_records().unwrap(),
            vec![],
        )
        .unwrap()
        .unwrap()
    }
    fn invoke(session: &mut Session, source: &str, value: &SessionValue) -> SessionValue {
        let function = session.eval(source).unwrap();
        session.invoke(&function, &[value]).unwrap()
    }
    #[test]
    fn completed_reader_key_owner_is_stable_across_type_revisions_and_checks_current_holder() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let original = add(&Environment::default(), "(def ^number watched 17)", phase);
            let first = add(&original, "(deftype* watched [] nil nil)", phase);
            let second = add(&first, "(deftype* watched [field] nil nil)", phase);
            let (global, ordinary) = binding(&original, phase, "watched");
            let (_, a) = binding(&first, phase, "watched");
            let (_, b) = binding(&second, phase, "watched");
            let owner = ordinary.completed_values.as_ref().unwrap();
            for current in [&a, &b] {
                let held = current
                    .type_key_history
                    .as_ref()
                    .and_then(|h| h.values(&global))
                    .and_then(|plan| plan.source.as_ref())
                    .unwrap();
                assert!(
                    Arc::ptr_eq(owner, held),
                    "fixture must retain the actual original source"
                );
            }
            let mut session = Session::new_macro().unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            graph.capture_roots = true;
            let key = graph
                .capture_completed_reader_key(&global, owner, &ordinary, (0, 0))
                .unwrap();
            assert_eq!(
                key,
                graph
                    .capture_completed_reader_key(&global, owner, &a, (0, 0))
                    .unwrap()
            );
            assert_eq!(
                key,
                graph
                    .capture_completed_reader_key(&global, owner, &b, (0, 0))
                    .unwrap()
            );
            let other = add(&Environment::default(), "(def ^number watched 17)", phase);
            let (_, foreign) = binding(&other, phase, "watched");
            assert!(
                graph
                    .capture_completed_reader_key(&global, owner, &foreign, (0, 0))
                    .is_err(),
                "equal source does not bind the original completed owner"
            );
            let alternate_phase = if phase == Phase::Runtime {
                Phase::Macro
            } else {
                Phase::Runtime
            };
            let wrong = add(
                &Environment::default(),
                "(def ^number watched 17)",
                alternate_phase,
            );
            let (wrong_global, wrong_holder) = binding(&wrong, alternate_phase, "watched");
            assert!(
                graph
                    .capture_completed_reader_key(&wrong_global, owner, &wrong_holder, (0, 0))
                    .is_err()
            );
            assert!(
                graph
                    .capture_completed_reader_key(&global, owner, &b, (99, 0))
                    .is_err()
            );
        }
    }
    #[test]
    fn normalized_reader_key_reuses_first_occurrence_and_keeps_foreign_owner_distinct() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let env = add(
                &Environment::default(),
                "(def ^{:tag left} ^{:tag right} watched 17) (def ^{:tag right} other 17)",
                phase,
            );
            let ns = hir::SourceNamespace::capture(&env, phase);
            let (global, info) = binding(&env, phase, "watched");
            let (other, other_info) = binding(&env, phase, "other");
            let mut session = Session::new_macro().unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            graph.capture_roots = true;
            let key = graph
                .capture_ordinary_reader_key(&ns, &global, &info, (1, 0))
                .unwrap();
            assert_eq!(
                key,
                graph
                    .capture_ordinary_reader_key(&ns, &global, &info, (1, 0))
                    .unwrap()
            );
            assert!(
                graph
                    .capture_ordinary_reader_key(&ns, &global, &info, (0, 0))
                    .is_err()
            );
            let foreign = graph
                .capture_ordinary_reader_key(&ns, &other, &other_info, (0, 0))
                .unwrap();
            assert_ne!(key, foreign, "equal spelling never joins original owners");
            let copy = Arc::new(info.as_ref().clone());
            assert!(
                graph
                    .capture_ordinary_reader_key(&ns, &global, &copy, (1, 0))
                    .is_err()
            );
        }
    }
    #[test]
    fn direct_key_batch_shares_two_uses_and_poison_after_exposure_fails_closed() {
        let env = add(
            &Environment::default(),
            "(def ^number watched 17) (def ^number other 17)",
            Phase::Runtime,
        );
        let ns = hir::SourceNamespace::capture(&env, Phase::Runtime);
        let (global, holder) = binding(&env, Phase::Runtime, "watched");
        let (foreign_global, foreign_holder) = binding(&env, Phase::Runtime, "other");
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.capture_roots = true;
        let key = graph
            .capture_ordinary_reader_key(&ns, &global, &holder, (0, 0))
            .unwrap();
        let foreign = graph
            .capture_ordinary_reader_key(&ns, &foreign_global, &foreign_holder, (0, 0))
            .unwrap();
        let proof = certificate(&mut graph, key);
        assert!(
            graph
                .checked_direct_reader_key(foreign, &proof)
                .unwrap()
                .is_none()
        );
        let mut direct = graph
            .checked_direct_reader_key(key, &proof)
            .unwrap()
            .unwrap();
        let mut foreign_session = Session::new_macro().unwrap();
        let foreign_bridge = FormBridge::new(&mut foreign_session).unwrap();
        assert!(matches!(
            direct.select(&mut foreign_session, &foreign_bridge),
            Err(SessionError::ForeignValue)
        ));
        let mut batch = DirectKeyBatch::new();
        let a = batch.select(&mut graph, key, &proof).unwrap().unwrap();
        graph.session.collect().unwrap();
        let b = batch.select(&mut graph, key, &proof).unwrap().unwrap();
        assert!(graph.session.values_identical(&a, &b).unwrap());
        batch
            .check_after_callbacks(graph.session, graph.bridge)
            .unwrap();
        // Real guest mutation/readback, not merely invoking a presumed setter.
        drop(invoke(
            graph.session,
            "(fn [key] (set! (.-_hash key) 991))",
            &a,
        ));
        let observed = invoke(graph.session, "(fn [key] (.-_hash key))", &a);
        assert_eq!(
            crate::portable_repl::display(graph.session, &observed).unwrap(),
            "991"
        );
        graph.session.collect().unwrap();
        assert!(batch.select(&mut graph, key, &proof).is_err());
        assert!(
            batch
                .check_after_callbacks(graph.session, graph.bridge)
                .is_err()
        );
    }
    #[test]
    fn direct_key_certificate_lease_does_not_self_keep_obsolete_completed_source() {
        let (mut session, bridge, proof, expected, mut cache, weak) = {
            let env = add(
                &Environment::default(),
                "(def ^number watched 17)",
                Phase::Runtime,
            );
            let ns = hir::SourceNamespace::capture(&env, Phase::Runtime);
            let (global, holder) = binding(&env, Phase::Runtime, "watched");
            let owner = holder.completed_values.as_ref().unwrap();
            let weak = Arc::downgrade(owner);
            let mut session = Session::new_macro().unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            graph.capture_roots = true;
            let alias = graph
                .capture_ordinary_reader_key(&ns, &global, &holder, (0, 0))
                .unwrap();
            let proof = certificate(&mut graph, alias);
            let expected = proof.source.clone();
            let mut cache = RootCache::default();
            assert!(
                cache
                    .publish(graph.session, graph.bridge, &expected, &proof)
                    .unwrap()
            );
            assert!(
                weak.strong_count() > 0,
                "live old captured owner is not prematurely retired"
            );
            drop(graph);
            // env/ns/current holder are the ONLY native owners. Certificates
            // and cache entries do not participate in their strong lifetime.
            (session, bridge, proof, expected, cache, weak)
        };
        assert_eq!(
            weak.strong_count(),
            0,
            "retained certificates must not keep obsolete source alive"
        );
        assert!(
            cache
                .lookup(&mut session, &bridge, &expected)
                .unwrap()
                .is_none()
        );
        cache.retire_obsolete(&mut session).unwrap();
        assert_eq!(cache.entries.len(), 0);
        assert!(!Certificate::validate(&mut session, &bridge, &proof).unwrap());
    }
    #[test]
    fn strict_reader_key_cache_selects_same_guest_child_in_a_new_graph_and_type_revision() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let original = add(&Environment::default(), "(def ^number watched 17)", phase);
            let (global, ordinary) = binding(&original, phase, "watched");
            let owner = ordinary.completed_values.as_ref().unwrap().clone();
            let mut session = Session::new_macro().unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let mut cache = RootCache::default();
            let (proof, first) = {
                let mut graph = AnalysisGraph::new(&bridge, &mut session);
                graph.capture_roots = true;
                let key = graph
                    .capture_completed_reader_key(&global, &owner, &ordinary, (0, 0))
                    .unwrap();
                let proof = certificate(&mut graph, key);
                assert!(
                    cache
                        .publish(graph.session, graph.bridge, &proof.source, &proof)
                        .unwrap()
                );
                let value = proof.value.clone();
                (proof, value)
            };
            let first_type = add(&original, "(deftype* watched [] nil nil)", phase);
            let second_type = add(&first_type, "(deftype* watched [field] nil nil)", phase);
            for environment in [&first_type, &second_type] {
                let (_, current) = binding(environment, phase, "watched");
                session.collect().unwrap();
                let mut graph = AnalysisGraph::new(&bridge, &mut session);
                graph.capture_roots = true;
                let key = graph
                    .capture_completed_reader_key(&global, &owner, &current, (0, 0))
                    .unwrap();
                let id = graph
                    .source_roots
                    .index(graph.session, key)
                    .unwrap()
                    .unwrap();
                let expected = graph.source_root_expectation(id).unwrap().unwrap();
                let selected = cache
                    .lookup(graph.session, graph.bridge, &expected)
                    .unwrap()
                    .unwrap();
                assert!(Rc::ptr_eq(&selected.certificate, &proof));
                assert!(
                    graph
                        .session
                        .values_identical(&first, selected.value())
                        .unwrap()
                );
                let mut batch = DirectKeyBatch::new();
                let actual = batch
                    .select(&mut graph, key, &selected.certificate)
                    .unwrap()
                    .unwrap();
                assert!(graph.session.values_identical(&actual, &first).unwrap());
                batch
                    .check_after_callbacks(graph.session, graph.bridge)
                    .unwrap();
            }
        }
    }

    #[test]
    fn reader_key_capture_fuel_irq_leaves_no_published_capture_and_retries() {
        let env = add(
            &Environment::default(),
            "(def ^number watched 17)",
            Phase::Macro,
        );
        let ns = hir::SourceNamespace::capture(&env, Phase::Macro);
        let (global, holder) = binding(&env, Phase::Macro, "watched");
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let initial_budget = {
            let _op = session.operation_budget_scope().unwrap();
            session.operation_fuel_remaining().unwrap()
        };
        let cost = {
            let op = session.operation_budget_scope().unwrap();
            let before = session.operation_fuel_remaining().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            graph.capture_roots = true;
            graph
                .capture_ordinary_reader_key(&ns, &global, &holder, (0, 0))
                .unwrap();
            let cost = before - graph.session.operation_fuel_remaining().unwrap();
            drop(graph);
            drop(op);
            cost
        };
        assert!(cost > 2);
        for allowance in [0, 1, cost / 2, cost - 1] {
            session.set_operation_fuel(allowance);
            let op = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            graph.capture_roots = true;
            let error = graph
                .capture_ordinary_reader_key(&ns, &global, &holder, (0, 0))
                .unwrap_err();
            assert!(matches!(error, SessionError::Trap(_)), "{error}");
            assert!(graph.source_roots.roots.is_empty());
            drop(graph);
            drop(op);
        }
        session.set_operation_fuel(initial_budget);
        let op = session.operation_budget_scope().unwrap();
        session.interrupt_handle().interrupt();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.capture_roots = true;
        assert!(
            graph
                .capture_ordinary_reader_key(&ns, &global, &holder, (0, 0))
                .unwrap_err()
                .is_interrupt()
        );
        assert!(graph.source_roots.roots.is_empty());
        graph
            .capture_ordinary_reader_key(&ns, &global, &holder, (0, 0))
            .unwrap();
        drop(graph);
        drop(op);
    }
}
