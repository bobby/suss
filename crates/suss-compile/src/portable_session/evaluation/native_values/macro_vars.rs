//! Publish actual source transformers in their caller's canonical Var cells.
//! Preparation admits Vars; successful source linking publishes their roots.
use super::*;
use crate::portable::resolve::Global as CellIdentity;

impl SessionHost<'_> {
    pub(crate) fn checkpoint_var_metadata(&mut self, target: &CellIdentity)
        -> Result<Option<VarMetadataCheckpoint>, SessionError> {
        self.charge_host_work((target.namespace().len() + target.name().len()) as u64 * 4 + 12)?;
        let hidden = registry_lock(&self.compiler).environment.existing_var_cell(target);
        let Some(hidden) = hidden else { return Ok(None); };
        let cell = registry_lock(&self.compiler).cells.get(&hidden).copied();
        let Some(cell) = cell else { return Ok(None); };
        if self.catalog_cell_flags(cell)? & runtime_abi::CELL_BOUND == 0 { return Ok(None); }
        let variable = self.binding_root_value(&hidden)?.object()?.clone();
        let before = self.var_metadata_root(&variable)?;
        Ok(Some(VarMetadataCheckpoint { variable, before, written: Vec::new() }))
    }

    fn var_metadata_root(&mut self, variable: &OwnedRooted<AnyRef>)
        -> Result<Option<OwnedRooted<AnyRef>>, SessionError> {
        let mut roots = RootScope::new(&mut self.store);
        let variable = variable.to_rooted(&mut roots).as_struct(&roots)?.unwrap();
        let fields = variable.field(&mut roots, 1)?.unwrap_anyref().unwrap().as_array(&roots)?.unwrap();
        if fields.len(&roots)? != 3 { return Err(wasmtime::Error::msg("Invalid canonical Var storage").into()); }
        Ok(fields.get(&mut roots, 2)?.unwrap_anyref()
            .map(|value| value.to_owned_rooted(&mut roots)).transpose()?)
    }

    // The declaration reserves recording and rollback work before invoking
    // Wasm. These native-only metadata reads also work during fuel cleanup.
    pub(crate) fn record_var_metadata_write(&mut self, checkpoint: &mut VarMetadataCheckpoint)
        -> Result<(), SessionError> {
        checkpoint.written.push(self.var_metadata_root(&checkpoint.variable)?);
        Ok(())
    }

    pub(crate) fn restore_var_metadata(&mut self, checkpoint: VarMetadataCheckpoint)
        -> Result<(), SessionError> {
        let mut roots = RootScope::new(&mut self.store);
        let variable = checkpoint.variable.to_rooted(&mut roots).as_struct(&roots)?.unwrap();
        let fields = variable.field(&mut roots, 1)?.unwrap_anyref().unwrap().as_array(&roots)?.unwrap();
        if fields.len(&roots)? != 3 { return Err(wasmtime::Error::msg("Invalid canonical Var storage").into()); }
        let current_value = fields.get(&mut roots, 2)?;
        let current = current_value.unwrap_anyref();
        for written in checkpoint.written.iter().rev() {
            let matches = match (current, written) {
                (None, None) => true,
                (Some(current), Some(written)) => {
                    let written = written.to_rooted(&mut roots);
                    wasmtime::Rooted::<AnyRef>::ref_eq(&roots, current, &written)?
                }
                _ => false,
            };
            if matches {
                let before = Val::AnyRef(checkpoint.before.as_ref().map(|value| value.to_rooted(&mut roots)));
                fields.set(&mut roots, 2, before)?;
                break;
            }
        }
        Ok(())
    }
    fn with_macro_var_caller<T>(&mut self, owner: u64,
        operation: impl FnOnce(&mut SessionHost<'_>) -> Result<T, SessionError>,
    ) -> Result<T, SessionError> {
        let allowance = self.operation_fuel_remaining()?;
        let result = super::super::super::suspended::scope(self, || {
            super::super::super::suspended::with_host(owner, |caller| {
                let before = caller.operation_fuel_remaining()?;
                let limit = caller.cap_operation_fuel(allowance)?;
                let result = operation(caller);
                let spent = limit.saturating_sub(caller.operation_fuel_remaining()?);
                caller.settle_nested_fuel(before, spent)?;
                Ok::<_, SessionError>((result, spent))
            })
        }).ok_or_else(|| wasmtime::Error::msg("Native macro Var destination is not suspended"))??;
        self.settle_nested_fuel(allowance, result.1)?;
        result.0
    }

    pub(crate) fn admit_source_macro_var(&mut self, owner: u64,
        target: &CellIdentity) -> Result<(), SessionError> {
        self.charge_host_work((target.namespace().len() + target.name().len()) as u64 * 4 + 1)?;
        self.with_macro_var_caller(owner, |caller| {
            let declaration = Form { span: 0..0, metadata: vec![],
                kind: Kind::Symbol(suss_reader::Symbol::new(target.name())) };
            caller.intern_analyzed_var(target, &declaration, None)
        })
    }

    pub(crate) fn project_source_macro_var(&mut self, owner: u64,
        target: &CellIdentity, source_link: bool) -> Result<(), SessionError> {
        self.charge_host_work((target.namespace().len() + target.name().len()) as u64 * 4 + 1)?;
        if !source_link {
            // Cache completion only: the live caller root remains authoritative.
            // A later explicit source-link publication bypasses this cache.
            let complete = super::super::super::suspended::scope(self, || {
                super::super::super::suspended::with_host(owner, |caller| {
                    let state = caller.value_state()?;
                    let complete = registry_lock(&state.projected_macros).contains(target);
                    Ok::<_, SessionError>(complete)
                })
            }).ok_or_else(|| wasmtime::Error::msg("Native macro Var destination is not suspended"))??;
            if complete { return Ok(()); }
        }
        let global = registry_lock(&self.compiler).environment.resolve(self.phase,
            &suss_reader::Symbol::namespaced(target.namespace(), target.name()), 0..0)
            .map_err(SessionError::Compile)?.global().clone();
        // Privileged bootstrap definitions need not have requested public Vars.
        let hidden = registry_lock(&self.compiler).environment.existing_var_cell(&global);
        let installed = hidden.as_ref().and_then(|hidden|
            registry_lock(&self.compiler).cells.get(hidden).copied());
        if installed.is_none() || self.catalog_cell_flags(installed.unwrap())? & runtime_abi::CELL_BOUND == 0 {
            let declaration = Form { span: 0..0, metadata: vec![],
                kind: Kind::Symbol(suss_reader::Symbol::new(global.name())) };
            self.intern_analyzed_var(&global, &declaration, None)?;
        }
        let hidden = registry_lock(&self.compiler).environment.existing_var_cell(&global)
            .ok_or_else(|| wasmtime::Error::msg("Source macro has no actual interned Var"))?;
        let variable = self.binding_root_value(&hidden)?;
        let root = self.binding_root_value(&global)?;
        let cell = registry_lock(&self.compiler).cells[&global];
        let source_flags = self.catalog_cell_flags(cell)?;
        let meta = self.original_core_function("meta")?;
        let metadata = self.invoke(&meta, &[&variable])?;
        let bridge = self.value_state()?.bridge(self)?;
        let key = |name| Form { span: 0..0, metadata: vec![], kind: Kind::Keyword(suss_reader::Keyword::new(name)) };
        let name = bridge.quote(self, key("name"))?;
        let namespace = bridge.quote(self, key("ns"))?;
        let dissoc = self.original_core_function("dissoc")?;
        let metadata = self.invoke(&dissoc, &[&metadata, &name, &namespace])?;
        let function = self.export_value(&root)?;
        let metadata = self.export_value(&metadata)?;
        self.with_macro_var_caller(owner, |caller| {
            let _values = caller.native_values_scope();
            let function = caller.import_value(function)?;
            let metadata = caller.import_value(metadata)?;
            caller.install_projected_macro_var(target, &function, &metadata, source_flags, source_link)
        })
    }

    fn install_projected_macro_var(&mut self, target: &CellIdentity,
        function: &SessionValue, metadata: &SessionValue, source_flags: i32,
        source_link: bool) -> Result<(), SessionError> {
        self.check(function)?; self.check(metadata)?;
        if target.phase() != self.phase { return Err(SessionError::ForeignValue); }
        let state = self.value_state()?;
        if !source_link && registry_lock(&state.projected_macros).contains(target) { return Ok(()); }
        let mut environment = registry_lock(&self.compiler).environment.clone();
        environment.declare_cell(self.phase, target.namespace(), target.name()).map_err(SessionError::Compile)?;
        let hidden_identity = environment.var_cell(target).map_err(SessionError::Compile)?;
        // Enumeration cannot replace an existing caller-owned root. Explicit
        // source linking has already checked that no newer native owner won.
        let known = registry_lock(&self.compiler).cells.get(target).copied();
        if !source_link {
            if let Some(cell) = known {
                if self.catalog_cell_flags(cell)? & runtime_abi::CELL_BOUND != 0 {
                    registry_lock(&state.projected_macros).insert(target.clone());
                    return Ok(());
                }
            }
        }
        environment.record_native_macro_definition(self.phase, target.namespace(), target.name(), self.identity);
        let namespace = self.catalog_namespace(target.namespace())?;
        self.install(&[], environment, &[target.clone(), hidden_identity.clone()])?;
        let cell = registry_lock(&self.compiler).cells[target];
        let hidden = registry_lock(&self.compiler).cells[&hidden_identity];
        let bridge = state.bridge(self)?;
        let name = bridge.quote(self, Form { span: 0..0, metadata: vec![],
            kind: Kind::Symbol(suss_reader::Symbol::new(target.name())) })?;
        let key = |name| Form { span: 0..0, metadata: vec![], kind: Kind::Keyword(suss_reader::Keyword::new(name)) };
        let name_key = bridge.quote(self, key("name"))?;
        let namespace_key = bridge.quote(self, key("ns"))?;
        let assoc = self.original_core_function("assoc")?;
        let metadata = self.invoke(&assoc, &[metadata, &name_key, &name, &namespace_key, &namespace])?;
        let existing = self.catalog_cell_flags(hidden)? & runtime_abi::CELL_BOUND != 0;
        let variable = if existing {
            self.binding_root_value(&hidden_identity)?
        } else {
            let getter = {
                let mut roots = RootScope::new(&mut self.store);
                let get = self.runtime.get_func(&mut roots, "binding-getter").unwrap();
                let input = cell.get(&mut roots);
                call(&mut roots, self.runtime, get, &[input], self.identity, &self.handles)?
            };
            let public_namespace = if target.namespace() == "suss.core" { "clojure.core" } else { target.namespace() };
            let symbol = bridge.quote(self, Form { span: 0..0, metadata: vec![],
                kind: Kind::Symbol(suss_reader::Symbol::namespaced(public_namespace, target.name())) })?;
            let constructor = self.original_core_function("Var")?;
            self.data_construct(&constructor, &[&getter, &symbol, &metadata])?
        };
        let unbound = if existing { None } else {
            let constructor = self.original_core_function("VarUnbound")?;
            Some(self.data_construct(&constructor, &[&variable])?)
        };
        // All guest construction and fallible transport precede publication.
        // Keep the canonical Var and its Unbound value across source reloads.
        self.charge_host_work(12)?;
        let mut scope = RootScope::new(&mut self.store);
        let root = function.val(&mut scope.as_context_mut());
        let variable_value = variable.val(&mut scope.as_context_mut());
        let variable = variable_value.unwrap_anyref().unwrap().as_struct(&scope)?.unwrap();
        let fields = variable.field(&mut scope, 1)?.unwrap_anyref().unwrap().as_array(&scope)?.unwrap();
        if fields.len(&scope)? != 3 { return Err(wasmtime::Error::msg("Invalid canonical Var storage").into()); }
        let metadata = metadata.val(&mut scope.as_context_mut());
        let cell = cell.get(&mut scope).unwrap_anyref().unwrap().as_struct(&scope)?.unwrap();
        let flags = cell.field(&mut scope, 1)?.unwrap_i32();
        let hidden = hidden.get(&mut scope).unwrap_anyref().unwrap().as_struct(&scope)?.unwrap();
        fields.set(&mut scope, 2, metadata)?;
        cell.set_field(&mut scope, 0, root)?;
        cell.set_field(&mut scope, 1, Val::I32((flags & !(runtime_abi::CELL_MACRO | runtime_abi::CELL_BOUND))
            | runtime_abi::CELL_DECLARED
            | (source_flags & (runtime_abi::CELL_MACRO | runtime_abi::CELL_BOUND | runtime_abi::CELL_DYNAMIC))))?;
        if let Some(unbound) = unbound {
            let unbound = unbound.val(&mut scope.as_context_mut());
            cell.set_field(&mut scope, 2, unbound)?;
        }
        hidden.set_field(&mut scope, 0, variable_value)?;
        hidden.set_field(&mut scope, 1, Val::I32(runtime_abi::CELL_BOUND))?;
        drop(scope);
        registry_lock(&state.projected_macros).insert(target.clone());
        Ok(())
    }
}
