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
    sync::{Arc, Weak},
};
use suss_reader::forms::{Form, Kind};

type Value = usize;
type Result<T> = std::result::Result<T, SessionError>;
type DeclarationKey = (portable::resolve::Global, usize);

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

/// Same-Store declaration metadata roots, owned by one compiled macro session.
/// Weak source owners prevent dead revisions from accumulating or address reuse
/// from mistaking a new declaration for an old one. Never an artifact cache.
#[derive(Default)]
pub(crate) struct DeclarationValues {
    metadata: BTreeMap<DeclarationKey, (Weak<portable::resolve::DefinitionInfo>, SessionValue)>,
}
impl DeclarationValues {
    fn prune(&mut self) {
        self.metadata
            .retain(|_, (source, _)| source.strong_count() != 0);
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
            let pairs = hir::reader_metadata_pairs_metered(form, &mut |work| {
                charge(work).map_err(|error| portable::Diagnostic {
                    span: form.span.clone(), message: error.to_string(),
                })
            }).map_err(SessionError::Compile)?;
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
fn form_key(form: &Form) -> Result<Vec<u8>> {
    form_key_metered(form, &mut |_| Ok(()))
}

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
    Local(LocalBinding),
    Field(FieldBinding),
    Scope(Arc<FunctionScope>),
    Callable(Arc<hir::SourceCallable>),
    Ast(Hir),
    Lowering(Hir),
}

/// Memo tables own their source identities as well as the resulting GC roots.
/// Graph memo tables live for one build. Only final declaration metadata roots
/// may outlive it in the owning macro Store; no pointer is an artifact cache key.
pub struct AnalysisGraph<'a> {
    bridge: &'a FormBridge,
    session: &'a mut Session,
    retained: Option<&'a mut DeclarationValues>,
    metadata_roots: Vec<(
        Value,
        DeclarationKey,
        Arc<portable::resolve::DefinitionInfo>,
    )>,
    keys: BTreeMap<String, Value>,
    symbols: BTreeMap<String, Value>,
    forms: BTreeMap<Vec<u8>, Value>,
    maps: BTreeMap<Vec<(Value, Value)>, Value>,
    vectors: BTreeMap<Vec<Value>, Value>,
    declarations: BTreeMap<
        (portable::resolve::Global, usize),
        (Option<Arc<portable::resolve::DefinitionInfo>>, Value),
    >,
    locals: BTreeMap<(usize, usize, String), (LocalBinding, Value)>,
    fields: BTreeMap<usize, (FieldBinding, Value)>,
    scopes: BTreeMap<usize, (Arc<FunctionScope>, Value)>,
    callables: BTreeMap<usize, (Arc<hir::SourceCallable>, Value)>,
    namespaces: BTreeMap<usize, (Arc<SourceNamespace>, Value)>,
    namespace_contents: BTreeMap<NamespaceKey, Value>,
    asts: BTreeMap<usize, (Arc<hir::SourceAnalysis>, Value)>,
    recipes: Vec<Option<Recipe>>,
    jobs: VecDeque<(Value, Task)>,
}
impl<'a> AnalysisGraph<'a> {
    fn reader_key(&mut self, form: &Form) -> Result<Vec<u8>> {
        form_key_metered(form, &mut |units| self.session.charge_host_work(units))
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
            session,
            retained: None,
            metadata_roots: vec![],
            keys: BTreeMap::new(),
            symbols: BTreeMap::new(),
            forms: BTreeMap::new(),
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
            jobs: VecDeque::new(),
        }
    }
    pub(crate) fn with_declaration_values(
        bridge: &'a FormBridge,
        session: &'a mut Session,
        retained: &'a mut DeclarationValues,
    ) -> Self {
        retained.prune();
        let mut graph = Self::new(bridge, session);
        graph.retained = Some(retained);
        graph
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
    fn materialize(&mut self, root: Value) -> Result<SessionValue> {
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
        self.session.charge_host_work(self.recipes.len().saturating_mul(3) as u64)?;
        // Check the compact DAG once. Compiler record recipes intentionally
        // share immutable snapshots. Reader maps/sets can invoke captured user
        // callbacks: each occurrence and its reader ancestors must execute.
        let mut state = vec![0u8; self.recipes.len()];
        let mut repeat = vec![false; self.recipes.len()];
        let mut stack = vec![(root, false)];
        while let Some((id, finish)) = stack.pop() {
            if state[id] == 2 {
                // Its incoming edge was already charged when queued. A shared
                // node that is fully checked needs no second validation visit;
                // reader occurrences retain their separate cold work budget.
                continue;
            }
            self.session.charge_host_work(1)?;
            let recipe = self.recipes[id].as_ref().expect("all graph tasks filled");
            if !finish {
                if state[id] == 1 {
                    return Err(SessionError::Host(wasmtime::Error::msg(
                        "Cyclic compiler analysis graph",
                    )));
                }
                state[id] = 1;
                stack.push((id, true));
                self.session.charge_host_work(dependency_count(recipe) as u64)?;
                let dependencies = dependencies(recipe);
                for dependency in dependencies.into_iter().rev() {
                    stack.push((dependency, false));
                }
            } else {
                let dependencies = dependencies(recipe);
                self.session.charge_host_work(dependencies.len() as u64)?;
                repeat[id] = match recipe {
                    Recipe::ReaderMap(_) | Recipe::ReaderSet(_) => true,
                    Recipe::List(_) | Recipe::ReaderVector(_) | Recipe::Metadata { .. } => {
                        matches!(
                            recipe,
                            Recipe::Metadata {
                                shape: MetadataShape::Set,
                                ..
                            }
                        ) || dependencies.iter().any(|id| repeat[*id])
                    }
                    // Immutable compiler record/namespace identities are sharing
                    // barriers, distinct from occurrences inside reader data.
                    _ => false,
                };
                state[id] = 2;
            }
        }
        struct Frame {
            id: Value,
            dependencies: Vec<Value>,
            next: usize,
            inputs: Vec<SessionValue>,
        }
        impl Frame {
            fn new(id: Value, recipe: &Recipe) -> Self {
                Self {
                    id,
                    dependencies: dependencies(recipe),
                    next: 0,
                    inputs: vec![],
                }
            }
        }
        // Validate the same logical work as a cold graph before callbacks or
        // retained-root reuse. Persistence must not turn an oversized graph into
        // acceptable input; compiler sharing within this build still applies.
        let mut logical_cached = vec![false; self.recipes.len()];
        let root_recipe = self.recipes[root].as_ref().unwrap();
        self.session
            .charge_host_work(dependency_count(root_recipe) as u64)?;
        let mut planned = vec![Frame::new(root, root_recipe)];
        while let Some(frame) = planned.last_mut() {
            if frame.next < frame.dependencies.len() {
                self.charge(1)?;
                let id = frame.dependencies[frame.next];
                frame.next += 1;
                if repeat[id] || !logical_cached[id] {
                    let recipe = self.recipes[id].as_ref().unwrap();
                    self.session
                        .charge_host_work(dependency_count(recipe) as u64)?;
                    planned.push(Frame::new(id, recipe));
                }
            } else {
                self.charge(1)?;
                let frame = planned.pop().unwrap();
                if !repeat[frame.id] {
                    logical_cached[frame.id] = true;
                }
            }
        }
        let mut cached: Vec<Option<SessionValue>> = vec![None; self.recipes.len()];
        if let Some(retained) = &self.retained {
            for (id, key, source) in &self.metadata_roots {
                if let Some((owner, value)) = retained.metadata.get(key)
                    && owner
                        .upgrade()
                        .is_some_and(|owner| Arc::ptr_eq(&owner, source))
                {
                    cached[*id] = Some(value.clone());
                }
            }
        }
        let root_recipe = self.recipes[root].as_ref().unwrap();
        self.session
            .charge_host_work(dependency_count(root_recipe) as u64)?;
        let mut frames = vec![Frame::new(root, root_recipe)];
        // DAG storage remains bounded independently of occurrence execution.
        // Charge before descending or invoking callbacks; diamonds cannot expand
        // into unbounded work even when the distinct recipe count is small.
        while !frames.is_empty() {
            let frame = frames.last_mut().unwrap();
            if frame.next < frame.dependencies.len() {
                self.charge(1)?;
                let id = frame.dependencies[frame.next];
                frame.next += 1;
                if !repeat[id]
                    && let Some(value) = &cached[id]
                {
                    frame.inputs.push(value.clone());
                } else {
                    let recipe = self.recipes[id].as_ref().unwrap();
                    self.session
                        .charge_host_work(dependency_count(recipe) as u64)?;
                    frames.push(Frame::new(id, recipe));
                }
                continue;
            }
            let frame = frames.pop().unwrap();
            self.charge(1)?;
            let recipe = self.recipes[frame.id].as_ref().unwrap();
            let inputs = frame.inputs;
            let value = match recipe {
                Recipe::Scalar(value) => self.bridge.scalar(self.session, value)?,
                Recipe::Identifier {
                    namespace,
                    name,
                    keyword,
                } => self.bridge.identifier(
                    self.session,
                    namespace.as_deref(),
                    name,
                    *keyword,
                    None,
                )?,
                Recipe::Form(form) => {
                    let form = crate::portable::clone_form_metered(form, &mut |units| {
                        charge_origin_work(self.session, form.span.clone(), units)
                    })
                    .map_err(SessionError::Compile)?;
                    self.bridge.quote(self.session, form)?
                }
                Recipe::Alias(_) => inputs[0].clone(),
                Recipe::List(_) => self.bridge.list_values(self.session, &inputs)?,
                Recipe::Vector(_) | Recipe::ReaderVector(_) => {
                    self.bridge.vector_values(self.session, &inputs)?
                }
                Recipe::Set(_) | Recipe::ReaderSet(_) => {
                    self.bridge.set_values(self.session, &inputs)?
                }
                Recipe::Map(_) | Recipe::ReaderMap(_) => self.bridge.map_values(
                    self.session,
                    &inputs
                        .chunks_exact(2)
                        .map(|pair| (pair[0].clone(), pair[1].clone()))
                        .collect::<Vec<_>>(),
                )?,
                Recipe::Metadata {
                    shape, set_items, ..
                } => self.bridge.with_form_metadata(
                    self.session,
                    *shape,
                    &inputs[0],
                    inputs.last().unwrap(),
                    &inputs[1..1 + set_items.len()],
                )?,
            };
            if !repeat[frame.id] {
                cached[frame.id] = Some(value.clone());
            }
            if let Some(parent) = frames.last_mut() {
                parent.inputs.push(value);
            } else {
                if let Some(retained) = &mut self.retained {
                    for (id, key, source) in &self.metadata_roots {
                        if let Some(value) = &cached[*id] {
                            retained
                                .metadata
                                .insert(key.clone(), (Arc::downgrade(source), value.clone()));
                        }
                    }
                }
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
            Literal::String(value) => Kind::String(value.clone()),
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
        if let Some(value) = self.forms.get(&key) {
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
    fn keyword(&mut self, name: &str) -> Result<Value> {
        self.charge(name.len().saturating_add(1))?;
        if let Some(value) = self.keys.get(name) {
            return Ok(value.clone());
        }
        self.charge(name.encode_utf16().count() * 2)?;
        let (namespace, name_part) = name
            .split_once('/')
            .map_or((None, name), |(ns, name)| (Some(ns), name));
        let value = self.recipe(Recipe::Identifier {
            namespace: namespace.map(str::to_owned),
            name: name_part.into(),
            keyword: true,
        })?;
        self.keys.insert(name.into(), value.clone());
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
        })?;
        self.symbols.insert(name.into(), value);
        Ok(value)
    }
    fn map(&mut self, entries: Vec<(&str, Value)>) -> Result<Value> {
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
    fn map_in_task_slot(&mut self, slot: Value, entries: Vec<(&str, Value)>) -> Result<Value> {
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
    fn store_form_key(&mut self, key: Vec<u8>, value: Value) -> Result<()> {
        self.charge(key.len().saturating_add(1))?;
        self.forms.insert(key, value);
        Ok(())
    }
    fn form_data(&mut self, form: &Form, _reader_depth: usize) -> Result<Value> {
        #[derive(Clone, Copy)]
        enum Shape {
            List,
            Vector,
            Set,
            Map,
            Form,
        }
        enum Work {
            Visit(Form),
            Finish {
                key: Vec<u8>,
                shape: Shape,
                child_count: usize,
                metadata_shape: Option<MetadataShape>,
                set_item_count: usize,
                base_value: Option<Value>,
            },
        }

        let root = crate::portable::clone_form_metered(form, &mut |units| {
            charge_origin_work(self.session, form.span.clone(), units)
        })
        .map_err(SessionError::Compile)?;
        let mut work = vec![Work::Visit(root)];
        let mut values = Vec::<Value>::new();
        while let Some(item) = work.pop() {
            match item {
                Work::Visit(mut form) => {
                    let key = self.reader_key(&form)?;
                    if let Some(value) = self.forms.get(&key).copied() {
                        self.charge(1)?;
                        values.push(value);
                        continue;
                    }
                    let scalar = match &form.kind {
                        Kind::Nil => Some(Literal::Nil),
                        Kind::Bool(value) => Some(Literal::Bool(*value)),
                        Kind::Number(value) => Some(Literal::Number(*value)),
                        Kind::String(value) => Some(Literal::String(value.clone())),
                        _ => None,
                    };
                    if let Some(scalar) = scalar {
                        if !form.metadata.is_empty() {
                            return Err(SessionError::Host(wasmtime::Error::msg(
                                "Metadata requires a symbol or collection",
                            )));
                        }
                        values.push(self.scalar(scalar)?);
                        continue;
                    }
                    let metadata_shape = if form.metadata.is_empty() {
                        None
                    } else {
                        Some(match &form.kind {
                            Kind::Symbol(_) => MetadataShape::Symbol,
                            Kind::List(items) => MetadataShape::List {
                                empty: items.is_empty(),
                            },
                            Kind::Vector(_) => MetadataShape::Vector,
                            Kind::Map(items) => MetadataShape::Map {
                                large: items.len() > 16,
                            },
                            Kind::Set(_) => MetadataShape::Set,
                            _ => {
                                return Err(SessionError::Host(wasmtime::Error::msg(
                                    "Metadata requires a symbol or collection",
                                )));
                            }
                        })
                    };
                    let metadata_form = if metadata_shape.is_some() {
                        let pairs =
                            crate::portable_macro_data::normalized_reader_metadata(self.session, &form)?;
                        Some(Form {
                            span: form.span.clone(),
                            metadata: vec![],
                            kind: Kind::Map(pairs),
                        })
                    } else {
                        None
                    };
                    let mut children = Vec::new();
                    let mut base_value = None;
                    let (shape, set_item_count) = match form.take_kind() {
                        Kind::List(items) => {
                            children = items;
                            (Shape::List, 0)
                        }
                        Kind::Vector(items) => {
                            children = items;
                            (Shape::Vector, 0)
                        }
                        Kind::Set(items) => {
                            let count = items.len();
                            children = items;
                            (Shape::Set, count)
                        }
                        Kind::Map(items) => {
                            if items.len() % 2 != 0 {
                                return Err(SessionError::Host(wasmtime::Error::msg(
                                    "Macro form map requires paired entries",
                                )));
                            }
                            children = items;
                            (Shape::Map, 0)
                        }
                        Kind::Conditional(_)
                        | Kind::SplicingConditional(_)
                        | Kind::Discard(_)
                        | Kind::Prefix { .. } => {
                            return Err(SessionError::Host(wasmtime::Error::msg(
                                "Native macro data requires resolved reader prefixes",
                            )));
                        }
                        kind => {
                            let data = Form {
                                span: form.span.clone(),
                                metadata: vec![],
                                kind,
                            };
                            let value = self.recipe(Recipe::Form(data))?;
                            if metadata_shape.is_none() {
                                self.store_form_key(key, value)?;
                                values.push(value);
                                continue;
                            }
                            base_value = Some(value);
                            (Shape::Form, 0)
                        }
                    };
                    if let Some(metadata_form) = metadata_form {
                        children.push(metadata_form);
                    }
                    let set_item_count = if matches!(shape, Shape::Set) {
                        set_item_count
                    } else {
                        0
                    };
                    work.push(Work::Finish {
                        key,
                        shape,
                        child_count: children.len(),
                        metadata_shape,
                        set_item_count,
                        base_value,
                    });
                    for child in children.into_iter().rev() {
                        work.push(Work::Visit(child));
                    }
                }
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
                }
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
        locals: &HashMap<String, LocalBinding>,
        fields: &HashMap<String, FieldBinding>,
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
        locals: &HashMap<String, LocalBinding>,
        fields: &HashMap<String, FieldBinding>,
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
        let locals = self.map_values(entries)?;
        let scopes = scopes
            .iter()
            .map(|scope| self.scope(scope, depth + 1))
            .collect::<Result<Vec<_>>>()?;
        let scopes = self.vector(&scopes)?;
        self.map(vec![
            ("context", context),
            ("ns", namespace),
            ("suss/catalog", catalog),
            ("locals", locals),
            ("fn-scope", scopes),
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
        self.charge(fact_count.saturating_add(namespace.namespace.len()).saturating_add(1))?;
        let key = Arc::as_ptr(namespace) as usize;
        self.charge(1)?;
        if let Some((_, value)) = self.namespaces.get(&key) {
            return Ok(value.clone());
        }
        let content_key = NamespaceKey::new(namespace);
        self.charge(fact_count.saturating_add(1))?;
        if let Some(value) = self.namespace_contents.get(&content_key) {
            let value = *value;
            // The content key owns a snapshot, keeping declaration Arc addresses
            // live for this graph's entire lifetime without cloning its maps.
            self.namespaces.insert(key, (namespace.clone(), value));
            return Ok(value);
        }
        let value = self.task(Task::Namespace(namespace.clone()))?;
        self.namespaces.insert(key, (namespace.clone(), value));
        self.namespace_contents.insert(content_key, value);
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
        self.map_in_task_slot(
            slot,
            vec![
                ("name", name),
                ("requires", aliases),
                ("suss/refers", refers),
                ("uses", uses),
                ("renames", renames),
                ("require-macros", macro_aliases),
                ("use-macros", macro_uses),
                ("rename-macros", macro_renames),
                ("excludes", exclusions),
                ("defs", declarations),
            ],
        )
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
        let name = self.symbol(&format!("{}/{}", global.namespace(), global.name()))?;
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
            let mut declaration = if let Some(origin) = &info.origin {
                origin
                    .macro_form_data_metered(&info.declaration, &mut |units| {
                        charge_origin_work(self.session, info.declaration.span.clone(), units)
                    })
                    .map_err(SessionError::Compile)?
            } else {
                self.clone_form(&info.declaration)?
            };
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
                        declaration.metadata = std::mem::take(&mut original.metadata);
                        for metadata in &info.declaration.metadata {
                            declaration.metadata.push(self.clone_form(metadata)?);
                        }
                    }
                }
            }
            let metadata =
                crate::portable_macro_data::normalized_reader_metadata(self.session, &declaration)?;
            let property = |name: &str| {
                metadata.chunks_exact(2).find_map(|pair| {
                    matches!(&pair[0].kind, Kind::Keyword(key)
                    if key.namespace.is_none() && key.name == name)
                    .then_some(&pair[1])
                })
            };
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
                let mut meta = Form {
                    span: 0..0,
                    metadata: vec![],
                    kind: Kind::Map(metadata.chunks_exact(2).filter(|pair| !matches!(&pair[0].kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == "test")).flat_map(|pair| pair.iter().cloned()).collect()),
                };
                if global.namespace() == "suss.core" {
                    let Kind::Map(items) = &mut meta.kind else {
                        unreachable!()
                    };
                    let mut pairs = items.chunks_exact(2)
                        .filter(|pair| !matches!(&pair[0].kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == "file"))
                        .flat_map(|pair| pair.iter().cloned()).collect::<Vec<_>>();
                    pairs.extend([
                        Form {
                            span: 0..0,
                            metadata: vec![],
                            kind: Kind::Keyword(suss_reader::Keyword::new("file")),
                        },
                        Form {
                            span: 0..0,
                            metadata: vec![],
                            kind: Kind::String("cljs/core.cljs".encode_utf16().collect()),
                        },
                    ]);
                    *items = pairs;
                }
                fields.push(("meta", self.form(&meta, depth + 1)?));
            } else if let Some(value) = property("meta") {
                fields.push(("meta", self.form(value, depth + 1)?));
            }
            if let Some(origin) = &info.origin {
                let definition = origin
                    .macro_form_data_metered(&info.definition_form, &mut |units| {
                        charge_origin_work(self.session, info.definition_form.span.clone(), units)
                    })
                    .map_err(SessionError::Compile)?;
                let position =
                    crate::portable_macro_data::normalized_reader_metadata(self.session, &definition)?;
                for field in ["line", "column", "file"] {
                    if let Some(value) = position.chunks_exact(2).find_map(|pair| {
                        matches!(&pair[0].kind, Kind::Keyword(key)
                            if key.namespace.is_none() && key.name == field)
                        .then_some(&pair[1])
                    }) {
                        fields.push((field, self.form(value, depth + 1)?));
                    }
                }
            }
            if global.namespace() == "suss.core" {
                fields.retain(|(name, _)| *name != "file");
                fields.push((
                    "file",
                    self.scalar(Literal::String("cljs/core.cljs".encode_utf16().collect()))?,
                ));
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
                    source.and_then(|source| source.tags.inferred.clone())
                }
            } else {
                hir::declaration_tag(info)
                    .map_err(|error| SessionError::Host(wasmtime::Error::msg(error.message)))?
            };
            fields.push((
                "suss/definition-form",
                self.form(&info.definition_form, depth + 1)?,
            ));
            fields.push((
                "suss/analysis-completed",
                self.flag(info.analysis_completed)?,
            ));
            fields.push(("suss/declaration", self.form(&info.declaration, depth + 1)?));
            fields.push(("suss/defonce", self.flag(info.once)?));
            if let Some(form) = &info.initializer_form {
                fields.push(("suss/initializer-form", self.form(form, depth + 1)?));
            }
            fields.push((
                "suss/initializer-recorded",
                self.flag(info.initializer.is_some())?,
            ));
            if !provisional && let Some(doc) = &info.docstring {
                fields.retain(|(name, _)| *name != "doc");
                fields.push(("doc", self.scalar(Literal::String(doc.clone()))?));
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
                    fields.push(("fn-var", self.flag(!macro_flag)?));
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
                            fields.push((field, self.form(form, depth + 1)?));
                        }
                    }
                } else {
                    // Normal callable defaults replace the selected raw group.
                    // Empty top-fn instead leaves its five raw arity fields.
                    fields.retain(|(name, _)| !FUNCTION_FIELDS.contains(name));
                    let macro_flag = property("macro")
                        .is_some_and(|value| !matches!(value.kind, Kind::Nil | Kind::Bool(false)));
                    fields.push(("fn-var", self.flag(!macro_flag)?));
                    fields.push(("variadic?", self.flag(callable.variadic())?));
                    let fixed = callable.max_fixed_arity().ok_or_else(|| {
                        SessionError::Host(wasmtime::Error::msg(
                            "Invalid source function declaration",
                        ))
                    })?;
                    fields.push(("max-fixed-arity", self.number(fixed)?));
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
                    fields.push(("method-params", self.recipe(Recipe::List(methods))?));
                    // The pinned analyzer retains the reader value of :arglists,
                    // including its quote, and maps meta over that value's elements.
                    let arglists = property("arglists");
                    let argument_data = if let Some(form) = arglists {
                        self.form(form, depth + 1)?
                    } else {
                        self.scalar(Literal::Nil)?
                    };
                    fields.push(("arglists", argument_data));
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
                    fields.push((
                        "arglists-meta",
                        self.recipe(Recipe::List(argument_metadata))?,
                    ));
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
                fields.push(("declared", self.flag(true)?));
                fields.push(("fn-var", self.flag(true)?));
                fields.push(("method-params", self.form(&parameters, depth + 1)?));
            }
            if let Some(tag) = completed_tag {
                let field = if callable.is_some() { "ret-tag" } else { "tag" };
                fields.retain(|(name, _)| *name != field);
                fields.push((field, self.form(&tag, depth + 1)?));
            }
        }
        // Share the final metadata value of this captured declaration revision,
        // including a top-fn overlay. Reader occurrences remain independently
        // constructed elsewhere; resolved var copies reuse this catalog value.
        if let Some((_, meta)) = fields.iter_mut().find(|(name, _)| *name == "meta") {
            self.charge(0)?;
            *meta = self.recipe(Recipe::Alias(*meta))?;
            if let Some(info) = info {
                self.metadata_roots.push((*meta, key.clone(), info.clone()));
            }
        }
        let value = self.map(fields)?;
        self.declarations.insert(key, (info.cloned(), value));
        Ok(value)
    }
    fn local(&mut self, binding: &LocalBinding, depth: usize) -> Result<Value> {
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
        let key = (Arc::as_ptr(&binding.identity) as usize, binding.id.0, role);
        self.charge(key.2.len().saturating_add(1))?;
        if let Some((_, value)) = self.locals.get(&key) {
            return Ok(value.clone());
        }
        let value = self.task(Task::Local(binding.clone()))?;
        self.locals.insert(key, (binding.clone(), value));
        Ok(value)
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
        let mut entries = vec![
            ("name", name.clone()),
            ("form", name),
            ("local", kind),
            ("suss/id", id),
        ];
        if let Some(tag) = hir::local_tag(binding)
            .map_err(|error| SessionError::Host(wasmtime::Error::msg(error.message)))?
        {
            entries.push(("tag", self.form(&tag, depth + 1)?));
        } else if binding.initializer.is_some()
            || matches!(binding.source_kind(), hir::LocalKind::Argument { .. })
        {
            entries.push(("tag", self.scalar(Literal::Nil)?));
        }
        if !matches!(binding.source_role, SourceRole::PrivateCatch { .. }) {
            entries.push(("op", self.keyword("binding")?));
            entries.push(("binding-form?", self.flag(true)?));
        }
        let position = binding
            .origin
            .as_ref()
            .map(|origin| symbol_position(self.session, origin, &binding.declaration))
            .transpose()?
            .flatten();
        if let Some(position) = position {
            entries.push(("line", self.number(position.line)?));
            entries.push(("column", self.number(position.column)?));
        }
        if let hir::LocalKind::Argument { index, rest } = binding.source_kind() {
            entries.push(("arg-id", self.number(index)?));
            entries.push(("suss/rest-parameter", self.flag(rest)?));
        }
        if let SourceRole::FunctionName { variadic, methods } = &binding.source_role {
            entries.push(("suss/variadic", self.flag(*variadic)?));
            if let Some(methods) = methods {
                entries.push(("fn-var", self.flag(true)?));
                entries.push(("variadic?", self.flag(*variadic)?));
                entries.push(("max-fixed-arity", self.number(methods.max_fixed_arity())?));
                let mut parameters = Vec::new();
                for method in &methods.methods {
                    let bindings = method
                        .parameters
                        .iter()
                        .map(|parameter| self.local(parameter, depth + 1))
                        .collect::<Result<Vec<_>>>()?;
                    parameters.push(self.vector(&bindings)?);
                }
                entries.push(("method-params", self.recipe(Recipe::List(parameters))?));
            }
        }
        if let Some(initializer) = &binding.initializer {
            entries.push(("init", self.ast(initializer, depth + 1)?));
            let init = self.keyword("init")?;
            entries.push(("children", self.vector(&[init])?));
        }
        if let Some(shadow) = &binding.shadow {
            entries.push(("shadow", self.local(shadow, depth + 1)?));
        } else if let Some(field) = &binding.shadow_field {
            entries.push(("shadow", self.field(field, depth + 1)?));
        }
        if matches!(binding.source_kind(), hir::LocalKind::Argument { .. }) {
            let shadow = if let Some((_, shadow)) = entries.iter().find(|(key, _)| *key == "shadow")
            {
                *shadow
            } else {
                let nil = self.scalar(Literal::Nil)?;
                entries.push(("shadow", nil));
                nil
            };
            let mut env = vec![("context", self.context(binding.declaration_context)?)];
            for key in ["line", "column"] {
                if let Some((_, value)) = entries.iter().find(|(name, _)| *name == key) {
                    env.push((key, *value));
                }
            }
            let info = self.map(vec![("name", entries[0].1), ("shadow", shadow)])?;
            entries.push(("env", self.map(env)?));
            entries.push(("info", info));
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
            entries.push((
                "suss/type-declaration",
                self.form(type_declaration, depth + 1)?,
            ));
            entries.push(("suss/declaration-namespace", self.symbol(namespace)?));
            entries.push(("suss/protocol-receiver", self.flag(*protocol_receiver)?));
            entries.push(("suss/receiver-access", self.lowering(access, depth + 1)?));
            if let Some(argument) = argument {
                entries.push(("shadow", self.local(argument, depth + 1)?));
            }
        }
        if let SourceRole::CatchBinding { hidden, access } = &binding.source_role {
            entries.push(("suss/private-payload", self.local(hidden, depth + 1)?));
            entries.push(("suss/payload-access", self.lowering(access, depth + 1)?));
        }
        let context = self.context(binding.declaration_context)?;
        entries.push(("suss/declaration-context", context));
        self.map_in_task_slot(slot, entries)
    }
    fn field(&mut self, field: &FieldBinding, depth: usize) -> Result<Value> {
        Self::depth(depth)?;
        let key = Arc::as_ptr(&field.identity) as usize;
        self.charge(1)?;
        if let Some((_, value)) = self.fields.get(&key) {
            return Ok(value.clone());
        }
        let value = self.task(Task::Field(field.clone()))?;
        self.fields.insert(key, (field.clone(), value));
        Ok(value)
    }
    fn field_record(&mut self, slot: Value, field: &FieldBinding, depth: usize) -> Result<Value> {
        let name = self.form(&field.declaration, depth + 1)?;
        let kind = self.keyword("field")?;
        let is_field = self.flag(true)?;
        let index = self.number(field.index)?;
        let access = self.lowering(&field.access, depth + 1)?;
        let mut entries = vec![
            ("name", name),
            ("local", kind),
            ("field", is_field),
            ("suss/index", index),
            ("suss/access", access),
        ];
        // Pinned parse-type retains raw nullable flags, independently of the
        // lowered mutability boolean. Unique source fields have no shadow;
        // duplicate declarations are rejected by the compiler before this path.
        let metadata =
            crate::portable_macro_data::normalized_reader_metadata(self.session, &field.declaration)?;
        for key in [
            "mutable",
            "unsynchronized-mutable",
            "volatile-mutable",
            "tag",
        ] {
            let value = metadata.chunks_exact(2).find_map(|pair| {
                matches!(&pair[0].kind, Kind::Keyword(k) if k.namespace.is_none() && k.name == key)
                    .then_some(&pair[1])
            });
            let value = if let Some(value) = value {
                self.form(value, depth + 1)?
            } else {
                self.scalar(Literal::Nil)?
            };
            entries.push((key, value));
        }
        entries.push(("shadow", self.scalar(Literal::Nil)?));
        let position = field
            .origin
            .as_ref()
            .map(|origin| symbol_position(self.session, origin, &field.declaration))
            .transpose()?
            .flatten();
        if let Some(position) = position {
            entries.push(("line", self.number(position.line)?));
            entries.push(("column", self.number(position.column)?));
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
        let value = self.task(Task::Scope(scope.clone()))?;
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
        let mut info = vec![
            ("fn-self-name", self.flag(true)?),
            ("ns", self.symbol(&scope.namespace)?),
            ("fn-scope", parents),
            ("suss/explicit-self", explicit),
            (
                "suss/function-form",
                self.form(&scope.function_form, depth + 1)?,
            ),
            ("suss/phase", phase),
        ];
        if let Some(shadow) = &scope.shadow {
            info.push(("shadow", self.local(shadow, depth + 1)?));
        } else if let Some(shadow) = &scope.shadow_field {
            info.push(("shadow", self.field(shadow, depth + 1)?));
        } else {
            info.push(("shadow", self.scalar(Literal::Nil)?));
        }
        let info = self.map(info)?;
        let mut entries = vec![
            ("op", self.keyword("binding")?),
            ("form", name),
            ("name", name),
            ("local", kind),
            ("env", env),
            ("info", info),
        ];
        let metadata =
            crate::portable_macro_data::normalized_reader_metadata(self.session, &scope.declaration)?;
        if let Some(tag) = metadata.chunks_exact(2).find_map(|pair| {
            matches!(&pair[0].kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == "tag")
                .then_some(&pair[1])
        }).filter(|tag| !matches!(tag.kind, Kind::Nil)) {
            entries.push(("ret-tag", self.form(tag, depth + 1)?));
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
        let value = self.task(Task::Ast(hir.clone()))?;
        self.asts.insert(key, (source.clone(), value));
        Ok(value)
    }
    fn callable(&mut self, callable: &Arc<hir::SourceCallable>) -> Result<Value> {
        let key = Arc::as_ptr(callable) as usize;
        self.charge(1)?;
        if let Some((_, value)) = self.callables.get(&key) {
            return Ok(*value);
        }
        let value = self.task(Task::Callable(callable.clone()))?;
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
        let mut fields = vec![("form", form), ("env", env), ("suss/lowering", lowering)];
        if source.is_body {
            fields.push(("body?", self.flag(true)?));
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
            fields.push(("op", self.keyword("const")?));
            fields.push(("val", form));
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
                    ("vector", vec![("items", items.iter().collect::<Vec<_>>())])
                }
                hir::SourceNode::Set(items) => {
                    ("set", vec![("items", items.iter().collect::<Vec<_>>())])
                }
                hir::SourceNode::Map(items) => {
                    if items.len() % 2 != 0 {
                        return Err(SessionError::Host(wasmtime::Error::msg(
                            "Unpaired source map children",
                        )));
                    }
                    (
                        "map",
                        vec![
                            ("keys", items.iter().step_by(2).collect::<Vec<_>>()),
                            ("vals", items.iter().skip(1).step_by(2).collect::<Vec<_>>()),
                        ],
                    )
                }
                _ => unreachable!("non-collection handled separately"),
            };
            fields.push(("op", self.keyword(operation)?));
            let mut names = Vec::new();
            for (name, children) in children {
                names.push(self.keyword(name)?);
                let mut records = Vec::new();
                for child in children {
                    records.push(self.ast(child, depth + 1)?);
                }
                fields.push((name, self.vector(&records)?));
            }
            fields.push(("children", self.vector(&names)?));
        }
        if let Some(node) = &source.node
            && let hir::SourceNode::Bindings {
                is_loop,
                bindings,
                body,
            } = node.as_ref()
        {
            fields.push(("op", self.keyword(if *is_loop { "loop" } else { "let" })?));
            let mut records = Vec::new();
            for binding in bindings.iter() {
                records.push(self.local(binding, depth + 1)?);
            }
            fields.push(("bindings", self.vector(&records)?));
            fields.push(("body", self.ast(body, depth + 1)?));
            let bindings = self.keyword("bindings")?;
            let body = self.keyword("body")?;
            fields.push(("children", self.vector(&[bindings, body])?));
        }
        if let Some(hir::SourceNode::Try {
            body,
            handler,
            cleanup,
            payload,
        }) = source.node.as_deref()
        {
            fields.push(("op", self.keyword("try")?));
            fields.push(("body", self.ast(body, depth + 1)?));
            fields.push(("catch", self.ast(handler, depth + 1)?));
            let name = if let Some(payload) = payload {
                self.form(&payload.declaration, depth + 1)?
            } else {
                self.scalar(Literal::Nil)?
            };
            fields.push(("name", name));
            let mut children = vec![self.keyword("body")?, self.keyword("catch")?];
            let finally = if let Some(cleanup) = cleanup {
                children.push(self.keyword("finally")?);
                self.ast(cleanup, depth + 1)?
            } else {
                self.scalar(Literal::Nil)?
            };
            fields.push(("finally", finally));
            fields.push(("children", self.vector(&children)?));
        }
        if let Some(node) = &source.node {
            // These are original analyzed operands retained before dispatch or
            // storage lowering, not children reconstructed from physical HIR.
            let (operation, many, one) = match node.as_ref() {
                hir::SourceNode::Do { statements, result } => (
                    "do",
                    vec![("statements", statements.as_ref())],
                    vec![("ret", result.as_ref())],
                ),
                hir::SourceNode::If {
                    condition,
                    consequent,
                    alternative,
                } => (
                    "if",
                    vec![],
                    vec![
                        ("test", condition.as_ref()),
                        ("then", consequent.as_ref()),
                        ("else", alternative.as_ref()),
                    ],
                ),
                hir::SourceNode::Recur(arguments) => {
                    ("recur", vec![("exprs", arguments.as_ref())], vec![])
                }
                hir::SourceNode::Throw(exception) => {
                    ("throw", vec![], vec![("exception", exception.as_ref())])
                }
                hir::SourceNode::Assign { target, value } => (
                    "set!",
                    vec![],
                    vec![("target", target.as_ref()), ("val", value.as_ref())],
                ),
                hir::SourceNode::Invoke { callee, arguments } => (
                    "invoke",
                    vec![("args", arguments.as_ref())],
                    vec![("fn", callee.as_ref())],
                ),
                hir::SourceNode::Construct { class, arguments } => (
                    "new",
                    vec![("args", arguments.as_ref())],
                    vec![("class", class.as_ref())],
                ),
                hir::SourceNode::Future(body) => {
                    ("suss/future", vec![], vec![("body", body.as_ref())])
                }
                hir::SourceNode::Await(value) => {
                    ("suss/await", vec![], vec![("value", value.as_ref())])
                }
                _ => ("", vec![], vec![]),
            };
            if !operation.is_empty() {
                fields.push(("op", self.keyword(operation)?));
                // Preserve the public analyzer's declared edge order.
                let names: &[&str] = match operation {
                    "do" => &["statements", "ret"],
                    "if" => &["test", "then", "else"],
                    "recur" => &["exprs"],
                    "throw" => &["exception"],
                    "set!" => &["target", "val"],
                    "invoke" => &["fn", "args"],
                    "new" => &["class", "args"],
                    "suss/future" => &["body"],
                    "suss/await" => &["value"],
                    _ => unreachable!(),
                };
                for (name, children) in many {
                    let mut records = Vec::new();
                    for child in children {
                        records.push(self.ast(child, depth + 1)?);
                    }
                    fields.push((name, self.vector(&records)?));
                }
                for (name, child) in one {
                    fields.push((name, self.ast(child, depth + 1)?));
                }
                let mut keywords = Vec::new();
                for name in names {
                    keywords.push(self.keyword(name)?);
                }
                fields.push(("children", self.vector(&keywords)?));
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
            fields.push(("op", self.keyword("with-meta")?));
            fields.push(("meta", self.ast(metadata, depth + 1)?));
            fields.push(("expr", self.ast(expression, depth + 1)?));
            let meta = self.keyword("meta")?;
            let expr = self.keyword("expr")?;
            fields.push(("children", self.vector(&[meta, expr])?));
        }
        // Pinned analyzer.cljc2630–2655 retains quote syntax separately
        // from its literal const child. Use the actual retained source datum;
        // quoted collection constructor HIR is not a source child AST.
        if let Some(datum) = quoted_datum {
            let literal = self.flag(true)?;
            let mut constant = vec![
                ("op", self.keyword("const")?),
                ("env", env),
                ("literal?", literal),
                ("val", datum),
                ("form", datum),
            ];
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
                constant.push(("tag", tag));
            }
            let constant = constant
                .into_iter()
                .map(|(key, value)| Ok((self.keyword(key)?, value)))
                .collect::<Result<Vec<_>>>()?;
            self.charge(0)?;
            let expr = self.recipe(Recipe::Map(constant))?;
            let expr_key = self.keyword("expr")?;
            fields.extend([
                ("op", self.keyword("quote")?),
                ("literal?", literal),
                ("expr", expr),
                ("children", self.vector(&[expr_key])?),
            ]);
        }
        // Only a genuine resolved source symbol is a local reference. A list
        // head may resolve to a local too, but its AST is an invocation, not a
        // reference. Share canonical declaration/initializer graph recipes.
        if matches!(&source.form.kind, Kind::Symbol(_)) {
            if let Some(hir::SourceBinding::Local(binding)) = &source.resolved {
                fields.push(("op", self.keyword("local")?));
                fields.push(("info", self.local(binding, depth + 1)?));
                fields.push(("name", self.form(&binding.declaration, depth + 1)?));
                fields.push((
                    "local",
                    self.keyword(match binding.source_kind() {
                        hir::LocalKind::Let => "let",
                        hir::LocalKind::Loop => "loop",
                        hir::LocalKind::Argument { .. } => "arg",
                        hir::LocalKind::FunctionName => "fn",
                        hir::LocalKind::Catch => "catch",
                    })?,
                ));
                if let hir::LocalKind::Argument { index, .. } = binding.source_kind() {
                    fields.push(("arg-id", self.number(index)?));
                }
                if let SourceRole::FunctionName {
                    variadic,
                    methods: Some(_),
                } = &binding.source_role
                {
                    fields.push(("variadic?", self.flag(*variadic)?));
                }
                if let Some(initializer) = &binding.initializer {
                    fields.push(("init", self.ast(initializer, depth + 1)?));
                }
            }
        }
        if matches!(&source.form.kind, Kind::Symbol(_))
            && let Some(hir::SourceBinding::Field(binding)) = &source.resolved
        {
            fields.push(("op", self.keyword("local")?));
            fields.push(("local", self.keyword("field")?));
            fields.push(("info", self.field(binding, depth + 1)?));
            fields.push(("name", self.form(&binding.declaration, depth + 1)?));
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
            let info = self.map(vec![("op", op), ("name", name), ("ns", ns), ("tag", tag)])?;
            fields.extend([
                ("op", op),
                ("name", name),
                ("ns", ns),
                ("tag", tag),
                ("info", info),
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
            let Some(Recipe::Map(entries)) = self.recipes.get(declaration).and_then(Option::as_ref)
            else {
                return Err(SessionError::Host(wasmtime::Error::msg(
                    "Missing resolved declaration graph recipe",
                )));
            };
            let mut entries = entries.clone();
            let op_key = self.keyword("op")?;
            let name_key = self.keyword("name")?;
            let ns_key = self.keyword("ns")?;
            let tag_key = self.keyword("tag")?;
            let tag = entries
                .iter()
                .find(|(key, _)| *key == tag_key)
                .map(|(_, value)| *value);
            let operation = self.keyword("var")?;
            let name = self.symbol(&format!("{}/{}", global.namespace(), global.name()))?;
            let namespace = self.symbol(global.namespace())?;
            entries.retain(|(key, _)| ![op_key, name_key, ns_key].contains(key));
            entries.extend([(op_key, operation), (name_key, name), (ns_key, namespace)]);
            self.charge(0)?;
            let info = self.recipe(Recipe::Map(entries))?;
            fields.extend([
                ("op", operation),
                ("name", name),
                ("ns", namespace),
                // Each resolution makes a fresh outer map, while reusing the
                // captured declaration's nested values. Structural interning
                // would incorrectly make two distinct references identical.
                ("info", info),
            ]);
            // Global AST tags are selected from resolved info, unlike local
            // reference inference. Preserve actual presence, including false.
            if let Some(tag) = tag {
                fields.push(("tag", tag));
            }
            true
        } else {
            false
        };
        if !resolved_var && let Some(tag) = &source.tags.tag {
            fields.push(("tag", self.form(tag, depth + 1)?));
        }
        if let Some(tag) = &source.tags.inferred_return {
            let value = if let Some(tag) = tag {
                self.form(tag, depth + 1)?
            } else {
                self.scalar(Literal::Nil)?
            };
            fields.push(("inferred-ret-tag", value));
        }
        if let Some(callable) = &source.callable {
            fields.push(("suss/source-function", self.callable(callable)?));
            if !matches!(
                source.node.as_deref(),
                Some(hir::SourceNode::WithMeta { .. })
            ) {
                fields.push(("op", self.keyword("fn")?));
                let mut methods = Vec::new();
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
                    let params = self.keyword("params")?;
                    let body_key = self.keyword("body")?;
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
                    methods.push(self.map(vec![
                        ("op", op),
                        ("form", form),
                        ("env", env),
                        ("params", parameters),
                        ("body", body),
                        ("children", children),
                        ("variadic?", variadic),
                        ("fixed-arity", fixed),
                        ("recurs", recurs),
                    ])?);
                }
                fields.push(("methods", self.vector(&methods)?));
                let methods = self.keyword("methods")?;
                let children = if let Some(scope) = &callable.name {
                    let name = self.scope(scope, depth + 1)?;
                    fields.push(("name", name));
                    fields.push(("local", name));
                    let local = self.keyword("local")?;
                    self.vector(&[local, methods])?
                } else {
                    fields.push(("name", self.scalar(Literal::Nil)?));
                    self.vector(&[methods])?
                };
                fields.push(("children", children));
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
                    .map(|binding| &binding.value)
                    .chain(std::iter::once(body.as_ref()))
                    .collect(),
            ),
            Expression::Loop { bindings, body, .. } => (
                "loop",
                bindings
                    .iter()
                    .map(|binding| &binding.value)
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
    fn retained_declaration_metadata_keeps_cold_budgets_and_releases_dead_revisions() {
        let mut session = Session::new_macro().unwrap();
        session
            .eval("(def ^{:doc \"retained\"} review-retained 17)")
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let snapshot = session.compilation_snapshot();
        let namespace =
            SourceNamespace::capture(&snapshot.environment, portable::resolve::Phase::Macro);
        let (global, source) = namespace
            .declarations
            .iter()
            .find(|(global, _)| global.name() == "review-retained")
            .map(|(global, source)| (global.clone(), source.clone()))
            .unwrap();
        let mut retained = DeclarationValues::default();
        {
            let mut graph =
                AnalysisGraph::with_declaration_values(&bridge, &mut session, &mut retained);
            let root = graph.declaration(&global, Some(&source), 0).unwrap();
            graph.materialize(root).unwrap();
        }
        assert_eq!(retained.metadata.len(), 1);
        session.collect().unwrap();
        session.set_operation_fuel(1);
        let graph_error = {
            // Updating the configured allowance affects the next request;
            // internal graph work must run inside that request's scope.
            let _request = session.operation_budget_scope().unwrap();
            let mut graph =
                AnalysisGraph::with_declaration_values(&bridge, &mut session, &mut retained);
            graph.declaration(&global, Some(&source), 0).unwrap_err()
        };
        assert!(
            graph_error.to_string().contains("fuel") || graph_error.to_string().contains("Fuel")
        );
        session.set_operation_fuel(10_000_000);
        let materialization_error = {
            let construction_request = session.operation_budget_scope().unwrap();
            let mut graph =
                AnalysisGraph::with_declaration_values(&bridge, &mut session, &mut retained);
            let root = graph.declaration(&global, Some(&source), 0).unwrap();
            drop(construction_request);
            graph.session.set_operation_fuel(1);
            let _request = graph.session.operation_budget_scope().unwrap();
            graph.materialize(root).unwrap_err()
        };
        assert!(
            materialization_error.to_string().contains("fuel")
                || materialization_error.to_string().contains("Fuel")
        );
        session.set_operation_fuel(10_000_000);
        // Equal source data in a distinct Arc is still a separate revision.
        let other = Arc::new(source.as_ref().clone());
        {
            let _request = session.operation_budget_scope().unwrap();
            let mut graph =
                AnalysisGraph::with_declaration_values(&bridge, &mut session, &mut retained);
            let root = graph.declaration(&global, Some(&other), 0).unwrap();
            graph.materialize(root).unwrap();
        }
        assert_eq!(retained.metadata.len(), 2);
        let compare = session.eval("(fn [a b] (identical? a b))").unwrap();
        let left = retained.metadata[&(global.clone(), Arc::as_ptr(&source) as usize)]
            .1
            .clone();
        let right = retained.metadata[&(global.clone(), Arc::as_ptr(&other) as usize)]
            .1
            .clone();
        let result = session.invoke(&compare, &[&left, &right]).unwrap();
        assert_eq!(
            bridge.read(&mut session, &result, 0..1).unwrap().kind,
            Kind::Bool(false)
        );
        drop(other);
        retained.prune();
        assert_eq!(retained.metadata.len(), 1);
        {
            let mut graph =
                AnalysisGraph::with_declaration_values(&bridge, &mut session, &mut retained);
            let root = graph.declaration(&global, Some(&source), 0).unwrap();
            graph.materialize(root).unwrap();
        }
        let value = session.eval("review-retained").unwrap();
        assert_eq!(
            bridge.read(&mut session, &value, 0..1).unwrap().kind,
            Kind::Number(17.0)
        );
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
