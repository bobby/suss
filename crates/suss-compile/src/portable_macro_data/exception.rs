//! Owned exception data across isolated Stores; never a foreign GC root.
use super::*;

#[derive(Debug)]
pub(crate) struct TransferredException {
    message: Form,
    data: Option<Form>,
    cause: Option<Box<TransferredException>>,
}
impl std::fmt::Display for TransferredException {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.message.kind {
            Kind::String(units) => output.write_str(&String::from_utf16_lossy(units)),
            _ => output.write_str("Transferred language exception"),
        }
    }
}
impl std::error::Error for TransferredException {}

impl FormBridge {
    pub(crate) fn transfer_exception(&self, session: &mut SessionHost<'_>, value: &SessionValue)
        -> Result<TransferredException, SessionError>
    {
        self.transfer_exception_at(session, value, 0)
    }
    fn transfer_exception_at(&self, session: &mut SessionHost<'_>, value: &SessionValue, depth: usize)
        -> Result<TransferredException, SessionError>
    {
        session.charge_host_work(1)?;
        if depth > 256 { return Err(wasmtime::Error::msg("Exception cause transfer exceeds nesting allowance").into()); }
        // These accessor roots were captured when the isolated Macro host was
        // built, before user source can replace its public core bindings.
        let message = session.invoke_compiler_root(&self.core.factories["exception-message"], &[value])?;
        let data = session.invoke_compiler_root(&self.core.factories["exception-data"], &[value])?;
        let cause = session.invoke_compiler_root(&self.core.factories["exception-cause"], &[value])?;
        let absent = |session: &mut SessionHost<'_>, value: &SessionValue| {
            session.inspect(value, |_, value| Ok(value.unwrap_anyref().is_none()))
        };
        let data = if absent(session, &data)? { None } else { Some(self.read(session, &data, 0..0)?) };
        let cause = if absent(session, &cause)? { None }
            else { Some(Box::new(self.transfer_exception_at(session, &cause, depth+1)?)) };
        Ok(TransferredException { message: self.read(session, &message, 0..0)?, data, cause })
    }
}
impl TransferredException {
    pub(crate) fn restore(&self, bridge: &FormBridge, session: &mut SessionHost<'_>, factory: &SessionValue)
        -> Result<SessionValue, SessionError>
    {
        session.charge_host_work(1)?;
        let message = bridge.quote(session, self.message.clone())?;
        if let Some(data) = &self.data {
            let data = bridge.quote(session, data.clone())?;
            let cause = if let Some(cause) = &self.cause { cause.restore(bridge, session, factory)? }
                else { bridge.scalar(session, &crate::portable::hir::Literal::Nil)? };
            session.invoke(factory, &[&message, &data, &cause])
        } else { session.data_language_error(&message) }
    }
}
