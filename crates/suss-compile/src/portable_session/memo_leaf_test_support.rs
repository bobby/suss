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
