//! Authenticated native value transport. GC capabilities retain real source
//! roots; one shared invocation callback per destination never owns those roots.
//! The source Store is borrowed only synchronously or leased by its own service.
use super::*;
use crate::portable_macros::MacroServiceWeak;
use std::sync::Weak;
use suss_reader::forms::{Form, Kind};
use wasmtime::{AsContext, ExternRef, FieldType, StorageType, StructRef, StructRefPre, StructType};

#[derive(Default)]
pub(super) struct Values {
    exports: Mutex<Vec<Weak<ForeignValue>>>,
    imports: Mutex<Vec<Imported>>,
    depth: AtomicUsize,
    invoke: Mutex<Option<Func>>,
    capability: Mutex<Option<StructType>>,
}
struct Imported {
    source: Arc<ForeignValue>,
    local: OwnedRooted<AnyRef>,
}
struct ForeignValue {
    owner: u64,
    state: Weak<Evaluation>,
    service: Option<MacroService>,
    root: OwnedRooted<AnyRef>,
    // A hash is never authority for identity. Copies use the actual source UID
    // so authenticated equal handles also satisfy the hash/equality contract.
    uid: i64,
    kind: NativeKind,
}
enum NativeKind {
    Function { metadata: Option<Transfer> },
    Materialized(Form),
}
pub(crate) struct ValuesScope(Arc<Evaluation>);
impl Drop for ValuesScope {
    fn drop(&mut self) {
        if self.0.values.depth.fetch_sub(1, Ordering::SeqCst) == 1 {
            let old = std::mem::take(&mut *registry_lock(&self.0.values.imports));
            drop(old);
        }
    }
}
impl Values {
    pub(super) fn clear(&self) {
        let imports = std::mem::take(&mut *registry_lock(&self.imports));
        let exports = std::mem::take(&mut *registry_lock(&self.exports));
        drop((imports, exports));
        registry_lock(&self.invoke).take();
        registry_lock(&self.capability).take();
    }
}
enum Transfer {
    Data(Form),
    Native(Arc<ForeignValue>),
}
fn capability(
    mut store: StoreContextMut<'_, ()>,
    value: Val,
) -> wasmtime::Result<Option<Arc<ForeignValue>>> {
    let Some(reference) = value.anyref().flatten() else {
        return Ok(None);
    };
    let Some(object) = reference.as_struct(&store)? else {
        return Ok(None);
    };
    if object.ty(&store)?.fields().len() != 1 {
        return Ok(None);
    }
    let Val::ExternRef(Some(value)) = object.field(&mut store, 0)? else {
        return Ok(None);
    };
    Ok(value
        .data(store.as_context())?
        .and_then(|value| value.downcast_ref::<Arc<ForeignValue>>())
        .cloned())
}
impl SessionHost<'_> {
    pub(super) fn install_native_identity(
        &mut self,
        state: &Arc<Evaluation>,
    ) -> Result<(), SessionError> {
        let signature = self
            .runtime
            .get_func(&mut self.store, "native-identical-test")
            .unwrap()
            .ty(&self.store);
        let state_ref = Arc::downgrade(state);
        let callback = Func::try_new(
            &mut self.store,
            signature,
            move |mut caller, arguments, results| {
                let state = state_ref
                    .upgrade()
                    .filter(|state| !state.retired.load(Ordering::SeqCst))
                    .ok_or_else(|| wasmtime::Error::msg("Native identity owner has retired"))?;
                let profile = registry_lock(&state.profile)
                    .clone()
                    .ok_or_else(|| wasmtime::Error::msg("No active native identity profile"))?;
                let mut view = profile.bind(caller.as_context_mut());
                view.charge_host_work(1).map_err(|error| match error {
                    SessionError::Trap(error) | SessionError::Host(error) => error,
                    error => wasmtime::Error::msg(error.to_string()),
                })?;
                let first = capability(view.store.as_context_mut(), arguments[0])?;
                let second = capability(view.store.as_context_mut(), arguments[1])?;
                results[0] = Val::I32(i32::from(
                    first.zip(second).is_some_and(|(a, b)| Arc::ptr_eq(&a, &b)),
                ));
                Ok(())
            },
        )?;
        self.runtime
            .get_func(&mut self.store, "native-identity-install")
            .unwrap()
            .call(&mut self.store, &[Val::FuncRef(Some(callback))], &mut [])?;
        Ok(())
    }
    pub(crate) fn native_values_scope(&self) -> Option<ValuesScope> {
        let state = self.evaluation.as_ref()?.clone();
        state.values.depth.fetch_add(1, Ordering::SeqCst);
        Some(ValuesScope(state))
    }
    pub(crate) fn attach_native_value_service(&self, service: MacroServiceWeak) {
        if let Some(state) = &self.evaluation {
            *registry_lock(&state.value_service) = Some(service);
        }
    }
    fn value_state(&self) -> Result<Arc<Evaluation>, SessionError> {
        self.evaluation
            .clone()
            .filter(|state| !state.retired.load(Ordering::SeqCst))
            .ok_or_else(|| wasmtime::Error::msg("Native value owner has retired").into())
    }
    fn root_value(&mut self, root: &OwnedRooted<AnyRef>) -> Result<SessionValue, SessionError> {
        let mut roots = RootScope::new(&mut self.store);
        let value = Val::AnyRef(Some(root.to_rooted(&mut roots)));
        own(&mut roots, value, self.identity, &self.handles)
    }
    fn native_value_endpoint(
        &mut self,
        value: &SessionValue,
    ) -> Result<Option<Arc<ForeignValue>>, SessionError> {
        self.check(value)?;
        let runtime = self.runtime;
        let mut roots = RootScope::new(&mut self.store);
        let value = value.val(&mut roots.as_context_mut());
        let mut test = [Val::I32(0)];
        runtime
            .get_func(&mut roots, "closure-value-test")
            .unwrap()
            .call(&mut roots, &[value], &mut test)?;
        if test[0].unwrap_i32() != 0 {
            let object = value.unwrap_anyref().unwrap().as_struct(&roots)?.unwrap();
            // Captured user closures do not have the universal host entry shape.
            if object.ty(&roots)?.fields().len() == runtime_abi::UNIVERSAL_MAX as usize + 1 {
                let env = object.field(&mut roots, runtime_abi::UNIVERSAL_ENVIRONMENT as usize)?;
                return capability(roots.as_context_mut(), env).map_err(Into::into);
            }
        } else {
            // Imported MetaFn values carry an authenticated capability in their
            // private prototype holder. User with-meta creates a fresh holder.
            let Some(object) = value
                .anyref()
                .flatten()
                .map(|r| r.as_struct(&roots))
                .transpose()?
                .flatten()
            else {
                return Ok(None);
            };
            if object.ty(&roots)?.fields().len() == 4 {
                let prototype = object.field(&mut roots, 2)?;
                if let Some(array) = prototype
                    .anyref()
                    .flatten()
                    .map(|r| r.as_array(&roots))
                    .transpose()?
                    .flatten()
                {
                    if array.len(&roots)? == 2 {
                        let value = array.get(&mut roots, 1)?;
                        return capability(roots.as_context_mut(), value).map_err(Into::into);
                    }
                }
            }
        }
        Ok(None)
    }
    fn existing_native_value(
        &mut self,
        state: &Arc<Evaluation>,
        value: &SessionValue,
    ) -> Result<Option<Arc<ForeignValue>>, SessionError> {
        let Ok(root) = value.object() else {
            return Ok(None);
        };
        let mut exports = registry_lock(&state.values.exports);
        exports.retain(|entry| entry.strong_count() != 0);
        for previous in exports.iter().filter_map(Weak::upgrade) {
            self.charge_host_work(1)?;
            let mut roots = RootScope::new(&mut self.store);
            let first = root.to_rooted(&mut roots);
            let second = previous.root.to_rooted(&mut roots);
            if wasmtime::Rooted::ref_eq(&roots, &first, &second)? {
                return Ok(Some(previous));
            }
        }
        Ok(None)
    }
    fn retain_native_value(
        &mut self,
        state: &Arc<Evaluation>,
        value: &SessionValue,
        kind: NativeKind,
    ) -> Result<Transfer, SessionError> {
        let service = registry_lock(&state.value_service)
            .as_ref()
            .and_then(MacroServiceWeak::upgrade);
        let uid = {
            let runtime = self.runtime;
            let mut roots = RootScope::new(&mut self.store);
            let input = value.val(&mut roots.as_context_mut());
            let mut result = [Val::null_any_ref()];
            runtime.get_func(&mut roots, "identity-uid").unwrap().call(
                &mut roots,
                &[input],
                &mut result,
            )?;
            let mut integer = [Val::I64(0)];
            runtime.get_func(&mut roots, "long-unbox").unwrap().call(
                &mut roots,
                &result,
                &mut integer,
            )?;
            integer[0].unwrap_i64()
        };
        let endpoint = Arc::new(ForeignValue {
            owner: self.identity,
            state: Arc::downgrade(state),
            service,
            root: value.object()?.clone(),
            uid,
            kind,
        });
        registry_lock(&state.values.exports).push(Arc::downgrade(&endpoint));
        Ok(Transfer::Native(endpoint))
    }
    fn export_materialized_value(
        &mut self,
        value: &SessionValue,
    ) -> Result<Transfer, SessionError> {
        if let Some(endpoint) = self.native_value_endpoint(value)?
            && matches!(&endpoint.kind, NativeKind::Materialized(_))
        {
            return Ok(Transfer::Native(endpoint));
        }
        let state = self.value_state()?;
        if let Some(endpoint) = self.existing_native_value(&state, value)?
            && matches!(&endpoint.kind, NativeKind::Materialized(_))
        {
            return Ok(Transfer::Native(endpoint));
        }
        let bridge = state.bridge(self)?;
        // This reader validates the original builtin descriptors and rejects
        // opaque/mutable values and unrealized sequences. It does not invoke
        // user code or turn unsupported objects into successful data.
        let form = bridge.read_materialized(self, value)?;
        let nominal = {
            let runtime = self.runtime;
            let mut roots = RootScope::new(&mut self.store);
            let input = value.val(&mut roots.as_context_mut());
            let mut result = [Val::I32(0)];
            runtime
                .get_func(&mut roots, "object-same-descriptor")
                .unwrap()
                .call(&mut roots, &[input, input], &mut result)?;
            result[0].unwrap_i32() != 0
        };
        // Keywords retain their interned destination roots. Actual canonical
        // empty roots also map to the destination's corresponding singleton;
        // merely having an empty payload is insufficient to qualify.
        if !nominal
            || matches!(&form.kind, Kind::Keyword(_))
            || bridge.is_canonical_empty(self, value)?
        {
            return Ok(Transfer::Data(form));
        }
        self.retain_native_value(&state, value, NativeKind::Materialized(form))
    }
    fn export_value(&mut self, value: &SessionValue) -> Result<Transfer, SessionError> {
        self.check(value)?;
        let _operation = self.operation_budget_scope()?;
        self.charge_host_work(1)?;
        if let Some(endpoint) = self.native_value_endpoint(value)? {
            return Ok(Transfer::Native(endpoint));
        }
        let state = self.value_state()?;
        let (function, wrapped) = {
            let mut roots = RootScope::new(&mut self.store);
            let input = value.val(&mut roots.as_context_mut());
            let mut result = [Val::I32(0)];
            self.runtime
                .get_func(&mut roots, "class-value-is")
                .unwrap()
                .call(&mut roots, &[input], &mut result)?;
            let class = result[0].unwrap_i32() != 0;
            self.runtime
                .get_func(&mut roots, "closure-value-test")
                .unwrap()
                .call(&mut roots, &[input], &mut result)?;
            let function = !class && result[0].unwrap_i32() != 0;
            let descriptor = registry_lock(&state.meta_fn_class).clone().unwrap();
            let descriptor = Val::AnyRef(Some(descriptor.to_rooted(&mut roots)));
            self.runtime
                .get_func(&mut roots, "object-instance")
                .unwrap()
                .call(&mut roots, &[descriptor, input], &mut result)?;
            (function, result[0].unwrap_i32() != 0)
        };
        if !function && !wrapped {
            return self.export_materialized_value(value);
        }
        if let Some(existing) = self.existing_native_value(&state, value)? {
            return Ok(Transfer::Native(existing));
        }
        let metadata = if wrapped {
            let fields = self.data_fields(value)?;
            Some(self.export_materialized_value(&fields[1])?)
        } else {
            None
        };
        self.retain_native_value(&state, value, NativeKind::Function { metadata })
    }
    fn new_value_capability(
        &mut self,
        state: &Arc<Evaluation>,
        endpoint: &Arc<ForeignValue>,
    ) -> Result<OwnedRooted<AnyRef>, SessionError> {
        let ty = {
            let mut slot = registry_lock(&state.values.capability);
            if slot.is_none() {
                *slot = Some(StructType::new(
                    &self.engine,
                    [FieldType::new(
                        Mutability::Const,
                        StorageType::ValType(ValType::EXTERNREF),
                    )],
                )?);
            }
            slot.clone().unwrap()
        };
        let mut roots = RootScope::new(&mut self.store);
        let external = ExternRef::new(&mut roots, endpoint.clone())?;
        let allocator = StructRefPre::new(&mut roots, ty);
        let cap =
            StructRef::new(&mut roots, &allocator, &[Val::ExternRef(Some(external))])?.to_anyref();
        Ok(cap.to_owned_rooted(&mut roots)?)
    }
    fn attach_value_capability(
        &mut self,
        value: &SessionValue,
        cap: &OwnedRooted<AnyRef>,
        uid: i64,
    ) -> Result<SessionValue, SessionError> {
        self.check(value)?;
        let runtime = self.runtime;
        let mut roots = RootScope::new(&mut self.store);
        let original = value
            .object()?
            .to_rooted(&mut roots)
            .as_struct(&roots)?
            .unwrap();
        let mut fields = original.fields(&mut roots)?.collect::<Vec<_>>();
        let old = match fields[2]
            .anyref()
            .flatten()
            .map(|value| value.as_array(&roots))
            .transpose()?
            .flatten()
        {
            Some(array) if array.len(&roots)? == 1 => array.get(&mut roots, 0)?,
            None if fields[2].unwrap_anyref().is_none() => {
                let mut result = [Val::null_any_ref()];
                runtime
                    .get_func(&mut roots, "owned-descriptor-prototype")
                    .unwrap()
                    .call(&mut roots, &[fields[0]], &mut result)?;
                result[0]
            }
            _ => return Err(wasmtime::Error::msg("Invalid native value prototype storage").into()),
        };
        let mut result = [Val::null_any_ref()];
        runtime.get_func(&mut roots, "args-new").unwrap().call(
            &mut roots,
            &[Val::I32(2)],
            &mut result,
        )?;
        let array = result[0]
            .unwrap_anyref()
            .unwrap()
            .as_array(&roots)?
            .unwrap();
        array.set(&mut roots, 0, old)?;
        let capability = Val::AnyRef(Some(cap.to_rooted(&mut roots)));
        array.set(&mut roots, 1, capability)?;
        fields[2] = result[0];
        let mut number = [Val::null_any_ref()];
        runtime.get_func(&mut roots, "long-box").unwrap().call(
            &mut roots,
            &[Val::I64(uid)],
            &mut number,
        )?;
        fields[3] = number[0];
        let ty = original.ty(&roots)?;
        let allocator = StructRefPre::new(&mut roots, ty);
        let object = StructRef::new(&mut roots, &allocator, &fields)?;
        own(
            &mut roots,
            Val::AnyRef(Some(object.to_anyref())),
            self.identity,
            &self.handles,
        )
    }
    fn import_value(&mut self, transfer: Transfer) -> Result<SessionValue, SessionError> {
        let _operation = self.operation_budget_scope()?;
        self.import_value_ref(&transfer)
    }
    fn import_value_ref(&mut self, transfer: &Transfer) -> Result<SessionValue, SessionError> {
        self.charge_host_work(1)?;
        let Transfer::Native(endpoint) = transfer else {
            let Transfer::Data(form) = transfer else {
                unreachable!()
            };
            return self.value_state()?.bridge(self)?.quote_ref(self, form);
        };
        let source = endpoint
            .state
            .upgrade()
            .filter(|state| !state.retired.load(Ordering::SeqCst))
            .ok_or_else(|| wasmtime::Error::msg("Source native value owner has retired"))?;
        if endpoint.owner == self.identity {
            if !self
                .evaluation
                .as_ref()
                .is_some_and(|state| Arc::ptr_eq(state, &source))
            {
                return Err(SessionError::ForeignValue);
            }
            return self.root_value(&endpoint.root);
        }
        let state = self.value_state()?;
        let cached = registry_lock(&state.values.imports)
            .iter()
            .find(|entry| Arc::ptr_eq(&entry.source, endpoint))
            .map(|entry| entry.local.clone());
        if let Some(cached) = cached {
            return self.root_value(&cached);
        }
        let cap = self.new_value_capability(&state, endpoint)?;
        let value = match &endpoint.kind {
            NativeKind::Materialized(form) => {
                let value = state.bridge(self)?.quote_ref(self, form)?;
                self.attach_value_capability(&value, &cap, endpoint.uid)?
            }
            NativeKind::Function { metadata } => {
                let constructor = self
                    .runtime
                    .get_func(&mut self.store, "closure-new")
                    .unwrap();
                let invoke = self.value_invoke(&state, constructor)?;
                let value = {
                    let mut roots = RootScope::new(&mut self.store);
                    let env = Val::AnyRef(Some(cap.to_rooted(&mut roots)));
                    let value = call(
                        &mut roots,
                        self.runtime,
                        constructor,
                        &[env, Val::FuncRef(Some(invoke)), Val::I32(0), Val::I32(-1)],
                        self.identity,
                        &self.handles,
                    )?;
                    let object = value
                        .object()?
                        .to_rooted(&mut roots)
                        .as_struct(&roots)?
                        .unwrap();
                    let mut uid = [Val::null_any_ref()];
                    self.runtime
                        .get_func(&mut roots, "long-box")
                        .unwrap()
                        .call(&mut roots, &[Val::I64(endpoint.uid)], &mut uid)?;
                    object.set_field(&mut roots, runtime_abi::CLOSURE_UID as usize, uid[0])?;
                    value
                };
                if let Some(metadata) = metadata {
                    let metadata = self.import_value_ref(metadata)?;
                    let constructor = self.original_core_function("MetaFn")?;
                    let wrapped = self.data_construct(&constructor, &[&value, &metadata])?;
                    self.attach_value_capability(&wrapped, &cap, endpoint.uid)?
                } else {
                    value
                }
            }
        };
        if state.values.depth.load(Ordering::SeqCst) != 0 {
            registry_lock(&state.values.imports).push(Imported {
                source: endpoint.clone(),
                local: value.object()?.clone(),
            });
        }
        Ok(value)
    }
    fn value_invoke(
        &mut self,
        state: &Arc<Evaluation>,
        constructor: Func,
    ) -> Result<Func, SessionError> {
        if let Some(invoke) = *registry_lock(&state.values.invoke) {
            return Ok(invoke);
        }
        let parameter = constructor.ty(&self.store).params().nth(1).unwrap();
        let wasmtime::HeapType::ConcreteFunc(signature) = parameter.unwrap_ref().heap_type() else {
            unreachable!()
        };
        let state_ref = Arc::downgrade(state);
        let invoke = Func::try_new(
            &mut self.store,
            signature.clone(),
            move |mut caller, arguments, results| {
                let state = state_ref
                    .upgrade()
                    .filter(|state| !state.retired.load(Ordering::SeqCst))
                    .ok_or_else(|| wasmtime::Error::msg("Native value owner has retired"))?;
                let endpoint = capability(caller.as_context_mut(), arguments[0])?
                    .ok_or_else(|| wasmtime::Error::msg("Invalid native invocation capability"))?;
                let profile = registry_lock(&state.profile)
                    .clone()
                    .ok_or_else(|| wasmtime::Error::msg("No active native value profile"))?;
                let mut view = profile.bind(caller.as_context_mut());
                let _values = view.native_values_scope();
                let result = view.invoke_foreign(&endpoint, arguments[1]);
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
        *registry_lock(&state.values.invoke) = Some(invoke);
        Ok(invoke)
    }
    fn invoke_foreign(
        &mut self,
        endpoint: &Arc<ForeignValue>,
        arguments: Val,
    ) -> Result<SessionValue, SessionError> {
        if !matches!(&endpoint.kind, NativeKind::Function { .. }) {
            return Err(wasmtime::Error::msg("Native value is not callable").into());
        }
        self.charge_host_work(1)?;
        let values = {
            let mut roots = RootScope::new(&mut self.store);
            let array = arguments
                .unwrap_anyref()
                .unwrap()
                .as_array(&roots)?
                .ok_or_else(|| wasmtime::Error::msg("Invalid native invocation arguments"))?;
            let count = array.len(&roots)?;
            SessionHost::consume_host_work(
                roots.as_context_mut(),
                u64::from(count) + 1,
                &self.interrupt,
            )?;
            let mut values = Vec::with_capacity(count as usize);
            for index in 0..count {
                let value = array.get(&mut roots, index)?;
                values.push(own(&mut roots, value, self.identity, &self.handles)?);
            }
            values
        };
        let mut arguments = Vec::with_capacity(values.len());
        for value in &values {
            arguments.push(self.export_value(value)?);
        }
        let result = endpoint.with_source(self, |source, function| {
            let mut values = Vec::with_capacity(arguments.len());
            for argument in arguments {
                values.push(source.import_value(argument)?);
            }
            let result = source.invoke_callable(function, &values.iter().collect::<Vec<_>>())?;
            source.export_value(&result)
        })?;
        self.import_value(result)
    }
    pub(super) fn construct_foreign_function(
        &mut self,
        value: &SessionValue,
    ) -> Result<Option<SessionValue>, SessionError> {
        let Some(endpoint) = self.native_value_endpoint(value)? else {
            return Ok(None);
        };
        if !matches!(&endpoint.kind, NativeKind::Function { .. }) {
            return Ok(None);
        }
        let constructed = endpoint.with_source(self, |source, function| {
            let value = source.materialize_native_function(function)?;
            source.export_value(&value)
        })?;
        self.import_value(constructed).map(Some)
    }
    pub(crate) fn import_evaluation_constant(
        &mut self,
        owner: u64,
        index: u64,
    ) -> Result<SessionValue, SessionError> {
        if owner == self.identity {
            return self.native_constant_value(owner, index);
        }
        let allowance = self.operation_fuel_remaining()?;
        let (transfer, spent) = super::super::suspended::scope(self, || {
            super::super::suspended::with_host(owner, |source| {
                let before = source.operation_fuel_remaining()?;
                let limit =
                    source.cap_operation_fuel(allowance.min(source.options.fuel_per_operation))?;
                let transfer = source
                    .native_constant_value(owner, index)
                    .and_then(|value| source.export_value(&value));
                let spent = limit.saturating_sub(source.operation_fuel_remaining()?);
                source.settle_nested_fuel(before, spent)?;
                Ok::<_, SessionError>((transfer, spent))
            })
        })
        .ok_or_else(|| wasmtime::Error::msg("Native constant owner is not suspended"))??;
        self.settle_nested_fuel(allowance, spent)?;
        self.import_value(transfer?)
    }
    pub(crate) fn transport_expansion_constants(
        &mut self,
        mut form: Form,
        owner: u64,
    ) -> Result<Form, SessionError> {
        if owner == self.identity {
            return Ok(form);
        }
        let mut remapped: BTreeMap<(u64, u64), Kind> = BTreeMap::new();
        let mut work = vec![&mut form];
        while let Some(form) = work.pop() {
            self.charge_host_work(1)?;
            if let Kind::NativeConstant {
                owner: source,
                index,
            } = &form.kind
            {
                let key = (*source, *index);
                let kind = if let Some(previous) = remapped.get(&key) {
                    previous.clone()
                } else {
                    let value = self.native_constant_value(*source, *index)?;
                    let mode = registry_lock(&self.compiler)
                        .native_function_cells
                        .contains_key(
                            &CellIdentity::native_constant(self.phase, self.identity, *index)
                                .import_name(),
                        );
                    let transfer = self.export_value(&value)?;
                    let allowance = self.operation_fuel_remaining()?;
                    let (kind, spent) = super::super::suspended::scope(self, || {
                        super::super::suspended::with_host(owner, |target| {
                            let before = target.operation_fuel_remaining()?;
                            let limit = target.cap_operation_fuel(
                                allowance.min(target.options.fuel_per_operation),
                            )?;
                            let result = (|| {
                                let value = target.import_value(transfer)?;
                                let bridge = target.value_state()?.bridge(target)?;
                                let form = target.read_native_evaluation(&bridge, &value, 0..0)?;
                                let Kind::NativeConstant { index, .. } = &form.kind else {
                                    return Err(wasmtime::Error::msg(
                                        "Imported native constant lost its actual value category",
                                    )
                                    .into());
                                };
                                if mode {
                                    let id = CellIdentity::native_constant(
                                        target.phase,
                                        target.identity,
                                        *index,
                                    );
                                    let mut state = registry_lock(&target.compiler);
                                    let global = state.cells[&id];
                                    Arc::make_mut(&mut state.native_function_cells)
                                        .insert(id.import_name(), global);
                                }
                                Ok::<_, SessionError>(form.into_parts().2)
                            })();
                            let spent = limit.saturating_sub(target.operation_fuel_remaining()?);
                            target.settle_nested_fuel(before, spent)?;
                            Ok::<_, SessionError>((result, spent))
                        })
                    })
                    .ok_or_else(|| wasmtime::Error::msg("Expansion caller is not suspended"))??;
                    self.settle_nested_fuel(allowance, spent)?;
                    let kind = kind?;
                    remapped.insert(key, kind.clone());
                    kind
                };
                form.kind = kind;
            }
            work.extend(form.metadata.iter_mut());
            match &mut form.kind {
                Kind::List(items) | Kind::Vector(items) | Kind::Map(items) | Kind::Set(items) => {
                    work.extend(items.iter_mut())
                }
                Kind::Tagged { form, .. } | Kind::Discard(form) => work.push(form),
                Kind::Prefix { operator, target } => {
                    work.push(operator);
                    work.push(target);
                }
                _ => {}
            }
        }
        Ok(form)
    }
}
impl ForeignValue {
    fn with_source<T>(
        &self,
        caller: &mut SessionHost<'_>,
        operation: impl FnOnce(&mut SessionHost<'_>, &SessionValue) -> Result<T, SessionError>,
    ) -> Result<T, SessionError> {
        let state = self
            .state
            .upgrade()
            .filter(|state| !state.retired.load(Ordering::SeqCst))
            .ok_or_else(|| wasmtime::Error::msg("Native function source has retired"))?;
        let context = caller.evaluation_math_context()?;
        let allowance = caller.operation_fuel_remaining()?;
        let interrupt = caller.host_work_interrupt();
        let mut context = Some(context);
        let mut operation = Some(operation);
        let mut run = |source: &mut SessionHost<'_>| {
            if source.identity != self.owner
                || !source
                    .evaluation
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, &state))
            {
                return Err(SessionError::ForeignValue);
            }
            let function = source.root_value(&self.root)?;
            let result = operation.take().unwrap()(source, &function);
            result.map_err(|error| match error {
                SessionError::Language(ref value) => match state
                    .bridge(source)
                    .and_then(|bridge| bridge.transfer_exception(source, value))
                {
                    Ok(payload) => SessionError::Host(wasmtime::Error::new(payload)),
                    Err(SessionError::Language(_)) => SessionError::Host(wasmtime::Error::msg(
                        "Cannot transfer native callback exception payload",
                    )),
                    Err(error) => error,
                },
                error => error,
            })
        };
        let (result, spent) = super::super::suspended::scope(caller, || {
            if let Some(result) = super::super::suspended::with_host(self.owner, |source| {
                let _budget = source.operation_budget_scope()?;
                let _relay = crate::portable_macros::InterruptRelay::start(
                    interrupt.clone(),
                    source.interrupt_handle(),
                )?;
                let before = source.operation_fuel_remaining()?;
                let limit =
                    source.cap_operation_fuel(allowance.min(source.options.fuel_per_operation))?;
                let bridge = source.value_state()?.bridge(source)?;
                let result =
                    match source.enter_evaluation_math_context(context.take().unwrap(), &bridge) {
                        Ok(checkpoint) => {
                            let result = run(source);
                            let restoration = source.leave_evaluation_math_context(&checkpoint);
                            match result {
                                Err(error) => Err(error),
                                Ok(value) => restoration.map(|()| value),
                            }
                        }
                        Err(error) => Err(error),
                    };
                let spent = limit.saturating_sub(source.operation_fuel_remaining()?);
                source.settle_nested_fuel(before, spent)?;
                Ok::<_, SessionError>((result, spent))
            }) {
                return result;
            }
            self.service
                .as_ref()
                .ok_or_else(|| wasmtime::Error::msg("Native function source is not active"))?
                .with_value_request(allowance, context.take().unwrap(), interrupt, run)
        })?;
        caller.settle_nested_fuel(allowance, spent)?;
        result
    }
}

#[cfg(all(test, not(suss_generator)))]
mod tests {
    use super::*;

    #[test]
    fn native_function_capability_rejects_forged_metadata_prototype_carriers() {
        let mut target = Session::new_macro().unwrap();
        let original = target.eval("(with-meta (fn [] 42) {:tag :kept})").unwrap();
        let runtime = target.runtime;
        let cap_type = StructType::new(
            &target.engine,
            [FieldType::new(
                Mutability::Const,
                StorageType::ValType(ValType::EXTERNREF),
            )],
        )
        .unwrap();
        let forged = {
            let mut roots = RootScope::new(&mut target.store);
            let object = original
                .object()
                .unwrap()
                .to_rooted(&mut roots)
                .as_struct(&roots)
                .unwrap()
                .unwrap();
            let mut fields = object.fields(&mut roots).unwrap().collect::<Vec<_>>();
            let prototype = fields[2]
                .unwrap_anyref()
                .unwrap()
                .as_array(&roots)
                .unwrap()
                .unwrap()
                .get(&mut roots, 0)
                .unwrap();
            let external = ExternRef::new(&mut roots, "not a native endpoint").unwrap();
            let allocator = StructRefPre::new(&mut roots, cap_type);
            let cap = StructRef::new(&mut roots, &allocator, &[Val::ExternRef(Some(external))])
                .unwrap()
                .to_anyref();
            let mut result = [Val::null_any_ref()];
            runtime
                .get_func(&mut roots, "args-new")
                .unwrap()
                .call(&mut roots, &[Val::I32(2)], &mut result)
                .unwrap();
            let carrier = result[0]
                .unwrap_anyref()
                .unwrap()
                .as_array(&roots)
                .unwrap()
                .unwrap();
            carrier.set(&mut roots, 0, prototype).unwrap();
            carrier.set(&mut roots, 1, Val::AnyRef(Some(cap))).unwrap();
            fields[2] = result[0];
            let ty = object.ty(&roots).unwrap();
            let allocator = StructRefPre::new(&mut roots, ty);
            let object = StructRef::new(&mut roots, &allocator, &fields).unwrap();
            own(
                &mut roots,
                Val::AnyRef(Some(object.to_anyref())),
                target.identity,
                &target.handles,
            )
            .unwrap()
        };
        let hash = target.eval("(fn [f] (hash f))").unwrap();
        let error = target.invoke(&hash, &[&forged]).unwrap_err();
        assert!(
            matches!(error, SessionError::Language(_)),
            "a matching private GC shape without authenticated host data must remain invalid: {error}"
        );
        assert!(target.eval("(+ 40 2)").is_ok());
    }

    #[test]
    fn native_function_capability_preserves_identity_across_import_scopes() {
        let mut source = Session::new_repl().unwrap();
        let mut target = Session::new_macro().unwrap();
        for expression in [
            "(fn [] 42)",
            "(with-meta (fn [] 42) {:tag :kept})",
            "(with-meta (fn [] 42) {})",
        ] {
            let original = source.eval(expression).unwrap();
            let first = target
                .import_value(source.export_value(&original).unwrap())
                .unwrap();
            let second = target
                .import_value(source.export_value(&original).unwrap())
                .unwrap();
            let other = source.eval(expression).unwrap();
            let other = target
                .import_value(source.export_value(&other).unwrap())
                .unwrap();
            let identity = target
                .eval("(fn [a b] (and (identical? a b) (= a b) (= (hash a) (hash b)) (identical? (meta a) (meta b))))")
                .unwrap();
            for _ in 0..2 {
                let result = target
                    .invoke(&identity, &[&first, &second])
                    .unwrap_or_else(|error| {
                        panic!(
                            "{expression}: {}",
                            crate::portable_repl::error_display(&mut target, &error)
                        )
                    });
                assert_eq!(
                    target
                        .inspect(&result, |mut store, value| sentinel_tag(&mut store, &value))
                        .unwrap(),
                    Some(runtime_abi::TRUE as u32),
                    "the same actual source function must preserve identity and hash across requests: {expression}"
                );
                let result = target.invoke(&identity, &[&first, &other]).unwrap();
                assert_eq!(
                    target
                        .inspect(&result, |mut store, value| sentinel_tag(&mut store, &value))
                        .unwrap(),
                    Some(runtime_abi::FALSE as u32),
                    "distinct functions must remain distinct: {expression}"
                );
                source.collect().unwrap();
                target.collect().unwrap();
            }
            let returned = source
                .import_value(target.export_value(&first).unwrap())
                .unwrap();
            let identity = source.eval("(fn [a b] (identical? a b))").unwrap();
            let result = source.invoke(&identity, &[&original, &returned]).unwrap();
            assert_eq!(
                source
                    .inspect(&result, |mut store, value| sentinel_tag(&mut store, &value))
                    .unwrap(),
                Some(runtime_abi::TRUE as u32),
                "return transport must recover the exact source root"
            );
        }
    }

    #[test]
    fn native_function_capability_metadata_lifetime_does_not_retain_its_function() {
        let mut source = Session::new_repl().unwrap();
        let mut target = Session::new_macro().unwrap();
        let original = source.eval("(with-meta (fn [] 42) {:tag :kept})").unwrap();
        let proxy = target
            .import_value(source.export_value(&original).unwrap())
            .unwrap();
        let endpoint = target.native_value_endpoint(&proxy).unwrap().unwrap();
        let function = Arc::downgrade(&endpoint);
        drop(endpoint);
        let get_meta = target.eval("(fn [f] (meta f))").unwrap();
        let metadata = target.invoke(&get_meta, &[&proxy]).unwrap();
        let endpoint = target.native_value_endpoint(&metadata).unwrap().unwrap();
        let data = Arc::downgrade(&endpoint);
        drop(endpoint);
        drop(original);
        drop(proxy);
        target.collect().unwrap();
        source.collect().unwrap();
        assert!(
            function.upgrade().is_none(),
            "retained metadata must not retain the function endpoint"
        );
        assert!(
            data.upgrade().is_some(),
            "the actual metadata still owns its source endpoint"
        );
        drop(metadata);
        target.collect().unwrap();
        source.collect().unwrap();
        assert!(
            data.upgrade().is_none(),
            "metadata endpoints must not become permanent roots"
        );
    }

    #[test]
    fn native_function_capability_calls_real_captures_and_releases_source_roots_after_gc() {
        let mut source = Session::new_repl().unwrap();
        let mut target = Session::new_macro().unwrap();
        let original = source
            .eval("(let [counter (atom 41)] (fn [] (swap! counter inc)))")
            .unwrap();
        assert!(matches!(
            target.invoke(&original, &[]),
            Err(SessionError::ForeignValue)
        ));
        let transfer = source.export_value(&original).unwrap();
        let proxy = target.import_value(transfer).unwrap();
        let endpoint = target.native_value_endpoint(&proxy).unwrap().unwrap();
        let weak = Arc::downgrade(&endpoint);
        drop(endpoint);
        drop(original);
        source.collect().unwrap();
        target.collect().unwrap();
        for required in [42, 43] {
            let value = super::super::super::suspended::scope(&mut source, || {
                target.invoke_callable(&proxy, &[])
            })
            .unwrap();
            assert_eq!(
                target
                    .inspect(&value, |mut store, value| runtime_number(
                        &mut store, &value
                    ))
                    .unwrap(),
                Some(RuntimeNumber::Long(required))
            );
            source.collect().unwrap();
            target.collect().unwrap();
        }
        assert!(weak.upgrade().is_some());
        drop(proxy);
        target.collect().unwrap();
        source.collect().unwrap();
        assert!(
            weak.upgrade().is_none(),
            "a per-function native callback or permanent import cache must not retain an unreachable capability"
        );
    }

    #[test]
    fn native_function_capability_rejects_a_retired_source_before_touching_its_gc_root() {
        let mut source = Session::new_repl().unwrap();
        let mut target = Session::new_macro().unwrap();
        let original = source.eval("(fn [] 42)").unwrap();
        let transfer = source.export_value(&original).unwrap();
        let proxy = target.import_value(transfer).unwrap();
        drop(original);
        source.reset().unwrap();
        let error = super::super::super::suspended::scope(&mut source, || {
            target.invoke_callable(&proxy, &[])
        })
        .unwrap_err();
        let message = crate::portable_repl::error_display(&mut target, &error);
        assert!(message.contains("retired"), "{message}");
        let value = target.eval("42").unwrap();
        assert_eq!(
            target
                .inspect(&value, |mut store, value| runtime_number(
                    &mut store, &value
                ))
                .unwrap(),
            Some(RuntimeNumber::Long(42))
        );
    }
}
