//! Persistent native execution of portable fragments and source module plans.
//! No source history is replayed. The legacy command frontend is not migrated yet.
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::PathBuf,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};
use suss_compile::{
    portable::{
        self, Diagnostic, PreparedFragment,
        modules::{ModuleDiagnostic, ModuleIdentity, PreparedModule},
        resolve::{Environment, Global as CellIdentity, Phase},
    },
    runtime_abi,
};
use wasmtime::{
    AnyRef, AsContextMut, Config, Engine, Func, Global, GlobalType, Instance, Linker, Memory,
    Module, Mutability, OwnedRooted, RefType, RootScope, Store, StoreContextMut, Tag, Val, ValType,
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
    artifact_bytes: usize,
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
fn call(
    scope: &mut RootScope<&mut Store<()>>,
    runtime: Instance,
    function: Func,
    args: &[Val],
    identity: u64,
    handles: &Arc<AtomicUsize>,
) -> Result<SessionValue, SessionError> {
    let mut result = [Val::null_any_ref()];
    function
        .call(&mut *scope, args, &mut result)
        .map_err(|error| execution_error(scope, runtime, error, identity, handles))?;
    own(scope, result[0].clone(), identity, handles)
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
    pub fn with_options(options: SessionOptions) -> Result<Self, SessionError> {
        Self::with_engine(engine()?, options)
    }
    /// Caller-supplied engines must enable ABI features and fuel consumption.
    pub fn with_engine(engine: Engine, options: SessionOptions) -> Result<Self, SessionError> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let identity = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let mut store = Store::new(&engine, ());
        store.set_fuel(u64::MAX)?;
        let runtime_bytes = runtime_abi::module();
        runtime_abi::verify_artifact(&runtime_bytes, &runtime_abi::Manifest::default())
            .map_err(|message| SessionError::Host(wasmtime::Error::msg(message)))?;
        let runtime = Instance::new(&mut store, &Module::new(&engine, runtime_bytes)?, &[])?;
        let numeric_memory = runtime
            .get_memory(&mut store, "numeric-scratch-memory")
            .ok_or_else(|| wasmtime::Error::msg("Missing numeric scratch memory"))?;
        let mut linker = Linker::new(&engine);
        linker.instance(&mut store, "suss.runtime", runtime)?;
        let provided = BTreeSet::from([
            ModuleIdentity::new(Phase::Runtime, "suss.core").map_err(SessionError::Compile)?
        ]);
        Ok(Self {
            engine,
            options,
            identity,
            handles: Arc::new(AtomicUsize::new(0)),
            store,
            runtime,
            numeric_memory,
            linker,
            environment: Environment::default(),
            cells: BTreeMap::new(),
            provided,
            resident: Vec::new(),
            artifact_bytes: 0,
        })
    }
    pub fn options(&self) -> &SessionOptions {
        &self.options
    }
    pub fn set_operation_fuel(&mut self, fuel: u64) {
        self.options.fuel_per_operation = fuel;
    }
    pub fn current_namespace(&self) -> &str {
        self.environment.current_namespace(Phase::Runtime)
    }
    pub fn enter_namespace(&mut self, namespace: &str) -> Result<(), SessionError> {
        self.environment
            .enter_namespace(Phase::Runtime, namespace)
            .map_err(SessionError::Compile)
    }
    pub fn stats(&self) -> SessionStats {
        SessionStats {
            resident_fragments: self.resident.len(),
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
        let replacement = Self::with_engine(self.engine.clone(), self.options.clone())?;
        *self = replacement;
        Ok(())
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
        let value = Val::AnyRef(Some(value.value.to_rooted(&mut scope)));
        let result = f(scope.as_context_mut(), value);
        let result = match result {
            Ok(_) if scope.as_context_mut().has_pending_exception() => Err(wasmtime::Error::msg(
                "Native callback returned success with a pending exception",
            )),
            result => result,
        };
        result.map_err(|error| {
            execution_error(
                &mut scope,
                self.runtime,
                error,
                self.identity,
                &self.handles,
            )
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
            compiled.push(Module::new(&self.engine, bytes)?);
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
            Phase::Runtime,
            &self.provided,
        )
        .map_err(|error| match error {
            portable::modules::InputDiagnostic::Compile(error) => SessionError::Compile(error),
            portable::modules::InputDiagnostic::Dependency(error) => SessionError::Module(error),
        })?;
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
        let mut plan = portable::modules::prepare_modules(
            namespace,
            &self.options.source_paths,
            &self.environment,
            Phase::Runtime,
            &self.provided,
        )
        .map_err(SessionError::Module)?;
        plan.environment
            .enter_namespace(Phase::Runtime, self.current_namespace())
            .map_err(SessionError::Compile)?;
        self.budget()?;
        let bytes: Vec<_> = plan
            .modules
            .iter()
            .map(|module| module.wasm.as_slice())
            .collect();
        let instances = self.install(&bytes, plan.environment, &plan.cells)?;
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
}
