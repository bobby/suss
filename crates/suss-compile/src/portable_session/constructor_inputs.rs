//! Direct ordinary-constructor facts derived before construction from its inputs.
//! Direct facts include the initial prototype carrier and a nil receiver UID.
//! Descendants, pristine class/prototype authority, authenticated UID changes and
//! memo transitions still require separate validation. This is not a complete
//! source certificate and does not authorize retained-root reuse.
use super::{Session, SessionError, SessionValue, own, storage_snapshot::StorageSnapshot};
use wasmtime::{AnyRef, ArrayType, AsContextMut, RootScope, Rooted, Val};

type Result<T> = std::result::Result<T, SessionError>;

#[derive(Debug)]
pub(crate) struct ConstructorInputs {
    descriptor: SessionValue,
    schema: SessionValue,
    schema_storage: std::sync::Arc<StorageSnapshot>,
    fields: Vec<Option<SessionValue>>,
    receiver: SessionValue,
    field_storage: SessionValue,
    properties: SessionValue,
    prototype: SessionValue,
    prototype_origin: Option<super::prototype_origin::PrototypeInputs>,
    memo_watch: Option<std::sync::Arc<super::memo_bridge::Watch>>,
}
// Session-local bounded retention, never source or pristine-class authority.
// Successful reset replaces the Session and this cache. Foreign roots are
// checked before access. Exclusive &mut Session prevents intervening cache edits.
const SCHEMA_WITNESS_LIMIT: usize = 16;
struct SchemaWitnessEntry {
    schema: SessionValue,
    snapshot: std::sync::Arc<StorageSnapshot>,
}
#[derive(Default)]
pub(super) struct SchemaWitnessCache {
    entries: Vec<SchemaWitnessEntry>,
    next: usize,
}
struct SchemaWitnessPublication {
    entry: SchemaWitnessEntry,
    target: usize,
    grown: Option<Vec<SchemaWitnessEntry>>,
}
impl SchemaWitnessPublication {
    // Every reserve, copy/control fee and retain precedes this infallible commit.
    fn publish(self, cache: &mut SchemaWitnessCache) {
        if let Some(mut grown) = self.grown {
            grown.extend(cache.entries.drain(..));
            cache.entries = grown;
        }
        if self.target == cache.entries.len() {
            cache.entries.push(self.entry);
        } else {
            cache.entries[self.target] = self.entry;
            cache.next = (self.target + 1) % SCHEMA_WITNESS_LIMIT;
        }
    }
}
// Test-only stage seam: cancellation is consumed by the REAL following
// operation. It grants no authority and replaces no validation/publication.
#[cfg(test)]
thread_local! {
    static SCHEMA_CANCEL_STAGE: std::cell::Cell<Option<&'static str>> = const { std::cell::Cell::new(None) };
}
#[cfg(test)]
fn cancel_schema_stage(session: &Session, stage: &'static str) {
    SCHEMA_CANCEL_STAGE.with(|armed| {
        if armed.get() == Some(stage) {
            armed.set(None);
            session.host_work_interrupt().store(true, std::sync::atomic::Ordering::SeqCst);
        }
    });
}

impl Session {
    fn prepare_schema_witness(&mut self, schema: &SessionValue) -> Result<(
        std::sync::Arc<StorageSnapshot>, Option<SchemaWitnessPublication>,
    )> {
        self.check(schema)?;
        let mut existing = None;
        self.charge_host_work(1)?;
        for index in 0..self.schema_witness_cache.entries.len() {
            self.charge_host_work(1)?;
            self.check(&self.schema_witness_cache.entries[index].schema)?;
            let equal = {
                let mut scope = RootScope::new(&mut self.store);
                let wanted = schema.value.to_rooted(&mut scope);
                let candidate = self.schema_witness_cache.entries[index].schema.value.to_rooted(&mut scope);
                Rooted::<AnyRef>::ref_eq(&scope, &wanted, &candidate)?
            };
            if equal {
                self.charge_host_work(1)?;
                let snapshot = self.schema_witness_cache.entries[index].snapshot.clone();
                if self.storage_matches(schema, &snapshot)? {
                    return Ok((snapshot, None));
                }
                existing = Some(index);
                break;
            }
        }
        // A changed schema is not rejected: ordinary construction observes its
        // current state through exactly the original complete, strict capture.
        // The cache and shared native fact witness own private roots. They
        // are not caller-owned SessionValues reported by SessionStats.
        // Each cold witness owns its counter through its rooted values, so
        // eviction/reset cannot invalidate a fact retaining the same witness.
        let accounting_owner = std::mem::size_of::<std::sync::atomic::AtomicUsize>()
            .checked_add(2 * std::mem::size_of::<usize>())
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| invalid("Schema witness accounting allocation overflow"))?;
        self.charge_host_work(accounting_owner)?;
        let handles = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let snapshot = self.capture_storage_accounted(schema, handles.clone())?;
        let owner = std::mem::size_of::<StorageSnapshot>()
            .checked_add(2 * std::mem::size_of::<usize>())
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| invalid("Schema witness owner allocation overflow"))?;
        self.charge_host_work(owner)?;
        let snapshot = std::sync::Arc::new(snapshot);
        let len = self.schema_witness_cache.entries.len();
        let target = existing.unwrap_or(if len < SCHEMA_WITNESS_LIMIT {
            len
        } else {
            self.schema_witness_cache.next
        });
        if target < len {
            // Retain/drop this temporary outside the cache borrow. Publication
            // can retire the cache's last witness owner; pay for the full release
            // even when another ConstructorInputs currently shares that owner.
            self.charge_host_work(2)?;
            let retiring = self.schema_witness_cache.entries[target].snapshot.clone();
            retiring.precharge_retirement(&mut |work| self.charge_host_work(work))?;
            self.charge_host_work(u64::try_from(std::mem::size_of::<SchemaWitnessEntry>())
                .map_err(|_| invalid("Schema entry retirement work overflow"))?)?;
            drop(retiring);
        }
        let mut grown = None;
        if target == len && len == self.schema_witness_cache.entries.capacity() {
            let capacity = len.checked_mul(2)
                .ok_or_else(|| invalid("Schema witness capacity overflow"))?
                .max(4).min(SCHEMA_WITNESS_LIMIT);
            let bytes = capacity.checked_add(len)
                .and_then(|n| n.checked_mul(std::mem::size_of::<SchemaWitnessEntry>()))
                .and_then(|n| n.checked_add(len))
                .and_then(|n| u64::try_from(n).ok())
                .ok_or_else(|| invalid("Schema witness allocation overflow"))?;
            self.charge_host_work(bytes)?;
            let mut values = Vec::new();
            values.try_reserve_exact(capacity)
                .map_err(|_| invalid("Schema witness allocation failed"))?;
            grown = Some(values);
        }
        // Exact schema root retention, shared witness retention, publication.
        self.charge_host_work(3)?;
        handles.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let cached_schema = SessionValue {
            identity: schema.identity,
            value: schema.value.clone(),
            handles,
        };
        let entry = SchemaWitnessEntry { schema: cached_schema, snapshot: snapshot.clone() };
        Ok((snapshot, Some(SchemaWitnessPublication { entry, target, grown })))
    }
}

fn invalid(message: &'static str) -> SessionError {
    SessionError::Host(wasmtime::Error::msg(message))
}
fn reserve<T>(session: &mut Session, values: &mut Vec<T>, count: usize) -> Result<()> {
    let bytes = count
        .checked_mul(std::mem::size_of::<T>())
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| invalid("Constructor input allocation overflow"))?;
    session.charge_host_work(bytes + 1)?;
    values
        .try_reserve_exact(count)
        .map_err(|_| invalid("Constructor input allocation failed"))?;
    Ok(())
}
fn field(session: &mut Session, value: &SessionValue, index: usize) -> Result<SessionValue> {
    session.check(value)?;
    session.charge_host_work(1)?;
    let mut scope = RootScope::new(&mut session.store);
    let object = value
        .value
        .to_rooted(&mut scope)
        .as_struct(&scope)?
        .ok_or_else(|| invalid("Constructor input needs a struct"))?;
    let value = object.field(&mut scope, index)?;
    own(&mut scope, value, session.identity, &session.handles)
}
fn same(session: &mut Session, a: &SessionValue, b: &SessionValue) -> Result<bool> {
    session.check(a)?;
    session.check(b)?;
    session.charge_host_work(1)?;
    let mut scope = RootScope::new(&mut session.store);
    let a = a.value.to_rooted(&mut scope);
    let b = b.value.to_rooted(&mut scope);
    Ok(Rooted::<AnyRef>::ref_eq(&scope, &a, &b)?)
}
fn names(session: &mut Session, schema: &SessionValue) -> Result<Vec<Option<Vec<u16>>>> {
    // Compare the actual canonical GC type, not just packed storage: a foreign
    // non-final/immutable u16 array is not necessarily the runtime STRING type.
    let empty = session.data_scalar(&crate::portable::hir::Literal::String(vec![]))?;
    let string_type = {
        let mut scope = RootScope::new(&mut session.store);
        empty
            .value
            .to_rooted(&mut scope)
            .as_array(&scope)?
            .unwrap()
            .ty(&scope)?
    };
    let count = session.inspect(schema, |store, value| {
        Ok(value
            .unwrap_anyref()
            .unwrap()
            .as_array(&store)?
            .unwrap()
            .len(&store)? as usize)
    })?;
    let mut names = Vec::new();
    reserve(session, &mut names, count)?;
    for index in 0..count {
        session.charge_host_work(1)?;
        let interrupt = session.host_work_interrupt();
        let mut scope = RootScope::new(&mut session.store);
        let schema = schema
            .value
            .to_rooted(&mut scope)
            .as_array(&scope)?
            .unwrap();
        let value = schema.get(&mut scope, index as u32)?;
        let array = match value {
            Val::AnyRef(Some(value)) => value.as_array(&scope)?,
            _ => None,
        };
        let Some(array) = array else {
            names.push(None);
            continue;
        };
        if !ArrayType::eq(&array.ty(&scope)?, &string_type) {
            names.push(None);
            continue;
        }
        let length = array.len(&scope)? as usize;
        Session::consume_host_work(
            scope.as_context_mut(),
            (length as u64).saturating_mul(3) + 1,
            &interrupt,
        )?;
        let mut units = Vec::new();
        units
            .try_reserve_exact(length)
            .map_err(|_| invalid("Constructor schema allocation failed"))?;
        for index in 0..length {
            Session::consume_host_work(scope.as_context_mut(), 1, &interrupt)?;
            units.push(array.get(&mut scope, index as u32)?.unwrap_i32() as u16);
        }
        names.push(Some(units));
    }
    Ok(names)
}
impl Session {
    /// For private, unexposed analysis values only. Ordinary construction keeps
    /// its existing special dispatch; this entry rejects that dispatch before
    /// any private argument can reach a guest constructor. The prepared native
    /// plan is consumed immediately, with no intervening guest callback.
    pub(crate) fn data_construct_without_guest_callbacks(
        &mut self,
        class: &SessionValue,
        arguments: &[&SessionValue],
    ) -> Result<SessionValue> {
        self.check(class)?;
        let _request = self.operation_budget_scope()?;
        self.check_data_input_owners(arguments)?;
        let (descriptor, plan) = self.data_constructor_plan(class)?;
        if !same(self, &descriptor, &plan)? {
            return Err(invalid("Private analysis values require a callback-free constructor"));
        }
        self.data_construct_from_plan(&plan, arguments)
    }

    /// Execute the exact prepared source-constructor route once. Special
    /// constructors retain their normal result but receive no ordinary facts.
    pub(crate) fn data_construct_with_inputs(
        &mut self,
        class: &SessionValue,
        arguments: &[&SessionValue],
    ) -> Result<(SessionValue, Option<ConstructorInputs>)> {
        self.data_construct_with_inputs_policy(class, arguments, false)
    }
    pub(super) fn data_construct_private_with_inputs(
        &mut self, class: &SessionValue, arguments: &[&SessionValue],
    ) -> Result<(SessionValue, ConstructorInputs)> {
        let (value, fact) = self.data_construct_with_inputs_policy(class, arguments, true)?;
        Ok((value, fact.ok_or_else(|| invalid("Private constructor has no ordinary facts"))?))
    }
    fn data_construct_with_inputs_policy(
        &mut self, class: &SessionValue, arguments: &[&SessionValue], private: bool,
    ) -> Result<(SessionValue, Option<ConstructorInputs>)> {
        self.check(class)?;
        let _request = self.operation_budget_scope()?;
        self.check_data_input_owners(arguments)?;
        let (descriptor, plan) = self.data_constructor_plan(class)?;
        if !same(self, &descriptor, &plan)? {
            if private {
                return Err(invalid("Private analysis values require a callback-free constructor"));
            }
            return Ok((self.data_construct_from_plan(&plan, arguments)?, None));
        }
        let schema = field(self, &descriptor, 1)?;
        let (schema_storage, schema_publication) = self.prepare_schema_witness(&schema)?;
        #[cfg(test)] cancel_schema_stage(self, "names");
        let names = names(self, &schema)?;
        let mut fields = Vec::new();
        reserve(self, &mut fields, names.len())?;
        for index in 0..names.len() {
            self.charge_host_work(1)?;
            let mut source = index;
            if let Some(name) = &names[index] {
                for next in index + 1..names.len() {
                    self.charge_host_work(1)?;
                    if let Some(other) = &names[next] {
                        self.charge_host_work(
                            (name.len() as u64).saturating_add(other.len() as u64),
                        )?;
                        if name == other {
                            source = next;
                        }
                    }
                }
            }
            fields.push(arguments.get(source).map(|value| (*value).clone()));
        }
        // All expected field references and schema facts precede construction.
        // The ordinary plan has no guest callback between this point and bind.
        #[cfg(test)] cancel_schema_stage(self, "factory");
        let value = self.data_construct_from_plan(&plan, arguments)?;
        let field_storage = field(self, &value, 1)?;
        let properties = field(self, &value, 2)?;
        // The factory itself resolves/initializes this prototype immediately
        // before allocating the object. Observe its cached result afterwards:
        // moving that potentially allocating getter before the factory changes
        // malformed-state/fuel ordering. No guest callback intervenes here.
        self.charge_host_work(1)?;
        let prototype = {
            let mut scope = RootScope::new(&mut self.store);
            let getter = self
                .runtime
                .get_func(&mut scope, "owned-descriptor-prototype")
                .ok_or_else(|| invalid("Missing constructor prototype adapter"))?;
            let descriptor = Val::AnyRef(Some(descriptor.value.to_rooted(&mut scope)));
            super::call(
                &mut scope,
                self.runtime,
                getter,
                &[descriptor],
                self.identity,
                &self.handles,
            )?
        };
        // Read-only provenance query after the existing post-factory getter.
        // No eager getter, and custom/unregistered constructors keep direct facts.
        let prototype_origin = self.prototype_origin_inputs(class, &descriptor, &prototype)?;
        let mut inputs = ConstructorInputs {
            descriptor,
            schema,
            schema_storage,
            fields,
            receiver: value.clone(),
            field_storage,
            properties,
            prototype,
            prototype_origin,
            memo_watch: None,
        };
        if !inputs.matches(self, &value)? {
            return Err(invalid(
                "Ordinary constructor did not preserve its input fields",
            ));
        }
        // Registration follows exact initial-field validation and precedes guest
        // exposure. Failed registration drops every staged root/watch.
        #[cfg(test)] cancel_schema_stage(self, "watch");
        inputs.memo_watch = self.watch_constructor_memo(
            class, &inputs.descriptor, &value, &inputs.field_storage, &inputs.fields,
        )?;
        // No publication on a names/factory/direct-check/watch failure. There
        // are no remaining fallible operations or guest calls after this point.
        if let Some(publication) = schema_publication {
            publication.publish(&mut self.schema_witness_cache);
        }
        Ok((value, Some(inputs)))
    }
}
impl ConstructorInputs {
    pub(super) fn bind_native_inputs(
        &mut self, session: &mut Session,
        bindings: &super::deferred_factory::NativeBindings<'_>,
    ) -> Result<()> {
        // Schema inspection is a restriction only, never new constructor/source
        // authority. Require the original immutable observation before reading it.
        if !session.storage_matches(&self.schema, &self.schema_storage)? {
            return Err(invalid("Deferred constructor schema changed"));
        }
        let names = names(session, &self.schema)?;
        if names.len() != self.fields.len() {
            return Err(invalid("Deferred constructor schema width changed"));
        }
        // Only pre-construction expected references change. No physical field
        // value supplies an expectation, and every other captured fact remains.
        let mut fields = Vec::new();
        reserve(session, &mut fields, self.fields.len())?;
        for (index, field) in self.fields.iter().enumerate() {
            session.charge_host_work(2)?;
            fields.push(match field {
                Some(value) => {
                    let bound = bindings.resolve(session, value)?;
                    if !same(session, value, bound)? {
                        // Length, at most six UTF16 comparisons, and selection.
                        session.charge_host_work(8)?;
                        if names[index].as_deref().is_some_and(|name|
                            matches!(name, [95, 104, 97, 115, 104] | [95, 95, 104, 97, 115, 104])) {
                            return Err(invalid("Deferred binding cannot replace a captured hash field"));
                        }
                    }
                    Some(bound.clone())
                },
                None => None,
            });
        }
        self.fields = fields;
        Ok(())
    }
    pub(super) fn matches_prototype_origin_audited(
        &self, session: &mut Session, audit: &mut super::certification_audit::CertificationAudit,
    ) -> Result<bool> {
        match &self.prototype_origin {
            Some(origin) => audit.prototype(session, origin),
            None => Ok(false),
        }
    }
    pub(super) fn matches_prototype_origin(&self, session: &mut Session) -> Result<bool> {
        match &self.prototype_origin {
            Some(origin) => origin.matches(session),
            None => Ok(false),
        }
    }
    pub(super) fn inputs(&self) -> impl Iterator<Item = &SessionValue> {
        self.fields.iter().filter_map(Option::as_ref)
    }
    pub(crate) fn matches(&self, session: &mut Session, value: &SessionValue) -> Result<bool> {
        session.check(value)?;
        session.check(&self.receiver)?;
        let _request = session.operation_budget_scope()?;
        if !same(session, value, &self.receiver)?
            || !session.storage_matches(&self.schema, &self.schema_storage)?
        {
            return Ok(false);
        }
        let descriptor = field(session, value, 0)?;
        let storage = field(session, value, 1)?;
        let properties = field(session, value, 2)?;
        if !same(session, &descriptor, &self.descriptor)?
            || !same(session, &storage, &self.field_storage)?
            || !same(session, &properties, &self.properties)?
        {
            return Ok(false);
        }
        // Revalidate receipt before borrowing the Store for the field walk.
        // Original input edges remain unchanged, including the original nil.
        let authenticated_slot = match &self.memo_watch {
            Some(watch) if watch.matches(session)? => Some(watch.slot()),
            _ => None,
        };
        let interrupt = session.host_work_interrupt();
        let mut scope = RootScope::new(&mut session.store);
        let object = value
            .value
            .to_rooted(&mut scope)
            .as_struct(&scope)?
            .ok_or_else(|| invalid("Constructor receiver needs object storage"))?;
        Session::consume_host_work(scope.as_context_mut(), 1, &interrupt)?;
        let uid = object.field(&mut scope, 3)?;
        if !matches!(uid, Val::AnyRef(Some(value)) if value.as_i31(&scope)?.is_some_and(|value| value.get_u32()==0))
        {
            return Ok(false);
        }
        let properties = properties
            .value
            .to_rooted(&mut scope)
            .as_array(&scope)?
            .ok_or_else(|| invalid("Constructor properties need prototype storage"))?;
        Session::consume_host_work(scope.as_context_mut(), 1, &interrupt)?;
        if properties.len(&scope)? != 1 {
            return Ok(false);
        }
        Session::consume_host_work(scope.as_context_mut(), 2, &interrupt)?;
        let prototype = properties.get(&mut scope, 0)?;
        let expected = self.prototype.value.to_rooted(&mut scope);
        if !matches!(prototype, Val::AnyRef(Some(value)) if Rooted::<AnyRef>::ref_eq(&scope, &value, &expected)?)
        {
            return Ok(false);
        }
        let array = storage
            .value
            .to_rooted(&mut scope)
            .as_array(&scope)?
            .ok_or_else(|| invalid("Constructor fields need array storage"))?;
        if array.len(&scope)? as usize != self.fields.len() {
            return Ok(false);
        }
        for (index, expected) in self.fields.iter().enumerate() {
            Session::consume_host_work(scope.as_context_mut(), 1, &interrupt)?;
            let found = array.get(&mut scope, index as u32)?;
            let found = found
                .unwrap_anyref()
                .ok_or_else(|| invalid("Null ordinary constructor field"))?;
            let equal = if let Some(expected) = expected {
                let expected = expected.value.to_rooted(&mut scope);
                Rooted::<AnyRef>::ref_eq(&scope, found, &expected)?
            } else {
                found
                    .as_i31(&scope)?
                    .is_some_and(|v| v.get_i32() == crate::runtime_abi::UNDEFINED)
            };
            if !equal && authenticated_slot != Some(index) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::resolve::Phase;

    fn schema_test_session(phase: Phase) -> Session {
        match phase { Phase::Runtime => Session::new_repl().unwrap(), Phase::Macro => Session::new_macro().unwrap() }
    }
    fn schema_test_array(session: &mut Session, values: &[&SessionValue]) -> SessionValue {
        session.budget().unwrap();
        let mut scope = RootScope::new(&mut session.store);
        let mut output = [Val::null_any_ref()];
        session.runtime.get_func(&mut scope, "args-new").unwrap()
            .call(&mut scope, &[Val::I32(values.len() as i32)], &mut output).unwrap();
        let array = output[0].unwrap_anyref().unwrap().as_array(&scope).unwrap().unwrap();
        for (i, value) in values.iter().enumerate() {
            let rooted = value.value.to_rooted(&mut scope);
            array.set(&mut scope, i as u32, Val::AnyRef(Some(rooted))).unwrap();
        }
        super::super::own(&mut scope, output[0].clone(), session.identity, &session.handles).unwrap()
    }
    fn schema_test_child(session: &mut Session, schema: &SessionValue, index: u32) -> SessionValue {
        let mut scope = RootScope::new(&mut session.store);
        let array = schema.value.to_rooted(&mut scope).as_array(&scope).unwrap().unwrap();
        let value = array.get(&mut scope, index).unwrap();
        super::super::own(&mut scope, value, session.identity, &session.handles).unwrap()
    }
    fn schema_test_prepare(session: &mut Session, schema: &SessionValue) -> (
        std::sync::Arc<StorageSnapshot>, Option<SchemaWitnessPublication>,
    ) {
        let _request = session.operation_budget_scope().unwrap();
        session.prepare_schema_witness(schema).unwrap()
    }

    #[test]
    fn checked_schema_cache_strict_hits_deep_mutation_distinct_roots_and_gc() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = schema_test_session(phase);
            session.eval("(deftype CheckedSchemaProbe [alpha beta])").unwrap();
            let class = session.eval("CheckedSchemaProbe").unwrap();
            let input = session.eval("17").unwrap();
            let (receiver, prior) = session.data_construct_with_inputs(&class, &[&input, &input]).unwrap();
            let prior = prior.unwrap();
            let schema = prior.schema.clone();
            session.collect().unwrap();
            let (hit, pending) = schema_test_prepare(&mut session, &schema);
            assert!(pending.is_none());
            assert!(std::sync::Arc::ptr_eq(&hit, &prior.schema_storage));
            // Same child references and content, genuinely different array root.
            let a = schema_test_child(&mut session, &schema, 0);
            let b = schema_test_child(&mut session, &schema, 1);
            let equal = schema_test_array(&mut session, &[&a, &b]);
            assert!(!same(&mut session, &schema, &equal).unwrap());
            let (distinct, pending) = schema_test_prepare(&mut session, &equal);
            assert!(pending.is_some());
            assert!(!std::sync::Arc::ptr_eq(&distinct, &hit));
            drop(pending); // private preparation cannot publish on its own
            // Mutate a reachable UTF16 child; root identity remains the same.
            session.inspect(&a, |mut store, value| {
                value.unwrap_anyref().unwrap().as_array(&store)?.unwrap().set(&mut store, 0, Val::I32(98))
            }).unwrap();
            assert!(!prior.matches(&mut session, &receiver).unwrap());
            let (fresh, pending) = schema_test_prepare(&mut session, &schema);
            assert!(!std::sync::Arc::ptr_eq(&fresh, &hit));
            assert!(!session.storage_matches(&schema, &hit).unwrap());
            assert!(session.storage_matches(&schema, &fresh).unwrap());
            pending.unwrap().publish(&mut session.schema_witness_cache);
            session.collect().unwrap();
            let (again, pending) = schema_test_prepare(&mut session, &schema);
            assert!(pending.is_none());
            assert!(std::sync::Arc::ptr_eq(&again, &fresh));
            assert!(!prior.matches(&mut session, &receiver).unwrap());
            // Restore original data; an old fact is not permanently tainted.
            session.inspect(&a, |mut store, value| {
                value.unwrap_anyref().unwrap().as_array(&store)?.unwrap().set(&mut store, 0, Val::I32(97))
            }).unwrap();
            assert!(prior.matches(&mut session, &receiver).unwrap());
        }
    }

    #[test]
    fn checked_schema_cache_fuel_irq_staging_rollback_and_retry() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = schema_test_session(phase);
            let name = session.data_scalar(&crate::portable::hir::Literal::String(vec![97])).unwrap();
            let schema = schema_test_array(&mut session, &[&name]);
            session.schema_witness_cache = SchemaWitnessCache::default();
            let capture_cost = {
                let _request = session.operation_budget_scope().unwrap();
                let before = session.store.get_fuel().unwrap();
                drop(session.capture_storage(&schema).unwrap());
                before - session.store.get_fuel().unwrap()
            };
            let owner = (std::mem::size_of::<StorageSnapshot>() + 2*std::mem::size_of::<usize>()) as u64;
            let growth = (4*std::mem::size_of::<SchemaWitnessEntry>()) as u64;
            // Actual cold path: lookup entry, original capture, owner, Vec, retains.
            for allowance in [0, 1, 1+capture_cost, 1+capture_cost+owner,
                1+capture_cost+owner+growth, 1+capture_cost+owner+growth+2] {
                let handles = session.stats().external_value_handles;
                let _request = session.operation_budget_scope().unwrap();
                session.store.set_fuel(allowance).unwrap();
                let error = session.prepare_schema_witness(&schema).err().expect("incomplete staging budget");
                assert!(matches!(error, SessionError::Trap(ref e) if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
                assert!(session.schema_witness_cache.entries.is_empty());
                assert_eq!(session.stats().external_value_handles, handles);
            }
            let (old, pending) = schema_test_prepare(&mut session, &schema);
            pending.unwrap().publish(&mut session.schema_witness_cache);
            let handles = session.stats().external_value_handles;
            for allowance in [0, 1, 2] {
                let _request = session.operation_budget_scope().unwrap();
                session.store.set_fuel(allowance).unwrap();
                assert!(session.prepare_schema_witness(&schema).is_err());
                assert!(std::sync::Arc::ptr_eq(&session.schema_witness_cache.entries[0].snapshot, &old));
                assert_eq!(session.stats().external_value_handles, handles);
            }
            {
                let _request = session.operation_budget_scope().unwrap();
                session.host_work_interrupt().store(true, std::sync::atomic::Ordering::SeqCst);
                let error = session.prepare_schema_witness(&schema).err().unwrap();
                assert!(error.is_interrupt());
                assert!(std::sync::Arc::ptr_eq(&session.schema_witness_cache.entries[0].snapshot, &old));
            }
            let (retry, pending) = schema_test_prepare(&mut session, &schema);
            assert!(pending.is_none());
            assert!(std::sync::Arc::ptr_eq(&old, &retry));
        }
    }

    #[test]
    fn checked_schema_cache_actual_constructor_stage_failures_do_not_publish() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = schema_test_session(phase);
            let class = session.eval("suss.core/List").unwrap();
            let nil = session.eval("nil").unwrap();
            let stages: &[&str] = if phase == Phase::Macro { &["names", "factory", "watch"] } else { &["names", "factory"] };
            for &stage in stages {
                session.schema_witness_cache = SchemaWitnessCache::default();
                SCHEMA_CANCEL_STAGE.with(|armed| armed.set(Some(stage)));
                let error = session.data_construct_with_inputs(&class, &[&nil, &nil, &nil, &nil, &nil]).unwrap_err();
                SCHEMA_CANCEL_STAGE.with(|armed| assert!(armed.get().is_none(), "actual requested stage reached"));
                assert!(error.is_interrupt(), "{stage}: {error:?}");
                assert!(session.schema_witness_cache.entries.is_empty(), "{stage}");
                let (value, fact) = session.data_construct_with_inputs(&class, &[&nil, &nil, &nil, &nil, &nil]).unwrap();
                let fact = fact.unwrap();
                assert!(fact.matches(&mut session, &value).unwrap());
                if phase == Phase::Macro { assert!(fact.memo_watch.is_some()); }
                assert!(session.schema_witness_cache.entries.iter().any(|entry| std::sync::Arc::ptr_eq(&entry.snapshot, &fact.schema_storage)));
            }
        }
    }

    #[test]
    fn checked_schema_cache_retirement_failure_preserves_last_and_shared_owners_then_retries() {
        for phase in [Phase::Runtime, Phase::Macro] {
            for shared in [false, true] {
                let mut session = schema_test_session(phase);
                let name = session.data_scalar(&crate::portable::hir::Literal::String(vec![97])).unwrap();
                session.eval("(deftype RetirementWitness [alpha])").unwrap();
                let class = session.eval("RetirementWitness").unwrap();
                session.schema_witness_cache = SchemaWitnessCache::default();
                let (receiver, fact) = session.data_construct_with_inputs(&class, &[&name]).unwrap();
                let fact = fact.unwrap();
                assert!(fact.matches(&mut session, &receiver).unwrap());
                let mut schemas = vec![fact.schema.clone()];
                for _ in 0..16 { schemas.push(schema_test_array(&mut session, &[&name])); }
                let older_fact = if shared { Some(fact) } else { drop(fact); None };
                for schema in &schemas[1..16] {
                    let (snapshot, publication) = schema_test_prepare(&mut session, schema);
                    publication.unwrap().publish(&mut session.schema_witness_cache);
                    drop(snapshot);
                }
                let target = session.schema_witness_cache.next;
                let witness = &session.schema_witness_cache.entries[target].snapshot;
                assert_eq!(std::sync::Arc::strong_count(witness), if shared { 2 } else { 1 });
                let weak = std::sync::Arc::downgrade(witness);
                let table = session.schema_witness_cache.entries.as_ptr();
                let cost = {
                    let _request = session.operation_budget_scope().unwrap();
                    let before = session.store.get_fuel().unwrap();
                    let staged = session.prepare_schema_witness(&schemas[16]).unwrap();
                    let spent = before - session.store.get_fuel().unwrap();
                    drop(staged); // preparation alone cannot retire the old owner
                    spent
                };
                // Full-cache eviction has no Vec growth. After snapshot release
                // prepayment only entry-release and final three retain/control
                // units remain. Fail IN that prepayment, and again after it.
                let tail = std::mem::size_of::<SchemaWitnessEntry>() as u64 + 3;
                for allowance in [cost.checked_sub(tail + 1).unwrap(), cost - 1] {
                    let handles = session.stats().external_value_handles;
                    let _request = session.operation_budget_scope().unwrap();
                    session.store.set_fuel(allowance).unwrap();
                    let error = session.prepare_schema_witness(&schemas[16]).err().unwrap();
                    assert!(matches!(error, SessionError::Trap(ref e) if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
                    assert_eq!(session.schema_witness_cache.entries.as_ptr(), table);
                    assert_eq!(session.schema_witness_cache.entries.len(), 16);
                    assert_eq!(session.schema_witness_cache.next, target);
                    assert!(std::sync::Arc::ptr_eq(&weak.upgrade().unwrap(), &session.schema_witness_cache.entries[target].snapshot));
                    assert_eq!(session.stats().external_value_handles, handles);
                }
                // The counting walk is also cancellable before any mutation.
                {
                    let old = weak.upgrade().unwrap();
                    let _request = session.operation_budget_scope().unwrap();
                    let mut calls = 0;
                    let error = old.precharge_retirement(&mut |units| {
                        calls += 1;
                        if calls == 2 {
                            session.host_work_interrupt().store(true, std::sync::atomic::Ordering::SeqCst);
                        }
                        session.charge_host_work(units)
                    }).unwrap_err();
                    assert!(error.is_interrupt());
                    assert_eq!(session.schema_witness_cache.entries.as_ptr(), table);
                }
                let (fresh, publication) = schema_test_prepare(&mut session, &schemas[16]);
                publication.unwrap().publish(&mut session.schema_witness_cache);
                assert!(session.storage_matches(&schemas[16], &fresh).unwrap());
                if let Some(old) = older_fact {
                    assert!(std::sync::Arc::ptr_eq(&weak.upgrade().unwrap(), &old.schema_storage));
                    assert!(old.matches(&mut session, &receiver).unwrap());
                    drop(old);
                }
                assert!(weak.upgrade().is_none(), "last old witness owner retires only on successful publication/drop");
            }
        }
    }

    #[test]
    fn checked_schema_cache_seventeen_roots_evict_without_rejecting_and_reset_is_foreign() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = schema_test_session(phase);
            let name = session.data_scalar(&crate::portable::hir::Literal::String(vec![97])).unwrap();
            let schemas: Vec<_> = (0..17).map(|_| schema_test_array(&mut session, &[&name])).collect();
            let mut witnesses = Vec::new();
            session.schema_witness_cache = SchemaWitnessCache::default();
            for schema in &schemas {
                let (witness, pending) = schema_test_prepare(&mut session, schema);
                pending.unwrap().publish(&mut session.schema_witness_cache);
                assert!(session.schema_witness_cache.entries.len() <= SCHEMA_WITNESS_LIMIT);
                witnesses.push(witness);
            }
            assert_eq!(session.schema_witness_cache.entries.len(), 16);
            session.collect().unwrap();
            for (schema, witness) in schemas.iter().zip(&witnesses) {
                assert!(session.storage_matches(schema, witness).unwrap());
            }
            let (recaptured, pending) = schema_test_prepare(&mut session, &schemas[0]);
            assert!(pending.is_some(), "eviction is a cache miss, never rejection");
            assert!(!std::sync::Arc::ptr_eq(&recaptured, &witnesses[0]));
            pending.unwrap().publish(&mut session.schema_witness_cache);
            let mut foreign = schema_test_session(phase);
            let foreign_schema = schema_test_array(&mut foreign, &[]);
            {
                let _request = session.operation_budget_scope().unwrap();
                assert!(matches!(session.prepare_schema_witness(&foreign_schema), Err(SessionError::ForeignValue)));
            }
            session.reset().unwrap();
            assert!(session.schema_witness_cache.entries.is_empty());
            {
                let _request = session.operation_budget_scope().unwrap();
                assert!(matches!(session.prepare_schema_witness(&schemas[0]), Err(SessionError::ForeignValue)));
            }
        }
    }

    #[test]
    fn private_constructor_route_rejects_actual_guest_argument_escape() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = match phase {
                Phase::Runtime => Session::new_repl().unwrap(),
                Phase::Macro => Session::new_macro().unwrap(),
            };
            session.eval("(def private-route-events 0) (deftype PrivateRouteInput [value])").unwrap();
            let private = session.data_array(&[]).unwrap();
            let ordinary = session.eval("PrivateRouteInput").unwrap();
            let value = session.data_construct_without_guest_callbacks(&ordinary, &[&private]).unwrap();
            let fields = session.data_fields(&value).unwrap();
            assert!(same(&mut session, &fields[0], &private).unwrap());
            let class = {
                let mut scope = RootScope::new(&mut session.store);
                let function = session.runtime.get_func(&mut scope, "error-constructor").unwrap();
                super::super::call(&mut scope, session.runtime, function, &[], session.identity, &session.handles).unwrap()
            };
            let descriptor = session.data_descriptor(&class, true).unwrap();
            let replacement = session.eval("(fn [x] (do (set! private-route-events (+ private-route-events 1)) x))").unwrap();
            session.inspect(&descriptor, |mut store, value| {
                let descriptor = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let cache = descriptor.field(&mut store, 2)?.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                let original = class.value.to_rooted(&mut store);
                let mut changed = 0;
                for index in (1..cache.len(&store)?).step_by(2) {
                    if let Some(entry) = cache.get(&mut store, index)?.unwrap_anyref() {
                        if Rooted::<AnyRef>::ref_eq(&store, entry, &original)? {
                            let replacement = replacement.value.to_rooted(&mut store);
                            cache.set(&mut store, index, Val::AnyRef(Some(replacement)))?;
                            changed += 1;
                        }
                    }
                }
                assert_eq!(changed, 1, "must replace the actual constructor cache entry");
                Ok(())
            }).unwrap();
            session.collect().unwrap();
            let error = session.data_construct_without_guest_callbacks(&class, &[&private]).unwrap_err();
            assert!(error.to_string().contains("callback-free constructor"));
            let count = session.eval("private-route-events").unwrap();
            assert_eq!(crate::portable_repl::display(&mut session, &count).unwrap(), "0");
            let escaped = session.data_construct(&class, &[&private]).unwrap();
            assert!(same(&mut session, &escaped, &private).unwrap(), "ordinary special dispatch actually receives and returns the argument");
            let count = session.eval("private-route-events").unwrap();
            assert_eq!(crate::portable_repl::display(&mut session, &count).unwrap(), "1");

            // The same guard is used by a parent bridge, even when its captured
            // source constructor's owner changes after the fork was created.
            session.eval("(set! private-route-events 0)").unwrap();
            let bridge = crate::portable_macro_data::FormBridge::new(&mut session).unwrap();
            let guarded = bridge.private_parent_bridge(&mut session).unwrap();
            assert!(guarded.recording(&mut session).is_err());
            let old_replacement = replacement;
            let replacement = session.eval("(fn [meta first rest count hash] (do (set! private-route-events (+ private-route-events 1)) first))").unwrap();
            session.inspect(&descriptor, |mut store, value| {
                let descriptor = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let cache = descriptor.field(&mut store, 2)?.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                // The earlier replacement is the actual cached route now.
                let old = old_replacement.value.to_rooted(&mut store);
                let mut changed = 0;
                for index in (1..cache.len(&store)?).step_by(2) {
                    let entry = cache.get(&mut store, index)?;
                    if let Some(entry) = entry.unwrap_anyref() {
                        if Rooted::<AnyRef>::ref_eq(&store, entry, &old)? {
                            let replacement = replacement.value.to_rooted(&mut store);
                            cache.set(&mut store, index, Val::AnyRef(Some(replacement)))?;
                            changed += 1;
                        }
                    }
                }
                assert_eq!(changed, 1);
                Ok(())
            }).unwrap();
            let list = session.eval("suss.core/List").unwrap();
            session.inspect(&list, |mut store, value| {
                let closure = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let owner = closure.field(&mut store, 0)?.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let fields = owner.field(&mut store, 1)?.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                let descriptor = descriptor.value.to_rooted(&mut store);
                fields.set(&mut store, 0, Val::AnyRef(Some(descriptor)))
            }).unwrap();
            let fresh_private = session.data_array(&[]).unwrap();
            let error = guarded.list_values(&mut session, &[fresh_private.clone()]).unwrap_err();
            assert!(error.to_string().contains("callback-free constructor"));
            let count = session.eval("private-route-events").unwrap();
            assert_eq!(crate::portable_repl::display(&mut session, &count).unwrap(), "0");
            let escaped = bridge.list_values(&mut session, &[fresh_private.clone()]).unwrap();
            assert!(same(&mut session, &escaped, &fresh_private).unwrap());
            let count = session.eval("private-route-events").unwrap();
            assert_eq!(crate::portable_repl::display(&mut session, &count).unwrap(), "1");
        }
    }

    #[test]
    fn constructor_inputs_reject_properties_and_uid_changes_without_field_changes() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = match phase {
                Phase::Runtime => Session::new_repl().unwrap(),
                Phase::Macro => Session::new_macro().unwrap(),
            };
            session.eval("(deftype InputStorageProbe [a])").unwrap();
            let class = session.eval("InputStorageProbe").unwrap();
            let input = session.eval("17").unwrap();
            for slot in [2, 3] {
                for null in [false, true] {
                    let (value, facts) = session
                        .data_construct_with_inputs(&class, &[&input])
                        .unwrap();
                    let facts = facts.unwrap();
                    assert!(facts.matches(&mut session, &value).unwrap());
                    session
                        .inspect(&value, |mut store, value| {
                            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                            let poison = if null {
                                Val::null_any_ref()
                            } else {
                                Val::AnyRef(Some(input.value.to_rooted(&mut store)))
                            };
                            if slot == 2 {
                                let properties = object
                                    .field(&mut store, 2)?
                                    .unwrap_anyref()
                                    .unwrap()
                                    .as_array(&store)?
                                    .unwrap();
                                properties.set(&mut store, 0, poison)?;
                            } else {
                                object.set_field(&mut store, slot, poison)?;
                            }
                            Ok(())
                        })
                        .unwrap();
                    session.collect().unwrap();
                    let fields = session.data_fields(&value).unwrap();
                    assert!(same(&mut session, &fields[0], &input).unwrap());
                    assert!(
                        !facts.matches(&mut session, &value).unwrap(),
                        "phase={phase:?}, slot={slot}, null={null}"
                    );
                }
            }
        }
    }
    #[test]
    fn constructor_inputs_bind_receiver_and_preconstruction_fields_after_gc() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = match phase {
                Phase::Runtime => Session::new_repl().unwrap(),
                Phase::Macro => Session::new_macro().unwrap(),
            };
            session.eval("(deftype InputProbe [a b])").unwrap();
            let class = session.eval("InputProbe").unwrap();
            let a = session.eval("(list 17)").unwrap();
            let b = session.eval("99").unwrap();
            let (value, inputs) = session
                .data_construct_with_inputs(&class, &[&a, &b])
                .unwrap();
            let inputs = inputs.unwrap();
            session.collect().unwrap();
            assert!(inputs.matches(&mut session, &value).unwrap());
            let clone = session.data_construct(&class, &[&a, &b]).unwrap();
            assert!(!inputs.matches(&mut session, &clone).unwrap());
            // A matching input reference is not descendant-storage validation.
            // Retained-source reuse must combine this with independent checks.
            session
                .inspect(&a, |mut store, value| {
                    let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    let fields = object
                        .field(&mut store, 1)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    let b = b.value.to_rooted(&mut store);
                    fields.set(&mut store, 1, Val::AnyRef(Some(b)))?;
                    Ok(())
                })
                .unwrap();
            assert!(inputs.matches(&mut session, &value).unwrap());
            session
                .inspect(&value, |mut store, value| {
                    let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    let fields = object
                        .field(&mut store, 1)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    let b = b.value.to_rooted(&mut store);
                    fields.set(&mut store, 0, Val::AnyRef(Some(b)))?;
                    Ok(())
                })
                .unwrap();
            session.collect().unwrap();
            assert!(!inputs.matches(&mut session, &value).unwrap());
            let mut foreign = Session::with_options_in(Default::default(), phase).unwrap();
            let foreign_value = foreign.eval("17").unwrap();
            assert!(matches!(
                inputs.matches(&mut session, &foreign_value),
                Err(SessionError::ForeignValue)
            ));
            session.reset().unwrap();
            assert!(matches!(
                inputs.matches(&mut session, &value),
                Err(SessionError::ForeignValue)
            ));
        }
    }

    #[test]
    fn constructor_inputs_follow_padding_extras_duplicate_names_and_live_schema() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = match phase {
                Phase::Runtime => Session::new_repl().unwrap(),
                Phase::Macro => Session::new_macro().unwrap(),
            };
            session.eval("(deftype InputSchemaProbe [a b])").unwrap();
            let class = session.eval("InputSchemaProbe").unwrap();
            let a = session.eval("7").unwrap();
            let b = session.eval("11").unwrap();
            let extra = session.eval("99").unwrap();
            for args in [vec![&a], vec![&a, &b, &extra]] {
                let (value, inputs) = session.data_construct_with_inputs(&class, &args).unwrap();
                session.collect().unwrap();
                assert!(inputs.unwrap().matches(&mut session, &value).unwrap());
            }
            let (before, prior) = session
                .data_construct_with_inputs(&class, &[&a, &b])
                .unwrap();
            let descriptor = session.data_descriptor(&class, true).unwrap();
            session
                .inspect(&descriptor, |mut store, value| {
                    let descriptor = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    let schema = descriptor
                        .field(&mut store, 1)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    let a = schema.get(&mut store, 0)?;
                    schema.set(&mut store, 1, a)?;
                    Ok(())
                })
                .unwrap();
            assert!(!prior.unwrap().matches(&mut session, &before).unwrap());
            let (value, inputs) = session
                .data_construct_with_inputs(&class, &[&a, &b])
                .unwrap();
            session.collect().unwrap();
            assert!(inputs.unwrap().matches(&mut session, &value).unwrap());
            let fields = session.data_fields(&value).unwrap();
            assert!(same(&mut session, &fields[0], &b).unwrap());
            assert!(same(&mut session, &fields[1], &b).unwrap());
            let (padded, inputs) = session.data_construct_with_inputs(&class, &[&a]).unwrap();
            assert!(inputs.unwrap().matches(&mut session, &padded).unwrap());
            for units in [vec![], vec![0], vec![0xd800, 0, 0xdc00]] {
                let first = session
                    .data_scalar(&crate::portable::hir::Literal::String(units.clone()))
                    .unwrap();
                let second = session
                    .data_scalar(&crate::portable::hir::Literal::String(units))
                    .unwrap();
                assert!(!same(&mut session, &first, &second).unwrap());
                session
                    .inspect(&descriptor, |mut store, value| {
                        let descriptor = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                        let schema = descriptor
                            .field(&mut store, 1)?
                            .unwrap_anyref()
                            .unwrap()
                            .as_array(&store)?
                            .unwrap();
                        let first = first.value.to_rooted(&mut store);
                        let second = second.value.to_rooted(&mut store);
                        schema.set(&mut store, 0, Val::AnyRef(Some(first)))?;
                        schema.set(&mut store, 1, Val::AnyRef(Some(second)))?;
                        Ok(())
                    })
                    .unwrap();
                let (value, inputs) = session
                    .data_construct_with_inputs(&class, &[&a, &b])
                    .unwrap();
                assert!(inputs.unwrap().matches(&mut session, &value).unwrap());
                let fields = session.data_fields(&value).unwrap();
                assert!(same(&mut session, &fields[0], &b).unwrap());
                assert!(same(&mut session, &fields[1], &b).unwrap());
            }
            session
                .inspect(&descriptor, |mut store, value| {
                    let descriptor = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    let schema = descriptor
                        .field(&mut store, 1)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    let name = a.value.to_rooted(&mut store);
                    schema.set(&mut store, 0, Val::AnyRef(Some(name)))?;
                    schema.set(&mut store, 1, Val::AnyRef(Some(name)))?;
                    Ok(())
                })
                .unwrap();
            let (value, inputs) = session
                .data_construct_with_inputs(&class, &[&a, &b])
                .unwrap();
            assert!(inputs.unwrap().matches(&mut session, &value).unwrap());
            let fields = session.data_fields(&value).unwrap();
            assert!(same(&mut session, &fields[0], &a).unwrap());
            assert!(same(&mut session, &fields[1], &b).unwrap());
        }
    }

    #[test]
    fn constructor_inputs_decline_special_identity_and_preserve_shared_budget() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = match phase {
                Phase::Runtime => Session::new_repl().unwrap(),
                Phase::Macro => Session::new_macro().unwrap(),
            };
            fn runtime_value(session: &mut Session, name: &str) -> SessionValue {
                session.budget().unwrap();
                let mut scope = RootScope::new(&mut session.store);
                let function = session.runtime.get_func(&mut scope, name).unwrap();
                super::super::call(
                    &mut scope,
                    session.runtime,
                    function,
                    &[],
                    session.identity,
                    &session.handles,
                )
                .unwrap()
            }
            let object = runtime_value(&mut session, "object-constructor");
            // The existing Object adapter accepts its owned native objects.
            // Language lists are unsupported by both routes; preserve that
            // boundary rather than assuming every language object is accepted.
            let list = session.eval("(list 17)").unwrap();
            let ordinary = session.data_construct(&object, &[&list]).unwrap_err();
            let tracked = session
                .data_construct_with_inputs(&object, &[&list])
                .unwrap_err();
            let (SessionError::Language(ordinary), SessionError::Language(tracked)) =
                (ordinary, tracked)
            else {
                panic!("Expected the same ordinary language-error boundary");
            };
            let ordinary_kind = field(&mut session, &ordinary, 0).unwrap();
            let tracked_kind = field(&mut session, &tracked, 0).unwrap();
            assert!(same(&mut session, &ordinary_kind, &tracked_kind).unwrap());
            fn message(session: &mut Session, payload: &SessionValue) -> Vec<u16> {
                session
                    .inspect(payload, |mut store, value| {
                        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                        let text = object
                            .field(&mut store, 1)?
                            .unwrap_anyref()
                            .unwrap()
                            .as_array(&store)?
                            .unwrap();
                        Ok(text
                            .elems(&mut store)?
                            .map(|unit| unit.unwrap_i32() as u16)
                            .collect())
                    })
                    .unwrap()
            }
            assert_eq!(
                message(&mut session, &ordinary),
                message(&mut session, &tracked)
            );
            drop((ordinary, tracked, ordinary_kind, tracked_kind, list));
            let input = runtime_value(&mut session, "native-object-default-new");
            let (value, inputs) = session
                .data_construct_with_inputs(&object, &[&input])
                .unwrap();
            assert!(inputs.is_none());
            assert!(same(&mut session, &value, &input).unwrap());
            session.eval("(deftype InputBudgetProbe [a b])").unwrap();
            let class = session.eval("InputBudgetProbe").unwrap();
            session.schema_witness_cache = SchemaWitnessCache::default();
            let handles = session.stats().external_value_handles;
            {
                let (value, inputs) = session
                    .data_construct_with_inputs(&class, &[&input, &input])
                    .unwrap();
                assert!(
                    inputs
                        .as_ref()
                        .unwrap()
                        .matches(&mut session, &value)
                        .unwrap()
                );
                drop(inputs);
                drop(value);
            }
            // Cache retention is intentional; release its owners before the
            // existing exact no-leak assertion (the new tests check retention).
            session.schema_witness_cache = SchemaWitnessCache::default();
            assert_eq!(session.stats().external_value_handles, handles);
            // Sample failure cleanup; this does not claim distinct stage coverage.
            for allowance in [20, 40, 80, 160, 320, 640, 1280] {
                let request = session.operation_budget_scope().unwrap();
                session.store.set_fuel(allowance).unwrap();
                match session.data_construct_with_inputs(&class, &[&input, &input]) {
                    Ok((value, inputs)) => {
                        drop(inputs);
                        drop(value);
                    }
                    Err(error) => assert!(
                        matches!(error, SessionError::Trap(ref error) if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel))
                    ),
                }
                session.schema_witness_cache = SchemaWitnessCache::default();
                assert_eq!(session.stats().external_value_handles, handles);
                drop(request);
            }
            let request = session.operation_budget_scope().unwrap();
            session.store.set_fuel(0).unwrap();
            let error = session
                .data_construct_with_inputs(&class, &[&input, &input])
                .unwrap_err();
            assert!(
                matches!(error, SessionError::Trap(ref error) if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel))
            );
            drop(request);
            let (value, inputs) = session
                .data_construct_with_inputs(&class, &[&input, &input])
                .unwrap();
            assert!(inputs.unwrap().matches(&mut session, &value).unwrap());
            let request = session.operation_budget_scope().unwrap();
            session.interrupt_handle().interrupt();
            let error = session
                .data_construct_with_inputs(&class, &[&input, &input])
                .unwrap_err();
            assert!(error.is_interrupt(), "{error:?}");
            drop(request);
            let (value, inputs) = session
                .data_construct_with_inputs(&class, &[&input, &input])
                .unwrap();
            assert!(inputs.unwrap().matches(&mut session, &value).unwrap());
        }
    }
    #[test]
    fn checked_schema_private_roots_preserve_external_stats_failures_and_fact_lifetimes() {
        use std::sync::{Arc, atomic::Ordering};
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = schema_test_session(phase);
            session.eval("(deftype PrivateSchemaAccounting [a b])").unwrap();
            let class = session.eval("PrivateSchemaAccounting").unwrap();
            let local = session.eval("17").unwrap();
            let external = session.stats().external_value_handles;
            assert!(session.schema_witness_cache.entries.is_empty());
            let (value, fact) = session.data_construct_with_inputs(&class, &[&local, &local]).unwrap();
            let fact = fact.unwrap();
            assert_eq!(session.schema_witness_cache.entries.len(), 1);
            let counter = session.schema_witness_cache.entries[0].schema.handles.clone();
            assert!(!Arc::ptr_eq(&counter, &session.handles));
            // Exactly one schema-array root, two UTF16 name-array roots,
            // plus the cache's separately owned schema handle. No hidden stats offset.
            assert_eq!(counter.load(Ordering::Relaxed), 4);
            assert!(Arc::ptr_eq(&fact.schema_storage, &session.schema_witness_cache.entries[0].snapshot));
            let weak = Arc::downgrade(&fact.schema_storage);
            assert!(fact.matches(&mut session, &value).unwrap());
            let held_external = session.stats().external_value_handles;
            // All rejected owner/poll work leaves current cache identity,
            // private roots, publication cursor and caller roots exactly intact.
            let cursor = session.schema_witness_cache.next;
            let mut other = Session::new().unwrap();
            let foreign = other.eval("99").unwrap();
            for interrupt in [false, true] {
                let request = session.operation_budget_scope().unwrap();
                session.store.set_fuel(0).unwrap();
                if interrupt { session.interrupt_handle().interrupt(); }
                assert!(matches!(session.data_construct_with_inputs(&class, &[&foreign]), Err(SessionError::ForeignValue)));
                let error = session.data_construct_with_inputs(&class, &[&local, &foreign]).unwrap_err();
                if interrupt { assert!(error.is_interrupt(), "{error:?}"); }
                else { assert!(matches!(error, SessionError::Trap(ref e) if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel))); }
                assert_eq!(session.stats().external_value_handles, held_external);
                assert_eq!(counter.load(Ordering::Relaxed), 4);
                assert_eq!(session.schema_witness_cache.next, cursor);
                assert_eq!(session.schema_witness_cache.entries.len(), 1);
                assert!(Arc::ptr_eq(&weak.upgrade().unwrap(), &session.schema_witness_cache.entries[0].snapshot));
                drop(request);
            }
            // A genuine hit shares the existing witness and creates no roots
            // after its returned value/fact have been dropped.
            let again = session.data_construct_with_inputs(&class, &[&local, &local]).unwrap();
            assert!(Arc::ptr_eq(&again.1.as_ref().unwrap().schema_storage, &fact.schema_storage));
            drop(again);
            assert_eq!(session.stats().external_value_handles, held_external);
            assert_eq!(counter.load(Ordering::Relaxed), 4);
            // Retiring only cache ownership leaves the fact's independent
            // private witness alive and complete across GC.
            session.schema_witness_cache = SchemaWitnessCache::default();
            assert_eq!(counter.load(Ordering::Relaxed), 3);
            session.collect().unwrap();
            assert!(fact.matches(&mut session, &value).unwrap());
            assert!(weak.upgrade().is_some());
            // Existing real cancellation seam, not a replacement publication.
            let request = session.operation_budget_scope().unwrap();
            SCHEMA_CANCEL_STAGE.with(|stage| stage.set(Some("factory")));
            let error = session.data_construct_with_inputs(&class, &[&local, &local]).unwrap_err();
            assert!(error.is_interrupt(), "{error:?}");
            assert!(session.schema_witness_cache.entries.is_empty());
            assert_eq!(session.stats().external_value_handles, held_external);
            assert_eq!(counter.load(Ordering::Relaxed), 3);
            drop(request);
            let retry = session.data_construct_with_inputs(&class, &[&local, &local]).unwrap();
            let retry_counter = session.schema_witness_cache.entries[0].schema.handles.clone();
            assert!(!Arc::ptr_eq(&counter, &retry_counter));
            assert_eq!(retry_counter.load(Ordering::Relaxed), 4);
            drop(retry);
            session.schema_witness_cache = SchemaWitnessCache::default();
            assert_eq!(retry_counter.load(Ordering::Relaxed), 0);
            drop((fact, value));
            assert_eq!(counter.load(Ordering::Relaxed), 0);
            assert!(weak.upgrade().is_none());
            assert_eq!(session.stats().external_value_handles, external);
            let (reset_value, reset_fact) = session.data_construct_with_inputs(&class, &[&local, &local]).unwrap();
            let reset_fact = reset_fact.unwrap();
            let reset_counter = session.schema_witness_cache.entries[0].schema.handles.clone();
            let reset_weak = Arc::downgrade(&reset_fact.schema_storage);
            assert_eq!(reset_counter.load(Ordering::Relaxed), 4);
            session.reset().unwrap();
            assert_eq!(session.stats().external_value_handles, 0);
            assert!(session.schema_witness_cache.entries.is_empty());
            assert_eq!(reset_counter.load(Ordering::Relaxed), 3);
            assert!(reset_weak.upgrade().is_some());
            assert!(matches!(reset_fact.matches(&mut session, &reset_value), Err(SessionError::ForeignValue)));
            drop((reset_fact, reset_value));
            assert!(reset_weak.upgrade().is_none());
            assert_eq!(reset_counter.load(Ordering::Relaxed), 0);
        }
    }

}

impl ConstructorInputs {
    pub(super) fn memo_watch(&self)->Option<std::sync::Arc<super::memo_bridge::Watch>> {self.memo_watch.clone()}
    pub(super) fn write_cuts(&self)->Vec<&SessionValue> {
        let mut cuts=vec![&self.descriptor];
        cuts.extend(self.fields.iter().filter_map(Option::as_ref));
        if let Some(origin)=&self.prototype_origin {cuts.extend(origin.write_cuts());}
        cuts
    }
}
