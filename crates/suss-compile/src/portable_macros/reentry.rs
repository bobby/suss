//! The same Macro authority, bound to its active caller rather than its owner.
use super::*;

pub(crate) struct MacroReentryProfile {
    session: crate::portable_session::ReentryProfile,
    bridge: Arc<FormBridge>,
    catalog: Arc<Mutex<MacroCatalog>>,
    namespace_interner: SessionValue,
    namespace_setter: SessionValue,
    exception_message: SessionValue,
}
impl MacroHost<'_> {
    /// Entry configuration is captured here; mutable compiler and macro state
    /// remains authoritative and shared. Bind only this Macro Store's Caller.
    pub(crate) fn reentry_profile(&self) -> MacroReentryProfile {
        MacroReentryProfile {
            session: self.session.reentry_profile(),
            bridge: self.bridge.clone(),
            catalog: self.catalog.clone(),
            namespace_interner: self.namespace_interner.clone(),
            namespace_setter: self.namespace_setter.clone(),
            exception_message: self.exception_message.clone(),
        }
    }
}
impl MacroReentryProfile {
    pub(crate) fn bind<'a>(&self, caller: wasmtime::StoreContextMut<'a, ()>) -> MacroHost<'a> {
        MacroHost {
            session: self.session.bind(caller),
            bridge: self.bridge.clone(),
            catalog: self.catalog.clone(),
            namespace_interner: self.namespace_interner.clone(),
            namespace_setter: self.namespace_setter.clone(),
            exception_message: self.exception_message.clone(),
        }
    }
}

#[cfg(all(test, not(suss_generator)))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use wasmtime::AsContextMut;

    #[test]
    fn borrowed_macro_authority_expands_existing_macros_and_publishes_new_ones() {
        let mut macros = MacroHost::new().unwrap();
        macros.define("(defmacro twice [x] (list '+ x x))").unwrap();
        let reader = macros.bridge.borrowed_reader(&mut macros.session).unwrap();
        let profile = macros.reentry_profile();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let input = macros.bridge.quote(&mut macros.session,
            read_forms("(twice 21)").unwrap().remove(0)).unwrap();
        let result = macros.session.probe_evaluation(&input, move |mut caller, value| {
            // An explicit evaluate-form caller is active in the same Macro
            // Store. Observation hooks keep their non-reentrant contract.
            assert!(profile.catalog.try_lock().is_ok(), "no catalog guard across Wasm");
            observed.fetch_add(1, Ordering::SeqCst);
            let form = reader.read(caller.as_context_mut(), &value, 0..0)?;
            let mut view = profile.bind(caller);
            view.define("(defmacro made-in-callback [x] (list '+ x 1))")
                .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
            let snapshot = view.session.compilation_snapshot();
            let prepared = snapshot.prepare_with_origin(vec![form], 0..0, &mut view, None)
                .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
            view.session.eval_prepared(prepared)
                .map_err(|error| wasmtime::Error::msg(error.to_string()))
        }).unwrap();
        macros.session.collect().unwrap();
        assert!(matches!(macros.bridge.read(&mut macros.session, &result, 0..0).unwrap().kind,
            Kind::Long(42)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let mut caller = Session::new_repl().unwrap();
        let bridge = FormBridge::new(&mut caller).unwrap();
        let result = caller.eval_with_macros("(made-in-callback 41)", &mut macros).unwrap();
        assert!(matches!(bridge.read(&mut caller, &result, 0..0).unwrap().kind, Kind::Long(42)));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "no reconstruction or replay");
    }
}
