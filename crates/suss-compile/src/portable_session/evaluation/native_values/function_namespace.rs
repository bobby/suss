//! Implicit thread namespace context follows calls while code/cells retain
//! their phase owner. Explicit Namespace arguments still retain actual owners.
use super::*;

pub(super) enum FunctionNamespace {
    Value(Transfer),
    Unsupported(String),
}
impl SessionHost<'_> {
    pub(super) fn export_function_namespace(&mut self) -> Result<FunctionNamespace, SessionError> {
        self.charge_host_work(1)?;
        let cell = self.core_cell("*ns*")?;
        let value = {
            let mut roots = RootScope::new(&mut self.store);
            // An unused deferred context must not throw while being forwarded.
            let get = self.runtime.get_func(&mut roots, "binding-read-unforced").unwrap();
            let cell = cell.get(&mut roots);
            call(&mut roots, self.runtime, get, &[cell], self.identity, &self.handles)?
        };
        // Namespace is an ordinary authenticated value here. Preserve its
        // actual owner and identity, as for explicit arguments. Compiler/cell
        // authority continues to come from the executing Session's snapshot.
        match self.export_value(&value) {
            Ok(value) => Ok(FunctionNamespace::Value(value)),
            Err(SessionError::Compile(error)) => Ok(FunctionNamespace::Unsupported(error.to_string())),
            Err(error) => Err(error),
        }
    }
    pub(super) fn enter_function_namespace(&mut self, context: FunctionNamespace) -> Result<DynamicCheckpoint, SessionError> {
        let namespace = match context {
            FunctionNamespace::Value(value) => self.import_value(value)?,
            FunctionNamespace::Unsupported(message) => {
                let bridge = self.value_state()?.bridge(self)?;
                self.deferred_context_error(&bridge, &message)?
            }
        };
        self.enter_compilation_namespace(&namespace)
    }
}
