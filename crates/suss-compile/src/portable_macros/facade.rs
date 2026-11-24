//! An owned Macro Store can be lent to one preparation at a time. The lease
//! owns the host, so no guard borrows itself and no Session pointer is captured.
use super::*;
use std::sync::Condvar;

struct SharedHost {
    available: Mutex<Option<MacroHost<'static>>>,
    returned: Condvar,
}
#[derive(Clone)]
pub(crate) struct MacroService(Arc<SharedHost>);
pub(crate) struct MacroLease {
    shared: Arc<SharedHost>,
    host: Option<MacroHost<'static>>,
}
impl MacroService {
    fn new(host: MacroHost<'static>) -> Self {
        Self(Arc::new(SharedHost {
            available: Mutex::new(Some(host)), returned: Condvar::new(),
        }))
    }
    pub(crate) fn lease(&self) -> MacroLease {
        let mut available = self.0.available.lock().unwrap_or_else(|error| error.into_inner());
        while available.is_none() {
            available = self.0.returned.wait(available).unwrap_or_else(|error| error.into_inner());
        }
        MacroLease { shared: self.0.clone(), host: available.take() }
    }
    fn with_host<T>(&self, operation: impl FnOnce(&mut MacroHost<'static>) -> T) -> T {
        operation(self.lease().host())
    }
}
impl MacroLease {
    pub(crate) fn host(&mut self) -> &mut MacroHost<'static> {
        self.host.as_mut().expect("live Macro lease")
    }
}
impl Drop for MacroLease {
    fn drop(&mut self) {
        let host = self.host.take().expect("return Macro lease once");
        let mut available = self.shared.available.lock().unwrap_or_else(|error| error.into_inner());
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
        Self { service: MacroService::new(host), active: None, requests: 0, interrupt }
    }
    pub(crate) fn service(&self) -> MacroService { self.service.clone() }
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
    pub fn current_namespace(&self) -> String { self.read_host(MacroHost::current_namespace) }
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
        callback: impl for<'a> Fn(wasmtime::StoreContextMut<'a, ()>, wasmtime::Val) -> wasmtime::Result<u32>
            + Send + Sync + 'static,
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
    pub(crate) fn restore_bindings(&mut self, checkpoint: MacroCheckpoint,
        globals: &[crate::portable::resolve::Global]) -> Result<(), SessionError> {
        self.with_host_mut(|host| host.restore_bindings(checkpoint, globals))
    }
    pub(crate) fn define_form_with_origin(&mut self, form: Form, span: std::ops::Range<usize>,
        origin: Option<&crate::portable::SourceOrigin>) -> Result<SessionValue, SessionError> {
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
            for _ in 0..self.requests { lease.host().end_expansion_request(); }
        }
    }
}
impl ExpansionHost for CompiledMacros {
    fn begin_expansion_request(&mut self, span: std::ops::Range<usize>) -> Result<(), Diagnostic> {
        if self.active.is_none() { self.active = Some(self.service.lease()); }
        let result = self.active.as_mut().unwrap().host().begin_expansion_request(span);
        if result.is_ok() { self.requests += 1; }
        else if self.requests == 0 { self.active = None; }
        result
    }
    fn end_expansion_request(&mut self) {
        assert!(self.requests > 0, "balanced Macro request");
        self.active.as_mut().unwrap().host().end_expansion_request();
        self.requests -= 1;
        if self.requests == 0 { self.active = None; }
    }
    fn expansion_work_fuel(&self) -> u64 { self.read_host(MacroHost::expansion_work_fuel) }
    fn charge_expansion_work(&mut self, work: u64, span: std::ops::Range<usize>) -> Result<(), Diagnostic> {
        self.with_host_mut(|host| host.charge_expansion_work(work, span))
    }
    fn begin_declarations(&mut self) -> Result<(), Diagnostic> {
        self.with_host_mut(MacroHost::begin_declarations)
    }
    fn end_declarations(&mut self, commit: bool) -> Result<(), Diagnostic> {
        self.with_host_mut(|host| host.end_declarations(commit))
    }
    fn begin_source_module(&mut self, forms: &[Form], origin: &crate::portable::SourceOrigin) -> Result<(), Diagnostic> {
        self.with_host_mut(|host| host.begin_source_module(forms, origin))
    }
    fn end_source_module(&mut self) -> Result<(), Diagnostic> {
        self.with_host_mut(MacroHost::end_source_module)
    }
    fn declared_macro_exports(&self, namespace: &str) -> Option<Vec<String>> {
        self.read_host(|host| host.declared_macro_exports(namespace))
    }
    fn artifact_dependencies(&self) -> Option<Vec<(String, String)>> {
        self.read_host(MacroHost::artifact_dependencies)
    }
    fn emit_fragment(&mut self, function: &crate::portable::ir::Function,
        phase: crate::portable::resolve::Phase, forms: &[Form],
        origin: Option<&crate::portable::SourceOrigin>) -> Result<Vec<u8>, Diagnostic> {
        self.with_host_mut(|host| host.emit_fragment(function, phase, forms, origin))
    }
    fn supports_macro_imports(&self) -> bool { true }
    fn source_paths(&mut self, paths: &[std::path::PathBuf]) {
        self.with_host_mut(|host| host.source_paths(paths));
    }
    fn load_macro_namespace(&mut self, namespace: &str, span: std::ops::Range<usize>) -> Result<Vec<String>, Diagnostic> {
        self.with_host_mut(|host| host.load_macro_namespace(namespace, span))
    }
    fn load_macro_namespace_with_policy(&mut self, namespace: &str, policy: crate::portable::MacroReload,
        span: std::ops::Range<usize>) -> Result<Vec<String>, Diagnostic> {
        self.with_host_mut(|host| host.load_macro_namespace_with_policy(namespace, policy, span))
    }
    fn expand(&mut self, form: &Form, context: ExpansionContext<'_>) -> Result<Option<Form>, Diagnostic> {
        self.with_host_mut(|host| host.expand(form, context))
    }
}

#[cfg(all(test, not(suss_generator)))]
mod tests {
    use super::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use wasmtime::AsContextMut;

    #[test]
    fn macro_service_preserves_state_returns_before_runtime_and_survives_unwind() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<MacroService>();
        let mut macros = CompiledMacros::new().unwrap();
        let service = macros.service();
        service.with_host(|host| {
            host.session.eval("(def macro-service-effects (atom 0))").unwrap();
        });
        macros.define(concat!(
            "(defmacro persisted [x] ",
            "(do (swap! macro-service-effects inc) (list '+ x 1)))",
        )).unwrap();
        let panic = catch_unwind(AssertUnwindSafe(|| {
            let _request = crate::portable::ExpansionRequest::new(&mut macros, 0..0).unwrap();
            panic!("request unwind fixture");
        }));
        assert!(panic.is_err());
        assert!(service.0.available.lock().unwrap().is_some(), "unwind returns the Store");
        let mut runtime = Session::new_repl().unwrap();
        let bridge = FormBridge::new(&mut runtime).unwrap();
        let available_during_runtime = service.clone();
        runtime.install_observation_hook(move |_, _| {
            // Pure inspection only: observation hooks still never re-enter.
            assert!(available_during_runtime.0.available.lock().unwrap().is_some(),
                "finish Macro preparation before executing Runtime code");
            Ok(0)
        }).unwrap();
        let value = runtime.eval_with_macros(concat!(
            "(do (suss.harness.observation/capture nil) (persisted 41))",
        ), &mut macros).unwrap();
        assert!(matches!(bridge.read(&mut runtime, &value, 0..0).unwrap().kind, Kind::Long(42)));
        let count = |service: &MacroService| service.with_host(|host| {
            let value = host.session.eval("@macro-service-effects").unwrap();
            host.bridge.read(&mut host.session, &value, 0..0).unwrap().kind
        });
        assert_eq!(count(&service), Kind::Long(1));

        let reader = bridge.borrowed_reader(&mut runtime).unwrap();
        let profile = runtime.reentry_profile();
        let compiler = service.clone();
        let input = bridge.quote(&mut runtime,
            read_forms("(fn* [] (persisted 41))").unwrap().remove(0)).unwrap();
        let function = runtime.probe_evaluation(&input, move |mut caller, value| {
            let form = reader.read(caller.as_context_mut(), &value, 0..0)?;
            let mut view = profile.bind(caller);
            let snapshot = view.compilation_snapshot();
            let mut lease = compiler.lease();
            let prepared = snapshot.prepare_with_origin(vec![form], 0..0, lease.host(), None)
                .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
            // Recursive Runtime evaluation must be able to borrow this same
            // compiler after preparation, without holding its owned host.
            drop(lease);
            view.eval_prepared(prepared).map_err(|error| wasmtime::Error::msg(error.to_string()))
        }).unwrap();
        assert_eq!(count(&service), Kind::Long(2), "one expansion per real input");
        drop(macros);
        runtime.collect().unwrap();
        let value = runtime.invoke(&function, &[]).unwrap();
        assert!(matches!(bridge.read(&mut runtime, &value, 0..0).unwrap().kind, Kind::Long(42)));
        assert_eq!(count(&service), Kind::Long(2), "no definition or expansion replay");
    }
}
