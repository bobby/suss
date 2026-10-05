//! Persistent native execution of portable fragments and source module plans.
//! No source history is replayed. The native command REPL uses this host.
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, OnceLock,
    },
};
use crate::{
    portable::{
        self,
        modules::{ModuleDiagnostic, ModuleIdentity, PreparedModule},
        resolve::{Environment, Global as CellIdentity, Phase},
        Diagnostic, PreparedFragment,
    },
    runtime_abi,
};
use wasmtime::{
    AnyRef, AsContextMut, Config, Engine, Func, Global, GlobalType, Instance, Linker, Memory,
    Mutability, OwnedRooted, RefType, RootScope, Store, StoreContextMut, Tag, Val, ValType,
};

#[derive(Clone, Debug)]
pub struct SessionOptions {
    pub source_paths: Vec<PathBuf>,
    /// One budget for the whole input/load/invocation, including dependencies.
    pub fuel_per_operation: u64,
}
impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            source_paths: vec![PathBuf::from("src")],
            fuel_per_operation: 10_000_000,
        }
    }
}

#[derive(Debug)]
pub enum SessionError {
    Compile(Diagnostic),
    Module(ModuleDiagnostic),
    Language(SessionValue),
    Trap(wasmtime::Error),
    Host(wasmtime::Error),
    ForeignValue,
}
impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compile(error) => write!(f, "{error}"),
            Self::Module(error) => write!(f, "{error}"),
            Self::Language(_) => f.write_str("Uncaught language exception"),
            Self::Trap(error) => write!(f, "Runtime trap: {error}"),
            Self::Host(error) => write!(f, "Host error: {error}"),
            Self::ForeignValue => {
                f.write_str("Value belongs to another session or a previous reset")
            }
        }
    }
}
impl std::error::Error for SessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Compile(error) => Some(error),
            Self::Module(error) => Some(error),
            Self::Trap(error) | Self::Host(error) => Some(error.as_ref()),
            Self::Language(_) | Self::ForeignValue => None,
        }
    }
}
impl From<wasmtime::Error> for SessionError {
    fn from(error: wasmtime::Error) -> Self {
        if error.is::<wasmtime::Trap>() {
            Self::Trap(error)
        } else {
            Self::Host(error)
        }
    }
}

/// An owned GC root. Drop releases the root; reset invalidates its session identity.
#[derive(Debug)]
pub struct SessionValue {
    identity: u64,
    value: OwnedRooted<AnyRef>,
    handles: Arc<AtomicUsize>,
}
impl SessionValue {
    pub(crate) fn rooted(&self, store: &mut wasmtime::StoreContextMut<'_, ()>) -> wasmtime::Rooted<AnyRef> {
        self.value.to_rooted(store)
    }
}
impl Clone for SessionValue {
    fn clone(&self) -> Self {
        self.handles.fetch_add(1, Ordering::Relaxed);
        Self {
            identity: self.identity,
            value: self.value.clone(),
            handles: self.handles.clone(),
        }
    }
}
impl Drop for SessionValue {
    fn drop(&mut self) {
        self.handles.fetch_sub(1, Ordering::Relaxed);
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionStats {
    pub resident_fragments: usize,
    /// Input sizes of the shared runtime and its core binding initializer,
    /// separate from subsequently installed fragments; not native JIT memory.
    pub base_runtime_artifact_bytes: usize,
    /// Sum of input artifact sizes for resident instances, not retained raw Wasm
    /// or actual JIT memory usage.
    pub resident_artifact_bytes: usize,
    pub binding_cells: usize,
    pub loaded_modules: usize,
    pub external_value_handles: usize,
    /// Allocated GC heap capacity; this is not a live-object or leak counter.
    pub gc_heap_capacity: usize,
    /// Private runtime numeric memory capacity (stack/data/scratch), separate
    /// from GC language objects. Scratch retains a high-water capacity until reset.
    pub numeric_memory_capacity: usize,
}

pub struct Session {
    engine: Engine,
    phase: Phase,
    options: SessionOptions,
    identity: u64,
    handles: Arc<AtomicUsize>,
    store: Store<()>,
    runtime: Instance,
    numeric_memory: Memory,
    linker: Linker<()>,
    environment: Environment,
    cells: BTreeMap<CellIdentity, Global>,
    provided: BTreeSet<ModuleIdentity>,
    resident: Vec<Instance>,
    base_runtime_artifact_bytes: usize,
    artifact_bytes: usize,
    bootstrap_core: bool,
}
/// Binding/catalog transaction only: reachable object effects and resident code are retained.
pub(crate) struct BindingCheckpoint {
    environment: Environment,
    values: BTreeMap<CellIdentity, (OwnedRooted<AnyRef>, i32)>,
}
/// Owned compiler inputs permit macro expansion to execute in this same phase
/// Store while preparation reads an immutable namespace/module snapshot.
pub(crate) struct CompilationSnapshot {
    pub(crate) environment: Environment,
    phase: Phase,
    pub(crate) source_paths: Vec<PathBuf>,
    pub(crate) provided: BTreeSet<ModuleIdentity>,
}
impl CompilationSnapshot {
    pub(crate) fn new(
        environment: Environment,
        phase: Phase,
        source_paths: Vec<PathBuf>,
        provided: BTreeSet<ModuleIdentity>,
    ) -> Self {
        Self { environment, phase, source_paths, provided }
    }
    pub(crate) fn phase(&self) -> Phase {
        self.phase
    }
    pub(crate) fn prepare_with_origin(
        &self, forms: Vec<suss_reader::forms::Form>, span: std::ops::Range<usize>,
        expander: &mut dyn portable::ExpansionHost, origin: Option<&portable::SourceOrigin>,
    ) -> Result<portable::modules::PreparedInput, SessionError> {
        portable::modules::prepare_input_forms_with_origin(
            forms,
            span,
            &self.source_paths,
            &self.environment,
            self.phase,
            &self.provided,
            expander,
            origin,
        )
        .map_err(|error| match error {
            portable::modules::InputDiagnostic::Compile(error) => SessionError::Compile(error),
            portable::modules::InputDiagnostic::Dependency(error) => SessionError::Module(error),
        })
    }
}
fn engine() -> Result<Engine, SessionError> {
    static ENGINE: OnceLock<Result<Engine, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut config = Config::new();
            config
                .wasm_gc(true)
                .wasm_function_references(true)
                .wasm_tail_call(true)
                .wasm_exceptions(true)
                .consume_fuel(true);
            Engine::new(&config).map_err(|error| error.to_string())
        })
        .clone()
        .map_err(|message| SessionError::Host(wasmtime::Error::msg(message)))
}
fn own(
    scope: &mut RootScope<&mut Store<()>>,
    value: Val,
    identity: u64,
    handles: &Arc<AtomicUsize>,
) -> Result<SessionValue, SessionError> {
    let object = value
        .anyref()
        .flatten()
        .ok_or_else(|| wasmtime::Error::msg("Expected non-null runtime Value"))?;
    let value = object.to_owned_rooted(scope)?;
    handles.fetch_add(1, Ordering::Relaxed);
    Ok(SessionValue {
        identity,
        value,
        handles: handles.clone(),
    })
}
// Snapshot the caller's rooted dynamic context, including native interop calls.
fn dynamic_checkpoint(
    scope: &mut RootScope<&mut Store<()>>,
    runtime: Instance,
) -> Result<OwnedRooted<AnyRef>, SessionError> {
    let frame = runtime
        .get_global(&mut *scope, "dynamic-frame")
        .ok_or_else(|| wasmtime::Error::msg("Missing dynamic frame root"))?
        .get(&mut *scope);
    Ok(frame
        .unwrap_anyref()
        .ok_or_else(|| wasmtime::Error::msg("Null dynamic frame root"))?
        .to_owned_rooted(scope)?)
}
fn restore_dynamic(
    scope: &mut RootScope<&mut Store<()>>,
    runtime: Instance,
    checkpoint: &OwnedRooted<AnyRef>,
) -> Result<(), SessionError> {
    let global = runtime
        .get_global(&mut *scope, "dynamic-frame")
        .ok_or_else(|| wasmtime::Error::msg("Missing dynamic frame root"))?;
    let pop = runtime
        .get_func(&mut *scope, "dynamic-pop")
        .ok_or_else(|| wasmtime::Error::msg("Missing dynamic frame restoration"))?;
    let fuel = scope.as_context_mut().get_fuel()?;
    // Restoration executes only finite, checked runtime frame/cell operations,
    // never a source body or finally callback. Preserve the operation's fuel.
    scope.as_context_mut().set_fuel(u64::MAX)?;
    let result = (|| -> wasmtime::Result<()> {
        loop {
            let current = global.get(&mut *scope);
            let current = current
                .unwrap_anyref()
                .ok_or_else(|| wasmtime::Error::msg("Null dynamic frame root"))?;
            let expected = checkpoint.to_rooted(&mut *scope);
            if wasmtime::Rooted::ref_eq(&*scope, current, &expected)? {
                return Ok(());
            }
            // Reaching nil before the checkpoint means a native callback invalidated it.
            if current.as_i31(&*scope)?.is_some() {
                return Err(wasmtime::Error::msg("Dynamic caller context was removed"));
            }
            let current = Val::AnyRef(Some(*current));
            pop.call(&mut *scope, &[current], &mut [])?;
        }
    })();
    scope.as_context_mut().set_fuel(fuel)?;
    if result.is_err() {
        scope.as_context_mut().take_pending_exception();
    }
    result.map_err(SessionError::Host)
}
fn call(
    scope: &mut RootScope<&mut Store<()>>,
    runtime: Instance,
    function: Func,
    args: &[Val],
    identity: u64,
    handles: &Arc<AtomicUsize>,
) -> Result<SessionValue, SessionError> {
    let checkpoint = dynamic_checkpoint(scope, runtime)?;
    let mut result = [Val::null_any_ref()];
    let outcome = match function.call(&mut *scope, args, &mut result) {
        Ok(()) => own(scope, result[0].clone(), identity, handles),
        Err(error) => Err(execution_error(scope, runtime, error, identity, handles)),
    };
    restore_dynamic(scope, runtime, &checkpoint)?;
    outcome
}
fn execution_error(
    scope: &mut RootScope<&mut Store<()>>,
    runtime: Instance,
    error: wasmtime::Error,
    identity: u64,
    handles: &Arc<AtomicUsize>,
) -> SessionError {
    if !error.is::<wasmtime::ThrownException>() {
        // Native callbacks may translate an exception into an ordinary host error.
        // It must not leave the Store carrying a stale pending exception.
        scope.as_context_mut().take_pending_exception();
        return error.into();
    }
    let payload = (|| -> Result<SessionValue, SessionError> {
        let exception = scope
            .as_context_mut()
            .take_pending_exception()
            .ok_or_else(|| wasmtime::Error::msg("Thrown exception has no pending payload"))?;
        let tag = exception.tag(&mut *scope)?;
        let language_tag = runtime
            .get_tag(&mut *scope, "language-exception")
            .ok_or_else(|| wasmtime::Error::msg("Missing language exception tag"))?;
        if !Tag::eq(&tag, &language_tag, &*scope) {
            return Err(SessionError::Host(wasmtime::Error::msg(
                "Unexpected foreign exception tag",
            )));
        }
        let payload = exception.field(&mut *scope, 0)?;
        own(scope, payload, identity, handles)
    })();
    match payload {
        Ok(value) => SessionError::Language(value),
        Err(error) => error,
    }
}

impl Session {
    pub fn new() -> Result<Self, SessionError> {
        Self::with_options(SessionOptions::default())
    }
    /// Compiled core for the native REPL. This is the bounded, provenance-tracked
    /// bootstrap artifact, not complete portable core compatibility.
    pub fn new_repl() -> Result<Self, SessionError> {
        let mut session = Self::new()?;
        session.provision_core()?;
        Ok(session)
    }
    /// Isolated compiled execution for macro bodies. This owns a separate Store
    /// and phase-qualified cells; it does not yet expand source defmacro forms.
    pub fn new_macro() -> Result<Self, SessionError> {
        let mut session = Self::with_options_in(SessionOptions::default(), Phase::Macro)?;
        session.provision_core()?;
        Ok(session)
    }
    fn provision_core(&mut self) -> Result<(), SessionError> {
        let namespace = self.current_namespace().to_owned();
        let fragment = portable::bootstrap::shipped(self.phase)
            .map_err(SessionError::Compile)?.clone();
        self.eval_prepared(portable::modules::PreparedInput { modules: Vec::new(), fragment })?;
        self.enter_namespace(&namespace)?;
        self.bootstrap_core = true;
        Ok(())
    }
    pub fn with_options(options: SessionOptions) -> Result<Self, SessionError> {
        Self::with_options_in(options, Phase::Runtime)
    }
    /// Create a minimal session whose artifacts and cells use one fixed phase.
    pub fn with_options_in(options: SessionOptions, phase: Phase) -> Result<Self, SessionError> {
        Self::with_engine_in(engine()?, options, phase)
    }
    /// Caller-supplied engines must enable ABI features and fuel consumption.
    pub fn with_engine(engine: Engine, options: SessionOptions) -> Result<Self, SessionError> {
        Self::with_engine_in(engine, options, Phase::Runtime)
    }
    pub fn with_engine_in(
        engine: Engine,
        options: SessionOptions,
        phase: Phase,
    ) -> Result<Self, SessionError> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let identity = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let mut store = Store::new(&engine, ());
        store.set_fuel(u64::MAX)?;
        let runtime_bytes = runtime_abi::module();
        runtime_abi::verify_artifact(&runtime_bytes, &runtime_abi::Manifest::default())
            .map_err(|message| SessionError::Host(wasmtime::Error::msg(message)))?;
        let runtime = Instance::new(
            &mut store,
            &crate::portable_module_cache::compile(&engine, &runtime_bytes)?,
            &[],
        )?;
        let numeric_memory = runtime
            .get_memory(&mut store, "numeric-scratch-memory")
            .ok_or_else(|| wasmtime::Error::msg("Missing numeric scratch memory"))?;
        let mut linker = Linker::new(&engine);
        linker.instance(&mut store, "suss.runtime", runtime)?;
        let environment = Environment::default();
        let mut cells = BTreeMap::new();
        let bindings = portable::core_bindings::compile(phase).map_err(SessionError::Compile)?;
        runtime_abi::verify_artifact(&bindings.wasm, &runtime_abi::Manifest::default())
            .map_err(|message| SessionError::Host(wasmtime::Error::msg(message)))?;
        portable::artifact_identity::verify(&bindings.wasm, portable::artifact_identity::Expected {
            phase: Some(phase), macro_dependencies: Some(&[]), ..Default::default()
        }).map_err(|message| SessionError::Host(wasmtime::Error::msg(message)))?;
        let module = crate::portable_module_cache::compile(&engine, &bindings.wasm)?;
        let initialized = linker.instantiate(&mut store, &module)?;
        for identity in bindings.cells {
            let global = initialized.get_global(&mut store, &identity.import_name())
                .ok_or_else(|| wasmtime::Error::msg("Missing compiled core binding"))?;
            linker.define(&store, identity.import_module(), &identity.import_name(), global)?;
            cells.insert(identity, global);
        }
        let provided = BTreeSet::from([
            ModuleIdentity::new(phase, "suss.core").map_err(SessionError::Compile)?
        ]);
        Ok(Self {
            engine,
            phase,
            options,
            identity,
            handles: Arc::new(AtomicUsize::new(0)),
            store,
            runtime,
            numeric_memory,
            linker,
            environment,
            cells,
            provided,
            resident: Vec::new(),
            base_runtime_artifact_bytes: runtime_bytes.len() + bindings.wasm.len(),
            artifact_bytes: 0,
            bootstrap_core: false,
        })
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn options(&self) -> &SessionOptions {
        &self.options
    }
    pub fn set_operation_fuel(&mut self, fuel: u64) {
        self.options.fuel_per_operation = fuel;
    }
    pub fn current_namespace(&self) -> &str {
        self.environment.current_namespace(self.phase)
    }
    // Use the same binary64 formatter as compiled arithmetic. The REPL never
    // recompiles or re-evaluates the input to print its already-rooted result.
    pub(crate) fn number_text(&mut self, value: &SessionValue) -> Result<String, SessionError> {
        self.check(value)?;
        self.store.set_fuel(self.options.fuel_per_operation)?;
        let runtime = self.runtime;
        self.inspect(value, |mut store, value| {
            let function = runtime
                .get_func(&mut store, "coerce-string")
                .ok_or_else(|| wasmtime::Error::msg("Missing numeric formatter"))?;
            let mut result = [Val::null_any_ref()];
            function.call(&mut store, &[value], &mut result)?;
            let array = result[0]
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            let units = array
                .elems(&mut store)?
                .map(|unit| unit.unwrap_i32() as u16)
                .collect::<Vec<_>>();
            String::from_utf16(&units).map_err(wasmtime::Error::from)
        })
    }
    pub fn enter_namespace(&mut self, namespace: &str) -> Result<(), SessionError> {
        self.environment
            .enter_namespace(self.phase, namespace)
            .map_err(SessionError::Compile)
    }
    pub fn stats(&self) -> SessionStats {
        SessionStats {
            resident_fragments: self.resident.len(),
            base_runtime_artifact_bytes: self.base_runtime_artifact_bytes,
            resident_artifact_bytes: self.artifact_bytes,
            binding_cells: self.cells.len(),
            loaded_modules: self.provided.len() - 1,
            external_value_handles: self.handles.load(Ordering::Relaxed),
            gc_heap_capacity: self.store.gc_heap_capacity(),
            numeric_memory_capacity: self.numeric_memory.data_size(&self.store),
        }
    }
    pub fn collect(&mut self) -> Result<(), SessionError> {
        self.store.gc(None).map_err(Into::into)
    }
    /// Create the replacement before discarding the old Store. No replay or retained
    /// fragment state crosses reset, and old/foreign handles are checked before use.
    pub fn reset(&mut self) -> Result<(), SessionError> {
        *self = self.replacement()?;
        Ok(())
    }
    pub(crate) fn replacement(&self) -> Result<Self, SessionError> {
        let mut replacement =
            Self::with_engine_in(self.engine.clone(), self.options.clone(), self.phase)?;
        if self.bootstrap_core {
            replacement.provision_core()?;
        }
        Ok(replacement)
    }
    pub(crate) fn resolves_macro_definition(&self, symbol: &suss_reader::Symbol) -> bool {
        self.environment
            .resolves_bootstrap_name(self.phase, symbol, "defmacro")
    }
    fn check(&self, value: &SessionValue) -> Result<(), SessionError> {
        if value.identity == self.identity {
            Ok(())
        } else {
            Err(SessionError::ForeignValue)
        }
    }
    /// Native inspection/interop in a bounded root scope. Do not return raw Rooted
    /// references from the callback; use ordinary observations or owned host data.
    pub fn inspect<R>(
        &mut self,
        value: &SessionValue,
        f: impl FnOnce(StoreContextMut<'_, ()>, Val) -> wasmtime::Result<R>,
    ) -> Result<R, SessionError> {
        self.check(value)?;
        let mut scope = RootScope::new(&mut self.store);
        let checkpoint = dynamic_checkpoint(&mut scope, self.runtime)?;
        let value = Val::AnyRef(Some(value.value.to_rooted(&mut scope)));
        let result = f(scope.as_context_mut(), value);
        let result = match result {
            Ok(_) if scope.as_context_mut().has_pending_exception() => Err(wasmtime::Error::msg(
                "Native callback returned success with a pending exception",
            )),
            result => result,
        };
        let outcome = result.map_err(|error| {
            execution_error(
                &mut scope,
                self.runtime,
                error,
                self.identity,
                &self.handles,
            )
        });
        restore_dynamic(&mut scope, self.runtime, &checkpoint)?;
        outcome
    }
    /// A single fueled operation inside the normal inspection/root/exception
    /// scope. Compiler-data decoding may call a captured source closure without
    /// escaping raw references or replenishing fuel per sequence element.
    pub(crate) fn data_inspect_calls<R>(
        &mut self,
        value: &SessionValue,
        function: &SessionValue,
        f: impl FnOnce(
            StoreContextMut<'_, ()>,
            Val,
            wasmtime::Func,
            wasmtime::Func,
            Val,
        ) -> wasmtime::Result<R>,
    ) -> Result<R, SessionError> {
        self.check(function)?;
        self.budget()?;
        let args_new = self
            .runtime
            .get_func(&mut self.store, "args-new")
            .expect("runtime args");
        let invoke = self
            .runtime
            .get_func(&mut self.store, "invoke")
            .expect("runtime invoke");
        self.inspect(value, |mut store, value| {
            let function = Val::AnyRef(Some(function.rooted(&mut store)));
            f(store, value, args_new, invoke, function)
        })
    }
    fn budget(&mut self) -> Result<(), SessionError> {
        self.store
            .set_fuel(self.options.fuel_per_operation)
            .map_err(Into::into)
    }
    /// Validate all artifacts before allocating bindings. Stage cell imports and
    /// instantiate all fragments before publishing declarations or executing eval.
    fn install(
        &mut self,
        bytes: &[&[u8]],
        environment: Environment,
        cells: &[CellIdentity],
    ) -> Result<Vec<Instance>, SessionError> {
        let mut compiled = Vec::new();
        for bytes in bytes {
            runtime_abi::verify_artifact(bytes, &runtime_abi::Manifest::default())
                .map_err(|message| SessionError::Host(wasmtime::Error::msg(message)))?;
            portable::artifact_identity::verify(bytes, portable::artifact_identity::Expected {
                phase: Some(self.phase), ..Default::default()
            }).map_err(|message| SessionError::Host(wasmtime::Error::msg(message)))?;
            compiled.push(crate::portable_module_cache::compile(&self.engine, bytes)?);
        }
        let mut linker = self.linker.clone();
        let mut staged = BTreeMap::new();
        for identity in cells {
            if self.cells.contains_key(identity) || staged.contains_key(identity) {
                continue;
            }
            let mut scope = RootScope::new(&mut self.store);
            let mut result = [Val::null_any_ref()];
            self.runtime
                .get_func(&mut scope, "binding-unbound")
                .unwrap()
                .call(&mut scope, &[], &mut result)?;
            let ty = result[0]
                .unwrap_anyref()
                .unwrap()
                .as_struct(&scope)?
                .unwrap()
                .ty(&scope)?;
            let global = Global::new(
                &mut scope,
                GlobalType::new(
                    ValType::Ref(RefType::new(false, ty.into())),
                    Mutability::Const,
                ),
                result[0].clone(),
            )?;
            linker.define(
                &scope,
                identity.import_module(),
                &identity.import_name(),
                global,
            )?;
            staged.insert(identity.clone(), global);
        }
        let mut instances = Vec::new();
        for module in compiled {
            instances.push(linker.instantiate(&mut self.store, &module)?);
        }
        self.linker = linker;
        self.cells.extend(staged);
        self.environment = environment;
        self.resident.extend(instances.iter().copied());
        self.artifact_bytes += bytes.iter().map(|bytes| bytes.len()).sum::<usize>();
        Ok(instances)
    }
    fn eval_instance(&mut self, instance: Instance) -> Result<SessionValue, SessionError> {
        let mut scope = RootScope::new(&mut self.store);
        let function = instance
            .get_func(&mut scope, "eval")
            .ok_or_else(|| wasmtime::Error::msg("Missing fragment eval"))?;
        call(
            &mut scope,
            self.runtime,
            function,
            &[],
            self.identity,
            &self.handles,
        )
    }
    pub fn eval(&mut self, source: &str) -> Result<SessionValue, SessionError> {
        let prepared = portable::modules::prepare_input(
            source,
            &self.options.source_paths,
            &self.environment,
            self.phase,
            &self.provided,
        )
        .map_err(|error| match error {
            portable::modules::InputDiagnostic::Compile(error) => SessionError::Compile(error),
            portable::modules::InputDiagnostic::Dependency(error) => SessionError::Module(error),
        })?;
        self.eval_prepared(prepared)
    }
    /// Publish only actual compiled macro roots owned by this phase Store.
    pub(crate) fn declare_macro_exports(
        &mut self,
        namespace: &str,
        names: &[String],
    ) -> Result<(), SessionError> {
        self.environment
            .declare_macro_exports(self.phase, namespace, names)
            .map_err(SessionError::Compile)
    }
    pub(crate) fn set_source_paths(&mut self, paths: &[PathBuf]) {
        self.options.source_paths = paths.to_vec();
    }
    pub(crate) fn invalidate_source_modules(&mut self, identities: &[ModuleIdentity]) {
        for identity in identities {
            self.provided.remove(identity);
        }
    }
    /// Called only after every form of this phase module has initialized.
    pub(crate) fn initialized_source_namespace(
        &mut self,
        namespace: &str,
    ) -> Result<(), SessionError> {
        self.provided
            .insert(ModuleIdentity::new(self.phase, namespace).map_err(SessionError::Compile)?);
        Ok(())
    }
    pub(crate) fn binding_checkpoint(&mut self) -> Result<BindingCheckpoint, SessionError> {
        let mut values = BTreeMap::new();
        let mut scope = RootScope::new(&mut self.store);
        for (identity, global) in &self.cells {
            let cell = global.get(&mut scope).unwrap_anyref().unwrap().as_struct(&scope)?.unwrap();
            let value = cell.field(&mut scope, 0)?.unwrap_anyref().unwrap().to_owned_rooted(&mut scope)?;
            let bound = cell.field(&mut scope, 1)?.unwrap_i32();
            values.insert(identity.clone(), (value, bound));
        }
        Ok(BindingCheckpoint { environment: self.environment.clone(),
            values })
    }
    pub(crate) fn restore_bindings(&mut self, checkpoint: BindingCheckpoint, globals: &[CellIdentity]) -> Result<(), SessionError> {
        let mut scope = RootScope::new(&mut self.store);
        for identity in globals {
            if let Some(global) = self.cells.get(identity) {
                let cell = global.get(&mut scope).unwrap_anyref().unwrap().as_struct(&scope)?.unwrap();
                if let Some((value, bound)) = checkpoint.values.get(identity) {
                    let value = Val::AnyRef(Some(value.to_rooted(&mut scope)));
                    cell.set_field(&mut scope, 0, value)?;
                    cell.set_field(&mut scope, 1, Val::I32(*bound))?;
                } else {
                    let nil = AnyRef::from_i31(&mut scope, wasmtime::I31::new_u32(0).unwrap());
                    cell.set_field(&mut scope, 0, Val::AnyRef(Some(nil)))?;
                    cell.set_field(&mut scope, 1, Val::I32(0))?;
                }
            }
        }
        self.environment.restore_declarations(&checkpoint.environment, globals, self.phase);
        // Keep initialized dependency cells/modules and resident code. Unpublished
        // fresh cells remain unbound and reusable under their stable identity.
        Ok(())
    }
    pub(crate) fn compilation_snapshot(&self) -> CompilationSnapshot {
        CompilationSnapshot {
            environment: self.environment.clone(),
            phase: self.phase,
            source_paths: self.options.source_paths.clone(),
            provided: self.provided.clone(),
        }
    }
    pub fn eval_with_macros(
        &mut self,
        source: &str,
        expander: &mut dyn portable::ExpansionHost,
    ) -> Result<SessionValue, SessionError> {
        let forms = suss_reader::forms::read_forms(source).map_err(|error| {
            SessionError::Compile(Diagnostic {
                span: error.span,
                message: error.message,
            })
        })?;
        let prepared = self
            .compilation_snapshot()
            .prepare_with_origin(forms, 0..source.len(), expander, Some(&portable::SourceOrigin::new(source, None)))?;
        self.eval_prepared(prepared)
    }
    /// Compile reader or expanded forms through the ordinary phase/module pipeline.
    /// No source printing/rereading, initializer replay or host interpretation.
    pub fn eval_forms(
        &mut self,
        forms: Vec<suss_reader::forms::Form>,
        span: std::ops::Range<usize>,
    ) -> Result<SessionValue, SessionError> {
        let prepared = portable::modules::prepare_input_forms(
            forms,
            span,
            &self.options.source_paths,
            &self.environment,
            self.phase,
            &self.provided,
        )
        .map_err(|error| match error {
            portable::modules::InputDiagnostic::Compile(error) => SessionError::Compile(error),
            portable::modules::InputDiagnostic::Dependency(error) => SessionError::Module(error),
        })?;
        self.eval_prepared(prepared)
    }
    pub(crate) fn eval_prepared(
        &mut self,
        prepared: portable::modules::PreparedInput,
    ) -> Result<SessionValue, SessionError> {
        let PreparedFragment {
            wasm,
            environment,
            cells,
            ..
        } = prepared.fragment;
        self.budget()?;
        let mut bytes: Vec<_> = prepared
            .modules
            .iter()
            .map(|module| module.wasm.as_slice())
            .collect();
        bytes.push(&wasm);
        let mut instances = self.install(&bytes, environment, &cells)?;
        let input = instances.pop().unwrap();
        self.initialize_modules(prepared.modules, instances)?;
        self.eval_instance(input)
    }
    /// Load source once and preserve the caller's namespace scope. Successful
    /// dependencies remain provided if a later module initializer fails.
    pub fn load_namespace(
        &mut self,
        namespace: &str,
    ) -> Result<Option<SessionValue>, SessionError> {
        self.load_namespace_with_provided(namespace, self.provided.clone(), false, None)
    }
    pub fn load_namespace_with_macros(
        &mut self,
        namespace: &str,
        expander: &mut dyn portable::ExpansionHost,
    ) -> Result<Option<SessionValue>, SessionError> {
        self.load_namespace_with_provided(namespace, self.provided.clone(), false, Some(expander))
    }
    /// Recompile and initialize a namespace using its existing live cells. When
    /// dependencies is true, reload its reachable dependencies in require order.
    /// The supplied core profile is reprovisioned only by reset, never file reload.
    pub fn reload_namespace(
        &mut self,
        namespace: &str,
        dependencies: bool,
    ) -> Result<Option<SessionValue>, SessionError> {
        self.reload_namespace_using(namespace, dependencies, None)
    }
    pub fn reload_namespace_with_macros(
        &mut self,
        namespace: &str,
        dependencies: bool,
        expander: &mut dyn portable::ExpansionHost,
    ) -> Result<Option<SessionValue>, SessionError> {
        self.reload_namespace_using(namespace, dependencies, Some(expander))
    }
    fn reload_namespace_using(
        &mut self,
        namespace: &str,
        dependencies: bool,
        expander: Option<&mut dyn portable::ExpansionHost>,
    ) -> Result<Option<SessionValue>, SessionError> {
        let identity = ModuleIdentity::new(self.phase, namespace).map_err(SessionError::Compile)?;
        if identity.namespace() == "suss.core" {
            return Err(SessionError::Compile(Diagnostic {
                span: 0..0,
                message: "The supplied core profile is reprovisioned by session reset".into(),
            }));
        }
        let provided = self
            .provided
            .iter()
            .filter(|module| {
                if dependencies {
                    module.namespace() == "suss.core" || module.phase() != self.phase
                } else {
                    **module != identity
                }
            })
            .cloned()
            .collect();
        self.load_namespace_with_provided(namespace, provided, true, expander)
    }
    fn load_namespace_with_provided(
        &mut self,
        namespace: &str,
        provided: BTreeSet<ModuleIdentity>,
        reloading: bool,
        expander: Option<&mut dyn portable::ExpansionHost>,
    ) -> Result<Option<SessionValue>, SessionError> {
        let mut plan = if let Some(expander) = expander {
            portable::modules::prepare_modules_with_expander(
                namespace,
                &self.options.source_paths,
                &self.environment,
                self.phase,
                &provided,
                expander,
            )
        } else {
            portable::modules::prepare_modules(
                namespace,
                &self.options.source_paths,
                &self.environment,
                self.phase,
                &provided,
            )
        }
        .map_err(SessionError::Module)?;
        plan.environment
            .enter_namespace(self.phase, self.current_namespace())
            .map_err(SessionError::Compile)?;
        self.budget()?;
        let bytes: Vec<_> = plan
            .modules
            .iter()
            .map(|module| module.wasm.as_slice())
            .collect();
        let instances = self.install(&bytes, plan.environment, &plan.cells)?;
        if reloading {
            // Preparation/linking failed without changing loaded identities. Once
            // execution starts, every selected unit must complete initialization
            // before it can be considered loaded again. Earlier effects remain.
            for module in &plan.modules {
                self.provided.remove(&module.identity);
            }
        }
        self.initialize_modules(plan.modules, instances)
    }
    fn initialize_modules(
        &mut self,
        modules: Vec<PreparedModule>,
        instances: Vec<Instance>,
    ) -> Result<Option<SessionValue>, SessionError> {
        let mut result = None;
        for (module, instance) in modules.into_iter().zip(instances) {
            let value = self.eval_instance(instance)?;
            self.provided.insert(module.identity);
            result = Some(value);
        }
        Ok(result)
    }
    /// Allocate compiler data scalars through the shared runtime without compiling
    /// source, replaying initializers or adding resident fragments.
    pub(crate) fn data_scalar(&mut self, literal: &portable::hir::Literal) -> Result<SessionValue, SessionError> {
        use portable::hir::Literal;
        self.budget()?;
        let mut scope = RootScope::new(&mut self.store);
        match literal {
            Literal::Undefined => Err(SessionError::Host(wasmtime::Error::msg("Compiler data has no undefined source literal"))),
            Literal::Nil | Literal::Bool(_) => {
                let sentinel = match literal {
                    Literal::Nil => 0,
                    Literal::Bool(false) => 2,
                    Literal::Bool(true) => 4,
                    _ => unreachable!(),
                };
                let value = Val::AnyRef(Some(AnyRef::from_i31(&mut scope, wasmtime::I31::new_u32(sentinel).expect("runtime sentinel fits i31"))));
                own(&mut scope, value, self.identity, &self.handles)
            }
            Literal::Number(value) => {
                let function = self.runtime.get_func(&mut scope, "number-box").expect("shared runtime number constructor");
                call(&mut scope, self.runtime, function, &[Val::F64(value.to_bits())], self.identity, &self.handles)
            }
            Literal::String(units) => {
                let length = i32::try_from(units.len()).map_err(|_| SessionError::Host(wasmtime::Error::msg("Compiler data string exceeds runtime length")))?;
                let function = self.runtime.get_func(&mut scope, "string-new").expect("shared runtime string constructor");
                let value = call(&mut scope, self.runtime, function, &[Val::I32(length)], self.identity, &self.handles)?;
                let array = value.value.to_rooted(&mut scope).as_array(&scope)?.expect("runtime string is a UTF-16 array");
                for (index, unit) in units.iter().enumerate() {
                    array.set(&mut scope, index as u32, Val::I32(i32::from(*unit)))?;
                }
                Ok(value)
            }
        }
    }
    /// Build a language source array from already rooted values. The runtime
    /// clones the internal argument buffer, preserving element identities.
    pub(crate) fn data_array(&mut self, values: &[&SessionValue]) -> Result<SessionValue, SessionError> {
        for value in values { self.check(value)?; }
        let length = i32::try_from(values.len()).map_err(|_| SessionError::Host(wasmtime::Error::msg("Compiler data array exceeds runtime length")))?;
        self.budget()?;
        let mut scope = RootScope::new(&mut self.store);
        let mut result = [Val::null_any_ref()];
        self.runtime.get_func(&mut scope, "args-new").expect("shared runtime argument constructor")
            .call(&mut scope, &[Val::I32(length)], &mut result)?;
        let array = result[0].unwrap_anyref().unwrap().as_array(&scope)?.expect("runtime argument array");
        for (index, value) in values.iter().enumerate() {
            let value = Val::AnyRef(Some(value.value.to_rooted(&mut scope)));
            array.set(&mut scope, index as u32, value)?;
        }
        let function = self.runtime.get_func(&mut scope, "source-array-new").expect("shared runtime source array constructor");
        call(&mut scope, self.runtime, function, &result, self.identity, &self.handles)
    }
    /// Follow the compiler's `new` path using a captured canonical class value.
    pub(crate) fn data_construct(&mut self, class: &SessionValue, arguments: &[&SessionValue]) -> Result<SessionValue, SessionError> {
        self.check(class)?;
        for argument in arguments { self.check(argument)?; }
        self.budget()?;
        let constructor = {
            let mut scope = RootScope::new(&mut self.store);
            let function = self.runtime.get_func(&mut scope, "constructor-descriptor").expect("shared runtime class descriptor");
            let value = Val::AnyRef(Some(class.value.to_rooted(&mut scope)));
            let descriptor = call(&mut scope, self.runtime, function, &[value], self.identity, &self.handles)?;
            let function = self.runtime.get_func(&mut scope, "source-constructor-new").expect("shared runtime source constructor");
            let value = Val::AnyRef(Some(descriptor.value.to_rooted(&mut scope)));
            call(&mut scope, self.runtime, function, &[value], self.identity, &self.handles)?
        };
        self.invoke(&constructor, arguments)
    }
    pub(crate) fn data_descriptor(&mut self, value: &SessionValue, class: bool) -> Result<SessionValue, SessionError> {
        self.check(value)?;
        let mut scope = RootScope::new(&mut self.store);
        let value = value.value.to_rooted(&mut scope);
        if class {
            let function = self.runtime.get_func(&mut scope, "constructor-descriptor").expect("shared runtime class descriptor");
            call(&mut scope, self.runtime, function, &[Val::AnyRef(Some(value))], self.identity, &self.handles)
        } else {
            let object = value.as_struct(&scope)?.expect("canonical compiler object");
            let descriptor = object.field(&mut scope, 0)?;
            own(&mut scope, descriptor, self.identity, &self.handles)
        }
    }
    /// Retain the field values of an already checked canonical compiler object.
    pub(crate) fn data_fields(
        &mut self,
        value: &SessionValue,
    ) -> Result<Vec<SessionValue>, SessionError> {
        self.check(value)?;
        let mut scope = RootScope::new(&mut self.store);
        let value = value.value.to_rooted(&mut scope);
        let object = value.as_struct(&scope)?.ok_or_else(|| {
            SessionError::Host(wasmtime::Error::msg("Compiler data needs object storage"))
        })?;
        let storage = object
            .field(&mut scope, 1)?
            .unwrap_anyref()
            .unwrap()
            .as_array(&scope)?
            .ok_or_else(|| {
                SessionError::Host(wasmtime::Error::msg("Compiler data needs field storage"))
            })?;
        let fields = storage.elems(&mut scope)?.collect::<Vec<_>>();
        fields
            .into_iter()
            .map(|field| own(&mut scope, field, self.identity, &self.handles))
            .collect()
    }
    pub fn invoke(
        &mut self,
        function: &SessionValue,
        arguments: &[&SessionValue],
    ) -> Result<SessionValue, SessionError> {
        self.check(function)?;
        for value in arguments {
            self.check(value)?;
        }
        let length = i32::try_from(arguments.len()).map_err(|_| {
            SessionError::Host(wasmtime::Error::msg("Too many invocation arguments"))
        })?;
        self.budget()?;
        let mut scope = RootScope::new(&mut self.store);
        let mut result = [Val::null_any_ref()];
        self.runtime
            .get_func(&mut scope, "args-new")
            .unwrap()
            .call(&mut scope, &[Val::I32(length)], &mut result)?;
        let array = result[0]
            .unwrap_anyref()
            .unwrap()
            .as_array(&scope)?
            .unwrap();
        for (index, value) in arguments.iter().enumerate() {
            let value = Val::AnyRef(Some(value.value.to_rooted(&mut scope)));
            array.set(&mut scope, index as u32, value)?;
        }
        let function = Val::AnyRef(Some(function.value.to_rooted(&mut scope)));
        let invoke = self.runtime.get_func(&mut scope, "invoke").unwrap();
        call(
            &mut scope,
            self.runtime,
            invoke,
            &[function, result[0].clone()],
            self.identity,
            &self.handles,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_lifecycle_native_callback_trap_restores_dynamic_context() {
        let mut session = Session::new().unwrap();
        session.eval("(def ^:dynamic *value* 1)").unwrap();
        let body = session
            .eval("(fn [] (binding [*value* 7] (loop [] (recur))))")
            .unwrap();
        let invoke = session
            .runtime
            .get_func(&mut session.store, "invoke")
            .unwrap();
        let args_new = session
            .runtime
            .get_func(&mut session.store, "args-new")
            .unwrap();
        session.store.set_fuel(5_000).unwrap();
        let result = session.inspect(&body, |mut store, body| {
            let mut args = [Val::null_any_ref()];
            args_new.call(&mut store, &[Val::I32(0)], &mut args)?;
            invoke.call(
                &mut store,
                &[body, args[0].clone()],
                &mut [Val::null_any_ref()],
            )
        });
        assert!(matches!(result, Err(SessionError::Trap(_))));
        session.set_operation_fuel(50_000);
        let value = session.eval("*value*").unwrap();
        let bits = session
            .inspect(&value, |mut store, value| {
                let number = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                Ok(number.field(&mut store, 0)?.unwrap_f64())
            })
            .unwrap();
        assert_eq!(bits.to_bits(), 1.0f64.to_bits());
    }
    #[test]
    fn session_lifecycle_numeric_helper_trap_resets_interrupted_rust_stack() {
        let mut session = Session::new().unwrap();
        let value = session.eval("\"1.2345678901234567890123456789\"").unwrap();
        let function = session
            .runtime
            .get_func(&mut session.store, "coerce-number")
            .unwrap();
        let stack = session
            .runtime
            .get_global(&mut session.store, "numeric-stack-pointer")
            .unwrap();
        let initial = stack.get(&mut session.store).unwrap_i32();
        let mut interrupted = false;
        for fuel in (64..=4096).step_by(64) {
            stack.set(&mut session.store, Val::I32(initial)).unwrap();
            session.store.set_fuel(fuel).unwrap();
            let result = session.inspect(&value, |mut scope, value| {
                let mut output = [Val::F64(0)];
                function.call(&mut scope, &[value], &mut output)?;
                Ok(output[0].unwrap_f64())
            });
            if matches!(result, Err(SessionError::Trap(ref error)) if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel))
                && stack.get(&mut session.store).unwrap_i32() != initial
            {
                interrupted = true;
                break;
            }
        }
        assert!(
            interrupted,
            "must actually interrupt a Rust helper frame, not just its GC copy loop"
        );
        session.set_operation_fuel(100000);
        let output = session.eval("(+ 42 \"\")").unwrap();
        let units = session
            .inspect(&output, |mut scope, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&scope)?.unwrap();
                Ok(array
                    .elems(&mut scope)?
                    .map(|unit| unit.unwrap_i32() as u16)
                    .collect::<Vec<_>>())
            })
            .unwrap();
        assert_eq!(units, [52, 50]);
        assert_eq!(stack.get(&mut session.store).unwrap_i32(), initial);
    }
    #[test]
    fn session_lifecycle_native_callback_foreign_exception_does_not_poison_the_store() {
        let mut session = Session::new().unwrap();
        let value = session.eval("7").unwrap();
        let error = session
            .inspect(&value, |mut store, _| -> wasmtime::Result<()> {
                let ty = wasmtime::ExnType::new(store.engine(), [ValType::I32])?;
                let tag = Tag::new(&mut store, &ty.tag_type())?;
                let allocator = wasmtime::ExnRefPre::new(&mut store, ty);
                let exception =
                    wasmtime::ExnRef::new(&mut store, &allocator, &tag, &[Val::I32(42)])?;
                store.throw(exception)
            })
            .unwrap_err();
        assert!(matches!(error, SessionError::Host(_)));
        assert!(!session.store.has_pending_exception());
        session.eval("42").unwrap();
    }
    #[test]
    fn session_lifecycle_callback_translated_exception_clears_pending_state() {
        let mut session = Session::new().unwrap();
        let value = session.eval("7").unwrap();
        for translated_success in [false, true] {
            let error = session
                .inspect(&value, |mut store, _| -> wasmtime::Result<()> {
                    let ty = wasmtime::ExnType::new(store.engine(), [ValType::I32])?;
                    let tag = Tag::new(&mut store, &ty.tag_type())?;
                    let allocator = wasmtime::ExnRefPre::new(&mut store, ty);
                    let exception =
                        wasmtime::ExnRef::new(&mut store, &allocator, &tag, &[Val::I32(42)])?;
                    let thrown = store.throw::<()>(exception);
                    if translated_success {
                        let _ = thrown;
                        Ok(())
                    } else {
                        thrown.map_err(|_| {
                            wasmtime::Error::msg("native callback translated exception")
                        })
                    }
                })
                .unwrap_err();
            let SessionError::Host(error) = error else {
                panic!("translated callback exceptions must be host errors")
            };
            assert!(error.to_string().contains(if translated_success {
                "returned success with a pending exception"
            } else {
                "native callback translated exception"
            }));
            assert!(!session.store.has_pending_exception());
            assert_eq!(session.stats().external_value_handles, 1);
            let next = session.eval("42").unwrap();
            let bits = session
                .inspect(&next, |mut store, value| {
                    let number = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    let fields = number.fields(&mut store)?.collect::<Vec<_>>();
                    let [Val::F64(bits)] = fields.as_slice() else {
                        panic!("expected exact Number layout")
                    };
                    Ok(*bits)
                })
                .unwrap();
            assert_eq!(bits, 42.0f64.to_bits());
        }
    }
    #[test]
    fn session_lifecycle_all_artifacts_are_gated_before_allocating_or_publishing_cells() {
        let mut session = Session::new().unwrap();
        let prepared =
            portable::prepare_fragment("(def ghost 7)", &session.environment, Phase::Runtime)
                .unwrap();
        let before = session.stats();
        assert!(matches!(
            session.install(
                &[&prepared.wasm, &[]],
                prepared.environment.clone(),
                &prepared.cells
            ),
            Err(SessionError::Host(_))
        ));
        assert_eq!(session.stats(), before);
        assert!(matches!(
            session.eval("ghost"),
            Err(SessionError::Compile(_))
        ));
        // A missing actual cell import also fails linking before publication/eval.
        assert!(matches!(
            session.install(&[&prepared.wasm], prepared.environment, &[]),
            Err(SessionError::Host(_))
        ));
        assert_eq!(session.stats(), before);
        assert!(matches!(
            session.eval("ghost"),
            Err(SessionError::Compile(_))
        ));
    }
    #[test]
    fn session_lifecycle_compiler_build_identity_is_checked_before_any_cells() {
        fn read_uleb(bytes: &[u8], offset: &mut usize) -> usize {
            let mut value = 0usize;
            let mut shift = 0;
            loop {
                let byte = bytes[*offset]; *offset += 1;
                value |= ((byte & 127) as usize) << shift;
                if byte & 128 == 0 { return value; }
                shift += 7;
            }
        }
        fn write_uleb(mut value: usize, bytes: &mut Vec<u8>) {
            loop {
                let mut byte = (value & 127) as u8; value >>= 7;
                if value != 0 { byte |= 128; }
                bytes.push(byte);
                if value == 0 { break; }
            }
        }
        let mut session = Session::new().unwrap();
        session.eval("(def keep 17)").unwrap();
        let prepared = portable::prepare_fragment("(def ghost 7)", &session.environment, Phase::Runtime).unwrap();
        // Remove the source identity section if emitted, keeping every other
        // byte intact. This fixture runs before and after the production gate.
        let mut wasm = prepared.wasm[..8].to_vec();
        let mut offset = 8;
        while offset < prepared.wasm.len() {
            let start = offset;
            let id = prepared.wasm[offset]; offset += 1;
            let length = read_uleb(&prepared.wasm, &mut offset);
            let end = offset + length;
            let skip = if id == 0 {
                let mut name_offset = offset;
                let name_length = read_uleb(&prepared.wasm, &mut name_offset);
                &prepared.wasm[name_offset..name_offset + name_length] == b"suss.source-artifact"
            } else { false };
            if !skip { wasm.extend_from_slice(&prepared.wasm[start..end]); }
            offset = end;
        }
        let abi = runtime_abi::Manifest::default();
        let manifest = serde_json::json!({
            "format_version": 1,
            "compiler_source_sha256": "0".repeat(64),
            "runtime_abi": abi.runtime_abi,
            "compiler": abi.compiler,
            "wasm_tools": abi.wasm_tools,
            "target": "portable-wasm-gc-shared-abi",
            "profile": "default",
            "flags": [],
            "phase": "runtime",
            "source_sha256": portable::bootstrap::sha256(b"(def ghost 7)"),
            "source_path": null,
            "macro_dependencies": null,
            "wasm_sha256": portable::bootstrap::sha256(&wasm)
        });
        let mut data = Vec::new();
        write_uleb(b"suss.source-artifact".len(), &mut data);
        data.extend_from_slice(b"suss.source-artifact");
        data.extend_from_slice(&serde_json::to_vec(&manifest).unwrap());
        wasm.push(0);
        write_uleb(data.len(), &mut wasm);
        wasm.extend_from_slice(&data);
        let before = session.stats();
        let result = session.install(&[&prepared.wasm, &wasm], prepared.environment, &prepared.cells);
        let error = result.expect_err("same package version with incompatible compiler bytes must be rejected");
        assert!(error.to_string().contains("compiler build identity"), "{error}");
        assert_eq!(session.stats(), before);
        assert!(matches!(session.eval("ghost"), Err(SessionError::Compile(_))));
        let keep = session.eval("keep").unwrap();
        let bits = session.inspect(&keep, |mut store, value| {
            let number = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = number.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else { panic!("Expected ordinary number") };
            Ok(*bits)
        }).unwrap();
        assert_eq!(bits, 17.0f64.to_bits());
    }

    #[test]
    fn session_lifecycle_phase_identity_rejects_entire_batch_before_publication() {
        let mut session = Session::new().unwrap();
        session.eval("(def keep 17)").unwrap();
        let prepared = portable::prepare_fragment("(def ghost 7)", &session.environment, Phase::Runtime).unwrap();
        let wrong_phase = portable::artifact_identity::annotate_source(
            &prepared.wasm, Phase::Macro,
            Some(&portable::SourceOrigin::new("(def ghost 7)", None)), Some(&[]),
        ).unwrap();
        // These are valid executable bytes with a valid body digest, but belong
        // to the other isolated phase. Even the preceding valid fragment must
        // not allocate or publish its staged bindings on this failure.
        portable::artifact_identity::verify(&wrong_phase, Default::default()).unwrap();
        let before = session.stats();
        let result = session.install(&[&prepared.wasm, &wrong_phase], prepared.environment, &prepared.cells);
        assert!(result.unwrap_err().to_string().contains("phase identity mismatch"));
        assert_eq!(session.stats(), before);
        assert!(matches!(session.eval("ghost"), Err(SessionError::Compile(_))));
        let keep = session.eval("keep").unwrap();
        assert_eq!(crate::portable_repl::display(&mut session, &keep).unwrap(), "17");
        // A failed batch must leave the ordinary loader usable.
        let recovered = session.eval("(def ghost 23)").unwrap();
        assert_eq!(crate::portable_repl::display(&mut session, &recovered).unwrap(), "23");
    }

}
