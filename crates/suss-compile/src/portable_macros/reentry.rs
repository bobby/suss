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
        let mut macros = CompiledMacros::new().unwrap();
        macros.define("(defmacro twice [x] (list '+ x x))").unwrap();
        let reader = macros.bridge.borrowed_reader(&mut macros.session).unwrap();
        let profile = macros.reentry_profile();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        macros.install_observation_hook(move |mut caller, value| {
            // This is reached while a compiled macro is executing in this very
            // Store, not from an idle/fresh macro host or a replayed definition.
            assert!(profile.catalog.try_lock().is_ok(), "no catalog guard across Wasm");
            observed.fetch_add(1, Ordering::SeqCst);
            let argument = reader.read(caller.as_context_mut(), &value, 0..0)?;
            let mut view = profile.bind(caller);
            view.define("(defmacro made-in-callback [x] (list '+ x 1))")
                .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
            let call = Form {
                span: 0..0,
                metadata: vec![],
                kind: Kind::List(vec![symbol("twice", 0..0), argument]),
            };
            let snapshot = view.session.compilation_snapshot();
            let prepared = snapshot.prepare_with_origin(vec![call], 0..0, &mut view, None)
                .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
            let value = view.session.eval_prepared(prepared)
                .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
            let value = view.bridge.read(&mut view.session, &value, 0..0)
                .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
            match value.kind {
                Kind::Long(value) => u32::try_from(value).map_err(wasmtime::Error::from),
                _ => Err(wasmtime::Error::msg("expected a compiled long result")),
            }
        }).unwrap();
        macros.define(concat!(
            "(defmacro reentrant [x] (list '+ ",
            "(suss.harness.observation/capture x) ",
            "(suss.harness.observation/capture 1)))",
        )).unwrap();
        let mut caller = Session::new_repl().unwrap();
        let bridge = FormBridge::new(&mut caller).unwrap();
        let result = caller.eval_with_macros("(reentrant 21)", &mut macros).unwrap();
        caller.collect().unwrap();
        assert!(matches!(bridge.read(&mut caller, &result, 0..0).unwrap().kind, Kind::Long(44)));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let result = caller.eval_with_macros("(made-in-callback 41)", &mut macros).unwrap();
        assert!(matches!(bridge.read(&mut caller, &result, 0..0).unwrap().kind, Kind::Long(42)));
        assert_eq!(calls.load(Ordering::SeqCst), 2, "no reconstruction or replay");
    }
}
