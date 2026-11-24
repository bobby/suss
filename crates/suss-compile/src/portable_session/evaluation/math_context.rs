//! Transfer only immutable MathContext scalars between phase-owned Stores.
use super::*;

impl SessionHost<'_> {
    pub(crate) fn evaluation_math_context(&mut self) -> Result<Option<(i32, i32)>, SessionError> {
        self.charge_host_work(1)?;
        let cell = self.core_cell("*math-context*")?;
        let mut scope = RootScope::new(&mut self.store);
        let getter = self.runtime.get_func(&mut scope, "binding-get").unwrap();
        let input = cell.get(&mut scope);
        let value = call(&mut scope, self.runtime, getter, &[input], self.identity, &self.handles)?;
        if value.value.is_none() { return Ok(None); }
        let context = self.runtime.get_func(&mut scope, "decimal-context")
            .ok_or_else(|| wasmtime::Error::msg("Missing decimal context reader"))?;
        let mut values = [Val::I32(0), Val::I32(0)];
        if let Err(error) = context.call(&mut scope, &[], &mut values) {
            return Err(execution_error(&mut scope, self.runtime, error, self.identity, &self.handles));
        }
        Ok(Some((values[0].unwrap_i32(), values[1].unwrap_i32())))
    }

    pub(crate) fn enter_evaluation_math_context(&mut self, context: Option<(i32, i32)>)
        -> Result<DynamicCheckpoint, SessionError>
    {
        self.charge_host_work(1)?;
        let cell = self.core_cell("*math-context*")?;
        let mut scope = RootScope::new(&mut self.store);
        let checkpoint = dynamic_checkpoint(&mut scope, self.runtime)?;
        let mut invoke = |name: &str, arguments: &[Val]| -> Result<Val, SessionError> {
            let function = self.runtime.get_func(&mut scope, name)
                .ok_or_else(|| wasmtime::Error::msg(format!("Missing context export {name}")))?;
            let value = call(&mut scope, self.runtime, function, arguments, self.identity, &self.handles)?;
            Ok(value.val(&mut scope.as_context_mut()))
        };
        let new = if let Some((precision, rounding)) = context {
            let precision = invoke("long-box", &[Val::I64(i64::from(precision))])?;
            let rounding = invoke("long-box", &[Val::I64(i64::from(rounding))])?;
            invoke("decimal-context-new", &[precision, rounding])?
        } else { Val::null_any_ref() };
        // A dynamic entry is cell, saved value, current value. No root crosses Stores.
        let input = cell.get(&mut scope);
        let getter = self.runtime.get_func(&mut scope, "binding-get").unwrap();
        let old = call(&mut scope, self.runtime, getter, &[input], self.identity, &self.handles)?;
        let old = old.val(&mut scope.as_context_mut());
        let allocator = self.runtime.get_func(&mut scope, "args-new").unwrap();
        let entries = call(&mut scope, self.runtime, allocator, &[Val::I32(3)], self.identity, &self.handles)?;
        let value = entries.val(&mut scope.as_context_mut());
        let array = value.unwrap_anyref().unwrap().as_array(&scope)?.unwrap();
        for (index, value) in [input, old, new].into_iter().enumerate() {
            array.set(&mut scope, index as u32, value)?;
        }
        let push = self.runtime.get_func(&mut scope, "dynamic-push").unwrap();
        let mut result = [Val::null_any_ref()];
        if let Err(error) = push.call(&mut scope, &[value], &mut result) {
            return Err(execution_error(&mut scope, self.runtime, error, self.identity, &self.handles));
        }
        Ok(checkpoint)
    }

    pub(crate) fn leave_evaluation_math_context(&mut self, checkpoint: &DynamicCheckpoint)
        -> Result<(), SessionError>
    {
        let mut scope = RootScope::new(&mut self.store);
        restore_dynamic(&mut scope, self.runtime, checkpoint)
    }
}
