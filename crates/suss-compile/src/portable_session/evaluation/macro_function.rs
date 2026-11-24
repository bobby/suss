//! A caller-side closure for one actual function in the isolated Macro Store.
//! This uses the existing universal native closure ABI, with no new runtime
//! import, serialized handle, second body compilation or cross-Store GC value.
use super::*;
use crate::portable::{Diagnostic, ExpansionContext, ExpansionHost};
use suss_reader::forms::{Form, Kind};

struct DefinitionCompiler<'a, 'store>(&'a mut SessionHost<'store>);
impl ExpansionHost for DefinitionCompiler<'_, '_> {
    fn expand(
        &mut self,
        _form: &Form,
        _context: ExpansionContext<'_>,
    ) -> Result<Option<Form>, Diagnostic> {
        Ok(None)
    }
    fn expansion_work_fuel(&self) -> u64 {
        self.0.options.fuel_per_operation
    }
    fn charge_expansion_work(
        &mut self,
        work: u64,
        span: std::ops::Range<usize>,
    ) -> Result<(), Diagnostic> {
        self.0.charge_host_work(work).map_err(|error| Diagnostic {
            span,
            message: error.to_string(),
        })
    }
}

impl SessionHost<'_> {
    pub(super) fn prepare_native_macro_declaration(
        &mut self,
        service: &MacroService,
        snapshot: &CompilationSnapshot,
        form: Form,
    ) -> Result<crate::portable::modules::PreparedInput, SessionError> {
        let namespace = snapshot
            .environment
            .current_namespace(self.phase)
            .to_owned();
        let original = self.current_namespace().to_owned();
        let root = Arc::new(Mutex::new(None));
        let slot = root.clone();
        let bridge_service = service.clone();
        let span = form.span.clone();
        let origin = portable::SourceOrigin::native_evaluation(self.identity)
            .for_macro_definition(self.identity);
        let allowance = self.operation_fuel_remaining()?;
        let (prepared, spent) = service.declare_evaluation(
            form,
            &namespace,
            allowance,
            self.evaluation_math_context()?,
            self.interrupt.clone(),
            |mut template, source, names, spent| {
                self.charge_host_work(spent)?;
                // Only the publication prefix enters this compiler: the actual
                // macro body has already been analyzed once in its owning Store.
                let function = self.macro_function_proxy(bridge_service, slot)?;
                let constant = self.retain_evaluation_constant(&function)?;
                let Kind::List(parts) = &mut template.kind else {
                    return Err(wasmtime::Error::msg("Invalid macro publication prefix").into());
                };
                let Kind::Symbol(name) = &parts[1].kind else {
                    unreachable!("validated macro name")
                };
                let name = name.name.clone();
                parts.push(Form {
                    span: span.clone(),
                    metadata: vec![],
                    kind: constant,
                });
                let mut compiler = DefinitionCompiler(self);
                let mut prepared = snapshot.prepare_with_origin(
                    vec![template],
                    span,
                    &mut compiler,
                    Some(&origin),
                )?;
                prepared
                    .fragment
                    .environment
                    .declare_macro_exports(snapshot.phase(), &namespace, &names)
                    .map_err(SessionError::Compile)?;
                prepared.fragment.environment.record_native_macro_source(
                    snapshot.phase(),
                    &namespace,
                    &name,
                    source,
                );
                prepared.fragment.environment.record_macro_declaration(
                    snapshot.phase(),
                    &namespace,
                    &name,
                );
                prepared
                    .fragment
                    .environment
                    .enter_namespace(snapshot.phase(), &original)
                    .map_err(SessionError::Compile)?;
                Ok((prepared, self.operation_fuel_remaining()?))
            },
        )?;
        self.charge_host_work(spent)?;
        let (prepared, function) = prepared?;
        *registry_lock(&root) = Some(function);
        Ok(prepared)
    }

    fn macro_function_proxy(
        &mut self,
        service: MacroService,
        root: Arc<Mutex<Option<SessionValue>>>,
    ) -> Result<SessionValue, SessionError> {
        self.charge_host_work(1)?;
        let state = self
            .evaluation
            .as_ref()
            .map(Arc::downgrade)
            .ok_or_else(|| wasmtime::Error::msg("Missing native evaluation authority"))?;
        let constructor = self
            .runtime
            .get_func(&mut self.store, "closure-new")
            .ok_or_else(|| wasmtime::Error::msg("Missing native closure constructor"))?;
        let parameter = constructor.ty(&self.store).params().nth(1).unwrap();
        let wasmtime::HeapType::ConcreteFunc(signature) = parameter.unwrap_ref().heap_type() else {
            return Err(wasmtime::Error::msg("Invalid native closure invocation type").into());
        };
        let callback = Func::try_new(
            &mut self.store,
            signature.clone(),
            move |mut caller, arguments, results| {
                let state = state
                    .upgrade()
                    .ok_or_else(|| wasmtime::Error::msg("Native compiler owner has retired"))?;
                let profile = registry_lock(&state.profile)
                    .clone()
                    .ok_or_else(|| wasmtime::Error::msg("No active native compiler profile"))?;
                let root = registry_lock(&root).clone().ok_or_else(|| {
                    wasmtime::Error::msg("Macro declaration has not published its root")
                })?;
                let mut view = profile.bind(caller.as_context_mut());
                match view.invoke_macro_function(&state, &service, &root, arguments[1]) {
                    Ok(value) => {
                        results[0] = value.val(&mut view.store.as_context_mut());
                        Ok(())
                    }
                    Err(SessionError::Trap(error)) => Err(error),
                    Err(error) => view.throw_evaluation_error(error),
                }
            },
        )?;
        let mut roots = RootScope::new(&mut self.store);
        let mut result = [Val::null_any_ref()];
        // The real Macro function checks its exact arities after data transfer.
        // Universal entries provide both fixed calls and the ordinary apply path.
        constructor.call(
            &mut roots,
            &[
                Val::null_any_ref(),
                Val::FuncRef(Some(callback)),
                Val::I32(0),
                Val::I32(-1),
            ],
            &mut result,
        )?;
        own(&mut roots, result[0], self.identity, &self.handles)
    }

    fn invoke_macro_function(
        &mut self,
        state: &Evaluation,
        service: &MacroService,
        function: &SessionValue,
        arguments: Val,
    ) -> Result<SessionValue, SessionError> {
        self.charge_host_work(1)?;
        let bridge = state.bridge(self)?;
        let mut values = Vec::new();
        {
            let interrupt = self.interrupt.clone();
            let mut roots = RootScope::new(&mut self.store);
            let array = arguments
                .unwrap_anyref()
                .and_then(|value| value.as_array(&roots).ok().flatten())
                .ok_or_else(|| {
                    wasmtime::Error::msg("Native macro call requires the shared argument array")
                })?;
            let count = array.len(&roots)?;
            Self::consume_host_work(roots.as_context_mut(), u64::from(count) + 1, &interrupt)?;
            values
                .try_reserve(count as usize)
                .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
            for index in 0..count {
                let value = array.get(&mut roots, index)?;
                values.push(own(&mut roots, value, self.identity, &self.handles)?);
            }
        }
        let mut forms = Vec::with_capacity(values.len());
        // Explicit form transfer is the same nominal syntax protocol used for
        // source macro arguments. Opaque caller functions/Vars are never shared.
        for value in values {
            forms.push(bridge.read(self, &value, 0..0)?);
        }
        let namespace = self.evaluation_namespace()?;
        let (result, spent) = service.invoke_root(
            function,
            &namespace,
            forms,
            self.operation_fuel_remaining()?,
            self.evaluation_math_context()?,
            self.interrupt.clone(),
        )?;
        self.charge_host_work(spent)?;
        bridge.quote(self, result?)
    }

    fn retain_evaluation_constant(&mut self, value: &SessionValue) -> Result<Kind, SessionError> {
        self.check(value)?;
        // Reuse the evaluation reader's authenticated classification/allocation
        // instead of introducing another public handle or placeholder syntax.
        let state = self
            .evaluation
            .clone()
            .ok_or_else(|| wasmtime::Error::msg("Missing native evaluation authority"))?;
        let bridge = state.bridge(self)?;
        let form = self.read_native_evaluation(&bridge, value, 0..0)?;
        if !matches!(form.kind, Kind::NativeConstant { .. }) {
            return Err(
                wasmtime::Error::msg("Macro root proxy is not a native function constant").into(),
            );
        }
        let (_, _, kind) = form.into_parts();
        Ok(kind)
    }
}
