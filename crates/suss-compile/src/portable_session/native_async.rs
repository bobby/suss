//! Native-profile source imports enqueue owned requests, never execute I/O or
//! source callbacks. Each Store installation owns fresh ABI2 closure handles.
use super::*;
use std::collections::VecDeque;
use wasmtime::{Caller, HeapType};

const CAPACITY: usize = 64;
const MAX_ARITY: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NativeRequestId {
    pub session_identity: u64,
    pub generation: u64,
}
struct Cancellation {
    requested: AtomicBool,
    hook: Mutex<Option<Box<dyn FnOnce() + Send + 'static>>>,
    attached: AtomicBool,
}
impl Cancellation {
    fn cancel(&self) {
        self.requested.store(true, Ordering::Release);
        let hook = registry_lock(&self.hook).take();
        if let Some(hook) = hook {
            hook();
        }
    }
}
/// Owned arguments and completion storage. Completion must use the originating
/// Session's resolve_future/reject_future; old identities are rejected on reset.
pub struct NativeRequest {
    pub id: NativeRequestId,
    pub args: Vec<SessionValue>,
    pub future: SessionValue,
    /// Cancellation observed when drained. Use is_cancelled for later changes.
    pub cancelled: bool,
    cancellation: Arc<Cancellation>,
}
impl NativeRequest {
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.requested.load(Ordering::Acquire)
    }
    /// Attach nonblocking native teardown after the controller starts an I/O
    /// operation. An already-cancelled request invokes it immediately, once.
    pub fn set_cancel_hook(
        &self,
        hook: impl FnOnce() + Send + 'static,
    ) -> Result<(), SessionError> {
        if self.cancellation.attached.swap(true, Ordering::AcqRel) {
            return Err(
                wasmtime::Error::msg("Native request already has a cancellation hook").into(),
            );
        }
        let mut slot = registry_lock(&self.cancellation.hook);
        if slot.is_some() {
            return Err(
                wasmtime::Error::msg("Native request already has a cancellation hook").into(),
            );
        }
        if self.is_cancelled() {
            drop(slot);
            hook();
        } else {
            *slot = Some(Box::new(hook));
        }
        Ok(())
    }
}
struct QueueState {
    next: u64,
    requests: VecDeque<NativeRequest>,
}
/// A bounded queue shared across reinstalls. Draining performs no Wasm or I/O.
#[derive(Clone)]
pub struct NativeRequestQueue(Arc<Mutex<QueueState>>);
impl NativeRequestQueue {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(QueueState {
            next: 1,
            requests: VecDeque::new(),
        })))
    }
    pub fn drain(&self) -> Vec<NativeRequest> {
        let mut state = registry_lock(&self.0);
        state
            .requests
            .drain(..)
            .map(|mut request| {
                request.cancelled = request.is_cancelled();
                request
            })
            .collect()
    }
    pub fn pending(&self) -> usize {
        registry_lock(&self.0).requests.len()
    }
    fn discard(&self, identity: u64) {
        let mut state = registry_lock(&self.0);
        state.requests.retain(|request| {
            if request.id.session_identity == identity {
                request
                    .cancellation
                    .requested
                    .store(true, Ordering::Release);
                false
            } else {
                true
            }
        });
    }
}
#[derive(Clone)]
pub(super) struct FactoryProfile {
    symbol: suss_reader::Symbol,
    arity: usize,
    queue: NativeRequestQueue,
}
fn host_value(
    caller: &mut Caller<'_, ()>,
    value: Val,
    identity: u64,
    handles: &Arc<AtomicUsize>,
) -> wasmtime::Result<SessionValue> {
    let reference = value
        .anyref()
        .flatten()
        .ok_or_else(|| wasmtime::Error::msg("Expected ABI2 Value"))?;
    let value = reference.to_owned_rooted(&mut *caller)?;
    handles.fetch_add(1, Ordering::Relaxed);
    Ok(SessionValue {
        identity,
        value,
        handles: handles.clone(),
    })
}

impl Session {
    /// Install an explicit Runtime-only native-profile function with fixed arity.
    /// Filesystem source must be absent; errors and ambiguity never select native
    /// fallback. The returned controller survives reset with fresh Store roots.
    pub fn install_native_future_factory(
        &mut self,
        symbol: suss_reader::Symbol,
        arity: usize,
    ) -> Result<NativeRequestQueue, SessionError> {
        let profile = FactoryProfile {
            symbol,
            arity,
            queue: NativeRequestQueue::new(),
        };
        let queue = profile.queue.clone();
        self.install_native_profile(profile)?;
        Ok(queue)
    }
    pub(super) fn install_native_profile(
        &mut self,
        profile: FactoryProfile,
    ) -> Result<(), SessionError> {
        if self.phase != Phase::Runtime {
            return Err(
                wasmtime::Error::msg("Native future factories require Runtime phase").into(),
            );
        }
        if profile.arity > MAX_ARITY {
            return Err(
                wasmtime::Error::msg("Native factory arity exceeds bounded profile").into(),
            );
        }
        let namespace =
            profile.symbol.namespace.as_deref().ok_or_else(|| {
                wasmtime::Error::msg("Native factory requires a qualified symbol")
            })?;
        if portable::resolve::locate_source_if_present(namespace, &self.options.source_paths, 0..0)
            .map_err(SessionError::Compile)?
            .is_some()
        {
            return Err(
                wasmtime::Error::msg("Native profile cannot replace filesystem source").into(),
            );
        }
        let own_namespace = self
            .native_factories
            .iter()
            .any(|p| p.symbol.namespace.as_deref() == Some(namespace));
        if !own_namespace
            && (self.environment.has_namespace(self.phase, namespace) || namespace == "suss.async")
        {
            return Err(wasmtime::Error::msg(
                "Native profile cannot replace an existing namespace",
            )
            .into());
        }
        if self
            .native_factories
            .iter()
            .any(|p| p.symbol == profile.symbol)
        {
            return Err(wasmtime::Error::msg("Native factory is already installed").into());
        }
        if self
            .environment
            .resolve(self.phase, &profile.symbol, 0..0)
            .is_ok()
        {
            return Err(
                wasmtime::Error::msg("Native factory target already has a source binding").into(),
            );
        }
        self.native_factories
            .try_reserve(1)
            .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
        self.budget()?;
        let mut environment = self.environment.clone();
        let cell_identity = environment
            .declare_cell(self.phase, namespace, &profile.symbol.name)
            .map_err(SessionError::Compile)?;
        let provided = ModuleIdentity::new(self.phase, namespace).map_err(SessionError::Compile)?;
        let closure_new = self
            .runtime
            .get_func(&mut self.store, "closure-new")
            .ok_or_else(|| wasmtime::Error::msg("Missing ABI2 closure constructor"))?;
        let parameter = closure_new.ty(&self.store).params().nth(1).unwrap();
        let HeapType::ConcreteFunc(invoke_type) = parameter.unwrap_ref().heap_type() else {
            return Err(wasmtime::Error::msg("Unexpected ABI2 invoke type").into());
        };
        let pending = self
            .runtime
            .get_func(&mut self.store, "future-pending-new")
            .unwrap();
        let registry = self.pending_host_futures.clone();
        let queue = profile.queue.clone();
        let identity = self.identity;
        let handles = self.handles.clone();
        let arity = profile.arity;
        let factory = Func::try_new(
            &mut self.store,
            invoke_type.clone(),
            move |mut caller, params, results| {
                let array = params[1]
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&caller)?
                    .ok_or_else(|| {
                        wasmtime::Error::msg("Native factory requires ABI2 argument array")
                    })?;
                if array.len(&caller)? as usize != arity {
                    return Err(wasmtime::Error::msg("Native factory arity mismatch"));
                }
                // Reserve both publication containers before any Wasm call. Store
                // execution serializes producers; controllers only remove requests.
                {
                    let mut entries = registry_lock(&registry);
                    if entries.len() >= CAPACITY {
                        return Err(wasmtime::Error::msg("Native pending registry is full"));
                    }
                    entries
                        .try_reserve(1)
                        .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
                }
                let generation = {
                    let mut state = registry_lock(&queue.0);
                    if state.requests.len() >= CAPACITY {
                        return Err(wasmtime::Error::msg("Native request queue is full"));
                    }
                    state
                        .requests
                        .try_reserve(1)
                        .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
                    let generation = state.next;
                    state.next = generation.checked_add(1).ok_or_else(|| {
                        wasmtime::Error::msg("Native request generation exhausted")
                    })?;
                    generation
                };
                let mut args = Vec::new();
                args.try_reserve(arity)
                    .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
                for index in 0..arity {
                    let value = array.get(&mut caller, index as u32)?;
                    args.push(host_value(&mut caller, value, identity, &handles)?);
                }
                let mut result = [Val::null_any_ref()];
                pending.call(&mut caller, &[], &mut result)?;
                let future = host_value(&mut caller, result[0].clone(), identity, &handles)?;
                let cancellation = Arc::new(Cancellation {
                    requested: AtomicBool::new(false),
                    hook: Mutex::new(None),
                    attached: AtomicBool::new(false),
                });
                let signal = cancellation.clone();
                let entry = Arc::new(PendingHostFuture {
                    root: future.value.clone(),
                    cancel: Mutex::new(Some(Box::new(move || signal.cancel()))),
                });
                let request = NativeRequest {
                    id: NativeRequestId {
                        session_identity: identity,
                        generation,
                    },
                    args,
                    future,
                    cancelled: false,
                    cancellation,
                };
                // No Wasm, I/O, source callback or fallible action between these
                // native publications and returning the already-rooted GC value.
                registry_lock(&registry).push(entry);
                registry_lock(&queue.0).requests.push_back(request);
                results[0] = result[0].clone();
                Ok(())
            },
        )?;
        let mut scope = RootScope::new(&mut self.store);
        let nil = self.runtime.get_func(&mut scope, "nil").unwrap();
        let mut env = [Val::null_any_ref()];
        nil.call(&mut scope, &[], &mut env)?;
        let mut closure = [Val::null_any_ref()];
        closure_new.call(
            &mut scope,
            &[
                env[0].clone(),
                Val::FuncRef(Some(factory)),
                Val::I32(arity as i32),
                Val::I32(arity as i32),
            ],
            &mut closure,
        )?;
        // Initialize a private staged cell. No catalog, linker or profile
        // publication occurs until every fueled Wasm action has succeeded.
        let closure = closure[0].clone();
        let mut cell = [Val::null_any_ref()];
        self.runtime
            .get_func(&mut scope, "binding-unbound")
            .unwrap()
            .call(&mut scope, &[], &mut cell)?;
        let cell_type = cell[0]
            .unwrap_anyref()
            .unwrap()
            .as_struct(&scope)?
            .unwrap()
            .ty(&scope)?;
        let global = Global::new(
            &mut scope,
            GlobalType::new(
                ValType::Ref(RefType::new(false, cell_type.into())),
                Mutability::Const,
            ),
            cell[0].clone(),
        )?;
        let mut linker = self.linker.clone();
        linker.define(
            &scope,
            cell_identity.import_module(),
            &cell_identity.import_name(),
            global,
        )?;
        self.runtime
            .get_func(&mut scope, "binding-set")
            .unwrap()
            .call(&mut scope, &[cell[0].clone(), closure], &mut [])?;
        drop(scope);
        self.linker = linker;
        self.cells.insert(cell_identity, global);
        self.environment = environment;
        self.provided.insert(provided);
        self.native_factories.push(profile);
        Ok(())
    }
    /// Cancel producerless host requests only once source tasks and their
    /// cleanup have fully retired. No source callback is invoked by this sweep.
    pub fn request_cancel_pending_host_requests(&mut self) -> Result<(), SessionError> {
        if self.async_task_counts()? != (0, 0, 0)
            || self.pending_stream_operation_count()? != 0
        {
            return Err(SessionError::ResetPending);
        }
        self.budget()?;
        self.cancel_host_requests()
    }
    /// Construct the genuine runtime ExceptionInfo without consulting mutable
    /// source constructor/ex-info bindings. The core-ex-info export constructs
    /// a source callable, so use the pinned descriptor and runtime constructor.
    pub fn native_exception_info(
        &mut self,
        message: &SessionValue,
        data: &SessionValue,
    ) -> Result<SessionValue, SessionError> {
        self.check(message)?;
        self.check(data)?;
        self.budget()?;
        let runtime = self.runtime;
        let mut scope = RootScope::new(&mut self.store);
        let class = runtime
            .get_func(&mut scope, "core-exception-info-class")
            .unwrap();
        let class = call(
            &mut scope,
            runtime,
            class,
            &[],
            self.identity,
            &self.handles,
        )?;
        let class_value = Val::AnyRef(Some(class.value.to_rooted(&mut scope)));
        let descriptor = runtime
            .get_func(&mut scope, "constructor-descriptor")
            .unwrap();
        let descriptor = call(
            &mut scope,
            runtime,
            descriptor,
            &[class_value],
            self.identity,
            &self.handles,
        )?;
        let buffer = runtime.get_func(&mut scope, "args-new").unwrap();
        let buffer = call(
            &mut scope,
            runtime,
            buffer,
            &[Val::I32(3)],
            self.identity,
            &self.handles,
        )?;
        let array = buffer
            .value
            .to_rooted(&mut scope)
            .as_array(&scope)?
            .unwrap();
        let message = Val::AnyRef(Some(message.value.to_rooted(&mut scope)));
        let data = Val::AnyRef(Some(data.value.to_rooted(&mut scope)));
        array.set(&mut scope, 0, message)?;
        array.set(&mut scope, 1, data)?;
        let nil = runtime.get_func(&mut scope, "nil").unwrap();
        let nil = call(&mut scope, runtime, nil, &[], self.identity, &self.handles)?;
        let nil = Val::AnyRef(Some(nil.value.to_rooted(&mut scope)));
        array.set(&mut scope, 2, nil)?;
        let descriptor = Val::AnyRef(Some(descriptor.value.to_rooted(&mut scope)));
        let buffer = Val::AnyRef(Some(buffer.value.to_rooted(&mut scope)));
        let constructor = runtime
            .get_func(&mut scope, "source-constructor-new")
            .unwrap();
        let constructor = call(
            &mut scope,
            runtime,
            constructor,
            &[descriptor],
            self.identity,
            &self.handles,
        )?;
        let constructor = Val::AnyRef(Some(constructor.value.to_rooted(&mut scope)));
        let invoke = runtime.get_func(&mut scope, "invoke").unwrap();
        call(
            &mut scope,
            runtime,
            invoke,
            &[constructor, buffer],
            self.identity,
            &self.handles,
        )
    }
    pub(super) fn sweep_native_requests(&self) {
        for profile in &self.native_factories {
            registry_lock(&profile.queue.0)
                .requests
                .retain(|request| !request.is_cancelled());
        }
    }
    pub(super) fn discard_native_requests(&self) {
        for profile in &self.native_factories {
            profile.queue.discard(self.identity);
        }
    }
}

// Cancel native producers and release registry/queued roots before Store field
// destruction. This performs no Wasm or source cleanup: reset uses retirement.
impl Drop for Session {
    fn drop(&mut self) {
        let requests = registry_lock(&self.pending_host_futures).clone();
        for request in requests {
            if let Some(hook) = request.take_cancel() {
                hook();
            }
        }
        self.discard_native_requests();
        registry_lock(&self.pending_host_futures).clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_factory_installation_fuel_failures_leave_catalog_unchanged_and_retryable() {
        let mut failures = 0;
        let mut successes = 0;
        for fuel in (0..240).step_by(8).chain(std::iter::once(10_000)) {
            let mut session = Session::new().unwrap();
            let before = session.stats().binding_cells;
            session.options.fuel_per_operation = fuel;
            let symbol = suss_reader::Symbol::namespaced("native.factory", "read");
            let installed = session.install_native_future_factory(symbol.clone(), 0);
            session.options.fuel_per_operation = 1_000_000;
            let queue =
                match installed {
                    Ok(queue) => {
                        successes += 1;
                        queue
                    }
                    Err(_) => {
                        failures += 1;
                        assert_eq!(session.stats().binding_cells, before, "fuel={fuel}");
                        assert!(
                            !session
                                .environment
                                .has_namespace(Phase::Runtime, "native.factory")
                        );
                        assert!(session.native_factories.is_empty());
                        assert!(!session.provided.contains(
                            &ModuleIdentity::new(Phase::Runtime, "native.factory").unwrap()
                        ));
                        session.collect().unwrap();
                        session.install_native_future_factory(symbol, 0).unwrap()
                    }
                };
            session.eval("(native.factory/read)").unwrap();
            assert_eq!(queue.pending(), 1);
            session.reset().unwrap();
            assert_eq!(queue.pending(), 0);
        }
        assert!(failures > 0 && successes > 0);
    }
}

pub(super) fn recovery_candidates(
    scope: &mut RootScope<&mut Store<()>>,
    runtime: Instance,
) -> wasmtime::Result<Vec<OwnedRooted<AnyRef>>> {
    let mut result = [Val::null_any_ref()];
    runtime
        .get_func(&mut *scope, "async-scheduler-owner-snapshot")
        .unwrap()
        .call(&mut *scope, &[], &mut result)?;
    let array = result[0]
        .unwrap_anyref()
        .unwrap()
        .as_array(&*scope)?
        .unwrap();
    let mut candidates = Vec::new();
    for index in 0..array.len(&*scope)? {
        let value = array.get(&mut *scope, index)?;
        candidates.push(value.unwrap_anyref().unwrap().to_owned_rooted(&mut *scope)?);
    }
    Ok(candidates)
}
pub(super) fn recovered_owner(
    scope: &mut RootScope<&mut Store<()>>,
    runtime: Instance,
    candidates: &[OwnedRooted<AnyRef>],
) -> wasmtime::Result<Option<OwnedRooted<AnyRef>>> {
    let status = runtime.get_func(&mut *scope, "future-status").unwrap();
    let result = runtime.get_func(&mut *scope, "future-result").unwrap();
    let marker = runtime
        .get_func(&mut *scope, "async-runtime-trap-is")
        .unwrap();
    let mut owner = None;
    for candidate in candidates {
        let future = Val::AnyRef(Some(candidate.to_rooted(&mut *scope)));
        let mut state = [Val::I32(0)];
        status.call(&mut *scope, &[future.clone()], &mut state)?;
        if state[0].unwrap_i32() != 2 {
            continue;
        }
        let mut payload = [Val::null_any_ref()];
        result.call(&mut *scope, &[future], &mut payload)?;
        marker.call(&mut *scope, &payload, &mut state)?;
        if state[0].unwrap_i32() != 0 {
            if owner.is_some() {
                return Err(wasmtime::Error::msg("Ambiguous async recovery owner"));
            }
            owner = Some(candidate.clone());
        }
    }
    Ok(owner)
}
impl Session {
    /// Evidence that the last turn failed an exact owner, retired its scheduler
    /// work and restored the caller without a pending exception. Cleared on entry.
    pub fn last_async_turn_recovered(&self) -> bool {
        self.last_async_recovered_owner.is_some()
    }
    pub fn last_async_turn_recovered_owner(&self) -> Option<SessionValue> {
        self.last_async_recovered_owner.as_ref().map(|owner| {
            self.handles.fetch_add(1, Ordering::Relaxed);
            SessionValue {
                identity: self.identity,
                value: owner.clone(),
                handles: self.handles.clone(),
            }
        })
    }
    /// Exact private singleton identity. The marker cannot be constructed from
    /// an ordinary nominal shape; source may still transport an existing marker.
    pub fn is_async_runtime_trap(&mut self, payload: &SessionValue) -> Result<bool, SessionError> {
        self.check(payload)?;
        self.budget()?;
        let marker = self
            .runtime
            .get_func(&mut self.store, "async-runtime-trap-is")
            .unwrap();
        self.inspect(payload, |mut store, value| {
            let mut result = [Val::I32(0)];
            marker.call(&mut store, &[value], &mut result)?;
            Ok(result[0].unwrap_i32() != 0)
        })
    }
}
