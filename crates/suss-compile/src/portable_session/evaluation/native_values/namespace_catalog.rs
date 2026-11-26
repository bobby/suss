//! Native namespace catalogs contain the session's real live binding-cell Vars.
//! Core materialization is lazy; no source initializer is reconstructed/replayed.
use super::*;

impl SessionHost<'_> {
    pub(crate) fn intern_analyzed_var(&mut self, identity: &CellIdentity,
        _declaration: &Form, _doc: Option<&[u16]>) -> Result<(), SessionError> {
        if identity.phase() != self.phase {
            return Err(wasmtime::Error::msg("Native Var admission has a foreign phase").into());
        }
        // A host-selected compilation namespace need not have executed a
        // source namespace frame yet. Intern its real object before Var metadata.
        let namespace = self.catalog_namespace(identity.namespace())?;
        let mut environment = registry_lock(&self.compiler).environment.clone();
        environment.declare_cell(self.phase, identity.namespace(), identity.name()).map_err(SessionError::Compile)?;
        let hidden = environment.var_cell(identity).map_err(SessionError::Compile)?;
        self.install(&[], environment, &[identity.clone(), hidden])?;
        let cell = registry_lock(&self.compiler).cells[identity];
        {
            let mut roots = RootScope::new(&mut self.store);
            let declare = self.runtime.get_func(&mut roots, "binding-declare").unwrap();
            let cell = cell.get(&mut roots);
            declare.call(&mut roots, &[cell], &mut [Val::null_any_ref()])?;
        }
        // Clojure admits the Var during analysis but publishes new declaration
        // metadata only after successful initialization. In particular, failed
        // compilation/initialization must not install new native metadata roots.
        self.intern_catalog_var(identity, Some(&namespace))?;
        Ok(())
    }

    pub(super) fn catalog_namespace(&mut self, name: &str) -> Result<SessionValue, SessionError> {
        self.charge_host_work(name.len() as u64 + 1)?;
        let bridge = self.value_state()?.bridge(self)?;
        let name = bridge.quote(self, Form { span: 0..0, metadata: vec![],
            kind: Kind::Symbol(suss_reader::Symbol::new(name)) })?;
        let intern = self.original_core_function("intern-namespace")?;
        self.invoke(&intern, &[&name])
    }

    pub(super) fn namespace_interns(&mut self, namespace: &SessionValue) -> Result<SessionValue, SessionError> {
        self.check(namespace)?;
        if let Some(endpoint) = self.native_value_endpoint(namespace)? {
            if !matches!(endpoint.kind, NativeKind::Nominal(LiveClass::Namespace)) {
                return Err(wasmtime::Error::msg("Namespace catalog requires an actual Namespace").into());
            }
            let catalog = endpoint.with_source(self, |source, namespace| {
                let _values = source.native_values_scope();
                let catalog = source.local_namespace_interns(namespace)?;
                source.export_value(&catalog)
            })?;
            return self.import_value(catalog);
        }
        self.local_namespace_interns(namespace)
    }

    pub(super) fn catalog_cell_flags(&mut self, cell: Global) -> Result<i32, SessionError> {
        self.charge_host_work(1)?;
        let mut roots = RootScope::new(&mut self.store);
        let cell = cell.get(&mut roots).unwrap_anyref().unwrap().as_struct(&roots)?.unwrap();
        Ok(cell.field(&mut roots, 1)?.unwrap_i32())
    }

    fn local_namespace_interns(&mut self, namespace: &SessionValue) -> Result<SessionValue, SessionError> {
        let state = self.value_state()?;
        let access = registry_lock(&state.namespace).as_ref().cloned()
            .ok_or_else(|| wasmtime::Error::msg("Missing native Namespace reader authority"))?;
        let name = access.value_name(self, namespace)?;
        let canonical = self.catalog_namespace(&name)?;
        if !self.identical_values(namespace, &canonical)? {
            return Err(wasmtime::Error::msg("Namespace catalog requires a registered canonical Namespace").into());
        }
        // A catalog must enumerate source macro exports even when no Var
        // literal has previously asked the analyzer to materialize one.
        if self.owned_macro_authority().is_none() {
            let service = registry_lock(&state.external).clone();
            if let Some(service) = service {
                let allowance = self.operation_fuel_remaining()?;
                let interrupt = self.host_work_interrupt();
                let owner = self.identity;
                let phase = self.phase;
                let (result, spent) = super::super::super::suspended::scope(self, ||
                    service.project_namespace_macros(allowance, interrupt, owner, phase, &name))?;
                self.settle_nested_fuel(allowance, spent)?;
                result?;
            }
        }
        let trace = std::env::var_os("SUSS_NAMESPACE_FUEL_TRACE").is_some();
        if trace { eprintln!("namespace-catalog start {name} fuel={}", self.operation_fuel_remaining()?); }
        let mut environment = registry_lock(&self.compiler).environment.clone();
        let mut identities = Vec::new();
        for identity in environment.declared_globals(self.phase, &name) {
            self.charge_host_work((identity.namespace().len() + identity.name().len()) as u64 + 1)?;
            let cell = registry_lock(&self.compiler).cells.get(identity).copied();
            if let Some(cell) = cell {
                if self.catalog_cell_flags(cell)? & (runtime_abi::CELL_BOUND | runtime_abi::CELL_DECLARED) != 0 {
                    identities.push(identity.clone());
                }
            }
        }
        // Stage missing hidden cells together. Cloning/publishing a linker for
        // every core Var would make a first catalog need quadratic host work.
        let known = registry_lock(&self.compiler).cells.clone();
        let mut missing = Vec::new();
        let mut annotations = false;
        for identity in &identities {
            self.charge_host_work((identity.namespace().len() + identity.name().len()) as u64 * 4 + 1)?;
            annotations |= environment.existing_var_cell(identity).is_none();
            let hidden = environment.var_cell(identity).map_err(SessionError::Compile)?;
            if !known.contains_key(&hidden) { missing.push(hidden); }
        }
        drop(known);
        if annotations || !missing.is_empty() { self.install(&[], environment, &missing)?; }
        if trace { eprintln!("namespace-catalog staged count={} fuel={}", identities.len(), self.operation_fuel_remaining()?); }
        let bridge = state.bridge(self)?;
        let mut entries = Vec::new();
        for identity in identities {
            if trace { eprintln!("namespace-catalog before-var {} fuel={}", identity.name(), self.operation_fuel_remaining()?); }
            let (value, key) = self.intern_catalog_var(&identity, Some(namespace))?;
            if trace { eprintln!("namespace-catalog after-var {} fuel={}", identity.name(), self.operation_fuel_remaining()?); }
            let key = match key {
                Some(key) => key,
                None => bridge.quote(self, Form { span: 0..0, metadata: vec![], kind: Kind::Symbol(suss_reader::Symbol::new(identity.name())) })?,
            };
            entries.push((key, value));
        }
        if trace { eprintln!("namespace-catalog before-map count={} fuel={}", entries.len(), self.operation_fuel_remaining()?); }
        let result = bridge.map_values(self, &entries);
        if trace { eprintln!("namespace-catalog after-map ok={} fuel={}", result.is_ok(), self.operation_fuel_remaining()?); }
        result
    }

    fn catalog_key(&mut self, identity: &CellIdentity) -> Result<SessionValue, SessionError> {
        self.charge_host_work((identity.namespace().len() + identity.name().len()) as u64 + 1)?;
        let state = self.value_state()?;
        let existing = registry_lock(&state.catalog_keys).get(identity).cloned();
        if let Some(key) = existing { return self.root_value(&key); }
        // One immutable symbol per actual interned cell, rooted with its Var.
        // Do not trust mutable Var metadata for catalog names. Reusing this
        // canonical key also preserves its ordinary source hash cache.
        let bridge = state.bridge(self)?;
        let key = bridge.quote(self, Form { span: 0..0, metadata: vec![],
            kind: Kind::Symbol(suss_reader::Symbol::new(identity.name())) })?;
        registry_lock(&state.catalog_keys).insert(identity.clone(), key.object()?.clone());
        Ok(key)
    }

    fn intern_catalog_var(&mut self, identity: &CellIdentity,
        namespace: Option<&SessionValue>) -> Result<(SessionValue, Option<SessionValue>), SessionError> {
        let environment = registry_lock(&self.compiler).environment.clone();
        let hidden = environment.existing_var_cell(identity)
            .ok_or_else(|| wasmtime::Error::msg("Catalog Var identity was not staged"))?;
        let existing = registry_lock(&self.compiler).cells.get(&hidden).copied()
            .ok_or_else(|| wasmtime::Error::msg("Catalog Var cell was not staged"))?;
        if self.catalog_cell_flags(existing)? & runtime_abi::CELL_BOUND != 0 {
            let key = self.catalog_key(identity)?;
            return Ok((self.binding_root_value(&hidden)?, Some(key)));
        }
        let cell = registry_lock(&self.compiler).cells.get(identity).copied()
            .ok_or_else(|| wasmtime::Error::msg("Missing actual catalog binding cell"))?;
        let flags = self.catalog_cell_flags(cell)?;
        let getter = {
            let mut roots = RootScope::new(&mut self.store);
            let value = cell.get(&mut roots);
            let get = self.runtime.get_func(&mut roots, "binding-getter").unwrap();
            call(&mut roots, self.runtime, get, &[value], self.identity, &self.handles)?
        };
        let bridge = self.value_state()?.bridge(self)?;
        let declaration = if flags & runtime_abi::CELL_BOUND != 0 {
            environment.definition_info(identity).map(|info| (&info.declaration, info.docstring.as_deref()))
        } else { None };
        let metadata = if let Some((declaration, doc)) = declaration {
            let mut pairs = crate::portable_macro_data::normalized_reader_metadata(self, declaration)?;
            if let Some(doc) = doc {
                self.charge_host_work(doc.len() as u64 + 1)?;
                pairs.push(Form { span: 0..0, metadata: vec![], kind: Kind::Keyword(suss_reader::Keyword::new("doc")) });
                pairs.push(Form { span: 0..0, metadata: vec![], kind: Kind::String(doc.to_vec()) });
            }
            pairs
        } else { vec![] };
        // Construction must preserve the live flag, including earlier changes
        // made before the first Var object was requested.
        let mut pairs = metadata.into_iter();
        let mut metadata = Vec::new();
        while let Some(key) = pairs.next() {
            let value = pairs.next().ok_or_else(|| wasmtime::Error::msg("Unpaired catalog Var metadata"))?;
            let macro_key = matches!(&key.kind, Kind::Keyword(k) if k.namespace.is_none() && k.name == "macro");
            if macro_key && !matches!(value.kind, Kind::Nil | Kind::Bool(false)) { continue; }
            metadata.extend([key, value]);
        }
        if flags & runtime_abi::CELL_MACRO != 0 {
            metadata.push(Form { span: 0..0, metadata: vec![], kind: Kind::Keyword(suss_reader::Keyword::new("macro")) });
            metadata.push(Form { span: 0..0, metadata: vec![], kind: Kind::Bool(true) });
        }
        let mut entries = Vec::new();
        for pair in metadata.chunks_exact(2) {
            let key = bridge.quote_ref(self, &pair[0])?;
            let value = bridge.quote_ref(self, &pair[1])?;
            entries.push((key, value));
        }
        let namespace = match namespace { Some(namespace) => namespace.clone(), None => self.catalog_namespace(identity.namespace())? };
        let name = self.catalog_key(identity)?;
        for (key, value) in [("name", name.clone()), ("ns", namespace)] {
            let key = bridge.quote(self, Form { span: 0..0, metadata: vec![], kind: Kind::Keyword(suss_reader::Keyword::new(key)) })?;
            entries.push((key, value));
        }
        let metadata = bridge.map_values(self, &entries)?;
        let public_namespace = if identity.namespace() == "suss.core" { "clojure.core" } else { identity.namespace() };
        let symbol = bridge.quote(self, Form { span: 0..0, metadata: vec![],
            kind: Kind::Symbol(suss_reader::Symbol::namespaced(public_namespace, identity.name())) })?;
        // The same original nominal class and three fields as make-binding-var,
        // built directly with canonical data factories. No public symbol/assoc
        // calls, original initializer, or extra compiled function body is run.
        let constructor = self.original_core_function("Var")?;
        let variable = self.data_construct(&constructor, &[&getter, &symbol, &metadata])?;
        // Intern the actual Var-owned Unbound object separately from its root.
        // Temporary binding/redefs restoration must preserve this identity.
        let constructor = self.original_core_function("VarUnbound")?;
        let unbound = self.data_construct(&constructor, &[&variable])?;
        {
            let mut roots = RootScope::new(&mut self.store);
            let cell = cell.get(&mut roots).unwrap_anyref().unwrap().as_struct(&roots)?.unwrap();
            let unbound = unbound.val(&mut roots.as_context_mut());
            cell.set_field(&mut roots, 2, unbound)?;
        }
        let target = registry_lock(&self.compiler).cells[&hidden];
        let mut roots = RootScope::new(&mut self.store);
        let set = self.runtime.get_func(&mut roots, "binding-root-set").unwrap();
        let args = [target.get(&mut roots), variable.val(&mut roots.as_context_mut())];
        call(&mut roots, self.runtime, set, &args, self.identity, &self.handles)?;
        // Freshly constructed names are canonical and can also be the catalog
        // key. Existing user metadata is never trusted to choose an intern name.
        Ok((variable, Some(name)))
    }
}
