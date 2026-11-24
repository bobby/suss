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
}
impl Evaluation {
    pub(super) fn clear(&self) {
        // Drop compiler services and roots outside the locks. Their teardown may
        // retire native requests in another isolated phase Store.
        let profile = registry_lock(&self.profile).take();
        let bridge = registry_lock(&self.bridge).take();
        let external = registry_lock(&self.external).take();
        let own = registry_lock(&self.own).take();
        drop((profile, bridge, external, own));
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
        let bridge = self.bridge(&mut view)?;
        let value = {
            let mut roots = RootScope::new(&mut view.store);
            own(&mut roots, input, view.identity, &view.handles)?
        };
        let form = bridge.read(&mut view, &value, 0..0)?;
        let authority = registry_lock(&self.own).clone();
        if let Some(authority) = authority {
            // This Store already owns user macro definitions. Reentry uses its
            // live catalog and the active Caller, not its unavailable owner.
            return authority.bind(view).evaluate_native_form(form);
        }
        let service = self.service()?;
        let snapshot = view.compilation_snapshot();
        let allowance = view.operation_fuel_remaining()?;
        let (prepared, spent) = service.prepare_evaluation(&snapshot, form, allowance)?;
        // Compilation and macro execution consume the caller's allowance too.
        // Check cancellation before considering the compiler's returned error.
        view.charge_host_work(spent)?;
        view.eval_prepared(prepared?)
    }
}

impl SessionHost<'_> {
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
