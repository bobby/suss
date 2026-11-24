//! Native form-data compilation in the caller's Store and persistent catalog.
//! One callback is installed per Store. Its profile is refreshed before owned
//! entries; borrowed entries retain the caller's fuel, cancellation and roots.
use super::*;
use crate::{
    portable_macro_data::FormBridge,
    portable_macros::{CompiledMacros, MacroAuthority, MacroService},
};

#[derive(Default)]
pub(super) struct Evaluation {
    profile: Mutex<Option<Arc<ReentryProfile>>>,
    bridge: Mutex<Option<Arc<FormBridge>>>,
    external: Mutex<Option<MacroService>>,
    own: Mutex<Option<MacroAuthority>>,
    namespace: Mutex<Option<NamespaceAccess>>,
}

#[derive(Clone)]
struct NamespaceAccess {
    cell: Global,
    namespace: OwnedRooted<AnyRef>,
    symbol: OwnedRooted<AnyRef>,
}
impl NamespaceAccess {
    fn capture(session: &mut SessionHost<'_>) -> Result<Self, SessionError> {
        let cell = session.core_cell("*ns*")?;
        let mut descriptor = |name: &str| {
            let cell = session.core_cell(name)?;
            let runtime = session.runtime;
            let value = {
                let mut roots = RootScope::new(&mut session.store);
                let get = runtime.get_func(&mut roots, "binding-get").unwrap();
                let cell = cell.get(&mut roots);
                call(
                    &mut roots,
                    runtime,
                    get,
                    &[cell],
                    session.identity,
                    &session.handles,
                )?
            };
            let descriptor = session.data_descriptor(&value, true)?;
            Ok::<_, SessionError>(descriptor.object()?.clone())
        };
        Ok(Self {
            cell,
            namespace: descriptor("Namespace")?,
            symbol: descriptor("Symbol")?,
        })
    }
    fn read(&self, session: &mut SessionHost<'_>) -> Result<String, SessionError> {
        session.charge_host_work(16)?;
        let runtime = session.runtime;
        let mut roots = RootScope::new(&mut session.store);
        let get = runtime.get_func(&mut roots, "binding-get").unwrap();
        let cell = self.cell.get(&mut roots);
        let namespace = call(
            &mut roots,
            runtime,
            get,
            &[cell],
            session.identity,
            &session.handles,
        )?;
        let namespace = namespace.val(&mut roots.as_context_mut());
        fn fields(
            store: &mut StoreContextMut<'_, ()>,
            value: Val,
            expected: &OwnedRooted<AnyRef>,
            count: u32,
        ) -> wasmtime::Result<wasmtime::Rooted<wasmtime::ArrayRef>> {
            fn reference(value: Val) -> wasmtime::Result<wasmtime::Rooted<AnyRef>> {
                match value {
                    Val::AnyRef(Some(value)) => Ok(value),
                    _ => Err(wasmtime::Error::msg(
                        "Namespace storage contains a non-reference",
                    )),
                }
            }
            let object = reference(value)?.as_struct(&*store)?.ok_or_else(|| {
                wasmtime::Error::msg("Current namespace has an invalid nominal shape")
            })?;
            if object.ty(&*store)?.fields().len() != 4 {
                return Err(wasmtime::Error::msg(
                    "Current namespace has invalid object storage",
                ));
            }
            let actual = reference(object.field(&mut *store, 0)?)?;
            let expected = expected.to_rooted(&mut *store);
            if !wasmtime::Rooted::ref_eq(&*store, &actual, &expected)? {
                return Err(wasmtime::Error::msg(
                    "Current namespace has a foreign descriptor",
                ));
            }
            let values = reference(object.field(&mut *store, 1)?)?
                .as_array(&*store)?
                .ok_or_else(|| wasmtime::Error::msg("Current namespace has invalid fields"))?;
            if values.len(&*store)? != count {
                return Err(wasmtime::Error::msg(
                    "Current namespace has an invalid field count",
                ));
            }
            Ok(values)
        }
        let data = fields(&mut roots.as_context_mut(), namespace, &self.namespace, 2)?;
        let name = data.get(&mut roots, 0)?;
        let data = fields(&mut roots.as_context_mut(), name, &self.symbol, 5)?;
        let name = data
            .get(&mut roots, 1)?
            .unwrap_anyref()
            .and_then(|value| value.as_array(&roots).ok().flatten())
            .ok_or_else(|| wasmtime::Error::msg("Namespace name is not a string"))?;
        if !matches!(name.ty(&roots)?.element_type(), wasmtime::StorageType::I16) {
            return Err(wasmtime::Error::msg("Namespace name is not UTF-16").into());
        }
        let length = name.len(&roots)?;
        SessionHost::consume_host_work(
            roots.as_context_mut(),
            u64::from(length) * 2 + 1,
            &session.interrupt,
        )?;
        let units = name
            .elems(&mut roots)?
            .map(|unit| unit.unwrap_i32() as u16)
            .collect::<Vec<_>>();
        String::from_utf16(&units).map_err(|error| wasmtime::Error::from(error).into())
    }
}
impl Evaluation {
    pub(super) fn clear(&self) {
        // Drop compiler services and roots outside the locks. Their teardown may
        // retire native requests in another isolated phase Store.
        let profile = registry_lock(&self.profile).take();
        let bridge = registry_lock(&self.bridge).take();
        let external = registry_lock(&self.external).take();
        let own = registry_lock(&self.own).take();
        let namespace = registry_lock(&self.namespace).take();
        drop((profile, bridge, external, own, namespace));
    }
    fn bridge(&self, view: &mut SessionHost<'_>) -> Result<Arc<FormBridge>, SessionError> {
        let existing = registry_lock(&self.bridge).clone();
        if let Some(bridge) = existing {
            return Ok(bridge);
        }
        let bridge = Arc::new(FormBridge::new(view)?);
        *registry_lock(&self.bridge) = Some(bridge.clone());
        Ok(bridge)
    }
    fn service(&self) -> Result<MacroService, SessionError> {
        let existing = registry_lock(&self.external).clone();
        if let Some(service) = existing {
            return Ok(service);
        }
        // A bare Session has no user macro host yet. Retain one bootstrap host
        // for this Session; never construct a fresh host for each eval input.
        let service = CompiledMacros::new()?.service();
        *registry_lock(&self.external) = Some(service.clone());
        Ok(service)
    }
    fn evaluate(
        &self,
        profile: &ReentryProfile,
        caller: StoreContextMut<'_, ()>,
        input: Val,
    ) -> Result<SessionValue, SessionError> {
        let mut view = profile.bind(caller);
        view.charge_host_work(1)?;
        let value = {
            let mut roots = RootScope::new(&mut view.store);
            own(&mut roots, input, view.identity, &view.handles)?
        };
        // Existing function objects are compiler constants, not syntax. Keep
        // their actual root/identity; every executable data form still compiles.
        let predicate = view
            .runtime
            .get_func(&mut view.store, "closure-value-test")
            .ok_or_else(|| wasmtime::Error::msg("Missing closure value classifier"))?;
        let argument = value.val(&mut view.store.as_context_mut());
        let mut is_function = [Val::I32(0)];
        predicate.call(&mut view.store, &[argument], &mut is_function)?;
        if is_function[0].unwrap_i32() != 0 {
            return Ok(value);
        }
        let bridge = self.bridge(&mut view)?;
        let form = bridge.read(&mut view, &value, 0..0)?;
        let namespace = registry_lock(&self.namespace)
            .clone()
            .ok_or_else(|| wasmtime::Error::msg("Missing native namespace authority"))?;
        let namespace = namespace.read(&mut view)?;
        let authority = registry_lock(&self.own).clone();
        if let Some(authority) = authority {
            // This Store already owns user macro definitions. Reentry uses its
            // live catalog and the active Caller, not its unavailable owner.
            return authority.bind(view).evaluate_native_form(form, &namespace);
        }
        let service = self.service()?;
        let mut snapshot = view.compilation_snapshot();
        let original = snapshot
            .environment
            .current_namespace(view.phase)
            .to_owned();
        snapshot
            .environment
            .enter_namespace(view.phase, &namespace)
            .map_err(SessionError::Compile)?;
        let allowance = view.operation_fuel_remaining()?;
        let (prepared, spent) = service.prepare_evaluation(&snapshot, form, allowance)?;
        // Compilation and macro execution consume the caller's allowance too.
        // Check cancellation before considering the compiler's returned error.
        view.charge_host_work(spent)?;
        let mut prepared = prepared?;
        if prepared.fragment.namespace_directive.is_none() {
            // The runtime binding selects this analysis context. Its temporary
            // namespace must not replace the host's ordinary input context.
            prepared
                .fragment
                .environment
                .enter_namespace(view.phase, &original)
                .map_err(SessionError::Compile)?;
        }
        view.eval_prepared(prepared)
    }
}

impl SessionHost<'_> {
    fn core_cell(&self, name: &str) -> Result<Global, SessionError> {
        let compiler = registry_lock(&self.compiler);
        let global = compiler
            .environment
            .resolve(
                self.phase,
                &suss_reader::Symbol::namespaced("suss.core", name),
                0..0,
            )
            .map_err(SessionError::Compile)?
            .global()
            .clone();
        compiler
            .cells
            .get(&global)
            .copied()
            .ok_or_else(|| wasmtime::Error::msg("Missing core binding cell").into())
    }
    pub(crate) fn attach_expander(&mut self, expander: &dyn portable::ExpansionHost) {
        let Some(service) = expander
            .persistent_macro_context()
            .and_then(|value| value.downcast_ref::<MacroService>())
        else {
            return;
        };
        if let Some(evaluation) = &self.evaluation {
            *registry_lock(&evaluation.external) = Some(service.clone());
        }
    }
    pub(crate) fn attach_macro_authority(&mut self, authority: MacroAuthority) {
        if let Some(evaluation) = &self.evaluation {
            *registry_lock(&evaluation.bridge) = Some(authority.bridge());
            *registry_lock(&evaluation.own) = Some(authority);
        }
    }
    pub(super) fn refresh_evaluation_profile(&self) {
        if let Some(evaluation) = &self.evaluation {
            *registry_lock(&evaluation.profile) = Some(Arc::new(self.reentry_profile()));
        }
    }
    pub(super) fn install_evaluation(&mut self) -> Result<(), SessionError> {
        if self.evaluation.is_some() {
            return Ok(());
        }
        let state = Arc::new(Evaluation::default());
        *registry_lock(&state.namespace) = Some(NamespaceAccess::capture(self)?);
        let runtime = self.runtime;
        let signature = runtime
            .get_func(&mut self.store, "evaluate-form")
            .ok_or_else(|| wasmtime::Error::msg("Missing evaluation capability"))?
            .ty(&self.store);
        let callback_state = state.clone();
        let callback = Func::try_new(
            &mut self.store,
            signature,
            move |mut caller, args, results| {
                let profile = registry_lock(&callback_state.profile)
                    .clone()
                    .ok_or_else(|| wasmtime::Error::msg("No active native compiler profile"))?;
                let result =
                    callback_state.evaluate(&profile, caller.as_context_mut(), args[0].clone());
                match result {
                    Ok(value) => {
                        results[0] = value.val(&mut caller.as_context_mut());
                        Ok(())
                    }
                    Err(SessionError::Trap(error)) => Err(error),
                    Err(error) => {
                        let mut view = profile.bind(caller.as_context_mut());
                        view.throw_evaluation_error(error)
                    }
                }
            },
        )?;
        runtime
            .get_func(&mut self.store, "evaluation-source-install")
            .ok_or_else(|| wasmtime::Error::msg("Missing evaluation installer"))?
            .call(&mut self.store, &[Val::FuncRef(Some(callback))], &mut [])?;
        self.evaluation = Some(state);
        self.refresh_evaluation_profile();
        Ok(())
    }
    fn throw_evaluation_error<T>(&mut self, error: SessionError) -> wasmtime::Result<T> {
        let payload = match error {
            SessionError::Language(value) => value,
            error => {
                let text = error.to_string();
                let units = text.encode_utf16().collect::<Vec<_>>();
                self.charge_host_work(units.len() as u64 * 2 + 1)
                    .map_err(native_error)?;
                let runtime = self.runtime;
                let mut roots = RootScope::new(&mut self.store);
                let new = runtime.get_func(&mut roots, "string-new").unwrap();
                let string = call(
                    &mut roots,
                    runtime,
                    new,
                    &[Val::I32(i32::try_from(units.len())?)],
                    self.identity,
                    &self.handles,
                )
                .map_err(native_error)?;
                let array = string
                    .object()?
                    .to_rooted(&mut roots)
                    .as_array(&roots)?
                    .unwrap();
                for (index, unit) in units.into_iter().enumerate() {
                    array.set(&mut roots, index as u32, Val::I32(i32::from(unit)))?;
                }
                let new = runtime.get_func(&mut roots, "language-error-new").unwrap();
                let string = string.val(&mut roots.as_context_mut());
                call(
                    &mut roots,
                    runtime,
                    new,
                    &[string],
                    self.identity,
                    &self.handles,
                )
                .map_err(native_error)?
            }
        };
        let tag = self
            .runtime
            .get_tag(&mut self.store, "language-exception")
            .ok_or_else(|| wasmtime::Error::msg("Missing language exception tag"))?;
        let ty = wasmtime::ExnType::new(&self.engine, tag.ty(&self.store).ty().params())?;
        let allocator = wasmtime::ExnRefPre::new(&mut self.store, ty);
        let value = payload.val(&mut self.store.as_context_mut());
        let exception = wasmtime::ExnRef::new(&mut self.store, &allocator, &tag, &[value])?;
        self.store.as_context_mut().throw(exception)
    }
}

fn native_error(error: SessionError) -> wasmtime::Error {
    match error {
        SessionError::Trap(error) | SessionError::Host(error) => error,
        error => wasmtime::Error::msg(error.to_string()),
    }
}
