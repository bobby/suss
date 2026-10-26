//! Source macro functions execute in a separate compiled phase Store.
use crate::portable::{Diagnostic, ExpansionContext, ExpansionHost, resolve};
use crate::{
    portable_macro_data::FormBridge,
    portable_macro_graph::AnalysisGraph,
    portable_session::{Session, SessionError, SessionValue},
};
use std::collections::{BTreeMap, BTreeSet};
use suss_reader::forms::{Form, Kind, read_forms};

// Only the fixed compiler library installed into a fresh Macro Store uses this
// allowance. Return the caller's configured limit before exposing the host;
// ordinary definitions, expansions and caller Runtime execution keep that limit.
const FIXED_MACRO_INITIALIZATION_FUEL: u64 = 100_000_000_000;

#[derive(Clone, Copy)]
pub(crate) enum CompilerCatalogMode {
    Full,
    RecordBindings,
}
pub struct CompiledMacros {
    session: Session,
    bridge: FormBridge,
    definitions: BTreeMap<(String, String), SessionValue>,
    // Macros whose definitions cannot read their implicit &env parameter.
    environment_free: BTreeSet<(String, String)>,
    // Conservatively proven definitions that cannot read their implicit &form.
    caller_form_free: BTreeSet<(String, String)>,
    // Explicit source compiler helpers need cross-namespace/protocol revisions.
    // This transport policy is rolled back with the corresponding macro binding.
    compiler_catalog_macros: BTreeMap<(String, String), CompilerCatalogMode>,
    environment_materializations: u64,
    // Source macro expansions performed by this host, in any context.
    source_expansions: u64,
    declaration_sources: BTreeMap<(String, String), String>,
    loaded_sources: BTreeMap<String, String>,
    incomplete_sources: BTreeSet<String>,
    artifact_cache: crate::portable::artifact_cache::ArtifactCache,
    declaration_values: crate::portable_macro_graph::DeclarationValues,
}
// Hash the identical legacy Debug/source/path stream without retaining a copy of
// that stream. The meter error must survive fmt::Error's intentionally tiny API.
fn declaration_provenance_metered(
    form: &Form,
    origin: Option<&crate::portable::SourceOrigin>,
    charge: &mut dyn FnMut(u64) -> Result<(), SessionError>,
) -> Result<String, SessionError> {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    const CHUNK: usize = 1024;
    struct Writer<'a> {
        digest: Sha256,
        charge: &'a mut dyn FnMut(u64) -> Result<(), SessionError>,
        failure: Option<SessionError>,
    }
    impl std::fmt::Write for Writer<'_> {
        fn write_str(&mut self, text: &str) -> std::fmt::Result {
            if self.failure.is_some() {
                return Err(std::fmt::Error);
            }
            for chunk in text.as_bytes().chunks(CHUNK) {
                // Bounded byte slices do not copy. Charge before reading/hash
                // update; +1 covers the reached chunk's fixed bookkeeping.
                if let Err(error) = (self.charge)(chunk.len() as u64 + 1) {
                    self.failure = Some(error);
                    return Err(std::fmt::Error);
                }
                self.digest.update(chunk);
            }
            Ok(())
        }
    }
    charge(std::mem::size_of::<Writer<'_>>() as u64 + 1)?;
    let mut writer = Writer { digest: Sha256::new(), charge, failure: None };
    let formatted = write!(&mut writer, "{form:?}:{}:{:?}",
        origin.map_or("", |origin| origin.text()), origin.and_then(|origin| origin.path()));
    let Writer { digest, failure, .. } = writer;
    if let Some(error) = failure {
        return Err(error);
    }
    if formatted.is_err() {
        return Err(SessionError::Host(wasmtime::Error::msg(
            "Macro declaration provenance formatting failed")));
    }
    // SHA final padding/compression and the fixed64-byte lowercase hex result:
    // reserve/write/key lookup work is paid before finalization/allocation.
    charge(128 + 4 * 64 + std::mem::size_of::<String>() as u64)?;
    let mut output = String::with_capacity(64);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest.finalize() {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 15) as usize] as char);
    }
    Ok(output)
}

fn declaration_provenance_context(
    error: SessionError,
    span: &std::ops::Range<usize>,
    origin: Option<&crate::portable::SourceOrigin>,
) -> SessionError {
    let context = || match origin.and_then(|origin| origin.path()) {
        Some(path) => format!("Macro declaration provenance failed at bytes {span:?} [{}]", path.display()),
        None => format!("Macro declaration provenance failed at bytes {span:?}"),
    };
    match error {
        SessionError::Trap(error) => SessionError::Trap(error.context(context())),
        SessionError::Host(error) => SessionError::Host(error.context(context())),
        error => error,
    }
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
fn trace_fuel(session: &Session, target: &(String, String), stage: &str) -> Result<(), SessionError> {
    if std::env::var_os("SUSS_MACRO_FUEL_TRACE").is_some() {
        eprintln!("macro-fuel {}/{} {stage}: {}", target.0, target.1,
            session.operation_fuel_remaining()?);
    }
    Ok(())
}
pub(crate) type MacroCheckpoint = (
    crate::portable_session::BindingCheckpoint,
    BTreeMap<(String, String), SessionValue>,
    BTreeMap<(String, String), String>,
    BTreeSet<(String, String)>,
    BTreeMap<(String, String), CompilerCatalogMode>,
    BTreeSet<(String, String)>,
);

/// Whether the definition contains the named implicit parameter, including raw
/// reader children and metadata. Both observation scans retain their old order.
fn mentions_environment(
    form: &Form,
    charge: &mut dyn FnMut(u64) -> Result<(), SessionError>,
) -> Result<bool, SessionError> {
    mentions_parameter(form, "&env", charge)
}
fn mentions_parameter(
    form: &Form,
    name: &str,
    charge: &mut dyn FnMut(u64) -> Result<(), SessionError>,
) -> Result<bool, SessionError> {
    fn overflow() -> SessionError {
        SessionError::Host(wasmtime::Error::msg("Macro parameter observation storage overflow"))
    }
    fn append<'a>(
        pending: &mut Vec<&'a Form>,
        forms: &'a [Form],
        charge: &mut dyn FnMut(u64) -> Result<(), SessionError>,
    ) -> Result<(), SessionError> {
        let needed = pending.len().checked_add(forms.len()).ok_or_else(overflow)?;
        let bytes = forms.len().checked_mul(std::mem::size_of::<&Form>()).ok_or_else(overflow)?;
        // Bookkeeping and reference reads/copies are paid even with reused capacity.
        charge(u64::try_from(bytes).map_err(|_| overflow())?.checked_add(1).ok_or_else(overflow)?)?;
        if needed > pending.capacity() {
            let capacity = needed.max(pending.capacity().checked_mul(2).ok_or_else(overflow)?).max(4);
            let allocation = capacity.checked_mul(std::mem::size_of::<&Form>()).ok_or_else(overflow)?;
            let relocation = pending.len().checked_mul(std::mem::size_of::<&Form>()).ok_or_else(overflow)?;
            let units = allocation.checked_add(relocation).and_then(|n| n.checked_add(1)).ok_or_else(overflow)?;
            // Same byte allocation/relocation convention as other native work
            // vectors; the fixed unit prepays allocation/cleanup bookkeeping.
            charge(u64::try_from(units).map_err(|_| overflow())?)?;
            pending.try_reserve_exact(capacity - pending.len()).map_err(|_| {
                SessionError::Host(wasmtime::Error::msg("Macro parameter observation allocation failed"))
            })?;
        }
        pending.extend(forms.iter());
        Ok(())
    }
    charge(std::mem::size_of::<Vec<&Form>>() as u64 + 1)?;
    let mut pending = Vec::new();
    append(&mut pending, std::slice::from_ref(form), charge)?;
    while !pending.is_empty() {
        charge(1)?;
        let form = pending.pop().unwrap();
        // Old code enqueued metadata before testing this node. Preserve that,
        // including allocation/error order when a symbol itself is a match.
        append(&mut pending, &form.metadata, charge)?;
        match &form.kind {
            Kind::Symbol(symbol) => {
                charge(1)?;
                if symbol.name.len() == name.len() {
                    let bytes = symbol.name.len().checked_mul(2).ok_or_else(overflow)?;
                    charge(u64::try_from(bytes).map_err(|_| overflow())?)?;
                    if symbol.name == name { return Ok(true); }
                }
            }
            Kind::List(items)
            | Kind::Vector(items)
            | Kind::Set(items)
            | Kind::Map(items)
            | Kind::Conditional(items)
            | Kind::SplicingConditional(items) => append(&mut pending, items, charge)?,
            Kind::Discard(target) => append(&mut pending, std::slice::from_ref(target.as_ref()), charge)?,
            Kind::Prefix { operator, target } => {
                append(&mut pending, std::slice::from_ref(operator.as_ref()), charge)?;
                append(&mut pending, std::slice::from_ref(target.as_ref()), charge)?;
            }
            _ => {}
        }
    }
    Ok(false)
}
fn declaration_observation_context(
    error: SessionError,
    span: &std::ops::Range<usize>,
    name: &str,
) -> SessionError {
    let context = || format!("Macro declaration parameter observation ({name}) failed at bytes {span:?}");
    match error {
        SessionError::Trap(error) => SessionError::Trap(error.context(context())),
        SessionError::Host(error) => SessionError::Host(error.context(context())),
        error => error,
    }
}

#[cfg(test)]
#[test]
fn environment_observation_traverses_raw_splicing_children_and_metadata() {
    let mut form = Form {
        span: 0..1,
        metadata: vec![],
        kind: Kind::SplicingConditional(vec![symbol("&env", 0..1)]),
    };
    assert!(mentions_environment(&form, &mut |_| Ok(())).unwrap());
    form.kind = Kind::SplicingConditional(vec![symbol("ordinary", 0..1)]);
    assert!(!mentions_environment(&form, &mut |_| Ok(())).unwrap());
    form.metadata.push(symbol("&env", 0..1));
    assert!(mentions_environment(&form, &mut |_| Ok(())).unwrap());
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
        // The two callers supply newly created/replacement Stores, never an
        // active user operation. No caller source or prior guest state is run.
        let caller_fuel = session.options().fuel_per_operation;
        session.set_initialization_fuel(FIXED_MACRO_INITIALIZATION_FUEL)?;
        let bridge = FormBridge::new(&mut session)?;
        let mut macros = Self {
            session,
            bridge,
            definitions: BTreeMap::new(),
            environment_free: BTreeSet::new(),
            caller_form_free: BTreeSet::new(),
            compiler_catalog_macros: BTreeMap::new(),
            environment_materializations: 0,
            source_expansions: 0,
            declaration_sources: BTreeMap::new(),
            loaded_sources: BTreeMap::new(),
            incomplete_sources: BTreeSet::new(),
            artifact_cache: Default::default(),
            declaration_values: Default::default(),
        };
        macros.enter_namespace("suss.core")?;
        macros.define(crate::portable_defn::SOURCE)?;
        macros.define(crate::portable_when_let::SOURCE)?;
        macros.define(crate::portable_lazy_seq::SOURCE)?;
        macros
            .session
            .eval_fixed_initialization_source(crate::portable_record_factories::SOURCE)?;
        macros.session.eval_fixed_initialization_source(crate::portable_record_label::SOURCE)?;
        macros
            .session
            .eval_fixed_initialization_source(crate::portable_record_protocol_facts::SOURCE)?;
        macros
            .session
            .eval_fixed_initialization_source(crate::portable_record_emitter::SOURCE)?;
        macros.session.eval_fixed_initialization_source(crate::portable_record_public::SOURCE)?;
        macros.enter_namespace("suss.core")?;
        macros.define(crate::portable_record_public::MACRO_SOURCE)?;
        macros.enter_namespace("user")?;
        // Bootstrap code is already rooted in the Store. User source cache
        // accounting starts empty, including after an explicit session reset.
        macros.artifact_cache = Default::default();
        macros.session.set_initialization_fuel(caller_fuel)?;
        Ok(macros)
    }
    /// Source normalization needs a rollback identity, not a fresh bootstrap
    /// Environment. Share the caller's actual catalog; debit any new owner to
    /// the Macro operation (a nested active operation is never replenished).
    pub(crate) fn checkpoint_global(&mut self, environment: &resolve::Environment, namespace: &str, name: &str) -> Result<resolve::Global, SessionError> {
        let _budget = self.session.operation_budget_scope()?;
        let mut host_error = None;
        let result = environment.checkpoint_global_metered(resolve::Phase::Macro, namespace, name, &mut |work| {
            self.session.charge_host_work(work).map_err(|error| {
                host_error = Some(error);
                Diagnostic { span: 0..0, message: "Macro checkpoint identity budget exhausted".into() }
            })
        });
        if let Some(error) = host_error { return Err(error); }
        result.map_err(SessionError::Compile)
    }
    pub(crate) fn binding_checkpoint(&mut self) -> Result<MacroCheckpoint, SessionError> {
        Ok((
            self.session.binding_checkpoint()?,
            self.definitions.clone(),
            self.declaration_sources.clone(),
            self.environment_free.clone(),
            self.compiler_catalog_macros.clone(),
            self.caller_form_free.clone(),
        ))
    }
    pub(crate) fn restore_bindings(
        &mut self,
        checkpoint: MacroCheckpoint,
        globals: &[crate::portable::resolve::Global],
    ) -> Result<(), SessionError> {
        self.session.restore_bindings(checkpoint.0, globals)?;
        for global in globals {
            let key = (global.namespace().to_owned(), global.name().to_owned());
            if let Some(value) = checkpoint.1.get(&key) {
                self.definitions.insert(key.clone(), value.clone());
            } else {
                self.definitions.remove(&key);
            }
            if checkpoint.3.contains(&key) {
                self.environment_free.insert(key.clone());
            } else {
                self.environment_free.remove(&key);
            }
            if checkpoint.5.contains(&key) {
                self.caller_form_free.insert(key.clone());
            } else {
                self.caller_form_free.remove(&key);
            }
            if let Some(mode) = checkpoint.4.get(&key) {
                self.compiler_catalog_macros.insert(key.clone(), *mode);
            } else {
                self.compiler_catalog_macros.remove(&key);
            }
            if let Some(source) = checkpoint.2.get(&key) {
                self.declaration_sources.insert(key, source.clone());
            } else {
                self.declaration_sources.remove(&key);
            }
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
        let _budget = self.session.operation_budget_scope()?;
        self.session.charge_host_work(source.len() as u64)?;
        let forms = suss_reader::forms::read_forms_metered(source, &mut |work| {
            self.session.charge_host_work(work).map_err(|error| suss_reader::ParseError {
                span: 0..source.len(), message: error.to_string(), expected: vec![],
            })
        }).map_err(|error| {
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
        self.define_form_with_origin(
            forms.into_iter().next().unwrap(),
            0..source.len(),
            Some(&crate::portable::SourceOrigin::new(source, None)),
        )
        .map(|_| ())
    }
    fn load_source_namespace(
        &mut self,
        namespace: &str,
        policy: crate::portable::MacroReload,
    ) -> Result<Vec<String>, SessionError> {
        use crate::portable::{modules, resolve::Phase};
        let _budget = self.session.operation_budget_scope()?;
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
        let graph = modules::discover_phase_modules_metered(
            namespace,
            &snapshot.source_paths,
            &snapshot.environment,
            Phase::Macro,
            &snapshot.provided,
            &mut |work| self.session.charge_host_work(work).map_err(|error| Diagnostic {
                span: 0..0,
                message: format!("Namespace discovery budget exhausted: {error}"),
            }),
        )
        .map_err(SessionError::Module)?;
        // Discovery errors preserve previous loaded identities. Once execution
        // starts, each selected phase unit must initialize before being loaded again.
        // A selected unit may publish cells before its initializer fails. Its old
        // immutable identity no longer establishes the complete live graph, so
        // emission reuse stays disabled until all selected units finish loading.
        self.incomplete_sources.extend(
            graph
                .iter()
                .map(|unit| unit.identity.namespace().to_owned()),
        );
        self.session.invalidate_source_modules(
            &graph
                .iter()
                .map(|unit| unit.identity.clone())
                .collect::<Vec<_>>(),
        );
        let caller = self.session.current_namespace().to_owned();
        let result = (|| {
            for unit in graph {
                self.session.charge_host_work(unit.source.len() as u64)?;
                let source_identity = crate::portable::bootstrap::sha256(
                    format!("{:?}:{:?}:{}", unit.path, unit.dependencies, unit.source).as_bytes(),
                );
                let origin = crate::portable::SourceOrigin::new(
                    unit.source.as_str(),
                    Some(unit.path.clone()),
                );
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
                        let prepared = self.session.compilation_snapshot().prepare_with_origin(
                            vec![form],
                            span,
                            self,
                            Some(&origin),
                        )?;
                        self.session.eval_prepared(prepared)?;
                    }
                }
                self.session
                    .initialized_source_namespace(unit.identity.namespace())?;
                self.loaded_sources
                    .insert(unit.identity.namespace().to_owned(), source_identity);
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
        &mut self,
        form: Form,
        span: std::ops::Range<usize>,
        origin: Option<&crate::portable::SourceOrigin>,
    ) -> Result<SessionValue, SessionError> {
        let _budget = self.session.operation_budget_scope()?;
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
        let metadata = crate::portable_macro_data::normalized_reader_metadata(&mut self.session, &name_form)?;
        let mut compiler_catalog = None;
        for pair in metadata.chunks_exact(2) {
            if matches!(&pair[0].kind, Kind::Keyword(key) if key.namespace.as_deref() == Some("suss") && key.name == "compiler-catalog")
            {
                match &pair[1].kind {
                    Kind::Bool(enabled) => {
                        compiler_catalog = enabled.then_some(CompilerCatalogMode::Full)
                    }
                    Kind::Nil => compiler_catalog = None,
                    Kind::Keyword(key)
                        if key.namespace.is_none() && key.name == "record-bindings" =>
                    {
                        compiler_catalog = Some(CompilerCatalogMode::RecordBindings)
                    }
                    _ => {
                        return Err(failure(
                            &pair[1],
                            "Compiler catalog transport metadata must be Boolean or nil",
                        ));
                    }
                }
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
        let reads_environment = mentions_environment(form,
            &mut |work| self.session.charge_host_work(work))
            .map_err(|error| declaration_observation_context(error, &form.span, "&env"))?
            || self.source_expansions != expansions;
        let reads_caller_form = mentions_parameter(form, "&form",
            &mut |work| self.session.charge_host_work(work))
            .map_err(|error| declaration_observation_context(error, &form.span, "&form"))?
            || self.source_expansions != expansions;
        // Complete all fallible provenance work before publishing this
        // replacement. Effects already executed during preparation are retained.
        let provenance = declaration_provenance_metered(form, origin,
            &mut |work| self.session.charge_host_work(work))
            .map_err(|error| declaration_provenance_context(error, &form.span, origin))?;
        let value = self.session.eval_prepared(prepared)?;
        let namespace = self.session.current_namespace().to_owned();
        let key = (namespace.clone(), name.name.clone());
        if reads_caller_form {
            self.caller_form_free.remove(&key);
        } else {
            self.caller_form_free.insert(key.clone());
        }
        if reads_environment || compiler_catalog.is_some() {
            self.environment_free.remove(&key);
        } else {
            self.environment_free.insert(key.clone());
        }
        if let Some(mode) = compiler_catalog {
            self.compiler_catalog_macros.insert(key.clone(), mode);
        } else {
            self.compiler_catalog_macros.remove(&key);
        }
        self.definitions.insert(key, value.clone());
        self.declaration_sources.insert(
            (namespace.clone(), name.name.clone()),
            provenance,
        );
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
    fn expansion_work_fuel(&self) -> u64 {
        self.session.options().fuel_per_operation
    }
    fn begin_expansion_request(&mut self, span: std::ops::Range<usize>) -> Result<(), Diagnostic> {
        self.session.begin_operation_budget().map_err(|error| Diagnostic {
            span,
            message: format!("Unable to start macro expansion budget: {error}"),
        })
    }
    fn end_expansion_request(&mut self) {
        self.session.end_operation_budget();
    }

    fn charge_expansion_work(&mut self, work: u64, span: std::ops::Range<usize>) -> Result<(), Diagnostic> {
        self.session.charge_host_work(work).map_err(|error| Diagnostic {
            span,
            message: format!("Macro expansion analysis budget exhausted: {error}"),
        })
    }

    fn emit_fragment(
        &mut self,
        function: &crate::portable::ir::Function,
        phase: crate::portable::resolve::Phase,
        forms: &[Form],
        origin: Option<&crate::portable::SourceOrigin>,
    ) -> Result<Vec<u8>, Diagnostic> {
        let dependencies = self.artifact_dependencies();
        let session = &mut self.session;
        self.artifact_cache.emit_metered(
            function, phase, forms, origin, dependencies.as_deref(),
            &mut |work| session.charge_host_work(work).map_err(|error| Diagnostic {
                span: function.span.clone(),
                message: format!("Artifact preparation budget exhausted: {error}"),
            }),
        )
    }
    fn artifact_dependencies(&self) -> Option<Vec<(String, String)>> {
        if !self.incomplete_sources.is_empty() {
            return None;
        }
        let mut dependencies = vec![(
            "bootstrap".into(),
            crate::portable::bootstrap::sha256(crate::portable::bootstrap::SOURCE.as_bytes()),
        )];
        dependencies.push((
            "record-factory-source".into(),
            crate::portable::bootstrap::sha256(crate::portable_record_factories::SOURCE.as_bytes()),
        ));
        dependencies.push((
            "record-label-source".into(),
            crate::portable::bootstrap::sha256(crate::portable_record_label::SOURCE.as_bytes()),
        ));
        dependencies.push((
            "record-protocol-facts-source".into(),
            crate::portable::bootstrap::sha256(
                crate::portable_record_protocol_facts::SOURCE.as_bytes(),
            ),
        ));
        dependencies.push((
            "record-emitter-source".into(),
            crate::portable::bootstrap::sha256(crate::portable_record_emitter::SOURCE.as_bytes()),
        ));
        dependencies.push((
            "record-public-source".into(),
            crate::portable::bootstrap::sha256(crate::portable_record_public::SOURCE.as_bytes()),
        ));
        dependencies.extend(
            self.loaded_sources
                .iter()
                .map(|(name, source)| (format!("module:{name}"), source.clone())),
        );
        dependencies.extend(
            self.declaration_sources
                .iter()
                .map(|((namespace, name), source)| {
                    (format!("macro:{namespace}/{name}"), source.clone())
                }),
        );
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
                    context
                        .environment
                        .current_namespace(context.phase)
                        .to_owned(),
                    name.name.clone(),
                );
                (name.namespace.is_none() && self.definitions.contains_key(&own)).then_some(own)
            })
            .or_else(|| {
                ["defn", "defrecord", "lazy-seq", "when-let"]
                    .into_iter()
                    .find_map(|core_name| {
                        context
                            .environment
                            .resolves_bootstrap_name(context.phase, name, core_name)
                            .then(|| ("suss.core".into(), core_name.into()))
                    })
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
        let mut stage = String::from("caller form provenance");
        let result = self.session.with_operation_budget(|session| -> Result<Form, SessionError> {
            trace_fuel(session, &target, "start")?;
            let mut meter = |work| {
                    session.charge_host_work(work).map_err(|error| Diagnostic {
                        span: form.span.clone(),
                        message: error.to_string(),
                    })
                };
            let caller_data = match context.origin {
                Some(origin) => origin.macro_form_data_metered(form, &mut meter),
                None => crate::portable::clone_form_metered(form, &mut meter),
            }.map_err(SessionError::Compile)?;
            trace_fuel(session, &target, "provenance")?;
            stage = "caller &form input transport".into();
            // Keep complete provenance for explicit arguments and &env. Only
            // the unused implicit parameter can be replaced with nil. Any
            // source expansion while compiling the body disables this proof.
            let caller_form = if self.caller_form_free.contains(&target) {
                self.bridge.quote(session, Form { span: form.span.clone(), metadata: vec![], kind: Kind::Nil })?
            } else {
                self.bridge.quote_ref(session, &caller_data)?
            };
            trace_fuel(session, &target, "caller-form")?;
            // A macro that cannot read &env receives nil instead of a
            // materialized analysis graph; nothing else can observe it.
            stage = "caller &env input transport".into();
            let caller_environment = if self.environment_free.contains(&target) {
                self.bridge.quote(
                    session,
                    Form {
                        span: form.span.clone(),
                        metadata: vec![],
                        kind: Kind::Nil,
                    },
                )?
            } else {
                self.environment_materializations += 1;
                let mut graph = AnalysisGraph::with_declaration_values(&self.bridge, session, &mut self.declaration_values);
                match self.compiler_catalog_macros.get(&target) {
                    Some(CompilerCatalogMode::Full) => {
                        graph.expansion_with_compiler_catalog(context)?
                    }
                    Some(CompilerCatalogMode::RecordBindings) => {
                        graph.expansion_with_record_catalog(context)?
                    }
                    None => graph.expansion(context)?,
                }
            };
            let mut arguments = vec![caller_form, caller_environment];
            trace_fuel(session, &target, "caller-environment")?;
            let Kind::List(caller_items) = &caller_data.kind else {
                unreachable!("matched macro call")
            };
            for (index, argument) in caller_items[1..].iter().enumerate() {
                stage = format!("explicit argument {} input transport", index + 1);
                arguments.push(self.bridge.quote_ref(session, argument)?);
            }
            trace_fuel(session, &target, "arguments")?;
            let arguments = arguments.iter().collect::<Vec<_>>();
            stage = "compiled macro invocation".into();
            let result = session.invoke(&function, &arguments)?;
            trace_fuel(session, &target, "invoked")?;
            stage = "expanded result transport".into();
            let expanded = self.bridge.read(session, &result, form.span.clone())?;
            trace_fuel(session, &target, "decoded")?;
            Ok(expanded)
        });
        result.map(Some).map_err(|error| Diagnostic {
            span: form.span.clone(),
            message: format!("Compiled macro expansion failed during {stage}: {error}"),
        })
    }
}

#[cfg(test)]
mod memo_leaf_tests;


#[cfg(test)]
mod initialization_budget_tests {
    use super::*;

    #[test]
    fn fixed_source_initialization_preserves_cumulative_fuel_interrupts_and_recovery() {
        // Measure the cumulative cost in identical fresh Sessions, then leave
        // half of it, so exhaustion happens well inside the initialization,
        // independent of the compiler's current costs. The second probe sees
        // the warm process artifact cache, as does the Session under test.
        let measure = || {
            let mut probe = Session::new_macro().unwrap();
            let _operation = probe.operation_budget_scope().unwrap();
            let before = probe.operation_fuel_remaining().unwrap();
            probe.eval_fixed_initialization_source("(def initialization-unpublished 23)").unwrap();
            before - probe.operation_fuel_remaining().unwrap()
        };
        measure();
        let needed = measure();
        assert!(needed > 1);
        let mut session = Session::new_macro().unwrap();
        {
            let _operation = session.operation_budget_scope().unwrap();
            let remaining = session.operation_fuel_remaining().unwrap();
            session.charge_host_work(remaining - needed / 2).unwrap();
            let error = session.eval_fixed_initialization_source(
                "(def initialization-unpublished 23)",
            ).unwrap_err();
            assert!(matches!(error, SessionError::Trap(ref trap)
                if trap.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
            assert_eq!(session.operation_fuel_remaining().unwrap(), 0);
        }
        assert!(matches!(session.eval("initialization-unpublished"), Err(SessionError::Compile(_))));
        {
            let _operation = session.operation_budget_scope().unwrap();
            session.host_work_interrupt().store(true, std::sync::atomic::Ordering::SeqCst);
            let error = session.eval_fixed_initialization_source(
                "(def initialization-unpublished 23)",
            ).unwrap_err();
            assert!(error.is_interrupt(), "{error:?}");
            assert_eq!(session.operation_fuel_remaining().unwrap(), 0);
        }
        session.eval_fixed_initialization_source("(def initialization-recovered 23)").unwrap();
        let value = session.eval("initialization-recovered").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let form = bridge.read(&mut session, &value, 0..1).unwrap();
        assert!(matches!(form.kind, Kind::Number(23.0)));
        assert_eq!(session.options().fuel_per_operation, 10_000_000);
    }

    #[test]
    fn fixed_macro_initialization_restores_default_and_replacement_caller_limits() {
        let mut macros = CompiledMacros::new().unwrap();
        assert_eq!(macros.session.options().fuel_per_operation, 10_000_000);
        assert_eq!(macros.session.operation_fuel_remaining().unwrap(), 10_000_000);
        {
            let _operation = macros.session.operation_budget_scope().unwrap();
            let remaining = macros.session.operation_fuel_remaining().unwrap();
            assert!(macros.session.set_initialization_fuel(FIXED_MACRO_INITIALIZATION_FUEL).is_err());
            assert_eq!(macros.session.operation_fuel_remaining().unwrap(), remaining);
            assert_eq!(macros.session.options().fuel_per_operation, 10_000_000);
        }
        macros.set_operation_fuel(1);
        let error = macros.define("(defmacro initialization-limit-probe [] 17)").unwrap_err();
        assert!(matches!(error, SessionError::Trap(ref trap)
            if trap.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
        assert_eq!(macros.session.options().fuel_per_operation, 1);
        assert!(!macros.definitions.contains_key(
            &("user".to_owned(), "initialization-limit-probe".to_owned())
        ));

        macros.set_operation_fuel(10_000_000);
        macros.define("(defmacro initialization-limit-probe [] 17)").unwrap();
        let mut caller = Session::new_repl().unwrap();
        let value = caller.eval_with_macros("(initialization-limit-probe)", &mut macros).unwrap();
        let bridge = FormBridge::new(&mut caller).unwrap();
        let form = bridge.read(&mut caller, &value, 0..1).unwrap();
        assert!(matches!(form.kind, Kind::Number(17.0)));
        assert_eq!(caller.options().fuel_per_operation, 10_000_000);

        // Replacement has a fresh Store, but must preserve the configured
        // caller allowance rather than returning the fixed setup allowance.
        macros.set_operation_fuel(77_777_777);
        let mut replacement = macros.replacement().unwrap();
        assert_eq!(macros.session.options().fuel_per_operation, 77_777_777);
        assert_eq!(replacement.session.options().fuel_per_operation, 77_777_777);
        assert_eq!(replacement.session.operation_fuel_remaining().unwrap(), 77_777_777);
        replacement.set_operation_fuel(1);
        let error = replacement.define("(defmacro replacement-limit-probe [] 23)").unwrap_err();
        assert!(matches!(error, SessionError::Trap(ref trap)
            if trap.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
        assert!(!replacement.definitions.contains_key(
            &("user".to_owned(), "replacement-limit-probe".to_owned())
        ));
    }
}

#[cfg(test)]
mod when_let_validation_tests {
    use super::*;

    #[test]
    fn compiled_when_let_validation_matches_pinned_messages_and_phase_after_gc() {
        let reference: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/oracle/when-let-validation-observations.json"
        )).unwrap();
        assert_eq!(reference["upstream"], "c4295f303100bbf5afac449242d30bca1126f1a1");
        let mut macros = CompiledMacros::new().unwrap();
        let function = macros.definitions[&("suss.core".to_owned(), "when-let".to_owned())].clone();
        let capture = macros.session.eval(r#"
          (fn [expand form bindings]
            (try (expand form nil bindings 1)
              (catch :default problem
                [(ex-message problem) (get (ex-data problem) :clojure.error/phase)])))
        "#).unwrap();
        let cases = reference["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 5);
        for case in cases {
            let source = case["source"].as_str().unwrap();
            let form = read_forms(source).unwrap().remove(0);
            let Kind::List(items) = &form.kind else { panic!("oracle macro call") };
            let bindings = macros.bridge.quote(&mut macros.session, items[1].clone()).unwrap();
            let form = macros.bridge.quote(&mut macros.session, form).unwrap();
            let value = macros.session.invoke(&capture, &[&function, &form, &bindings]).unwrap();
            macros.session.collect().unwrap();
            let observed = macros.bridge.read(&mut macros.session, &value, 0..source.len()).unwrap();
            let Kind::Vector(fields) = &observed.kind else { panic!("validation result") };
            assert_eq!(fields.len(), 2);
            let Kind::String(message) = &fields[0].kind else { panic!("exception message") };
            assert_eq!(String::from_utf16(message).unwrap(), case["message"].as_str().unwrap());
            assert!(matches!(&fields[1].kind, Kind::Keyword(key)
                if key.namespace.is_none() && key.name == "macro-syntax-check"));
            assert_eq!(case["phase"], ":macro-syntax-check");
            let recovered = macros.session.eval("(+ 20 22)").unwrap();
            let recovered = macros.bridge.read(&mut macros.session, &recovered, 0..1).unwrap();
            assert!(matches!(recovered.kind, Kind::Number(n) if n == 42.0));
        }
    }
}

#[cfg(test)]
mod declaration_provenance_tests {
    use super::*;
    use crate::portable::SourceOrigin;

    fn legacy(form: &Form, origin: Option<&SourceOrigin>) -> String {
        crate::portable::bootstrap::sha256(format!("{form:?}:{}:{:?}",
            origin.map_or("", |origin| origin.text()), origin.and_then(|origin| origin.path()))
            .as_bytes())
    }
    fn small() -> Form { Form { span: 2..7, metadata: vec![], kind: Kind::Nil } }
    fn check_digest(form: &Form, origin: Option<&SourceOrigin>) {
        let mut work = 0u64;
        let got = declaration_provenance_metered(form, origin, &mut |units| {
            work = work.checked_add(units).unwrap(); Ok(())
        }).unwrap();
        assert_eq!(got, legacy(form, origin));
        assert_eq!(got.len(), 64);
        assert!(work > 0);
    }
    #[test]
    fn declaration_provenance_stream_matches_legacy_rich_deep_and_large_bytes() {
        let source = "(defmacro ^{:doc \"λ雪\" :private true} rich \"doc\\ntext\" [x] (list 'quote ^{:tag Thing} x))";
        let rich = read_forms(source).unwrap().remove(0);
        let origin = SourceOrigin::new(source, Some("dir/λ\\quoted\"/source.sus".into()));
        let no_path = SourceOrigin::new(source, None);
        for origin in [None, Some(&origin), Some(&no_path)] { check_digest(&rich, origin); }
        let mut deep = small();
        for _ in 0..256 {
            deep = Form { span: 3..11, metadata: vec![], kind: Kind::Vector(vec![deep]) };
        }
        check_digest(&deep, None);
        let large = Form { span: 0..1, metadata: vec![rich],
            kind: Kind::String([vec![0xd800, 0, 0xd83d, 0xde00], vec![65; 8192]].concat()) };
        let text = format!("{}\r\nλ", "p".repeat(16384));
        let origin = SourceOrigin::new(text.as_str(), Some("large.sus".into()));
        check_digest(&large, Some(&origin));
        // Path Debug is intentionally the native formatter, including non-UTF8.
        #[cfg(unix)] {
            use std::os::unix::ffi::OsStringExt;
            let path = std::ffi::OsString::from_vec(vec![b'x', 0xff, b'"', b'\\']);
            let origin = SourceOrigin::new("source", Some(path.into()));
            check_digest(&small(), Some(&origin));
        }
    }
    #[test]
    fn declaration_provenance_low_fuel_and_midstream_irq_keep_typed_errors_and_recover() {
        let mut session = Session::new_macro().unwrap();
        let form = small();
        let large_text = "x".repeat(16384);
        let origin = SourceOrigin::new(large_text.as_str(), None);
        // Same1000-unit policy: large source fails, the small exact digest fits.
        session.set_operation_fuel(1000);
        {
            let _scope = session.operation_budget_scope().unwrap();
            let error = declaration_provenance_metered(&form, Some(&origin),
                &mut |units| session.charge_host_work(units)).unwrap_err();
            let error = declaration_provenance_context(error, &form.span, Some(&origin));
            assert!(error.to_string().contains("Macro declaration provenance failed"));
            assert!(matches!(error, SessionError::Trap(ref error)
                if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{error:?}");
        }
        {
            let _scope = session.operation_budget_scope().unwrap();
            let got = declaration_provenance_metered(&form, None,
                &mut |units| session.charge_host_work(units)).unwrap();
            assert_eq!(got, legacy(&form, None));
        }
        assert_eq!(session.options().fuel_per_operation, 1000);
        // Raise only the explicit cancellation flag INSIDE an active scope,
        // after a real formatting/hash prefix; preserve the configured allowance.
        session.set_operation_fuel(100_000);
        {
            let _scope = session.operation_budget_scope().unwrap();
            let interrupt = session.interrupt_handle();
            let mut total = 0;
            let mut raised = false;
            let error = declaration_provenance_metered(&form, Some(&origin), &mut |units| {
                if total >= 2048 && !raised { interrupt.interrupt(); raised = true; }
                session.charge_host_work(units)?;
                total += units; Ok(())
            }).unwrap_err();
            assert!(raised && total >= 2048);
            let error = declaration_provenance_context(error, &form.span, Some(&origin));
            assert!(error.is_interrupt(), "fmt/context must retain the Session interrupt: {error:?}");
            assert_eq!(error.to_string(), "Interrupted");
            let SessionError::Trap(ref trap) = error else { panic!("typed interrupt") };
            assert_eq!(trap.downcast_ref::<wasmtime::Trap>(), Some(&wasmtime::Trap::Interrupt));
            assert!(format!("{trap:#}").contains("Macro declaration provenance failed"));
        }
        {
            let _scope = session.operation_budget_scope().unwrap();
            assert_eq!(declaration_provenance_metered(&form, Some(&origin),
                &mut |units| session.charge_host_work(units)).unwrap(), legacy(&form, Some(&origin)));
        }
        assert_eq!(session.options().fuel_per_operation, 100_000);
    }
    #[test]
    fn failed_macro_replacement_budget_preserves_old_definition_and_catalog_policy() {
        let mut macros = CompiledMacros::new().unwrap();
        macros.define("(defmacro retained-policy [] 17)").unwrap();
        let key = ("user".to_owned(), "retained-policy".to_owned());
        let old_hash = macros.declaration_sources[&key].clone();
        assert!(macros.environment_free.contains(&key));
        assert!(macros.caller_form_free.contains(&key));
        assert!(!macros.compiler_catalog_macros.contains_key(&key));
        let replacement = "(defmacro ^{:suss/compiler-catalog true} retained-policy [] 99)";
        let form = read_forms(replacement).unwrap().remove(0);
        let text = format!("{replacement}\n;{}", "x".repeat(2_100_000));
        let origin = SourceOrigin::new(text.as_str(), Some("replacement.sus".into()));
        // Cold source metadata has its own legitimate reader work. Pay that
        // setup using the real default-budget Session before isolating the
        // per-declaration hash, then share only the immutable native snapshot.
        {
            let _scope = macros.session.operation_budget_scope().unwrap();
            let before = macros.session.operation_fuel_remaining().unwrap();
            origin.reader_tree_metered(&form, &mut |work| {
                macros.session.charge_host_work(work).map_err(|error| Diagnostic {
                    span: form.span.clone(), message: error.to_string(),
                })
            }).unwrap();
            eprintln!("provenance paid-source-setup={} default={}",
                before - macros.session.operation_fuel_remaining().unwrap(),
                macros.session.options().fuel_per_operation);
        }
        macros.set_operation_fuel(2_000_000);
        let error = macros.define_form_with_origin(form, 0..replacement.len(), Some(&origin)).unwrap_err();
        assert!(matches!(error, SessionError::Trap(ref error)
            if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
            "only a typed trap is accepted: {error:?}");
        let message = error.to_string();
        assert!(message.contains("Macro declaration provenance failed"),
            "an unrelated preparation failure is not provenance coverage: {message}");
        assert!(message.contains("replacement.sus"), "{message}");
        assert!(message.contains(&format!("bytes {:?}", 0..replacement.len())), "{message}");
        assert_eq!(macros.declaration_sources[&key], old_hash);
        assert!(macros.environment_free.contains(&key));
        assert!(macros.caller_form_free.contains(&key));
        assert!(!macros.compiler_catalog_macros.contains_key(&key));
        // Actual public expansion of the old definition at the SAME2M policy.
        let mut caller = Session::new_repl().unwrap();
        let value = caller.eval_with_macros("(retained-policy)", &mut macros).unwrap();
        let bridge = FormBridge::new(&mut caller).unwrap();
        assert_eq!(bridge.read(&mut caller, &value, 0..1).unwrap().kind, Kind::Number(17.0));
        assert_eq!(macros.expansion_work_fuel(), 2_000_000);
    }
}


#[cfg(test)]
mod lazy_result_interrupt_tests;

#[cfg(test)]
mod parameter_observation_tests {
    use super::*;

    fn form(kind: Kind) -> Form { Form { span: 0..1, metadata: vec![], kind } }
    fn nil() -> Form { form(Kind::Nil) }
    fn vector(items: Vec<Form>) -> Form {
        Form { span: 0..1, metadata: vec![], kind: Kind::Vector(items) }
    }
    fn out_of_fuel(error: &SessionError) -> bool {
        matches!(error, SessionError::Trap(error)
            if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel))
    }
    #[test]
    fn parameter_observation_retains_raw_children_metadata_and_name_matching() {
        let fixtures = [
            (symbol("&env", 0..1), true, false),
            (form(Kind::Symbol(suss_reader::Symbol { namespace: Some("qualified".into()), name: "&env".into() })), true, false),
            (symbol("&env-suffix", 0..1), false, false),
            (form(Kind::SplicingConditional(vec![symbol("&form", 0..1)])), false, true),
            (form(Kind::Conditional(vec![symbol("&env", 0..1)])), true, false),
            (form(Kind::Discard(Box::new(symbol("&env", 0..1)))), true, false),
            (form(Kind::Prefix { operator: Box::new(symbol("&form", 0..1)), target: Box::new(nil()) }), false, true),
            (form(Kind::Prefix { operator: Box::new(nil()), target: Box::new(symbol("&env", 0..1)) }), true, false),
            (Form { span: 0..1, metadata: vec![symbol("&env", 0..1)], kind: Kind::Nil }, true, false),
            (form(Kind::Map(vec![symbol("&form", 0..1), nil()])), false, true),
            (form(Kind::Set(vec![symbol("&env", 0..1)])), true, false),
            (form(Kind::List(vec![symbol("&form", 0..1)])), false, true),
        ];
        let mut session = Session::new().unwrap();
        for (input, env, caller_form) in fixtures {
            let _scope = session.operation_budget_scope().unwrap();
            assert_eq!(mentions_environment(&input, &mut |n| session.charge_host_work(n)).unwrap(), env);
            assert_eq!(mentions_parameter(&input, "&form", &mut |n| session.charge_host_work(n)).unwrap(), caller_form);
        }
        // Both have identical metadata and child-reference storage. LIFO visits
        // the matching final child first; its preceding large sibling is not scanned.
        let long = vector((0..1024).map(|_| nil()).collect());
        let early = vector(vec![long.clone(), symbol("&env", 0..1)]);
        let late = vector(vec![symbol("&env", 0..1), long]);
        let mut costs = Vec::new();
        for form in [early, late] {
            let _scope = session.operation_budget_scope().unwrap();
            let before = session.operation_fuel_remaining().unwrap();
            assert!(mentions_environment(&form, &mut |n| session.charge_host_work(n)).unwrap());
            costs.push(before - session.operation_fuel_remaining().unwrap());
        }
        assert!(costs[0] > 0 && costs[1] > costs[0], "preserve LIFO short circuit: {costs:?}");
    }

    #[test]
    fn parameter_observation_real_counter_fuel_irq_and_same_budget_recovery() {
        let small = vector(vec![symbol("ordinary", 0..1)]);
        let mut session = Session::new().unwrap();
        session.set_operation_fuel(1_000);
        {
            let _scope = session.operation_budget_scope().unwrap();
            let initial = session.operation_fuel_remaining().unwrap();
            assert!(!mentions_environment(&small, &mut |n| session.charge_host_work(n)).unwrap());
            let first = session.operation_fuel_remaining().unwrap();
            assert!(!mentions_environment(&small, &mut |n| session.charge_host_work(n)).unwrap());
            let second = session.operation_fuel_remaining().unwrap();
            assert!(second < first && first < initial, "actual same-Store counters: {initial}/{first}/{second}");
            eprintln!("parameter-observation fuel initial={initial} first={first} second={second}");
            let mut completed = 2;
            let error = loop {
                match mentions_environment(&small, &mut |n| session.charge_host_work(n)) {
                    Ok(false) => { completed += 1; assert!(completed < 128); }
                    Ok(true) => panic!("no implicit parameter"),
                    Err(error) => break error,
                }
            };
            assert!(out_of_fuel(&error), "{error:?}");
            assert!(completed > 2);
            assert_eq!(session.operation_fuel_remaining().unwrap(), 0);
        }
        {
            let _scope = session.operation_budget_scope().unwrap();
            assert!(!mentions_environment(&small, &mut |n| session.charge_host_work(n)).unwrap());
        }
        {
            let _scope = session.operation_budget_scope().unwrap();
            assert!(!mentions_environment(&small, &mut |n| session.charge_host_work(n)).unwrap());
            session.interrupt_handle().interrupt();
            let error = mentions_environment(&small, &mut |n| session.charge_host_work(n)).unwrap_err();
            let error = declaration_observation_context(error, &small.span, "&env");
            assert!(error.is_interrupt());
            assert_eq!(error.to_string(), "Interrupted");
            let SessionError::Trap(ref trap) = error else { panic!("typed IRQ") };
            assert_eq!(trap.downcast_ref::<wasmtime::Trap>(), Some(&wasmtime::Trap::Interrupt));
            assert!(format!("{trap:#}").contains("parameter observation (&env)"));
        }
        {
            let _scope = session.operation_budget_scope().unwrap();
            assert!(!mentions_environment(&small, &mut |n| session.charge_host_work(n)).unwrap());
        }
        assert_eq!(session.options().fuel_per_operation, 1_000);
    }

    #[test]
    fn parameter_observation_public_replacement_failure_preserves_definition_and_policies() {
        const OLD: &str = "(defmacro retained-observation [] 17)";
        let payload = (0..256).map(|n| n.to_string()).collect::<Vec<_>>().join(" ");
        let replacement = format!("(defmacro ^{{:suss/compiler-catalog true :scan-payload [{payload}]}} retained-observation [] 99)");
        let mut macros = CompiledMacros::new().unwrap();
        let allowance = macros.expansion_work_fuel();
        // Establish that this finite replacement really succeeds at the same
        // ordinary default; no larger allowance or production hook is introduced.
        macros.define(&replacement).unwrap();
        macros.define(OLD).unwrap();
        let key = ("user".to_owned(), "retained-observation".to_owned());
        let old_hash = macros.declaration_sources[&key].clone();
        let (mut low, mut high) = (replacement.len() as u64, allowance);
        let mut reached = None;
        // Locate a genuine scan debit using public define calls, not a copied
        // preparation implementation or a test-only production stage hook.
        for _ in 0..24 {
            if high <= low + 1 { break; }
            macros.set_operation_fuel(allowance);
            macros.define(OLD).unwrap();
            let budget = low + (high - low) / 2;
            macros.set_operation_fuel(budget);
            match macros.define(&replacement) {
                Ok(_) => high = budget,
                Err(error) => {
                    let message = error.to_string();
                    if message.contains("Macro declaration parameter observation (") {
                        assert!(out_of_fuel(&error), "actual scan Trap::OutOfFuel required: {error:?}");
                        assert!(message.contains("bytes"));
                        reached = Some((budget, message));
                        break;
                    }
                    assert!(message.contains("out of fuel") || message.contains("all fuel consumed"),
                        "unrelated public setup error: {error:?}");
                    match error {
                        // Preparation/reader errors precede the scan. Direct
                        // provenance/evaluation traps follow it in this fixture.
                        SessionError::Compile(_) => low = budget,
                        SessionError::Trap(_) => high = budget,
                        _ => panic!("unexpected error class: {message}"),
                    }
                }
            }
        }
        let (budget, stage) = reached.expect("must observe an actual scan-stage typed debit; no unrelated failure earns credit");
        eprintln!("parameter-observation public budget={budget} stage={stage}");
        assert_eq!(macros.declaration_sources[&key], old_hash);
        assert!(macros.environment_free.contains(&key));
        assert!(macros.caller_form_free.contains(&key));
        assert!(!macros.compiler_catalog_macros.contains_key(&key));
        let mut caller = Session::new_repl().unwrap();
        let value = caller.eval_with_macros("(retained-observation)", &mut macros).unwrap();
        let bridge = FormBridge::new(&mut caller).unwrap();
        assert_eq!(bridge.read(&mut caller, &value, 0..1).unwrap().kind, Kind::Number(17.0));
        assert_eq!(macros.expansion_work_fuel(), budget);
    }
}
#[cfg(test)]
mod analyzer_literal_effect_tests;

#[cfg(test)]
#[path = "portable_macros/body_origin_tests.rs"]
mod body_origin_tests;

#[cfg(test)]
#[path = "portable_macros/reference_field_tests.rs"]
mod reference_field_tests;

#[cfg(test)]
#[path = "portable_macros/reader_metadata_tests.rs"]
mod reader_metadata_tests;
