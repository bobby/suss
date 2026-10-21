//! Test-only capture/restore of the actual initial IHash method. No guest API.
use super::*;

pub(crate) struct Method {
    descriptor: SessionValue,
    key: SessionValue,
    implementation: SessionValue,
}
impl Method {
    pub(crate) fn capture(session: &mut Session, owner: &str) -> Result<Self, SessionError> {
        let class = session.eval(owner)?;
        let descriptor = session.data_descriptor(&class, true)?;
        let name = format!(
            "key-{}",
            "suss.core/IHash/-hash/1"
                .bytes()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let handles = session.handles.clone();
        let key =
            canonical_methods::binding(session, "suss.internal.protocol-keys", &name, &handles)?;
        let implementation = {
            let mut scope = RootScope::new(&mut session.store);
            let descriptor = Val::AnyRef(Some(descriptor.value.to_rooted(&mut scope)));
            let key = Val::AnyRef(Some(key.value.to_rooted(&mut scope)));
            let function = session
                .runtime
                .get_func(&mut scope, "protocol-method-get")
                .unwrap();
            call(
                &mut scope,
                session.runtime,
                function,
                &[descriptor, key],
                session.identity,
                &session.handles,
            )?
        };
        Ok(Self {
            descriptor,
            key,
            implementation,
        })
    }
    pub(crate) fn is_current(
        &self,
        session: &mut Session,
        owner: &str,
    ) -> Result<bool, SessionError> {
        let current = Self::capture(session, owner)?;
        session.charge_host_work(3)?;
        let mut scope = RootScope::new(&mut session.store);
        for (expected, actual) in [
            (&self.descriptor, &current.descriptor),
            (&self.key, &current.key),
            (&self.implementation, &current.implementation),
        ] {
            let expected = expected.value.to_rooted(&mut scope);
            let actual = actual.value.to_rooted(&mut scope);
            if !wasmtime::Rooted::ref_eq(&scope, &expected, &actual)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub(crate) fn bind_delegate(&self, session: &mut Session) -> Result<(), SessionError> {
        let bind = session.eval("(fn [f] (set! leaf-original-method f))")?;
        session.invoke(&bind, &[&self.implementation])?;
        Ok(())
    }
    pub(crate) fn restore(&self, session: &mut Session) -> Result<(), SessionError> {
        let mut scope = RootScope::new(&mut session.store);
        let descriptor = Val::AnyRef(Some(self.descriptor.value.to_rooted(&mut scope)));
        let key = Val::AnyRef(Some(self.key.value.to_rooted(&mut scope)));
        let method = Val::AnyRef(Some(self.implementation.value.to_rooted(&mut scope)));
        let function = session
            .runtime
            .get_func(&mut scope, "protocol-method-set")
            .unwrap();
        function
            .call(&mut scope, &[descriptor, key, method], &mut [])
            .map_err(Into::into)
    }
}

// The parent module is cfg(test). This helper grants only native compilation
// headroom; Session options and Runtime Store allowances are never changed.
#[path = "../../tests/support/compilation_fuel.rs"]
mod compilation_fuel;
pub(crate) use compilation_fuel::COMPILATION_FUEL;


pub(crate) fn eval_with_compilation_fuel(
    session: &mut Session,
    source: &str,
) -> Result<SessionValue, SessionError> {
    struct Compilation(u64);
    impl crate::portable::ExpansionHost for Compilation {
        fn expansion_work_fuel(&self) -> u64 {
            self.0
        }
        fn artifact_dependencies(&self) -> Option<Vec<(String, String)>> {
            Some(vec![])
        }
        fn charge_expansion_work(
            &mut self,
            work: u64,
            span: std::ops::Range<usize>,
        ) -> Result<(), crate::portable::Diagnostic> {
            self.0 =
                self.0
                    .checked_sub(work)
                    .ok_or_else(|| crate::portable::Diagnostic {
                        span,
                        message: "Semantic corpus compilation fuel exhausted".into(),
                    })?;
            Ok(())
        }
        fn expand(
            &mut self,
            _: &suss_reader::forms::Form,
            _: crate::portable::ExpansionContext<'_>,
        ) -> Result<Option<suss_reader::forms::Form>, crate::portable::Diagnostic> {
            Ok(None)
        }
    }
    let runtime_fuel = session.options().fuel_per_operation;
    let result = session.eval_with_macros(source, &mut Compilation(COMPILATION_FUEL));
    assert_eq!(session.options().fuel_per_operation, runtime_fuel);
    result
}

// Test-only convenience for semantic fixtures whose source compilation is not
// the resource limit under test. Runtime options and guest allowances stay put.
impl Session {
    pub(crate) fn eval_with_test_compilation_fuel(
        &mut self,
        source: &str,
    ) -> Result<SessionValue, SessionError> {
        eval_with_compilation_fuel(self, source)
    }
}

/// One top-level compiler-data operation in a Macro Store. The original Runtime
/// setting is restored on success, error and unwind, before later projections.
/// This never refills an already active request or accepts a Runtime Store.
pub(crate) fn with_compilation_data_fuel<T>(
    session: &mut Session,
    operation: impl FnOnce(&mut Session) -> Result<T, SessionError>,
) -> Result<T, SessionError> {
    assert_eq!(session.phase(), crate::portable::resolve::Phase::Macro);
    assert_eq!(session.operation_budget_depth.load(std::sync::atomic::Ordering::SeqCst), 0);
    struct Restore<'a> { session: &'a mut Session, fuel: u64 }
    impl Drop for Restore<'_> {
        fn drop(&mut self) { self.session.set_operation_fuel(self.fuel); }
    }
    let fuel = session.options().fuel_per_operation;
    session.set_operation_fuel(COMPILATION_FUEL);
    let restore = Restore { session, fuel };
    restore.session.with_operation_budget(operation)
}

#[test]
fn compiler_data_allowance_restores_runtime_fuel_after_success_error_and_unwind() {
    let mut session = Session::new_macro().unwrap();
    session.set_operation_fuel(1);
    with_compilation_data_fuel(&mut session, |s| s.charge_host_work(2)).unwrap();
    assert_eq!(session.options().fuel_per_operation, 1);
    let runtime_error = session.with_operation_budget(|s| s.charge_host_work(2)).unwrap_err();
    assert!(matches!(runtime_error, SessionError::Trap(ref error)
        if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
    let error = with_compilation_data_fuel(&mut session, |s| s.charge_host_work(u64::MAX)).unwrap_err();
    assert!(matches!(error, SessionError::Trap(ref trap)
        if trap.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
    assert_eq!(session.options().fuel_per_operation, 1);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_compilation_data_fuel(&mut session, |_| -> Result<(), SessionError> {
            panic!("intentional compiler-data scope unwind");
        })
    }));
    assert!(result.is_err());
    assert_eq!(session.options().fuel_per_operation, 1);
    session.with_operation_budget(|s| s.charge_host_work(1)).unwrap();
}
