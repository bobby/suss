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
                let profile = registry_lock(&state.profile)
                    .clone()
                    .ok_or_else(|| wasmtime::Error::msg("No active native value profile"))?;
                let mut view = profile.bind(caller.as_context_mut());
                let _values = view.native_values_scope();
                let code = args[1].unwrap_i32();
                if matches!(code, op::LOCAL_FIELD_SET | op::VAR_METADATA_DEFINE) {
                    let defining = code == op::VAR_METADATA_DEFINE;
                    let index = args[2].unwrap_i32();
                    let result = if defining || index == 2 {
                        view.local_var_metadata_write(args[0], args[3], defining.then_some(args[4]), defining && index != 0)
                    } else { Ok(false) };
                    return match result {
                        Ok(handled) => {
                            results[0] = if handled { args[0] } else { Val::null_any_ref() };
                            Ok(())
                        }
                        Err(SessionError::Trap(error)) => Err(error),
                        Err(error) => view.throw_evaluation_error(error),
                    };
                }
                let result = if args[1].unwrap_i32() == op::NAMESPACE_INTERNS {
                    let namespace = {
                        let mut roots = RootScope::new(&mut view.store);
                        own(&mut roots, args[0], view.identity, &view.handles)?
                    };
                    view.namespace_interns(&namespace)
                } else {
                    let endpoint = capability(view.store.as_context_mut(), args[0])?
                        .ok_or_else(|| wasmtime::Error::msg("Invalid native operation capability"))?;
                    view.native_operation(&endpoint, args[1].unwrap_i32(), args[2].unwrap_i32(), args[3], args[4])
                };
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

    /// Handle only the actual canonical Var, authenticated by its original
    /// descriptor and registered hidden-cell root plus actual binding cell. The
    /// operation writes metadata and its ownership bit together, after all
    /// fallible guest work. A cloned/forged Var cannot alter another journal.
    fn local_var_metadata_write(&mut self, receiver: Val, metadata: Val,
        macro_flag: Option<Val>, staged: bool) -> Result<bool, SessionError> {
        self.charge_host_work(1)?;
        let state = self.value_state()?;
        let descriptor = registry_lock(&state.var_class).clone()
            .ok_or_else(|| wasmtime::Error::msg("Missing original Var descriptor"))?;
        let (variable, metadata, getter, symbol, macro_on) = {
            let mut roots = RootScope::new(&mut self.store);
            let Val::AnyRef(Some(reference)) = receiver else { return Ok(false); };
            let Some(object) = reference.as_struct(&roots)? else { return Ok(false); };
            if object.ty(&roots)?.fields().len() != 4 { return Ok(false); }
            let Val::AnyRef(Some(actual_descriptor)) = object.field(&mut roots, 0)? else { return Ok(false); };
            let expected = descriptor.to_rooted(&mut roots);
            if !wasmtime::Rooted::<AnyRef>::ref_eq(&roots, &actual_descriptor, &expected)? { return Ok(false); }
            let Val::AnyRef(Some(fields)) = object.field(&mut roots, 1)? else { return Ok(false); };
            let Some(fields) = fields.as_array(&roots)? else { return Ok(false); };
            if fields.len(&roots)? != 3 { return Ok(false); }
            let getter = fields.get(&mut roots, 0)?;
            let symbol = fields.get(&mut roots, 1)?;
            let macro_on = macro_flag.map(|value| sentinel_tag(&mut roots.as_context_mut(), &value)
                .map(|tag| tag == Some(runtime_abi::TRUE as u32))).transpose()?;
            (own(&mut roots, receiver, self.identity, &self.handles)?,
             own(&mut roots, metadata, self.identity, &self.handles)?,
             own(&mut roots, getter, self.identity, &self.handles)?,
             own(&mut roots, symbol, self.identity, &self.handles)?, macro_on)
        };
        self.charge_host_work(15)?;
        let cell = self.live_runtime_value("binding-getter-cell", &getter, None, &[], false, false)?;
        {
            let mut roots = RootScope::new(&mut self.store);
            let Val::AnyRef(Some(reference)) = cell.val(&mut roots.as_context_mut()) else { return Ok(false); };
            let Some(binding) = reference.as_struct(&roots)? else { return Ok(false); };
            if binding.ty(&roots)?.fields().len() != 3 { return Ok(false); }
        }
        let bridge = state.bridge(self)?;
        let Some((namespace, name)) = bridge.qualified_symbol_candidate(self, &symbol)? else { return Ok(false); };
        self.charge_host_work(((namespace.len() + name.len()) as u64).saturating_mul(8).saturating_add(16))?;
        // The public core spelling can also be an explicitly declared
        // namespace. Both are only candidates; exact registered roots decide.
        let namespaces = if namespace == "clojure.core" { vec![namespace.as_str(), "suss.core"] } else { vec![namespace.as_str()] };
        let candidates = {
            let compiler = registry_lock(&self.compiler);
            namespaces.into_iter().filter_map(|namespace| {
                let (target, hidden) = compiler.environment.registered_var_cells(self.phase, namespace, &name)?;
                Some((*compiler.cells.get(&target)?, *compiler.cells.get(&hidden)?))
            }).collect::<Vec<_>>()
        };
        let registered = {
            let mut roots = RootScope::new(&mut self.store);
            let actual = variable.object()?.to_rooted(&mut roots);
            let mut registered = None;
            for (target, hidden) in candidates {
                let holder = hidden.get(&mut roots).unwrap_anyref().unwrap().as_struct(&roots)?.unwrap();
                if holder.field(&mut roots, 1)?.unwrap_i32() & runtime_abi::CELL_BOUND == 0 { continue; }
                let Val::AnyRef(Some(canonical)) = holder.field(&mut roots, 0)? else { continue; };
                if wasmtime::Rooted::<AnyRef>::ref_eq(&roots, &canonical, &actual)? { registered = Some(target); break; }
            }
            registered
        };
        let Some(registered) = registered else { return Ok(false); };
        {
            let mut roots = RootScope::new(&mut self.store);
            let Val::AnyRef(Some(reference)) = cell.val(&mut roots.as_context_mut()) else { return Ok(false); };
            let Val::AnyRef(Some(actual)) = registered.get(&mut roots) else { return Ok(false); };
            if !wasmtime::Rooted::<AnyRef>::ref_eq(&roots, &reference, &actual)? { return Ok(false); }
        }
        if staged {
            let authority = self.owned_macro_authority()
                .ok_or_else(|| wasmtime::Error::msg("Staged Var metadata requires its source Macro authority"))?;
            authority.bind(self.reborrow()).preserve_runtime_metadata_before_staging(&variable)?;
        }
        // The receiver/cell/field shapes and values are now checked. No guest
        // execution or allocation separates the actual write and its marker.
        let mut roots = RootScope::new(&mut self.store);
        let binding = cell.object()?.to_rooted(&mut roots).as_struct(&roots)?.unwrap();
        let object = variable.object()?.to_rooted(&mut roots).as_struct(&roots)?.unwrap();
        let fields = object.field(&mut roots, 1)?.unwrap_anyref().unwrap().as_array(&roots)?.unwrap();
        let flags = binding.field(&mut roots, 1)?.unwrap_i32();
        let mut flags = flags & !runtime_abi::CELL_STAGED_METADATA;
        if staged { flags |= runtime_abi::CELL_STAGED_METADATA; }
        if let Some(macro_on) = macro_on {
            flags = (flags & !runtime_abi::CELL_MACRO) | if macro_on { runtime_abi::CELL_MACRO } else { 0 };
        }
        let value = metadata.val(&mut roots.as_context_mut());
        fields.set(&mut roots, 2, value)?;
        binding.set_field(&mut roots, 1, Val::I32(flags))?;
        Ok(true)
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
            (NativeKind::Function { .. }, op::GETTER_ROOT_GET) => {
                ("binding-getter-root-get", 0, false, false, false)
            }
            (NativeKind::Function { .. }, op::GETTER_ROOT_SET) => {
                ("binding-getter-root-set!", 2, false, false, false)
            }
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

#[cfg(all(test, not(suss_generator)))]
mod tests {
    use super::*;

    #[test]
    fn native_metadata_hook_changes_only_the_actual_canonical_var() {
        let mut failures = Vec::new();
        for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
            let variable = session.eval("(def metadata-hook-target 42)").unwrap();
            let original_fields = session.data_fields(&variable).unwrap();
            let original_metadata = original_fields[2].clone();
            let original_class = session.original_core_function("Var").unwrap();
            let unrelated_class = session.eval("(deftype MetadataLookalike [val sym ^:mutable _meta]) MetadataLookalike").unwrap();
            let target = registry_lock(&session.compiler).environment.resolve(session.phase,
                &suss_reader::Symbol::new("metadata-hook-target"), 0..0).unwrap().global().clone();
            let replacement = session.eval("{:replacement true}").unwrap();
            for class in [&original_class, &unrelated_class] {
                // A genuine Var-class clone even carries the real getter. It
                // is not the canonical Var named by that cell's Unbound root.
                let fields = original_fields.iter().collect::<Vec<_>>();
                let forged = session.data_construct(class, &fields).unwrap();
                session.mark_metadata_staged(&target, true).unwrap();
                session.collect().unwrap();
                session.live_runtime_value("object-field-set", &forged, Some(2), &[replacement.clone()], false, true).unwrap();
                let cell = registry_lock(&session.compiler).cells[&target];
                assert_ne!(session.catalog_cell_flags(cell).unwrap() & runtime_abi::CELL_STAGED_METADATA, 0);
                let actual = session.data_fields(&variable).unwrap();
                assert!(session.identical_values(&original_metadata, &actual[2]).unwrap());
                let updated = session.data_fields(&forged).unwrap();
                assert!(session.identical_values(&replacement, &updated[2]).unwrap());
            }
            session.live_runtime_value("object-field-set", &variable, Some(2), &[replacement.clone()], false, true).unwrap();
            let cell = registry_lock(&session.compiler).cells[&target];
            assert_eq!(session.catalog_cell_flags(cell).unwrap() & runtime_abi::CELL_STAGED_METADATA, 0);
            let actual = session.data_fields(&variable).unwrap();
            assert!(session.identical_values(&replacement, &actual[2]).unwrap());
            // Clojure permits a Var to hold another Var's genuine Unbound
            // root. Its back-reference changes; the canonical target Var does
            // not. Metadata authority must survive this valid root operation.
            assert_eq!(include_str!("../../../../../../results/native-host-foundation/unbound-replacement-primary/primary.stdout"), "[true false]\n");
            session.eval("(def metadata-hook-other-unbound) (alter-var-root #'metadata-hook-target (fn [_] @#'metadata-hook-other-unbound))").unwrap();
            let identity = session.eval("(identical? @#'metadata-hook-target @#'metadata-hook-other-unbound)").unwrap();
            assert!(session.inspect(&identity, |mut store, value| Ok(sentinel_tag(&mut store, &value)? == Some(runtime_abi::TRUE as u32))).unwrap());
            let cell = registry_lock(&session.compiler).cells[&target];
            assert_eq!(session.catalog_cell_flags(cell).unwrap() & runtime_abi::CELL_BOUND, 0);
            session.mark_metadata_staged(&target, true).unwrap();
            session.live_runtime_value("object-field-set", &variable, Some(2), &[replacement.clone()], false, true).unwrap();
            if session.catalog_cell_flags(cell).unwrap() & runtime_abi::CELL_STAGED_METADATA != 0 {
                failures.push(format!("{:?}: canonical metadata write lost after replacing the Unbound root", session.phase()));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
