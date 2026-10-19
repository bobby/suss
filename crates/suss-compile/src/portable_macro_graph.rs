//! Original bounded transport of actual compiler analysis graphs.
//! This preserves native facts and sharing; source-level &env schema is separate.
use crate::portable::{
    self,
    hir::{
        self, Expression, FieldBinding, FunctionScope, Hir, Literal, LocalBinding, SourceNamespace,
        SourceRole,
    },
};
use crate::{
    portable_macro_data::{FormBridge, MetadataShape},
    portable_session::{Session, SessionError, SessionValue},
};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashMap, VecDeque},
    sync::Arc,
};
use suss_reader::forms::{Form, Kind};

#[path = "portable_macro_graph/raw_keys.rs"]
mod raw_keys;
mod initializer_plan;
#[cfg(test)]
mod plan_scratch_tests;
mod source_facts;
mod metadata_expectations;
pub(crate) mod source_roots;
mod construction;
mod private_placement;
mod reuse;
pub(crate) use reuse::DeclarationValues;
#[path = "portable_macro_graph/occurrence_work.rs"]
mod occurrence_work;
#[path = "portable_macro_graph/materialization_index.rs"]
mod materialization_index;
#[cfg(test)]
#[path = "portable_macro_graph/recorded_scratch_tests.rs"]
mod recorded_scratch_tests;
#[macro_use]
mod field_entry_work;
#[macro_use]
mod retained_field_keys;
mod reference_fields;
mod type_key_history;
mod reader_metadata;
use retained_field_keys::RetainedFieldKeys;
use raw_keys::{Interner, KeyId, Plan};

// Private native provenance; neither source Forms nor guest values can mint it.
// Only the explicitly annotated analyzer field positions below construct tokens.
// Pin c4295f303100bbf5afac449242d30bca1126f1a1: analyzer.cljc empty-env
// 725-736 and analyze/parse AST field literals; compiler.cljc emits-keyword
// 368-381 initializes their memo before any runtime map/hash dispatch.
#[derive(Clone, Copy)]
pub(crate) struct AnalyzerLiteralKeyword(&'static str);
impl AnalyzerLiteralKeyword {
    pub(crate) fn reader_shorthand_tag() -> Self { Self("tag") }
    pub(crate) fn name(self) -> &'static str { self.0 }
}
#[derive(Clone, Copy)]
enum FieldKey<'a> { Cold(&'a str), Literal(AnalyzerLiteralKeyword) }
impl<'a> From<&'a str> for FieldKey<'a> {
    fn from(name: &'a str) -> Self { Self::Cold(name) }
}
impl<'a> FieldKey<'a> {
    fn name(self) -> &'a str { match self { Self::Cold(name) => name, Self::Literal(token) => token.name() } }
}

type Value = usize;
type Result<T> = std::result::Result<T, SessionError>;

fn charge_origin_work(
    session: &mut Session,
    span: std::ops::Range<usize>,
    units: u64,
) -> std::result::Result<(), portable::Diagnostic> {
    session.charge_host_work(units).map_err(|error| portable::Diagnostic {
        span,
        message: error.to_string(),
    })
}

// Reader APIs use Diagnostics, but metering errors keep their original typed
// SessionError. Never relabel fuel/cancellation as a source syntax failure.
fn preserve_source_error<T>(
    span: std::ops::Range<usize>,
    charge: &mut impl FnMut(u64) -> Result<()>,
    operation: impl FnOnce(&mut dyn FnMut(u64) -> std::result::Result<(), portable::Diagnostic>)
        -> std::result::Result<T, portable::Diagnostic>,
) -> Result<T> {
    let mut original = None;
    let result = operation(&mut |units| charge(units).map_err(|error| {
        let diagnostic = portable::Diagnostic { span: span.clone(), message: error.to_string() };
        if original.is_none() { original = Some(error); }
        diagnostic
    }));
    match original {
        Some(error) => Err(error),
        None => result.map_err(SessionError::Compile),
    }
}

fn symbol_position(
    session: &mut Session,
    origin: &portable::SourceOrigin,
    form: &Form,
) -> Result<Option<portable::SourcePosition>> {
    origin
        .symbol_position_metered(form, &mut |work| {
            charge_origin_work(session, form.span.clone(), work)
        })
        .map_err(|error| SessionError::Host(wasmtime::Error::msg(error.to_string())))
}

/// Content identity for an immutable namespace snapshot. Equivalent snapshots
/// can be allocated at separate source-analysis boundaries. Declaration
/// revisions remain identity-bearing, so their Arc identities are part of the
/// key rather than comparing their user-visible fields.
#[derive(Debug, Clone)]
struct NamespaceKey(Arc<SourceNamespace>);
impl NamespaceKey {
    fn new(namespace: &Arc<SourceNamespace>) -> Self {
        Self(namespace.clone())
    }
    fn compare_declarations(
        left: &BTreeMap<portable::resolve::Global, Arc<portable::resolve::DefinitionInfo>>,
        right: &BTreeMap<portable::resolve::Global, Arc<portable::resolve::DefinitionInfo>>,
    ) -> Ordering {
        let mut left = left.iter();
        let mut right = right.iter();
        loop {
            match (left.next(), right.next()) {
                (Some((left_global, left_info)), Some((right_global, right_info))) => {
                    let order = left_global.cmp(right_global).then_with(|| {
                        (Arc::as_ptr(left_info) as usize).cmp(&(Arc::as_ptr(right_info) as usize))
                    });
                    if order != Ordering::Equal {
                        return order;
                    }
                }
                (None, None) => return Ordering::Equal,
                (None, Some(_)) => return Ordering::Less,
                (Some(_), None) => return Ordering::Greater,
            }
        }
    }
}
impl PartialEq for NamespaceKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for NamespaceKey {}
impl PartialOrd for NamespaceKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for NamespaceKey {
    fn cmp(&self, other: &Self) -> Ordering {
        let left = &self.0;
        let right = &other.0;
        left.namespace
            .cmp(&right.namespace)
            .then_with(|| left.aliases.cmp(&right.aliases))
            .then_with(|| left.refers.cmp(&right.refers))
            .then_with(|| left.used_refers.cmp(&right.used_refers))
            .then_with(|| left.renamed_refers.cmp(&right.renamed_refers))
            .then_with(|| left.excluded_core.cmp(&right.excluded_core))
            .then_with(|| left.macro_aliases.cmp(&right.macro_aliases))
            .then_with(|| left.macro_refers.cmp(&right.macro_refers))
            .then_with(|| left.used_macro_refers.cmp(&right.used_macro_refers))
            .then_with(|| left.renamed_macro_refers.cmp(&right.renamed_macro_refers))
            .then_with(|| Self::compare_declarations(&left.declarations, &right.declarations))
            .then_with(|| left.identities.cmp(&right.identities))
    }
}

// Iterative validation keeps guest-facing reader forms off the Rust call stack.
// Every visit, normalized metadata pass, and text unit is charged to the active
// Session operation instead of a second per-form quota.
fn validate_form_data(form: &Form, charge: &mut impl FnMut(u64) -> Result<()>) -> Result<()> {
    let mut pending = vec![form];
    while let Some(form) = pending.pop() {
        let text_units = match &form.kind {
            Kind::String(value) => value.len(),
            Kind::Symbol(value) => value.to_string().encode_utf16().count(),
            Kind::Keyword(value) => {
                value.name.encode_utf16().count()
                    + value
                        .namespace
                        .as_ref()
                        .map_or(0, |ns| ns.encode_utf16().count() + 1)
            }
            _ => 0,
        };
        charge(1u64.saturating_add(text_units as u64))?;
        match &form.kind {
            Kind::List(items) | Kind::Vector(items) | Kind::Map(items) | Kind::Set(items) => {
                if matches!(form.kind, Kind::Map(_)) && items.len() % 2 != 0 {
                    return Err(SessionError::Host(wasmtime::Error::msg(
                        "Macro form map requires paired entries",
                    )));
                }
                pending.extend(items.iter().rev());
            }
            Kind::Conditional(_)
            | Kind::SplicingConditional(_)
            | Kind::Discard(_)
            | Kind::Prefix { .. } => {
                return Err(SessionError::Host(wasmtime::Error::msg(
                    "Native macro data requires resolved reader prefixes",
                )));
            }
            _ => {}
        }
        if !form.metadata.is_empty() {
            if !matches!(
                form.kind,
                Kind::Symbol(_) | Kind::List(_) | Kind::Vector(_) | Kind::Map(_) | Kind::Set(_)
            ) {
                return Err(SessionError::Host(wasmtime::Error::msg(
                    "Metadata requires a symbol or collection",
                )));
            }
            // Normalize and clone metadata within the same graph work budget.
            charge(form.metadata.len() as u64)?;
            let pairs = preserve_source_error(form.span.clone(), charge, |charge| {
                hir::reader_metadata_pairs_metered(form, &mut |work| charge(work))
            })?;
            let normalized = Form {
                span: form.span.clone(),
                metadata: vec![],
                kind: Kind::Map(pairs),
            };
            // Keep the temporary alive until it is visited below by validating
            // it immediately with a second iterative stack.
            let mut metadata_pending = vec![&normalized];
            while let Some(item) = metadata_pending.pop() {
                let used = match &item.kind {
                    Kind::String(value) => value.len(),
                    Kind::Symbol(value) => value.to_string().encode_utf16().count(),
                    Kind::Keyword(value) => {
                        value.name.encode_utf16().count()
                            + value
                                .namespace
                                .as_ref()
                                .map_or(0, |ns| ns.encode_utf16().count() + 1)
                    }
                    _ => 0,
                };
                charge(1u64.saturating_add(used as u64))?;
                match &item.kind {
                    Kind::Map(items) if items.len() % 2 == 0 => {
                        metadata_pending.extend(items.iter().rev())
                    }
                    Kind::Map(_) => {
                        return Err(SessionError::Host(wasmtime::Error::msg(
                            "Macro form map requires paired entries",
                        )));
                    }
                    Kind::List(items) | Kind::Vector(items) | Kind::Set(items) => {
                        metadata_pending.extend(items.iter().rev())
                    }
                    Kind::Conditional(_)
                    | Kind::SplicingConditional(_)
                    | Kind::Discard(_)
                    | Kind::Prefix { .. } => {
                        return Err(SessionError::Host(wasmtime::Error::msg(
                            "Native macro data requires resolved reader prefixes",
                        )));
                    }
                    _ => {}
                }
            }
        }
        pending.extend(form.metadata.iter().rev());
    }
    Ok(())
}

// Exact, framed runtime reader-data identity: metadata/order and binary64 bits
// remain distinct. Bare byte spans are not carried by FormBridge; source positions
// remain in the actual reader metadata and separate compiler records. This is
// local graph sharing, not a persisted artifact key.
#[cfg(test)]
fn form_key(form: &Form) -> Result<Vec<u8>> {
    form_key_metered(form, &mut |_| Ok(()))
}

#[cfg(test)]
fn form_key_metered(form: &Form, charge: &mut impl FnMut(u64) -> Result<()>) -> Result<Vec<u8>> {
    enum Task<'a> {
        Form(&'a Form),
        Kind(&'a Form),
    }
    fn append(
        key: &mut Vec<u8>,
        bytes: &[u8],
        charge: &mut impl FnMut(u64) -> Result<()>,
    ) -> Result<()> {
        charge(bytes.len() as u64)?;
        key.extend_from_slice(bytes);
        Ok(())
    }
    fn write_size(
        key: &mut Vec<u8>,
        value: usize,
        charge: &mut impl FnMut(u64) -> Result<()>,
    ) -> Result<()> {
        append(key, &(value as u64).to_le_bytes(), charge)
    }
    fn write_text(
        key: &mut Vec<u8>,
        value: &str,
        charge: &mut impl FnMut(u64) -> Result<()>,
    ) -> Result<()> {
        write_size(key, value.len(), charge)?;
        append(key, value.as_bytes(), charge)
    }
    fn write_identifier(
        key: &mut Vec<u8>,
        namespace: Option<&str>,
        name: &str,
        charge: &mut impl FnMut(u64) -> Result<()>,
    ) -> Result<()> {
        append(key, &[u8::from(namespace.is_some())], charge)?;
        if let Some(namespace) = namespace {
            write_text(key, namespace, charge)?;
        }
        write_text(key, name, charge)
    }
    let mut key = Vec::new();
    let mut pending = vec![Task::Form(form)];
    while let Some(task) = pending.pop() {
        match task {
            Task::Form(form) => {
                charge(1)?;
                write_size(&mut key, form.metadata.len(), charge)?;
                pending.push(Task::Kind(form));
                pending.extend(form.metadata.iter().rev().map(Task::Form));
            }
            Task::Kind(form) => match &form.kind {
                Kind::Nil => append(&mut key, &[0], charge)?,
                Kind::Bool(value) => append(&mut key, &[1, u8::from(*value)], charge)?,
                Kind::Number(value) => {
                    append(&mut key, &[2], charge)?;
                    append(&mut key, &value.to_bits().to_le_bytes(), charge)?;
                }
                Kind::String(value) => {
                    append(&mut key, &[3], charge)?;
                    write_size(&mut key, value.len(), charge)?;
                    for unit in value {
                        append(&mut key, &unit.to_le_bytes(), charge)?;
                    }
                }
                Kind::Symbol(value) => {
                    append(&mut key, &[4], charge)?;
                    write_identifier(&mut key, value.namespace.as_deref(), &value.name, charge)?;
                }
                Kind::Keyword(value) => {
                    append(&mut key, &[5], charge)?;
                    write_identifier(&mut key, value.namespace.as_deref(), &value.name, charge)?;
                }
                Kind::List(items)
                | Kind::Vector(items)
                | Kind::Set(items)
                | Kind::Map(items)
                | Kind::Conditional(items)
                | Kind::SplicingConditional(items) => {
                    let tag = match &form.kind {
                        Kind::List(_) => 6,
                        Kind::Vector(_) => 7,
                        Kind::Set(_) => 8,
                        Kind::Map(_) => 9,
                        Kind::Conditional(_) => 10,
                        Kind::SplicingConditional(_) => 13,
                        _ => unreachable!(),
                    };
                    append(&mut key, &[tag], charge)?;
                    write_size(&mut key, items.len(), charge)?;
                    pending.extend(items.iter().rev().map(Task::Form));
                }
                Kind::Discard(value) => {
                    append(&mut key, &[11], charge)?;
                    pending.push(Task::Form(value));
                }
                Kind::Prefix { operator, target } => {
                    append(&mut key, &[12], charge)?;
                    pending.push(Task::Form(target));
                    pending.push(Task::Form(operator));
                }
            },
        }
    }
    Ok(key)
}

enum Recipe {
    Scalar(Literal),
    Identifier {
        namespace: Option<String>,
        name: String,
        keyword: bool,
        literal: Option<AnalyzerLiteralKeyword>,
    },
    Form(Form),
    List(Vec<Value>),
    ReaderMap(Vec<(Value, Value)>),
    ReaderVector(Vec<Value>),
    ReaderSet(Vec<Value>),
    Metadata {
        shape: MetadataShape,
        value: Value,
        metadata: Value,
        set_items: Vec<Value>,
    },
    Map(Vec<(Value, Value)>),
    Vector(Vec<Value>),
    Set(Vec<Value>),
    Alias(Value),
}
enum Task {
    Namespace(Arc<SourceNamespace>),
    Local(Arc<LocalBinding>),
    Field(Arc<FieldBinding>),
    Scope(Arc<FunctionScope>),
    Callable(Arc<hir::SourceCallable>),
    Ast(Hir),
    Lowering(Hir),
}

/// Memo tables own their source identities as well as the resulting GC roots.
/// Recipe memo tables live for one build. Captured declaration metadata may be
/// reused only through checked source/representation records after callbacks.
/// An owned revision pointer is not an artifact cache key.
pub struct AnalysisGraph<'a> {
    bridge: &'a FormBridge,
    retained: Option<&'a mut DeclarationValues>,
    session: &'a mut Session,
    keys: [BTreeMap<String, Value>; 2],
    symbols: BTreeMap<String, Value>,
    raw_keys: Interner,
    // Invocation-local empty storage only: no live keys, nodes or form references.
    form_plan: Plan,
    forms: Vec<Option<Value>>,
    maps: BTreeMap<Vec<(Value, Value)>, Value>,
    vectors: BTreeMap<Vec<Value>, Value>,
    declarations: BTreeMap<
        (portable::resolve::Global, usize),
        (Option<Arc<portable::resolve::DefinitionInfo>>, Value),
    >,
    locals: BTreeMap<(usize, usize, String), (Arc<LocalBinding>, Value)>,
    fields: BTreeMap<usize, (Arc<FieldBinding>, Value)>,
    scopes: BTreeMap<usize, (Arc<FunctionScope>, Value)>,
    callables: BTreeMap<usize, (Arc<hir::SourceCallable>, Value)>,
    namespaces: BTreeMap<usize, (Arc<SourceNamespace>, Value)>,
    namespace_contents: BTreeMap<NamespaceKey, Value>,
    asts: BTreeMap<usize, (Arc<hir::SourceAnalysis>, Value)>,
    recipes: Vec<Option<Recipe>>,
    metadata_sources: Vec<metadata_expectations::CapturedMetadata>,
    source_roots: source_roots::Captures,
    capture_roots: bool,
    jobs: VecDeque<(Value, Task)>,
}
impl<'a> AnalysisGraph<'a> {
    fn reader_key(&mut self, form: &Form) -> Result<KeyId> {
        let mut plan = Plan::default();
        let root = self.raw_keys.plan(form, &mut plan, &mut |units| {
            self.session.charge_host_work(units)
        })?;
        Ok(plan.nodes[root].key)
    }
    fn clone_form(&mut self, form: &Form) -> Result<Form> {
        crate::portable::clone_form_metered(form, &mut |units| {
            charge_origin_work(self.session, form.span.clone(), units)
        })
        .map_err(SessionError::Compile)
    }
    pub fn new(bridge: &'a FormBridge, session: &'a mut Session) -> Self {
        Self {
            bridge,
            retained: None,
            session,
            keys: std::array::from_fn(|_| BTreeMap::new()),
            symbols: BTreeMap::new(),
            raw_keys: Interner::default(),
            form_plan: Plan::default(),
            forms: Vec::new(),
            maps: BTreeMap::new(),
            vectors: BTreeMap::new(),
            declarations: BTreeMap::new(),
            locals: BTreeMap::new(),
            fields: BTreeMap::new(),
            scopes: BTreeMap::new(),
            callables: BTreeMap::new(),
            namespaces: BTreeMap::new(),
            namespace_contents: BTreeMap::new(),
            asts: BTreeMap::new(),
            recipes: Vec::new(),
            metadata_sources: Vec::new(),
            source_roots: source_roots::Captures::default(),
            capture_roots: false,
            jobs: VecDeque::new(),
        }
    }
    pub(crate) fn with_declaration_values(bridge: &'a FormBridge,session: &'a mut Session,values: &'a mut DeclarationValues)->Self {
        let mut graph=Self::new(bridge,session);graph.retained=Some(values);graph.capture_roots=true;graph
    }
    fn recipe(&mut self, recipe: Recipe) -> Result<Value> {
        self.charge(1)?;
        let id = self.recipes.len();
        self.recipes.push(Some(recipe));
        Ok(id)
    }
    fn task(&mut self, task: Task) -> Result<Value> {
        self.charge(1)?;
        let id = self.recipes.len();
        self.recipes.push(None);
        self.jobs.push_back((id, task));
        Ok(id)
    }
    fn finish_recipes(&mut self) -> Result<()> {
        while let Some((id, task)) = self.jobs.pop_front() {
            match task {
                Task::Namespace(value) => self.namespace_record(id, &value, 0)?,
                Task::Local(value) => self.local_record(id, &value, 0)?,
                Task::Field(value) => self.field_record(id, &value, 0)?,
                Task::Scope(value) => self.scope_record(id, &value, 0)?,
                Task::Callable(value) => self.callable_record(id, &value, 0)?,
                Task::Ast(value) => self.ast_record(id, &value, 0)?,
                Task::Lowering(value) => self.lowering_record(id, &value, 0)?,
            };
            debug_assert!(self.recipes[id].is_some(), "task root was not finalized");
        }
        Ok(())
    }
    fn materialize(&mut self,root:Value)->Result<SessionValue> {
        if let Some(cache)=self.retained.take() {
            let result=self.materialize_reusing(root,cache);
            self.retained=Some(cache);result
        } else {self.materialize_using(root,self.bridge)}
    }
    fn materialize_using(&mut self,root:Value,bridge:&FormBridge)->Result<SessionValue> {
        self.materialize_controlled(root,bridge,None)
    }
    fn materialize_recorded_subroot(&mut self, root: Value, bridge: &FormBridge) -> Result<SessionValue> {
        self.finish_recipes()?;
        // Preserve the current recorded-root sparse admission and its fee.
        self.session.charge_host_work(4)?;
        if self.recipes.len() <= materialization_index::SPARSE_THRESHOLD {
            return self.materialize_controlled(root, bridge, None);
        }
        let index = materialization_index::Sparse::new(&mut |n| self.session.charge_host_work(n))?;
        self.materialize_indexed(root, bridge, None, index)
    }
    fn materialize_controlled(&mut self,root:Value,bridge:&FormBridge,control:Option<&mut reuse::Build<'_>>)->Result<SessionValue> {
        self.session.charge_host_work(1)?; // current explicit dense adapter
        self.materialize_indexed(root, bridge, control, materialization_index::Dense)
    }
    // Preserve the current generic interface used by recorded_scratch_tests.
    fn materialize_indexed<I: materialization_index::Index>(
        &mut self, root: Value, bridge: &FormBridge,
        control: Option<&mut reuse::Build<'_>>, index: I,
    ) -> Result<SessionValue> {
        self.session.charge_host_work(1)?; // new forwarding adapter work
        self.materialize_indexed_with_inputs(root, bridge, control, None, index)
    }
    fn materialize_with_inputs(
        &mut self, root: Value, bridge: &FormBridge,
        control: Option<&mut reuse::Build<'_>>,
        source: Option<(&source_roots::SourceInputs, &FormBridge)>,
    ) -> Result<SessionValue> {
        self.session.charge_host_work(1)?; // source-aware dense adapter work
        self.materialize_indexed_with_inputs(root, bridge, control, source, materialization_index::Dense)
    }
    fn materialize_indexed_with_inputs<I: materialization_index::Index>(
        &mut self, root: Value, bridge: &FormBridge,
        mut control: Option<&mut reuse::Build<'_>>,
        source: Option<(&source_roots::SourceInputs, &FormBridge)>, mut index: I,
    ) -> Result<SessionValue> {
        self.finish_recipes()?;
        if source.is_some() && control.is_some() {
            return Err(SessionError::Host(wasmtime::Error::msg("Source and legacy metadata controllers cannot overlap")));
        }
        let has_inputs = control.is_some() || source.is_some();
        fn dependencies(recipe: &Recipe) -> Vec<Value> {
            match recipe {
                Recipe::Alias(value) => vec![*value],
                Recipe::List(values)
                | Recipe::Vector(values)
                | Recipe::Set(values)
                | Recipe::ReaderVector(values)
                | Recipe::ReaderSet(values) => values.clone(),
                Recipe::Metadata {
                    value,
                    metadata,
                    set_items,
                    ..
                } => std::iter::once(*value)
                    .chain(set_items.iter().copied())
                    .chain(std::iter::once(*metadata))
                    .collect(),
                Recipe::Map(entries) | Recipe::ReaderMap(entries) => entries
                    .iter()
                    .flat_map(|(key, value)| [*key, *value])
                    .collect(),
                _ => vec![],
            }
        }
        fn dependency_count(recipe: &Recipe) -> usize {
            match recipe {
                Recipe::Alias(_) => 1,
                Recipe::List(values)
                | Recipe::Vector(values)
                | Recipe::Set(values)
                | Recipe::ReaderVector(values)
                | Recipe::ReaderSet(values) => values.len(),
                Recipe::Metadata { set_items, .. } => set_items.len().saturating_add(2),
                Recipe::Map(entries) | Recipe::ReaderMap(entries) => entries.len().saturating_mul(2),
                _ => 0,
            }
        }
        // One shared traversal, addressed either by exact recipe ID or by a
        // private native-ID ordinal. Dense retains its original 3N + N fees.
        let (mut state, mut repeat) = index.states(self.recipes.len(),
            &mut |n| self.session.charge_host_work(n))?;
        let mut postorder = index.postorder(self.recipes.len(),
            &mut |n| self.session.charge_host_work(n))?;
        let mut stack = vec![(root, false)];
        while let Some((id, finish)) = stack.pop() {
            let slot = index.enter(id, &mut state, &mut repeat,
                &mut |n| self.session.charge_host_work(n))?;
            if state[slot] == 2 {
                // Its incoming edge was already charged when queued. A shared
                // node that is fully checked needs no second validation visit;
                // reader occurrences retain their separate cold work budget.
                continue;
            }
            self.session.charge_host_work(1)?;
            let recipe = self.recipes[id].as_ref().expect("all graph tasks filled");
            if !finish {
                if state[slot] == 1 {
                    return Err(SessionError::Host(wasmtime::Error::msg(
                        "Cyclic compiler analysis graph",
                    )));
                }
                state[slot] = 1;
                stack.push((id, true));
                self.session.charge_host_work(dependency_count(recipe) as u64)?;
                let dependencies = if (control.as_ref().is_some_and(|b|b.intercepts(id)) || source.is_some_and(|(inputs,_)|inputs.intercepts(id))) {vec![]} else {dependencies(recipe)};
                for dependency in dependencies.into_iter().rev() {
                    stack.push((dependency, false));
                }
            } else {
                self.session.charge_host_work(dependency_count(recipe) as u64)?;
                let dependencies = if (control.as_ref().is_some_and(|b|b.intercepts(id)) || source.is_some_and(|(inputs,_)|inputs.intercepts(id))) {vec![]} else {dependencies(recipe)};
                repeat[slot] = match recipe {
                    Recipe::ReaderMap(_) | Recipe::ReaderSet(_) => true,
                    Recipe::List(_) | Recipe::ReaderVector(_) | Recipe::Metadata { .. } => {
                        matches!(
                            recipe,
                            Recipe::Metadata {
                                shape: MetadataShape::Set,
                                ..
                            }
                        ) || index.any_repeat(&dependencies, &repeat,
                            &mut |n| self.session.charge_host_work(n))?
                    }
                    // Immutable compiler record/namespace identities are sharing
                    // barriers, distinct from occurrences inside reader data.
                    _ => false,
                };
                state[slot] = 2;
                index.push_postorder(&mut postorder, id,
                    &mut |n| self.session.charge_host_work(n))?;
            }
        }
        struct Frame {
            id: Value,
            dependencies: Vec<Value>,
            next: usize,
            inputs: Vec<SessionValue>,
            tainted: bool,
            input_taint: Vec<bool>,
        }
        impl Frame {
            fn new(id: Value, recipe: &Recipe,skip:bool) -> Self {
                Self {
                    id,
                    dependencies: if skip {vec![]} else {dependencies(recipe)},
                    next: 0,
                    inputs: vec![],
                    tainted: false,
                    input_taint: Vec::new(),
                }
            }
        }
        // Count logical frame work on the compact DAG before any callbacks.
        // The actual traversal debits that work once; this planner debits only
        // its own counting/copying work, never a hypothetical expanded walk.
        // Seal membership before counting or entering any guest callback.
        let index = &index;
        let slots = state.len();
        let degree = |id: Value| if control.as_ref().is_some_and(|b| b.intercepts(id)) || source.is_some_and(|(inputs,_)| inputs.intercepts(id)) {0}
            else {dependency_count(self.recipes[id].as_ref().unwrap())};
        let edges = |id: Value| if control.as_ref().is_some_and(|b| b.intercepts(id)) || source.is_some_and(|(inputs,_)| inputs.intercepts(id)) {vec![]}
            else {dependencies(self.recipes[id].as_ref().unwrap())};
        let mut charge = |n| self.session.charge_host_work(n);
        let work = if I::DENSE {
            occurrence_work::count(slots, root, &postorder, &repeat, degree, edges, &mut charge)?
        } else {
            occurrence_work::count_indexed(slots, root, &postorder, &repeat,
                degree, edges, |id, charge| index.slot(id, charge), &mut charge)?
        };
        let remaining = self.session.operation_fuel_remaining()?;
        if work.is_none_or(|work| work > remaining) {
            // Overflow proves that even a u64::MAX allowance cannot admit the
            // traversal. Exhaust using the same typed Store-fuel mechanism.
            self.session.charge_host_work(u64::MAX)?;
            self.session.charge_host_work(1)?;
            unreachable!("an insufficient or overflowing work count must exhaust fuel");
        }
        // Dense prepaid cache initialization in 3N. Sparse pays its actual R.
        if !I::DENSE { self.session.charge_host_work(slots as u64)?; }
        let mut cached: Vec<Option<SessionValue>> = vec![None; slots];
        self.session.charge_host_work(if has_inputs {slots as u64}else{0})?;
        let mut value_taint=if has_inputs {vec![false;slots]}else{Vec::new()};
        let root_recipe = self.recipes[root].as_ref().unwrap();
        self.session
            .charge_host_work(dependency_count(root_recipe) as u64)?;
        let mut frames = vec![Frame::new(root, root_recipe, (control.as_ref().is_some_and(|b|b.intercepts(root)) || source.is_some_and(|(inputs,_)|inputs.intercepts(root))))];
        // DAG storage remains bounded independently of occurrence execution.
        // Charge before descending or invoking callbacks; diamonds cannot expand
        // into unbounded work even when the distinct recipe count is small.
        while !frames.is_empty() {
            let frame = frames.last_mut().unwrap();
            if frame.next < frame.dependencies.len() {
                self.charge(1)?;
                let id = frame.dependencies[frame.next];
                frame.next += 1;
                let slot = index.slot(id, &mut |n| self.session.charge_host_work(n))?;
                if !repeat[slot]
                    && let Some(value) = &cached[slot]
                {
                    if has_inputs {
                        frame.tainted |= value_taint[slot];
                        reuse::reserve(self.session, &mut frame.input_taint, 1)?;
                        frame.input_taint.push(value_taint[slot]);
                    }
                    frame.inputs.push(value.clone());
                } else {
                    let recipe = self.recipes[id].as_ref().unwrap();
                    self.session
                        .charge_host_work(dependency_count(recipe) as u64)?;
                    frames.push(Frame::new(id, recipe, (control.as_ref().is_some_and(|b|b.intercepts(id)) || source.is_some_and(|(inputs,_)|inputs.intercepts(id)))));
                }
                continue;
            }
            let frame = frames.pop().unwrap();
            let slot = index.slot(frame.id, &mut |n| self.session.charge_host_work(n))?;
            self.charge(1)?;
            let inputs = frame.inputs;
            if control.is_some() && frame.tainted && !(control.as_ref().is_some_and(|b| b.intercepts(frame.id)) || source.is_some_and(|(inputs,_)|inputs.intercepts(frame.id))) {
                for (value, tainted) in inputs.iter().zip(&frame.input_taint) {
                    self.session.charge_host_work(1)?;
                    if !tainted {control.as_mut().unwrap().cut(self.session, value)?;}
                }
            }
            if control.is_some() && frame.tainted && !(control.as_ref().is_some_and(|b|b.intercepts(frame.id)) || source.is_some_and(|(inputs,_)|inputs.intercepts(frame.id))) {
                // Exhaustive route decision BEFORE construction, never an error retry.
                // All allocating collection routes funnel through array/construct;
                // aliases allocate nothing. Leaves cannot carry incoming tokens.
                let covered=match self.recipes[frame.id].as_ref().unwrap() {
                    Recipe::Alias(_) | Recipe::List(_) | Recipe::Vector(_) | Recipe::ReaderVector(_)
                    | Recipe::Map(_) | Recipe::ReaderMap(_) | Recipe::Set(_) | Recipe::ReaderSet(_)
                    | Recipe::Metadata{..}=>true,
                    Recipe::Scalar(_) | Recipe::Identifier{..} | Recipe::Form(_)=>false,
                };
                control.as_mut().unwrap().note_slot_route(self.session,covered)?;
            }
            let active_bridge = if !frame.tainted && has_inputs {
                source.map_or(self.bridge, |(_, normal)| normal)
            } else { bridge };
            let value = if let Some((inputs, _)) = source.filter(|(inputs,_)| inputs.intercepts(frame.id)) {
                self.session.charge_host_work(1)?;
                inputs.selected(self.session, frame.id)?.ok_or_else(|| SessionError::Host(wasmtime::Error::msg("Missing selected source input")))?.clone()
            } else if control.as_ref().is_some_and(|b|b.intercepts(frame.id)) {
                control.as_mut().unwrap().produce(self,frame.id)?
            } else {let recipe = self.recipes[frame.id].as_ref().unwrap();match recipe {
                Recipe::Scalar(value) => active_bridge.scalar(self.session, value)?,
                Recipe::Identifier {
                    namespace,
                    name,
                    keyword,
                    literal,
                } => if let Some(token) = literal {
                    active_bridge.analyzer_literal_keyword(self.session, *token)?
                } else { active_bridge.identifier(
                    self.session,
                    namespace.as_deref(),
                    name,
                    *keyword,
                    None,
                )? },
                Recipe::Form(form) => {
                    let form = crate::portable::clone_form_metered(form, &mut |units| {
                        charge_origin_work(self.session, form.span.clone(), units)
                    })
                    .map_err(SessionError::Compile)?;
                    active_bridge.quote(self.session, form)?
                }
                Recipe::Alias(_) => inputs[0].clone(),
                Recipe::List(_) => active_bridge.list_values(self.session, &inputs)?,
                Recipe::Vector(_) | Recipe::ReaderVector(_) => {
                    active_bridge.vector_values(self.session, &inputs)?
                }
                Recipe::Set(_) | Recipe::ReaderSet(_) => {
                    active_bridge.set_values(self.session, &inputs)?
                }
                Recipe::Map(_) | Recipe::ReaderMap(_) => active_bridge.map_values(
                    self.session,
                    &inputs
                        .chunks_exact(2)
                        .map(|pair| (pair[0].clone(), pair[1].clone()))
                        .collect::<Vec<_>>(),
                )?,
                Recipe::Metadata {
                    shape, set_items, ..
                } => active_bridge.with_form_metadata(
                    self.session,
                    *shape,
                    &inputs[0],
                    inputs.last().unwrap(),
                    &inputs[1..1 + set_items.len()],
                )?,
            }};
            let tainted=if source.is_some_and(|(inputs,_)|inputs.intercepts(frame.id)) {
                true
            } else if control.as_ref().is_some_and(|b|b.intercepts(frame.id)) {
                control.as_ref().unwrap().last_is_private(frame.id)
            } else {frame.tainted};
            if has_inputs {value_taint[slot]=tainted;}
            if !repeat[slot] {
                cached[slot] = Some(value.clone());
            }
            if let Some(parent) = frames.last_mut() {
                parent.tainted |= tainted;
                if has_inputs {
                    reuse::reserve(self.session, &mut parent.input_taint, 1)?;
                    parent.input_taint.push(tainted);
                }
                parent.inputs.push(value);
            } else {
                return Ok(value);
            }
        }
        unreachable!("graph has a root")
    }
    fn charge(&mut self, units: usize) -> Result<()> {
        self.session.charge_host_work(units as u64)
    }
    fn scalar(&mut self, value: Literal) -> Result<Value> {
        let kind = match &value {
            Literal::Nil => Kind::Nil,
            Literal::Bool(value) => Kind::Bool(*value),
            Literal::Number(value) => Kind::Number(*value),
            Literal::String(value) => {
                self.charge(value.len())?;
                Kind::String(value.clone())
            },
            Literal::Undefined => {
                self.charge(0)?;
                return self.recipe(Recipe::Scalar(value));
            }
        };
        let key = self.reader_key(&Form {
            span: 0..0,
            metadata: vec![],
            kind,
        })?;
        self.charge(1)?;
        if let Some(value) = self.forms.get(key.0).and_then(Option::as_ref) {
            return Ok(*value);
        }
        self.charge(if let Literal::String(units) = &value {
            units.len()
        } else {
            0
        })?;
        let value = self.recipe(Recipe::Scalar(value))?;
        self.store_form_key(key, value)?;
        Ok(value)
    }
    fn text(&mut self, value: &str) -> Result<Value> {
        self.scalar(Literal::String(value.encode_utf16().collect()))
    }
    fn number(&mut self, value: usize) -> Result<Value> {
        self.scalar(Literal::Number(value as f64))
    }
    fn flag(&mut self, value: bool) -> Result<Value> {
        self.scalar(Literal::Bool(value))
    }
    fn keyword<'k>(&mut self, key: impl Into<FieldKey<'k>>) -> Result<Value> {
        let key = key.into();
        let name = key.name();
        let literal = match key { FieldKey::Cold(_) => None, FieldKey::Literal(token) => Some(token) };
        self.charge(name.len().saturating_add(2))?;
        let origin = usize::from(literal.is_some());
        if let Some(value) = self.keys[origin].get(name) { return Ok(*value); }
        self.charge(name.encode_utf16().count() * 2 + std::mem::size_of::<bool>())?;
        let (namespace, name_part) = name.split_once('/').map_or((None, name), |(ns, name)| (Some(ns), name));
        let value = self.recipe(Recipe::Identifier {
            namespace: namespace.map(str::to_owned), name: name_part.into(), keyword: true, literal,
        })?;
        self.keys[origin].insert(name.to_owned(), value);
        Ok(value)
    }
    fn symbol(&mut self, name: &str) -> Result<Value> {
        // Only compiler-created plain symbols are interned. Reader forms retain
        // their own metadata, spans and provenance through form transport.
        self.charge(name.len().saturating_add(1))?;
        if let Some(value) = self.symbols.get(name) {
            return Ok(*value);
        }
        self.charge(name.encode_utf16().count() * 2)?;
        let (namespace, short) = name
            .split_once('/')
            .filter(|_| name != "/")
            .map_or((None, name), |(ns, name)| (Some(ns), name));
        let value = self.recipe(Recipe::Identifier {
            namespace: namespace.map(str::to_owned),
            name: short.into(),
            keyword: false,
            literal: None,
        })?;
        self.symbols.insert(name.into(), value);
        Ok(value)
    }
    fn map<'k, K: Into<FieldKey<'k>>>(&mut self, entries: Vec<(K, Value)>) -> Result<Value> {
        let entries = entries
            .into_iter()
            .map(|(key, value)| Ok((self.keyword(key)?, value)))
            .collect::<Result<Vec<_>>>()?;
        self.map_values(entries)
    }
    fn map_values(&mut self, entries: Vec<(Value, Value)>) -> Result<Value> {
        self.charge(entries.len().saturating_add(1))?;
        if let Some(value) = self.maps.get(&entries) {
            return Ok(*value);
        }
        self.charge(0)?;
        let value = self.recipe(Recipe::Map(entries.clone()))?;
        self.maps.insert(entries, value);
        Ok(value)
    }
    /// Finalize a task in its already-charged placeholder. New immutable maps
    /// occupy that slot directly; a cache hit keeps an alias to the canonical
    /// map recipe so runtime identity sharing remains unchanged.
    fn map_in_task_slot<'k, K: Into<FieldKey<'k>>>(&mut self, slot: Value, entries: Vec<(K, Value)>) -> Result<Value> {
        let entries = entries
            .into_iter()
            .map(|(key, value)| Ok((self.keyword(key)?, value)))
            .collect::<Result<Vec<_>>>()?;
        self.charge(entries.len().saturating_add(1))?;
        if let Some(value) = self.maps.get(&entries) {
            self.recipes[slot] = Some(Recipe::Alias(*value));
            return Ok(slot);
        }
        self.recipes[slot] = Some(Recipe::Map(entries.clone()));
        self.maps.insert(entries, slot);
        Ok(slot)
    }
    fn vector(&mut self, values: &[Value]) -> Result<Value> {
        self.charge(values.len().saturating_add(1))?;
        if let Some(value) = self.vectors.get(values) {
            return Ok(*value);
        }
        self.charge(0)?;
        let value = self.recipe(Recipe::Vector(values.to_vec()))?;
        self.vectors.insert(values.to_vec(), value);
        Ok(value)
    }
    fn form(&mut self, form: &Form, _depth: usize) -> Result<Value> {
        validate_form_data(form, &mut |units| self.session.charge_host_work(units))?;
        self.form_data(form, 0)
    }
    fn store_form_key(&mut self, key: KeyId, value: Value) -> Result<()> {
        raw_keys::bind(&mut self.forms, key, value, &mut |units| {
            self.session.charge_host_work(units)
        })
    }
    fn form_data(&mut self, form: &Form, reader_depth: usize) -> Result<Value> {
        self.form_data_selected(form, reader_depth, None, &mut None)
    }
    fn definition_with_initializer(
        &mut self, definition: &Form, initializer: Option<&Form>, depth: usize,
    ) -> Result<(Value, Option<Value>)> {
        // Exactly the existing full definition guard, before any correspondence
        // or recipe lookup. It also validates the candidate child and metadata.
        validate_form_data(definition, &mut |units| self.session.charge_host_work(units))?;
        let selected = match (&definition.kind, initializer) {
            (Kind::List(items), Some(initializer)) if !items.is_empty() => {
                if initializer_plan::same(self.session, items.last().unwrap(), initializer)? {
                    Some(items.len() - 1)
                } else { None }
            }
            _ => None,
        };
        let mut child_key = None;
        let value = self.form_data_selected(definition, depth, selected, &mut child_key)?;
        self.charge(1)?;
        let child = child_key.and_then(|key| self.forms.get(key.0).and_then(Option::as_ref).copied());
        Ok((value, child))
    }
    fn form_data_selected(
        &mut self, form: &Form, reader_depth: usize, selected: Option<usize>,
        child_key: &mut Option<KeyId>,
    ) -> Result<Value> {
        self.charge(8)?;
        let mut plan = std::mem::take(&mut self.form_plan);
        debug_assert!(plan.nodes.is_empty());
        // All early returns remain inside the original builder. A failed build
        // or failed reset drops its local Plan normally, retaining no entries
        // and preserving the original build error without fallible cleanup.
        let value = self.form_data_planned(form, reader_depth, selected, child_key, &mut plan)?;
        plan.clear_metered(&mut |units| self.session.charge_host_work(units))?;
        self.charge(8)?;
        self.form_plan = plan;
        Ok(value)
    }
    fn form_data_planned(
        &mut self, form: &Form, _reader_depth: usize, selected: Option<usize>,
        child_key: &mut Option<KeyId>, plan: &mut Plan,
    ) -> Result<Value> {
        #[derive(Clone, Copy)]
        enum Shape {
            List,
            Vector,
            Set,
            Map,
            Form,
        }
        enum Input<'a> { Borrowed(&'a Form), Owned(Form) }
        impl Input<'_> {
            fn form(&self) -> &Form {
                match self { Self::Borrowed(form) => form, Self::Owned(form) => form }
            }
            fn leaf(self, session: &mut Session) -> Result<Form> {
                match self {
                    Self::Owned(mut form) => {
                        let kind = form.take_kind();
                        Ok(Form { span: form.span.clone(), metadata: vec![], kind })
                    }
                    Self::Borrowed(form) => {
                        let bytes = match &form.kind {
                            Kind::String(v) => v.len().saturating_mul(2),
                            Kind::Symbol(v) => v.name.len().saturating_add(v.namespace.as_ref().map_or(0, String::len)),
                            Kind::Keyword(v) => v.name.len().saturating_add(v.namespace.as_ref().map_or(0, String::len)),
                            _ => 0,
                        };
                        session.charge_host_work((bytes as u64).saturating_add(1))?;
                        let kind = match &form.kind {
                            Kind::Nil => Kind::Nil,
                            Kind::Bool(v) => Kind::Bool(*v),
                            Kind::Number(v) => Kind::Number(*v),
                            Kind::String(v) => Kind::String(v.clone()),
                            Kind::Symbol(v) => Kind::Symbol(v.clone()),
                            Kind::Keyword(v) => Kind::Keyword(v.clone()),
                            _ => return Err(SessionError::Host(wasmtime::Error::msg("Expected atomic native form"))),
                        };
                        Ok(Form { span: form.span.clone(), metadata: vec![], kind })
                    }
                }
            }
        }
        enum Work<'a> {
            Metadata(crate::portable::hir::ReaderMetadata, std::ops::Range<usize>),
            Visit(Form, usize),
            BorrowedVisit(&'a Form, usize),
            Finish {
                key: KeyId,
                shape: Shape,
                child_count: usize,
                metadata_shape: Option<MetadataShape>,
                set_item_count: usize,
                base_value: Option<Value>,
            },
        }
        #[allow(dead_code)]
        enum OriginalWork<'a> {
            Visit(Form, usize),
            BorrowedVisit(&'a Form, usize),
            Finish {
                key: KeyId,
                shape: Shape,
                child_count: usize,
                metadata_shape: Option<MetadataShape>,
                set_item_count: usize,
                base_value: Option<Value>,
            },
        }
        // New metadata task must not enlarge the existing queue backing.
        // If its actual target layout grows, stop compilation until growth and
        // relocation are explicitly prepaid; no assumed byte-size discount.
        const _: () = assert!(std::mem::size_of::<Work<'static>>() == std::mem::size_of::<OriginalWork<'static>>());


        // Identity planning is native-only. Raw metadata participates in keys,
        // but only the existing normalized-metadata path builds guest recipes.
        let root_plan = self.raw_keys.plan(form, plan, &mut |units| {
            self.session.charge_host_work(units)
        })?;
        if let Some(index) = selected {
            self.charge(2)?;
            let node = &plan.nodes[root_plan];
            *child_key = node.children.get(node.metadata_count + index)
                .map(|id| plan.nodes[*id].key);
        }
        // A recipe hit needs no owned payload. Plan the exact borrowed key
        // before copying, retaining full validation and source-data equality.
        self.charge(1)?;
        let root_key = plan.nodes[root_plan].key;
        if let Some(value) = self.forms.get(root_key.0).and_then(Option::as_ref).copied() {
            self.charge(1)?;
            return Ok(value);
        }
        // Retain the existing owned-visit stack for generated metadata. Body
        // visits borrow immutable inputs; no child arena or whole-tree copy.
        let mut work = vec![Work::BorrowedVisit(form, root_plan)];
        let mut values = Vec::<Value>::new();
        while let Some(item) = work.pop() {
            let (input, plan_id) = match item {
                Work::Metadata(metadata, span) => {
                    let value = stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
                        self.reader_metadata_recipe(metadata, span)
                    })?;
                    values.push(value);
                    continue;
                }
                Work::BorrowedVisit(form, id) => (Input::Borrowed(form), id),
                Work::Visit(form, id) => (Input::Owned(form), id),
                Work::Finish {
                    key,
                    shape,
                    child_count,
                    metadata_shape,
                    set_item_count,
                    base_value,
                } => {
                    if values.len() < child_count {
                        return Err(SessionError::Host(wasmtime::Error::msg(
                            "Invalid reader form work stack",
                        )));
                    }
                    let children = values.split_off(values.len() - child_count);
                    let metadata_offset = usize::from(metadata_shape.is_some());
                    let data_values = &children[..child_count - metadata_offset];
                    let base = match (shape, base_value) {
                        (Shape::List, _) => self.recipe(Recipe::List(data_values.to_vec()))?,
                        (Shape::Vector, _) => {
                            self.recipe(Recipe::ReaderVector(data_values.to_vec()))?
                        }
                        (Shape::Set, _) if metadata_shape.is_none() => {
                            self.recipe(Recipe::ReaderSet(data_values.to_vec()))?
                        }
                        (Shape::Set, _) => self.recipe(Recipe::Scalar(Literal::Nil))?,
                        (Shape::Map, _) => {
                            let entries = data_values
                                .chunks_exact(2)
                                .map(|pair| (pair[0], pair[1]))
                                .collect();
                            self.recipe(Recipe::ReaderMap(entries))?
                        }
                        (Shape::Form, Some(value)) => value,
                        (Shape::Form, None) => unreachable!(),
                    };
                    let value = if let Some(metadata_shape) = metadata_shape {
                        let metadata = *children.last().unwrap();
                        let set_items = if set_item_count > 0 {
                            data_values[..set_item_count].to_vec()
                        } else {
                            vec![]
                        };
                        self.recipe(Recipe::Metadata {
                            shape: metadata_shape,
                            value: base,
                            metadata,
                            set_items,
                        })?
                    } else {
                        base
                    };
                    self.store_form_key(key, value)?;
                    values.push(value);
                    continue;
                }
            };
            self.charge(1)?;
            let key = plan.nodes[plan_id].key;
            if let Some(value) = self.forms.get(key.0).and_then(Option::as_ref).copied() {
                self.charge(1)?;
                values.push(value);
                continue;
            }
            let form = input.form();
            if matches!(&form.kind, Kind::Nil | Kind::Bool(_) | Kind::Number(_) | Kind::String(_)) {
                if !form.metadata.is_empty() {
                    return Err(SessionError::Host(wasmtime::Error::msg("Metadata requires a symbol or collection")));
                }
                let mut leaf = input.leaf(self.session)?;
                let literal = match leaf.take_kind() {
                    Kind::Nil => Literal::Nil,
                    Kind::Bool(v) => Literal::Bool(v),
                    Kind::Number(v) => Literal::Number(v),
                    Kind::String(v) => Literal::String(v),
                    _ => unreachable!("scalar kind checked before ownership copy"),
                };
                let value = self.recipe(Recipe::Scalar(literal))?;
                self.store_form_key(key, value)?;
                values.push(value);
                continue;
            }
            let metadata_shape = if form.metadata.is_empty() { None } else {
                Some(match &form.kind {
                    Kind::Symbol(_) => MetadataShape::Symbol,
                    Kind::List(items) => MetadataShape::List { empty: items.is_empty() },
                    Kind::Vector(_) => MetadataShape::Vector,
                    Kind::Map(items) => MetadataShape::Map { large: items.len() > 16 },
                    Kind::Set(_) => MetadataShape::Set,
                    _ => return Err(SessionError::Host(wasmtime::Error::msg("Metadata requires a symbol or collection"))),
                })
            };
            let metadata_form = if metadata_shape.is_some() {
                Some((crate::portable_macro_data::normalized_reader_metadata_view(self.session, form)?, form.span.clone()))
            } else { None };
            let (shape, body_count) = match &form.kind {
                Kind::List(items) => (Shape::List, items.len()),
                Kind::Vector(items) => (Shape::Vector, items.len()),
                Kind::Set(items) => (Shape::Set, items.len()),
                Kind::Map(items) => {
                    if items.len() % 2 != 0 {
                        return Err(SessionError::Host(wasmtime::Error::msg("Macro form map requires paired entries")));
                    }
                    (Shape::Map, items.len())
                }
                Kind::Conditional(_) | Kind::SplicingConditional(_) | Kind::Discard(_) | Kind::Prefix { .. } => {
                    return Err(SessionError::Host(wasmtime::Error::msg("Native macro data requires resolved reader prefixes")));
                }
                _ => (Shape::Form, 0),
            };
            let mut base_value = None;
            let body = if matches!(shape, Shape::Form) {
                let data = input.leaf(self.session)?;
                let value = self.recipe(Recipe::Form(data))?;
                if metadata_shape.is_none() {
                    self.store_form_key(key, value)?;
                    values.push(value);
                    continue;
                }
                base_value = Some(value);
                None
            } else { Some(input) };
            let metadata_plan = metadata_form;
            work.push(Work::Finish {
                key, shape, child_count: body_count + usize::from(metadata_plan.is_some()),
                metadata_shape, set_item_count: if matches!(shape, Shape::Set) { body_count } else { 0 }, base_value,
            });
            // LIFO: push metadata first, then reversed body children, preserving
            // textual body order followed by normalized metadata.
            if let Some((metadata, span)) = metadata_plan {
                self.charge(std::mem::size_of::<hir::ReaderMetadata>().saturating_add(8))?;
                work.push(Work::Metadata(metadata, span));
            }
            let offset = plan.nodes[plan_id].metadata_count;
            match body {
                Some(Input::Borrowed(form)) => {
                    let items = match &form.kind {
                        Kind::List(v) | Kind::Vector(v) | Kind::Set(v) | Kind::Map(v) => v,
                        _ => unreachable!("collection shape checked above"),
                    };
                    for (index, child) in items.iter().enumerate().rev() {
                        work.push(Work::BorrowedVisit(child, plan.nodes[plan_id].children[offset + index]));
                    }
                }
                Some(Input::Owned(mut form)) => {
                    let items = match form.take_kind() {
                        Kind::List(v) | Kind::Vector(v) | Kind::Set(v) | Kind::Map(v) => v,
                        _ => unreachable!("collection shape checked above"),
                    };
                    for (index, child) in items.into_iter().enumerate().rev() {
                        work.push(Work::Visit(child, plan.nodes[plan_id].children[offset + index]));
                    }
                }
                None => {}
            }
        }
        if values.len() != 1 {
            return Err(SessionError::Host(wasmtime::Error::msg(
                "Invalid reader form result stack",
            )));
        }
        Ok(values[0])
    }
    fn depth(depth: usize) -> Result<()> {
        let _ = depth;
        Ok(())
    }
    fn context(&mut self, context: portable::AnalysisContext) -> Result<Value> {
        self.keyword(match context {
            portable::AnalysisContext::Statement => "statement",
            portable::AnalysisContext::Expression => "expr",
            portable::AnalysisContext::Return => "return",
        })
    }
    pub fn expansion(&mut self, context: portable::ExpansionContext<'_>) -> Result<SessionValue> {
        self.expansion_catalog(context, false, false)
    }
    /// Explicit compiler-helper transport; ordinary macros keep the existing
    /// graph schema/work. Full catalogs use the same guarded recipe builder.
    pub fn expansion_with_compiler_catalog(
        &mut self,
        context: portable::ExpansionContext<'_>,
    ) -> Result<SessionValue> {
        self.expansion_catalog(context, true, false)
    }
    /// Explicit record-helper resolution projection; excludes ASTs and var
    /// records while retaining genuine namespace membership and scope facts.
    pub fn expansion_with_record_catalog(
        &mut self,
        context: portable::ExpansionContext<'_>,
    ) -> Result<SessionValue> {
        self.expansion_catalog(context, true, true)
    }
    fn expansion_catalog(
        &mut self,
        context: portable::ExpansionContext<'_>,
        include_compiler: bool,
        record_bindings: bool,
    ) -> Result<SessionValue> {
        let catalog = context.resolution_catalog();
        let mut root = if record_bindings {
            let snapshot = self.namespace_membership(context.namespace_snapshot)?;
            let live = self.namespace_membership(&catalog)?;
            let projection = self.keyword("record-bindings")?;
            self.map(vec![
                ("ns", snapshot),
                ("suss/catalog", live),
                ("suss/projection", projection),
            ])?
        } else {
            self.environment(
                context.namespace_snapshot,
                &catalog,
                context.locals,
                context.fields,
                context.function_scopes,
                context.context,
                0,
            )?
        };
        if include_compiler {
            let compiler = portable::compiler_facts::CompilerNamespaceCatalog::capture(
                context.environment,
                context.phase,
            );
            let mut namespaces = Vec::new();
            for (name, revision) in &compiler.namespaces {
                let value = match revision {
                    Some(revision) if record_bindings => self.namespace_membership(revision)?,
                    Some(revision) => self.namespace(revision, 1)?,
                    None => self.scalar(Literal::Nil)?,
                };
                namespaces.push((self.symbol(name)?, value));
            }
            let namespaces = self.map_values(namespaces)?;
            let mut protocols = Vec::new();
            // Faithful lookup projection of the same phase-filtered protocol
            // identities. Helpers use local keys without rebuilding an index
            // inside each macro invocation. Metadata never supplies identity.
            let mut protocol_globals_by_namespace =
                std::collections::BTreeMap::<String, Vec<(Value, Value)>>::new();
            let mut protocol_names = std::collections::BTreeSet::new();
            for (global, methods) in &context.environment.protocols {
                if global.phase() != context.phase {
                    continue;
                }
                let mut signatures = Vec::new();
                for method in methods {
                    let arities = method
                        .signatures
                        .iter()
                        .map(|(arity, _)| self.number(*arity))
                        .collect::<Result<Vec<_>>>()?;
                    signatures.push((self.symbol(&method.name)?, self.vector(&arities)?));
                }
                let signatures = self.map_values(signatures)?;
                let identity = self.symbol(&format!("{}/{}", global.namespace(), global.name()))?;
                let local = self.symbol(global.name())?;
                protocol_names.insert(global.name().to_owned());
                protocol_globals_by_namespace
                    .entry(global.namespace().to_owned())
                    .or_default()
                    .push((local, identity));
                protocols.push((identity, signatures));
            }
            let protocols = self.map_values(protocols)?;
            let mut protocol_namespaces = Vec::new();
            for (namespace, globals) in protocol_globals_by_namespace {
                let namespace = self.text(&namespace)?;
                let globals = self.map_values(globals)?;
                protocol_namespaces.push((namespace, globals));
            }
            let protocol_globals_by_namespace = self.map_values(protocol_namespaces)?;
            // Resolve candidate names using the actual compiler scope. Preserve
            // refers/own/exclusions/core precedence; no metadata or signature
            // entry is treated as a resolved binding.
            let mut unqualified_protocol_bindings = Vec::new();
            for name in protocol_names {
                let local = suss_reader::Symbol::new(&name);
                if let Ok(portable::resolve::Binding::Cell(global)) =
                    context.environment.resolve(context.phase, &local, 0..0)
                {
                    let local = self.symbol(&name)?;
                    let identity =
                        self.symbol(&format!("{}/{}", global.namespace(), global.name()))?;
                    unqualified_protocol_bindings.push((local, identity));
                }
            }
            let unqualified_protocol_bindings = self.map_values(unqualified_protocol_bindings)?;
            let mut native_interfaces = Vec::new();
            for fact in &compiler.native_interfaces {
                let global = self.symbol(&format!(
                    "{}/{}",
                    fact.global.namespace(),
                    fact.global.name()
                ))?;
                let phase = self.keyword(match fact.global.phase() {
                    portable::resolve::Phase::Runtime => "runtime",
                    portable::resolve::Phase::Macro => "macro",
                })?;
                let binding_kind = self.keyword("environment-core")?;
                let export = self.text(fact.export)?;
                let extension_kind = self.keyword(match fact.kind {
                    portable::compiler_facts::NativeInterfaceKind::NativeObject => "native-object",
                })?;
                let value = self.map(vec![
                    ("global", global),
                    ("phase", phase),
                    ("binding-kind", binding_kind),
                    ("export", export),
                    ("extension-kind", extension_kind),
                ])?;
                native_interfaces.push((global, value));
            }
            let native_interfaces = self.map_values(native_interfaces)?;
            let phase = self.keyword(match compiler.phase {
                portable::resolve::Phase::Runtime => "runtime",
                portable::resolve::Phase::Macro => "macro",
            })?;
            let current = self.symbol(&compiler.current)?;
            let compiler = self.map(vec![
                ("phase", phase),
                ("current", current),
                ("namespaces", namespaces),
                ("protocol-signatures", protocols),
                (
                    "protocol-globals-by-namespace",
                    protocol_globals_by_namespace,
                ),
                (
                    "unqualified-protocol-bindings",
                    unqualified_protocol_bindings,
                ),
                ("native-interfaces", native_interfaces),
            ])?;
            let key = self.keyword("suss/compiler-catalog")?;
            let mut entries = match self.recipes[root].as_ref().expect("environment map recipe") {
                Recipe::Map(entries) => entries.clone(),
                _ => unreachable!("environment recipe is a map"),
            };
            entries.push((key, compiler));
            root = self.map_values(entries)?;
        }
        self.materialize(root)
    }
    fn environment(
        &mut self,
        namespace: &Arc<SourceNamespace>,
        catalog: &Arc<SourceNamespace>,
        locals: &hir::SourceLocals,
        fields: &hir::SourceFields,
        scopes: &[Arc<FunctionScope>],
        context: portable::AnalysisContext,
        depth: usize,
    ) -> Result<Value> {
        self.environment_record(namespace, catalog, locals, fields, scopes, context, depth)
    }
    fn environment_record(
        &mut self,
        namespace: &Arc<SourceNamespace>,
        catalog: &Arc<SourceNamespace>,
        locals: &hir::SourceLocals,
        fields: &hir::SourceFields,
        scopes: &[Arc<FunctionScope>],
        context: portable::AnalysisContext,
        depth: usize,
    ) -> Result<Value> {
        Self::depth(depth)?;
        let context = self.context(context)?;
        let namespace = self.namespace(namespace, depth + 1)?;
        let catalog = self.namespace(catalog, depth + 1)?;
        let mut entries = BTreeMap::new();
        for (name, field) in fields.iter().collect::<BTreeMap<_, _>>() {
            entries.insert(name.clone(), self.field(field, depth + 1)?);
        }
        for (name, local) in locals.iter().collect::<BTreeMap<_, _>>() {
            entries.insert(name.clone(), self.local(local, depth + 1)?);
        }
        let entries = entries
            .into_iter()
            .map(|(name, value)| Ok((self.symbol(&name)?, value)))
            .collect::<Result<Vec<_>>>()?;
        let locals_recipe = self.map_values(entries)?;
        let locals = self.source_boundary(source_roots::Owner::Locals(locals.clone(), fields.clone()), locals_recipe)?;
        let scopes = scopes
            .iter()
            .map(|scope| self.scope(scope, depth + 1))
            .collect::<Result<Vec<_>>>()?;
        let scopes = self.vector(&scopes)?;
        field_map!(self; [
            (FieldKey::Literal(AnalyzerLiteralKeyword("context")), context),
            (FieldKey::Literal(AnalyzerLiteralKeyword("ns")), namespace),
            (FieldKey::Cold("suss/catalog"), catalog),
            (FieldKey::Literal(AnalyzerLiteralKeyword("locals")), locals),
            (FieldKey::Literal(AnalyzerLiteralKeyword("fn-scope")), scopes),
        ])
    }
    fn namespace_membership(&mut self, namespace: &SourceNamespace) -> Result<Value> {
        let name = self.symbol(&namespace.namespace)?;
        let mut aliases = Vec::new();
        for (alias, target) in &namespace.aliases {
            aliases.push((self.symbol(alias)?, self.symbol(target)?));
        }
        let aliases = self.nullable_map(aliases)?;
        let mut refers = Vec::new();
        for (local, global) in &namespace.refers {
            refers.push((
                self.symbol(local)?,
                self.symbol(&format!("{}/{}", global.namespace(), global.name()))?,
            ));
        }
        let refers = self.map_values(refers)?;
        let exclusions = namespace
            .excluded_core
            .iter()
            .map(|name| self.symbol(name))
            .collect::<Result<Vec<_>>>()?;
        let exclusions = self.recipe(Recipe::Set(exclusions))?;
        let names = namespace
            .identities
            .iter()
            .map(|global| self.symbol(global.name()))
            .collect::<Result<Vec<_>>>()?;
        let names = self.recipe(Recipe::Set(names))?;
        self.map(vec![
            ("name", name),
            ("requires", aliases),
            ("suss/refers", refers),
            ("excludes", exclusions),
            ("suss/binding-names", names),
        ])
    }
    fn namespace(&mut self, namespace: &Arc<SourceNamespace>, depth: usize) -> Result<Value> {
        Self::depth(depth)?;
        let fact_count = namespace.aliases.len()
            + namespace.refers.len()
            + namespace.used_refers.len()
            + namespace.renamed_refers.len()
            + namespace.excluded_core.len()
            + namespace.macro_aliases.len()
            + namespace.macro_refers.len()
            + namespace.used_macro_refers.len()
            + namespace.renamed_macro_refers.len()
            + namespace.identities.len()
            + namespace.declarations.len();
        let key = Arc::as_ptr(namespace) as usize;
        self.charge(1)?;
        if let Some((_, value)) = self.namespaces.get(&key) {
            return Ok(value.clone());
        }
        self.charge(fact_count.saturating_add(namespace.namespace.len()).saturating_add(1))?;
        let content_key = NamespaceKey::new(namespace);
        self.charge(fact_count.saturating_add(1))?;
        if !self.capture_roots && let Some(value) = self.namespace_contents.get(&content_key) {
            let value = *value;
            // The content key owns a snapshot, keeping declaration Arc addresses
            // live for this graph's entire lifetime without cloning its maps.
            self.namespaces.insert(key, (namespace.clone(), value));
            return Ok(value);
        }
        let native = self.task(Task::Namespace(namespace.clone()))?;
        let value = self.source_boundary(source_roots::Owner::Namespace(namespace.clone()), native)?;
        self.namespaces.insert(key, (namespace.clone(), value));
        if !self.capture_roots { self.namespace_contents.insert(content_key, value); }
        Ok(value)
    }
    // The pinned namespace contract retains nil for unused ordinary requires,
    // ordinary uses and macro requires; rename and macro-use maps stay maps.
    fn nullable_map(&mut self, entries: Vec<(Value, Value)>) -> Result<Value> {
        if entries.is_empty() {
            self.scalar(Literal::Nil)
        } else {
            self.map_values(entries)
        }
    }
    fn namespace_record(
        &mut self,
        slot: Value,
        namespace: &Arc<SourceNamespace>,
        depth: usize,
    ) -> Result<Value> {
        let name = self.symbol(&namespace.namespace)?;
        let mut aliases = Vec::new();
        for (alias, target) in &namespace.aliases {
            aliases.push((self.symbol(alias)?, self.symbol(target)?));
        }
        let aliases = self.nullable_map(aliases)?;
        let mut refers = Vec::new();
        for (alias, target) in &namespace.refers {
            refers.push((
                self.symbol(alias)?,
                self.symbol(&format!("{}/{}", target.namespace(), target.name()))?,
            ));
        }
        let refers = self.map_values(refers)?;
        let mut uses = Vec::new();
        let mut renames = Vec::new();
        for (local, target) in &namespace.refers {
            if namespace.used_refers.contains(local) {
                uses.push((self.symbol(local)?, self.symbol(target.namespace())?));
            }
            if namespace.renamed_refers.contains(local) {
                renames.push((
                    self.symbol(local)?,
                    self.symbol(&format!("{}/{}", target.namespace(), target.name()))?,
                ));
            }
        }
        let uses = self.nullable_map(uses)?;
        let renames = self.map_values(renames)?;
        let mut macro_aliases = Vec::new();
        for (alias, target) in &namespace.macro_aliases {
            macro_aliases.push((self.symbol(alias)?, self.symbol(target)?));
        }
        let macro_aliases = self.nullable_map(macro_aliases)?;
        let mut macro_uses = Vec::new();
        let mut macro_renames = Vec::new();
        for (local, (ns, original)) in &namespace.macro_refers {
            if namespace.used_macro_refers.contains(local) {
                macro_uses.push((self.symbol(local)?, self.symbol(ns)?));
            }
            if namespace.renamed_macro_refers.contains(local) {
                macro_renames.push((
                    self.symbol(local)?,
                    self.symbol(&format!("{ns}/{original}"))?,
                ));
            }
        }
        let macro_uses = self.map_values(macro_uses)?;
        let macro_renames = self.map_values(macro_renames)?;
        let exclusions = namespace
            .excluded_core
            .iter()
            .map(|name| self.symbol(name))
            .collect::<Result<Vec<_>>>()?;
        self.charge(0)?;
        let exclusions = self.recipe(Recipe::Set(exclusions))?;
        let mut declarations = Vec::new();
        for global in &namespace.identities {
            let short = self.symbol(global.name())?;
            let record = self.declaration(global, namespace.declarations.get(global), depth + 1)?;
            declarations.push((short, record));
        }
        let declarations = self.map_values(declarations)?;
        let declarations = self.source_boundary(source_roots::Owner::Defs(namespace.clone()), declarations)?;
        // Literal creation sites, pinned analyzer.cljc c4295f3:
        // ns-info 3504-3517 / require-info 3588-3600 are merged into
        // compiler namespace state. :defs is created by ensure-defs 5091
        // (also parse :def assoc-in 2093), not inferred from lookup spelling.
        // This proof applies only to this namespace-record constructor.
        field_map_in_task_slot!(self, slot; [
                (FieldKey::Literal(AnalyzerLiteralKeyword("name")), name),
                (FieldKey::Literal(AnalyzerLiteralKeyword("requires")), aliases),
                (FieldKey::Cold("suss/refers"), refers),
                (FieldKey::Literal(AnalyzerLiteralKeyword("uses")), uses),
                (FieldKey::Literal(AnalyzerLiteralKeyword("renames")), renames),
                (FieldKey::Literal(AnalyzerLiteralKeyword("require-macros")), macro_aliases),
                (FieldKey::Literal(AnalyzerLiteralKeyword("use-macros")), macro_uses),
                (FieldKey::Literal(AnalyzerLiteralKeyword("rename-macros")), macro_renames),
                (FieldKey::Literal(AnalyzerLiteralKeyword("excludes")), exclusions),
                (FieldKey::Literal(AnalyzerLiteralKeyword("defs")), declarations),
            ])
    }
    // Copyright (c) Rich Hickey. All rights reserved.
    // The use and distribution terms for this software are covered by the
    // Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
    // which can be found in the file epl-v10.html at the root of this distribution.
    // By using this software in any fashion, you are agreeing to be bound by
    // the terms of this license. You must not remove this notice.
    // SPDX-License-Identifier: EPL-1.0
    // Declaration field policy adapts cljs/analyzer.cljc parse :def (2022–2199)
    // and source-info (745–756), pinned c4295f303100bbf5afac449242d30bca1126f1a1.
    // Source SHA-256: 297802c627474434f1ef868e31f5f9913c290a4e80c509a40c704dced95bbf47.
    // Native graph recipes, immutable record identities and provenance checks
    // are original; no JVM AST or runtime namespace object is fabricated.
    fn declaration(
        &mut self,
        global: &portable::resolve::Global,
        info: Option<&Arc<portable::resolve::DefinitionInfo>>,
        depth: usize,
    ) -> Result<Value> {
        Self::depth(depth)?;
        // Own each revision so addresses cannot be recycled within this graph.
        // Global identity also distinguishes plain catalog entries without facts.
        let key = (
            global.clone(),
            info.map_or(0, |info| Arc::as_ptr(info) as usize),
        );
        self.charge(global.namespace().len().saturating_add(global.name().len()).saturating_add(1))?;
        if let Some((_, value)) = self.declarations.get(&key) {
            return Ok(*value);
        }
        let tracing = std::env::var_os("SUSS_METADATA_FUEL_TRACE").is_some();
        let trace = |session: &Session, stage: &str| -> Result<()> {
            if tracing {
                eprintln!("metadata-declaration {}/{} {stage}: {}", global.namespace(), global.name(), session.operation_fuel_remaining()?);
            }
            Ok(())
        };
        trace(self.session, "start")?;
        let name = self.symbol(&format!("{}/{}", global.namespace(), global.name()))?;
        self.charge(24)?; // native definition/shape/role reads and sidecar branch
        let prefixes = info.map_or(0, |info| info.declaration.metadata.len());
        self.charge(prefixes.saturating_mul(3))?; // all prefix-kind reads/checks before scanning
        let ordinary_def = info.is_some_and(|info| info.type_fields.is_none()
            // Reader shorthand eligibility uses the actual normalized entry
            // origin below, never a synthesized Form or tag spelling alone.
            && info.declaration.metadata.iter().all(|prefix|
                matches!(&prefix.kind, Kind::Map(_) | Kind::Keyword(_) | Kind::Symbol(_) | Kind::String(_)))
            && matches!(&info.definition_form.kind, Kind::List(items)
                if matches!(items.first().map(|head| &head.kind), Some(Kind::Symbol(head))
                    if head.namespace.is_none() && matches!(head.name.as_str(), "def" | "defonce"))
                && matches!(items.get(1).map(|name| &name.kind), Some(Kind::Symbol(_)))));
        let mut retained = if ordinary_def {
            let mut keys = RetainedFieldKeys::new(self)?;
            keys.remember_named(self, AnalyzerLiteralKeyword("name"))?;
            Some(keys)
        } else if info.is_some_and(|info| info.type_fields.is_some()) {
            Some(RetainedFieldKeys::type_history(self)?)
        } else { None };
        let mut fields = vec![("name", name)];
        if let Some(info) = info {
            // Preserve the transport guard before requesting reader enrichment.
            // An oversized metadata value must not reach source snapshots or
            // guest construction merely because only selected fields are read.
            validate_form_data(&info.definition_form, &mut |units| {
                self.session.charge_host_work(units)
            })?;
            validate_form_data(&info.declaration, &mut |units| {
                self.session.charge_host_work(units)
            })?;
            trace(self.session, "guarded")?;
            self.charge(3)?;
            let (mut declaration, mut owned_reader_prefixes) = if let Some(origin) = &info.origin {
                origin
                    .macro_form_data_with_metadata_origins_metered(&info.declaration, &mut |units| {
                        charge_origin_work(self.session, info.declaration.span.clone(), units)
                    })
                    .map_err(SessionError::Compile)?
            } else {
                (self.clone_form(&info.declaration)?, Some(info.declaration.metadata.len()))
            };
            trace(self.session, "declaration-origin")?;
            // declare adds semantic metadata to an original source symbol. Only
            // recover positions through the verified original declaration form,
            // never by trusting a generated symbol's reused byte span alone.
            if let (Some(origin), Kind::List(items)) = (&info.origin, &info.definition_form.kind) {
                if items.first().is_some_and(|head| {
                    matches!(&head.kind, Kind::Symbol(symbol)
                    if symbol.namespace.is_none() && symbol.name == "declare")
                }) {
                    if let Some(original) = items[1..].iter().find(|name| {
                        name.kind == info.declaration.kind && name.span == info.declaration.span
                    }) {
                        let mut original = origin
                            .macro_form_data_metered(original, &mut |units| {
                                charge_origin_work(self.session, original.span.clone(), units)
                            })
                            .map_err(SessionError::Compile)?;
                        self.charge(1)?;
                        owned_reader_prefixes = None;
                        declaration.metadata = std::mem::take(&mut original.metadata);
                        for metadata in &info.declaration.metadata {
                            declaration.metadata.push(self.clone_form(metadata)?);
                        }
                    }
                }
            }
            let reader_metadata =
                crate::portable_macro_data::normalized_reader_metadata_view(self.session, &declaration)?;
            let metadata = &reader_metadata.pairs;
            let mut metadata_keys = self.reader_key_slots(reader_metadata.origins.len())?;
            let property = |name: &str| {
                metadata.chunks_exact(2).find_map(|pair| {
                    matches!(&pair[0].kind, Kind::Keyword(key)
                    if key.namespace.is_none() && key.name == name)
                    .then_some(&pair[1])
                })
            };
            // Source KEY facts come from the original owned declaration, not
            // synthetic line/file entries appended by source-origin enrichment.
            // The original full guards/enrichment/normalization have run first.
            self.charge(2)?;
            if ordinary_def && let Some(keys) = retained.as_mut() {
                // Consume ONE owned normalized view. The source clone recorded
                // which prefix slots belong to the actual selected reader
                // clone input. Normalization retains the KEY's (prefix,pair)
                // selector even when a later prefix replaced its VALUE.
                self.charge(3)?;
                if let Some(original_count) = owned_reader_prefixes {
                    for label in ["name", "ns", "private", "dynamic", "doc", "declared", "tag", "ret-tag",
                        "fn-var", "variadic?", "max-fixed-arity", "method-params", "arglists", "arglists-meta",
                        "export", "type", "num-fields", "record", "meta"] {
                        self.charge(2)?;
                        for (index, pair) in metadata.chunks_exact(2).enumerate() {
                            self.charge(label.len().saturating_add(8))?;
                            if reader_metadata.key_positions[index].0 < original_count
                                && matches!(&pair[0].kind, Kind::Keyword(key)
                                    if key.namespace.is_none() && key.name == label) {
                                let recipe = self.reader_key_recipe(&pair[0], reader_metadata.origins[index])?;
                                metadata_keys[index] = Some(recipe);
                                keys.remember_reader_occurrence(self, label, &pair[0], recipe)?;
                                break;
                            }
                        }
                    }
                    // Root position enrichment is not assessed by the tag
                    // reference. Preserve its ORIGINAL raw-key registration
                    // policy without joining it to an enriched occurrence.
                    let position_keys = preserve_source_error(info.declaration.span.clone(),
                        &mut |units| self.session.charge_host_work(units),
                        |meter| hir::reader_metadata_view_metered(&info.declaration, meter))?;
                    for label in ["line", "column", "file"] {
                        for entry in &position_keys {
                            self.charge(label.len().saturating_add(6))?;
                            if matches!(&entry.key.as_ref().kind, Kind::Keyword(key)
                                if key.namespace.is_none() && key.name == label) {
                                keys.remember_source(self, label, entry.key.as_ref())?; break;
                            }
                        }
                    }
                } else {
                    // Only non-Symbol/unsupported native root shapes can lack
                    // a Symbol prefix-owner proof. All selected Symbol readers,
                    // including eager/paired mappings, take the owned path.
                    // Do not join equal keys from separate views here.
                    let raw = preserve_source_error(info.declaration.span.clone(),
                        &mut |units| self.session.charge_host_work(units),
                        |meter| hir::reader_metadata_pairs_metered(&info.declaration, meter))?;
                    for label in ["name", "ns", "private", "dynamic", "doc", "declared", "tag", "ret-tag",
                        "fn-var", "variadic?", "max-fixed-arity", "method-params", "arglists", "arglists-meta",
                        "export", "type", "num-fields", "record", "meta", "line", "column", "file"] {
                        for pair in raw.chunks_exact(2) {
                            self.charge(label.len().saturating_add(4))?;
                            if matches!(&pair[0].kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == label) {
                                keys.remember_source(self, label, &pair[0])?; break;
                            }
                        }
                    }
                }
            }
            self.charge(2)?;
            if info.type_fields.is_some() && let Some(keys) = retained.as_mut() {
                self.seed_type_key_history(keys, global, info)?;
            }
            let truthy = |form: &Form| !matches!(form.kind, Kind::Nil | Kind::Bool(false));
            if let Some(count) = info.type_fields {
                fields.extend([
                    ("type", self.flag(true)?),
                    ("num-fields", self.number(count)?),
                    ("record", self.flag(info.type_is_record)?),
                ]);
            }
            // Namespace declaration records do not have the :ns field of a
            // resolved var AST. Preserve observed raw metadata in either stage.
            for field in [
                "name",
                "ns",
                "private",
                "dynamic",
                "doc",
                "declared",
                "tag",
                "ret-tag",
                "fn-var",
                "variadic?",
                "max-fixed-arity",
                "method-params",
                "arglists",
                "arglists-meta",
                "export",
                "type",
                "num-fields",
                "record",
            ] {
                if let Some(value) = property(field) {
                    fields.retain(|(name, _)| *name != field);
                    fields.push((field, self.form(value, depth + 1)?));
                }
            }
            let provisional = !info.analysis_completed
                || (info.initializer_form.is_some() && property("declared").is_some_and(truthy));
            if !provisional {
                let meta = self.declaration_reader_metadata_map(
                    &reader_metadata, &mut metadata_keys, global.namespace() == "suss.core", depth + 1)?;
                retained_decl_push!(self, retained, fields; "meta", AnalyzerLiteralKeyword("meta"), meta);
            } else if let Some(value) = property("meta") {
                fields.push(("meta", self.form(value, depth + 1)?));
            }
            trace(self.session, "metadata")?;
            if let Some(origin) = &info.origin {
                let (span, metadata) = origin
                    .macro_form_root_metadata_metered(&info.definition_form, &mut |units| {
                        charge_origin_work(self.session, info.definition_form.span.clone(), units)
                    })
                    .map_err(SessionError::Compile)?;
                // Normalization reads metadata only. This local carrier is not
                // quoted or exposed as a replacement definition form.
                let definition = Form { span, metadata, kind: Kind::Nil };
                let position =
                    crate::portable_macro_data::normalized_reader_metadata(self.session, &definition)?;
                for field in ["line", "column", "file"] {
                    if let Some(value) = position.chunks_exact(2).find_map(|pair| {
                        matches!(&pair[0].kind, Kind::Keyword(key)
                            if key.namespace.is_none() && key.name == field)
                        .then_some(&pair[1])
                    }) {
                    let value = self.form(value, depth + 1)?;
                        self.charge(2)?;
                        if let Some(keys) = retained.as_mut() {
                            let token = match field {
                                "line" => AnalyzerLiteralKeyword("line"),
                                "column" => AnalyzerLiteralKeyword("column"),
                                "file" => AnalyzerLiteralKeyword("file"),
                                _ => unreachable!("fixed source-info positions"),
                            };
                            keys.remember_named(self, token)?;
                        }
                        fields.push((field, value));
                    }
                }
            }
            trace(self.session, "definition-origin")?;
            if global.namespace() == "suss.core" {
                fields.retain(|(name, _)| *name != "file");
                retained_decl_push!(self, retained, fields; "file", AnalyzerLiteralKeyword("file"),
                    self.scalar(Literal::String("cljs/core.cljs".encode_utf16().collect()))?);
            }
            let source = info
                .initializer
                .as_ref()
                .and_then(|init| init.source.as_ref());
            let callable = source.and_then(|source| source.callable.as_ref());
            // parse-def chooses a callable return independently of dynamic
            // var-reference inference. False hints fall through just like nil.
            let completed_tag = if info.type_fields.is_some() {
                hir::declaration_tag(info).map_err(SessionError::Compile)?
            } else if provisional {
                None
            } else if callable.is_some() {
                property("tag")
                    .filter(|tag| truthy(tag))
                    .cloned()
                    .or_else(|| {
                        source
                            .and_then(|source| source.tags.inferred_return.as_ref())
                            .and_then(|tag| tag.clone())
                    })
            } else if property("tag").is_some_and(|tag| matches!(tag.kind, Kind::Bool(false))) {
                if property("dynamic").is_some_and(truthy) {
                    Some(Form {
                        span: 0..0,
                        metadata: vec![],
                        kind: Kind::Symbol(suss_reader::Symbol::new("any")),
                    })
                } else {
                    source.and_then(|source| source.tags.inferred_ref().cloned())
                }
            } else {
                hir::declaration_tag(info)
                    .map_err(|error| SessionError::Host(wasmtime::Error::msg(error.message)))?
            };
            let (definition, initializer_recipe) = self.definition_with_initializer(
                &info.definition_form, info.initializer_form.as_ref(), depth + 1,
            )?;
            if tracing && info.initializer_form.is_some() {
                eprintln!("metadata-initializer {}/{} reused={}", global.namespace(), global.name(), initializer_recipe.is_some());
            }
            fields.push(("suss/definition-form", definition));
            fields.push((
                "suss/analysis-completed",
                self.flag(info.analysis_completed)?,
            ));
            fields.push(("suss/declaration", self.form(&info.declaration, depth + 1)?));
            fields.push(("suss/defonce", self.flag(info.once)?));
            if let Some(form) = &info.initializer_form {
                let value = match initializer_recipe {
                    Some(value) => value,
                    None => self.form(form, depth + 1)?,
                };
                fields.push(("suss/initializer-form", value));
            }
            fields.push((
                "suss/initializer-recorded",
                self.flag(info.initializer.is_some())?,
            ));
            if !provisional && let Some(doc) = &info.docstring {
                fields.retain(|(name, _)| *name != "doc");
                retained_decl_push!(self, retained, fields; "doc", AnalyzerLiteralKeyword("doc"), self.scalar(Literal::String(doc.clone()))?);
            }
            if let Some(callable) = callable {
                fields.push(("suss/source-function", self.callable(callable)?));
                const FUNCTION_FIELDS: [&str; 6] = [
                    "fn-var",
                    "variadic?",
                    "max-fixed-arity",
                    "method-params",
                    "arglists",
                    "arglists-meta",
                ];
                if property("declared").is_some_and(truthy) {
                    // The pinned analyzer leaves this initializer's provisional
                    // symbol metadata in the catalog. Completed source callable
                    // facts above remain a separate Suss extension.
                    // The raw selected fields were already copied above, even
                    // without a source callable in a provisional initializer.
                } else if let Some(top) =
                    property("top-fn").filter(|form| !matches!(form.kind, Kind::Nil))
                {
                    // if-some distinguishes nil from an empty map. The map
                    // replaces the computed arity/arglist group and can also
                    // override fn-var; retain selected field presence and data.
                    let pairs: Vec<(&Form, &Form)> = match &top.kind {
                        Kind::Map(items) => items
                            .chunks_exact(2)
                            .map(|pair| (&pair[0], &pair[1]))
                            .collect(),
                        // Pinned merge conjoins a direct vector as one entry.
                        Kind::Vector(items) if items.len() == 2 => {
                            vec![(&items[0], &items[1])]
                        }
                        // Empty seqable values conjoin no entries. Nonempty
                        // reader sequences cannot contain JVM Map$Entry values.
                        Kind::List(items) | Kind::Set(items) if items.is_empty() => vec![],
                        Kind::String(units) if units.is_empty() => vec![],
                        _ => {
                            return Err(SessionError::Host(wasmtime::Error::msg(
                                "Function declaration top-fn requires mergeable reader data",
                            )));
                        }
                    };
                    let macro_flag = property("macro").is_some_and(truthy);
                    fields.retain(|(name, _)| *name != "fn-var");
                    retained_decl_push!(self, retained, fields; "fn-var", AnalyzerLiteralKeyword("fn-var"), self.flag(!macro_flag)?);
                    for field in [
                        "name",
                        "ns",
                        "tag",
                        "ret-tag",
                        "fn-var",
                        "variadic?",
                        "max-fixed-arity",
                        "method-params",
                        "arglists",
                        "arglists-meta",
                        "private",
                        "dynamic",
                        "doc",
                        "declared",
                        "line",
                        "column",
                        "file",
                        "meta",
                    ] {
                        if let Some(form) = pairs.iter().rev().find_map(|(key, value)| {
                            matches!(&key.kind, Kind::Keyword(key)
                                if key.namespace.is_none() && key.name == field)
                            .then_some(*value)
                        }) {
                            fields.retain(|(name, _)| *name != field);
                            let value = self.form(form, depth + 1)?;
                            self.charge(2)?;
                            if let Some(keys) = retained.as_mut() {
                                // `pairs` is verified native top-fn reader data.
                                // First equal KEY survives; the value above is
                                // still selected by the original reverse scan.
                                let mut source_key = None;
                                for (key, _) in &pairs {
                                    self.charge(field.len().saturating_add(4))?;
                                    if matches!(&key.kind, Kind::Keyword(key)
                                        if key.namespace.is_none() && key.name == field) {
                                        source_key = Some(*key); break;
                                    }
                                }
                                if let Some(key) = source_key { keys.remember_source(self, field, key)?; }
                            }
                            fields.push((field, value));
                        }
                    }
                } else {
                    // Normal callable defaults replace the selected raw group.
                    // Empty top-fn instead leaves its five raw arity fields.
                    fields.retain(|(name, _)| !FUNCTION_FIELDS.contains(name));
                    let macro_flag = property("macro")
                        .is_some_and(|value| !matches!(value.kind, Kind::Nil | Kind::Bool(false)));
                    retained_decl_push!(self, retained, fields; "fn-var", AnalyzerLiteralKeyword("fn-var"), self.flag(!macro_flag)?);
                    retained_decl_push!(self, retained, fields; "variadic?", AnalyzerLiteralKeyword("variadic?"), self.flag(callable.variadic())?);
                    let fixed = callable.max_fixed_arity().ok_or_else(|| {
                        SessionError::Host(wasmtime::Error::msg(
                            "Invalid source function declaration",
                        ))
                    })?;
                    retained_decl_push!(self, retained, fields; "max-fixed-arity", AnalyzerLiteralKeyword("max-fixed-arity"), self.number(fixed)?);
                    // Preserve every analyzed source signature, including duplicate
                    // arities. Emitted dispatch methods are a separate contract.
                    let mut methods = Vec::new();
                    for method in &callable.methods {
                        let parameters = method
                            .parameters
                            .iter()
                            .map(|parameter| self.symbol(&parameter.name))
                            .collect::<Result<Vec<_>>>()?;
                        methods.push(self.vector(&parameters)?);
                    }
                    self.charge(0)?;
                    retained_decl_push!(self, retained, fields; "method-params", AnalyzerLiteralKeyword("method-params"), self.recipe(Recipe::List(methods))?);
                    // The pinned analyzer retains the reader value of :arglists,
                    // including its quote, and maps meta over that value's elements.
                    let arglists = property("arglists");
                    let argument_data = if let Some(form) = arglists {
                        self.form(form, depth + 1)?
                    } else {
                        self.scalar(Literal::Nil)?
                    };
                    retained_decl_push!(self, retained, fields; "arglists", AnalyzerLiteralKeyword("arglists"), argument_data);
                    let mut argument_metadata = Vec::new();
                    if let Some(form) = arglists {
                        let items = match &form.kind {
                            Kind::List(items) | Kind::Vector(items) | Kind::Set(items) => {
                                items.as_slice()
                            }
                            // Map sequence entries and UTF-16 character values have
                            // no metadata, unlike original reader collection items.
                            Kind::Map(items) => {
                                for _ in 0..items.len() / 2 {
                                    argument_metadata.push(self.scalar(Literal::Nil)?);
                                }
                                &[]
                            }
                            Kind::String(units) => {
                                for _ in units {
                                    argument_metadata.push(self.scalar(Literal::Nil)?);
                                }
                                &[]
                            }
                            Kind::Nil => &[],
                            _ => {
                                return Err(SessionError::Host(wasmtime::Error::msg(
                                    "Function declaration arglists requires seqable reader data",
                                )));
                            }
                        };
                        for item in items {
                            let pairs =
                                crate::portable_macro_data::normalized_reader_metadata(self.session, item)?;
                            let value = if pairs.is_empty() {
                                self.scalar(Literal::Nil)?
                            } else {
                                self.form(
                                    &Form {
                                        span: item.span.clone(),
                                        metadata: vec![],
                                        kind: Kind::Map(pairs),
                                    },
                                    depth + 1,
                                )?
                            };
                            argument_metadata.push(value);
                        }
                    }
                    self.charge(0)?;
                    retained_decl_push!(self, retained, fields; "arglists-meta", AnalyzerLiteralKeyword("arglists-meta"),
                        self.recipe(Recipe::List(argument_metadata))?);
                }
            }
            if !provisional
                && property("declared").is_some_and(truthy)
                && let Some(arglists) = property("arglists").filter(|value| truthy(value))
            {
                // Fresh forward declaration metadata supplies method data, not
                // an analyzed initializer or an executable source callable.
                // Match parse-def's second over the retained reader value.
                let nil = || Form {
                    span: 0..0,
                    metadata: vec![],
                    kind: Kind::Nil,
                };
                let parameters = match &arglists.kind {
                    Kind::List(items) | Kind::Vector(items) | Kind::Set(items) => {
                        items.get(1).cloned().unwrap_or_else(nil)
                    }
                    Kind::Map(items) => {
                        items.chunks_exact(2).nth(1).map_or_else(nil, |pair| Form {
                            span: 0..0,
                            metadata: vec![],
                            kind: Kind::Vector(pair.to_vec()),
                        })
                    }
                    Kind::String(units) => units.get(1).map_or_else(nil, |unit| Form {
                        span: 0..0,
                        metadata: vec![],
                        kind: Kind::String(vec![*unit]),
                    }),
                    _ => {
                        return Err(SessionError::Host(wasmtime::Error::msg(
                            "Declared argument lists require seqable reader data",
                        )));
                    }
                };
                fields
                    .retain(|(name, _)| !matches!(*name, "declared" | "fn-var" | "method-params"));
                retained_decl_push!(self, retained, fields; "declared", AnalyzerLiteralKeyword("declared"), self.flag(true)?);
                retained_decl_push!(self, retained, fields; "fn-var", AnalyzerLiteralKeyword("fn-var"), self.flag(true)?);
                retained_decl_push!(self, retained, fields; "method-params", AnalyzerLiteralKeyword("method-params"), self.form(&parameters, depth + 1)?);
            }
            if let Some(tag) = completed_tag {
                let field = if callable.is_some() { "ret-tag" } else { "tag" };
                fields.retain(|(name, _)| *name != field);
                let value = self.form(&tag, depth + 1)?;
                self.charge(2)?;
                if let Some(keys) = retained.as_mut() {
                    let token = if callable.is_some() { AnalyzerLiteralKeyword("ret-tag") }
                        else { AnalyzerLiteralKeyword("tag") };
                    keys.remember_named(self, token)?;
                }
                fields.push((field, value));
            }
        }
        // Share the final metadata value of this captured declaration revision,
        // including a top-fn overlay. Reader occurrences remain independently
        // constructed elsewhere; resolved var copies reuse this catalog value only
        // within this build. Cross-build roots require the checked reuse path.
        let mut metadata_recipe = None;
        if let Some((_, meta)) = fields.iter_mut().find(|(name, _)| *name == "meta") {
            self.charge(0)?;
            *meta = self.recipe(Recipe::Alias(*meta))?;
            metadata_recipe = Some(*meta);
        }
        if self.capture_roots && let Some(revision) = info {
            for (slot, (_, value)) in fields.iter_mut().enumerate() {
                self.session.charge_host_work((global.namespace().len()+global.name().len()) as u64+2)?;
                *value = self.source_boundary(source_roots::Owner::DeclarationField(
                    global.clone(), revision.clone(), slot), *value)?;
            }
        }
        self.charge(2)?;
        let native = if let Some(keys) = retained { keys.finish(self, fields)? }
            else { self.map(fields)? };
        let value = if let Some(revision) = info {
            self.source_boundary(source_roots::Owner::Declaration(global.clone(), revision.clone()), native)?
        } else { native };
        if let (Some(recipe), Some(revision)) = (metadata_recipe, info) {
            // Capture only after the final declaration and overlaid metadata
            // recipe exist. No guest storage is consulted or retained.
            metadata_expectations::capture(self.session, &mut self.metadata_sources,
                global, revision, recipe)?;
        }
        trace(self.session, "finished")?;
        self.declarations.insert(key, (info.cloned(), value));
        Ok(value)
    }
    fn local(&mut self, binding: &Arc<LocalBinding>, depth: usize) -> Result<Value> {
        Self::depth(depth)?;
        // Same declaration can have distinct receiver/argument roles or lowered
        // IDs. Keep that adaptation in the key; retain the identity owner.
        let role = match &binding.source_role {
            SourceRole::Plain => "plain".into(),
            SourceRole::FunctionName { variadic, methods } => format!(
                "function:{variadic}:{}",
                methods
                    .as_ref()
                    .map_or(0, |methods| Arc::as_ptr(methods) as usize)
            ),
            SourceRole::PrivateCatch { .. } => "private-catch".into(),
            SourceRole::CatchBinding { .. } => "catch-alias".into(),
            SourceRole::MethodArgument { index, rest } => format!("method-arg:{index}:{rest}"),
            SourceRole::MethodThis { .. } => "method-this".into(),
        };
        let key = (Arc::as_ptr(binding) as usize, binding.id.0, role);
        self.charge(key.2.len().saturating_add(1))?;
        if let Some((_, value)) = self.locals.get(&key) {
            return Ok(value.clone());
        }
        let native = self.task(Task::Local(binding.clone()))?;
        let value = self.source_boundary(source_roots::Owner::Local(binding.clone()), native)?;
        self.locals.insert(key, (binding.clone(), value));
        Ok(value)
    }
    // c4295f3 analyzer.cljc2220-2235 (params),2500-2530 (let/loop).
    // Adapted CatchBinding/MethodThis report source_kind Let but do not establish
    // these constructors. Inspect the actual immutable role/kind pair instead.
    fn binding_literal_key(&mut self, binding: &LocalBinding, token: AnalyzerLiteralKeyword) -> Result<FieldKey<'static>> {
        self.charge(6)?; // role/kind reads, bounded matches, payload and selection
        Ok(match (&binding.source_role, binding.kind) {
            (SourceRole::Plain, hir::LocalKind::Let | hir::LocalKind::Loop | hir::LocalKind::Argument { .. })
            | (SourceRole::MethodArgument { .. }, _) => FieldKey::Literal(token),
            _ => FieldKey::Cold(token.name()),
        })
    }
    // Independent fn-name-var2285–2297 constructor proof. Do not broaden
    // parameter/let binding_literal_key or turn adapted source_kind into proof.
    fn name_var_literal_key(&mut self, binding: &LocalBinding, token: AnalyzerLiteralKeyword) -> Result<FieldKey<'static>> {
        self.charge(6)?;
        if matches!((&binding.source_role, binding.kind),
            (SourceRole::FunctionName { .. }, hir::LocalKind::FunctionName)) {
            Ok(FieldKey::Literal(token))
        } else { self.binding_literal_key(binding, token) }
    }
    fn local_record(&mut self, slot: Value, binding: &LocalBinding, depth: usize) -> Result<Value> {
        let name = self.form(&binding.declaration, depth + 1)?;
        let kind = self.keyword(match binding.source_kind() {
            hir::LocalKind::Let => "let",
            hir::LocalKind::Loop => "loop",
            hir::LocalKind::Argument { .. } => "arg",
            hir::LocalKind::FunctionName => "fn",
            hir::LocalKind::Catch => "catch",
        })?;
        let id = self.number(binding.id.0)?;
        let mut entries = field_entries!(self; [
            (FieldKey::Literal(AnalyzerLiteralKeyword("name")), name.clone()),
            (self.name_var_literal_key(binding, AnalyzerLiteralKeyword("form"))?, name),
            (self.name_var_literal_key(binding, AnalyzerLiteralKeyword("local"))?, kind),
            (FieldKey::Cold("suss/id"), id),
        ]);
        if let Some(tag) = hir::local_tag(binding)
            .map_err(|error| SessionError::Host(wasmtime::Error::msg(error.message)))?
        {
            field_push!(self, entries; (FieldKey::Literal(AnalyzerLiteralKeyword("tag")), self.form(&tag, depth + 1)?));
        } else if binding.initializer.is_some()
            || matches!(binding.source_kind(), hir::LocalKind::Argument { .. })
        {
            field_push!(self, entries; (FieldKey::Literal(AnalyzerLiteralKeyword("tag")), self.scalar(Literal::Nil)?));
        }
        if !matches!(binding.source_role, SourceRole::PrivateCatch { .. }) {
            field_push!(self, entries; (FieldKey::Literal(AnalyzerLiteralKeyword("op")), self.keyword("binding")?));
            field_push!(self, entries; (self.binding_literal_key(binding, AnalyzerLiteralKeyword("binding-form?"))?, self.flag(true)?));
        }
        let position = binding
            .origin
            .as_ref()
            .map(|origin| symbol_position(self.session, origin, &binding.declaration))
            .transpose()?
            .flatten();
        if let Some(position) = position {
            field_push!(self, entries; (self.binding_literal_key(binding, AnalyzerLiteralKeyword("line"))?, self.number(position.line)?));
            field_push!(self, entries; (self.binding_literal_key(binding, AnalyzerLiteralKeyword("column"))?, self.number(position.column)?));
        }
        if let hir::LocalKind::Argument { index, rest } = binding.source_kind() {
            field_push!(self, entries; (self.binding_literal_key(binding, AnalyzerLiteralKeyword("arg-id"))?, self.number(index)?));
            field_push!(self, entries; (FieldKey::Cold("suss/rest-parameter"), self.flag(rest)?));
        }
        if let SourceRole::FunctionName { variadic, methods } = &binding.source_role {
            field_push!(self, entries; (FieldKey::Cold("suss/variadic"), self.flag(*variadic)?));
            if let Some(methods) = methods {
                field_push!(self, entries; (FieldKey::Literal(AnalyzerLiteralKeyword("fn-var")), self.flag(true)?));
                field_push!(self, entries; (FieldKey::Literal(AnalyzerLiteralKeyword("variadic?")), self.flag(*variadic)?));
                field_push!(self, entries; (FieldKey::Literal(AnalyzerLiteralKeyword("max-fixed-arity")), self.number(methods.max_fixed_arity())?));
                let mut parameters = Vec::new();
                for method in &methods.methods {
                    let bindings = method
                        .parameters
                        .iter()
                        .map(|parameter| self.local(parameter, depth + 1))
                        .collect::<Result<Vec<_>>>()?;
                    parameters.push(self.vector(&bindings)?);
                }
                field_push!(self, entries; (FieldKey::Literal(AnalyzerLiteralKeyword("method-params")), self.recipe(Recipe::List(parameters))?));
            }
        }
        if let Some(initializer) = &binding.initializer {
            field_push!(self, entries; (self.binding_literal_key(binding, AnalyzerLiteralKeyword("init"))?, self.ast(initializer, depth + 1)?));
            let init_key = self.binding_literal_key(binding, AnalyzerLiteralKeyword("init"))?;
            let init = self.keyword(init_key)?;
            field_push!(self, entries; (self.binding_literal_key(binding, AnalyzerLiteralKeyword("children"))?, self.vector(&[init])?));
        }
        if let Some(shadow) = &binding.shadow {
            field_push!(self, entries; (self.binding_literal_key(binding, AnalyzerLiteralKeyword("shadow"))?, self.local(shadow, depth + 1)?));
        } else if let Some(field) = &binding.shadow_field {
            field_push!(self, entries; (self.binding_literal_key(binding, AnalyzerLiteralKeyword("shadow"))?, self.field(field, depth + 1)?));
        }
        if matches!(binding.source_kind(), hir::LocalKind::Argument { .. }) {
            let shadow = if let Some((_, shadow)) = entries.iter().find(|(key, _)| key.name() == "shadow")
            {
                *shadow
            } else {
                let nil = self.scalar(Literal::Nil)?;
                field_push!(self, entries; (self.binding_literal_key(binding, AnalyzerLiteralKeyword("shadow"))?, nil));
                nil
            };
            let mut env = field_entries!(self; [(FieldKey::Literal(AnalyzerLiteralKeyword("context")), self.context(binding.declaration_context)?)]);
            for token in [AnalyzerLiteralKeyword("line"), AnalyzerLiteralKeyword("column")] {
                let key = token.name();
                if let Some((_, value)) = entries.iter().find(|(name, _)| name.name() == key) {
                    field_push!(self, env; (FieldKey::Literal(token), *value));
                }
            }
            let shadow_key = self.binding_literal_key(binding, AnalyzerLiteralKeyword("shadow"))?;
            let info = field_map!(self; [(FieldKey::Literal(AnalyzerLiteralKeyword("name")), entries[0].1), (shadow_key, shadow)])?;
            field_push!(self, entries; (FieldKey::Literal(AnalyzerLiteralKeyword("env")), self.map(env)?));
            field_push!(self, entries; (self.name_var_literal_key(binding, AnalyzerLiteralKeyword("info"))?, info));
        }
        if let SourceRole::MethodThis {
            type_declaration,
            namespace,
            protocol_receiver,
            argument,
            access,
            ..
        } = &binding.source_role
        {
            field_push!(self, entries; (FieldKey::Cold("suss/type-declaration"),
                self.form(type_declaration, depth + 1)?,
            ));
            field_push!(self, entries; (FieldKey::Cold("suss/declaration-namespace"), self.symbol(namespace)?));
            field_push!(self, entries; (FieldKey::Cold("suss/protocol-receiver"), self.flag(*protocol_receiver)?));
            field_push!(self, entries; (FieldKey::Cold("suss/receiver-access"), self.lowering(access, depth + 1)?));
            if let Some(argument) = argument {
                field_push!(self, entries; (self.binding_literal_key(binding, AnalyzerLiteralKeyword("shadow"))?, self.local(argument, depth + 1)?));
            }
        }
        if let SourceRole::CatchBinding { hidden, access } = &binding.source_role {
            field_push!(self, entries; (FieldKey::Cold("suss/private-payload"), self.local(hidden, depth + 1)?));
            field_push!(self, entries; (FieldKey::Cold("suss/payload-access"), self.lowering(access, depth + 1)?));
        }
        let context = self.context(binding.declaration_context)?;
        field_push!(self, entries; (FieldKey::Cold("suss/declaration-context"), context));
        self.map_in_task_slot(slot, entries)
    }
    fn field(&mut self, field: &Arc<FieldBinding>, depth: usize) -> Result<Value> {
        Self::depth(depth)?;
        let key = Arc::as_ptr(field) as usize;
        self.charge(1)?;
        if let Some((_, value)) = self.fields.get(&key) {
            return Ok(value.clone());
        }
        let native = self.task(Task::Field(field.clone()))?;
        let value = self.source_boundary(source_roots::Owner::Field(field.clone()), native)?;
        self.fields.insert(key, (field.clone(), value));
        Ok(value)
    }
    fn field_record(&mut self, slot: Value, field: &FieldBinding, depth: usize) -> Result<Value> {
        let name = self.form(&field.declaration, depth + 1)?;
        let kind = self.keyword("field")?;
        let is_field = self.flag(true)?;
        let index = self.number(field.index)?;
        let access = self.lowering(&field.access, depth + 1)?;
        let mut entries = field_entries!(self; [
            (FieldKey::Literal(AnalyzerLiteralKeyword("name")), name),
            (FieldKey::Literal(AnalyzerLiteralKeyword("local")), kind),
            (FieldKey::Literal(AnalyzerLiteralKeyword("field")), is_field),
            (FieldKey::Cold("suss/index"), index),
            (FieldKey::Cold("suss/access"), access),
        ]);
        // Pinned parse-type retains raw nullable flags, independently of the
        // lowered mutability boolean. Unique source fields have no shadow;
        // duplicate declarations are rejected by the compiler before this path.
        let metadata =
            crate::portable_macro_data::normalized_reader_metadata(self.session, &field.declaration)?;
        // c4295f3 analyzer.cljc3619-3628 parse-type: literal keys;
        // metadata-derived values still use the original reader Form route.
        for token in [
            AnalyzerLiteralKeyword("mutable"),
            AnalyzerLiteralKeyword("unsynchronized-mutable"),
            AnalyzerLiteralKeyword("volatile-mutable"),
            AnalyzerLiteralKeyword("tag"),
        ] {
            let key = token.name();
            let value = metadata.chunks_exact(2).find_map(|pair| {
                matches!(&pair[0].kind, Kind::Keyword(k) if k.namespace.is_none() && k.name == key)
                    .then_some(&pair[1])
            });
            let value = if let Some(value) = value {
                self.form(value, depth + 1)?
            } else {
                self.scalar(Literal::Nil)?
            };
            field_push!(self, entries; (FieldKey::Literal(token), value));
        }
        field_push!(self, entries; (FieldKey::Literal(AnalyzerLiteralKeyword("shadow")), self.scalar(Literal::Nil)?));
        let position = field
            .origin
            .as_ref()
            .map(|origin| symbol_position(self.session, origin, &field.declaration))
            .transpose()?
            .flatten();
        if let Some(position) = position {
            field_push!(self, entries; (FieldKey::Literal(AnalyzerLiteralKeyword("line")), self.number(position.line)?));
            field_push!(self, entries; (FieldKey::Literal(AnalyzerLiteralKeyword("column")), self.number(position.column)?));
        }
        self.map_in_task_slot(slot, entries)
    }
    fn scope(&mut self, scope: &Arc<FunctionScope>, depth: usize) -> Result<Value> {
        Self::depth(depth)?;
        let key = Arc::as_ptr(scope) as usize;
        self.charge(1)?;
        if let Some((_, value)) = self.scopes.get(&key) {
            return Ok(value.clone());
        }
        let native = self.task(Task::Scope(scope.clone()))?;
        let value = self.source_boundary(source_roots::Owner::Scope(scope.clone()), native)?;
        self.scopes.insert(key, (scope.clone(), value));
        Ok(value)
    }
    fn scope_record(
        &mut self,
        slot: Value,
        scope: &Arc<FunctionScope>,
        depth: usize,
    ) -> Result<Value> {
        let name = self.form(&scope.declaration, depth + 1)?;
        let kind = self.keyword("fn")?;
        let parents = scope
            .parents
            .iter()
            .map(|scope| self.scope(scope, depth + 1))
            .collect::<Result<Vec<_>>>()?;
        let parents = self.vector(&parents)?;
        let env = self.environment(
            &scope.namespace_snapshot,
            &scope.scope,
            &scope.locals,
            &scope.fields,
            &scope.parents,
            scope.declaration_context,
            depth + 1,
        )?;
        let explicit = self.flag(scope.self_binding.is_some())?;
        let phase = self.keyword(match scope.phase {
            portable::resolve::Phase::Runtime => "runtime",
            portable::resolve::Phase::Macro => "macro",
        })?;
        // c4295f3 analyzer.cljc2285-2297 fn-name-var.
        let mut info = field_entries!(self; [
            (FieldKey::Literal(AnalyzerLiteralKeyword("fn-self-name")), self.flag(true)?),
            (FieldKey::Literal(AnalyzerLiteralKeyword("ns")), self.symbol(&scope.namespace)?),
            (FieldKey::Literal(AnalyzerLiteralKeyword("fn-scope")), parents),
            (FieldKey::Cold("suss/explicit-self"), explicit),
            (FieldKey::Cold("suss/function-form"),
                self.form(scope.function_form.as_form(), depth + 1)?,
            ),
            (FieldKey::Cold("suss/phase"), phase),
        ]);
        if let Some(shadow) = &scope.shadow {
            field_push!(self, info; (FieldKey::Literal(AnalyzerLiteralKeyword("shadow")), self.local(shadow, depth + 1)?));
        } else if let Some(shadow) = &scope.shadow_field {
            field_push!(self, info; (FieldKey::Literal(AnalyzerLiteralKeyword("shadow")), self.field(shadow, depth + 1)?));
        } else {
            field_push!(self, info; (FieldKey::Literal(AnalyzerLiteralKeyword("shadow")), self.scalar(Literal::Nil)?));
        }
        let info = self.map(info)?;
        let mut entries = field_entries!(self; [
            (FieldKey::Literal(AnalyzerLiteralKeyword("op")), self.keyword("binding")?),
            (FieldKey::Literal(AnalyzerLiteralKeyword("form")), name),
            (FieldKey::Literal(AnalyzerLiteralKeyword("name")), name),
            (FieldKey::Literal(AnalyzerLiteralKeyword("local")), kind),
            (FieldKey::Literal(AnalyzerLiteralKeyword("env")), env),
            (FieldKey::Literal(AnalyzerLiteralKeyword("info")), info),
        ]);
        let metadata =
            crate::portable_macro_data::normalized_reader_metadata(self.session, &scope.declaration)?;
        if let Some(tag) = metadata.chunks_exact(2).find_map(|pair| {
            matches!(&pair[0].kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == "tag")
                .then_some(&pair[1])
        }).filter(|tag| !matches!(tag.kind, Kind::Nil)) {
            field_push!(self, entries; (FieldKey::Literal(AnalyzerLiteralKeyword("ret-tag")), self.form(tag, depth + 1)?));
        }
        self.map_in_task_slot(slot, entries)
    }
    fn ast(&mut self, hir: &Hir, depth: usize) -> Result<Value> {
        Self::depth(depth)?;
        let Some(source) = &hir.source else {
            return self.lowering(hir, depth);
        };
        let key = Arc::as_ptr(source) as usize;
        self.charge(1)?;
        if let Some((_, value)) = self.asts.get(&key) {
            return Ok(value.clone());
        }
        let native = self.task(Task::Ast(hir.clone()))?;
        let value = self.source_boundary(source_roots::Owner::Ast(source.clone()), native)?;
        self.asts.insert(key, (source.clone(), value));
        Ok(value)
    }
    fn callable(&mut self, callable: &Arc<hir::SourceCallable>) -> Result<Value> {
        let key = Arc::as_ptr(callable) as usize;
        self.charge(1)?;
        if let Some((_, value)) = self.callables.get(&key) {
            return Ok(*value);
        }
        let native = self.task(Task::Callable(callable.clone()))?;
        let value = self.source_boundary(source_roots::Owner::Callable(callable.clone()), native)?;
        self.callables.insert(key, (callable.clone(), value));
        Ok(value)
    }
    fn callable_record(
        &mut self,
        slot: Value,
        callable: &hir::SourceCallable,
        depth: usize,
    ) -> Result<Value> {
        let mut methods = Vec::new();
        for method in &callable.methods {
            let mut parameters = Vec::new();
            for parameter in &method.parameters {
                let form = Form {
                    span: parameter.span.clone(),
                    metadata: parameter.metadata.clone(),
                    kind: Kind::Symbol(suss_reader::Symbol {
                        namespace: None,
                        name: parameter.name.clone(),
                    }),
                };
                parameters.push(self.form(&form, depth + 1)?);
            }
            let parameters = self.vector(&parameters)?;
            let body = self.ast(&method.body, depth + 1)?;
            let variadic = self.flag(method.variadic)?;
            methods.push(self.map(vec![
                ("suss/parameters", parameters),
                ("suss/body", body),
                ("suss/variadic", variadic),
            ])?);
        }
        let methods = self.vector(&methods)?;
        let variadic = self.flag(callable.variadic())?;
        let fixed = callable.max_fixed_arity().ok_or_else(|| {
            SessionError::Host(wasmtime::Error::msg(
                "Invalid source callable method or variadic parameter list",
            ))
        })?;
        let fixed = self.number(fixed)?;
        self.map_in_task_slot(
            slot,
            vec![
                ("suss/methods", methods),
                ("suss/variadic", variadic),
                ("suss/max-fixed-arity", fixed),
            ],
        )
    }
    // Pinned analyzer.cljc1934/1966/2273/2590 marks these body maps using
    // literal :body?. The actual this-as expansion is a lexical binding body
    // (core.cljc1438-1443). A Suss future body has no such pinned origin.
    fn source_body_key(&mut self, source: &hir::SourceAnalysis) -> Result<FieldKey<'static>> {
        self.charge(5)?; // flag/origin reads, native match and key selection
        Ok(if source.is_body && matches!(source.body_origin,
            Some(hir::SourceBodyOrigin::FunctionMethod | hir::SourceBodyOrigin::Bindings
                | hir::SourceBodyOrigin::TryBody | hir::SourceBodyOrigin::FinallyBody
                | hir::SourceBodyOrigin::ThisAsBinding))
        {
            FieldKey::Literal(AnalyzerLiteralKeyword("body?"))
        } else {
            FieldKey::Cold("body?")
        })
    }
    fn ast_record(&mut self, slot: Value, hir: &Hir, depth: usize) -> Result<Value> {
        let source = hir.source.as_ref().expect("source AST task");
        let quote = if source.form.metadata.is_empty()
            && let Kind::List(items) = &source.form.kind
            && items.len() == 2
            && matches!(&items[0].kind, Kind::Symbol(head)
                if head.namespace.is_none() && head.name == "quote")
        {
            Some(items)
        } else {
            None
        };
        let (form, quoted_datum) = if let Some(items) = quote {
            // One actual quoted datum is shared by the source form and the
            // const child's form/val, even when reader construction repeats
            // ordinary map/set occurrences elsewhere in the analysis graph.
            validate_form_data(&source.form, &mut |units| {
                self.session.charge_host_work(units)
            })?;
            let datum = self.form(&items[1], depth + 1)?;
            self.charge(0)?;
            let datum = self.recipe(Recipe::Alias(datum))?;
            let head = self.form(&items[0], depth + 1)?;
            self.charge(0)?;
            let form = self.recipe(Recipe::List(vec![head, datum]))?;
            (form, Some(datum))
        } else {
            (self.form(&source.form, depth + 1)?, None)
        };
        let env = self.environment(
            &source.namespace_snapshot,
            &source.scope,
            &source.locals,
            &source.fields,
            &source.function_scopes,
            source.context,
            depth + 1,
        )?;
        let lowering = self.lowering(hir, depth + 1)?;
        // Only proved retained source-node routes authorize this literal key.
        // Unknown routes, including Suss-only Future/Await, stay cold.
        // Pinned fn2362-2368 and analyze-symbol4108-4116 construct their
        // own literal :env, independently of the :form payload. Pay the new
        // callable/form/resolution checks before inspecting native facts.
        self.charge(8)?;
        let env_key = match source.node.as_deref() {
            Some(hir::SourceNode::Vector(_) | hir::SourceNode::Map(_) | hir::SourceNode::Set(_)
                | hir::SourceNode::Do { .. } | hir::SourceNode::If { .. }
                | hir::SourceNode::Bindings { .. } | hir::SourceNode::Recur(_)
                | hir::SourceNode::Throw(_) | hir::SourceNode::Try { .. }
                | hir::SourceNode::Assign { .. } | hir::SourceNode::Invoke { .. }
                | hir::SourceNode::Construct { .. } | hir::SourceNode::WithMeta { .. }) =>
                FieldKey::Literal(AnalyzerLiteralKeyword("env")),
            None if source.form.metadata.is_empty()
                && matches!(&source.form.kind, Kind::Nil | Kind::Bool(_) | Kind::Number(_)
                    | Kind::String(_) | Kind::Keyword(_)) =>
                FieldKey::Literal(AnalyzerLiteralKeyword("env")),
            None if quoted_datum.is_some() => FieldKey::Literal(AnalyzerLiteralKeyword("env")),
            None if source.callable.is_some() => FieldKey::Literal(AnalyzerLiteralKeyword("env")),
            None if matches!(&source.form.kind, Kind::Symbol(_))
                && matches!(&source.resolved, Some(hir::SourceBinding::Global { .. }
                    | hir::SourceBinding::JavaScript { .. }
                    | hir::SourceBinding::Local(_) | hir::SourceBinding::Field(_))) =>
                FieldKey::Literal(AnalyzerLiteralKeyword("env")),
            _ => FieldKey::Cold("env"),
        };
        // Pinned analyzer constructors: control1838–1903/2450–2768,
        // fn2362–2379, resolved symbols4113–4163, collections4369–4400,
        // metadata4453–4461 and scalar const4592. The payload stays source data.
        // Unknown/generated/extension nodes and copied local/field references
        // do not acquire a literal token merely by having a source form.
        self.charge(20)?; // prepaid node/role/shape reads and bounded key selection
        let form_key = match source.node.as_deref() {
            Some(hir::SourceNode::Vector(_) | hir::SourceNode::Map(_) | hir::SourceNode::Set(_)
                | hir::SourceNode::Do { .. } | hir::SourceNode::If { .. }
                | hir::SourceNode::Bindings { .. } | hir::SourceNode::Recur(_)
                | hir::SourceNode::Throw(_) | hir::SourceNode::Try { .. }
                | hir::SourceNode::Assign { .. } | hir::SourceNode::Invoke { .. }
                | hir::SourceNode::Construct { .. } | hir::SourceNode::WithMeta { .. }) =>
                FieldKey::Literal(AnalyzerLiteralKeyword("form")),
            None if quoted_datum.is_some() => FieldKey::Literal(AnalyzerLiteralKeyword("form")),
            None if source.form.metadata.is_empty()
                && matches!(&source.form.kind, Kind::Nil | Kind::Bool(_) | Kind::Number(_)
                    | Kind::String(_) | Kind::Keyword(_)) =>
                FieldKey::Literal(AnalyzerLiteralKeyword("form")),
            None if source.callable.is_some() => FieldKey::Literal(AnalyzerLiteralKeyword("form")),
            None if matches!(&source.form.kind, Kind::Symbol(_))
                && matches!(&source.resolved, Some(hir::SourceBinding::Global { .. }
                    | hir::SourceBinding::JavaScript { .. }
                    | hir::SourceBinding::Local(_) | hir::SourceBinding::Field(_))) =>
                FieldKey::Literal(AnalyzerLiteralKeyword("form")),
            _ => FieldKey::Cold("form"),
        };
        let mut fields = field_entries!(self; [(form_key, form), (env_key, env), (FieldKey::Cold("suss/lowering"), lowering)]);
        if source.is_body {
            let body_key = self.source_body_key(source)?;
            field_push!(self, fields; (body_key, self.flag(true)?));
        }
        // Genuine scalar source constants follow pinned analyzer.cljc's
        // analyze-keyword/analyze-form contract, not the physical HIR value.
        // Metadata wrappers, quotes and compound source ASTs need their own
        // source records; do not fabricate those from a lowered result.
        if source.form.metadata.is_empty()
            && matches!(
                &source.form.kind,
                Kind::Nil | Kind::Bool(_) | Kind::Number(_) | Kind::String(_) | Kind::Keyword(_)
            )
        {
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("op")), self.keyword("const")?));
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("val")), form));
        }
        // Source entries were analyzed exactly once before physical collection
        // construction. Empty and factory literals keep these same source ops.
        // Metadata wrappers are handled separately; do not infer their schema
        // from the enclosing form or its lowered with-meta call.
        if let Some(collection) = &source.node
            && matches!(
                collection.as_ref(),
                hir::SourceNode::Vector(_) | hir::SourceNode::Map(_) | hir::SourceNode::Set(_)
            )
        {
            let (operation, children) = match collection.as_ref() {
                hir::SourceNode::Vector(items) => {
                    ("vector", field_entries!(self; [(FieldKey::Literal(AnalyzerLiteralKeyword("items")), items.iter().collect::<Vec<_>>())]))
                }
                hir::SourceNode::Set(items) => {
                    ("set", field_entries!(self; [(FieldKey::Literal(AnalyzerLiteralKeyword("items")), items.iter().collect::<Vec<_>>())]))
                }
                hir::SourceNode::Map(items) => {
                    if items.len() % 2 != 0 {
                        return Err(SessionError::Host(wasmtime::Error::msg(
                            "Unpaired source map children",
                        )));
                    }
                    (
                        "map",
                        field_entries!(self; [
                            (FieldKey::Literal(AnalyzerLiteralKeyword("keys")), items.iter().step_by(2).collect::<Vec<_>>()),
                            (FieldKey::Literal(AnalyzerLiteralKeyword("vals")), items.iter().skip(1).step_by(2).collect::<Vec<_>>()),
                        ]),
                    )
                }
                _ => unreachable!("non-collection handled separately"),
            };
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("op")), self.keyword(operation)?));
            let mut names = Vec::new();
            for (name, children) in children {
                names.push(self.keyword(name)?);
                let mut records = Vec::new();
                for child in children {
                    records.push(self.ast(child, depth + 1)?);
                }
                field_push!(self, fields; (name, self.vector(&records)?));
            }
            // c4295f3 analyzer.cljc4369–4400: all three collection
            // builders emit literal :children and literal child-name keywords.
            // `name` above carries that same explicit construction-site token.
            let children_key = FieldKey::Literal(AnalyzerLiteralKeyword("children"));
            field_push!(self, fields; (children_key, self.vector(&names)?));
        }
        if let Some(node) = &source.node
            && let hir::SourceNode::Bindings {
                is_loop,
                bindings,
                body,
            } = node.as_ref()
        {
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("op")), self.keyword(if *is_loop { "loop" } else { "let" })?));
            let mut records = Vec::new();
            for binding in bindings.iter() {
                records.push(self.local(binding, depth + 1)?);
            }
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("bindings")), self.vector(&records)?));
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("body")), self.ast(body, depth + 1)?));
            let bindings = self.keyword(FieldKey::Literal(AnalyzerLiteralKeyword("bindings")))?;
            let body = self.keyword(FieldKey::Literal(AnalyzerLiteralKeyword("body")))?;
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("children")), self.vector(&[bindings, body])?));
        }
        if let Some(hir::SourceNode::Try {
            body,
            handler,
            cleanup,
            payload,
        }) = source.node.as_deref()
        {
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("op")), self.keyword("try")?));
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("body")), self.ast(body, depth + 1)?));
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("catch")), self.ast(handler, depth + 1)?));
            let name = if let Some(payload) = payload {
                self.form(&payload.declaration, depth + 1)?
            } else {
                self.scalar(Literal::Nil)?
            };
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("name")), name));
            let mut children = vec![self.keyword(FieldKey::Literal(AnalyzerLiteralKeyword("body")))?, self.keyword(FieldKey::Literal(AnalyzerLiteralKeyword("catch")))?];
            let finally = if let Some(cleanup) = cleanup {
                children.push(self.keyword(FieldKey::Literal(AnalyzerLiteralKeyword("finally")))?);
                self.ast(cleanup, depth + 1)?
            } else {
                self.scalar(Literal::Nil)?
            };
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("finally")), finally));
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("children")), self.vector(&children)?));
        }
        if let Some(node) = &source.node {
            // These are original analyzed operands retained before dispatch or
            // storage lowering, not children reconstructed from physical HIR.
            let (operation, many, one) = match node.as_ref() {
                hir::SourceNode::Do { statements, result } => (
                    "do",
                    field_entries!(self; [(FieldKey::Literal(AnalyzerLiteralKeyword("statements")), statements.as_ref())]),
                    field_entries!(self; [(FieldKey::Literal(AnalyzerLiteralKeyword("ret")), result.as_ref())]),
                ),
                hir::SourceNode::If {
                    condition,
                    consequent,
                    alternative,
                } => (
                    "if",
                    vec![],
                    field_entries!(self; [
                        (FieldKey::Literal(AnalyzerLiteralKeyword("test")), condition.as_ref()),
                        (FieldKey::Literal(AnalyzerLiteralKeyword("then")), consequent.as_ref()),
                        (FieldKey::Literal(AnalyzerLiteralKeyword("else")), alternative.as_ref()),
                    ]),
                ),
                hir::SourceNode::Recur(arguments) => {
                    ("recur", field_entries!(self; [(FieldKey::Literal(AnalyzerLiteralKeyword("exprs")), arguments.as_ref())]), vec![])
                }
                hir::SourceNode::Throw(exception) => {
                    ("throw", vec![], field_entries!(self; [(FieldKey::Literal(AnalyzerLiteralKeyword("exception")), exception.as_ref())]))
                }
                hir::SourceNode::Assign { target, value } => (
                    "set!",
                    vec![],
                    field_entries!(self; [(FieldKey::Literal(AnalyzerLiteralKeyword("target")), target.as_ref()), (FieldKey::Literal(AnalyzerLiteralKeyword("val")), value.as_ref())]),
                ),
                hir::SourceNode::Invoke { callee, arguments } => (
                    "invoke",
                    field_entries!(self; [(FieldKey::Literal(AnalyzerLiteralKeyword("args")), arguments.as_ref())]),
                    field_entries!(self; [(FieldKey::Literal(AnalyzerLiteralKeyword("fn")), callee.as_ref())]),
                ),
                hir::SourceNode::Construct { class, arguments } => (
                    "new",
                    field_entries!(self; [(FieldKey::Literal(AnalyzerLiteralKeyword("args")), arguments.as_ref())]),
                    field_entries!(self; [(FieldKey::Literal(AnalyzerLiteralKeyword("class")), class.as_ref())]),
                ),
                hir::SourceNode::Future(body) => {
                    ("suss/future", vec![], field_entries!(self; [(FieldKey::Cold("body"), body.as_ref())]))
                }
                hir::SourceNode::Await(value) => {
                    ("suss/await", vec![], field_entries!(self; [(FieldKey::Cold("value"), value.as_ref())]))
                }
                _ => ("", vec![], vec![]),
            };
            if !operation.is_empty() {
                let op_key = match node.as_ref() {
                    hir::SourceNode::Do { .. } | hir::SourceNode::If { .. }
                    | hir::SourceNode::Recur(_) | hir::SourceNode::Throw(_)
                    | hir::SourceNode::Assign { .. } | hir::SourceNode::Invoke { .. }
                    | hir::SourceNode::Construct { .. } => FieldKey::Literal(AnalyzerLiteralKeyword("op")),
                    _ => FieldKey::Cold("op"),
                };
                field_push!(self, fields; (op_key, self.keyword(operation)?));
                // Pinned declared edge order. Authorize keys from the actual
                // retained SourceNode, never from the operation string alone.
                self.charge(4)?; // node read, native dispatch and pair selection
                let (names, children_key): (&[FieldKey<'static>], FieldKey<'static>) = match node.as_ref() {
                    hir::SourceNode::Do { .. } => (&[FieldKey::Literal(AnalyzerLiteralKeyword("statements")), FieldKey::Literal(AnalyzerLiteralKeyword("ret"))], FieldKey::Literal(AnalyzerLiteralKeyword("children"))),
                    hir::SourceNode::If { .. } => (&[FieldKey::Literal(AnalyzerLiteralKeyword("test")), FieldKey::Literal(AnalyzerLiteralKeyword("then")), FieldKey::Literal(AnalyzerLiteralKeyword("else"))], FieldKey::Literal(AnalyzerLiteralKeyword("children"))),
                    hir::SourceNode::Recur(_) => (&[FieldKey::Literal(AnalyzerLiteralKeyword("exprs"))], FieldKey::Literal(AnalyzerLiteralKeyword("children"))),
                    hir::SourceNode::Throw(_) => (&[FieldKey::Literal(AnalyzerLiteralKeyword("exception"))], FieldKey::Literal(AnalyzerLiteralKeyword("children"))),
                    hir::SourceNode::Assign { .. } => (&[FieldKey::Literal(AnalyzerLiteralKeyword("target")), FieldKey::Literal(AnalyzerLiteralKeyword("val"))], FieldKey::Literal(AnalyzerLiteralKeyword("children"))),
                    hir::SourceNode::Invoke { .. } => (&[FieldKey::Literal(AnalyzerLiteralKeyword("fn")), FieldKey::Literal(AnalyzerLiteralKeyword("args"))], FieldKey::Literal(AnalyzerLiteralKeyword("children"))),
                    hir::SourceNode::Construct { .. } => (&[FieldKey::Literal(AnalyzerLiteralKeyword("class")), FieldKey::Literal(AnalyzerLiteralKeyword("args"))], FieldKey::Literal(AnalyzerLiteralKeyword("children"))),
                    hir::SourceNode::Future(_) => (&[FieldKey::Cold("body")], FieldKey::Cold("children")),
                    hir::SourceNode::Await(_) => (&[FieldKey::Cold("value")], FieldKey::Cold("children")),
                    _ => unreachable!("operation route selected above"),
                };
                for (name, children) in many {
                    let mut records = Vec::new();
                    for child in children {
                        records.push(self.ast(child, depth + 1)?);
                    }
                    field_push!(self, fields; (name, self.vector(&records)?));
                }
                for (name, child) in one {
                    field_push!(self, fields; (name, self.ast(child, depth + 1)?));
                }
                let mut keywords = Vec::new();
                for name in names {
                    self.charge(2)?; // typed key/tag read before dispatch; no new heap metadata
                    keywords.push(self.keyword(*name)?);
                }
                field_push!(self, fields; (children_key, self.vector(&keywords)?));
            }
        }
        if let Some(node) = &source.node
            && let hir::SourceNode::WithMeta {
                expression,
                metadata,
                ..
            } = node.as_ref()
        {
            if expression.source.is_none() || metadata.source.is_none() {
                return Err(SessionError::Host(wasmtime::Error::msg(
                    "Incomplete metadata source children",
                )));
            }
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("op")), self.keyword("with-meta")?));
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("meta")), self.ast(metadata, depth + 1)?));
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("expr")), self.ast(expression, depth + 1)?));
            let meta = self.keyword(FieldKey::Literal(AnalyzerLiteralKeyword("meta")))?;
            let expr = self.keyword(FieldKey::Literal(AnalyzerLiteralKeyword("expr")))?;
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("children")), self.vector(&[meta, expr])?));
        }
        // Pinned analyzer.cljc2630–2655 retains quote syntax separately
        // from its literal const child. Use the actual retained source datum;
        // quoted collection constructor HIR is not a source child AST.
        if let Some(datum) = quoted_datum {
            let literal = self.flag(true)?;
            let mut constant = field_entries!(self; [
                (FieldKey::Literal(AnalyzerLiteralKeyword("op")), self.keyword("const")?),
                (FieldKey::Literal(AnalyzerLiteralKeyword("env")), env),
                (FieldKey::Literal(AnalyzerLiteralKeyword("literal?")), literal),
                (FieldKey::Literal(AnalyzerLiteralKeyword("val")), datum),
                (FieldKey::Literal(AnalyzerLiteralKeyword("form")), datum),
            ]);
            if let Some(tag) = &source.tags.quoted_const_tag {
                let tag = match tag {
                    Some(tag) => self.form(tag, depth + 1)?,
                    None => self.form(
                        &Form {
                            kind: Kind::Nil,
                            span: 0..0,
                            metadata: vec![],
                        },
                        depth + 1,
                    )?,
                };
                field_push!(self, constant; (FieldKey::Literal(AnalyzerLiteralKeyword("tag")), tag));
            }
            let constant = constant
                .into_iter()
                .map(|(key, value)| Ok((self.keyword(key)?, value)))
                .collect::<Result<Vec<_>>>()?;
            self.charge(0)?;
            let expr = self.recipe(Recipe::Map(constant))?;
            let expr_key = self.keyword(FieldKey::Literal(AnalyzerLiteralKeyword("expr")))?;
            field_extend!(self, fields; [
                (FieldKey::Literal(AnalyzerLiteralKeyword("op")), self.keyword("quote")?),
                (FieldKey::Literal(AnalyzerLiteralKeyword("literal?")), literal),
                (FieldKey::Literal(AnalyzerLiteralKeyword("expr")), expr),
                (FieldKey::Literal(AnalyzerLiteralKeyword("children")), self.vector(&[expr_key])?),
            ]);
        }
        // Only a genuine resolved source symbol is a local reference. A list
        // head may resolve to a local too, but its AST is an invocation, not a
        // reference. Pinned analyzer4113–4120 supplies literal select-keys
        // receivers; core.cljs9289–9301 assoc uses those receivers, not keys
        // copied from the binding descriptor. Descriptor role guards stay separate.
        // Share canonical declaration/initializer graph recipes.
        if matches!(&source.form.kind, Kind::Symbol(_)) {
            if let Some(hir::SourceBinding::Local(binding)) = &source.resolved {
                field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("op")), self.keyword("local")?));
                field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("info")), self.local(binding, depth + 1)?));
                field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("name")), self.form(&binding.declaration, depth + 1)?));
                field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("local")),
                    self.keyword(match binding.source_kind() {
                        hir::LocalKind::Let => "let",
                        hir::LocalKind::Loop => "loop",
                        hir::LocalKind::Argument { .. } => "arg",
                        hir::LocalKind::FunctionName => "fn",
                        hir::LocalKind::Catch => "catch",
                    })?,
                ));
                if let hir::LocalKind::Argument { index, .. } = binding.source_kind() {
                    field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("arg-id")), self.number(index)?));
                }
                if let SourceRole::FunctionName {
                    variadic,
                    methods: Some(_),
                } = &binding.source_role
                {
                    field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("variadic?")), self.flag(*variadic)?));
                }
                if let Some(initializer) = &binding.initializer {
                    field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("init")), self.ast(initializer, depth + 1)?));
                }
            }
        }
        if matches!(&source.form.kind, Kind::Symbol(_))
            && let Some(hir::SourceBinding::Field(binding)) = &source.resolved
        {
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("op")), self.keyword("local")?));
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("local")), self.keyword("field")?));
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("info")), self.field(binding, depth + 1)?));
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("name")), self.form(&binding.declaration, depth + 1)?));
        }
        // Owned JS references retain their genuine source operation. They do
        // not manufacture a source namespace/declaration revision or extern map.
        if matches!(&source.form.kind, Kind::Symbol(_))
            && let Some(hir::SourceBinding::JavaScript { global }) = &source.resolved
        {
            let op = self.keyword("js-var")?;
            let name = self.symbol(&format!("js/{}", global.name()))?;
            let ns = self.symbol("js")?;
            let tag = self.symbol("js")?;
            let info = field_map!(self; [(FieldKey::Literal(AnalyzerLiteralKeyword("op")), op), (FieldKey::Literal(AnalyzerLiteralKeyword("name")), name), (FieldKey::Literal(AnalyzerLiteralKeyword("ns")), ns), (FieldKey::Literal(AnalyzerLiteralKeyword("tag")), tag)])?;
            field_extend!(self, fields; [
                (FieldKey::Literal(AnalyzerLiteralKeyword("op")), op),
                (FieldKey::Literal(AnalyzerLiteralKeyword("name")), name),
                (FieldKey::Literal(AnalyzerLiteralKeyword("ns")), ns),
                (FieldKey::Literal(AnalyzerLiteralKeyword("tag")), tag),
                (FieldKey::Literal(AnalyzerLiteralKeyword("info")), info),
            ]);
        }
        let resolved_var = if matches!(&source.form.kind, Kind::Symbol(_))
            && let Some(hir::SourceBinding::Global {
                global,
                declaration,
            }) = &source.resolved
        {
            // Pinned resolve-var copies the captured declaration revision and
            // overrides these resolved identity fields. The catalog stays
            // untouched, including any raw :name/:ns metadata in that record.
            let declaration = self.declaration(global, declaration.as_ref(), depth + 1)?;
            // Source identity/retention alias is separate from the native map
            // whose selected fields resolve-var copies before its own overlay.
            let declaration = self.source_roots.native(self.session, declaration)?;
            let Some(Recipe::Map(entries)) = self.recipes.get(declaration).and_then(Option::as_ref)
            else {
                return Err(SessionError::Host(wasmtime::Error::msg(
                    "Missing resolved declaration graph recipe",
                )));
            };
            let mut entries = entries.clone();
            // Literal and source keys occupy distinct native recipe domains.
            // Read semantic native fields rather than inventing cold query keys.
            let tag = self.declaration_field_value(&entries, "tag")?;
            let operation = self.keyword("var")?;
            let name = self.symbol(&format!("{}/{}", global.namespace(), global.name()))?;
            let namespace = self.symbol(global.namespace())?;
            // Pinned resolve-var merges {:name ... :op :var :ns ...} into
            // the captured declaration. Existing key origin/order survives.
            self.declaration_field_assoc(&mut entries, AnalyzerLiteralKeyword("name"), name)?;
            self.declaration_field_assoc(&mut entries, AnalyzerLiteralKeyword("op"), operation)?;
            self.declaration_field_assoc(&mut entries, AnalyzerLiteralKeyword("ns"), namespace)?;
            self.charge(0)?;
            let info = self.recipe(Recipe::Map(entries))?;
            field_extend!(self, fields; [
                (FieldKey::Literal(AnalyzerLiteralKeyword("op")), operation),
                (FieldKey::Literal(AnalyzerLiteralKeyword("name")), name),
                (FieldKey::Literal(AnalyzerLiteralKeyword("ns")), namespace),
                // Each resolution makes a fresh outer map, while reusing the
                // captured declaration's nested values. Structural interning
                // would incorrectly make two distinct references identical.
                (FieldKey::Literal(AnalyzerLiteralKeyword("info")), info),
            ]);
            // Global AST tags are selected from resolved info, unlike local
            // reference inference. Preserve actual presence, including false.
            if let Some(tag) = tag {
                field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("tag")), tag));
            }
            true
        } else {
            false
        };
        if !resolved_var && let Some(tag) = &source.tags.tag {
            field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("tag")), self.form(tag, depth + 1)?));
        }
        if let Some(tag) = &source.tags.inferred_return {
            let value = if let Some(tag) = tag {
                self.form(tag, depth + 1)?
            } else {
                self.scalar(Literal::Nil)?
            };
            self.charge(5)?; // callable/node reads and exact native fn key selection
            let key = if source.callable.is_some() && source.node.is_none() {
                FieldKey::Literal(AnalyzerLiteralKeyword("inferred-ret-tag"))
            } else { FieldKey::Cold("inferred-ret-tag") };
            field_push!(self, fields; (key, value));
        }
        if let Some(callable) = &source.callable {
            field_push!(self, fields; (FieldKey::Cold("suss/source-function"), self.callable(callable)?));
            if !matches!(
                source.node.as_deref(),
                Some(hir::SourceNode::WithMeta { .. })
            ) {
                field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("op")), self.keyword("fn")?));
                self.charge(8)?; // node-presence read and three bounded key selections
                let (methods_key, local_key, children_key) = if source.node.is_none() {
                    (FieldKey::Literal(AnalyzerLiteralKeyword("methods")),
                     FieldKey::Literal(AnalyzerLiteralKeyword("local")),
                     FieldKey::Literal(AnalyzerLiteralKeyword("children")))
                } else {
                    (FieldKey::Cold("methods"), FieldKey::Cold("local"), FieldKey::Cold("children"))
                };
                let mut methods = Vec::new();
                // c4295f3 analyzer.cljc2263-2275 analyze-fn-method.
                for method in &callable.methods {
                    let mut parameters = Vec::new();
                    for declaration in &method.declarations {
                        parameters.push(self.local(declaration, depth + 1)?);
                    }
                    let parameters = self.vector(&parameters)?;
                    let body = self.ast(&method.body, depth + 1)?;
                    let form = self.form(&method.form, depth + 1)?;
                    let entry = &method.environment;
                    let env = self.environment(
                        &entry.namespace_snapshot,
                        &entry.scope,
                        &entry.locals,
                        &entry.fields,
                        &entry.function_scopes,
                        entry.context,
                        depth + 1,
                    )?;
                    let op = self.keyword("fn-method")?;
                    let params = self.keyword(FieldKey::Literal(AnalyzerLiteralKeyword("params")))?;
                    let body_key = self.keyword(FieldKey::Literal(AnalyzerLiteralKeyword("body")))?;
                    let children = self.vector(&[params, body_key])?;
                    let variadic = self.flag(method.variadic)?;
                    let arity = method
                        .parameters
                        .len()
                        .checked_sub(usize::from(method.variadic))
                        .ok_or_else(|| {
                            SessionError::Host(wasmtime::Error::msg(
                                "Invalid source method parameter list",
                            ))
                        })?;
                    let fixed = self.number(arity)?;
                    let recurs = match method.recurs {
                        Some(value) => self.flag(value)?,
                        None => self.scalar(Literal::Nil)?,
                    };
                    methods.push(field_map!(self; [
                        (FieldKey::Literal(AnalyzerLiteralKeyword("op")), op),
                        (FieldKey::Literal(AnalyzerLiteralKeyword("form")), form),
                        (FieldKey::Literal(AnalyzerLiteralKeyword("env")), env),
                        (FieldKey::Literal(AnalyzerLiteralKeyword("params")), parameters),
                        (FieldKey::Literal(AnalyzerLiteralKeyword("body")), body),
                        (FieldKey::Literal(AnalyzerLiteralKeyword("children")), children),
                        (FieldKey::Literal(AnalyzerLiteralKeyword("variadic?")), variadic),
                        (FieldKey::Literal(AnalyzerLiteralKeyword("fixed-arity")), fixed),
                        (FieldKey::Literal(AnalyzerLiteralKeyword("recurs")), recurs),
                    ])?);
                }
                field_push!(self, fields; (methods_key, self.vector(&methods)?));
                let methods = self.keyword(methods_key)?;
                let children = if let Some(scope) = &callable.name {
                    let name = self.scope(scope, depth + 1)?;
                    field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("name")), name));
                    field_push!(self, fields; (local_key, name));
                    let local = self.keyword(local_key)?;
                    self.vector(&[local, methods])?
                } else {
                    field_push!(self, fields; (FieldKey::Literal(AnalyzerLiteralKeyword("name")), self.scalar(Literal::Nil)?));
                    self.vector(&[methods])?
                };
                field_push!(self, fields; (children_key, children));
            }
        }
        self.map_in_task_slot(slot, fields)
    }
    fn lowering(&mut self, hir: &Hir, depth: usize) -> Result<Value> {
        self.task(Task::Lowering(hir.clone()))
    }
    fn lowering_record(&mut self, slot: Value, hir: &Hir, depth: usize) -> Result<Value> {
        Self::depth(depth)?;
        let (operation, children): (&str, Vec<&Hir>) = match &hir.kind {
            Expression::Future { body, .. } => ("future", vec![body]),
            Expression::Await { value } => ("await", vec![value]),
            Expression::AsyncTry {
                body,
                handler,
                cleanup,
                ..
            } => (
                "async-try",
                std::iter::once(body.as_ref())
                    .chain(handler.iter().map(|h| h.as_ref()))
                    .chain(cleanup.iter().map(|h| h.as_ref()))
                    .collect(),
            ),
            Expression::Literal(_) => ("literal", vec![]),
            Expression::Local(_) => ("local", vec![]),
            Expression::Global(_) => ("global", vec![]),
            Expression::GlobalCell(_) => ("global-cell", vec![]),
            Expression::GlobalOrFallback { .. } => ("reader-global-fallback", vec![]),
            Expression::Bitwise { arguments, .. }
            | Expression::Comparison { arguments, .. }
            | Expression::Array { arguments, .. }
            | Expression::Nominal { arguments, .. }
            | Expression::Arithmetic { arguments, .. } => {
                ("native-operation", arguments.iter().collect())
            }
            Expression::NilTest(value) => ("nil-test", vec![value]),
            Expression::Throw(value) => ("throw", vec![value]),
            Expression::Assign { value, .. } => ("assign", vec![value]),
            Expression::Definition { initializer, .. } => (
                "definition",
                initializer.iter().map(|value| value.as_ref()).collect(),
            ),
            Expression::Let { bindings, body } => (
                "let",
                bindings
                    .iter()
                    .map(|binding| binding.value.as_ref())
                    .chain(std::iter::once(body.as_ref()))
                    .collect(),
            ),
            Expression::Loop { bindings, body, .. } => (
                "loop",
                bindings
                    .iter()
                    .map(|binding| binding.value.as_ref())
                    .chain(std::iter::once(body.as_ref()))
                    .collect(),
            ),
            Expression::If {
                condition,
                consequent,
                alternative,
            } => ("if", vec![condition, consequent, alternative]),
            Expression::Do(values) => ("do", values.iter().collect()),
            Expression::Recur { arguments, .. } => ("recur", arguments.iter().collect()),
            Expression::Call { callee, arguments } => (
                "call",
                std::iter::once(callee.as_ref())
                    .chain(arguments.iter())
                    .collect(),
            ),
            Expression::Function { body, .. } => ("function", vec![body]),
            Expression::GeneralFunction { methods, .. } => (
                "function",
                methods.iter().map(|method| method.body.as_ref()).collect(),
            ),
            Expression::AsyncDynamicScope { bindings, body } => (
                "async-dynamic-scope",
                bindings
                    .iter()
                    .map(|(_, value)| value)
                    .chain(std::iter::once(body.as_ref()))
                    .collect(),
            ),
            Expression::DynamicScope { bindings, body } => (
                "dynamic-scope",
                bindings
                    .iter()
                    .map(|(_, value)| value)
                    .chain(std::iter::once(body.as_ref()))
                    .collect(),
            ),
            Expression::Try { regions } => (
                "try",
                regions.iter().map(|region| region.as_ref()).collect(),
            ),
        };
        let operation = self.keyword(operation)?;
        let mut entries = vec![("suss/operation", operation)];
        match &hir.kind {
            Expression::Literal(value) if !matches!(value, Literal::Undefined) => {
                entries.push(("suss/value", self.scalar(value.clone())?))
            }
            Expression::Local(id) => entries.push(("suss/id", self.number(id.0)?)),
            Expression::Literal(Literal::Undefined) => {
                entries.push(("suss/undefined", self.flag(true)?))
            }
            Expression::Arithmetic { operator, .. } => {
                entries.push(("suss/native-operator", self.text(&format!("{operator:?}"))?))
            }
            Expression::Nominal { operation, .. } => entries.push((
                "suss/native-operator",
                self.text(&format!("{operation:?}"))?,
            )),
            Expression::Array { operation, .. } => entries.push((
                "suss/native-operator",
                self.text(&format!("{operation:?}"))?,
            )),
            Expression::Comparison { operation, .. } => entries.push((
                "suss/native-operator",
                self.text(&format!("{operation:?}"))?,
            )),
            Expression::Bitwise { operation, .. } => entries.push((
                "suss/native-operator",
                self.text(&format!("{operation:?}"))?,
            )),
            Expression::Loop { target, .. } | Expression::Recur { target, .. } => {
                entries.push(("suss/loop-id", self.number(target.0)?))
            }
            _ => {}
        }
        let children = children
            .into_iter()
            .map(|child| self.ast(child, depth + 1))
            .collect::<Result<Vec<_>>>()?;
        entries.push(("suss/children", self.vector(&children)?));
        self.map_in_task_slot(slot, entries)
    }
}

#[cfg(test)]
mod analyzer_literal_tests;

#[cfg(test)]
mod sharing_tests {
    use super::*;
    fn number(bits: u64) -> Form {
        Form {
            span: 0..0,
            metadata: vec![],
            kind: Kind::Number(f64::from_bits(bits)),
        }
    }
    #[test]
    fn owned_normalized_metadata_fuel_failure_preserves_input_and_retry_recovers() {
        const MARKER: &str = "owned-prefix-marker";
        fn marker_emitted(graph: &AnalysisGraph<'_>) -> bool {
            graph.recipes.iter().flatten().any(|recipe| {
                matches!(recipe, Recipe::Form(Form { kind: Kind::Symbol(symbol), .. })
                    if symbol.namespace.is_none() && symbol.name == MARKER)
            })
        }
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        // Marker exists only inside metadata. Raw-key planning and normalization
        // do not emit Recipe::Form, so observing it witnesses owned Work::Visit.
        let source = format!("^{{:owned [owned-prefix-marker {}]}} [17]",
            (0..64).map(|n| format!("tail-{n}")).collect::<Vec<_>>().join(" "));
        let form = suss_reader::forms::read_forms(&source).unwrap().remove(0);
        let original = form.clone();
        let full_cost = {
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            let operation = graph.session.operation_budget_scope().unwrap();
            let before = graph.session.operation_fuel_remaining().unwrap();
            graph.form(&form, 0).unwrap();
            assert!(marker_emitted(&graph));
            let cost = before - graph.session.operation_fuel_remaining().unwrap();
            drop(operation);
            cost
        };
        // Find the first allowance at which the owned marker has been published.
        // Traversal is native/deterministic; each probe has a fresh graph/interner.
        // This avoids a slow unit-by-unit fuel sweep or timing-based IRQ injection.
        let (mut low, mut high) = (0u64, full_cost);
        while low < high {
            let fuel = low + (high - low) / 2;
            session.set_operation_fuel(fuel);
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            let operation = graph.session.operation_budget_scope().unwrap();
            if let Err(error) = graph.form(&form, 0) {
                assert!(matches!(error, SessionError::Trap(ref error)
                    if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel))
                    || matches!(error, SessionError::Compile(ref error)
                        if error.message.contains("all fuel consumed")), "{error:?}");
            }
            let reached = marker_emitted(&graph);
            drop(operation);
            if reached { high = fuel; } else { low = fuel + 1; }
        }
        assert!(low < full_cost, "fixture needs owned work after its first marker");
        session.set_operation_fuel(low);
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let operation = graph.session.operation_budget_scope().unwrap();
        let error = graph.form(&form, 0).unwrap_err();
        assert!(matches!(error, SessionError::Trap(ref error)
            if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
        assert!(marker_emitted(&graph), "must reach owned normalized-metadata traversal");
        assert_eq!(form, original, "failed import cannot consume borrowed source");
        drop(operation);

        graph.session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        let retry_operation = graph.session.operation_budget_scope().unwrap();
        let root = graph.form(&form, 0).unwrap();
        assert_eq!(form, original);
        let value = graph.materialize(root).unwrap();
        drop(retry_operation);
        graph.session.collect().unwrap();
        let check = graph.session.eval(r#"
          (fn [v] [(= (nth v 0) 17)
                    (= (first (:owned (meta v))) 'owned-prefix-marker)
                    (= (count (:owned (meta v))) 65)
                    (= (last (:owned (meta v))) 'tail-63)])
        "#).unwrap();
        let result = graph.session.invoke(&check, &[&value]).unwrap();
        assert_eq!(crate::portable_repl::display(graph.session, &result).unwrap(),
            "[true true true true]");
    }

    #[test]
    fn borrowed_body_and_owned_metadata_cold_utf16_leaves_are_independent_copies() {
        fn plain(span: std::ops::Range<usize>, kind: Kind) -> Form {
            Form { span, metadata: Vec::new(), kind }
        }
        let body_units = vec![0, 0xd800, 97, 0xdc00, 0xffff];
        let metadata_units = vec![0xdc00, 0, 0xd800, 0xd83d, 0xde00];
        let mut form = Form {
            span: 10..90,
            metadata: vec![plain(10..40, Kind::Map(vec![
                plain(11..18, Kind::Keyword(suss_reader::Keyword::new("payload"))),
                plain(19..39, Kind::String(metadata_units.clone())),
            ]))],
            kind: Kind::Vector(vec![plain(50..89, Kind::String(body_units.clone()))]),
        };
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        // No priming: body is borrowed, normalized metadata is an owned form, and
        // distinct strings ensure neither leaf can be substituted by the other's hit.
        let root = graph.form(&form, 0).unwrap();
        let Kind::Vector(body) = &mut form.kind else { unreachable!() };
        let Kind::String(units) = &mut body[0].kind else { unreachable!() };
        units.fill(88);
        let Kind::Map(pairs) = &mut form.metadata[0].kind else { unreachable!() };
        let Kind::String(units) = &mut pairs[1].kind else { unreachable!() };
        units.fill(89);

        let value = graph.materialize(root).unwrap();
        graph.session.collect().unwrap();
        let extract = graph.session.eval("(fn [v] [(nth v 0) (:payload (meta v))])").unwrap();
        let result = graph.session.invoke(&extract, &[&value]).unwrap();
        let decoded = bridge.read(graph.session, &result, 0..1).unwrap();
        let Kind::Vector(items) = &decoded.kind else { panic!("string pair") };
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, Kind::String(body_units));
        assert_eq!(items[1].kind, Kind::String(metadata_units));
    }
    #[test]
    fn shared_dag_validation_charges_each_dependency_edge_once() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let value = {
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            let scalar = graph.scalar(Literal::Number(42.0)).unwrap();
            let root = graph.vector(&[scalar, scalar]).unwrap();
            // Two unique nodes require entry/finish charges (4), and both
            // dependency edges require a charge (2). The second scalar edge
            // must not revalidate the already-checked node. Occurrence work
            // still charges both edges plus the two actual constructions (4).
            graph.materialize(root).unwrap()
        };
        session.collect().unwrap();
        let Kind::Vector(values) = bridge.read(&mut session, &value, 0..1).unwrap().into_parts().2 else {
            panic!("Expected shared scalar vector");
        };
        assert_eq!(values.len(), 2);
        assert!(values.iter().all(|value| value.kind == Kind::Number(42.0)));
    }
    #[test]
    fn exact_namespace_identity_hits_charge_lookup_not_snapshot_size() {
        let mut session = Session::new_macro().unwrap();
        session.eval("(def namespace-identity-revision-probe 17)").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let snapshot = session.compilation_snapshot();
        let mut small = SourceNamespace::capture(
            &snapshot.environment, portable::resolve::Phase::Macro,
        );
        // Keep actual immutable namespace facts; change only ordinary aliases.
        let mut large = small.clone();
        for index in 0..128 {
            large.aliases.insert(format!("identity-probe-{index}"), "identity.target".into());
        }
        // Keep namespace text equal so the comparison isolates fact count.
        small.aliases.clear();
        let small = Arc::new(small);
        let large = Arc::new(large);
        let equal_content = Arc::new(large.as_ref().clone());
        let mut changed_revision = large.as_ref().clone();
        let global = changed_revision.declarations.keys().next().unwrap().clone();
        let definition = changed_revision.declarations[&global].clone();
        changed_revision.declarations.insert(global, Arc::new(definition.as_ref().clone()));
        let changed_revision = Arc::new(changed_revision);

        let _request = session.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let small_root = graph.namespace(&small, 0).unwrap();
        let large_root = graph.namespace(&large, 0).unwrap();
        assert_ne!(small_root, large_root);
        assert_eq!(large_root, graph.namespace(&equal_content, 0).unwrap());
        assert_ne!(large_root, graph.namespace(&changed_revision, 0).unwrap());
        // These caches are native recipe identities, not retained guest roots.
        // Measure only repeated lookups after the initial recipe build.
        let recipes = graph.recipes.len();
        let jobs = graph.jobs.len();
        let before = graph.session.operation_fuel_remaining().unwrap();
        for _ in 0..16 {
            assert_eq!(small_root, graph.namespace(&small, 0).unwrap());
        }
        let after_small = graph.session.operation_fuel_remaining().unwrap();
        for _ in 0..16 {
            assert_eq!(large_root, graph.namespace(&large, 0).unwrap());
        }
        let after_large = graph.session.operation_fuel_remaining().unwrap();
        let small_cost = before - after_small;
        let large_cost = after_small - after_large;
        assert!(small_cost >= 16, "identity hits must still charge lookup work");
        assert_eq!(large_cost, small_cost,
            "exact identity hits must not charge traversal of namespace facts");
        assert_eq!(graph.recipes.len(), recipes);
        assert_eq!(graph.jobs.len(), jobs);

        // Cancellation must be observed even on the already-populated fast path.
        let interrupt = graph.session.host_work_interrupt();
        interrupt.store(true, std::sync::atomic::Ordering::SeqCst);
        let error = graph.namespace(&large, 0).unwrap_err();
        assert!(error.is_interrupt());
        assert_eq!(graph.session.operation_fuel_remaining().unwrap(), 0);
        assert_eq!(graph.recipes.len(), recipes);
        assert_eq!(graph.jobs.len(), jobs);
        drop(graph);
        drop(_request);

        // A separate operation recovers with the default unchanged allowance.
        let _request = session.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let root = graph.namespace(&large, 0).unwrap();
        assert_eq!(root, graph.namespace(&large, 0).unwrap());
    }

    #[test]
    fn namespace_snapshots_share_only_when_all_facts_and_declaration_revisions_match() {
        let mut session = Session::new_macro().unwrap();
        session.eval("(def namespace-cache-value 17)").unwrap();
        let snapshot = session.compilation_snapshot();
        let source = Arc::new(SourceNamespace::capture(
            &snapshot.environment,
            portable::resolve::Phase::Macro,
        ));
        let same_revision = Arc::new(source.as_ref().clone());
        assert_eq!(
            NamespaceKey::new(&source),
            NamespaceKey::new(&same_revision)
        );

        let target = source.declarations.keys().next().unwrap().clone();
        for field in 0..11 {
            let mut changed = source.as_ref().clone();
            match field {
                0 => changed.namespace.push_str(".different"),
                1 => {
                    changed.aliases.insert("probe".into(), "other".into());
                }
                2 => {
                    changed.refers.insert("probe".into(), target.clone());
                }
                3 => {
                    changed.used_refers.insert("probe".into());
                }
                4 => {
                    changed.renamed_refers.insert("probe".into());
                }
                5 => {
                    changed.excluded_core.insert("probe".into());
                }
                6 => {
                    changed.macro_aliases.insert("probe".into(), "other".into());
                }
                7 => {
                    changed
                        .macro_refers
                        .insert("probe".into(), ("other".into(), "macro".into()));
                }
                8 => {
                    changed.used_macro_refers.insert("probe".into());
                }
                9 => {
                    changed.renamed_macro_refers.insert("probe".into());
                }
                10 => {
                    changed.identities.push(target.clone());
                }
                _ => unreachable!(),
            }
            assert_ne!(
                NamespaceKey::new(&source),
                NamespaceKey::new(&Arc::new(changed)),
                "namespace field {field}"
            );
        }

        let mut changed_facts = source.as_ref().clone();
        changed_facts.excluded_core.insert("cache-probe".into());
        let changed_facts = Arc::new(changed_facts);
        assert_ne!(
            NamespaceKey::new(&source),
            NamespaceKey::new(&changed_facts)
        );

        let mut changed_revision = source.as_ref().clone();
        let (global, definition) = changed_revision
            .declarations
            .iter()
            .find(|(global, _)| global.name() == "namespace-cache-value")
            .map(|(global, definition)| (global.clone(), definition.clone()))
            .expect("captured definition revision");
        changed_revision
            .declarations
            .insert(global, Arc::new(definition.as_ref().clone()));
        let changed_revision = Arc::new(changed_revision);
        assert_ne!(
            NamespaceKey::new(&source),
            NamespaceKey::new(&changed_revision)
        );

        let bridge = FormBridge::new(&mut session).unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let original = graph.namespace(&source, 0).unwrap();
        let same = graph.namespace(&same_revision, 0).unwrap();
        let changed = graph.namespace(&changed_facts, 0).unwrap();
        let new_revision = graph.namespace(&changed_revision, 0).unwrap();
        assert_eq!(
            original, same,
            "same source revision should reuse its graph root"
        );
        assert_ne!(
            original, changed,
            "different namespace facts must remain distinct"
        );
        assert_ne!(
            original, new_revision,
            "different declaration Arc is a new revision"
        );
        assert_eq!(graph.namespace_contents.len(), 3);
        assert_eq!(graph.namespaces.len(), 4);
        graph.materialize(original).unwrap();
        assert!(matches!(
            graph.recipes[original].as_ref(),
            Some(Recipe::Map(_))
        ));
        assert!(graph.maps.values().any(|value| *value == original));
        assert!(matches!(
            graph.recipes[changed].as_ref(),
            Some(Recipe::Map(_))
        ));
        assert!(matches!(
            graph.recipes[new_revision].as_ref(),
            Some(Recipe::Map(_))
        ));
    }
    #[test]
    fn task_map_slot_keeps_alias_on_a_canonical_map_cache_hit() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let value = graph.scalar(Literal::Number(17.0)).unwrap();
        let canonical = graph.map(vec![("value", value)]).unwrap();
        let map_count = graph.maps.len();

        // A task slot is already charged by task(); simulate only its reserved
        // recipe storage so this test isolates the final-root cache behavior.
        graph.charge(0).unwrap();
        let slot = graph.recipes.len();
        graph.recipes.push(None);
        assert_eq!(
            graph
                .map_in_task_slot(slot, vec![("value", value)])
                .unwrap(),
            slot
        );
        assert!(
            matches!(graph.recipes[slot].as_ref(), Some(Recipe::Alias(target)) if *target == canonical)
        );
        assert_eq!(graph.maps.len(), map_count);
    }
    #[test]
    fn declaration_metadata_builds_do_not_reuse_guest_mutated_nested_storage() {
        let mut session = Session::new_macro().unwrap();
        session.eval("(def ^{:audit {:x 17}} review-poisoned-metadata 0)").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let snapshot = session.compilation_snapshot();
        let namespace = SourceNamespace::capture(&snapshot.environment, portable::resolve::Phase::Macro);
        let (global, source) = namespace.declarations.iter()
            .find(|(global, _)| global.name() == "review-poisoned-metadata")
            .map(|(global, source)| (global.clone(), source.clone())).unwrap();
        let build = |session: &mut Session| {
            let _request = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, session);
            let root = graph.declaration(&global, Some(&source), 0).unwrap();
            graph.materialize(root).unwrap()
        };
        let observe = session.eval("(fn [declaration] (get (get (get declaration :meta) :audit) :x))").unwrap();
        let mutate = session.eval("(fn [declaration] (let [audit (get (get declaration :meta) :audit)] (aset (.-arr audit) 1 99) (get audit :x)))").unwrap();
        let first = build(&mut session);
        let value = session.invoke(&observe, &[&first]).unwrap();
        assert_eq!(bridge.read(&mut session, &value, 0..1).unwrap().kind, Kind::Number(17.0));
        let value = session.invoke(&mutate, &[&first]).unwrap();
        assert_eq!(bridge.read(&mut session, &value, 0..1).unwrap().kind, Kind::Number(99.0),
            "witness backing-array mutation before testing a new build");
        session.collect().unwrap();
        for _ in 0..2 {
            let fresh = build(&mut session);
            let value = session.invoke(&observe, &[&fresh]).unwrap();
            assert_eq!(bridge.read(&mut session, &value, 0..1).unwrap().kind, Kind::Number(17.0),
                "the same source Arc must produce unpoisoned metadata on every build");
            let value = session.invoke(&observe, &[&first]).unwrap();
            assert_eq!(bridge.read(&mut session, &value, 0..1).unwrap().kind, Kind::Number(99.0),
                "new builds must neither reuse nor repair the old guest root");
            session.collect().unwrap();
        }
    }

    #[test]
    fn declaration_metadata_builds_keep_budgets_and_revision_independence() {
        let mut session = Session::new_macro().unwrap();
        session.eval("(def ^{:doc \"retained\"} review-retained 17)").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let snapshot = session.compilation_snapshot();
        let namespace = SourceNamespace::capture(&snapshot.environment, portable::resolve::Phase::Macro);
        let (global, source) = namespace.declarations.iter()
            .find(|(global, _)| global.name() == "review-retained")
            .map(|(global, source)| (global.clone(), source.clone())).unwrap();
        let build = |session: &mut Session, source: &Arc<portable::resolve::DefinitionInfo>| {
            let _request = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, session);
            let root = graph.declaration(&global, Some(source), 0).unwrap();
            assert_eq!(root, graph.declaration(&global, Some(source), 0).unwrap(),
                "same-revision recipe sharing remains local to a build");
            graph.materialize(root).unwrap()
        };
        let left = build(&mut session, &source);
        session.collect().unwrap();
        session.set_operation_fuel(1);
        let graph_error = {
            let _request = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            graph.declaration(&global, Some(&source), 0).unwrap_err()
        };
        assert!(graph_error.to_string().to_lowercase().contains("fuel"));
        // Repeated builds after success retain the same admission constraints.
        for _ in 0..2 {
            session.set_operation_fuel(10_000_000);
            let materialization_error = {
                let construction_request = session.operation_budget_scope().unwrap();
                let mut graph = AnalysisGraph::new(&bridge, &mut session);
                let root = graph.declaration(&global, Some(&source), 0).unwrap();
                drop(construction_request);
                graph.session.set_operation_fuel(1);
                let _request = graph.session.operation_budget_scope().unwrap();
                graph.materialize(root).unwrap_err()
            };
            assert!(materialization_error.to_string().to_lowercase().contains("fuel"));
        }
        session.set_operation_fuel(10_000_000);
        let other = Arc::new(source.as_ref().clone());
        {
            let _request = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            let original = graph.declaration(&global, Some(&source), 0).unwrap();
            let distinct = graph.declaration(&global, Some(&other), 0).unwrap();
            assert_ne!(original, distinct, "equal data in distinct source Arcs remains a separate revision");
        }
        let right = build(&mut session, &other);
        let compare = session.eval("(fn [a b] (identical? (get a :meta) (get b :meta)))").unwrap();
        let result = session.invoke(&compare, &[&left, &right]).unwrap();
        assert_eq!(bridge.read(&mut session, &result, 0..1).unwrap().kind, Kind::Bool(false));
        let observe = session.eval("(fn [d] (get (get d :meta) :doc))").unwrap();
        for root in [&left, &right] {
            let value = session.invoke(&observe, &[root]).unwrap();
            assert_eq!(bridge.read(&mut session, &value, 0..1).unwrap().kind,
                Kind::String("retained".encode_utf16().collect()));
        }
        drop(right);
        drop(other);
        session.collect().unwrap();
        let fresh = build(&mut session, &source);
        let value = session.invoke(&observe, &[&fresh]).unwrap();
        assert_eq!(bridge.read(&mut session, &value, 0..1).unwrap().kind,
            Kind::String("retained".encode_utf16().collect()));
        let value = session.eval("review-retained").unwrap();
        assert_eq!(bridge.read(&mut session, &value, 0..1).unwrap().kind, Kind::Number(17.0));
    }

    #[test]
    fn declaration_metadata_rebuilds_repeat_hash_callbacks_in_source_order() {
        let mut session = Session::new_macro().unwrap();
        session.enter_namespace("cljs.core").unwrap();
        session.eval("(def metadata-hash-log (array)) (def metadata-original-hash suss.core/hash) (def hash (fn [key] (do (if (number? key) (aset metadata-hash-log (alength metadata-hash-log) key) nil) (metadata-original-hash key))))").unwrap();
        session.eval("(def ^{:audit #{101 102 103 104 105 106 107 108 109}} review-callback-metadata 0)").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let snapshot = session.compilation_snapshot();
        let namespace = SourceNamespace::capture(&snapshot.environment, portable::resolve::Phase::Macro);
        let (global, source) = namespace.declarations.iter()
            .find(|(global, _)| global.name() == "review-callback-metadata")
            .map(|(global, source)| (global.clone(), source.clone())).unwrap();
        let mut baseline: Option<Vec<f64>> = None;
        for _ in 0..2 {
            session.eval("(set! metadata-hash-log (array))").unwrap();
            let value = {
                let _request = session.operation_budget_scope().unwrap();
                let mut graph = AnalysisGraph::new(&bridge, &mut session);
                let root = graph.declaration(&global, Some(&source), 0).unwrap();
                graph.materialize(root).unwrap()
            };
            session.collect().unwrap();
            let log = session.eval("(seq metadata-hash-log)").unwrap();
            let form = bridge.read(&mut session, &log, 0..1).unwrap();
            let Kind::List(items) = &form.kind else { panic!("nonempty numeric hash callback log") };
            let actual = items.iter().map(|item| {
                let Kind::Number(value) = &item.kind else { panic!("numeric hash key") };
                *value
            }).collect::<Vec<_>>();
            // A declaration can contain more than one reader occurrence. Each
            // nine-item set occurrence must keep its complete textual order.
            assert!(!actual.is_empty());
            assert_eq!(actual.len() % 9, 0);
            for occurrence in actual.chunks_exact(9) {
                assert_eq!(occurrence, &[101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0, 108.0, 109.0]);
            }
            if let Some(expected) = &baseline {
                assert_eq!(&actual, expected, "later builds must repeat callback count and order");
            } else {
                baseline = Some(actual);
            }
            drop(value);
        }
    }

    #[test]
    fn raw_splicing_forms_have_distinct_cache_identity_and_are_rejected_by_consumers() {
        let items = suss_reader::forms::read_forms("[:cljs [17]]").unwrap();
        let splice = Form {
            span: 0..1,
            metadata: vec![],
            kind: Kind::SplicingConditional(items.clone()),
        };
        let conditional = Form {
            span: 0..1,
            metadata: vec![],
            kind: Kind::Conditional(items.clone()),
        };
        let vector = Form {
            span: 0..1,
            metadata: vec![],
            kind: Kind::Vector(items),
        };
        assert_ne!(form_key(&splice).unwrap(), form_key(&conditional).unwrap());
        assert_ne!(form_key(&splice).unwrap(), form_key(&vector).unwrap());
        let nested = Form {
            span: 0..1,
            metadata: vec![],
            kind: Kind::List(vec![splice]),
        };
        assert!(
            validate_form_data(&nested, &mut |_| Ok(()))
                .unwrap_err()
                .to_string()
                .contains("resolved reader prefixes")
        );
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let error = bridge
            .quote(&mut session, nested)
            .err()
            .expect("raw splice must be rejected");
        assert!(error.to_string().contains("resolved reader prefixes"));
    }
    #[test]
    fn new_parent_of_interned_large_child_fits_finite_fuel_without_discarded_copy() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let child = Form { span: 0..0, metadata: vec![], kind: Kind::String(vec![120; 65536]) };
        let parent = Form { span: 0..0, metadata: vec![], kind: Kind::Vector(vec![child.clone()]) };
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let operation = graph.session.operation_budget_scope().unwrap();
        let child_value = graph.form(&child, 0).unwrap();
        drop(operation);
        graph.session.set_operation_fuel(1_000_000);
        let operation = graph.session.operation_budget_scope().unwrap();
        let before = graph.session.operation_fuel_remaining().unwrap();
        let root = graph.form(&parent, 0).unwrap();
        eprintln!("new parent of interned child fuel: {}", before - graph.session.operation_fuel_remaining().unwrap());
        assert!(matches!(&graph.recipes[root], Some(Recipe::ReaderVector(values)) if values == &[child_value]));
        assert!(matches!(&parent.kind, Kind::Vector(values) if values.len() == 1 && values[0].kind == child.kind));
        drop(operation);
        graph.session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        let value = graph.materialize(root).unwrap();
        graph.session.collect().unwrap();
        let actual = bridge.read(graph.session, &value, 0..0).unwrap();
        assert!(matches!(&actual.kind, Kind::Vector(values) if values.len() == 1 && values[0].kind == child.kind));
    }
    #[test]
    fn interned_large_literal_import_fits_finite_fuel_without_discarded_copy() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let form = Form {span:0..0,metadata:vec![],kind:Kind::String(vec![120;65536])};
        let first_operation = session.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let first = graph.form(&form,0).unwrap();
        drop(first_operation);
        graph.session.set_operation_fuel(1_000_000);
        let _operation = graph.session.operation_budget_scope().unwrap();
        let before = graph.session.operation_fuel_remaining().unwrap();
        let repeated = graph.form(&form,0);
        eprintln!("interned literal fuel spent: {}", before - graph.session.operation_fuel_remaining().unwrap());
        assert_eq!(repeated.unwrap(),first);
    }
    #[test]
    fn bottom_up_scalar_and_reader_forms_share_recipe_keys() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let _request = session.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        for (literal, kind) in [
            (Literal::Nil, Kind::Nil),
            (Literal::Bool(true), Kind::Bool(true)),
            (Literal::Number(-0.0), Kind::Number(-0.0)),
            (Literal::Number(f64::from_bits(0x7ff8_0000_0000_0001)), Kind::Number(f64::from_bits(0x7ff8_0000_0000_0001))),
            (Literal::String(vec![97, 0xd800]), Kind::String(vec![97, 0xd800])),
        ] {
            let scalar = graph.scalar(literal).unwrap();
            let form = Form { span: 100..200, metadata: vec![], kind };
            let count = graph.recipes.len();
            assert_eq!(scalar, graph.form(&form, 0).unwrap());
            assert_eq!(graph.recipes.len(), count);
        }
        let form = Form { span: 8..19, metadata: vec![], kind: Kind::String(vec![0xdc00, 98]) };
        let recipe = graph.form(&form, 0).unwrap();
        let count = graph.recipes.len();
        assert_eq!(recipe, graph.scalar(Literal::String(vec![0xdc00, 98])).unwrap());
        assert_eq!(graph.recipes.len(), count);
        let plus_zero = graph.scalar(Literal::Number(0.0)).unwrap();
        let minus_zero = graph.scalar(Literal::Number(-0.0)).unwrap();
        assert_ne!(plus_zero, minus_zero);
    }

    #[test]
    fn bottom_up_keys_preserve_metadata_set_repetition_and_quote_alias_barrier() {
        let mut session = Session::new_macro().unwrap();
        session.enter_namespace("cljs.core").unwrap();
        session.eval("(def raw-key-hash-calls 0) (def raw-key-original-hash suss.core/hash) (def hash (fn [key] (do (set! raw-key-hash-calls (+ raw-key-hash-calls 1)) (raw-key-original-hash key))))").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let value = {
            let _request = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            let form = suss_reader::forms::read_forms(
                "^{:nested #{101 102 103 104 105 106 107 108 109}} #{1 2 3 4 5 6 7 8 9}"
            ).unwrap().remove(0);
            let occurrence = graph.form(&form, 0).unwrap();
            assert_eq!(occurrence, graph.form(&form.clone(), 0).unwrap());
            assert!(matches!(graph.recipes[occurrence].as_ref(), Some(Recipe::Metadata { shape: MetadataShape::Set, .. })));
            // Same explicit sharing barrier as ast_record's quoted datum.
            let alias = graph.recipe(Recipe::Alias(occurrence)).unwrap();
            let root = graph.vector(&[occurrence, alias, alias]).unwrap();
            let value = graph.materialize(root).unwrap();
            assert!(matches!(graph.recipes[alias].as_ref(), Some(Recipe::Alias(target)) if *target == occurrence));
            value
        };
        session.collect().unwrap();
        let count = session.eval("raw-key-hash-calls").unwrap();
        // Direct occurrence and first alias each construct two nine-item sets;
        // the second alias reuses its result. Key interning executes no hashes.
        assert_eq!(bridge.read(&mut session, &count, 0..1).unwrap().kind, Kind::Number(36.0));
        let probe = session.eval("(fn [v] [(identical? (nth v 0) (nth v 1)) (identical? (nth v 1) (nth v 2))])").unwrap();
        let result = session.invoke(&probe, &[&value]).unwrap();
        let result = bridge.read(&mut session, &result, 0..1).unwrap();
        let Kind::Vector(items) = &result.kind else { panic!("identity projection") };
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, Kind::Bool(false));
        assert_eq!(items[1].kind, Kind::Bool(true));
    }

    #[test]
    fn reader_data_identity_preserves_number_bits_metadata_and_order() {
        assert_ne!(
            form_key(&number(0)).unwrap(),
            form_key(&number((-0.0f64).to_bits())).unwrap()
        );
        assert_ne!(
            form_key(&number(0x7ff8_0000_0000_0001)).unwrap(),
            form_key(&number(0x7ff8_0000_0000_0002)).unwrap()
        );
        let forms = suss_reader::forms::read_forms_with_source_metadata(
            "(a 1)\n(a 1)",
            Some("located.sus"),
        )
        .unwrap();
        assert_ne!(form_key(&forms[0]).unwrap(), form_key(&forms[1]).unwrap());
        let mut moved = number(42f64.to_bits());
        moved.span = 10..20;
        assert_eq!(
            form_key(&moved).unwrap(),
            form_key(&number(42f64.to_bits())).unwrap()
        );
        let forms = suss_reader::forms::read_forms("[1 2] [2 1]").unwrap();
        assert_ne!(form_key(&forms[0]).unwrap(), form_key(&forms[1]).unwrap());
        let wide = Form {
            span: 0..0,
            metadata: vec![],
            kind: Kind::Vector(vec![number(0); 4096]),
        };
        let mut wide_work = 0u64;
        validate_form_data(&wide, &mut |units| {
            wide_work += units;
            Ok(())
        })
        .unwrap();
        assert!(wide_work > 4096, "validation must meter every form node");
        let tagged = suss_reader::forms::read_forms("^:private x")
            .unwrap()
            .remove(0);
        let normalized_wide = Form {
            span: 0..0,
            metadata: vec![],
            kind: Kind::Vector(vec![tagged; 1024]),
        };
        let mut normalized_work = 0u64;
        validate_form_data(&normalized_wide, &mut |units| {
            normalized_work += units;
            Ok(())
        })
        .unwrap();
        assert!(
            normalized_work > 4096,
            "normalized metadata nodes must be metered"
        );
    }
    #[test]
    fn shared_reader_data_matches_existing_canonical_transport_after_gc() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let source = r#"^{:custom true} (a ^int x [1 -0.0 "a\ud800"])
            ^{:custom 42} #{:a :b}
            ^{:custom true} {:a 1 :b 2 :c 3 :d 4 :e 5 :f 6 :g 7 :h 8 :i 9}
            ^{:custom true} [] ^{:custom true} ()"#;
        let mut forms =
            suss_reader::forms::read_forms_with_source_metadata(source, Some("data.sus")).unwrap();
        forms.push(number(0x7ff8_0000_0000_0001));
        for form in forms {
            let expected = bridge.quote(&mut session, form.clone()).unwrap();
            let actual = {
                let mut graph = AnalysisGraph::new(&bridge, &mut session);
                let first = graph.form(&form, 0).unwrap();
                assert_eq!(first, graph.form(&form.clone(), 0).unwrap());
                graph.materialize(first).unwrap()
            };
            session.collect().unwrap();
            let expected = bridge.read(&mut session, &expected, 0..1).unwrap();
            let actual = bridge.read(&mut session, &actual, 0..1).unwrap();
            assert_eq!(form_key(&actual).unwrap(), form_key(&expected).unwrap());
        }
    }
    #[test]
    fn metadata_set_preserves_canonical_hash_callback_count_and_order() {
        let mut session = Session::new_macro().unwrap();
        session.enter_namespace("cljs.core").unwrap();
        session.eval("(def review-hash-effects 0) (def review-first-hash nil) (def review-original-hash suss.core/hash) (def hash (fn [key] (do (if (== review-hash-effects 0) (set! review-first-hash key) nil) (set! review-hash-effects (+ review-hash-effects 1)) (review-original-hash key))))").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        for (source, calls, first) in [
            ("^:private #{1 2 3 4 5 6 7 8 9}", 9.0, 1.0),
            (
                "^{:nested #{101 102 103 104 105 106 107 108 109}} #{1 2 3 4 5 6 7 8 9}",
                18.0,
                101.0,
            ),
            (
                "^{:nested #{101 102 103 104 105 106 107 108 109}} #{{201 1 202 2 203 3 204 4 205 5 206 6 207 7 208 8 209 9}}",
                18.0,
                201.0,
            ),
        ] {
            session.eval("(set! review-hash-effects 0)").unwrap();
            let form = suss_reader::forms::read_forms(source).unwrap().remove(0);
            let expected = bridge.quote(&mut session, form.clone()).unwrap();
            let count = session.eval("review-hash-effects").unwrap();
            assert_eq!(
                bridge.read(&mut session, &count, 0..1).unwrap().kind,
                Kind::Number(calls)
            );
            let first_key = session.eval("review-first-hash").unwrap();
            assert_eq!(
                bridge.read(&mut session, &first_key, 0..1).unwrap().kind,
                Kind::Number(first)
            );
            session.eval("(set! review-hash-effects 0)").unwrap();
            let actual = {
                let mut graph = AnalysisGraph::new(&bridge, &mut session);
                let root = graph.form(&form, 0).unwrap();
                graph.materialize(root).unwrap()
            };
            session.collect().unwrap();
            let count = session.eval("review-hash-effects").unwrap();
            assert_eq!(
                bridge.read(&mut session, &count, 0..1).unwrap().kind,
                Kind::Number(calls)
            );
            let first_key = session.eval("review-first-hash").unwrap();
            assert_eq!(
                bridge.read(&mut session, &first_key, 0..1).unwrap().kind,
                Kind::Number(first)
            );
            assert_eq!(
                form_key(&bridge.read(&mut session, &actual, 0..1).unwrap()).unwrap(),
                form_key(&bridge.read(&mut session, &expected, 0..1).unwrap()).unwrap()
            );
        }
    }
    #[test]
    fn duplicate_reader_collections_preserve_effects_and_occurrence_results() {
        let mut session = Session::new_macro().unwrap();
        session.enter_namespace("cljs.core").unwrap();
        session.eval("(def review-hash-effects 0) (def hash (fn [key] (do (set! review-hash-effects (+ review-hash-effects 1)) review-hash-effects)))").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let probe = session
            .eval("(fn [value index] (nth (nth value index) 0))")
            .unwrap();
        fn bitmaps(
            session: &mut Session,
            bridge: &FormBridge,
            probe: &SessionValue,
            value: &SessionValue,
        ) -> Vec<u64> {
            (0..2)
                .map(|index| {
                    let index = bridge
                        .scalar(session, &Literal::Number(index as f64))
                        .unwrap();
                    let set = session.invoke(probe, &[value, &index]).unwrap();
                    let map = session.data_fields(&set).unwrap()[1].clone();
                    let root = session.data_fields(&map).unwrap()[2].clone();
                    let bitmap = session.data_fields(&root).unwrap()[1].clone();
                    let Kind::Number(bitmap) = bridge.read(session, &bitmap, 0..1).unwrap().kind
                    else {
                        panic!("bitmap scalar");
                    };
                    bitmap.to_bits()
                })
                .collect()
        }
        for source in [
            "[[#{1 2 3 4 5 6 7 8 9}] [#{1 2 3 4 5 6 7 8 9}]]",
            "([#{1 2 3 4 5 6 7 8 9}] [#{1 2 3 4 5 6 7 8 9}])",
        ] {
            session.eval("(set! review-hash-effects 0)").unwrap();
            let form = suss_reader::forms::read_forms(source).unwrap().remove(0);
            let expected = bridge.quote(&mut session, form.clone()).unwrap();
            let expected_probe = bitmaps(&mut session, &bridge, &probe, &expected);
            assert_ne!(expected_probe[0], expected_probe[1]);
            session.eval("(set! review-hash-effects 0)").unwrap();
            let actual = {
                let mut graph = AnalysisGraph::new(&bridge, &mut session);
                let root = graph.form(&form, 0).unwrap();
                graph.materialize(root).unwrap()
            };
            session.collect().unwrap();
            let count = session.eval("review-hash-effects").unwrap();
            assert_eq!(
                bridge.read(&mut session, &count, 0..1).unwrap().kind,
                Kind::Number(18.0)
            );
            let actual_probe = bitmaps(&mut session, &bridge, &probe, &actual);
            assert_eq!(actual_probe, expected_probe);
        }
    }
    #[test]
    fn compact_diamond_admission_rejects_before_reader_callbacks() {
        let mut session = Session::new_macro().unwrap();
        session.enter_namespace("cljs.core").unwrap();
        session.eval("(def review-diamond-effects 0) (def hash (fn [key] (do (set! review-diamond-effects (+ review-diamond-effects 1)) key)))").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let form = suss_reader::forms::read_forms("#{1 2 3 4 5 6 7 8 9}").unwrap().remove(0);
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let leaf = graph.form(&form, 0).unwrap();
        let mut root = leaf;
        for _ in 0..16 {
            root = graph.recipe(Recipe::ReaderVector(vec![root, root])).unwrap();
        }
        // Fewer than 40 distinct recipes; validation and compact counting cost
        // far less than 50k. Their expanded frame work alone exceeds 300k.
        assert!(graph.recipes.len() < 40);
        graph.session.set_operation_fuel(50_000);
        let request = graph.session.operation_budget_scope().unwrap();
        let error = graph.materialize(root).unwrap_err();
        assert!(matches!(error, SessionError::Trap(ref error)
            if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
        assert_eq!(graph.session.operation_fuel_remaining().unwrap(), 0);
        drop(request);
        drop(graph);
        // The same allowance admits a genuinely small independent operation.
        let effects = session.eval("review-diamond-effects").unwrap();
        assert_eq!(bridge.read(&mut session, &effects, 0..1).unwrap().kind, Kind::Number(0.0));
        let small = {
            let request = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            let root = graph.scalar(Literal::Number(42.0)).unwrap();
            let result = graph.materialize(root).unwrap();
            drop(request);
            result
        };
        assert_eq!(bridge.read(&mut session, &small, 0..1).unwrap().kind, Kind::Number(42.0));
    }
    #[test]
    fn reader_occurrence_work_bounds_fail_before_callbacks_and_allow_recovery() {
        let mut session = Session::new_macro().unwrap();
        session.enter_namespace("cljs.core").unwrap();
        session.eval("(def review-hash-effects 0) (def hash (fn [key] (do (set! review-hash-effects (+ review-hash-effects 1)) key)))").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let form = suss_reader::forms::read_forms("[#{1 2 3 4 5 6 7 8 9} #{1 2 3 4 5 6 7 8 9}]")
            .unwrap()
            .remove(0);
        let error = {
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            let root = graph.form(&form, 0).unwrap();
            graph.session.set_operation_fuel(1);
            graph.materialize(root).unwrap_err()
        };
        assert!(error.to_string().contains("fuel") || error.to_string().contains("Fuel"));
        session.set_operation_fuel(10_000_000);
        let count = session.eval("review-hash-effects").unwrap();
        assert_eq!(
            bridge.read(&mut session, &count, 0..1).unwrap().kind,
            Kind::Number(0.0)
        );
        let value = {
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            let root = graph.form(&form, 0).unwrap();
            graph.materialize(root).unwrap()
        };
        session.collect().unwrap();
        let count = session.eval("review-hash-effects").unwrap();
        assert_eq!(
            bridge.read(&mut session, &count, 0..1).unwrap().kind,
            Kind::Number(18.0)
        );
        assert!(matches!(
            bridge.read(&mut session, &value, 0..1).unwrap().kind,
            Kind::Vector(_)
        ));
    }
}

#[cfg(test)]
#[path = "portable_macro_graph/body_origin_tests.rs"]
mod body_origin_tests;
