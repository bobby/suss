//! Invoke a thread-selected native transformer in its own caller Store.
//! Rust analysis/emission stays shared; isolated transformers retain their
//! Macro service. No Store reference or GC value crosses that boundary.
use super::*;
use crate::portable::{Diagnostic, ExpansionContext, ExpansionHost, MacroReload, SourceOrigin};
use crate::portable_macros::MacroOperation;
use crate::portable_macro_graph::AnalysisGraph;
use suss_reader::forms::{Form, Kind};
use std::ops::Range;

fn diagnostic(span: Range<usize>, error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic { span, message: error.to_string() }
}
impl SessionHost<'_> {
    fn native_macro_binding_differs(&mut self, global: &CellIdentity) -> Result<bool, SessionError> {
        let selected = self.binding_value(global)?;
        let root = self.binding_root_value(global)?;
        self.identical_values(&selected, &root).map(|same| !same)
    }
    pub(super) fn has_native_macro_binding_override(&mut self, snapshot: &CompilationSnapshot) -> Result<bool, SessionError> {
        for global in snapshot.environment.native_macro_declarations(self.identity) {
            if global.phase() != self.phase || snapshot.environment.source_macro_overridden(
                self.phase, global.namespace(), global.name()) { continue; }
            if !registry_lock(&self.compiler).cells.contains_key(&global) { continue; }
            self.charge_host_work((global.namespace().len() + global.name().len()) as u64 + 1)?;
            if self.native_macro_binding_differs(&global)? { return Ok(true); }
        }
        Ok(false)
    }
}
struct CallerExpansion<'a, 'store> {
    caller: &'a mut SessionHost<'store>,
    service: MacroService,
    bridge: Arc<FormBridge>,
    _operation: MacroOperation,
    deferred: Option<Diagnostic>,
    invocation_error: Option<SessionError>,
}
impl CallerExpansion<'_, '_> {
    fn step<T>(&mut self, span: Range<usize>, context: bool,
        operation: impl FnOnce(&mut dyn ExpansionHost) -> Result<T, Diagnostic>) -> Result<T, Diagnostic>
    {
        let allowance = self.caller.operation_fuel_remaining().map_err(|e| diagnostic(span.clone(), e))?;
        let context = if context { Some(self.caller.evaluation_math_context().map_err(|e| diagnostic(span.clone(), e))?) } else { None };
        let (result, spent) = self.service.preparation_step(allowance, context,
            self.caller.interrupt.clone(), operation).map_err(|e| diagnostic(span.clone(), e))?;
        self.caller.charge_host_work(spent).map_err(|e| diagnostic(span.clone(), e))?;
        result.map_err(|error| {
            let message = error.to_string();
            self.invocation_error = Some(error);
            Diagnostic { span, message }
        })
    }
    fn invoke(&mut self, global: &CellIdentity, form: &Form, context: ExpansionContext<'_>) -> Result<Form, SessionError> {
        // Read at each invocation: a transformer may change this thread's
        // binding before another expansion in the same compilation unit.
        let function = self.caller.binding_value(global)?;
        let mut meter = |work| self.caller.charge_host_work(work).map_err(|e| diagnostic(form.span.clone(), e));
        let caller_data = match context.origin {
            Some(origin) => origin.macro_form_data_metered(form, &mut meter),
            None => crate::portable::clone_form_metered(form, &mut meter),
        }.map_err(SessionError::Compile)?;
        let owner = self.caller.native_owner();
        let caller_form = self.bridge.quote_evaluation_ref(self.caller, &caller_data, owner)?;
        let environment = AnalysisGraph::new(&self.bridge, self.caller).native_evaluation_locals(context)?;
        let mut arguments = vec![caller_form, environment];
        let Kind::List(items) = &caller_data.kind else { unreachable!("matched transformer call") };
        for argument in &items[1..] {
            arguments.push(self.bridge.quote_evaluation_ref(self.caller, argument, owner)?);
        }
        let references = arguments.iter().collect::<Vec<_>>();
        let result = self.caller.invoke(&function, &references)?;
        self.caller.read_native_evaluation(&self.bridge, &result, form.span.clone())
    }
}
impl ExpansionHost for CallerExpansion<'_, '_> {
    fn persistent_macro_context(&self) -> Option<&(dyn std::any::Any + Send + Sync)> { Some(&self.service) }
    fn expansion_work_fuel(&self) -> u64 { self.caller.options.fuel_per_operation }
    fn begin_expansion_request(&mut self, span: Range<usize>) -> Result<(), Diagnostic> {
        self.caller.begin_operation_budget().map_err(|e| diagnostic(span, e))
    }
    fn end_expansion_request(&mut self) { self.caller.end_operation_budget(); }
    fn charge_expansion_work(&mut self, work: u64, span: Range<usize>) -> Result<(), Diagnostic> {
        self.caller.charge_host_work(work).map_err(|e| diagnostic(span, e))
    }
    fn artifact_dependencies(&self) -> Option<Vec<(String, String)>> {
        self.service.preparation_read(|host| host.artifact_dependencies())
    }
    fn emit_fragment(&mut self, function: &crate::portable::ir::Function,
        phase: crate::portable::resolve::Phase, forms: &[Form], origin: Option<&SourceOrigin>) -> Result<Vec<u8>, Diagnostic> {
        self.step(function.span.clone(), false, |host| host.emit_fragment(function, phase, forms, origin))
    }
    fn supports_macro_imports(&self) -> bool { true }
    fn source_paths(&mut self, paths: &[std::path::PathBuf]) {
        if let Err(error) = self.step(0..0, false, |host| { host.source_paths(paths); Ok(()) }) { self.deferred = Some(error); }
    }
    fn begin_declarations(&mut self) -> Result<(), Diagnostic> { self.step(0..0, false, |host| host.begin_declarations()) }
    fn end_declarations(&mut self, commit: bool) -> Result<(), Diagnostic> {
        self.service.preparation_cleanup(|host| host.end_declarations(commit))
    }
    fn begin_source_module(&mut self, forms: &[Form], origin: &SourceOrigin) -> Result<(), Diagnostic> {
        self.step(0..0, false, |host| host.begin_source_module(forms, origin))
    }
    fn end_source_module(&mut self) -> Result<(), Diagnostic> {
        self.service.preparation_cleanup(|host| host.end_source_module())
    }
    fn declared_macro_exports(&self, namespace: &str) -> Option<Vec<String>> {
        self.service.preparation_read(|host| host.declared_macro_exports(namespace))
    }
    fn load_macro_namespace(&mut self, namespace: &str, span: Range<usize>) -> Result<Vec<String>, Diagnostic> {
        self.step(span.clone(), true, |host| host.load_macro_namespace(namespace, span))
    }
    fn load_macro_namespace_with_policy(&mut self, namespace: &str, policy: MacroReload, span: Range<usize>) -> Result<Vec<String>, Diagnostic> {
        self.step(span.clone(), true, |host| host.load_macro_namespace_with_policy(namespace, policy, span))
    }
    fn expand(&mut self, form: &Form, context: ExpansionContext<'_>) -> Result<Option<Form>, Diagnostic> {
        if let Kind::List(items) = &form.kind {
            if let Some(Form { kind: Kind::Symbol(symbol), .. }) = items.first() {
                if let Some((namespace, name)) = context.environment.resolve_source_macro(context.phase, symbol) {
                    if let Some(global) = context.environment.native_macro_global(context.phase, &namespace, &name, self.caller.native_owner()) {
                        if !context.environment.source_macro_overridden(context.phase, &namespace, &name)
                            && self.caller.native_macro_binding_differs(&global).map_err(|e| diagnostic(form.span.clone(), e))? {
                            return match self.invoke(&global, form, context) {
                                Ok(expanded) => Ok(Some(expanded)),
                                Err(error) => {
                                    let failure = diagnostic(form.span.clone(), &error);
                                    self.invocation_error = Some(error);
                                    Err(failure)
                                }
                            };
                        }
                    }
                }
            }
        }
        self.step(form.span.clone(), true, |host| host.expand(form, context))
    }
}
pub(super) fn prepare(caller: &mut SessionHost<'_>, service: &MacroService, bridge: &Arc<FormBridge>,
    snapshot: &CompilationSnapshot, form: Form) -> Result<PreparedEvaluation, SessionError> {
    let operation = service.operation();
    let mut expander = CallerExpansion { caller, service: service.clone(), bridge: bridge.clone(), _operation: operation, deferred: None, invocation_error: None };
    let result = crate::portable_macros::prepare_with_expander(snapshot, form, &mut expander);
    // Keep typed caller traps/cancellation and phase-owned language payloads;
    // the analyzer's Diagnostic cannot carry their actual Store ownership.
    expander.caller.charge_host_work(0)?;
    if let Some(error) = expander.invocation_error.take() { return Err(error); }
    match (result, expander.deferred.take()) {
        (Ok(_), Some(error)) => Err(SessionError::Compile(error)),
        (result, _) => result,
    }
}
