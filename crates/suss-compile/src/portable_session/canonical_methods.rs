//! Initial source-class/schema witnesses and canonical IHash installations.
//! This is not source-data certification and issues no memo receipts.
use super::{Session, SessionError, SessionValue, call, own, storage_snapshot::StorageSnapshot};
use crate::portable::bootstrap;
use std::sync::{Arc, atomic::AtomicUsize};
use wasmtime::{AnyRef, AsContextMut, Instance, RootScope, Rooted, StoreContextMut, Val};

type Result<T> = std::result::Result<T, SessionError>;

struct Method {
    owner: &'static str,
    slot: usize,
    descriptor: SessionValue,
    implementation: SessionValue,
    table: SessionValue,
    implementation_index: u32,
    schema: StorageSnapshot,
    closure: StorageSnapshot,
}

// Schema-only snapshots deliberately exclude descriptor protocol tables,
// constructor properties/prototypes, UID and memo state. They establish no
// authority for those independently mutable surfaces.
struct SourceClass {
    owner: &'static str,
    constructor: Option<SessionValue>,
    descriptor: SessionValue,
    schema: StorageSnapshot,
}
fn descriptor_field(
    session: &mut Session,
    descriptor: &SessionValue,
    index: usize,
    handles: &Arc<AtomicUsize>,
) -> Result<SessionValue> {
    session.check(descriptor)?;
    session.charge_host_work(1)?;
    let mut scope = RootScope::new(&mut session.store);
    let descriptor = descriptor
        .value
        .to_rooted(&mut scope)
        .as_struct(&scope)?
        .ok_or_else(|| invalid("Source class needs a descriptor"))?;
    let value = descriptor.field(&mut scope, index)?;
    own(&mut scope, value, session.identity, handles)
}
fn same_reference(
    session: &mut Session,
    left: &SessionValue,
    right: &SessionValue,
) -> Result<bool> {
    session.check(left)?;
    session.check(right)?;
    session.charge_host_work(1)?;
    let mut scope = RootScope::new(&mut session.store);
    let left = left.value.to_rooted(&mut scope);
    let right = right.value.to_rooted(&mut scope);
    Ok(Rooted::<AnyRef>::ref_eq(&scope, &left, &right)?)
}
fn capture_source_classes(
    session: &mut Session,
    handles: &Arc<AtomicUsize>,
    #[cfg(test)] interrupt_after: Option<usize>,
) -> Result<Vec<SourceClass>> {
    let count = crate::portable_macro_data::source_class_names().len() + 1;
    let bytes = count
        .checked_mul(std::mem::size_of::<SourceClass>())
        .ok_or_else(|| invalid("Source class allocation overflow"))?;
    session.charge_host_work(bytes as u64 + 1)?;
    let mut classes = Vec::new();
    classes
        .try_reserve_exact(count)
        .map_err(|_| invalid("Source class allocation failed"))?;
    for owner in crate::portable_macro_data::source_class_names() {
        let constructor = binding(session, "suss.core", owner, handles)?;
        let descriptor = runtime(session, "constructor-descriptor", &[&constructor], handles)?;
        runtime(
            session,
            "nominal-source-constructor-plan",
            &[&descriptor],
            handles,
        )?;
        let fields = descriptor_field(session, &descriptor, 1, handles)?;
        let schema = session.capture_storage_accounted(&fields, handles.clone())?;
        session.charge_host_work(1)?;
        classes.push(SourceClass {
            owner,
            constructor: Some(constructor),
            descriptor,
            schema,
        });
        #[cfg(test)]
        if interrupt_after == Some(classes.len()) {
            session
                .interrupt
                .store(true, std::sync::atomic::Ordering::SeqCst);
            session.charge_host_work(0)?;
        }
    }
    // The sample uses the private native source-array factory, not a public
    // array binding. No caller has access during this provisioning interval.
    let sample = session.data_array(&[])?;
    let descriptor = descriptor_field(session, &sample, 0, handles)?;
    let fields = descriptor_field(session, &descriptor, 1, handles)?;
    let schema = session.capture_storage_accounted(&fields, handles.clone())?;
    session.charge_host_work(1)?;
    classes.push(SourceClass {
        owner: "SourceArray",
        constructor: None,
        descriptor,
        schema,
    });
    Ok(classes)
}

pub(super) struct InspectionContext<'a> {
    pub store: StoreContextMut<'a, ()>,
    pub identity: u64,
    pub runtime: Instance,
    pub interrupt: &'a std::sync::atomic::AtomicBool,
}

pub(super) struct CanonicalMethods {
    identity: u64,
    methods: Vec<Method>,
    source_classes: Vec<SourceClass>,
    key: SessionValue,
    key_storage: StorageSnapshot,
    // Internal roots must not inflate the public external-value-handle count.
    handles: Arc<AtomicUsize>,
}

fn invalid(message: &'static str) -> SessionError {
    SessionError::Host(wasmtime::Error::msg(message))
}

pub(super) fn binding(
    session: &mut Session,
    namespace: &str,
    name: &str,
    handles: &Arc<AtomicUsize>,
) -> Result<SessionValue> {
    session.charge_host_work(
        session.cells.len() as u64 + namespace.len() as u64 + name.len() as u64 + 3,
    )?;
    let global = session
        .cells
        .iter()
        .find(|(identity, _)| {
            identity.phase() == session.phase
                && identity.namespace() == namespace
                && identity.name() == name
        })
        .map(|(_, value)| *value)
        .ok_or_else(|| invalid("Missing canonical method binding"))?;
    let mut scope = RootScope::new(&mut session.store);
    let cell = global
        .get(&mut scope)
        .unwrap_anyref()
        .ok_or_else(|| invalid("Null canonical binding cell"))?
        .as_struct(&scope)?
        .ok_or_else(|| invalid("Invalid canonical binding cell"))?;
    if cell.field(&mut scope, 1)?.unwrap_i32() == 0 {
        return Err(invalid("Unbound canonical method binding"));
    }
    let value = cell.field(&mut scope, 0)?;
    own(&mut scope, value, session.identity, handles)
}

pub(super) fn runtime(
    session: &mut Session,
    name: &str,
    arguments: &[&SessionValue],
    handles: &Arc<AtomicUsize>,
) -> Result<SessionValue> {
    session.charge_host_work(arguments.len() as u64 + 1)?;
    for argument in arguments {
        session.check(argument)?;
    }
    let mut scope = RootScope::new(&mut session.store);
    let arguments: Vec<_> = arguments
        .iter()
        .map(|value| Val::AnyRef(Some(value.value.to_rooted(&mut scope))))
        .collect();
    let function = session
        .runtime
        .get_func(&mut scope, name)
        .ok_or_else(|| invalid("Missing canonical method adapter"))?;
    call(
        &mut scope,
        session.runtime,
        function,
        &arguments,
        session.identity,
        handles,
    )
}

fn schema(
    session: &mut Session,
    descriptor: &SessionValue,
    slot: usize,
    handles: &Arc<AtomicUsize>,
) -> Result<SessionValue> {
    session.check(descriptor)?;
    session.charge_host_work(12)?;
    let mut scope = RootScope::new(&mut session.store);
    let descriptor = descriptor.value.to_rooted(&mut scope);
    let value = schema_value(&mut scope, descriptor, slot)?;
    own(&mut scope, value, session.identity, handles)
}

fn schema_value<C: AsContextMut<Data = ()>>(
    scope: &mut RootScope<C>,
    descriptor: Rooted<AnyRef>,
    slot: usize,
) -> Result<Val> {
    let descriptor = descriptor
        .as_struct(&*scope)?
        .ok_or_else(|| invalid("Invalid canonical class descriptor"))?;
    let value = descriptor.field(&mut *scope, 1)?;
    let fields = value
        .unwrap_anyref()
        .ok_or_else(|| invalid("Null canonical schema"))?
        .as_array(&*scope)?
        .ok_or_else(|| invalid("Invalid canonical schema"))?;
    if slot >= fields.len(&*scope)? as usize {
        return Err(invalid("Canonical memo slot is outside its schema"));
    }
    let name = fields.get(&mut *scope, slot as u32)?;
    let name = name
        .unwrap_anyref()
        .ok_or_else(|| invalid("Null canonical memo field name"))?
        .as_array(&*scope)?
        .ok_or_else(|| invalid("Invalid canonical memo field name"))?;
    if !matches!(name.len(&*scope)?, 5 | 6) {
        return Err(invalid("Wrong canonical memo field name"));
    }
    let units: Vec<_> = name
        .elems(&mut *scope)?
        .map(|unit| match unit {
            Val::I32(unit) if (0..=u16::MAX as i32).contains(&unit) => Ok(unit as u16),
            _ => Err(invalid("Invalid canonical memo field code unit")),
        })
        .collect::<Result<_>>()?;
    if units != "__hash".encode_utf16().collect::<Vec<_>>()
        && units != "_hash".encode_utf16().collect::<Vec<_>>()
    {
        return Err(invalid("Wrong canonical memo field name"));
    }
    Ok(value)
}

impl CanonicalMethods {
    // Called only inside the authorized provisioning interval. Never refresh
    // this registry by reading mutable globals after returning a Session.
    pub(super) fn capture(
        session: &mut Session,
        authority: &bootstrap::IntegrityAuthority,
    ) -> Result<Self> {
        if session.bootstrap_core || session.canonical_methods.is_some() {
            return Err(invalid(
                "Canonical methods can only be captured during initial provisioning",
            ));
        }
        Self::capture_accounted(
            session,
            authority,
            Arc::new(AtomicUsize::new(0)),
            #[cfg(test)]
            None,
        )
    }

    fn capture_accounted(
        session: &mut Session,
        authority: &bootstrap::IntegrityAuthority,
        handles: Arc<AtomicUsize>,
        #[cfg(test)] interrupt_after: Option<usize>,
    ) -> Result<Self> {
        if !std::ptr::eq(
            authority,
            bootstrap::shipped_authority(session.phase).map_err(SessionError::Compile)?,
        ) {
            return Err(invalid(
                "Canonical method capture needs this phase's shipped authority",
            ));
        }
        let _request = session.operation_budget_scope()?;
        let key_name = format!(
            "key-{}",
            "suss.core/IHash/-hash/1"
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        let key = binding(session, "suss.internal.protocol-keys", &key_name, &handles)?;
        let key_storage = session.capture_storage_accounted(&key, handles.clone())?;
        let count = bootstrap::memo_owner_slots().count();
        let bytes = count.checked_mul(std::mem::size_of::<Method>())
            .and_then(|bytes| bytes.checked_add(count.checked_mul(2)?))
            .and_then(|bytes| bytes.checked_add(1))
            .ok_or_else(|| invalid("Canonical method allocation overflow"))?;
        session.charge_host_work(u64::try_from(bytes)
            .map_err(|_| invalid("Canonical method allocation overflow"))?)?;
        let mut methods = Vec::new();
        methods.try_reserve_exact(count)
            .map_err(|_| invalid("Canonical method allocation failed"))?;
        for (owner, slot) in bootstrap::memo_owner_slots() {
            session.charge_host_work(1)?;
            let class = binding(session, "suss.core", owner, &handles)?;
            let descriptor = runtime(session, "constructor-descriptor", &[&class], &handles)?;
            // Preserve the runtime's descriptor/schema validity checks.
            runtime(
                session,
                "nominal-source-constructor-plan",
                &[&descriptor],
                &handles,
            )?;
            let implementation = runtime(
                session,
                "protocol-method-get",
                &[&descriptor, &key],
                &handles,
            )?;
            runtime(session, "closure-environment", &[&implementation], &handles)?;
            // Bind the first exact key selected by protocol-method-get while
            // private shipped provisioning still owns the installation.
            let table = descriptor_field(session, &descriptor, 2, &handles)?;
            session.charge_host_work(5)?;
            let implementation_index = {
                let mut scope = RootScope::new(&mut session.store);
                let array = table.value.to_rooted(&mut scope).as_array(&scope)?
                    .ok_or_else(|| invalid("Canonical method table requires array storage"))?;
                let len = array.len(&scope)?;
                if len % 2 != 0 { return Err(invalid("Invalid canonical method table length")); }
                let key_root = key.value.to_rooted(&mut scope);
                let implementation_root = implementation.value.to_rooted(&mut scope);
                let mut found = None;
                for index in (0..len).step_by(2) {
                    Session::consume_host_work(scope.as_context_mut(), 6, &session.interrupt)?;
                    let candidate = array.get(&mut scope, index)?;
                    let Some(candidate) = candidate.anyref().flatten() else { continue; };
                    if Rooted::<AnyRef>::ref_eq(&scope, candidate, &key_root)? {
                        let value_index = index.checked_add(1)
                            .ok_or_else(|| invalid("Canonical method index overflow"))?;
                        let selected = array.get(&mut scope, value_index)?;
                        let Some(selected) = selected.anyref().flatten() else {
                            return Err(invalid("Canonical method implementation is null"));
                        };
                        if !Rooted::<AnyRef>::ref_eq(&scope, selected, &implementation_root)? {
                            return Err(invalid("Canonical method selection changed"));
                        }
                        found = Some(value_index);
                        break;
                    }
                }
                found.ok_or_else(|| invalid("Canonical method key is absent"))?
            };
            let fields = schema(session, &descriptor, slot, &handles)?;
            let schema = session.capture_storage_accounted(&fields, handles.clone())?;
            let closure = session.capture_storage_accounted(&implementation, handles.clone())?;
            methods.push(Method {
                owner,
                slot,
                descriptor,
                implementation,
                table,
                implementation_index,
                schema,
                closure,
            });
            #[cfg(test)]
            if interrupt_after == Some(methods.len()) {
                session
                    .interrupt
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                session.charge_host_work(0)?;
            }
        }
        // This remains inside capture_accounted's phase-qualified shipped
        // authority interval. Stage all class roots before registry publication.
        let source_classes = capture_source_classes(
            session,
            &handles,
            #[cfg(test)]
            None,
        )?;
        session.charge_host_work(0)?;
        Ok(Self {
            identity: session.identity,
            methods,
            source_classes,
            key,
            key_storage,
            handles,
        })
    }

    pub(super) fn source_constructor_roots(&self) -> impl Iterator<Item = &SessionValue> {
        self.source_classes.iter().filter_map(|class| class.constructor.as_ref())
    }
    pub(super) fn source_constructor_matches(&self, session: &mut Session, constructor: &SessionValue, descriptor: &SessionValue) -> Result<bool> {
        for class in &self.source_classes {
            session.charge_host_work(1)?;
            if let Some(initial) = &class.constructor {
                if same_reference(session, initial, constructor)? {
                    return self.source_class_matches(session, class.owner, Some(constructor), descriptor);
                }
            }
        }
        Ok(false)
    }
    fn source_class_matches(
        &self,
        session: &mut Session,
        owner: &str,
        constructor: Option<&SessionValue>,
        descriptor: &SessionValue,
    ) -> Result<bool> {
        self.source_class_matches_at(session, owner, constructor, descriptor, None)
    }
    // An ordinal is only a lookup hint. Its stored owner must match the supplied
    // name; arbitrary/reordered inputs retain the original metered name lookup.
    fn source_class_matches_at(
        &self, session: &mut Session, owner: &str,
        constructor: Option<&SessionValue>, descriptor: &SessionValue,
        ordinal: Option<usize>,
    ) -> Result<bool> {
        session.check(descriptor)?;
        if let Some(constructor) = constructor {
            session.check(constructor)?;
        }
        let _request = session.operation_budget_scope()?;
        if self.identity != session.identity {
            return Err(SessionError::ForeignValue);
        }
        session.charge_host_work(1)?;
        let mut found = None;
        if let Some(index) = ordinal {
            session.charge_host_work(1)?;
            if let Some(class) = self.source_classes.get(index) {
                session.charge_host_work(owner.len() as u64 + class.owner.len() as u64 + 1)?;
                if class.owner == owner { found = Some(class); }
            }
        }
        if found.is_none() {
            // Unknown names and untrusted/wrong ordinals retain full lookup.
            for class in &self.source_classes {
                session.charge_host_work(owner.len() as u64 + class.owner.len() as u64 + 1)?;
                if class.owner == owner {
                    found = Some(class);
                    break;
                }
            }
        }
        let Some(expected) = found else {
            return Ok(false);
        };
        if !same_reference(session, descriptor, &expected.descriptor)? {
            return Ok(false);
        }
        match (constructor, &expected.constructor) {
            (Some(current), Some(initial)) => {
                if !same_reference(session, current, initial)? {
                    return Ok(false);
                }
                // Same closure identity alone is insufficient: its reachable
                // constructor-owner storage can have been changed by a caller.
                let current_descriptor =
                    runtime(session, "constructor-descriptor", &[current], &self.handles)?;
                if !same_reference(session, &current_descriptor, &expected.descriptor)? {
                    return Ok(false);
                }
            }
            (None, None) => {}
            _ => return Ok(false),
        }
        let fields = descriptor_field(session, descriptor, 1, &self.handles)?;
        session.storage_matches(&fields, &expected.schema)
    }

    /// Installation eligibility only. Actual source storage and authenticated
    /// memo writes still require their separate checks before candidate reuse.
    pub(super) fn matches(
        &self,
        session: &mut Session,
        owner: &str,
        descriptor: &SessionValue,
    ) -> Result<bool> {
        session.check(descriptor)?;
        let _request = session.operation_budget_scope()?;
        let interrupt = session.host_work_interrupt();
        let identity = session.identity;
        let runtime = session.runtime;
        let mut scope = RootScope::new(&mut session.store);
        let descriptor = Val::AnyRef(Some(descriptor.value.to_rooted(&mut scope)));
        self.matches_in_context(
            InspectionContext {
                store: scope.as_context_mut(),
                identity,
                runtime,
                interrupt: &interrupt,
            },
            owner,
            descriptor,
        )
    }

    /// Bind a protected memo site using initial class authority, never a public
    /// name currently rebound by a caller. Custom classes receive no receipt.
    pub(super) fn constructor_memo_site(
        &self, session: &mut Session, constructor: &SessionValue, descriptor: &SessionValue,
    ) -> Result<Option<(usize, &'static str, usize)>> {
        if !self.source_constructor_matches(session, constructor, descriptor)? {
            return Ok(None);
        }
        for (site, (owner, slot)) in bootstrap::memo_owner_slots().enumerate() {
            session.charge_host_work(1)?;
            let Some(class) = self.source_classes.iter().find(|class| class.owner == owner) else { continue };
            // Registration records a method obligation even while the live method
            // differs. Write-time checks decide whether it can authorize a memo.
            if same_reference(session, descriptor, &class.descriptor)? {
                return Ok(Some((site, owner, slot)));
            }
        }
        Ok(None)
    }

    /// No budget scope/refill: host callbacks debit their active caller Store.
    // Read-only recognition of an actual dispatcher environment. Never performs
    // method lookup or invokes code; malformed/non-IHash environments decline.
    pub(super) fn ihash_dispatch_matches_in_context(
        &self, context: InspectionContext<'_>, environment: &Val, legacy: bool,
    ) -> Result<bool> {
        if context.identity != self.identity { return Err(SessionError::ForeignValue); }
        let mut scope = RootScope::new(context.store);
        Session::consume_host_work(scope.as_context_mut(), 4, context.interrupt)?;
        let Some(value) = environment.anyref().flatten() else { return Ok(false); };
        let Some(array) = value.as_array(&scope)? else { return Ok(false); };
        if array.len(&scope)? != if legacy { 1 } else { 2 } { return Ok(false); }
        let found = array.get(&mut scope, 0)?;
        let Some(found) = found.anyref().flatten() else { return Ok(false); };
        let key = self.key.value.to_rooted(&mut scope);
        if !Rooted::<AnyRef>::ref_eq(&scope, found, &key)? { return Ok(false); }
        self.key_storage.matches_in_context(scope.as_context_mut(),
            Val::AnyRef(Some(key)), context.identity, context.interrupt)
    }
    pub(super) fn matches_in_context(
        &self, context: InspectionContext<'_>, owner: &str, descriptor: Val,
    ) -> Result<bool> {
        self.inspect_method_in_context(context, owner, descriptor, true)
    }
    // A past authenticated memo does not require today's method to be the one
    // which computed it. Keep the same checked live table/key/schema inspection.
    pub(super) fn method_layout_in_context(
        &self, context: InspectionContext<'_>, owner: &str, descriptor: Val,
    ) -> Result<bool> {
        self.inspect_method_in_context(context, owner, descriptor, false)
    }
    /// Fresh seed data only: no implementation integrity, receipt, membership,
    /// or cache authority. The enclosing snapshot still checks every other slot.
    pub(super) fn fresh_seed_method_slot_matches(
        &self, context: InspectionContext<'_>, array: Val, index: u32, actual: &Val,
    ) -> Result<bool> {
        if context.identity != self.identity { return Err(SessionError::ForeignValue); }
        let mut scope = RootScope::new(context.store);
        Session::consume_host_work(scope.as_context_mut(), 3, context.interrupt)?;
        let Some(array_root) = array.anyref().flatten() else { return Ok(false); };
        let Some(actual_root) = actual.anyref().flatten() else { return Ok(false); };
        for method in &self.methods {
            Session::consume_host_work(scope.as_context_mut(), 4, context.interrupt)?;
            if index != method.implementation_index { continue; }
            let initial_table = method.table.value.to_rooted(&mut scope);
            if !Rooted::<AnyRef>::ref_eq(&scope, array_root, &initial_table)? { continue; }
            Session::consume_host_work(scope.as_context_mut(), 15, context.interrupt)?;
            let descriptor = method.descriptor.value.to_rooted(&mut scope);
            let object = descriptor.as_struct(&scope)?
                .ok_or_else(|| invalid("Invalid canonical class descriptor"))?;
            let current_table = object.field(&mut scope, 2)?;
            let Some(current_table) = current_table.anyref().flatten() else { return Ok(false); };
            if !Rooted::<AnyRef>::ref_eq(&scope, current_table, &initial_table)? { return Ok(false); }
            let table = initial_table.as_array(&scope)?
                .ok_or_else(|| invalid("Invalid canonical method table"))?;
            let len = table.len(&scope)?;
            if len % 2 != 0 || index == 0 || index >= len { return Ok(false); }
            let key = table.get(&mut scope, index - 1)?;
            let Some(key) = key.anyref().flatten() else { return Ok(false); };
            let initial_key = self.key.value.to_rooted(&mut scope);
            if !Rooted::<AnyRef>::ref_eq(&scope, key, &initial_key)? { return Ok(false); }
            let selected = table.get(&mut scope, index)?;
            let Some(selected) = selected.anyref().flatten() else { return Ok(false); };
            if !Rooted::<AnyRef>::ref_eq(&scope, selected, actual_root)? { return Ok(false); }
            return match self.method_layout_in_context(InspectionContext {
                store: scope.as_context_mut(), identity: context.identity,
                runtime: context.runtime, interrupt: context.interrupt,
            }, method.owner, Val::AnyRef(Some(descriptor))) {
                Err(SessionError::Language(_)) => Ok(false),
                result => result,
            };
        }
        Ok(false)
    }
    fn inspect_method_in_context(
        &self, context: InspectionContext<'_>, owner: &str, descriptor: Val,
        require_initial: bool,
    ) -> Result<bool> {
        if context.identity != self.identity {
            return Err(SessionError::ForeignValue);
        }
        let mut scope = RootScope::new(context.store);
        Session::consume_host_work(
            scope.as_context_mut(),
            self.methods.len() as u64 + 1,
            context.interrupt,
        )?;
        let Some(method) = self.methods.iter().find(|method| method.owner == owner) else {
            return Ok(false);
        };
        let Some(descriptor) = descriptor.anyref().flatten().copied() else {
            return Ok(false);
        };
        let expected = method.descriptor.value.to_rooted(&mut scope);
        if !Rooted::<AnyRef>::ref_eq(&scope, &descriptor, &expected)? {
            return Ok(false);
        }
        let key = self.key.value.to_rooted(&mut scope);
        if !self.key_storage.matches_in_context(
            scope.as_context_mut(),
            Val::AnyRef(Some(key)),
            context.identity,
            context.interrupt,
        )? {
            return Ok(false);
        }
        Session::consume_host_work(scope.as_context_mut(), 3, context.interrupt)?;
        let function = context
            .runtime
            .get_func(&mut scope, "protocol-method-get")
            .ok_or_else(|| invalid("Missing canonical method adapter"))?;
        let current = call(
            &mut scope,
            context.runtime,
            function,
            &[Val::AnyRef(Some(descriptor)), Val::AnyRef(Some(key))],
            context.identity,
            &self.handles,
        )?;
        Session::consume_host_work(scope.as_context_mut(), 1, context.interrupt)?;
        let current = current.value.to_rooted(&mut scope);
        let expected = method.implementation.value.to_rooted(&mut scope);
        if require_initial {
            if !Rooted::<AnyRef>::ref_eq(&scope, &current, &expected)? {
                return Ok(false);
            }
            if !method.closure.matches_in_context(
                scope.as_context_mut(),
                Val::AnyRef(Some(current)),
                context.identity,
                context.interrupt,
            )? {
                return Ok(false);
            }
        } else {
            Session::consume_host_work(scope.as_context_mut(), 4, context.interrupt)?;
            let Some(current) = current.as_struct(&scope)? else { return Ok(false); };
            let expected = expected.as_struct(&scope)?
                .ok_or_else(|| invalid("Initial hash implementation requires closure storage"))?;
            if !wasmtime::StructType::eq(&current.ty(&scope)?, &expected.ty(&scope)?) {
                return Ok(false);
            }
        }
        Session::consume_host_work(scope.as_context_mut(), 12, context.interrupt)?;
        let fields = schema_value(&mut scope, descriptor, method.slot)?;
        method.schema.matches_in_context(
            scope.as_context_mut(),
            fields,
            context.identity,
            context.interrupt,
        )
    }
}

impl Session {
    pub(crate) fn initial_source_class_matches(
        &mut self,
        owner: &str,
        constructor: Option<&SessionValue>,
        descriptor: &SessionValue,
    ) -> Result<bool> {
        let _request = self.operation_budget_scope()?;
        self.charge_host_work(1)?;
        // Local/unprovisioned Sessions never gain runtime source authority.
        let Some(registry) = self.canonical_methods.clone() else {
            return Ok(false);
        };
        registry.source_class_matches(self, owner, constructor, descriptor)
    }
    pub(super) fn initial_source_class_matches_at(
        &mut self, owner: &str, constructor: Option<&SessionValue>,
        descriptor: &SessionValue, ordinal: Option<usize>,
    ) -> Result<bool> {
        let _request = self.operation_budget_scope()?;
        self.charge_host_work(1)?;
        let Some(registry) = self.canonical_methods.clone() else { return Ok(false); };
        registry.source_class_matches_at(self, owner, constructor, descriptor, ordinal)
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::resolve::Phase;

    fn session(phase: Phase) -> Session {
        match phase {
            Phase::Runtime => Session::new_repl().unwrap(),
            Phase::Macro => Session::new_macro().unwrap(),
        }
    }

    fn fresh_slot_probe(
        session: &mut Session, methods: &CanonicalMethods, owner: &str,
        offset: u32, foreign: bool,
    ) -> Result<bool> {
        let _request = session.operation_budget_scope()?;
        let method = methods.methods.iter().find(|method| method.owner == owner).unwrap();
        let identity = session.identity;
        let runtime = session.runtime;
        let interrupt = session.host_work_interrupt();
        let mut scope = RootScope::new(&mut session.store);
        let table = method.table.value.to_rooted(&mut scope);
        let array = table.as_array(&scope)?.unwrap();
        let actual = array.get(&mut scope, method.implementation_index)?;
        methods.fresh_seed_method_slot_matches(InspectionContext {
            store: scope.as_context_mut(), identity: if foreign { identity.wrapping_add(1) } else { identity },
            runtime, interrupt: &interrupt,
        }, Val::AnyRef(Some(table)), method.implementation_index + offset, &actual)
    }

    #[test]
    fn fresh_seed_slot_preserves_strict_method_authority_and_exact_binding() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let methods = session.canonical_methods.clone().unwrap();
            let descriptor = methods.methods.iter().find(|m| m.owner == "PersistentArrayMap")
                .unwrap().descriptor.clone();
            assert!(fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, false).unwrap());
            session.eval("(extend-type PersistentArrayMap IHash (-hash [self] 91))").unwrap();
            session.collect().unwrap();
            assert!(fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, false).unwrap());
            assert!(!methods.matches(&mut session, "PersistentArrayMap", &descriptor).unwrap());
            assert!(!fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 1, false).unwrap());
            assert!(matches!(fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, true),
                Err(SessionError::ForeignValue)));
            // A number cannot be treated as an opaque closure implementation.
            let number = session.eval("17").unwrap();
            let method = methods.methods.iter().find(|m| m.owner == "PersistentArrayMap").unwrap();
            session.inspect(&method.table, |mut store, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                let number = number.value.to_rooted(&mut store);
                array.set(&mut store, method.implementation_index, Val::AnyRef(Some(number)))
            }).unwrap();
            assert!(!fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, false).unwrap());
        }
    }

    #[test]
    fn fresh_seed_slot_rejects_changed_key_owner_table_and_supplied_method() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let methods = session.canonical_methods.clone().unwrap();
            let method = methods.methods.iter().find(|m| m.owner == "PersistentArrayMap").unwrap();
            let other = methods.methods.iter().find(|m| m.owner == "List").unwrap();
            let nil = session.eval("nil").unwrap();
            // A different supplied method does not describe this actual slot.
            {
                let _request = session.operation_budget_scope().unwrap();
                let identity = session.identity;
                let runtime = session.runtime;
                let interrupt = session.host_work_interrupt();
                let mut scope = RootScope::new(&mut session.store);
                let table = method.table.value.to_rooted(&mut scope);
                let wrong = Val::AnyRef(Some(other.implementation.value.to_rooted(&mut scope)));
                assert!(!methods.fresh_seed_method_slot_matches(InspectionContext {
                    store: scope.as_context_mut(), identity, runtime, interrupt: &interrupt,
                }, Val::AnyRef(Some(table)), method.implementation_index, &wrong).unwrap());
                let wrong_table = Val::AnyRef(Some(nil.value.to_rooted(&mut scope)));
                assert!(!methods.fresh_seed_method_slot_matches(InspectionContext {
                    store: scope.as_context_mut(), identity, runtime, interrupt: &interrupt,
                }, wrong_table, method.implementation_index, &wrong).unwrap());
            }
            session.inspect(&method.table, |mut store, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                let nil = nil.value.to_rooted(&mut store);
                array.set(&mut store, method.implementation_index - 1, Val::AnyRef(Some(nil)))
            }).unwrap();
            session.collect().unwrap();
            assert!(!fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, false).unwrap());
            session.inspect(&method.table, |mut store, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                let key = methods.key.value.to_rooted(&mut store);
                array.set(&mut store, method.implementation_index - 1, Val::AnyRef(Some(key)))
            }).unwrap();
            // The retained table alone does not authorize a changed owner table.
            session.inspect(&method.descriptor, |mut store, value| {
                let descriptor = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let other_table = other.table.value.to_rooted(&mut store);
                descriptor.set_field(&mut store, 2, Val::AnyRef(Some(other_table)))
            }).unwrap();
            session.collect().unwrap();
            assert!(!fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, false).unwrap());
            session.inspect(&method.descriptor, |mut store, value| {
                let descriptor = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let table = method.table.value.to_rooted(&mut store);
                descriptor.set_field(&mut store, 2, Val::AnyRef(Some(table)))
            }).unwrap();
            assert!(fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, false).unwrap());
        }
    }

    #[test]
    fn fresh_seed_slot_every_direct_fuel_cut_is_typed_and_retryable() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let methods = session.canonical_methods.clone().unwrap();
            session.eval("(extend-type PersistentArrayMap IHash (-hash [self] 91))").unwrap();
            let cost = session.with_operation_budget(|session| {
                let before = session.store.get_fuel()?;
                assert!(fresh_slot_probe(session, &methods, "PersistentArrayMap", 0, false)?);
                Ok(before - session.store.get_fuel()?)
            }).unwrap();
            assert!(cost > 0);
            eprintln!("fresh-seed-direct-helper phase={phase:?} exhaustive-fuel-cuts={cost}");
            // Only this direct helper is swept, never the complete seed snapshot.
            for allowance in 0..cost {
                session.with_operation_budget(|session| {
                    session.store.set_fuel(allowance)?;
                    let error = fresh_slot_probe(session, &methods, "PersistentArrayMap", 0, false)
                        .expect_err("each allowance below measured helper cost must fail");
                    assert!(matches!(error, SessionError::Trap(ref inner)
                        if inner.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
                        "phase={phase:?} allowance={allowance}/{cost}: {error:?}");
                    Ok(())
                }).unwrap();
                assert!(fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, false).unwrap(),
                    "full retry after allowance={allowance}/{cost}");
            }
        }
    }

    #[test]
    fn fresh_seed_slot_interrupt_is_not_a_soft_mismatch_and_recovers() {
        use std::sync::atomic::Ordering;
        let mut session = session(Phase::Macro);
        let methods = session.canonical_methods.clone().unwrap();
        let request = session.operation_budget_scope().unwrap();
        session.interrupt.store(true, Ordering::SeqCst);
        assert!(fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, false).unwrap_err().is_interrupt());
        session.interrupt.store(false, Ordering::SeqCst);
        drop(request);
        assert!(fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, false).unwrap());
    }

    fn caller_probe(session: &mut Session) -> (wasmtime::Func, Arc<std::sync::atomic::AtomicBool>) {
        use wasm_encoder::*;
        let methods = session.canonical_methods.clone().unwrap();
        let identity = session.identity;
        let runtime = session.runtime;
        let interrupt = session.host_work_interrupt();
        let trigger = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let requested = trigger.clone();
        let ty = wasmtime::FuncType::new(
            &session.engine,
            [wasmtime::ValType::Ref(wasmtime::RefType::EQREF)],
            [wasmtime::ValType::I32],
        );
        // Store-bound, so this callback cannot import roots into another Store.
        let probe = wasmtime::Func::new(
            &mut session.store,
            ty,
            move |mut caller, arguments, results| {
                if requested.swap(false, std::sync::atomic::Ordering::SeqCst) {
                    interrupt.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                let eligible = methods
                    .matches_in_context(
                        InspectionContext {
                            store: caller.as_context_mut(),
                            identity,
                            runtime,
                            interrupt: &interrupt,
                        },
                        "List",
                        arguments[0].clone(),
                    )
                    .map_err(|error| match error {
                        SessionError::Host(error) | SessionError::Trap(error) => error,
                        other => wasmtime::Error::msg(other.to_string()),
                    })?;
                results[0] = Val::I32(i32::from(eligible));
                Ok(())
            },
        );
        let mut module = Module::new();
        let mut types = TypeSection::new();
        types
            .ty()
            .function([ValType::Ref(RefType::EQREF)], [ValType::I32]);
        module.section(&types);
        let mut imports = ImportSection::new();
        imports.import("test", "probe", EntityType::Function(0));
        module.section(&imports);
        let mut functions = FunctionSection::new();
        functions.function(0);
        module.section(&functions);
        let mut exports = ExportSection::new();
        exports.export("run", ExportKind::Func, 1);
        module.section(&exports);
        let mut code = CodeSection::new();
        let mut body = Function::new([]);
        body.instruction(&Instruction::LocalGet(0))
            .instruction(&Instruction::Call(0))
            .instruction(&Instruction::End);
        code.function(&body);
        module.section(&code);
        let module = wasmtime::Module::new(&session.engine, module.finish()).unwrap();
        let instance =
            wasmtime::Instance::new(&mut session.store, &module, &[probe.into()]).unwrap();
        (
            instance.get_func(&mut session.store, "run").unwrap(),
            trigger,
        )
    }

    fn invoke_probe(
        session: &mut Session,
        probe: wasmtime::Func,
        descriptor: &SessionValue,
    ) -> Result<i32> {
        session.check(descriptor)?;
        let _request = session.operation_budget_scope()?;
        let mut scope = RootScope::new(&mut session.store);
        let value = Val::AnyRef(Some(descriptor.value.to_rooted(&mut scope)));
        let mut result = [Val::I32(0)];
        probe.call(&mut scope, &[value], &mut result)?;
        Ok(result[0].unwrap_i32())
    }

    // All local authority inputs come from the genuine initial registry.
    #[test]
    fn class_coverage_preserves_order_independence_and_rejects_bad_catalogs() {
        use super::super::construction_records::{BridgeClassInput, ConstructionRecords};

        fn inputs(registry: &CanonicalMethods) -> Vec<BridgeClassInput> {
            registry.source_classes.iter().map(|class| BridgeClassInput {
                name: class.owner,
                constructor: class.constructor.clone(),
                descriptor: class.descriptor.clone(),
            }).collect()
        }
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut sess = session(phase);
            let registry = sess.canonical_methods.clone().unwrap();
            let expected_names: Vec<_> = crate::portable_macro_data::source_class_names()
                .chain(std::iter::once("SourceArray")).collect();
            assert_eq!(registry.source_classes.iter().map(|class| class.owner)
                .collect::<Vec<_>>(), expected_names);
            assert_eq!(expected_names.len(), 24);

            let canonical = ConstructionRecords::with_bridge_classes(inputs(&registry));
            assert!(canonical.matches_class_schemas(&mut sess).unwrap());

            let mut reversed = inputs(&registry);
            reversed.reverse();
            assert!(ConstructionRecords::with_bridge_classes(reversed)
                .matches_class_schemas(&mut sess).unwrap(),
                "positional misses must retain arbitrary-order support");

            let mut duplicate = inputs(&registry);
            let first = &registry.source_classes[0];
            duplicate[1] = BridgeClassInput {
                name: first.owner,
                constructor: first.constructor.clone(),
                descriptor: first.descriptor.clone(),
            };
            assert!(!ConstructionRecords::with_bridge_classes(duplicate)
                .matches_class_schemas(&mut sess).unwrap(),
                "same-length catalog with a duplicated class is invalid");

            let mut missing = inputs(&registry);
            missing.pop().unwrap();
            assert!(!ConstructionRecords::with_bridge_classes(missing)
                .matches_class_schemas(&mut sess).unwrap());

            let mut unknown = inputs(&registry);
            unknown[0].name = "CoverageUnknown";
            assert!(!ConstructionRecords::with_bridge_classes(unknown)
                .matches_class_schemas(&mut sess).unwrap());

            let mut extra = inputs(&registry);
            extra.push(BridgeClassInput {
                name: first.owner,
                constructor: first.constructor.clone(),
                descriptor: first.descriptor.clone(),
            });
            assert!(!ConstructionRecords::with_bridge_classes(extra)
                .matches_class_schemas(&mut sess).unwrap(),
                "input beyond expected_names exhaustion must still be audited");

            // Unknown first name makes coverage false while preserving local roots.
            // A genuine foreign value at the final input then forces the existing
            // owner check to fail. A blanket short-circuit after false loses this
            // error. No synthetic registry or authority is installed.
            let mut foreign = session(phase);
            let foreign_value = foreign.eval("nil").unwrap();
            let mut later_foreign = inputs(&registry);
            later_foreign[0].name = "CoverageUnknown";
            later_foreign.last_mut().unwrap().descriptor = foreign_value;
            let records = ConstructionRecords::with_bridge_classes(later_foreign);
            assert!(matches!(records.matches_class_schemas(&mut sess),
                Err(SessionError::ForeignValue)),
                "later ownership errors must not disappear after earlier false coverage");
            assert!(canonical.matches_class_schemas(&mut sess).unwrap());

            // Supplemental cancellation/recovery coverage. This deliberately does
            // not claim cancellation was injected after the first false input.
            sess.with_operation_budget(|sess| {
                sess.interrupt_handle().interrupt();
                assert!(canonical.matches_class_schemas(sess).unwrap_err().is_interrupt());
                Ok(())
            }).unwrap();
            assert!(canonical.matches_class_schemas(&mut sess).unwrap());
        }
    }
    #[test]
    fn canonical_methods_active_wasm_caller_charges_cumulative_fuel_and_recovers() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let methods = session.canonical_methods.clone().unwrap();
            let descriptor = methods
                .methods
                .iter()
                .find(|method| method.owner == "List")
                .unwrap()
                .descriptor
                .clone();
            let (probe, trigger) = caller_probe(&mut session);
            session.collect().unwrap();
            let cost = session
                .with_operation_budget(|session| {
                    let before = session.store.get_fuel()?;
                    assert_eq!(invoke_probe(session, probe, &descriptor)?, 1);
                    Ok(before - session.store.get_fuel()?)
                })
                .unwrap();
            assert!(cost >= 26, "native inspection must debit the caller");
            let original_allowance = session.options.fuel_per_operation;
            let allowance = cost.checked_mul(3).unwrap() + 32;
            assert!(allowance < session.options.fuel_per_operation);
            session.set_operation_fuel(allowance);
            let mut completed = 0;
            let exhausted = session
                .with_operation_budget(|session| {
                    for _ in 0..1000 {
                        assert_eq!(invoke_probe(session, probe, &descriptor)?, 1);
                        completed += 1;
                    }
                    Ok(())
                })
                .unwrap_err();
            assert!(
                matches!(exhausted, SessionError::Trap(error) if matches!(error.downcast_ref::<wasmtime::Trap>(), Some(wasmtime::Trap::OutOfFuel)))
            );
            assert!(completed > 0 && completed < 1000);
            assert_eq!(invoke_probe(&mut session, probe, &descriptor).unwrap(), 1);
            trigger.store(true, std::sync::atomic::Ordering::SeqCst);
            assert!(
                invoke_probe(&mut session, probe, &descriptor)
                    .unwrap_err()
                    .is_interrupt()
            );
            assert_eq!(invoke_probe(&mut session, probe, &descriptor).unwrap(), 1);
            session.set_operation_fuel(original_allowance);
            session
                .eval("(extend-type List IHash (-hash [_] 91))")
                .unwrap();
            assert_eq!(invoke_probe(&mut session, probe, &descriptor).unwrap(), 0);
            assert_eq!(session.stats().external_value_handles, 0);
        }
    }

    #[test]
    fn canonical_methods_capture_actual_installations_without_external_handles() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            assert_eq!(session.stats().external_value_handles, 0);
            let methods = session.canonical_methods.clone().unwrap();
            assert_eq!(methods.methods.len(), 25);
            session.collect().unwrap();
            session
                .with_operation_budget(|session| {
                    for method in &methods.methods {
                        assert!(methods.matches(session, method.owner, &method.descriptor)?);
                    }
                    Ok(())
                })
                .unwrap();
            assert_eq!(session.stats().external_value_handles, 0);
            let descriptor = methods
                .methods
                .iter()
                .find(|method| method.owner == "List")
                .unwrap()
                .descriptor
                .clone();
            session
                .eval("(extend-type List ICounted (-count [_] 73))")
                .unwrap();
            session.collect().unwrap();
            assert!(
                methods.matches(&mut session, "List", &descriptor).unwrap(),
                "unrelated protocol stays live"
            );
            session
                .eval("(ns suss.core) (def registry-saved-list List) (def List nil)")
                .unwrap();
            assert!(
                methods.matches(&mut session, "List", &descriptor).unwrap(),
                "rebinding globals does not redirect captured identity"
            );
            session.eval("(def List registry-saved-list) (def registry-saved-hash hash-ordered-coll) (def hash-ordered-coll (fn [_] 711))").unwrap();
            session.enter_namespace("user").unwrap();
            let hash = session.eval("(hash (list 1))").unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &hash).unwrap(),
                "711"
            );
            drop(hash);
            assert!(
                methods.matches(&mut session, "List", &descriptor).unwrap(),
                "live hash helper remains ordinary dispatch"
            );
            session
                .eval("(extend-type List IHash (-hash [_] 91))")
                .unwrap();
            session.collect().unwrap();
            assert!(!methods.matches(&mut session, "List", &descriptor).unwrap());
            assert!(
                CanonicalMethods::capture(
                    &mut session,
                    bootstrap::shipped_authority(phase).unwrap()
                )
                .is_err(),
                "late capture must not refresh replaced methods"
            );
            session.eval("(ns suss.core) (def List nil)").unwrap();
            assert!(
                !methods.matches(&mut session, "List", &descriptor).unwrap(),
                "mutable globals cannot refresh authority"
            );
            assert_eq!(session.stats().external_value_handles, 0);
        }
    }

    #[test]
    fn canonical_methods_partial_capture_failure_releases_roots_and_keeps_registry() {
        use std::sync::atomic::Ordering;
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let original = session.canonical_methods.clone().unwrap();
            let pending = Arc::new(AtomicUsize::new(0));
            let authority = bootstrap::shipped_authority(phase).unwrap();
            let error = CanonicalMethods::capture_accounted(
                &mut session,
                authority,
                pending.clone(),
                Some(3),
            )
            .err()
            .expect("injected cancellation after three captures");
            assert!(
                matches!(error, SessionError::Trap(error) if matches!(error.downcast_ref::<wasmtime::Trap>(), Some(wasmtime::Trap::Interrupt)))
            );
            assert_eq!(pending.load(Ordering::SeqCst), 0);
            assert!(Arc::ptr_eq(
                &original,
                session.canonical_methods.as_ref().unwrap()
            ));
            session.with_operation_budget(|session| {
                session.store.set_fuel(0)?;
                let error = CanonicalMethods::capture_accounted(session, authority, pending.clone(), None).err().expect("actual exhausted budget");
                assert!(matches!(error, SessionError::Trap(error) if matches!(error.downcast_ref::<wasmtime::Trap>(), Some(wasmtime::Trap::OutOfFuel))));
                Ok(())
            }).unwrap();
            assert_eq!(pending.load(Ordering::SeqCst), 0);
            let value = session.eval("42").unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &value).unwrap(),
                "42"
            );
            drop(value);
            assert_eq!(session.stats().external_value_handles, 0);
        }
    }

    #[test]
    fn canonical_methods_key_mutation_and_reset_do_not_refresh_old_roots() {
        use std::sync::atomic::Ordering;
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let methods = session.canonical_methods.clone().unwrap();
            let handles = methods.handles.clone();
            let descriptor = methods.methods[0].descriptor.clone();
            let owner = methods.methods[0].owner;
            let nil = session.eval("nil").unwrap();
            session
                .inspect(&methods.key, |mut store, value| {
                    let descriptor = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    let schema = descriptor
                        .field(&mut store, 1)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    assert!(schema.len(&store)? > 0);
                    let nil = nil.value.to_rooted(&mut store);
                    schema.set(&mut store, 0, Val::AnyRef(Some(nil)))
                })
                .unwrap();
            drop(nil);
            session.collect().unwrap();
            assert!(!methods.matches(&mut session, owner, &descriptor).unwrap());
            session.reset().unwrap();
            let fresh = session.canonical_methods.clone().unwrap();
            assert!(!Arc::ptr_eq(&methods, &fresh));
            assert!(matches!(
                fresh.matches(&mut session, owner, &descriptor),
                Err(SessionError::ForeignValue)
            ));
            drop(descriptor);
            drop(methods);
            assert_eq!(handles.load(Ordering::SeqCst), 0);
            assert_eq!(session.stats().external_value_handles, 0);
        }
    }
    #[test]
    fn initial_source_classes_cover_bridge_and_allow_live_unrelated_protocols_after_gc() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let registry = session.canonical_methods.clone().unwrap();
            assert_eq!(crate::portable_macro_data::source_class_names().len(), 23);
            assert_eq!(registry.source_classes.len(), 24);
            let mut expected = crate::portable_macro_data::source_class_names()
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(expected.len(), 23, "shared bridge names must be unique");
            assert!(expected.insert("SourceArray"));
            let observed = registry
                .source_classes
                .iter()
                .map(|class| class.owner)
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                observed.len(),
                registry.source_classes.len(),
                "captured owners must be unique"
            );
            assert_eq!(observed, expected);
            let first = &registry.source_classes[0];
            let repeated = (0..registry.source_classes.len())
                .map(|_| super::super::construction_records::BridgeClassInput {
                    name: first.owner,
                    constructor: first.constructor.clone(),
                    descriptor: first.descriptor.clone(),
                })
                .collect();
            let repeated =
                super::super::construction_records::ConstructionRecords::with_bridge_classes(
                    repeated,
                );
            assert!(
                !repeated.matches_class_schemas(&mut session).unwrap(),
                "count alone cannot prove class coverage"
            );
            assert_eq!(session.stats().external_value_handles, 0);
            session.collect().unwrap();
            for class in &registry.source_classes {
                assert!(
                    session
                        .initial_source_class_matches(
                            class.owner,
                            class.constructor.as_ref(),
                            &class.descriptor
                        )
                        .unwrap()
                );
            }
            session
                .eval("(extend-type List ICounted (-count [_] 73))")
                .unwrap();
            session.collect().unwrap();
            for class in &registry.source_classes {
                assert!(
                    session
                        .initial_source_class_matches(
                            class.owner,
                            class.constructor.as_ref(),
                            &class.descriptor
                        )
                        .unwrap(),
                    "{}",
                    class.owner
                );
            }
            let count = session.eval("(count (list 1))").unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &count).unwrap(),
                "73"
            );
            drop(count);
            assert_eq!(session.stats().external_value_handles, 0);
        }
    }
    #[test]
    fn initial_source_classes_reject_non_hash_schema_entry_and_utf16_mutation() {
        for phase in [Phase::Runtime, Phase::Macro] {
            for owner in ["VectorNode"] {
                for in_place in [false, true] {
                    assert!(!bootstrap::memo_owner_slots().any(|(name, _)| name == owner));
                    let mut session = session(phase);
                    let registry = session.canonical_methods.clone().unwrap();
                    let class = registry
                        .source_classes
                        .iter()
                        .find(|class| class.owner == owner)
                        .unwrap();
                    let poison = session
                        .data_scalar(&crate::portable::hir::Literal::String(
                            "changed-field".encode_utf16().collect(),
                        ))
                        .unwrap();
                    session
                        .inspect(&class.descriptor, |mut store, value| {
                            let descriptor =
                                value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                            let schema = descriptor
                                .field(&mut store, 1)?
                                .unwrap_anyref()
                                .unwrap()
                                .as_array(&store)?
                                .unwrap();
                            assert!(schema.len(&store)? > 0);
                            if in_place {
                                let name = schema
                                    .get(&mut store, 0)?
                                    .unwrap_anyref()
                                    .unwrap()
                                    .as_array(&store)?
                                    .unwrap();
                                let unit = name.get(&mut store, 0)?.unwrap_i32();
                                name.set(&mut store, 0, Val::I32(unit ^ 1))
                            } else {
                                let poison = poison.value.to_rooted(&mut store);
                                schema.set(&mut store, 0, Val::AnyRef(Some(poison)))
                            }
                        })
                        .unwrap();
                    drop(poison);
                    session.collect().unwrap();
                    assert!(
                        !session
                            .initial_source_class_matches(
                                owner,
                                class.constructor.as_ref(),
                                &class.descriptor
                            )
                            .unwrap()
                    );
                    // A bridge captured after mutation must not refresh authority.
                    let bridge = crate::portable_macro_data::FormBridge::new(&mut session).unwrap();
                    let records = bridge
                        .recording(&mut session)
                        .unwrap()
                        .into_records()
                        .unwrap();
                    assert!(!records.matches_class_schemas(&mut session).unwrap());
                }
            }
        }
    }
    #[test]
    fn initial_source_classes_bind_private_array_empty_schema_and_descriptor_identity() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let registry = session.canonical_methods.clone().unwrap();
            let class = registry
                .source_classes
                .iter()
                .find(|class| class.owner == "SourceArray")
                .unwrap();
            assert!(class.constructor.is_none());
            assert!(!bootstrap::memo_owner_slots().any(|(name, _)| name == class.owner));
            session
                .inspect(&class.descriptor, |mut store, value| {
                    let descriptor = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    let schema = descriptor
                        .field(&mut store, 1)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    assert_eq!(
                        schema.len(&store)?,
                        0,
                        "private array schema has no mutable name entries"
                    );
                    Ok(())
                })
                .unwrap();
            let impostor = {
                let mut scope = RootScope::new(&mut session.store);
                let descriptor = class
                    .descriptor
                    .value
                    .to_rooted(&mut scope)
                    .as_struct(&scope)
                    .unwrap()
                    .unwrap();
                let ty = descriptor.ty(&scope).unwrap();
                let values = (0..5)
                    .map(|index| descriptor.field(&mut scope, index))
                    .collect::<wasmtime::Result<Vec<_>>>()
                    .unwrap();
                let allocator = wasmtime::StructRefPre::new(&mut scope, ty);
                let fake = wasmtime::StructRef::new(&mut scope, &allocator, &values)
                    .unwrap()
                    .to_anyref();
                own(
                    &mut scope,
                    Val::AnyRef(Some(fake)),
                    session.identity,
                    &registry.handles,
                )
                .unwrap()
            };
            session.collect().unwrap();
            assert!(
                session
                    .initial_source_class_matches(class.owner, None, &class.descriptor)
                    .unwrap()
            );
            assert!(
                !session
                    .initial_source_class_matches(class.owner, None, &impostor)
                    .unwrap(),
                "identical type, nominal ID and fields cannot forge descriptor identity"
            );
            let inputs = registry
                .source_classes
                .iter()
                .map(
                    |entry| super::super::construction_records::BridgeClassInput {
                        name: entry.owner,
                        constructor: entry.constructor.clone(),
                        descriptor: if entry.owner == "SourceArray" {
                            impostor.clone()
                        } else {
                            entry.descriptor.clone()
                        },
                    },
                )
                .collect();
            let records =
                super::super::construction_records::ConstructionRecords::with_bridge_classes(
                    inputs,
                );
            assert!(!records.matches_class_schemas(&mut session).unwrap());
        }
    }
    #[test]
    fn initial_source_classes_remember_failed_used_class_precheck_after_restoration() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let registry = session.canonical_methods.clone().unwrap();
            let class = registry
                .source_classes
                .iter()
                .find(|class| class.owner == "VectorNode")
                .unwrap();
            let schema =
                descriptor_field(&mut session, &class.descriptor, 1, &registry.handles).unwrap();
            let original = {
                let mut scope = RootScope::new(&mut session.store);
                let array = schema
                    .value
                    .to_rooted(&mut scope)
                    .as_array(&scope)
                    .unwrap()
                    .unwrap();
                let name = array.get(&mut scope, 0).unwrap();
                own(&mut scope, name, session.identity, &registry.handles).unwrap()
            };
            let bridge = crate::portable_macro_data::FormBridge::new(&mut session)
                .unwrap()
                .recording(&mut session)
                .unwrap();
            let poison = session
                .data_scalar(&crate::portable::hir::Literal::String(
                    "bad-edit".encode_utf16().collect(),
                ))
                .unwrap();
            session
                .inspect(&schema, |mut store, value| {
                    let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                    let poison = poison.value.to_rooted(&mut store);
                    array.set(&mut store, 0, Val::AnyRef(Some(poison)))
                })
                .unwrap();
            let item = bridge
                .scalar(&mut session, &crate::portable::hir::Literal::Number(17.0))
                .unwrap();
            drop(bridge.vector_values(&mut session, &[item]).unwrap());
            session
                .inspect(&schema, |mut store, value| {
                    let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                    let original = original.value.to_rooted(&mut store);
                    array.set(&mut store, 0, Val::AnyRef(Some(original)))
                })
                .unwrap();
            session.collect().unwrap();
            assert!(
                session
                    .initial_source_class_matches(
                        class.owner,
                        class.constructor.as_ref(),
                        &class.descriptor
                    )
                    .unwrap()
            );
            let records = bridge.into_records().unwrap();
            assert!(
                !records.matches_class_schemas(&mut session).unwrap(),
                "restoration cannot erase a failed construction precheck"
            );
        }
    }
    #[test]
    fn initial_source_classes_bind_constructor_reference_and_current_descriptor() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let registry = session.canonical_methods.clone().unwrap();
            let list = registry
                .source_classes
                .iter()
                .find(|class| class.owner == "List")
                .unwrap();
            let constructor = list.constructor.as_ref().unwrap();
            let impostor = session.eval("(deftype ClassSchemaImpostor [meta first rest count __hash]) ClassSchemaImpostor").unwrap();
            let descriptor = session.data_descriptor(&impostor, true).unwrap();
            assert!(
                !session
                    .initial_source_class_matches("List", Some(&impostor), &descriptor)
                    .unwrap()
            );
            assert!(
                !session
                    .initial_source_class_matches("List", Some(&impostor), &list.descriptor)
                    .unwrap()
            );
            session.eval("(ns suss.core) (def List Keyword)").unwrap();
            let rebound = session.eval("List").unwrap();
            let rebound_descriptor = session.data_descriptor(&rebound, true).unwrap();
            assert!(
                !session
                    .initial_source_class_matches("List", Some(&rebound), &rebound_descriptor)
                    .unwrap()
            );
            assert!(
                session
                    .initial_source_class_matches("List", Some(constructor), &list.descriptor)
                    .unwrap(),
                "old captured class is still the original class"
            );
            // Retain the closure identity but poison its reachable owner storage.
            session
                .inspect(constructor, |mut store, value| {
                    let closure = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    let owner = closure
                        .field(&mut store, 0)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_struct(&store)?
                        .unwrap();
                    let fields = owner
                        .field(&mut store, 1)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    let changed = descriptor.value.to_rooted(&mut store);
                    fields.set(&mut store, 0, Val::AnyRef(Some(changed)))
                })
                .unwrap();
            session.collect().unwrap();
            assert!(
                !session
                    .initial_source_class_matches("List", Some(constructor), &list.descriptor)
                    .unwrap()
            );
        }
    }
    #[test]
    fn initial_source_classes_stage_private_roots_and_decline_missing_authority() {
        use std::sync::atomic::Ordering;
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let original = session.canonical_methods.clone().unwrap();
            let pending = Arc::new(AtomicUsize::new(0));
            // Private fault injection exercises staging only; it cannot publish
            // a replacement registry or mint public bootstrap authority.
            let error = capture_source_classes(&mut session, &pending, Some(2))
                .err()
                .unwrap();
            assert!(error.is_interrupt());
            assert_eq!(pending.load(Ordering::SeqCst), 0);
            assert!(Arc::ptr_eq(
                &original,
                session.canonical_methods.as_ref().unwrap()
            ));
            let class = &original.source_classes[0];
            session.with_operation_budget(|session| {
                session.store.set_fuel(0)?;
                let error = session.initial_source_class_matches(class.owner, class.constructor.as_ref(), &class.descriptor).unwrap_err();
                assert!(matches!(error, SessionError::Trap(ref error) if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{error:?}");
                Ok(())
            }).unwrap();
            session
                .with_operation_budget(|session| {
                    session.interrupt_handle().interrupt();
                    assert!(
                        session
                            .initial_source_class_matches(
                                class.owner,
                                class.constructor.as_ref(),
                                &class.descriptor
                            )
                            .unwrap_err()
                            .is_interrupt()
                    );
                    Ok(())
                })
                .unwrap();
            assert!(
                session
                    .initial_source_class_matches(
                        class.owner,
                        class.constructor.as_ref(),
                        &class.descriptor
                    )
                    .unwrap()
            );
            session.reset().unwrap();
            assert!(matches!(
                session.initial_source_class_matches(
                    class.owner,
                    class.constructor.as_ref(),
                    &class.descriptor
                ),
                Err(SessionError::ForeignValue)
            ));
            let mut local = Session::with_options_in(Default::default(), phase).unwrap();
            let value = local.eval("nil").unwrap();
            assert!(
                !local
                    .initial_source_class_matches("List", Some(&value), &value)
                    .unwrap()
            );
        }
    }
}

impl CanonicalMethods {
    pub(super) fn source_classes_in_context(&self, mut context: InspectionContext<'_>) -> Result<bool> {
        let mut scope=RootScope::new(context.store.as_context_mut());
        let mut valid=true;
        for class in &self.source_classes {
            Session::consume_host_work(scope.as_context_mut(),4,context.interrupt)?;
            let expected=class.descriptor.value.to_rooted(&mut scope);
            if let Some(constructor)=&class.constructor {
                let function=context.runtime.get_func(&mut scope,"constructor-descriptor").ok_or_else(||invalid("Missing constructor descriptor adapter"))?;
                let constructor=Val::AnyRef(Some(constructor.value.to_rooted(&mut scope)));
                let current=call(&mut scope,context.runtime,function,&[constructor],context.identity,&self.handles)?;
                let current=current.value.to_rooted(&mut scope);
                valid &= Rooted::<AnyRef>::ref_eq(&scope,&expected,&current)?;
            }
            let descriptor=expected.as_struct(&scope)?.ok_or_else(||invalid("Invalid initial source descriptor"))?;
            let fields=descriptor.field(&mut scope,1)?;
            valid &= class.schema.matches_in_context(scope.as_context_mut(),fields,context.identity,context.interrupt)?;
        }
        Ok(valid)
    }
}
