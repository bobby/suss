//! Source macro functions execute in a separate compiled phase Store.
use crate::{
    portable_macro_data::FormBridge,
    portable_macro_graph::{AnalysisGraph, DeclarationValues},
    portable_session::{Session, SessionError, SessionValue},
};
use std::collections::{BTreeMap, BTreeSet};
use crate::portable::{Diagnostic, ExpansionContext, ExpansionHost};
use suss_reader::forms::{Form, Kind, read_forms};
pub struct CompiledMacros {
    session: Session,
    bridge: FormBridge,
    declaration_values: DeclarationValues,
    definitions: BTreeMap<(String, String), SessionValue>,
    // Macros whose definitions cannot read their implicit &env parameter.
    environment_free: BTreeSet<(String, String)>,
    environment_materializations: u64,
    // Source macro expansions performed by this host, in any context.
    source_expansions: u64,
    declaration_sources: BTreeMap<(String, String), String>,
    loaded_sources: BTreeMap<String, String>,
    incomplete_sources: BTreeSet<String>,
    artifact_cache: crate::portable::artifact_cache::ArtifactCache,
}
fn symbol(name: &str, span: std::ops::Range<usize>) -> Form {
    Form {
        span,
        metadata: vec![],
        kind: Kind::Symbol(suss_reader::Symbol {
            namespace: None,
            name: name.into(),
        }),
    }
}
fn failure(form: &Form, message: &str) -> SessionError {
    SessionError::Compile(Diagnostic {
        span: form.span.clone(),
        message: message.into(),
    })
}
pub(crate) type MacroCheckpoint = (
    crate::portable_session::BindingCheckpoint,
    BTreeMap<(String, String), SessionValue>,
    BTreeMap<(String, String), String>,
    BTreeSet<(String, String)>,
);

/// Whether a definition contains a symbol named &env anywhere, including in
/// quoted or syntax-quoted data. Together with source-macro expansions observed
/// while its body compiles, this decides whether a macro may read &env.
fn mentions_environment(form: &Form) -> bool {
    let mut pending = vec![form];
    while let Some(form) = pending.pop() {
        pending.extend(&form.metadata);
        match &form.kind {
            Kind::Symbol(symbol) if symbol.name == "&env" => return true,
            Kind::List(items)
            | Kind::Vector(items)
            | Kind::Set(items)
            | Kind::Map(items)
            | Kind::Conditional(items) => pending.extend(items),
            Kind::Discard(target) => pending.push(target.as_ref()),
            Kind::Prefix { operator, target } => {
                pending.push(operator.as_ref());
                pending.push(target.as_ref());
            }
            _ => {}
        }
    }
    false
}
fn add_implicit_arguments(parts: &[Form], form: &Form) -> Result<Vec<Form>, SessionError> {
    let Some(parameters) = parts.first() else {
        return Err(failure(
            form,
            "Macro signature requires parameters and body",
        ));
    };
    let Kind::Vector(names) = &parameters.kind else {
        return Err(failure(parameters, "Macro parameters must be a vector"));
    };
    if parts.len() < 2 {
        return Err(failure(form, "Macro signature requires a body"));
    }
    let mut names = names.clone();
    names.insert(0, symbol("&env", form.span.clone()));
    names.insert(0, symbol("&form", form.span.clone()));
    let mut result = vec![Form {
        span: parameters.span.clone(),
        metadata: parameters.metadata.clone(),
        kind: Kind::Vector(names),
    }];
    result.extend_from_slice(&parts[1..]);
    Ok(result)
}
impl CompiledMacros {
    pub fn new() -> Result<Self, SessionError> {
        Self::with_bootstrap(Session::new_macro()?)
    }
    fn with_bootstrap(mut session: Session) -> Result<Self, SessionError> {
        let bridge = FormBridge::new(&mut session)?;
        let mut macros = Self {
            session,
            bridge,
            declaration_values: Default::default(),
            definitions: BTreeMap::new(),
            environment_free: BTreeSet::new(),
            environment_materializations: 0,
            source_expansions: 0,
            declaration_sources: BTreeMap::new(),
            loaded_sources: BTreeMap::new(),
            incomplete_sources: BTreeSet::new(),
            artifact_cache: Default::default(),
        };
        macros.enter_namespace("suss.core")?;
        macros.define(crate::portable_defn::SOURCE)?;
        macros.enter_namespace("user")?;
        // Bootstrap code is already rooted in the Store. User source cache
        // accounting starts empty, including after an explicit session reset.
        macros.artifact_cache = Default::default();
        Ok(macros)
    }
    pub(crate) fn binding_checkpoint(&mut self) -> Result<MacroCheckpoint, SessionError> {
        Ok((self.session.binding_checkpoint()?, self.definitions.clone(), self.declaration_sources.clone(), self.environment_free.clone()))
    }
    pub(crate) fn restore_bindings(&mut self, checkpoint: MacroCheckpoint, globals: &[crate::portable::resolve::Global]) -> Result<(), SessionError> {
        self.session.restore_bindings(checkpoint.0, globals)?;
        for global in globals {
            let key = (global.namespace().to_owned(), global.name().to_owned());
            if let Some(value) = checkpoint.1.get(&key) { self.definitions.insert(key.clone(), value.clone()); }
            else { self.definitions.remove(&key); }
            if checkpoint.3.contains(&key) { self.environment_free.insert(key.clone()); }
            else { self.environment_free.remove(&key); }
            if let Some(source) = checkpoint.2.get(&key) { self.declaration_sources.insert(key, source.clone()); }
            else { self.declaration_sources.remove(&key); }
        }
        Ok(())
    }
    pub fn current_namespace(&self) -> &str {
        self.session.current_namespace()
    }
    pub fn enter_namespace(&mut self, namespace: &str) -> Result<(), SessionError> {
        self.session.enter_namespace(namespace)
    }
    pub fn define(&mut self, source: &str) -> Result<(), SessionError> {
        let forms = read_forms(source).map_err(|error| {
            SessionError::Compile(Diagnostic {
                span: error.span,
                message: error.message,
            })
        })?;
        if forms.len() != 1 {
            return Err(SessionError::Compile(Diagnostic {
                span: 0..source.len(),
                message: "Expected exactly one source defmacro".into(),
            }));
        }
        self.define_form_with_origin(forms.into_iter().next().unwrap(), 0..source.len(), Some(&crate::portable::SourceOrigin::new(source, None)))
            .map(|_| ())
    }
    fn load_source_namespace(
        &mut self,
        namespace: &str,
        policy: crate::portable::MacroReload,
    ) -> Result<Vec<String>, SessionError> {
        use crate::portable::{modules, resolve::Phase};
        if matches!(namespace, "suss.core" | "cljs.core")
            && policy != crate::portable::MacroReload::Once
        {
            return Err(SessionError::Compile(Diagnostic {
                span: 0..0,
                message: "The compiled macro bootstrap core cannot be source-reloaded".into(),
            }));
        }
        let mut snapshot = self.session.compilation_snapshot();
        match policy {
            crate::portable::MacroReload::Once => {}
            crate::portable::MacroReload::Reload => {
                snapshot.provided.remove(
                    &modules::ModuleIdentity::new(Phase::Macro, namespace)
                        .map_err(SessionError::Compile)?,
                );
            }
            crate::portable::MacroReload::ReloadAll => {
                snapshot
                    .provided
                    .retain(|identity| identity.namespace() == "suss.core");
            }
        }
        let graph = modules::discover_phase_modules(
            namespace,
            &snapshot.source_paths,
            &snapshot.environment,
            Phase::Macro,
            &snapshot.provided,
        )
        .map_err(SessionError::Module)?;
        // Discovery errors preserve previous loaded identities. Once execution
        // starts, each selected phase unit must initialize before being loaded again.
        // A selected unit may publish cells before its initializer fails. Its old
        // immutable identity no longer establishes the complete live graph, so
        // emission reuse stays disabled until all selected units finish loading.
        self.incomplete_sources.extend(graph.iter().map(|unit| unit.identity.namespace().to_owned()));
        self.session.invalidate_source_modules(
            &graph
                .iter()
                .map(|unit| unit.identity.clone())
                .collect::<Vec<_>>(),
        );
        let caller = self.session.current_namespace().to_owned();
        let result = (|| {
            for unit in graph {
                let source_identity = crate::portable::bootstrap::sha256(
                    format!("{:?}:{:?}:{}", unit.path, unit.dependencies, unit.source).as_bytes());
                let origin = crate::portable::SourceOrigin::new(unit.source.as_str(), Some(unit.path.clone()));
                // Source snapshots are already selected and dependency-first. Each
                // form executes once in Macro phase; definitions compile to the
                // same native function pipeline and become available to later forms.
                for mut form in unit.forms {
                    let definition = matches!(&form.kind, Kind::List(items) if matches!(items.first().map(|head| &head.kind), Some(Kind::Symbol(head)) if self.session.resolves_macro_definition(head)));
                    if definition {
                        let Kind::List(items) = &mut form.kind else {
                            unreachable!()
                        };
                        let Kind::Symbol(head) = &mut items[0].kind else {
                            unreachable!()
                        };
                        head.namespace = Some("suss.core".into());
                        head.name = "defmacro".into();
                        let span = form.span.clone();
                        self.define_form_with_origin(form, span, Some(&origin))?;
                    } else {
                        let span = form.span.clone();
                        let prepared =
                            self.session
                                .compilation_snapshot()
                                .prepare_with_origin(vec![form], span, self, Some(&origin))?;
                        self.session.eval_prepared(prepared)?;
                    }
                }
                self.session
                    .initialized_source_namespace(unit.identity.namespace())?;
                self.loaded_sources.insert(unit.identity.namespace().to_owned(), source_identity);
                self.incomplete_sources.remove(unit.identity.namespace());
            }
            Ok(self
                .definitions
                .keys()
                .filter(|(ns, _)| ns == namespace)
                .map(|(_, name)| name.clone())
                .collect())
        })();
        self.session.enter_namespace(&caller)?;
        result
    }
    pub fn set_operation_fuel(&mut self, fuel: u64) {
        self.session.set_operation_fuel(fuel);
    }
    /// Interrupts a running macro expansion in this compiled macro session.
    pub fn interrupt_handle(&self) -> crate::portable_session::InterruptHandle {
        self.session.interrupt_handle()
    }
    pub(crate) fn replacement(&self) -> Result<Self, SessionError> {
        Self::with_bootstrap(self.session.replacement()?)
    }
    pub(crate) fn retire_for_reset(&mut self) -> Result<(), SessionError> {
        self.session.retire_for_reset()
    }
    pub(crate) fn display_value(&mut self, value: &SessionValue) -> Result<String, SessionError> {
        crate::portable_repl::display(&mut self.session, value)
    }
    pub(crate) fn define_form_display(
        &mut self,
        form: Form,
        span: std::ops::Range<usize>,
        origin: Option<&crate::portable::SourceOrigin>,
    ) -> Result<String, SessionError> {
        let value = self.define_form_with_origin(form, span, origin)?;
        crate::portable_repl::display(&mut self.session, &value)
    }
    pub(crate) fn define_form_with_origin(
        &mut self, form: Form, span: std::ops::Range<usize>,
        origin: Option<&crate::portable::SourceOrigin>,
    ) -> Result<SessionValue, SessionError> {
        let form = &form;
        let Kind::List(items) = &form.kind else {
            return Err(failure(form, "Expected source defmacro"));
        };
        if items.len() < 3
            || !matches!(&items[0].kind, Kind::Symbol(s) if s.name == "defmacro" && (s.namespace.is_none() || matches!(s.namespace.as_deref(), Some("suss.core" | "cljs.core" | "clojure.core"))))
        {
            return Err(failure(form, "Expected defmacro name parameters and body"));
        }
        let Kind::Symbol(name) = &items[1].kind else {
            return Err(failure(&items[1], "Macro name must be a symbol"));
        };
        if name.namespace.is_some() {
            return Err(failure(
                &items[1],
                "Macro definition name must be unqualified",
            ));
        }
        let mut name_form = items[1].clone();
        let mut docstring = None;
        let mut start = 2;
        // Pinned defmacro/defn retain leading docs/attributes before signatures.
        while let Some(item) = items.get(start) {
            match &item.kind {
                Kind::String(_) => docstring = Some(item.clone()),
                Kind::Map(_) => name_form.metadata.push(item.clone()),
                _ => break,
            }
            start += 1;
        }
        let declarations = &items[start..];
        let Some(first) = declarations.first() else {
            return Err(failure(form, "Macro requires at least one signature"));
        };
        let mut function = vec![symbol("fn", form.span.clone())];
        if matches!(first.kind, Kind::Vector(_)) {
            function.extend(add_implicit_arguments(declarations, form)?);
        } else {
            let mut signatures = declarations;
            if let Some(attributes) = signatures
                .last()
                .filter(|item| matches!(item.kind, Kind::Map(_)))
            {
                name_form.metadata.push(attributes.clone());
                signatures = &signatures[..signatures.len() - 1];
            }
            if signatures.is_empty() {
                return Err(failure(form, "Macro requires at least one signature"));
            }
            for signature in signatures {
                let Kind::List(parts) = &signature.kind else {
                    return Err(failure(signature, "Macro signature must be a list"));
                };
                function.push(Form {
                    span: signature.span.clone(),
                    metadata: signature.metadata.clone(),
                    kind: Kind::List(add_implicit_arguments(parts, signature)?),
                });
            }
        }
        let mut definition = vec![symbol("def", form.span.clone()), name_form];
        if let Some(docstring) = docstring {
            definition.push(docstring);
        }
        definition.push(Form {
            span: form.span.clone(),
            metadata: vec![],
            kind: Kind::List(function),
        });
        let definition = Form {
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            kind: Kind::List(definition),
        };
        // A macro observes &env only through a lexical reference to its
        // implicit parameter: written in its definition, or produced by a source
        // macro expanded while its body compiles (through any rename, alias or
        // form rewrite). Compiler-implemented forms never introduce one. A macro
        // with neither cannot observe &env, so it receives nil.
        let expansions = self.source_expansions;
        let snapshot = self.session.compilation_snapshot();
        let prepared = snapshot.prepare_with_origin(vec![definition], span, self, origin)?;
        let reads_environment =
            mentions_environment(form) || self.source_expansions != expansions;
        let value = self.session.eval_prepared(prepared)?;
        let namespace = self.session.current_namespace().to_owned();
        let key = (namespace.clone(), name.name.clone());
        if reads_environment {
            self.environment_free.remove(&key);
        } else {
            self.environment_free.insert(key.clone());
        }
        self.definitions.insert(key, value.clone());
        let provenance = format!("{form:?}:{}:{:?}", origin.map_or("", |origin| origin.text()), origin.and_then(|origin| origin.path()));
        self.declaration_sources.insert((namespace.clone(), name.name.clone()), crate::portable::bootstrap::sha256(provenance.as_bytes()));
        let exports = self
            .definitions
            .keys()
            .filter(|(ns, _)| ns == &namespace)
            .map(|(_, name)| name.clone())
            .collect::<Vec<_>>();
        self.session.declare_macro_exports(&namespace, &exports)?;
        Ok(value)
    }
}
impl CompiledMacros {
    pub fn artifact_cache_stats(&self) -> crate::portable::artifact_cache::CacheStats {
        self.artifact_cache.stats()
    }
    /// Expansions that materialized a caller &env analysis graph.
    pub fn environment_materializations(&self) -> u64 {
        self.environment_materializations
    }
}
impl ExpansionHost for CompiledMacros {
    fn emit_fragment(
        &mut self, function: &crate::portable::ir::Function,
        phase: crate::portable::resolve::Phase, forms: &[Form],
        origin: Option<&crate::portable::SourceOrigin>,
    ) -> Result<Vec<u8>, Diagnostic> {
        let dependencies = self.artifact_dependencies();
        self.artifact_cache.emit(function, phase, forms, origin, dependencies.as_deref())
    }
    fn artifact_dependencies(&self) -> Option<Vec<(String, String)>> {
        if !self.incomplete_sources.is_empty() { return None; }
        let mut dependencies = vec![("bootstrap".into(), crate::portable::bootstrap::sha256(crate::portable::bootstrap::SOURCE.as_bytes()))];
        dependencies.extend(self.loaded_sources.iter().map(|(name, source)| (format!("module:{name}"), source.clone())));
        dependencies.extend(self.declaration_sources.iter().map(|((namespace, name), source)| (format!("macro:{namespace}/{name}"), source.clone())));
        Some(dependencies)
    }

    fn supports_macro_imports(&self) -> bool {
        true
    }
    fn source_paths(&mut self, paths: &[std::path::PathBuf]) {
        self.session.set_source_paths(paths);
    }
    fn load_macro_namespace(
        &mut self,
        namespace: &str,
        span: std::ops::Range<usize>,
    ) -> Result<Vec<String>, Diagnostic> {
        self.load_source_namespace(namespace, crate::portable::MacroReload::Once)
            .map_err(|error| Diagnostic {
                span,
                message: format!("Compiled macro namespace loading failed: {error}"),
            })
    }

    fn load_macro_namespace_with_policy(
        &mut self,
        namespace: &str,
        policy: crate::portable::MacroReload,
        span: std::ops::Range<usize>,
    ) -> Result<Vec<String>, Diagnostic> {
        self.load_source_namespace(namespace, policy)
            .map_err(|error| Diagnostic {
                span,
                message: format!("Compiled macro namespace loading failed: {error}"),
            })
    }
    fn expand(
        &mut self,
        form: &Form,
        context: ExpansionContext<'_>,
    ) -> Result<Option<Form>, Diagnostic> {
        let Kind::List(items) = &form.kind else {
            return Ok(None);
        };
        let Some(Form {
            kind: Kind::Symbol(name),
            ..
        }) = items.first()
        else {
            return Ok(None);
        };
        if name.namespace.is_none()
            && matches!(
                name.name.as_str(),
                "quote"
                    | "if"
                    | "do"
                    | "let"
                    | "let*"
                    | "loop"
                    | "loop*"
                    | "recur"
                    | "fn"
                    | "fn*"
                    | "def"
                    | "defonce"
                    | "set!"
                    | "throw"
                    | "try"
                    | "new"
                    | "ns"
            )
        {
            return Ok(None);
        }
        let target = context
            .environment
            .resolve_source_macro(context.phase, name)
            .or_else(|| {
                // Standalone source definitions are registered in the host
                // before the Runtime snapshot has a macro-export entry.
                let own = (
                    context.environment.current_namespace(context.phase).to_owned(),
                    name.name.clone(),
                );
                (name.namespace.is_none() && self.definitions.contains_key(&own)).then_some(own)
            })
            .or_else(|| {
                context.environment
                    .resolves_bootstrap_name(context.phase, name, "defn")
                    .then(|| ("suss.core".into(), "defn".into()))
            })
            .unwrap_or_else(|| {
                (
                    name.namespace
                        .as_deref()
                        .unwrap_or(context.environment.current_namespace(context.phase))
                        .into(),
                    name.name.clone(),
                )
            });
        let Some(function) = self.definitions.get(&target).cloned() else {
            return Ok(None);
        };
        self.source_expansions += 1;
        let result = (|| -> Result<Form, SessionError> {
            let caller_data = context.origin.map_or_else(|| Ok(form.clone()), |origin| origin.macro_form_data(form))
                .map_err(SessionError::Compile)?;
            let caller_form = self.bridge.quote(&mut self.session, caller_data.clone())?;
            // A macro that cannot read &env receives nil instead of a
            // materialized analysis graph; nothing else can observe it.
            let caller_environment = if self.environment_free.contains(&target) {
                self.bridge.quote(&mut self.session, Form { span: form.span.clone(), metadata: vec![], kind: Kind::Nil })?
            } else {
                self.environment_materializations += 1;
                AnalysisGraph::with_declaration_values(
                    &self.bridge, &mut self.session, &mut self.declaration_values,
                ).expansion(context)?
            };
            let mut arguments = vec![caller_form, caller_environment];
            let Kind::List(caller_items) = &caller_data.kind else { unreachable!("matched macro call") };
            for argument in &caller_items[1..] {
                arguments.push(self.bridge.quote(&mut self.session, argument.clone())?);
            }
            let arguments = arguments.iter().collect::<Vec<_>>();
            let result = self.session.invoke(&function, &arguments)?;
            self.bridge
                .read(&mut self.session, &result, form.span.clone())
        })();
        result.map(Some).map_err(|error| Diagnostic {
            span: form.span.clone(),
            message: format!("Compiled macro expansion failed: {error}"),
        })
    }
}
