//! Operations on genuine source-owned Vars, Namespaces and namespace arrays.
use super::*;
use crate::portable::hir::Literal;
use crate::runtime_abi::native_values as op;

impl SessionHost<'_> {
    pub(in crate::portable_session) fn install_native_operations(
        &mut self,
        state: &Arc<Evaluation>,
    ) -> Result<(), SessionError> {
        let signature = self
            .runtime
            .get_func(&mut self.store, "native-value-operation")
            .unwrap()
            .ty(&self.store);
        let state_ref = Arc::downgrade(state);
        let callback = Func::try_new(
            &mut self.store,
            signature,
            move |mut caller, args, results| {
                let state = state_ref
                    .upgrade()
                    .filter(|s| !s.retired.load(Ordering::SeqCst))
                    .ok_or_else(|| wasmtime::Error::msg("Native value owner has retired"))?;
                let endpoint = capability(caller.as_context_mut(), args[0])?
                    .ok_or_else(|| wasmtime::Error::msg("Invalid native operation capability"))?;
                let profile = registry_lock(&state.profile)
                    .clone()
                    .ok_or_else(|| wasmtime::Error::msg("No active native value profile"))?;
                let mut view = profile.bind(caller.as_context_mut());
                let _values = view.native_values_scope();
                let result = view.native_operation(
                    &endpoint,
                    args[1].unwrap_i32(),
                    args[2].unwrap_i32(),
                    args[3],
                    args[4],
                );
                match result {
                    Ok(value) => {
                        results[0] = value.val(&mut view.store.as_context_mut());
                        Ok(())
                    }
                    Err(SessionError::Trap(error)) => Err(error),
                    Err(error) => view.throw_evaluation_error(error),
                }
            },
        )?;
        self.runtime
            .get_func(&mut self.store, "native-value-install")
            .unwrap()
            .call(&mut self.store, &[Val::FuncRef(Some(callback))], &mut [])?;
        Ok(())
    }

    fn live_runtime_value(
        &mut self,
        name: &str,
        value: &SessionValue,
        index: Option<i32>,
        args: &[SessionValue],
        boolean: bool,
        void: bool,
    ) -> Result<SessionValue, SessionError> {
        self.check(value)?;
        self.charge_host_work(args.len() as u64 + 1)?;
        for arg in args {
            self.check(arg)?;
        }
        let result = {
            let mut roots = RootScope::new(&mut self.store);
            let mut params = vec![value.val(&mut roots.as_context_mut())];
            if let Some(index) = index {
                params.push(Val::I32(index));
            }
            for arg in args {
                params.push(arg.val(&mut roots.as_context_mut()));
            }
            let function = self.runtime.get_func(&mut roots, name).unwrap();
            if boolean {
                let mut result = [Val::I32(0)];
                function.call(&mut roots, &params, &mut result)?;
                result[0].unwrap_i32() != 0
            } else if void {
                function.call(&mut roots, &params, &mut [])?;
                false
            } else {
                return call(
                    &mut roots,
                    self.runtime,
                    function,
                    &params,
                    self.identity,
                    &self.handles,
                );
            }
        };
        self.data_scalar(&if boolean {
            Literal::Bool(result)
        } else {
            Literal::Nil
        })
    }

    fn native_operation(
        &mut self,
        endpoint: &Arc<ForeignValue>,
        code: i32,
        index: i32,
        first: Val,
        second: Val,
    ) -> Result<SessionValue, SessionError> {
        self.charge_host_work(1)?;
        let (name, count, indexed, boolean, void) = match (&endpoint.kind, code) {
            (NativeKind::Nominal(_), op::FIELD_GET) => ("object-field-get", 0, true, false, false),
            (NativeKind::Nominal(_), op::FIELD_SET) => ("object-field-set", 1, true, false, true),
            (NativeKind::Function { .. }, op::GETTER_MACRO) => {
                ("binding-getter-macro?", 0, false, true, false)
            }
            (NativeKind::Function { .. }, op::GETTER_MACRO_SET) => {
                ("binding-getter-macro-set!", 1, false, false, false)
            }
            (NativeKind::ObjectArray, op::ARRAY_LENGTH) => {
                ("packed-array-length", 0, false, false, false)
            }
            (NativeKind::ObjectArray, op::ARRAY_GET) => {
                ("packed-array-get", 1, false, false, false)
            }
            (NativeKind::ObjectArray, op::ARRAY_SET) => {
                ("packed-array-set", 2, false, false, false)
            }
            (NativeKind::ObjectArray, op::ARRAY_CLONE) => {
                ("packed-array-clone", 0, false, false, false)
            }
            (NativeKind::ObjectArray, op::ARRAY_ELEMENTS) => {
                return self.native_array_elements(endpoint);
            }
            (NativeKind::ObjectArray, op::ARRAY_COPY) => {
                return self.native_array_copy(endpoint, first);
            }
            _ => {
                return Err(wasmtime::Error::msg(
                    "Native operation does not match its actual source value",
                )
                .into());
            }
        };
        let args = {
            let mut roots = RootScope::new(&mut self.store);
            [first, second]
                .into_iter()
                .take(count)
                .map(|value| own(&mut roots, value, self.identity, &self.handles))
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut transferred = Vec::with_capacity(count);
        for arg in &args {
            transferred.push(self.export_value(arg)?);
        }
        let result = endpoint.with_source(self, |source, original| {
            let _values = source.native_values_scope();
            let mut args = Vec::with_capacity(count);
            for arg in transferred {
                args.push(source.import_value(arg)?);
            }
            let value = source.live_runtime_value(
                name,
                original,
                indexed.then_some(index),
                &args,
                boolean,
                void,
            )?;
            source.export_value(&value)
        })?;
        self.import_value(result)
    }

    pub(crate) fn native_namespace_name(
        &mut self,
        value: &SessionValue,
    ) -> Result<Option<String>, SessionError> {
        let Some(endpoint) = self.native_value_endpoint(value)? else {
            return Ok(None);
        };
        if !matches!(endpoint.kind, NativeKind::Nominal(LiveClass::Namespace)) {
            return Ok(None);
        }
        endpoint
            .with_source(self, |source, original| {
                let access = registry_lock(&source.value_state()?.namespace)
                    .as_ref()
                    .cloned()
                    .ok_or_else(|| {
                        wasmtime::Error::msg("Missing native Namespace reader authority")
                    })?;
                access.value_name(source, original)
            })
            .map(Some)
    }

    pub(crate) fn native_live_fields(
        &mut self,
        value: &SessionValue,
    ) -> Result<Option<Vec<SessionValue>>, SessionError> {
        let Some(endpoint) = self.native_value_endpoint(value)? else {
            return Ok(None);
        };
        let NativeKind::Nominal(class) = endpoint.kind else {
            return Ok(None);
        };
        let _values = self.native_values_scope();
        let fields = endpoint.with_source(self, |source, original| {
            let fields = source.data_fields(original)?;
            if fields.len() != class.fields() {
                return Err(wasmtime::Error::msg("Invalid native nominal source storage").into());
            }
            let mut values = Vec::with_capacity(fields.len());
            for field in fields {
                source.charge_host_work(1)?;
                values.push(source.export_value(&field)?);
            }
            Ok(values)
        })?;
        let mut values = Vec::with_capacity(fields.len());
        for field in fields {
            values.push(self.import_value(field)?);
        }
        Ok(Some(values))
    }

    fn native_array_elements(
        &mut self,
        endpoint: &Arc<ForeignValue>,
    ) -> Result<SessionValue, SessionError> {
        let values = endpoint.with_source(self, |source, original| {
            let length = source.live_runtime_value(
                "packed-array-length",
                original,
                None,
                &[],
                false,
                false,
            )?;
            let length = source.native_array_integer(&length)?;
            source.charge_host_work(u64::from(length) + 1)?;
            let mut values = Vec::with_capacity(length as usize);
            for index in 0..length {
                let index = source.data_scalar(&Literal::Long(i64::from(index)))?;
                let value = source.live_runtime_value(
                    "packed-array-get",
                    original,
                    None,
                    &[index],
                    false,
                    false,
                )?;
                values.push(source.export_value(&value)?);
            }
            Ok(values)
        })?;
        self.charge_host_work(values.len() as u64 + 1)?;
        let mut imported = Vec::with_capacity(values.len());
        for value in values {
            imported.push(self.import_value(value)?);
        }
        let mut roots = RootScope::new(&mut self.store);
        let mut result = [Val::null_any_ref()];
        self.runtime
            .get_func(&mut roots, "args-new")
            .unwrap()
            .call(&mut roots, &[Val::I32(imported.len() as i32)], &mut result)?;
        let array = result[0]
            .unwrap_anyref()
            .unwrap()
            .as_array(&roots)?
            .unwrap();
        for (i, value) in imported.iter().enumerate() {
            let value = value.val(&mut roots.as_context_mut());
            array.set(&mut roots, i as u32, value)?;
        }
        own(&mut roots, result[0], self.identity, &self.handles)
    }
    fn native_array_integer(&mut self, value: &SessionValue) -> Result<u32, SessionError> {
        match self.inspect(value, |mut store, value| runtime_number(&mut store, &value))? {
            Some(RuntimeNumber::Long(value)) => Ok(value as u32), // Original private array.copy's i32.wrap_i64.
            _ => Err(SessionError::Trap(wasmtime::Error::new(
                wasmtime::Trap::CastFailure,
            ))),
        }
    }
    fn native_array_copy(
        &mut self,
        endpoint: &Arc<ForeignValue>,
        arguments: Val,
    ) -> Result<SessionValue, SessionError> {
        self.charge_host_work(6)?;
        let args = {
            let mut roots = RootScope::new(&mut self.store);
            let array = arguments
                .anyref()
                .flatten()
                .and_then(|r| r.as_array(&roots).ok().flatten())
                .ok_or_else(|| {
                    wasmtime::Error::msg("Invalid native array copy argument storage")
                })?;
            if array.len(&roots)? != 5 {
                return Err(wasmtime::Error::msg("Invalid native array copy arity").into());
            }
            (0..5)
                .map(|i| {
                    let value = array.get(&mut roots, i)?;
                    own(&mut roots, value, self.identity, &self.handles)
                })
                .collect::<Result<Vec<_>, SessionError>>()?
        };
        let first = self.native_value_endpoint(&args[0])?;
        let second = self.native_value_endpoint(&args[2])?;
        if ![first, second]
            .into_iter()
            .flatten()
            .any(|value| Arc::ptr_eq(&value, endpoint))
        {
            return Err(
                wasmtime::Error::msg("Array copy capability does not own an operand").into(),
            );
        }
        for array in [&args[0], &args[2]] {
            let valid =
                self.live_runtime_value("object-array-test", array, None, &[], false, false)?;
            if self.inspect(&valid, |mut store, value| sentinel_tag(&mut store, &value))?
                != Some(runtime_abi::TRUE as u32)
            {
                return Err(SessionError::Trap(wasmtime::Error::new(
                    wasmtime::Trap::CastFailure,
                )));
            }
        }
        let start = self.native_array_integer(&args[1])?;
        let target = self.native_array_integer(&args[3])?;
        let count = self.native_array_integer(&args[4])?;
        for (array, index) in [(&args[0], start), (&args[2], target)] {
            let length =
                self.live_runtime_value("packed-array-length", array, None, &[], false, false)?;
            let length = self.native_array_integer(&length)?;
            if index > length || count > length - index {
                return Err(SessionError::Trap(wasmtime::Error::new(
                    wasmtime::Trap::ArrayOutOfBounds,
                )));
            }
        }
        self.charge_host_work(u64::from(count) + 1)?;
        let mut values = Vec::with_capacity(count as usize);
        // Snapshot before writes, retaining memmove's overlap behavior even
        // when two proxies denote the same actual source array.
        for i in 0..count {
            let index = self.data_scalar(&Literal::Long(i64::from(start + i)))?;
            values.push(self.live_runtime_value(
                "packed-array-get",
                &args[0],
                None,
                &[index],
                false,
                false,
            )?);
        }
        for (i, value) in values.into_iter().enumerate() {
            let index = self.data_scalar(&Literal::Long(i64::from(target) + i as i64))?;
            self.live_runtime_value(
                "packed-array-set",
                &args[2],
                None,
                &[index, value],
                false,
                false,
            )?;
        }
        Ok(args[2].clone())
    }
}
