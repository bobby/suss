//! Authenticated native value transport. GC capabilities retain real source
//! roots; one shared invocation callback per destination never owns those roots.
//! The source Store is borrowed only synchronously or leased by its own service.
use super::*;
mod operations;
mod namespace_catalog;
mod macro_vars;
mod function_namespace;
use crate::portable_macros::MacroServiceWeak;
use crate::runtime_abi::native_values as native_op;
use std::sync::Weak;
use suss_reader::forms::{Form, Kind};
use wasmtime::{AsContext, ExternRef, FieldType, StorageType, StructRef, StructRefPre, StructType};

#[derive(Default)]
pub(super) struct Values {
    exports: Mutex<ExportCache>,
    imports: Mutex<Vec<Imported>>,
    depth: AtomicUsize,
    invoke: Mutex<Option<Func>>,
    capability: Mutex<Option<StructType>>,
}
#[derive(Default)]
struct ExportCache {
    // UID narrows the search; ref_eq on the actual root decides identity.
    by_uid: BTreeMap<i64, Vec<Weak<ForeignValue>>>,
    sweep_cursor: Option<i64>,
    sweep_until: Option<i64>,
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
    Function {
        metadata: Option<Transfer>,
    },
    Materialized {
        form: Form,
        constants: BTreeMap<u64, Transfer>,
    },
    Nominal(LiveClass),
    ObjectArray,
}
#[derive(Clone, Copy)]
enum LiveClass {
    Var,
    VarUnbound,
    Cons,
    LazySeq,
    Namespace,
}
impl LiveClass {
    fn name(self) -> &'static str {
        match self {
            Self::Var => "Var",
            Self::VarUnbound => "VarUnbound",
            Self::Cons => "Cons",
            Self::LazySeq => "LazySeq",
            Self::Namespace => "Namespace",
        }
    }
    fn fields(self) -> usize {
        match self {
            Self::Var => 3,
            Self::VarUnbound => 1,
            Self::Cons | Self::LazySeq => 4,
            Self::Namespace => 2,
        }
    }
}
impl NativeKind {
    fn tag(&self) -> i32 {
        match self {
            Self::Function { .. } => native_op::FUNCTION,
            Self::Materialized { .. } => native_op::DATA,
            Self::Nominal(LiveClass::Var) => native_op::VAR,
            Self::Nominal(LiveClass::VarUnbound) => native_op::VAR_UNBOUND,
            Self::Nominal(LiveClass::Cons) => native_op::CONS,
            Self::Nominal(LiveClass::LazySeq) => native_op::LAZY_SEQ,
            Self::Nominal(LiveClass::Namespace) => native_op::NAMESPACE,
            Self::ObjectArray => native_op::OBJECT_ARRAY,
        }
    }
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
                results[0] = if matches!(arguments[1], Val::AnyRef(None)) {
                    // Private kind query; normal identity always supplies two caps.
                    Val::I32(first.map_or(0, |value| value.kind.tag()))
                } else {
                    let second = capability(view.store.as_context_mut(), arguments[1])?;
                    Val::I32(i32::from(
                        first.zip(second).is_some_and(|(a, b)| Arc::ptr_eq(&a, &b)),
                    ))
                };
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
    pub(crate) fn root_value(&mut self, root: &OwnedRooted<AnyRef>) -> Result<SessionValue, SessionError> {
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
        let mut result = [Val::null_any_ref()];
        runtime
            .get_func(&mut roots, "native-identity-capability")
            .unwrap()
            .call(&mut roots, &[value], &mut result)?;
        capability(roots.as_context_mut(), result[0]).map_err(Into::into)
    }
    pub(crate) fn matches_imported_function_root(&mut self, value: &SessionValue, expected: &SessionValue)
        -> Result<bool, SessionError> {
        self.charge_host_work(4)?;
        let Some(endpoint) = self.native_value_endpoint(value)? else { return Ok(false); };
        if endpoint.owner != expected.identity || !matches!(&endpoint.kind, NativeKind::Function { .. }) {
            return Ok(false);
        }
        let _state = endpoint.state.upgrade().filter(|state| !state.retired.load(Ordering::SeqCst))
            .ok_or_else(|| wasmtime::Error::msg("Native function source has retired"))?;
        let Some(service) = &endpoint.service else { return Ok(false); };
        service.identical_source_function_root(&endpoint.root, expected)
    }
    fn native_value_uid(&mut self, value: &SessionValue) -> Result<i64, SessionError> {
        let runtime = self.runtime;
        let mut roots = RootScope::new(&mut self.store);
        let input = value.val(&mut roots.as_context_mut());
        let mut result = [Val::null_any_ref()];
        runtime.get_func(&mut roots, "identity-uid").unwrap().call(&mut roots, &[input], &mut result)?;
        let mut integer = [Val::I64(0)];
        runtime.get_func(&mut roots, "long-unbox").unwrap().call(&mut roots, &result, &mut integer)?;
        Ok(integer[0].unwrap_i64())
    }
    fn existing_native_value(
        &mut self, state: &Arc<Evaluation>, value: &SessionValue,
    ) -> Result<Option<Arc<ForeignValue>>, SessionError> {
        let Ok(root) = value.object() else { return Ok(None); };
        // Scalar data and class constructors have no exported owner identity.
        // Check actual runtime types before asking the UID intrinsic to read it.
        let supported = {
            let runtime = self.runtime;
            let mut roots = RootScope::new(&mut self.store);
            let input = value.val(&mut roots.as_context_mut());
            let Some(object) = root.to_rooted(&mut roots).as_struct(&roots)? else { return Ok(None); };
            if object.ty(&roots)?.fields().len() == 1 { return Ok(None); }
            let mut result = [Val::I32(0)];
            runtime.get_func(&mut roots, "class-value-is").unwrap().call(&mut roots, &[input], &mut result)?;
            if result[0].unwrap_i32() != 0 { return Ok(None); }
            runtime.get_func(&mut roots, "closure-value-test").unwrap().call(&mut roots, &[input], &mut result)?;
            if result[0].unwrap_i32() != 0 { true } else {
                runtime.get_func(&mut roots, "object-same-descriptor").unwrap().call(&mut roots, &[input, input], &mut result)?;
                if result[0].unwrap_i32() != 0 { true } else {
                    let mut result = [Val::null_any_ref()];
                    runtime.get_func(&mut roots, "object-array-test").unwrap().call(&mut roots, &[input], &mut result)?;
                    sentinel_tag(&mut roots.as_context_mut(), &result[0])? == Some(runtime_abi::TRUE as u32)
                }
            }
        };
        if !supported { return Ok(None); }
        let uid = self.native_value_uid(value)?;
        self.charge_host_work(1)?;
        let exports = registry_lock(&state.values.exports);
        let Some(candidates) = exports.by_uid.get(&uid) else { return Ok(None); };
        self.charge_host_work(candidates.len() as u64)?;
        for previous in candidates.iter().filter_map(Weak::upgrade) {
            let mut roots = RootScope::new(&mut self.store);
            let first = root.to_rooted(&mut roots);
            let second = previous.root.to_rooted(&mut roots);
            if wasmtime::Rooted::ref_eq(&roots, &first, &second)? { return Ok(Some(previous)); }
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
        let uid = self.native_value_uid(value)?;
        self.charge_host_work(4)?;
        let endpoint = Arc::new(ForeignValue {
            owner: self.identity,
            state: Arc::downgrade(state),
            service,
            root: value.object()?.clone(),
            uid,
            kind,
        });
        let mut exports = registry_lock(&state.values.exports);
        exports.by_uid.entry(uid).or_default().push(Arc::downgrade(&endpoint));
        // Sweep one bucket per insertion. A fixed frontier makes each cycle
        // finish even while newly assigned UIDs continue arriving. This keeps
        // dead weak entries bounded without a large cleanup charge on a later
        // small request. No strong GC roots are retained by this index.
        if exports.sweep_until.is_none() {
            exports.sweep_until = exports.by_uid.last_key_value().map(|(uid, _)| *uid);
        }
        if let Some(frontier) = exports.sweep_until {
            use std::ops::Bound::{Excluded, Included, Unbounded};
            let start = exports.sweep_cursor.map_or(Unbounded, Excluded);
            let next = exports.by_uid.range((start, Included(frontier))).next().map(|(uid, _)| *uid);
            if let Some(next) = next {
                let bucket = exports.by_uid.get_mut(&next).unwrap();
                self.charge_host_work(bucket.len() as u64 + 1)?;
                bucket.retain(|entry| entry.strong_count() != 0);
                if bucket.is_empty() { exports.by_uid.remove(&next); }
                exports.sweep_cursor = Some(next);
            } else {
                exports.sweep_cursor = None;
                exports.sweep_until = None;
            }
        }
        Ok(Transfer::Native(endpoint))
    }
    fn export_materialized_value(
        &mut self,
        value: &SessionValue,
    ) -> Result<Transfer, SessionError> {
        if let Some(endpoint) = self.native_value_endpoint(value)?
            && matches!(&endpoint.kind, NativeKind::Materialized { .. })
        {
            return Ok(Transfer::Native(endpoint));
        }
        let state = self.value_state()?;
        if let Some(endpoint) = self.existing_native_value(&state, value)?
            && matches!(&endpoint.kind, NativeKind::Materialized { .. })
        {
            return Ok(Transfer::Native(endpoint));
        }
        let bridge = state.bridge(self)?;
        // Temporary nested roots are authenticated by the original builtin
        // descriptors. They never enter the persistent compiler constant pool.
        // Opaque/mutable objects and unrealized sequences remain unsupported.
        let runtime = self.runtime;
        let closure_test = runtime
            .get_func(&mut self.store, "closure-value-test")
            .unwrap();
        let class_test = runtime.get_func(&mut self.store, "class-value-is").unwrap();
        let instance_test = runtime
            .get_func(&mut self.store, "object-instance")
            .unwrap();
        let identity_test = runtime
            .get_func(&mut self.store, "native-identical-test")
            .unwrap();
        let var_class = registry_lock(&state.var_class).clone().unwrap();
        let unbound_class = registry_lock(&state.unbound_class).clone().unwrap();
        let meta_fn_class = registry_lock(&state.meta_fn_class).clone().unwrap();
        let namespace_class = registry_lock(&state.namespace)
            .as_ref()
            .unwrap()
            .namespace
            .clone();
        let array_test = self
            .runtime
            .get_func(&mut self.store, "object-array-test")
            .unwrap();
        let (form, roots) = bridge.read_transport(
            self,
            value,
            self.identity,
            closure_test,
            class_test,
            instance_test,
            identity_test,
            var_class,
            unbound_class,
            meta_fn_class,
            namespace_class,
            array_test,
        )?;
        let mut constants = BTreeMap::new();
        for (index, root, _) in roots {
            self.charge_host_work(1)?;
            let value = self.root_value(&root)?;
            let transfer =
                stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || self.export_value(&value))?;
            constants.insert(index, transfer);
        }
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
            if !constants.is_empty() {
                return Err(wasmtime::Error::msg(
                    "Scalar or canonical singleton cannot contain scoped native roots",
                )
                .into());
            }
            return Ok(Transfer::Data(form));
        }
        self.retain_native_value(&state, value, NativeKind::Materialized { form, constants })
    }
    /// Scalars have no native identity carrier or metadata. Use the same
    /// physical scalar predicates as the nominal decoder, without running every
    /// function/class/Var test for each nil or numeric sequence field.
    fn plain_transfer_scalar(&mut self, value: &SessionValue) -> Result<Option<Form>, SessionError> {
        let interrupt = self.host_work_interrupt();
        self.inspect(value, |mut store, value| {
            let kind = if let Some(number) = runtime_number(&mut store, &value)? {
                match number {
                    RuntimeNumber::Long(value) => Kind::Long(value),
                    RuntimeNumber::Double(value) => Kind::Number(value),
                }
            } else if let Some(tag) = sentinel_tag(&mut store, &value)? {
                match tag as i32 {
                    runtime_abi::NIL => Kind::Nil,
                    runtime_abi::FALSE => Kind::Bool(false),
                    runtime_abi::TRUE => Kind::Bool(true),
                    _ => return Ok(None),
                }
            } else {
                let Some(reference) = value.anyref().flatten() else { return Ok(None); };
                let Some(array) = reference.as_array(&store)? else { return Ok(None); };
                if !matches!(array.ty(&store)?.element_type(), StorageType::I16) {
                    return Ok(None);
                }
                let length = array.len(&store)?;
                let mut units = Vec::with_capacity((length as usize).min(4096));
                for index in 0..length {
                    SessionHost::consume_host_work(store.as_context_mut(), 1, &interrupt)?;
                    units.push(array.get(&mut store, index)?.unwrap_i32() as u16);
                }
                Kind::String(units)
            };
            SessionHost::consume_host_work(store.as_context_mut(), 1, &interrupt)?;
            Ok(Some(Form { span: 0..0, metadata: vec![], kind }))
        })
    }
    fn export_value(&mut self, value: &SessionValue) -> Result<Transfer, SessionError> {
        self.check(value)?;
        let _operation = self.operation_budget_scope()?;
        self.charge_host_work(1)?;
        if let Some(form) = self.plain_transfer_scalar(value)? {
            self.value_state()?;
            return Ok(Transfer::Data(form));
        }
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
            if let Some(existing) = self.existing_native_value(&state, value)? {
                return Ok(Transfer::Native(existing));
            }
            let sequences = registry_lock(&state.sequence_classes).clone();
            let classes = [
                (registry_lock(&state.var_class).clone().unwrap(), LiveClass::Var),
                (registry_lock(&state.unbound_class).clone().unwrap(), LiveClass::VarUnbound),
                (registry_lock(&state.namespace).as_ref().unwrap().namespace.clone(), LiveClass::Namespace),
                (sequences["Cons"].clone(), LiveClass::Cons),
                (sequences["LazySeq"].clone(), LiveClass::LazySeq),
            ];
            let live = {
                let mut roots = RootScope::new(&mut self.store);
                let input = value.val(&mut roots.as_context_mut());
                let test = self.runtime.get_func(&mut roots, "object-instance").unwrap();
                let mut found = None;
                for (descriptor, class) in classes {
                    let descriptor = Val::AnyRef(Some(descriptor.to_rooted(&mut roots)));
                    let mut result = [Val::I32(0)];
                    test.call(&mut roots, &[descriptor, input], &mut result)?;
                    if result[0].unwrap_i32() != 0 { found = Some(NativeKind::Nominal(class)); break; }
                }
                if found.is_none() {
                    let mut result = [Val::null_any_ref()];
                    self.runtime.get_func(&mut roots, "object-array-test").unwrap()
                        .call(&mut roots, &[input], &mut result)?;
                    if sentinel_tag(&mut roots.as_context_mut(), &result[0])? == Some(runtime_abi::TRUE as u32) {
                        found = Some(NativeKind::ObjectArray);
                    }
                }
                found
            };
            return match live {
                Some(kind) => self.retain_native_value(&state, value, kind),
                None => self.export_materialized_value(value),
            };
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
            NativeKind::Nominal(class) => {
                let constructor = self.original_core_function(class.name())?;
                let nil = self.data_scalar(&crate::portable::hir::Literal::Nil)?;
                let fields = vec![&nil; class.fields()];
                let object = self.data_construct(&constructor, &fields)?;
                let object = self.attach_value_capability(&object, &cap, endpoint.uid)?;
                let mut roots = RootScope::new(&mut self.store);
                let original = object
                    .object()?
                    .to_rooted(&mut roots)
                    .as_struct(&roots)?
                    .unwrap();
                let mut fields = original.fields(&mut roots)?.collect::<Vec<_>>();
                let mut empty = [Val::null_any_ref()];
                self.runtime
                    .get_func(&mut roots, "args-new")
                    .unwrap()
                    .call(&mut roots, &[Val::I32(0)], &mut empty)?;
                fields[1] = empty[0];
                let ty = original.ty(&roots)?;
                let allocator = StructRefPre::new(&mut roots, ty);
                let object = StructRef::new(&mut roots, &allocator, &fields)?;
                own(
                    &mut roots,
                    Val::AnyRef(Some(object.to_anyref())),
                    self.identity,
                    &self.handles,
                )?
            }
            NativeKind::ObjectArray => {
                let kind = self.data_scalar(&crate::portable::hir::Literal::Long(6))?;
                let length = self.data_scalar(&crate::portable::hir::Literal::Long(0))?;
                let mut roots = RootScope::new(&mut self.store);
                let mut original = [Val::null_any_ref()];
                let kind = kind.val(&mut roots.as_context_mut());
                let length = length.val(&mut roots.as_context_mut());
                self.runtime
                    .get_func(&mut roots, "packed-array-new")
                    .unwrap()
                    .call(&mut roots, &[kind, length], &mut original)?;
                let original = original[0]
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&roots)?
                    .unwrap();
                let mut uid = [Val::null_any_ref()];
                self.runtime
                    .get_func(&mut roots, "long-box")
                    .unwrap()
                    .call(&mut roots, &[Val::I64(endpoint.uid)], &mut uid)?;
                let ty = original.ty(&roots)?;
                let allocator = StructRefPre::new(&mut roots, ty);
                let capability = Val::AnyRef(Some(cap.to_rooted(&mut roots)));
                let object = StructRef::new(&mut roots, &allocator, &[capability, uid[0]])?;
                own(
                    &mut roots,
                    Val::AnyRef(Some(object.to_anyref())),
                    self.identity,
                    &self.handles,
                )?
            }
            NativeKind::Materialized { form, constants } => {
                let mut values = BTreeMap::new();
                for (index, transfer) in constants {
                    self.charge_host_work(1)?;
                    let value = stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
                        self.import_value_ref(transfer)
                    })?;
                    values.insert(*index, value);
                }
                let value =
                    state
                        .bridge(self)?
                        .quote_transport_ref(self, form, endpoint.owner, &values)?;
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
        let namespace = self.export_function_namespace()?;
        let result = endpoint.with_source(self, |source, function| {
            let mut values = Vec::with_capacity(arguments.len());
            for argument in arguments {
                values.push(source.import_value(argument)?);
            }
            let checkpoint = source.enter_function_namespace(namespace)?;
            let result = source.invoke_callable(function, &values.iter().collect::<Vec<_>>());
            let restoration = source.leave_compilation_namespace(&checkpoint);
            let result = match result { Err(error) => return Err(error), Ok(value) => { restoration?; value } };
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
            .ok_or_else(|| wasmtime::Error::msg("Native value source has retired"))?;
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
                .ok_or_else(|| wasmtime::Error::msg("Native value source is not active"))?
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
    fn native_scalar_exports_fit_a_small_budget_without_changing_categories_or_units() {
        use crate::portable::hir::Literal;
        let cases = vec![Literal::Nil, Literal::Bool(false), Literal::Bool(true),
            Literal::Long(i64::MIN), Literal::Long(i64::MAX), Literal::Long(42),
            Literal::Number(1.25), Literal::Number(-0.0), Literal::Number(f64::INFINITY),
            Literal::Number(f64::NEG_INFINITY), Literal::Number(f64::NAN),
            Literal::String(vec![]), Literal::String(vec![0xd800, 0, 0xdc00, 0xffff])];
        for mut source in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
            for literal in &cases {
                source.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
                let value = source.data_scalar(literal).unwrap();
                source.collect().unwrap();
                source.set_operation_fuel(256);
                let Transfer::Data(form) = source.export_value(&value).unwrap() else {
                    panic!("a scalar must not gain a native identity capability")
                };
                assert!(form.metadata.is_empty());
                match (&form.kind, literal) {
                    (Kind::Nil, Literal::Nil) => {},
                    (Kind::Bool(actual), Literal::Bool(expected)) => assert_eq!(actual, expected),
                    (Kind::Long(actual), Literal::Long(expected)) => assert_eq!(actual, expected),
                    (Kind::Number(actual), Literal::Number(expected)) => assert_eq!(actual.to_bits(), expected.to_bits()),
                    (Kind::String(actual), Literal::String(expected)) => assert_eq!(actual, expected),
                    other => panic!("scalar transport changed category: {other:?}"),
                }
            }
        }
    }

    #[test]
    fn native_transport_uid_collisions_preserve_distinct_functions_and_repeated_aliases() {
        let mut source = Session::new_repl().unwrap();
        let mut target = Session::new_macro().unwrap();
        let left = source.eval("(def left (fn [] 1)) left").unwrap();
        let right = source.eval("(def right (fn [] 2)) right").unwrap();
        let _retained = source.export_value(&left).unwrap();
        // Deliberately collide the private UID. It may narrow an index lookup,
        // but it must never authenticate a different function object.
        {
            let mut roots = RootScope::new(&mut source.store);
            let a = left.object().unwrap().to_rooted(&mut roots).as_struct(&roots).unwrap().unwrap();
            let b = right.object().unwrap().to_rooted(&mut roots).as_struct(&roots).unwrap().unwrap();
            let uid = a.field(&mut roots, runtime_abi::CLOSURE_UID as usize).unwrap();
            b.set_field(&mut roots, runtime_abi::CLOSURE_UID as usize, uid).unwrap();
        }
        let original = source.eval("[left right left]").unwrap();
        let imported = target.import_value(source.export_value(&original).unwrap()).unwrap();
        let inspect = target.eval("(fn [v] (if (and (not (identical? (nth v 0) (nth v 1))) (identical? (nth v 0) (nth v 2)) (= 1 ((nth v 0))) (= 2 ((nth v 1)))) 42 -1))").unwrap();
        for _ in 0..2 {
            source.collect().unwrap();
            target.collect().unwrap();
            let result = super::super::super::suspended::scope(&mut source, || target.invoke(&inspect, &[&imported])).unwrap();
            assert_eq!(target.inspect(&result, |mut store, value| runtime_number(&mut store, &value)).unwrap(), Some(RuntimeNumber::Long(42)));
        }
        let returned = source.import_value(target.export_value(&imported).unwrap()).unwrap();
        assert!(source.identical_values(&original, &returned).unwrap());
    }

    #[test]
    fn native_function_capability_nested_immutable_aliases_keep_real_roots() {
        let mut source = Session::new_repl().unwrap();
        let mut target = Session::new_macro().unwrap();
        let maker = source.eval(
            "(def immutable-maker (let [v (with-meta [42] {:tag :kept})] (fn [] v))) immutable-maker"
        ).unwrap();
        let original = source.eval(
            "(with-meta (fn [] 1) {:item (immutable-maker) :again (immutable-maker)})"
        ).unwrap();
        let inspect = target.eval(
            "(fn [f maker] (let [m (meta f) a (:item m) b (:again m) returned (maker)] (if (and (identical? a b) (identical? a returned) (identical? (meta a) (meta returned)) (= a returned)) 42 -1)))"
        ).unwrap();
        let read = target.eval("(fn [f] (:item (meta f)))").unwrap();
        let compiler_sizes = |session: &Session| {
            let compiler = registry_lock(&session.compiler);
            (compiler.cells.len(), compiler.native_function_cells.len())
        };
        let before = (compiler_sizes(&source), compiler_sizes(&target));
        let imported = target.import_value(source.export_value(&original).unwrap()).unwrap();
        let imported_maker = target.import_value(source.export_value(&maker).unwrap()).unwrap();
        for _ in 0..2 {
            source.collect().unwrap();
            target.collect().unwrap();
            let result = super::super::super::suspended::scope(&mut source, || {
                target.invoke(&inspect, &[&imported, &imported_maker])
            }).unwrap();
            assert_eq!(
                target.inspect(&result, |mut store, value| runtime_number(&mut store, &value)).unwrap(),
                Some(RuntimeNumber::Long(42)),
                "metadata aliases and a real callback return must retain the same immutable object and metadata"
            );
        }
        assert_eq!((compiler_sizes(&source), compiler_sizes(&target)), before);
        let returned = target.invoke(&read, &[&imported]).unwrap();
        let restored = source.import_value(target.export_value(&returned).unwrap()).unwrap();
        let actual = source.invoke(&maker, &[]).unwrap();
        assert!(source.identical_values(&restored, &actual).unwrap(),
            "return to the source must unwrap the exact immutable object");
        let endpoint = target.native_value_endpoint(&imported).unwrap().unwrap();
        let weak = Arc::downgrade(&endpoint);
        let NativeKind::Function { metadata: Some(Transfer::Native(metadata)) } = &endpoint.kind else {
            panic!("actual metadata endpoint");
        };
        let metadata = Arc::downgrade(metadata);
        let nested = Arc::downgrade(&target.native_value_endpoint(&returned).unwrap().unwrap());
        drop(endpoint);
        drop(original);
        drop(imported);
        drop(returned);
        source.collect().unwrap();
        target.collect().unwrap();
        assert!(weak.upgrade().is_none());
        assert!(metadata.upgrade().is_none());
        assert!(nested.upgrade().is_none(),
            "scoped immutable roots must not become a persistent compiler cache");
    }

    #[test]
    fn native_function_capability_nested_metadata_uses_scoped_roots() {
        let mut source = Session::new_repl().unwrap();
        let mut target = Session::new_macro().unwrap();
        let original = source.eval(
            "(let [counter (atom 41) g (fn [] (swap! counter inc))] (with-meta (fn [] 1) {:callback g :again g}))"
        ).unwrap();
        let inspect = target.eval(
            "(fn [a b] (let [g (:callback (meta a)) h (:again (meta b))] (if (and (identical? (meta a) (meta b)) (identical? g h) (= (hash g) (hash h))) (g) -1)))"
        ).unwrap();
        let compiler_sizes = |session: &Session| {
            let compiler = registry_lock(&session.compiler);
            (compiler.cells.len(), compiler.native_function_cells.len())
        };
        let source_before = compiler_sizes(&source);
        let target_before = compiler_sizes(&target);
        let first = target
            .import_value(source.export_value(&original).unwrap())
            .unwrap();
        let second = target
            .import_value(source.export_value(&original).unwrap())
            .unwrap();
        assert_eq!(
            compiler_sizes(&source),
            source_before,
            "transport must not retain nested source roots in the compiler constant pool"
        );
        assert_eq!(
            compiler_sizes(&target),
            target_before,
            "destination transport constants must remain scoped"
        );
        for expected in [42, 43] {
            source.collect().unwrap();
            target.collect().unwrap();
            let result = super::super::super::suspended::scope(&mut source, || {
                target.invoke(&inspect, &[&first, &second])
            })
            .unwrap();
            assert_eq!(
                target
                    .inspect(&result, |mut store, value| runtime_number(
                        &mut store, &value
                    ))
                    .unwrap(),
                Some(RuntimeNumber::Long(expected))
            );
        }
        let endpoint = target.native_value_endpoint(&first).unwrap().unwrap();
        let function = Arc::downgrade(&endpoint);
        let NativeKind::Function {
            metadata: Some(Transfer::Native(metadata)),
        } = &endpoint.kind
        else {
            panic!("actual metadata endpoint");
        };
        let NativeKind::Materialized { constants, .. } = &metadata.kind else {
            panic!("materialized metadata");
        };
        let nested: Vec<_> = constants
            .values()
            .filter_map(|value| match value {
                Transfer::Native(endpoint) => Some(Arc::downgrade(endpoint)),
                _ => None,
            })
            .collect();
        assert_eq!(
            nested.len(),
            1,
            "repeated nested function aliases share one transport root"
        );
        let metadata = Arc::downgrade(metadata);
        drop(endpoint);
        drop(original);
        drop(first);
        drop(second);
        target.collect().unwrap();
        source.collect().unwrap();
        assert!(function.upgrade().is_none());
        assert!(
            metadata.upgrade().is_none(),
            "nested transport must not become a persistent metadata root"
        );
        // The compiled `(g)` call above has a normal #378 last-callee cache.
        // Its latest callee remains an independent owner after the transport
        // bundle is gone. Replacing that callee must release the old endpoint;
        // transport must not create another persistent per-value owner.
        assert!(nested.iter().all(|root| root.upgrade().is_some()));
        let replacement = target
            .eval("(let [g (fn [] 0)] (with-meta (fn [] 1) {:callback g :again g}))")
            .unwrap();
        let value = target
            .invoke(&inspect, &[&replacement, &replacement])
            .unwrap();
        assert_eq!(
            target
                .inspect(&value, |mut store, value| runtime_number(
                    &mut store, &value
                ))
                .unwrap(),
            Some(RuntimeNumber::Long(0))
        );
        target.collect().unwrap();
        source.collect().unwrap();
        assert!(
            nested.into_iter().all(|root| root.upgrade().is_none()),
            "replacing the independent last-callee cache must release every old nested endpoint"
        );
        let opaque = source
            .eval("(with-meta (fn [] 1) {:opaque (atom 42)})")
            .unwrap();
        assert!(
            source.export_value(&opaque).is_err(),
            "nested opaque mutable data must retain its existing rejection"
        );
    }

    #[test]
    fn native_function_capability_live_var_namespace_and_state_keep_real_owners_without_public_eval_binding() {
        let mut source = Session::new_repl().unwrap();
        let mut target = Session::new_macro().unwrap();
        let original = source.eval("(def live-forwarded-var 11)").unwrap();
        let callback = source.eval(
            "(fn [] (binding [*ns* (:ns (meta #'live-forwarded-var))] (suss.bootstrap/evaluate-form '(def live-forwarded-var 42))))"
        ).unwrap();
        let read = target.eval(
            "(fn [v f] (let [before @v ns (:ns (meta v))] (f) (binding [*ns* ns] (alias 'live-forwarded-alias 'user)) (if (and (var? v) (= before 11) (= @v 42) (not (.isMacro v)) (identical? (meta v) (meta v)) (identical? ns (:ns (meta v))) (= 'user (ns-name ns))) 42 -1)))"
        ).unwrap();
        let imported = target
            .import_value(source.export_value(&original).unwrap())
            .unwrap();
        let callback = target
            .import_value(source.export_value(&callback).unwrap())
            .unwrap();
        source.collect().unwrap();
        target.collect().unwrap();
        let result = super::super::super::suspended::scope(&mut source, || {
            target.invoke(&read, &[&imported, &callback])
        })
        .unwrap_or_else(|error| {
            panic!(
                "{}",
                crate::portable_repl::error_display(&mut target, &error)
            )
        });
        assert_eq!(
            target
                .inspect(&result, |mut store, value| runtime_number(
                    &mut store, &value
                ))
                .unwrap(),
            Some(RuntimeNumber::Long(42))
        );
        let alias = source
            .eval("(contains? (ns-aliases *ns*) 'live-forwarded-alias)")
            .unwrap();
        assert_eq!(
            source
                .inspect(&alias, |mut store, value| sentinel_tag(&mut store, &value))
                .unwrap(),
            Some(runtime_abi::TRUE as u32),
            "namespace writes must reach the actual original state array"
        );
        let returned = source
            .import_value(target.export_value(&imported).unwrap())
            .unwrap();
        let same = source.eval("(fn [a b] (identical? a b))").unwrap();
        let same = source.invoke(&same, &[&original, &returned]).unwrap();
        assert_eq!(
            source
                .inspect(&same, |mut store, value| sentinel_tag(&mut store, &value))
                .unwrap(),
            Some(runtime_abi::TRUE as u32),
            "return transport must unwrap the exact original Var"
        );
    }

    #[test]
    fn native_function_capability_live_arrays_copy_clone_and_retire_without_fake_storage() {
        fn runtime(
            target: &mut Session,
            name: &str,
            values: &[&SessionValue],
        ) -> Result<SessionValue, SessionError> {
            target.budget()?;
            target.check_data_input_owners(values)?;
            let mut roots = RootScope::new(&mut target.store);
            let params = values
                .iter()
                .map(|value| value.val(&mut roots.as_context_mut()))
                .collect::<Vec<_>>();
            let function = target.runtime.get_func(&mut roots, name).unwrap();
            call(
                &mut roots,
                target.runtime,
                function,
                &params,
                target.identity,
                &target.handles,
            )
        }
        let mut source = Session::new_repl().unwrap();
        let mut target = Session::new_macro().unwrap();
        let original = source.eval("(object-array [11 22 33 44])").unwrap();
        let first = target
            .import_value(source.export_value(&original).unwrap())
            .unwrap();
        let second = target
            .import_value(source.export_value(&original).unwrap())
            .unwrap();
        let same = target
            .eval("(fn [a b] (and (identical? a b) (= 4 (alength a)) (= 11 (aget a 0))))")
            .unwrap();
        let same = super::super::super::suspended::scope(&mut source, || {
            target.invoke(&same, &[&first, &second])
        })
        .unwrap();
        assert_eq!(
            target
                .inspect(&same, |mut store, value| sentinel_tag(&mut store, &value))
                .unwrap(),
            Some(runtime_abi::TRUE as u32)
        );
        let hinted = target.eval("(fn [^objects a] (let [before (aget ^objects a 0)] (aset ^objects a 0 before) before))").unwrap();
        let hinted = super::super::super::suspended::scope(&mut source, || {
            target.invoke(&hinted, &[&first])
        })
        .unwrap();
        assert_eq!(
            target
                .inspect(&hinted, |mut store, value| runtime_number(
                    &mut store, &value
                ))
                .unwrap(),
            Some(RuntimeNumber::Long(11))
        );
        let elements = super::super::super::suspended::scope(&mut source, || {
            runtime(&mut target, "object-array-elements", &[&first])
        })
        .unwrap();
        target
            .inspect(&elements, |mut store, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                assert_eq!(array.len(&store)?, 4);
                for (i, expected) in [11, 22, 33, 44].into_iter().enumerate() {
                    let value = array.get(&mut store, i as u32)?;
                    assert_eq!(
                        runtime_number(&mut store, &value)?,
                        Some(RuntimeNumber::Long(expected))
                    );
                }
                Ok(())
            })
            .unwrap();
        let clone = super::super::super::suspended::scope(&mut source, || {
            runtime(&mut target, "packed-array-clone", &[&first])
        })
        .unwrap();
        let zero = target
            .data_scalar(&crate::portable::hir::Literal::Long(0))
            .unwrap();
        let one = target
            .data_scalar(&crate::portable::hir::Literal::Long(1))
            .unwrap();
        let three = target
            .data_scalar(&crate::portable::hir::Literal::Long(3))
            .unwrap();
        let four = target
            .data_scalar(&crate::portable::hir::Literal::Long(4))
            .unwrap();
        let changed = target
            .data_scalar(&crate::portable::hir::Literal::Long(99))
            .unwrap();
        super::super::super::suspended::scope(&mut source, || {
            runtime(&mut target, "packed-array-set", &[&clone, &zero, &changed])
        })
        .unwrap();
        super::super::super::suspended::scope(&mut source, || {
            runtime(
                &mut target,
                "packed-array-copy",
                &[&first, &zero, &second, &one, &three],
            )
        })
        .unwrap();
        let verify = target.eval("(fn [a b] (and (not (identical? a b)) (= 11 (aget a 0)) (= 11 (aget a 1)) (= 22 (aget a 2)) (= 33 (aget a 3)) (= 99 (aget b 0)) (= 22 (aget b 1))))").unwrap();
        for _ in 0..2 {
            source.collect().unwrap();
            target.collect().unwrap();
            let valid = super::super::super::suspended::scope(&mut source, || {
                target.invoke(&verify, &[&first, &clone])
            })
            .unwrap();
            assert_eq!(
                target
                    .inspect(&valid, |mut store, value| sentinel_tag(&mut store, &value))
                    .unwrap(),
                Some(runtime_abi::TRUE as u32)
            );
        }
        let error = super::super::super::suspended::scope(&mut source, || {
            runtime(
                &mut target,
                "packed-array-copy",
                &[&first, &four, &clone, &zero, &one],
            )
        })
        .unwrap_err();
        assert!(
            matches!(&error, SessionError::Trap(error) if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::ArrayOutOfBounds))
        );
        let valid = super::super::super::suspended::scope(&mut source, || {
            target.invoke(&verify, &[&first, &clone])
        })
        .unwrap();
        assert_eq!(
            target
                .inspect(&valid, |mut store, value| sentinel_tag(&mut store, &value))
                .unwrap(),
            Some(runtime_abi::TRUE as u32),
            "invalid copy ranges must not write a prefix"
        );
        let returned = source
            .import_value(target.export_value(&first).unwrap())
            .unwrap();
        let same = source.eval("(fn [a b] (identical? a b))").unwrap();
        let same = source.invoke(&same, &[&original, &returned]).unwrap();
        assert_eq!(
            source
                .inspect(&same, |mut store, value| sentinel_tag(&mut store, &value))
                .unwrap(),
            Some(runtime_abi::TRUE as u32)
        );
        source.reset().unwrap();
        let error = super::super::super::suspended::scope(&mut source, || {
            runtime(&mut target, "packed-array-get", &[&first, &zero])
        })
        .unwrap_err();
        assert!(crate::portable_repl::error_display(&mut target, &error).contains("retired"));
        assert!(target.eval("(+ 40 2)").is_ok());
    }

    #[test]
    fn native_function_capability_transport_reader_does_not_recapture_its_root() {
        let mut source = Session::new_repl().unwrap();
        let value = source.eval("#'identity").unwrap();
        let state = source.value_state().unwrap();
        let bridge = state.bridge(&mut source).unwrap();
        let runtime = source.runtime;
        let closure_test = runtime
            .get_func(&mut source.store, "closure-value-test")
            .unwrap();
        let class_test = runtime
            .get_func(&mut source.store, "class-value-is")
            .unwrap();
        let instance_test = runtime
            .get_func(&mut source.store, "object-instance")
            .unwrap();
        let identity_test = runtime
            .get_func(&mut source.store, "native-identical-test")
            .unwrap();
        let var_class = registry_lock(&state.var_class).clone().unwrap();
        let unbound_class = registry_lock(&state.unbound_class).clone().unwrap();
        let meta_fn_class = registry_lock(&state.meta_fn_class).clone().unwrap();
        let namespace_class = registry_lock(&state.namespace)
            .as_ref()
            .unwrap()
            .namespace
            .clone();
        let array_test = source
            .runtime
            .get_func(&mut source.store, "object-array-test")
            .unwrap();
        let owner = source.identity;
        let result = bridge.read_transport(
            &mut source,
            &value,
            owner,
            closure_test,
            class_test,
            instance_test,
            identity_test,
            var_class,
            unbound_class,
            meta_fn_class,
            namespace_class,
            array_test,
        );
        assert!(
            matches!(result, Err(SessionError::Compile(_))),
            "an opaque root must not recursively capture/re-export itself"
        );
        assert!(source.eval("(+ 40 2)").is_ok());
    }

    #[test]
    fn native_function_capability_scoped_tokens_require_actual_destination_roots() {
        let mut source = Session::new_repl().unwrap();
        let mut target = Session::new_macro().unwrap();
        let foreign = source.eval("(fn [] 42)").unwrap();
        let owner = source.identity;
        let form = Form {
            span: 0..0,
            metadata: vec![],
            kind: Kind::NativeConstant { owner, index: 1 },
        };
        let bridge = target.value_state().unwrap().bridge(&mut target).unwrap();
        let values = BTreeMap::from([(1, foreign)]);
        assert!(matches!(
            bridge.quote_transport_ref(&mut target, &form, owner, &values),
            Err(SessionError::ForeignValue)
        ));
        assert!(matches!(
            bridge.quote_transport_ref(&mut target, &form, owner + 1, &values),
            Err(SessionError::Compile(_))
        ));
        assert!(matches!(
            bridge.quote_transport_ref(&mut target, &form, owner, &BTreeMap::new()),
            Err(SessionError::Compile(_))
        ));
        assert!(target.eval("(+ 40 2)").is_ok());
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
