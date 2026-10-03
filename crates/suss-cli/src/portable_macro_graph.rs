//! Original bounded transport of actual compiler analysis graphs.
//! This preserves native facts and sharing; source-level &env schema is separate.
use crate::{
    portable_macro_data::FormBridge,
    portable_session::{Session, SessionError, SessionValue},
};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::Arc,
};
use suss_compile::portable::{
    self,
    hir::{
        self, Expression, FieldBinding, FunctionScope, Hir, Literal, LocalBinding, SourceNamespace,
        SourceRole,
    },
};
use suss_reader::forms::{Form, Kind};

type Value = usize;
type Result<T> = std::result::Result<T, SessionError>;

enum Recipe {
    Scalar(Literal),
    Identifier {
        namespace: Option<String>,
        name: String,
        keyword: bool,
    },
    Form(Form),
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
    Environment {
        namespace: Arc<SourceNamespace>,
        catalog: Arc<SourceNamespace>,
        locals: Arc<HashMap<String, LocalBinding>>,
        fields: Arc<HashMap<String, FieldBinding>>,
        scopes: Arc<[Arc<FunctionScope>]>,
        context: portable::AnalysisContext,
    },
}

/// Memo tables own their source identities as well as the resulting GC roots.
/// They live for one graph build; no pointer or generation is an artifact cache key.
pub struct AnalysisGraph<'a> {
    bridge: &'a FormBridge,
    session: &'a mut Session,
    nodes: usize,
    units: usize,
    keys: BTreeMap<String, Value>,
    locals: BTreeMap<(usize, usize, String), (LocalBinding, Value)>,
    fields: BTreeMap<usize, (FieldBinding, Value)>,
    scopes: BTreeMap<usize, (Arc<FunctionScope>, Value)>,
    callables: BTreeMap<usize, (Arc<hir::SourceCallable>, Value)>,
    namespaces: BTreeMap<usize, (Arc<SourceNamespace>, Value)>,
    asts: BTreeMap<usize, (Arc<hir::SourceAnalysis>, Value)>,
    recipes: Vec<Option<Recipe>>,
    jobs: VecDeque<(Value, Task)>,
}
impl<'a> AnalysisGraph<'a> {
    pub fn new(bridge: &'a FormBridge, session: &'a mut Session) -> Self {
        Self {
            bridge,
            session,
            nodes: 65_536,
            units: 1_048_576,
            keys: BTreeMap::new(),
            locals: BTreeMap::new(),
            fields: BTreeMap::new(),
            scopes: BTreeMap::new(),
            callables: BTreeMap::new(),
            namespaces: BTreeMap::new(),
            asts: BTreeMap::new(),
            recipes: Vec::new(),
            jobs: VecDeque::new(),
        }
    }
    fn recipe(&mut self, recipe: Recipe) -> Result<Value> {
        let id = self.recipes.len();
        self.recipes.push(Some(recipe));
        Ok(id)
    }
    fn task(&mut self, task: Task) -> Result<Value> {
        self.charge(0)?;
        let id = self.recipes.len();
        self.recipes.push(None);
        self.jobs.push_back((id, task));
        Ok(id)
    }
    fn materialize(&mut self, root: Value) -> Result<SessionValue> {
        while let Some((id, task)) = self.jobs.pop_front() {
            let result = match task {
                Task::Namespace(value) => self.namespace_record(&value, 0)?,
                Task::Local(value) => self.local_record(&value, 0)?,
                Task::Field(value) => self.field_record(&value, 0)?,
                Task::Scope(value) => self.scope_record(&value, 0)?,
                Task::Callable(value) => self.callable_record(&value, 0)?,
                Task::Ast(value) => self.ast_record(&value, 0)?,
                Task::Lowering(value) => self.lowering_record(&value, 0)?,
                Task::Environment {
                    namespace,
                    catalog,
                    locals,
                    fields,
                    scopes,
                    context,
                } => self.environment_record(&namespace, &catalog, &locals, &fields, &scopes, context, 0)?,
            };
            self.recipes[id] = Some(Recipe::Alias(result));
        }
        // Reader syntax has its own 64-level bound. Analysis dependency depth
        // instead follows the bounded DAG and uses no recursive Rust calls.
        let mut state = vec![0u8; self.recipes.len()];
        let mut values: Vec<Option<SessionValue>> = vec![None; self.recipes.len()];
        let mut stack = vec![(root, false)];
        while let Some((id, finish)) = stack.pop() {
            if state[id] == 2 {
                continue;
            }
            let recipe = self.recipes[id].as_ref().expect("all graph tasks filled");
            if !finish {
                if state[id] == 1 {
                    return Err(SessionError::Host(wasmtime::Error::msg(
                        "Cyclic compiler analysis graph",
                    )));
                }
                state[id] = 1;
                stack.push((id, true));
                let dependencies: Vec<_> = match recipe {
                    Recipe::Alias(value) => vec![*value],
                    Recipe::Vector(values) | Recipe::Set(values) => values.clone(),
                    Recipe::Map(entries) => entries
                        .iter()
                        .flat_map(|(key, value)| [*key, *value])
                        .collect(),
                    _ => vec![],
                };
                for dependency in dependencies.into_iter().rev() {
                    stack.push((dependency, false));
                }
                continue;
            }
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
                Recipe::Form(form) => self.bridge.quote(self.session, form.clone())?,
                Recipe::Alias(value) => values[*value]
                    .as_ref()
                    .expect("ordered alias dependency")
                    .clone(),
                Recipe::Vector(items) | Recipe::Set(items) => {
                    let items = items
                        .iter()
                        .map(|id| {
                            values[*id]
                                .as_ref()
                                .expect("ordered vector dependency")
                                .clone()
                        })
                        .collect::<Vec<_>>();
                    if matches!(recipe, Recipe::Set(_)) {
                        self.bridge.set_values(self.session, &items)?
                    } else {
                        self.bridge.vector_values(self.session, &items)?
                    }
                }
                Recipe::Map(entries) => {
                    let entries = entries
                        .iter()
                        .map(|(key, value)| {
                            (
                                values[*key].as_ref().expect("ordered map key").clone(),
                                values[*value].as_ref().expect("ordered map value").clone(),
                            )
                        })
                        .collect::<Vec<_>>();
                    self.bridge.map_values(self.session, &entries)?
                }
            };
            values[id] = Some(value);
            state[id] = 2;
        }
        Ok(values[root].take().expect("materialized graph root"))
    }
    fn charge(&mut self, units: usize) -> Result<()> {
        self.nodes = self.nodes.checked_sub(1).ok_or_else(|| {
            SessionError::Host(wasmtime::Error::msg("Analysis graph exceeds 65536 nodes"))
        })?;
        self.units = self.units.checked_sub(units).ok_or_else(|| {
            SessionError::Host(wasmtime::Error::msg(
                "Analysis graph exceeds UTF-16 storage bound",
            ))
        })?;
        Ok(())
    }
    fn scalar(&mut self, value: Literal) -> Result<Value> {
        self.charge(if let Literal::String(units) = &value {
            units.len()
        } else {
            0
        })?;
        self.recipe(Recipe::Scalar(value))
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
        self.charge(name.encode_utf16().count() * 2)?;
        let (namespace, name) = name
            .split_once('/')
            .filter(|_| name != "/")
            .map_or((None, name), |(ns, name)| (Some(ns), name));
        self.recipe(Recipe::Identifier {
            namespace: namespace.map(str::to_owned),
            name: name.into(),
            keyword: false,
        })
    }
    fn map(&mut self, entries: Vec<(&str, Value)>) -> Result<Value> {
        self.charge(0)?;
        let entries = entries
            .into_iter()
            .map(|(key, value)| Ok((self.keyword(key)?, value)))
            .collect::<Result<Vec<_>>>()?;
        self.recipe(Recipe::Map(entries))
    }
    fn vector(&mut self, values: &[Value]) -> Result<Value> {
        self.charge(0)?;
        self.recipe(Recipe::Vector(values.to_vec()))
    }
    fn form(&mut self, form: &Form, _depth: usize) -> Result<Value> {
        // Account for the complete data tree against this build's aggregate
        // budget, in addition to the bridge's per-form construction bounds.
        fn account(graph: &mut AnalysisGraph<'_>, form: &Form, depth: usize) -> Result<()> {
            if depth >= 64 {
                return Err(SessionError::Host(wasmtime::Error::msg(
                    "Analysis graph exceeds 64 levels",
                )));
            }
            let units = match &form.kind {
                Kind::String(value) => value.len(),
                Kind::Symbol(value) => value.to_string().encode_utf16().count() * 2,
                Kind::Keyword(value) => {
                    (value.name.encode_utf16().count()
                        + value
                            .namespace
                            .as_ref()
                            .map_or(0, |ns| ns.encode_utf16().count() + 1))
                        * 2
                }
                _ => 0,
            };
            graph.charge(units)?;
            for metadata in &form.metadata {
                account(graph, metadata, depth + 1)?;
            }
            match &form.kind {
                Kind::List(items)
                | Kind::Vector(items)
                | Kind::Map(items)
                | Kind::Set(items)
                | Kind::Conditional(items) => {
                    for item in items {
                        account(graph, item, depth + 1)?;
                    }
                }
                Kind::Discard(value) => account(graph, value, depth + 1)?,
                Kind::Prefix { operator, target } => {
                    account(graph, operator, depth + 1)?;
                    account(graph, target, depth + 1)?;
                }
                _ => {}
            }
            Ok(())
        }
        // Reader nesting starts at this form, independently of graph edges.
        account(self, form, 0)?;
        self.recipe(Recipe::Form(form.clone()))
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
        let catalog = Arc::new(SourceNamespace::capture(context.environment, context.phase));
        let root = self.environment(
            context.namespace_snapshot,
            &catalog,
            context.locals,
            context.fields,
            context.function_scopes,
            context.context,
            0,
        )?;
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
        self.task(Task::Environment {
            namespace: namespace.clone(),
            catalog: catalog.clone(),
            locals: Arc::new(locals.clone()),
            fields: Arc::new(fields.clone()),
            scopes: scopes.to_vec().into(),
            context,
        })
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
        self.charge(0)?;
        let locals = self.recipe(Recipe::Map(entries))?;
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
    fn namespace(&mut self, namespace: &Arc<SourceNamespace>, depth: usize) -> Result<Value> {
        Self::depth(depth)?;
        let key = Arc::as_ptr(namespace) as usize;
        if let Some((_, value)) = self.namespaces.get(&key) {
            return Ok(value.clone());
        }
        let value = self.task(Task::Namespace(namespace.clone()))?;
        self.namespaces.insert(key, (namespace.clone(), value));
        Ok(value)
    }
    // The pinned namespace contract retains nil for unused ordinary requires,
    // ordinary uses and macro requires; rename and macro-use maps stay maps.
    fn nullable_map(&mut self, entries: Vec<(Value, Value)>) -> Result<Value> {
        self.charge(0)?;
        self.recipe(if entries.is_empty() { Recipe::Scalar(Literal::Nil) } else { Recipe::Map(entries) })
    }
    fn namespace_record(
        &mut self,
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
        self.charge(0)?;
        let refers = self.recipe(Recipe::Map(refers))?;
        let mut uses = Vec::new();
        let mut renames = Vec::new();
        for (local, target) in &namespace.refers {
            if namespace.used_refers.contains(local) {
                uses.push((self.symbol(local)?, self.symbol(target.namespace())?));
            }
            if namespace.renamed_refers.contains(local) {
                renames.push((self.symbol(local)?, self.symbol(&format!("{}/{}", target.namespace(), target.name()))?));
            }
        }
        let uses = self.nullable_map(uses)?;
        self.charge(0)?;
        let renames = self.recipe(Recipe::Map(renames))?;
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
                macro_renames.push((self.symbol(local)?, self.symbol(&format!("{ns}/{original}"))?));
            }
        }
        self.charge(0)?;
        let macro_uses = self.recipe(Recipe::Map(macro_uses))?;
        self.charge(0)?;
        let macro_renames = self.recipe(Recipe::Map(macro_renames))?;
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
            let name = self.symbol(&format!("{}/{}", global.namespace(), global.name()))?;
            let ns = self.symbol(global.namespace())?;
            let mut fields = vec![("name", name), ("ns", ns)];
            if let Some(info) = namespace.declarations.get(global) {
                if let Some(tag) = hir::declaration_tag(info).map_err(|error| SessionError::Host(wasmtime::Error::msg(error.message)))? {
                    fields.push(("tag", self.form(&tag, depth + 1)?));
                }
                fields.push(("suss/definition-form", self.form(&info.definition_form, depth + 1)?));
                fields.push(("suss/analysis-completed", self.flag(info.analysis_completed)?));
                fields.push(("suss/declaration", self.form(&info.declaration, depth + 1)?));
                fields.push(("suss/defonce", self.flag(info.once)?));
                if let Some(form) = &info.initializer_form {
                    fields.push(("suss/initializer-form", self.form(form, depth + 1)?));
                }
                fields.push((
                    "suss/initializer-recorded",
                    self.flag(info.initializer.is_some())?,
                ));
                if let Some(doc) = &info.docstring {
                    fields.push(("doc", self.scalar(Literal::String(doc.clone()))?));
                }
                if let Some(callable) = info.initializer.as_ref().and_then(|init| init.source.as_ref()).and_then(|source| source.callable.as_ref()) {
                    fields.push(("suss/source-function", self.callable(callable)?));
                }
            }
            declarations.push((short, self.map(fields)?));
        }
        self.charge(0)?;
        let declarations = self.recipe(Recipe::Map(declarations))?;
        let value = self.map(vec![
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
        ])?;
        Ok(value)
    }
    fn local(&mut self, binding: &LocalBinding, depth: usize) -> Result<Value> {
        Self::depth(depth)?;
        // Same declaration can have distinct receiver/argument roles or lowered
        // IDs. Keep that adaptation in the key; retain the identity owner.
        let role = match &binding.source_role {
            SourceRole::Plain => "plain".into(),
            SourceRole::FunctionName { variadic } => format!("function:{variadic}"),
            SourceRole::PrivateCatch { .. } => "private-catch".into(),
            SourceRole::CatchBinding { .. } => "catch-alias".into(),
            SourceRole::MethodArgument { index, rest } => format!("method-arg:{index}:{rest}"),
            SourceRole::MethodThis { .. } => "method-this".into(),
        };
        let key = (Arc::as_ptr(&binding.identity) as usize, binding.id.0, role);
        if let Some((_, value)) = self.locals.get(&key) {
            return Ok(value.clone());
        }
        let value = self.task(Task::Local(binding.clone()))?;
        self.locals.insert(key, (binding.clone(), value));
        Ok(value)
    }
    fn local_record(&mut self, binding: &LocalBinding, depth: usize) -> Result<Value> {
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
        if let Some(tag) = hir::local_tag(binding).map_err(|error| SessionError::Host(wasmtime::Error::msg(error.message)))? {
            entries.push(("tag", self.form(&tag, depth + 1)?));
        }
        if !matches!(binding.source_role, SourceRole::PrivateCatch { .. }) {
            entries.push(("op", self.keyword("binding")?));
            entries.push(("binding-form?", self.flag(true)?));
        }
        if let Some(position) = binding
            .origin
            .as_ref()
            .and_then(|origin| origin.symbol_position(&binding.declaration))
        {
            entries.push(("line", self.number(position.line)?));
            entries.push(("column", self.number(position.column)?));
        }
        if let hir::LocalKind::Argument { index, rest } = binding.source_kind() {
            entries.push(("arg-id", self.number(index)?));
            entries.push(("suss/rest-parameter", self.flag(rest)?));
        }
        if let SourceRole::FunctionName { variadic } = binding.source_role {
            entries.push(("suss/variadic", self.flag(variadic)?));
        }
        if let Some(initializer) = &binding.initializer {
            entries.push(("init", self.ast(initializer, depth + 1)?));
        }
        if let Some(shadow) = &binding.shadow {
            entries.push(("shadow", self.local(shadow, depth + 1)?));
        } else if let Some(field) = &binding.shadow_field {
            entries.push(("shadow", self.field(field, depth + 1)?));
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
        let value = self.map(entries)?;
        Ok(value)
    }
    fn field(&mut self, field: &FieldBinding, depth: usize) -> Result<Value> {
        Self::depth(depth)?;
        let key = Arc::as_ptr(&field.identity) as usize;
        if let Some((_, value)) = self.fields.get(&key) {
            return Ok(value.clone());
        }
        let value = self.task(Task::Field(field.clone()))?;
        self.fields.insert(key, (field.clone(), value));
        Ok(value)
    }
    fn field_record(&mut self, field: &FieldBinding, depth: usize) -> Result<Value> {
        let name = self.form(&field.declaration, depth + 1)?;
        let kind = self.keyword("field")?;
        let is_field = self.flag(true)?;
        let mutable = self.flag(field.mutable)?;
        let index = self.number(field.index)?;
        let access = self.lowering(&field.access, depth + 1)?;
        let value = self.map(vec![
            ("name", name),
            ("local", kind),
            ("field", is_field),
            ("mutable", mutable),
            ("suss/index", index),
            ("suss/access", access),
        ])?;
        Ok(value)
    }
    fn scope(&mut self, scope: &Arc<FunctionScope>, depth: usize) -> Result<Value> {
        Self::depth(depth)?;
        let key = Arc::as_ptr(scope) as usize;
        if let Some((_, value)) = self.scopes.get(&key) {
            return Ok(value.clone());
        }
        let value = self.task(Task::Scope(scope.clone()))?;
        self.scopes.insert(key, (scope.clone(), value));
        Ok(value)
    }
    fn scope_record(&mut self, scope: &Arc<FunctionScope>, depth: usize) -> Result<Value> {
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
            ("fn-scope", parents),
            ("suss/explicit-self", explicit),
            ("suss/function-form", self.form(&scope.function_form, depth + 1)?),
            ("suss/phase", phase),
        ];
        if let Some(shadow) = &scope.shadow {
            info.push(("shadow", self.local(shadow, depth + 1)?));
        } else if let Some(shadow) = &scope.shadow_field {
            info.push(("shadow", self.field(shadow, depth + 1)?));
        }
        let info = self.map(info)?;
        let value = self.map(vec![
            ("name", name),
            ("local", kind),
            ("env", env),
            ("info", info),
        ])?;
        Ok(value)
    }
    fn ast(&mut self, hir: &Hir, depth: usize) -> Result<Value> {
        Self::depth(depth)?;
        let Some(source) = &hir.source else {
            return self.lowering(hir, depth);
        };
        let key = Arc::as_ptr(source) as usize;
        if let Some((_, value)) = self.asts.get(&key) {
            return Ok(value.clone());
        }
        let value = self.task(Task::Ast(hir.clone()))?;
        self.asts.insert(key, (source.clone(), value));
        Ok(value)
    }
    fn callable(&mut self, callable: &Arc<hir::SourceCallable>) -> Result<Value> {
        let key = Arc::as_ptr(callable) as usize;
        if let Some((_, value)) = self.callables.get(&key) { return Ok(*value); }
        let value = self.task(Task::Callable(callable.clone()))?;
        self.callables.insert(key, (callable.clone(), value));
        Ok(value)
    }
    fn callable_record(&mut self, callable: &hir::SourceCallable, depth: usize) -> Result<Value> {
        let mut methods = Vec::new();
        for method in &callable.methods {
            let mut parameters = Vec::new();
            for parameter in &method.parameters {
                let form = Form {
                    span: parameter.span.clone(), metadata: parameter.metadata.clone(),
                    kind: Kind::Symbol(suss_reader::Symbol { namespace: None, name: parameter.name.clone() }),
                };
                parameters.push(self.form(&form, depth + 1)?);
            }
            let parameters = self.vector(&parameters)?;
            let body = self.ast(&method.body, depth + 1)?;
            let variadic = self.flag(method.variadic)?;
            methods.push(self.map(vec![("suss/parameters", parameters), ("suss/body", body), ("suss/variadic", variadic)])?);
        }
        let methods = self.vector(&methods)?;
        let variadic = self.flag(callable.variadic())?;
        let fixed = callable.max_fixed_arity().ok_or_else(|| SessionError::Host(wasmtime::Error::msg("Invalid source callable method or variadic parameter list")))?;
        let fixed = self.number(fixed)?;
        self.map(vec![("suss/methods", methods), ("suss/variadic", variadic), ("suss/max-fixed-arity", fixed)])
    }
    fn ast_record(&mut self, hir: &Hir, depth: usize) -> Result<Value> {
        let source = hir.source.as_ref().expect("source AST task");
        let form = self.form(&source.form, depth + 1)?;
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
        let mut fields = vec![
            ("form", form),
            ("env", env),
            ("suss/lowering", lowering),
        ];
        if let Some(tag) = &source.tags.tag {
            fields.push(("tag", self.form(tag, depth + 1)?));
        }
        if let Some(tag) = &source.tags.inferred_return {
            let value = if let Some(tag) = tag { self.form(tag, depth + 1)? } else { self.scalar(Literal::Nil)? };
            fields.push(("inferred-ret-tag", value));
        }
        if let Some(callable) = &source.callable {
            fields.push(("suss/source-function", self.callable(callable)?));
        }
        let value = self.map(fields)?;
        Ok(value)
    }
    fn lowering(&mut self, hir: &Hir, depth: usize) -> Result<Value> {
        self.task(Task::Lowering(hir.clone()))
    }
    fn lowering_record(&mut self, hir: &Hir, depth: usize) -> Result<Value> {
        Self::depth(depth)?;
        let (operation, children): (&str, Vec<&Hir>) = match &hir.kind {
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
        self.map(entries)
    }
}
