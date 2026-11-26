//! Source macro functions execute in a separate compiled phase Store.
use crate::portable::{Diagnostic, ExpansionContext, ExpansionHost};
use crate::{
    portable_macro_data::FormBridge,
    portable_macro_graph::AnalysisGraph,
    portable_session::{Session, SessionHost, SessionError, SessionValue},
};
use std::{collections::{BTreeMap, BTreeSet}, sync::{Arc, Mutex, MutexGuard}};
use suss_reader::forms::{Form, Kind, read_forms};
mod reentry;
pub(crate) use reentry::MacroAuthority;
mod facade;
mod evaluation;
mod definitions;
mod native_invocation;
pub(crate) use evaluation::{PreparedEvaluation, prepare_with_expander};
pub use facade::CompiledMacros;
pub(crate) use facade::{MacroService, MacroServiceWeak, MacroOperation};
pub(crate) use native_invocation::InterruptRelay;
#[derive(Clone, Copy)]
pub(crate) enum CompilerCatalogMode {
    Full,
    RecordBindings,
}
/// The phase Store and catalog. Callback views borrow that same Store and
/// share its catalog; they never reconstruct a macro host.
pub(crate) struct MacroHost<'store> {
    session: SessionHost<'store>,
    bridge: Arc<FormBridge>,
    catalog: Arc<Mutex<MacroCatalog>>,
    namespace_interner: SessionValue,
    namespace_setter: SessionValue,
    exception_message: SessionValue,
    // Native eval definitions belong to this caller Store. Imported macros
    // keep their separate compiled host and are consulted only on a local miss.
    fallback: Option<MacroService>,
    invocation_error: Option<SessionError>,
    native_effects: Vec<(crate::portable::resolve::Environment, crate::portable::resolve::Environment)>,
}
#[derive(Default)]
struct MacroCatalog {
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
    namespace_objects: BTreeMap<String, SessionValue>,
    macro_namespace: Option<String>,
    inline_modules: Vec<InlineModule>,
    declaration_transactions: Vec<DeclarationTransaction>,
}
// Catalog guards cover native metadata work only. Clone roots/modes before any
// guest invocation, form transport, compilation or namespace initialization.
fn catalog(state: &Mutex<MacroCatalog>) -> MutexGuard<'_, MacroCatalog> {
    state.lock().unwrap_or_else(|poison| poison.into_inner())
}
struct InlineModule {
    external: bool,
    declarations: Vec<Form>,
    active_declaration: Option<usize>,
    top_level_heads: BTreeMap<(usize, usize), suss_reader::Symbol>,
    origin: crate::portable::SourceOrigin,
    caller: Option<String>,
}
#[derive(Default)]
struct DeclarationTransaction {
    checkpoint: Option<MacroCheckpoint>,
    globals: BTreeSet<crate::portable::resolve::Global>,
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
fn trace_fuel(session: &SessionHost<'_>, target: &(String, String), stage: &str) -> Result<(), SessionError> {
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
            Kind::Discard(target) | Kind::Tagged { form: target, .. } => {
                append(&mut pending, std::slice::from_ref(target.as_ref()), charge)?
            }
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

#[cfg(all(test, not(suss_generator)))]
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
impl MacroHost<'static> {
    pub fn new() -> Result<Self, SessionError> {
        Self::with_bootstrap(Session::new_macro()?)
    }
    fn with_bootstrap(mut session: Session) -> Result<Self, SessionError> {
        let bridge = FormBridge::new(&mut session)?;
        let namespace_interner = session.eval("suss.core/intern-namespace")?;
        let namespace_setter = session.eval("suss.core/enter-macro-namespace!")?;
        let exception_message = session.eval("suss.core/ex-message")?;
        // Intern the namespaces most expansions run in before any request
        // budget applies, so a macro's budget covers the macro's own work.
        let mut namespace_objects = BTreeMap::new();
        for namespace in ["suss.core", "user"] {
            let name = bridge.quote(
                &mut session,
                Form { span: 0..0, metadata: vec![], kind: Kind::Symbol(suss_reader::Symbol::new(namespace)) },
            )?;
            namespace_objects.insert(namespace.to_owned(), session.invoke(&namespace_interner, &[&name])?);
        }
        let mut macros = Self {
            session,
            bridge: Arc::new(bridge),
            catalog: Arc::new(Mutex::new(MacroCatalog {
                namespace_objects,
                ..MacroCatalog::default()
            })),
            namespace_interner,
            namespace_setter,
            exception_message,
            fallback: None,
            invocation_error: None,
            native_effects: vec![],
        };
        macros.enter_namespace("suss.core")?;
        macros.define(crate::portable_defn::SOURCE)?;
        macros.define(crate::portable_when_let::SOURCE)?;
        macros.define(crate::portable_lazy_seq::SOURCE)?;
        macros.define(crate::portable_state_macros::DELAY)?;
        macros.define(crate::portable_state_macros::VSWAP)?;
        macros.define(crate::portable_state_macros::WITH_PRECISION)?;
        macros.define_compiler_macro(crate::portable_state_macros::CASE)?;
        macros.define(crate::portable_core_macros::DOSEQ)?;
        macros.define(crate::portable_core_macros::THREAD_LAST)?;
        macros.define(crate::portable_core_macros::DEFMULTI)?;
        macros.define(crate::portable_core_macros::DEFMETHOD)?;
        macros.define(crate::portable_core_macros::ASSERT)?;
        // Shipped record macro implementations build forms from core's Cons
        // directly, so they are evaluated as core (#279).
        macros
            .session
            .eval_privileged(crate::portable_record_factories::SOURCE)?;
        macros.session.eval_privileged(crate::portable_record_label::SOURCE)?;
        macros
            .session
            .eval_privileged(crate::portable_record_protocol_facts::SOURCE)?;
        macros
            .session
            .eval_privileged(crate::portable_record_emitter::SOURCE)?;
        macros.session.eval_privileged(crate::portable_record_public::SOURCE)?;
        macros.enter_namespace("suss.core")?;
        macros.define(crate::portable_record_public::MACRO_SOURCE)?;
        macros.enter_namespace("user")?;
        // Bootstrap code is already rooted in the Store. User source cache
        // accounting starts empty, including after an explicit session reset.
        catalog(&macros.catalog).artifact_cache = Default::default();
        let authority = macros.authority();
        macros.session.attach_macro_authority(authority);
        Ok(macros)
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
}
impl MacroHost<'_> {
    pub(crate) fn define_inline_macro(
        &mut self, form: &Form, environment: &crate::portable::resolve::Environment,
        phase: crate::portable::resolve::Phase,
    ) -> Result<(), SessionError> {
        // Checkpoints are lazy: an ordinary module must not copy the macro
        // catalog or initialize any additional compile-time dependencies.
        let transactions = catalog(&self.catalog).declaration_transactions.len();
        for index in 0..transactions {
            if catalog(&self.catalog).declaration_transactions[index].checkpoint.is_none() {
                let checkpoint = self.binding_checkpoint()?;
                catalog(&self.catalog).declaration_transactions[index].checkpoint = Some(checkpoint);
            }
        }
        let mut module = catalog(&self.catalog).inline_modules.pop().expect("inline module scope");
        let namespace = environment.current_namespace(phase).to_owned();
        let result = (|| {
            if module.caller.is_none() {
                module.caller = Some(self.session.current_namespace().to_owned());
            }
            self.enter_namespace(&namespace)?;
            let index = module.declarations.partition_point(|declaration| declaration.span.start <= form.span.start);
            if let Some(index) = index.checked_sub(1) {
                if module.active_declaration != Some(index) {
                    let declaration = crate::portable::clone_form_metered(&module.declarations[index], &mut |work| {
                        self.session.charge_host_work(work).map_err(|error| Diagnostic { span: form.span.clone(), message: error.to_string() })
                    }).map_err(SessionError::Compile)?;
                    // Required Macro dependencies that initialize successfully
                    // keep their existing once-only loading contract even when
                    // the caller's later declaration preparation fails.
                    let transactions = std::mem::take(&mut catalog(&self.catalog).declaration_transactions);
                    let imported = crate::portable::analysis_record::internal_setup(|| {
                        let span = declaration.span.clone();
                        let prepared = self.session.compilation_snapshot().prepare_with_origin(
                            vec![declaration], span, self, Some(&module.origin),
                        )?;
                        let mut sources = Vec::new();
                        for unit in &prepared.modules {
                            let work = unit.dependencies.iter().fold(
                                unit.source.len().saturating_mul(2)
                                    .saturating_add(unit.source_path.as_os_str().len().saturating_mul(4))
                                    .saturating_add(256),
                                |work, dependency| work.saturating_add(dependency.namespace().len().saturating_mul(4)).saturating_add(64),
                            );
                            self.session.charge_host_work(u64::try_from(work).unwrap_or(u64::MAX))?;
                            let identity = crate::portable::bootstrap::sha256(
                                format!("{:?}:{:?}:{}", unit.source_path, unit.dependencies, unit.source).as_bytes(),
                            );
                            sources.push((unit.identity.clone(), identity));
                        }
                        catalog(&self.catalog).incomplete_sources.extend(sources.iter().map(|(unit, _)| unit.namespace().to_owned()));
                        let initialized = self.session.eval_prepared(prepared);
                        // Successful dependencies remain provided even if a later
                        // initializer fails. Retain their captured source identities
                        // now: discovery will omit these units on the next attempt.
                        for (unit, identity) in sources {
                            if self.session.has_initialized_source_module(&unit) {
                                catalog(&self.catalog).incomplete_sources.remove(unit.namespace());
                                catalog(&self.catalog).loaded_sources.insert(unit.namespace().to_owned(), identity);
                            }
                        }
                        initialized?;
                        Ok::<_, SessionError>(())
                    });
                    debug_assert!(catalog(&self.catalog).declaration_transactions.is_empty());
                    catalog(&self.catalog).declaration_transactions = transactions;
                    imported?;
                    module.active_declaration = Some(index);
                }
            }
            self.enter_namespace(&namespace)?;
            let mut declaration = crate::portable::clone_form_metered(form, &mut |work| {
                self.session.charge_host_work(work).map_err(|error| Diagnostic {
                    span: form.span.clone(), message: error.to_string(),
                })
            }).map_err(SessionError::Compile)?;
            let Kind::List(items) = &mut declaration.kind else { unreachable!() };
            items[0].kind = Kind::Symbol(suss_reader::Symbol { namespace: Some("suss.core".into()), name: "defmacro".into() });
            if let Some(Form { kind: Kind::Symbol(name), .. }) = items.get(1) {
                let global = crate::portable::resolve::Environment::default().declare_cell(
                    crate::portable::resolve::Phase::Macro, &namespace, &name.name,
                ).map_err(SessionError::Compile)?;
                for transaction in &mut catalog(&self.catalog).declaration_transactions {
                    transaction.globals.insert(global.clone());
                }
            }
            crate::portable::source::resolve_auto_keywords(
                std::slice::from_mut(&mut declaration), environment, phase,
            ).map_err(SessionError::Compile)?;
            let span = declaration.span.clone();
            self.define_form_with_origin(declaration, span, Some(&module.origin))?;
            Ok(())
        })();
        catalog(&self.catalog).inline_modules.push(module);
        result
    }
    pub(crate) fn binding_checkpoint(&mut self) -> Result<MacroCheckpoint, SessionError> {
        let bindings = self.session.binding_checkpoint()?;
        let state = catalog(&self.catalog);
        Ok((bindings, state.definitions.clone(), state.declaration_sources.clone(),
            state.environment_free.clone(), state.compiler_catalog_macros.clone(),
            state.caller_form_free.clone()))
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
                catalog(&self.catalog).definitions.insert(key.clone(), value.clone());
            } else {
                catalog(&self.catalog).definitions.remove(&key);
            }
            if checkpoint.3.contains(&key) {
                catalog(&self.catalog).environment_free.insert(key.clone());
            } else {
                catalog(&self.catalog).environment_free.remove(&key);
            }
            if checkpoint.5.contains(&key) {
                catalog(&self.catalog).caller_form_free.insert(key.clone());
            } else {
                catalog(&self.catalog).caller_form_free.remove(&key);
            }
            if let Some(mode) = checkpoint.4.get(&key) {
                catalog(&self.catalog).compiler_catalog_macros.insert(key.clone(), *mode);
            } else {
                catalog(&self.catalog).compiler_catalog_macros.remove(&key);
            }
            if let Some(source) = checkpoint.2.get(&key) {
                catalog(&self.catalog).declaration_sources.insert(key, source.clone());
            } else {
                catalog(&self.catalog).declaration_sources.remove(&key);
            }
        }
        Ok(())
    }
    pub fn current_namespace(&self) -> String {
        self.session.current_namespace()
    }
    pub fn enter_namespace(&mut self, namespace: &str) -> Result<(), SessionError> {
        self.session.enter_namespace(namespace)
    }
    pub fn define(&mut self, source: &str) -> Result<(), SessionError> {
        self.define_source(source, false)
    }
    // Only compiler-owned source may read shipped core's private link cells.
    // The public define API always uses an unprivileged origin.
    fn define_compiler_macro(&mut self, source: &str) -> Result<(), SessionError> {
        self.define_source(source, true)
    }
    fn define_source(&mut self, source: &str, privileged: bool) -> Result<(), SessionError> {
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
        let origin = if privileged { crate::portable::SourceOrigin::privileged(source) }
            else { crate::portable::SourceOrigin::new(source, None) };
        self.define_form_with_origin(
            forms.into_iter().next().unwrap(),
            0..source.len(),
            Some(&origin),
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
        catalog(&self.catalog).incomplete_sources.extend(
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
                catalog(&self.catalog).loaded_sources
                    .insert(unit.identity.namespace().to_owned(), source_identity);
                catalog(&self.catalog).incomplete_sources.remove(unit.identity.namespace());
            }
            Ok(catalog(&self.catalog)
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
    /// Explicitly provide the harness observer in this isolated Macro Store.
    /// Runtime observers are never inherited. The callback is synchronous and
    /// must not re-enter the Store; replacement/reset drops this capability.
    pub fn install_observation_hook(
        &mut self,
        callback: impl for<'a> Fn(wasmtime::StoreContextMut<'a, ()>, wasmtime::Val) -> wasmtime::Result<u32>
            + Send + Sync + 'static,
    ) -> Result<(), SessionError> {
        self.session.install_phase_observation_hook(callback)
    }
    /// Interrupts a running macro expansion in this compiled macro session.
    pub fn interrupt_handle(&self) -> crate::portable_session::InterruptHandle {
        self.session.interrupt_handle()
    }
    pub(crate) fn define_form_with_origin(
        &mut self, form: Form, span: std::ops::Range<usize>,
        origin: Option<&crate::portable::SourceOrigin>,
    ) -> Result<SessionValue, SessionError> {
        let _budget = self.session.operation_budget_scope()?;
        let pending = self.prepare_macro_definition(&form, span, origin)?;
        self.commit_macro_definition(pending).map(|(function, _variable)| function)
    }

}
impl MacroHost<'_> {
    pub fn artifact_cache_stats(&self) -> crate::portable::artifact_cache::CacheStats {
        catalog(&self.catalog).artifact_cache.stats()
    }
    /// Expansions that materialized a caller &env analysis graph.
    pub fn environment_materializations(&self) -> u64 {
        catalog(&self.catalog).environment_materializations
    }
}
impl MacroHost<'_> {
    fn with_math_context<T>(&mut self, context: crate::portable_session::EvaluationMathContext,
        operation: impl FnOnce(&mut MacroHost<'_>) -> Result<T, SessionError>) -> Result<T, SessionError> {
        let _budget = self.session.operation_budget_scope()?;
        let bridge = self.bridge.clone();
        let checkpoint = self.session.enter_evaluation_math_context(context, &bridge)?;
        let result = operation(self);
        let restoration = self.session.leave_evaluation_math_context(&checkpoint);
        match result { Err(error) => Err(error), Ok(value) => restoration.map(|()| value) }
    }
    fn fallback_step<T>(&mut self, span: std::ops::Range<usize>, context: bool,
        operation: impl FnOnce(&mut dyn ExpansionHost) -> Result<T, Diagnostic>) -> Result<T, Diagnostic> {
        let service = self.fallback.clone().expect("native fallback service");
        let math = if context { Some(self.session.evaluation_math_context().map_err(|e| Diagnostic { span: span.clone(), message: e.to_string() })?) } else { None };
        let allowance = self.session.operation_fuel_remaining().map_err(|e| Diagnostic { span: span.clone(), message: e.to_string() })?;
        let interrupt = self.session.host_work_interrupt();
        let (result, spent) = crate::portable_session::suspended::scope(&mut self.session, || service.preparation_step(allowance, math, interrupt, operation))
            .map_err(|e| Diagnostic { span: span.clone(), message: e.to_string() })?;
        self.session.settle_nested_fuel(allowance, spent).map_err(|e| Diagnostic { span: span.clone(), message: e.to_string() })?;
        result.map_err(|error| {
            let message = error.to_string();
            self.invocation_error = Some(error);
            Diagnostic { span, message }
        })
    }
    fn fallback_expand(&mut self, form: &Form, context: ExpansionContext<'_>) -> Result<Option<Form>, Diagnostic> {
        if self.fallback.is_none() { return Ok(None); }
        let result = self.fallback_step(form.span.clone(), true, |host| host.expand(form, context));
        if matches!(&result, Ok(Some(_))) { catalog(&self.catalog).source_expansions += 1; }
        result
    }
    fn native_preparation_result<T>(&mut self, result: Result<T, SessionError>) -> Result<T, SessionError> {
        self.session.charge_host_work(0)?;
        if let Some(error) = self.invocation_error.take() { return Err(error); }
        result
    }
}
impl ExpansionHost for MacroHost<'_> {
    fn intern_native_var(&mut self, owner: u64, global: &crate::portable::resolve::Global,
        declaration: &Form, doc: Option<&[u16]>) -> Result<(), Diagnostic> {
        let result = if owner == self.session.native_owner() {
            self.session.intern_analyzed_var(global, declaration, doc)
        } else {
            Err(wasmtime::Error::msg("Native Var admission has a foreign owner").into())
        };
        result.map_err(|error| {
            let message = error.to_string();
            self.invocation_error = Some(error);
            Diagnostic { span: declaration.span.clone(), message }
        })
    }
    fn refresh_native_environment(&mut self, environment: &mut crate::portable::resolve::Environment, span: std::ops::Range<usize>) -> Result<(), Diagnostic> {
        for (before, after) in std::mem::take(&mut self.native_effects) {
            environment.apply_native_effects(&before, &after, &mut |work| {
                self.session.charge_host_work(work).map_err(|error| Diagnostic { span: span.clone(), message: error.to_string() })
            })?;
        }
        Ok(())
    }
    fn begin_declarations(&mut self) -> Result<(), Diagnostic> {
        if self.fallback.is_some() {
            let mut began = false;
            let result = self.fallback_step(0..0, false, |host| {
                host.begin_declarations()?;
                began = true;
                Ok(())
            });
            if let Err(error) = result {
                if began {
                    let _ = self.fallback.as_ref().unwrap().preparation_cleanup(|host| host.end_declarations(false));
                }
                return Err(error);
            }
        }
        catalog(&self.catalog).declaration_transactions.push(DeclarationTransaction::default());
        Ok(())
    }
    fn end_declarations(&mut self, commit: bool) -> Result<(), Diagnostic> {
        let transaction = catalog(&self.catalog).declaration_transactions.pop().expect("declaration transaction");
        let local = if !commit {
            if let Some(checkpoint) = transaction.checkpoint {
                self.restore_bindings(checkpoint, &transaction.globals.into_iter().collect::<Vec<_>>())
                    .map_err(|error| Diagnostic { span: 0..0, message: error.to_string() })
            } else { Ok(()) }
        } else { Ok(()) };
        let external = if let Some(service) = &self.fallback {
            service.preparation_cleanup(|host| host.end_declarations(commit))
        } else { Ok(()) };
        local.and(external)
    }
    fn begin_source_module(&mut self, forms: &[Form], origin: &crate::portable::SourceOrigin) -> Result<(), Diagnostic> {
        let external = self.fallback.is_some() && origin.native_owner().is_none();
        if external {
            let mut began = false;
            let result = self.fallback_step(0..0, false, |host| {
                host.begin_source_module(forms, origin)?;
                began = true;
                Ok(())
            });
            if let Err(error) = result {
                if began {
                    let _ = self.fallback.as_ref().unwrap().preparation_cleanup(|host| host.end_source_module());
                }
                return Err(error);
            }
        }
        let result = (|| {
        let mut declarations = Vec::new();
        let mut top_level_heads = BTreeMap::new();
        for form in forms {
            self.charge_expansion_work(1, form.span.clone())?;
            let Kind::List(items) = &form.kind else { continue };
            let Some(Form { kind: Kind::Symbol(head), .. }) = items.first() else { continue };
            self.charge_expansion_work((std::mem::size_of::<(usize, usize, suss_reader::Symbol)>() + head.name.len() + head.namespace.as_ref().map_or(0, String::len)) as u64, form.span.clone())?;
            top_level_heads.insert((form.span.start, form.span.end), head.clone());
            if head.name == "ns" && (head.namespace.is_none() || matches!(head.namespace.as_deref(), Some("suss.core" | "clojure.core" | "cljs.core"))) {
                declarations.push(crate::portable::clone_form_metered(form, &mut |work| self.charge_expansion_work(work, form.span.clone()))?);
            }
        }
        catalog(&self.catalog).inline_modules.push(InlineModule { external, declarations, active_declaration: None, top_level_heads, origin: origin.clone(), caller: None });
        Ok(())
        })();
        if result.is_err() && external {
            let _ = self.fallback.as_ref().unwrap().preparation_cleanup(|host| host.end_source_module());
        }
        result
    }
    fn end_source_module(&mut self) -> Result<(), Diagnostic> {
        let module = catalog(&self.catalog).inline_modules.pop().expect("source module scope");
        if module.external {
            return self.fallback.as_ref().expect("source module fallback").preparation_cleanup(|host| host.end_source_module());
        }
        if let Some(caller) = module.caller {
            self.enter_namespace(&caller).map_err(|error| Diagnostic { span: 0..0, message: error.to_string() })?;
        }
        Ok(())
    }
    fn declared_macro_exports(&self, namespace: &str) -> Option<Vec<String>> {
        let mut names: BTreeSet<_> = catalog(&self.catalog).definitions.keys()
            .filter(|(ns, _)| ns == namespace).map(|(_, name)| name.clone()).collect();
        names.extend(self.session.native_macro_exports(namespace));
        if let Some(service) = &self.fallback {
            if let Some(external) = service.preparation_read(|host| host.declared_macro_exports(namespace)) {
                names.extend(external);
            }
        }
        Some(names.into_iter().collect())
    }
    fn macro_binding_owner(&self, namespace: &str, name: &str) -> Option<u64> {
        if catalog(&self.catalog).inline_modules.last().is_some_and(|module| module.external) {
            return self.fallback.as_ref().and_then(|service| service.preparation_read(|host| host.macro_binding_owner(namespace, name)));
        }
        if catalog(&self.catalog).definitions.contains_key(&(namespace.to_owned(), name.to_owned())) {
            return Some(self.session.native_owner());
        }
        self.fallback.as_ref().and_then(|service| service.preparation_read(|host| host.macro_binding_owner(namespace, name)))
    }
    fn expansion_work_fuel(&self) -> u64 {
        self.session.compilation_work_allowance()
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
        catalog(&self.catalog).artifact_cache.emit_metered(
            function, phase, forms, origin, dependencies.as_deref(),
            &mut |work| session.charge_host_work(work).map_err(|error| Diagnostic {
                span: function.span.clone(),
                message: format!("Artifact preparation budget exhausted: {error}"),
            }),
        )
    }
    fn artifact_dependencies(&self) -> Option<Vec<(String, String)>> {
        if !catalog(&self.catalog).incomplete_sources.is_empty() {
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
            catalog(&self.catalog).loaded_sources
                .iter()
                .map(|(name, source)| (format!("module:{name}"), source.clone())),
        );
        dependencies.extend(
            catalog(&self.catalog).declaration_sources
                .iter()
                .map(|((namespace, name), source)| {
                    (format!("macro:{namespace}/{name}"), source.clone())
                }),
        );
        if let Some(service) = &self.fallback {
            let external = service.preparation_read(|host| host.artifact_dependencies())?;
            dependencies.extend(external.into_iter().map(|(name, hash)| (format!("isolated:{name}"), hash)));
        }
        Some(dependencies)
    }

    fn supports_macro_imports(&self) -> bool {
        true
    }
    fn source_paths(&mut self, paths: &[std::path::PathBuf]) {
        self.session.set_source_paths(paths);
        if self.fallback.is_some() {
            if let Err(error) = self.fallback_step(0..0, false, |host| { host.source_paths(paths); Ok(()) }) {
                self.invocation_error = Some(SessionError::Compile(error));
            }
        }
    }
    fn load_macro_namespace(
        &mut self,
        namespace: &str,
        span: std::ops::Range<usize>,
    ) -> Result<Vec<String>, Diagnostic> {
        if self.fallback.is_some() {
            return self.fallback_step(span.clone(), true, |host| host.load_macro_namespace(namespace, span));
        }
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
        if self.fallback.is_some() {
            return self.fallback_step(span.clone(), true, |host| host.load_macro_namespace_with_policy(namespace, policy, span));
        }
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
        let inline_declaration = {
            let state = catalog(&self.catalog);
            state.inline_modules.last().is_some_and(|module|
                context.origin.is_some_and(|origin| origin.same_input(&module.origin))
                    && module.top_level_heads.get(&(form.span.start, form.span.end)) == Some(name))
        };
        if inline_declaration
            && context.function_scopes.is_empty() && context.locals.is_empty() && context.fields.is_empty()
            && context.environment.resolves_bootstrap_name(context.phase, name, "defmacro")
        {
            if catalog(&self.catalog).inline_modules.last().is_some_and(|module| module.external) {
                return self.fallback_expand(form, context);
            }
            self.define_inline_macro(form, context.environment, context.phase).map_err(|error| Diagnostic {
                span: form.span.clone(), message: format!("Compiled local macro declaration failed: {error}"),
            })?;
            return Ok(Some(Form { span: form.span.clone(), metadata: vec![], kind: Kind::Nil }));
        }
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
                    | "var"
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
                (name.namespace.is_none() && catalog(&self.catalog).definitions.contains_key(&own)).then_some(own)
            })
            .or_else(|| {
                ["->>", "assert", "defmethod", "defmulti", "defn", "defrecord", "delay", "doseq", "lazy-seq", "vswap!", "when-let", "with-precision", "case"]
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
        let native = context.phase == self.session.phase()
            && (self.fallback.is_some() || context.origin.and_then(|origin| origin.native_owner()) == Some(self.session.native_owner()));
        let live_status = if native {
            match context.environment.native_macro_global(context.phase, &target.0, &target.1, self.session.native_owner()) {
                Some(global) => self.session.native_macro_enabled(&global).map_err(|e| Diagnostic {
                    span: form.span.clone(), message: e.to_string(),
                })?,
                None => None,
            }
        } else { None };
        if live_status == Some(false) || (live_status.is_none()
            && context.environment.source_macro_overridden(context.phase, &target.0, &target.1)) {
            return Ok(None);
        }
        let foreign = context.environment.macro_binding_owner(context.phase, &target.0, &target.1)
            .is_some_and(|owner| owner != self.session.native_owner());
        let function = if foreign { None } else { catalog(&self.catalog).definitions.get(&target).cloned() };
        let mut binding_override = false;
        let function = if native {
            if let Some(global) = context.environment.native_macro_global(context.phase, &target.0, &target.1, self.session.native_owner())
                && (function.is_some() || live_status == Some(true)) {
                let selected = self.session.binding_value(&global).map_err(|error| Diagnostic {
                    span: form.span.clone(), message: error.to_string(),
                })?;
                binding_override = match function.as_ref() {
                    Some(function) => !self.session.identical_values(function, &selected).map_err(|error| Diagnostic {
                        span: form.span.clone(), message: error.to_string(),
                    })?,
                    None => true,
                };
                Some(selected)
            } else { function }
        } else { function };
        let Some(function) = function else {
            if !self.fallback.as_ref().is_some_and(|service| service.contains_source_macro(&target.0, &target.1)) {
                return Ok(None);
            }
            return self.fallback_expand(form, context);
        };
        catalog(&self.catalog).source_expansions += 1;
        let native_owner = context.origin.and_then(|origin| origin.native_owner());
        let before = native.then(|| self.session.compilation_snapshot().environment);
        let mut stage = String::from("caller form provenance");
        let caller_namespace = context.environment.current_namespace(context.phase).to_owned();
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
            let quote = |session: &mut SessionHost<'_>, form: &Form| {
                if let Some(owner) = native_owner {
                    self.bridge.quote_evaluation_ref(session, form, owner)
                } else { self.bridge.quote_ref(session, form) }
            };
            let caller_form = if !binding_override && catalog(&self.catalog).caller_form_free.contains(&target) {
                self.bridge.quote(session, Form { span: form.span.clone(), metadata: vec![], kind: Kind::Nil })?
            } else {
                quote(session, &caller_data)?
            };
            trace_fuel(session, &target, "caller-form")?;
            // A macro that cannot read &env receives nil instead of a
            // materialized analysis graph; nothing else can observe it.
            stage = "caller &env input transport".into();
            let caller_environment = if !binding_override && catalog(&self.catalog).environment_free.contains(&target) {
                self.bridge.quote(
                    session,
                    Form {
                        span: form.span.clone(),
                        metadata: vec![],
                        kind: Kind::Nil,
                    },
                )?
            } else {
                catalog(&self.catalog).environment_materializations += 1;
                let mut graph = AnalysisGraph::new(&self.bridge, session);
                let mode = if binding_override { None } else { catalog(&self.catalog).compiler_catalog_macros.get(&target).copied() };
                match mode {
                    Some(CompilerCatalogMode::Full) => {
                        graph.expansion_with_compiler_catalog(context)?
                    }
                    Some(CompilerCatalogMode::RecordBindings) => {
                        graph.expansion_with_record_catalog(context)?
                    }
                    None if native_owner.is_some() => graph.native_evaluation_locals(context)?,
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
                arguments.push(quote(session, argument)?);
            }
            trace_fuel(session, &target, "arguments")?;
            let arguments = arguments.iter().collect::<Vec<_>>();
            stage = "caller namespace".into();
            let namespace = caller_namespace.as_str();
            let current_namespace = catalog(&self.catalog).macro_namespace.clone();
            if !native && current_namespace.as_deref() != Some(namespace) {
                let object = catalog(&self.catalog).namespace_objects.get(namespace).cloned();
                let object = match object {
                    Some(object) => object,
                    None => {
                        let name = self.bridge.quote(
                            session,
                            Form {
                                span: form.span.clone(), metadata: vec![],
                                kind: Kind::Symbol(suss_reader::Symbol::new(namespace)),
                            },
                        )?;
                        let object = session.invoke(&self.namespace_interner, &[&name])?;
                        catalog(&self.catalog).namespace_objects.insert(namespace.to_owned(), object.clone());
                        object
                    }
                };
                session.invoke(&self.namespace_setter, &[&object])?;
                catalog(&self.catalog).macro_namespace = Some(namespace.to_owned());
            }
            // A caller-owned macro can also be used while an ordinary source
            // dependency is compiled. Bind its call-site namespace temporarily;
            // the caller's root and a native eval's existing frame stay intact.
            let namespace_checkpoint = if native && native_owner.is_none() {
                let object = catalog(&self.catalog).namespace_objects.get(namespace).cloned();
                let object = match object {
                    Some(object) => object,
                    None => {
                        let name = self.bridge.quote(session, Form {
                            span: form.span.clone(), metadata: vec![],
                            kind: Kind::Symbol(suss_reader::Symbol::new(namespace)),
                        })?;
                        let object = session.invoke(&self.namespace_interner, &[&name])?;
                        catalog(&self.catalog).namespace_objects.insert(namespace.to_owned(), object.clone());
                        object
                    }
                };
                Some(session.enter_compilation_namespace(&object)?)
            } else { None };
            stage = "compiled macro invocation".into();
            let invocation = session.invoke(&function, &arguments);
            let restoration = match &namespace_checkpoint {
                Some(checkpoint) => session.leave_compilation_namespace(checkpoint),
                None => Ok(()),
            };
            let invocation = match invocation {
                Err(error) => Err(error),
                Ok(value) => restoration.map(|()| value),
            };
            let result = match invocation {
                Ok(value) => value,
                Err(error) => {
                    if native || native_owner.is_some() { return Err(error); }
                    if let SessionError::Language(payload) = &error {
                        // The native accessor reads canonical exception storage;
                        // arbitrary thrown values return nil. Keep the original
                        // failure if extraction cannot produce a String.
                        if let Ok(message) = session.invoke(&self.exception_message, &[payload]) {
                            if let Ok(Form { kind: Kind::String(ref units), .. }) =
                                self.bridge.read(session, &message, form.span.clone())
                            {
                                return Err(SessionError::Compile(Diagnostic {
                                    span: form.span.clone(),
                                    message: String::from_utf16_lossy(units),
                                }));
                            }
                        }
                    }
                    return Err(error);
                }
            };
            trace_fuel(session, &target, "invoked")?;
            stage = "expanded result transport".into();
            let expanded = if let Some(owner) = native_owner {
                let expanded = session.read_native_evaluation(&self.bridge, &result, form.span.clone())?;
                session.transport_expansion_constants(expanded, owner)?
            } else { self.bridge.read(session, &result, form.span.clone())? };
            trace_fuel(session, &target, "decoded")?;
            Ok(expanded)
        });
        if let Some(before) = before {
            let after = self.session.compilation_snapshot().environment;
            if before.source_generation() != after.source_generation() {
                self.native_effects.push((before, after));
            }
        }
        match result {
            Ok(value) => Ok(Some(value)),
            Err(error) => {
                let message = format!("Compiled macro expansion failed during {stage}: {error}");
                if native || native_owner.is_some() { self.invocation_error = Some(error); }
                Err(Diagnostic { span: form.span.clone(), message })
            }
        }
    }
}



#[cfg(all(test, not(suss_generator)))]
mod when_let_validation_tests {
    use super::*;

    #[test]
    fn compiled_when_let_validation_matches_pinned_messages_and_phase_after_gc() {
        let reference: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/oracle/when-let-validation-observations.json"
        )).unwrap();
        assert_eq!(reference["upstream"], "c4295f303100bbf5afac449242d30bca1126f1a1");
        let mut macros = MacroHost::new().unwrap();
        let function = catalog(&macros.catalog).definitions[&("suss.core".to_owned(), "when-let".to_owned())].clone();
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
            assert!(matches!(recovered.kind, Kind::Long(42)));
        }
    }
}

#[cfg(all(test, not(suss_generator)))]
mod doseq_validation_tests {
    use super::*;

    /// doseq checks its bindings as Clojure's assert-args does (#30).
    #[test]
    fn doseq_validation_matches_clojures_messages_and_phase() {
        let mut macros = MacroHost::new().unwrap();
        let function = catalog(&macros.catalog).definitions[&("suss.core".to_owned(), "doseq".to_owned())].clone();
        let capture = macros.session.eval(r#"
          (fn [expand form bindings]
            (try (expand form nil bindings 1)
              (catch :default problem
                [(ex-message problem) (get (ex-data problem) :clojure.error/phase)])))
        "#).unwrap();
        for (source, message) in [
            ("(doseq (x [1]) 1)", "doseq requires a vector for its binding"),
            ("(doseq [x] 1)", "doseq requires an even number of forms in binding vector"),
        ] {
            let form = read_forms(source).unwrap().remove(0);
            let Kind::List(items) = &form.kind else { panic!("doseq call") };
            let bindings = macros.bridge.quote(&mut macros.session, items[1].clone()).unwrap();
            let form = macros.bridge.quote(&mut macros.session, form).unwrap();
            let value = macros.session.invoke(&capture, &[&function, &form, &bindings]).unwrap();
            let observed = macros.bridge.read(&mut macros.session, &value, 0..source.len()).unwrap();
            let Kind::Vector(fields) = &observed.kind else { panic!("validation result") };
            let Kind::String(found) = &fields[0].kind else { panic!("exception message") };
            assert_eq!(String::from_utf16(found).unwrap(), message, "{source}");
            assert!(matches!(&fields[1].kind, Kind::Keyword(key)
                if key.namespace.is_none() && key.name == "macro-syntax-check"));
        }
    }
}

#[cfg(all(test, not(suss_generator)))]
mod defmulti_validation_tests {
    use super::*;

    /// defmulti's syntax checks throw Clojure's messages (#30).
    #[test]
    fn defmulti_validation_matches_clojures_messages() {
        let mut macros = MacroHost::new().unwrap();
        let function = catalog(&macros.catalog).definitions[&("suss.core".to_owned(), "defmulti".to_owned())].clone();
        let capture = macros.session.eval(r#"
          (fn [expand form args]
            (try (apply expand form nil args)
              (catch :default problem (ex-message problem))))
        "#).unwrap();
        for (source, message) in [
            ("(defmulti f identity :default)",
             "The syntax for defmulti has changed. Example: (defmulti name dispatch-fn :default dispatch-value)"),
            ("(defmulti f identity :hierarchy h :other 1)", "Only these options are valid: :default, :hierarchy"),
            ("(defmulti f identity :default 1 :b)", "No value supplied for key: :b"),
        ] {
            let form = read_forms(source).unwrap().remove(0);
            let Kind::List(items) = &form.kind else { panic!("defmulti call") };
            let args = Form { span: form.span.clone(), metadata: Vec::new(), kind: Kind::List(items[1..].to_vec()) };
            let args = macros.bridge.quote(&mut macros.session, args).unwrap();
            let form = macros.bridge.quote(&mut macros.session, form).unwrap();
            let value = macros.session.invoke(&capture, &[&function, &form, &args]).unwrap();
            let observed = macros.bridge.read(&mut macros.session, &value, 0..source.len()).unwrap();
            let Kind::String(found) = &observed.kind else { panic!("exception message for {source}") };
            assert_eq!(String::from_utf16(found).unwrap(), message, "{source}");
        }
    }
}

#[cfg(all(test, not(suss_generator)))]
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
        let mut macros = MacroHost::new().unwrap();
        macros.define("(defmacro retained-policy [] 17)").unwrap();
        let key = ("user".to_owned(), "retained-policy".to_owned());
        let old_hash = catalog(&macros.catalog).declaration_sources[&key].clone();
        assert!(catalog(&macros.catalog).environment_free.contains(&key));
        assert!(catalog(&macros.catalog).caller_form_free.contains(&key));
        assert!(!catalog(&macros.catalog).compiler_catalog_macros.contains_key(&key));
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
        assert_eq!(catalog(&macros.catalog).declaration_sources[&key], old_hash);
        assert!(catalog(&macros.catalog).environment_free.contains(&key));
        assert!(catalog(&macros.catalog).caller_form_free.contains(&key));
        assert!(!catalog(&macros.catalog).compiler_catalog_macros.contains_key(&key));
        // Actual public expansion of the old definition at the SAME2M policy.
        let mut caller = Session::new_repl().unwrap();
        let value = caller.eval_with_macros("(retained-policy)", &mut macros).unwrap();
        let bridge = FormBridge::new(&mut caller).unwrap();
        assert_eq!(bridge.read(&mut caller, &value, 0..1).unwrap().kind, Kind::Long(17));
        assert_eq!(macros.expansion_work_fuel(), 2_000_000);
    }
}


#[cfg(all(test, not(suss_generator)))]
mod lazy_result_interrupt_tests;

#[cfg(all(test, not(suss_generator)))]
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
        let mut macros = MacroHost::new().unwrap();
        let allowance = macros.expansion_work_fuel();
        // Establish that this finite replacement really succeeds at the same
        // ordinary default; no larger allowance or production hook is introduced.
        macros.define(&replacement).unwrap();
        macros.define(OLD).unwrap();
        let key = ("user".to_owned(), "retained-observation".to_owned());
        let old_hash = catalog(&macros.catalog).declaration_sources[&key].clone();
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
        assert_eq!(catalog(&macros.catalog).declaration_sources[&key], old_hash);
        assert!(catalog(&macros.catalog).environment_free.contains(&key));
        assert!(catalog(&macros.catalog).caller_form_free.contains(&key));
        assert!(!catalog(&macros.catalog).compiler_catalog_macros.contains_key(&key));
        let mut caller = Session::new_repl().unwrap();
        let value = caller.eval_with_macros("(retained-observation)", &mut macros).unwrap();
        let bridge = FormBridge::new(&mut caller).unwrap();
        assert_eq!(bridge.read(&mut caller, &value, 0..1).unwrap().kind, Kind::Long(17));
        assert_eq!(macros.expansion_work_fuel(), budget);
    }
}

#[cfg(all(test, not(suss_generator)))]
mod assert_compilation_tests {
    use super::*;

    #[test]
    fn assert_flag_is_read_in_the_macro_store_before_operand_analysis() {
        for mut target in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
            let mut macros = MacroHost::new().unwrap();
            let initial = macros.session.eval("suss.core/*assert*").unwrap();
            assert!(matches!(macros.bridge.read(&mut macros.session, &initial, 0..0).unwrap().kind, Kind::Bool(true)));
            macros.session.eval("(set! suss.core/*assert* false)").unwrap();
            macros.session.collect().unwrap();
            let source = "(assert definitely-missing (also-missing))";
            let omitted = target.eval_with_macros(source, &mut macros).unwrap();
            let mut bridge = FormBridge::new(&mut target).unwrap();
            assert!(matches!(bridge.read(&mut target, &omitted, 0..source.len()).unwrap().kind, Kind::Nil));
            // A disabled macro still has its real compiled arities. Omitting
            // analysis is not a host-side blanket skip of assert forms.
            for source in ["(assert)", "(assert true 1 2)"] {
                assert!(matches!(target.eval_with_macros(source, &mut macros),
                    Err(SessionError::Compile(_))), "{source}");
            }
            let repeated_source = "(try (assert false) (catch :default e (ex-message e)))";
            let disabled_known = target.eval_with_macros(repeated_source, &mut macros).unwrap();
            assert!(matches!(bridge.read(&mut target, &disabled_known, 0..0).unwrap().kind, Kind::Nil));
            macros.session.eval("(set! suss.core/*assert* true)").unwrap();
            macros.session.collect().unwrap();
            // Recompile identical text after the flag changes. Expansion must
            // observe current Macro state rather than replaying a cached result.
            let problem = target.eval_with_macros(repeated_source, &mut macros).unwrap();
            let observed = bridge.read(&mut target, &problem, 0..0).unwrap();
            assert!(matches!(observed.kind, Kind::String(ref units)
                if String::from_utf16(units).unwrap() == "Assert failed: false"));
        }
    }
}
