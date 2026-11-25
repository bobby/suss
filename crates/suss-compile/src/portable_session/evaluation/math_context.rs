//! Snapshot phase-owned dynamic context without validating an unused binding.
//! Valid MathContexts transfer scalar fields. Other materialized data use the
//! existing nominal bridge; unsupported transport is reported only on a read.
use super::*;

pub(crate) enum EvaluationMathContext {
    Nil,
    Context(i32, i32),
    Data(suss_reader::forms::Form),
    Unsupported(String),
}

impl SessionHost<'_> {
    pub(crate) fn evaluation_math_context(
        &mut self,
    ) -> Result<EvaluationMathContext, SessionError> {
        self.charge_host_work(1)?;
        let cell = self.core_cell("*math-context*")?;
        let value = {
            let mut scope = RootScope::new(&mut self.store);
            let getter = self.runtime.get_func(&mut scope, "binding-get").unwrap();
            let input = cell.get(&mut scope);
            call(
                &mut scope,
                self.runtime,
                getter,
                &[input],
                self.identity,
                &self.handles,
            )?
        };
        if value.value.is_none() {
            return Ok(EvaluationMathContext::Nil);
        }
        let context = {
            let mut scope = RootScope::new(&mut self.store);
            let input = value.val(&mut scope.as_context_mut());
            let test = self
                .runtime
                .get_func(&mut scope, "decimal-context-test")
                .unwrap();
            let mut valid = [Val::I32(0)];
            test.call(&mut scope, &[input], &mut valid)?;
            if valid[0].unwrap_i32() != 0 {
                let context = self
                    .runtime
                    .get_func(&mut scope, "decimal-context")
                    .unwrap();
                let mut values = [Val::I32(0), Val::I32(0)];
                if let Err(error) = context.call(&mut scope, &[], &mut values) {
                    return Err(execution_error(
                        &mut scope,
                        self.runtime,
                        error,
                        self.identity,
                        &self.handles,
                    ));
                }
                Some((values[0].unwrap_i32(), values[1].unwrap_i32()))
            } else {
                None
            }
        };
        if let Some((precision, rounding)) = context {
            return Ok(EvaluationMathContext::Context(precision, rounding));
        }
        let state = self
            .evaluation
            .clone()
            .ok_or_else(|| wasmtime::Error::msg("Missing native context transport authority"))?;
        let bridge = state.bridge(self)?;
        match bridge.read_materialized(self, &value) {
            Ok(form) => Ok(EvaluationMathContext::Data(form)),
            // Cancellation, exhausted fuel and language errors remain immediate;
            // only a nominal-data transport diagnostic can be deferred. Never
            // retain a foreign exception/root in the leased Store.
            Err(SessionError::Compile(error)) => {
                Ok(EvaluationMathContext::Unsupported(error.to_string()))
            }
            Err(error) => Err(error),
        }
    }

    fn deferred_context_error(
        &mut self,
        bridge: &FormBridge,
        message: &str,
    ) -> Result<SessionValue, SessionError> {
        let message = bridge.quote(
            self,
            suss_reader::forms::Form {
                span: 0..0,
                metadata: vec![],
                kind: suss_reader::forms::Kind::String(message.encode_utf16().collect()),
            },
        )?;
        let mut scope = RootScope::new(&mut self.store);
        let message = message.val(&mut scope.as_context_mut());
        let delay = self
            .runtime
            .get_func(&mut scope, "binding-delay-error")
            .unwrap();
        call(
            &mut scope,
            self.runtime,
            delay,
            &[message],
            self.identity,
            &self.handles,
        )
    }

    pub(crate) fn enter_evaluation_math_context(
        &mut self,
        context: EvaluationMathContext,
        bridge: &FormBridge,
    ) -> Result<DynamicCheckpoint, SessionError> {
        self.charge_host_work(1)?;
        let cell = self.core_cell("*math-context*")?;
        let data = match &context {
            EvaluationMathContext::Data(form) => Some(bridge.quote_ref(self, form)?),
            EvaluationMathContext::Unsupported(message) => {
                Some(self.deferred_context_error(bridge, message)?)
            }
            _ => None,
        };
        let mut scope = RootScope::new(&mut self.store);
        let checkpoint = dynamic_checkpoint(&mut scope, self.runtime)?;
        let mut invoke = |name: &str, arguments: &[Val]| -> Result<Val, SessionError> {
            let function = self
                .runtime
                .get_func(&mut scope, name)
                .ok_or_else(|| wasmtime::Error::msg(format!("Missing context export {name}")))?;
            let value = call(
                &mut scope,
                self.runtime,
                function,
                arguments,
                self.identity,
                &self.handles,
            )?;
            Ok(value.val(&mut scope.as_context_mut()))
        };
        let new = if let Some(data) = &data {
            data.val(&mut scope.as_context_mut())
        } else if let EvaluationMathContext::Context(precision, rounding) = context {
            let precision = invoke("long-box", &[Val::I64(i64::from(precision))])?;
            let rounding = invoke("long-box", &[Val::I64(i64::from(rounding))])?;
            invoke("decimal-context-new", &[precision, rounding])?
        } else {
            Val::null_any_ref()
        };
        // A dynamic entry is cell, saved value, current value. No root crosses Stores.
        let input = cell.get(&mut scope);
        let getter = self
            .runtime
            .get_func(&mut scope, "binding-snapshot")
            .unwrap();
        let old = call(
            &mut scope,
            self.runtime,
            getter,
            &[input],
            self.identity,
            &self.handles,
        )?;
        let old = old.val(&mut scope.as_context_mut());
        let allocator = self.runtime.get_func(&mut scope, "args-new").unwrap();
        let entries = call(
            &mut scope,
            self.runtime,
            allocator,
            &[Val::I32(3)],
            self.identity,
            &self.handles,
        )?;
        let value = entries.val(&mut scope.as_context_mut());
        let array = value.unwrap_anyref().unwrap().as_array(&scope)?.unwrap();
        for (index, value) in [input, old, new].into_iter().enumerate() {
            array.set(&mut scope, index as u32, value)?;
        }
        let push = self.runtime.get_func(&mut scope, "dynamic-push").unwrap();
        let mut result = [Val::null_any_ref()];
        if let Err(error) = push.call(&mut scope, &[value], &mut result) {
            return Err(execution_error(
                &mut scope,
                self.runtime,
                error,
                self.identity,
                &self.handles,
            ));
        }
        Ok(checkpoint)
    }

    pub(crate) fn leave_evaluation_math_context(
        &mut self,
        checkpoint: &DynamicCheckpoint,
    ) -> Result<(), SessionError> {
        let mut scope = RootScope::new(&mut self.store);
        restore_dynamic(&mut scope, self.runtime, checkpoint)
    }
}
