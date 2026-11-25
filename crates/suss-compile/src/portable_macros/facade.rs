//! An owned Macro Store can be lent to one preparation at a time. The lease
//! owns the host, so no guard borrows itself and no Session pointer is captured.
use super::*;
use std::sync::Condvar;

struct SharedHost {
    available: Mutex<Option<MacroHost<'static>>>,
    returned: Condvar,
    operation: Mutex<(Option<std::thread::ThreadId>, usize)>,
    operation_returned: Condvar,
}
/// Keep a preparation's catalogs exclusive while releasing the Store before
/// invoking a caller-owned transformer. Same-thread nested calls may borrow it.
pub(crate) struct MacroOperation(Arc<SharedHost>, std::thread::ThreadId);
impl Drop for MacroOperation {
    fn drop(&mut self) {
        let mut owner = self.0.operation.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(owner.0, Some(self.1));
        owner.1 -= 1;
        if owner.1 == 0 { owner.0 = None; self.0.operation_returned.notify_all(); }
    }
}
#[derive(Clone)]
pub(crate) struct MacroService(Arc<SharedHost>);
pub(crate) struct MacroLease {
    shared: Arc<SharedHost>,
    host: Option<MacroHost<'static>>,
    saved_fuel: Option<u64>,
    _operation: MacroOperation,
}
impl MacroService {
    fn new(host: MacroHost<'static>) -> Self {
        Self(Arc::new(SharedHost {
            available: Mutex::new(Some(host)),
            returned: Condvar::new(),
            operation: Mutex::new((None, 0)),
            operation_returned: Condvar::new(),
        }))
    }
    pub(crate) fn operation(&self) -> MacroOperation {
        let current = std::thread::current().id();
        let mut owner = self.0.operation.lock().unwrap_or_else(|e| e.into_inner());
        while owner.0.is_some_and(|thread| thread != current) {
            owner = self.0.operation_returned.wait(owner).unwrap_or_else(|e| e.into_inner());
        }
        owner.0 = Some(current);
        owner.1 = owner.1.checked_add(1).expect("macro preparation depth");
        MacroOperation(self.0.clone(), current)
    }
    pub(crate) fn lease(&self) -> MacroLease {
        let operation = self.operation();
        let mut available = self
            .0
            .available
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        while available.is_none() {
            available = self
                .0
                .returned
                .wait(available)
                .unwrap_or_else(|error| error.into_inner());
        }
        MacroLease {
            shared: self.0.clone(),
            host: available.take(),
            saved_fuel: None,
            _operation: operation,
        }
    }
    pub(crate) fn preparation_step<T>(
        &self, fuel: u64,
        context: Option<crate::portable_session::EvaluationMathContext>,
        interrupt: Arc<std::sync::atomic::AtomicBool>,
        span: std::ops::Range<usize>,
        operation: impl FnOnce(&mut dyn ExpansionHost) -> Result<T, Diagnostic>,
    ) -> Result<(Result<T, Diagnostic>, u64), SessionError> {
        let mut lease = self.lease();
        let allowance = lease.limit_fuel(fuel);
        let run = |host: &mut MacroHost<'static>| -> Result<Result<T, Diagnostic>, SessionError> {
            let _budget = host.session.operation_budget_scope()?;
            let _relay = super::native_invocation::InterruptRelay::start(interrupt, host.session.interrupt_handle())?;
            Ok(operation(host))
        };
        let result = match context {
            Some(context) => lease.with_math_context(context, run),
            None => run(lease.host()),
        };
        let remaining = lease.host().session.operation_fuel_remaining()?;
        Ok((result.unwrap_or_else(|error| Err(Diagnostic { span, message: error.to_string() })),
            allowance.saturating_sub(remaining)))
    }
    pub(crate) fn contains_source_macro(&self, namespace: &str, name: &str) -> bool {
        self.with_host(|host| catalog(&host.catalog).definitions.contains_key(&(namespace.to_owned(), name.to_owned())))
    }
    pub(crate) fn preparation_read<T>(&self, operation: impl FnOnce(&dyn ExpansionHost) -> T) -> T {
        self.with_host(|host| operation(host))
    }
    pub(crate) fn preparation_cleanup<T>(&self, operation: impl FnOnce(&mut dyn ExpansionHost) -> Result<T, Diagnostic>) -> Result<T, Diagnostic> {
        // Pop Rust journals even when the operation has exhausted its fuel.
        self.with_host(|host| operation(host))
    }
    pub(crate) fn prepare_evaluation(
        &self,
        snapshot: &crate::portable_session::CompilationSnapshot,
        form: Form,
        fuel: u64,
        context: crate::portable_session::EvaluationMathContext,
        interrupt: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<
        (
            Result<PreparedEvaluation, SessionError>,
            u64,
        ),
        SessionError,
    > {
        let mut lease = self.lease();
        let allowance = lease.limit_fuel(fuel);
        lease.with_math_context(context, |host| {
        let _budget = host.session.operation_budget_scope()?;
        let _relay = super::native_invocation::InterruptRelay::start(interrupt, host.session.interrupt_handle())?;
        let result = host.prepare_native_evaluation(snapshot, form);
        let remaining = host.session.operation_fuel_remaining()?;
        Ok((result, allowance.saturating_sub(remaining)))
        })
    }
    fn with_host<T>(&self, operation: impl FnOnce(&mut MacroHost<'static>) -> T) -> T {
        operation(self.lease().host())
    }
}
impl MacroLease {
    pub(super) fn with_math_context<T>(&mut self, context: crate::portable_session::EvaluationMathContext,
        operation: impl FnOnce(&mut MacroHost<'static>) -> Result<T, SessionError>) -> Result<T, SessionError>
    {
        let _budget = self.host().session.operation_budget_scope()?;
        let bridge = self.host().bridge.clone();
        let checkpoint = self.host().session.enter_evaluation_math_context(context, &bridge)?;
        let result = operation(self.host());
        let restoration = self.host().session.leave_evaluation_math_context(&checkpoint);
        match result { Err(error) => Err(error), Ok(value) => restoration.map(|()| value) }
    }
    pub(super) fn limit_fuel(&mut self, fuel: u64) -> u64 {
        let previous = self.host().session.options().fuel_per_operation;
        self.saved_fuel = Some(previous);
        let allowance = previous.min(fuel);
        self.host().set_operation_fuel(allowance);
        allowance
    }
    pub(crate) fn host(&mut self) -> &mut MacroHost<'static> {
        self.host.as_mut().expect("live Macro lease")
    }
}
impl Drop for MacroLease {
    fn drop(&mut self) {
        let mut host = self.host.take().expect("return Macro lease once");
        if let Some(fuel) = self.saved_fuel.take() {
            host.set_operation_fuel(fuel);
        }
        let mut available = self
            .shared
            .available
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        assert!(available.is_none(), "Macro Store already returned");
        *available = Some(host);
        self.shared.returned.notify_one();
    }
}

/// Public owner of a persistent isolated Macro host. Only the native compiler
/// service receives a clone of its internal authority; public callers retain
/// the existing exclusive preparation API.
pub struct CompiledMacros {
    service: MacroService,
    active: Option<MacroLease>,
    requests: usize,
    interrupt: crate::portable_session::InterruptHandle,
}
impl CompiledMacros {
    pub fn new() -> Result<Self, SessionError> {
        MacroHost::new().map(Self::from_host)
    }
    fn from_host(host: MacroHost<'static>) -> Self {
        let interrupt = host.interrupt_handle();
        Self {
            service: MacroService::new(host),
            active: None,
            requests: 0,
            interrupt,
        }
    }
    pub(crate) fn service(&self) -> MacroService {
        self.service.clone()
    }
    fn with_host_mut<T>(&mut self, operation: impl FnOnce(&mut MacroHost<'static>) -> T) -> T {
        match &mut self.active {
            Some(lease) => operation(lease.host()),
            None => self.service.with_host(operation),
        }
    }
    fn read_host<T>(&self, operation: impl FnOnce(&MacroHost<'static>) -> T) -> T {
        match &self.active {
            Some(lease) => operation(lease.host.as_ref().expect("live Macro lease")),
            None => self.service.with_host(|host| operation(host)),
        }
    }
    pub fn current_namespace(&self) -> String {
        self.read_host(MacroHost::current_namespace)
    }
    pub fn enter_namespace(&mut self, namespace: &str) -> Result<(), SessionError> {
        self.with_host_mut(|host| host.enter_namespace(namespace))
    }
    pub fn define(&mut self, source: &str) -> Result<(), SessionError> {
        self.with_host_mut(|host| host.define(source))
    }
    pub fn set_operation_fuel(&mut self, fuel: u64) {
        self.with_host_mut(|host| host.set_operation_fuel(fuel));
    }
    pub fn interrupt_handle(&self) -> crate::portable_session::InterruptHandle {
        self.interrupt.clone()
    }
    /// The existing synchronous observation hook remains non-reentrant.
    pub fn install_observation_hook(
        &mut self,
        callback: impl for<'a> Fn(
            wasmtime::StoreContextMut<'a, ()>,
            wasmtime::Val,
        ) -> wasmtime::Result<u32>
        + Send
        + Sync
        + 'static,
    ) -> Result<(), SessionError> {
        self.with_host_mut(|host| host.install_observation_hook(callback))
    }
    pub fn artifact_cache_stats(&self) -> crate::portable::artifact_cache::CacheStats {
        self.read_host(MacroHost::artifact_cache_stats)
    }
    pub fn environment_materializations(&self) -> u64 {
        self.read_host(MacroHost::environment_materializations)
    }
    pub(crate) fn binding_checkpoint(&mut self) -> Result<MacroCheckpoint, SessionError> {
        self.with_host_mut(MacroHost::binding_checkpoint)
    }
    pub(crate) fn restore_bindings(
        &mut self,
        checkpoint: MacroCheckpoint,
        globals: &[crate::portable::resolve::Global],
    ) -> Result<(), SessionError> {
        self.with_host_mut(|host| host.restore_bindings(checkpoint, globals))
    }
    pub(crate) fn define_form_with_origin(
        &mut self,
        form: Form,
        span: std::ops::Range<usize>,
        origin: Option<&crate::portable::SourceOrigin>,
    ) -> Result<SessionValue, SessionError> {
        self.with_host_mut(|host| host.define_form_with_origin(form, span, origin))
    }
    pub(crate) fn replacement(&self) -> Result<Self, SessionError> {
        self.read_host(MacroHost::replacement).map(Self::from_host)
    }
    pub(crate) fn retire_for_reset(&mut self) -> Result<(), SessionError> {
        self.with_host_mut(MacroHost::retire_for_reset)
    }
    pub(crate) fn display_value(&mut self, value: &SessionValue) -> Result<String, SessionError> {
        self.with_host_mut(|host| host.display_value(value))
    }
}
impl Drop for CompiledMacros {
    fn drop(&mut self) {
        // ExpansionRequest normally balances these, including unwinding. Also
        // retire an explicitly abandoned owner request before returning its host.
        if let Some(lease) = &mut self.active {
            for _ in 0..self.requests {
                lease.host().end_expansion_request();
            }
        }
    }
}
impl ExpansionHost for CompiledMacros {
    fn persistent_macro_context(&self) -> Option<&(dyn std::any::Any + Send + Sync)> {
        Some(&self.service)
    }

    fn begin_expansion_request(&mut self, span: std::ops::Range<usize>) -> Result<(), Diagnostic> {
        if self.active.is_none() {
            self.active = Some(self.service.lease());
        }
        let result = self
            .active
            .as_mut()
            .unwrap()
            .host()
            .begin_expansion_request(span);
        if result.is_ok() {
            self.requests += 1;
        } else if self.requests == 0 {
            self.active = None;
        }
        result
    }
    fn end_expansion_request(&mut self) {
        assert!(self.requests > 0, "balanced Macro request");
        self.active.as_mut().unwrap().host().end_expansion_request();
        self.requests -= 1;
        if self.requests == 0 {
            self.active = None;
        }
    }
    fn expansion_work_fuel(&self) -> u64 {
        self.read_host(MacroHost::expansion_work_fuel)
    }
    fn charge_expansion_work(
        &mut self,
        work: u64,
        span: std::ops::Range<usize>,
    ) -> Result<(), Diagnostic> {
        self.with_host_mut(|host| host.charge_expansion_work(work, span))
    }
    fn begin_declarations(&mut self) -> Result<(), Diagnostic> {
        self.with_host_mut(MacroHost::begin_declarations)
    }
    fn end_declarations(&mut self, commit: bool) -> Result<(), Diagnostic> {
        self.with_host_mut(|host| host.end_declarations(commit))
    }
    fn begin_source_module(
        &mut self,
        forms: &[Form],
        origin: &crate::portable::SourceOrigin,
    ) -> Result<(), Diagnostic> {
        self.with_host_mut(|host| host.begin_source_module(forms, origin))
    }
    fn end_source_module(&mut self) -> Result<(), Diagnostic> {
        self.with_host_mut(MacroHost::end_source_module)
    }
    fn declared_macro_exports(&self, namespace: &str) -> Option<Vec<String>> {
        self.read_host(|host| host.declared_macro_exports(namespace))
    }
    fn macro_binding_owner(&self, namespace: &str, name: &str) -> Option<u64> {
        self.read_host(|host| host.macro_binding_owner(namespace, name))
    }
    fn artifact_dependencies(&self) -> Option<Vec<(String, String)>> {
        self.read_host(MacroHost::artifact_dependencies)
    }
    fn emit_fragment(
        &mut self,
        function: &crate::portable::ir::Function,
        phase: crate::portable::resolve::Phase,
        forms: &[Form],
        origin: Option<&crate::portable::SourceOrigin>,
    ) -> Result<Vec<u8>, Diagnostic> {
        self.with_host_mut(|host| host.emit_fragment(function, phase, forms, origin))
    }
    fn supports_macro_imports(&self) -> bool {
        true
    }
    fn source_paths(&mut self, paths: &[std::path::PathBuf]) {
        self.with_host_mut(|host| host.source_paths(paths));
    }
    fn load_macro_namespace(
        &mut self,
        namespace: &str,
        span: std::ops::Range<usize>,
    ) -> Result<Vec<String>, Diagnostic> {
        self.with_host_mut(|host| host.load_macro_namespace(namespace, span))
    }
    fn load_macro_namespace_with_policy(
        &mut self,
        namespace: &str,
        policy: crate::portable::MacroReload,
        span: std::ops::Range<usize>,
    ) -> Result<Vec<String>, Diagnostic> {
        self.with_host_mut(|host| host.load_macro_namespace_with_policy(namespace, policy, span))
    }
    fn expand(
        &mut self,
        form: &Form,
        context: ExpansionContext<'_>,
    ) -> Result<Option<Form>, Diagnostic> {
        self.with_host_mut(|host| host.expand(form, context))
    }
}

#[cfg(all(test, not(suss_generator)))]
mod tests {
    use super::*;

    #[test]
    fn source_link_releases_macro_operation_before_runtime_waits_for_compilation() {
        use std::{sync::mpsc, time::Duration};
        let mut runtime = Session::new_repl().unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        runtime.eval_with_macros("(eval '(+ 0 0))", &mut macros).unwrap();
        let service = macros.service();
        let (request, requests) = mpsc::sync_channel(1);
        let (reply, replies) = mpsc::sync_channel(1);
        let replies = Mutex::new(replies);
        let worker = std::thread::spawn(move || {
            requests.recv_timeout(Duration::from_secs(5)).unwrap();
            let result = service.with_host(|host| {
                host.define("(defmacro compiled-during-runtime [] 42)")
                    .map_err(|error| error.to_string())
            });
            reply.send(result).unwrap();
        });
        runtime.install_observation_hook(move |_, _| {
            // The worker uses the independent Macro Store. It never reenters
            // this callback's Runtime Store or receives its Caller/context.
            request.send(()).map_err(|error| wasmtime::Error::msg(error.to_string()))?;
            replies.lock().unwrap().recv_timeout(Duration::from_secs(5))
                .map_err(|error| wasmtime::Error::msg(format!("Runtime blocked its compiler: {error}")))?
                .map_err(wasmtime::Error::msg)?;
            Ok(0)
        }).unwrap();
        let result = crate::portable_repl::evaluate_script_compiled(
            &mut runtime, &mut macros,
            "(defmacro staged-before-runtime [] 17) (do (suss.harness.observation/capture 77) 42)",
            None,
        );
        worker.join().unwrap();
        assert_eq!(result.unwrap(), "42");
        assert_eq!(crate::portable_repl::evaluate_compiled(
            &mut runtime, &mut macros, "(compiled-during-runtime)",
        ).unwrap(), "42");
    }
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use wasmtime::AsContextMut;

    #[test]
    fn caller_transformer_operation_allows_nested_leases_and_serializes_other_threads() {
        let macros = CompiledMacros::new().unwrap();
        let service = macros.service();
        let operation = service.operation();
        service.with_host(|_| ());
        let other = service.clone();
        let (started, starting) = std::sync::mpsc::channel();
        let (finished, finishing) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            started.send(()).unwrap();
            other.with_host(|_| finished.send(()).unwrap());
        });
        starting.recv().unwrap();
        assert!(matches!(finishing.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)));
        drop(operation);
        finishing.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        worker.join().unwrap();
        service.with_host(|_| ());
    }

    #[test]
    fn macro_service_preserves_state_returns_before_runtime_and_survives_unwind() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<MacroService>();
        let mut macros = CompiledMacros::new().unwrap();
        let service = macros.service();
        service.with_host(|host| {
            host.session
                .eval("(def macro-service-effects (atom 0))")
                .unwrap();
        });
        macros
            .define(concat!(
                "(defmacro persisted [x] ",
                "(do (swap! macro-service-effects inc) (list '+ x 1)))",
            ))
            .unwrap();
        let panic = catch_unwind(AssertUnwindSafe(|| {
            let _request = crate::portable::ExpansionRequest::new(&mut macros, 0..0).unwrap();
            panic!("request unwind fixture");
        }));
        assert!(panic.is_err());
        assert!(
            service.0.available.lock().unwrap().is_some(),
            "unwind returns the Store"
        );
        let mut runtime = Session::new_repl().unwrap();
        let bridge = FormBridge::new(&mut runtime).unwrap();
        let available_during_runtime = service.clone();
        runtime
            .install_observation_hook(move |_, _| {
                // Pure inspection only: observation hooks still never re-enter.
                assert!(
                    available_during_runtime
                        .0
                        .available
                        .lock()
                        .unwrap()
                        .is_some(),
                    "finish Macro preparation before executing Runtime code"
                );
                Ok(0)
            })
            .unwrap();
        let value = runtime
            .eval_with_macros(
                concat!("(do (suss.harness.observation/capture nil) (persisted 41))",),
                &mut macros,
            )
            .unwrap();
        assert!(matches!(
            bridge.read(&mut runtime, &value, 0..0).unwrap().kind,
            Kind::Long(42)
        ));
        let count = |service: &MacroService| {
            service.with_host(|host| {
                let value = host.session.eval("@macro-service-effects").unwrap();
                host.bridge
                    .read(&mut host.session, &value, 0..0)
                    .unwrap()
                    .into_parts()
                    .2
            })
        };
        assert_eq!(count(&service), Kind::Long(1));

        let reader = bridge.borrowed_reader(&mut runtime).unwrap();
        let profile = runtime.reentry_profile();
        let compiler = service.clone();
        let input = bridge
            .quote(
                &mut runtime,
                read_forms("(fn* [] (persisted 41))").unwrap().remove(0),
            )
            .unwrap();
        let function = runtime
            .probe_evaluation(&input, move |mut caller, value| {
                let form = reader.read(caller.as_context_mut(), &value, 0..0)?;
                let mut view = profile.bind(caller);
                let snapshot = view.compilation_snapshot();
                let mut lease = compiler.lease();
                let prepared = snapshot
                    .prepare_with_origin(vec![form], 0..0, lease.host(), None)
                    .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
                // Recursive Runtime evaluation must be able to borrow this same
                // compiler after preparation, without holding its owned host.
                drop(lease);
                view.eval_prepared(prepared)
                    .map_err(|error| wasmtime::Error::msg(error.to_string()))
            })
            .unwrap();
        assert_eq!(
            count(&service),
            Kind::Long(2),
            "one expansion per real input"
        );
        drop(macros);
        runtime.collect().unwrap();
        let value = runtime.invoke(&function, &[]).unwrap();
        assert!(matches!(
            bridge.read(&mut runtime, &value, 0..0).unwrap().kind,
            Kind::Long(42)
        ));
        assert_eq!(
            count(&service),
            Kind::Long(2),
            "no definition or expansion replay"
        );
    }
}
