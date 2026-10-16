//! Protected Macro-bootstrap memo writes, witnessed only for registered inputs.
//! The ledger holds Weak watches; ConstructorInputs owns every live watch/root.
//! No generic setter, public registration or uninstrumented Runtime store mints.
use super::{Session, SessionError, SessionValue, own, registry_lock};
use super::canonical_methods::{CanonicalMethods, InspectionContext};
use crate::portable::{bootstrap, resolve::Phase};
use std::sync::{Arc, Mutex, Weak, atomic::{AtomicBool, AtomicUsize}};
use wasmtime::{AnyRef, AsContextMut, Func, FuncType, Instance, Linker, RefType,
    RootScope, Rooted, Store, StoreContextMut, StructType, Val, ValType};

#[derive(Debug)]
pub(super) struct Receipt {
    value: SessionValue,
    // Independent scalar contents; canonical Number payloads are immutable.
    bits: u64,
    source_safe: bool,
}
#[derive(Debug)]
pub(super) struct Watch {
    identity: u64,
    site: usize,
    owner: &'static str,
    slot: usize,
    receiver: SessionValue,
    descriptor: SessionValue,
    constructor: SessionValue,
    storage: SessionValue,
    number_type: StructType,
    eligible_transition: bool,
    receipt: Mutex<Option<Receipt>>,
    source_domain: Mutex<Option<Weak<super::write_domain::WriteDomain>>>,
}
struct Entries {
    authority: Option<Arc<CanonicalMethods>>,
    prototype: Option<Arc<super::prototype_origin::Authority>>,
    watches: Vec<Weak<Watch>>,
}
pub(super) struct Ledger {
    identity: u64,
    entries: Mutex<Entries>,
    handles: Arc<AtomicUsize>,
}
impl Ledger {
    pub(super) fn new(identity: u64) -> Arc<Self> {
        Arc::new(Self {
            identity,
            entries: Mutex::new(Entries { authority: None, prototype: None, watches: Vec::new() }),
            handles: Arc::new(AtomicUsize::new(0)),
        })
    }
    // Called once inside initial provisioning, never after caller exposure.
    pub(super) fn seal(&self, authority: Arc<CanonicalMethods>,prototype:Arc<super::prototype_origin::Authority>) -> Result<(), SessionError> {
        let mut entries = registry_lock(&self.entries);
        if entries.authority.is_some() {
            return Err(wasmtime::Error::msg("Memo authority already sealed").into());
        }
        entries.authority = Some(authority);
        entries.prototype=Some(prototype);
        Ok(())
    }
    // Metered weak-list compaction. No guest call (including adapters) under lock.
    pub(super) fn sweep(&self, mut store: StoreContextMut<'_, ()>, irq: &AtomicBool) -> wasmtime::Result<()> {
        let mut entries = registry_lock(&self.entries);
        let mut index = 0;
        while index < entries.watches.len() {
            Session::consume_host_work(store.as_context_mut(), 1, irq)?;
            if entries.watches[index].strong_count() == 0 {
                entries.watches.swap_remove(index);
            } else { index += 1; }
        }
        Ok(())
    }
}
fn same(store: &StoreContextMut<'_, ()>, a: &Val, b: &Val) -> wasmtime::Result<bool> {
    match (a.anyref().flatten(), b.anyref().flatten()) {
        (Some(a), Some(b)) => Rooted::<AnyRef>::ref_eq(store, a, b),
        _ => Ok(false),
    }
}
fn nil(store: &StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<bool> {
    Ok(match value.anyref().flatten() {
        Some(value) => value.as_i31(store)?.is_some_and(|v| v.get_u32() == 0),
        None => false,
    })
}
fn bits(mut store: StoreContextMut<'_, ()>, value: &Val, ty: &StructType) -> wasmtime::Result<Option<u64>> {
    let Some(value) = value.anyref().flatten() else { return Ok(None) };
    let Some(number) = value.as_struct(&store)? else { return Ok(None) };
    if !StructType::eq(&number.ty(&store)?, ty) { return Ok(None) }
    Ok(match number.field(&mut store, 0)? { Val::F64(bits) => Some(bits), _ => None })
}
impl Watch {
    fn current(&self, mut store: StoreContextMut<'_, ()>, receiver: &Val,
        irq: &AtomicBool) -> wasmtime::Result<Option<Val>> {
        Session::consume_host_work(store.as_context_mut(), 8, irq)?;
        let expected = Val::AnyRef(Some(self.receiver.value.to_rooted(&mut store)));
        if !same(&store, receiver, &expected)? { return Ok(None) }
        let Some(object) = receiver.anyref().flatten().unwrap().as_struct(&store)? else { return Ok(None) };
        let descriptor = object.field(&mut store, 0)?;
        let expected = Val::AnyRef(Some(self.descriptor.value.to_rooted(&mut store)));
        if !same(&store, &descriptor, &expected)? { return Ok(None) }
        let storage = object.field(&mut store, 1)?;
        let expected = Val::AnyRef(Some(self.storage.value.to_rooted(&mut store)));
        if !same(&store, &storage, &expected)? { return Ok(None) }
        let Some(array) = storage.anyref().flatten().unwrap().as_array(&store)? else { return Ok(None) };
        if self.slot >= array.len(&store)? as usize { return Ok(None) }
        Ok(Some(array.get(&mut store, self.slot as u32)?))
    }
    fn authenticated(&self, mut store: StoreContextMut<'_, ()>, value: &Val,
        irq: &AtomicBool) -> wasmtime::Result<bool> {
        Session::consume_host_work(store.as_context_mut(), 4, irq)?;
        let receipt = registry_lock(&self.receipt);
        let Some(receipt) = receipt.as_ref() else { return Ok(false) };
        if let Some(domain)=registry_lock(&self.source_domain).as_ref() {
            if !receipt.source_safe || domain.upgrade().is_none() {return Ok(false);}
        }
        let expected = Val::AnyRef(Some(receipt.value.value.to_rooted(&mut store)));
        Ok(same(&store, value, &expected)? &&
            bits(store.as_context_mut(), value, &self.number_type)? == Some(receipt.bits))
    }
    pub(super) fn canonical(&self, store: StoreContextMut<'_, ()>, runtime: Instance,
        authority: &CanonicalMethods, irq: &AtomicBool) -> wasmtime::Result<bool> {
        self.method_valid(store, runtime, authority, irq, true)
    }
    fn method_valid(&self, mut store: StoreContextMut<'_, ()>, runtime: Instance,
        authority: &CanonicalMethods, irq: &AtomicBool, require_initial: bool,
    ) -> wasmtime::Result<bool> {
        let descriptor = Val::AnyRef(Some(self.descriptor.value.to_rooted(&mut store)));
        let context = InspectionContext { store, identity: self.identity, runtime, interrupt: irq };
        let result = if require_initial {
            authority.matches_in_context(context, self.owner, descriptor)
        } else {
            authority.method_layout_in_context(context, self.owner, descriptor)
        };
        match result {
            Ok(valid) => Ok(valid),
            // A malformed protocol table is failed inspection, not an extra
            // guest setter exception. Fuel/cancellation/host errors still escape.
            Err(SessionError::Language(_)) => Ok(false),
            Err(SessionError::Host(error) | SessionError::Trap(error)) => Err(error),
            Err(other) => Err(wasmtime::Error::msg(other.to_string())),
        }
    }
    pub(super) fn domain_method(&self, mut store: StoreContextMut<'_, ()>, runtime: Instance,
        authority: &CanonicalMethods, irq: &AtomicBool, layout_descriptor: Option<&Val>,
    ) -> wasmtime::Result<bool> {
        Session::consume_host_work(store.as_context_mut(), 1, irq)?;
        let descriptor = Val::AnyRef(Some(self.descriptor.value.to_rooted(&mut store)));
        let relaxed = if let Some(active) = layout_descriptor { same(&store, &descriptor, active)? } else { false };
        self.method_valid(store, runtime, authority, irq, !relaxed)
    }
    pub(super) fn slot(&self) -> usize { self.slot }
    pub(super) fn matches(&self, session: &mut Session) -> Result<bool, SessionError> {
        session.check(&self.receiver)?;
        let _request = session.operation_budget_scope()?;
        session.charge_host_work(1)?;
        if registry_lock(&self.receipt).is_none() { return Ok(false) }
        let Some(authority) = session.canonical_methods.clone() else { return Ok(false) };
        // Schema and constructor identity are initial-source authority, not live
        // public names. Unrelated protocols and live hash helper bindings survive.
        if !authority.source_constructor_matches(session, &self.constructor, &self.descriptor)? {
            return Ok(false);
        }
        let irq = session.host_work_interrupt();
        let runtime = session.runtime;
        let mut scope = RootScope::new(&mut session.store);
        // Authenticate the existing transition independently of a later valid
        // method replacement. New receipts still require canonical write sites.
        if !self.method_valid(scope.as_context_mut(), runtime, &authority, &irq, false)? { return Ok(false) }
        let receiver = Val::AnyRef(Some(self.receiver.value.to_rooted(&mut scope)));
        let Some(current) = self.current(scope.as_context_mut(), &receiver, &irq)? else { return Ok(false) };
        Ok(self.authenticated(scope.as_context_mut(), &current, &irq)?)
    }
}

impl Session {
    pub(super) fn watch_constructor_memo(&mut self, class: &SessionValue,
        descriptor: &SessionValue, receiver: &SessionValue, storage: &SessionValue,
        fields: &[Option<SessionValue>]) -> Result<Option<Arc<Watch>>, SessionError> {
        if self.phase != Phase::Macro { return Ok(None) }
        let Some(authority) = self.canonical_methods.clone() else { return Ok(None) };
        let Some((site, owner, slot)) = authority.constructor_memo_site(self, class, descriptor)? else {
            return Ok(None);
        };
        let Some(Some(initial)) = fields.get(slot) else { return Ok(None) };
        let irq = self.host_work_interrupt();
        let ledger = self.memo_ledger.clone();
        // Only a preconstruction nil slot may begin an authenticated transition.
        let eligible_transition = {
            let mut scope = RootScope::new(&mut self.store);
            let initial = Val::AnyRef(Some(initial.value.to_rooted(&mut scope)));
            nil(&scope.as_context_mut(), &initial)?
        };
        self.charge_host_work((std::mem::size_of::<Watch>() + std::mem::size_of::<Weak<Watch>>()) as u64 + 1)?;
        let zero = self.data_scalar(&crate::portable::hir::Literal::Number(0.0))?;
        let mut scope = RootScope::new(&mut self.store);
        let number_type = zero.value.to_rooted(&mut scope).as_struct(&scope)?.unwrap().ty(&scope)?;
        let mut private = |value: &SessionValue| -> Result<SessionValue, SessionError> {
            let value = Val::AnyRef(Some(value.value.to_rooted(&mut scope)));
            own(&mut scope, value, self.identity, &ledger.handles)
        };
        let watch = Arc::new(Watch { identity: self.identity, site, owner, slot,
            receiver: private(receiver)?, descriptor: private(descriptor)?, storage: private(storage)?,
            constructor: private(class)?, number_type, eligible_transition, receipt: Mutex::new(None), source_domain:Mutex::new(None) });
        ledger.sweep(scope.as_context_mut(), &irq)?;
        let mut entries = registry_lock(&ledger.entries);
        if entries.watches.len() == entries.watches.capacity() {
            let capacity = entries.watches.capacity().checked_mul(2)
                .ok_or_else(|| wasmtime::Error::msg("Memo watch capacity overflow"))?.max(4);
            let bytes = capacity.checked_add(entries.watches.len())
                .and_then(|count| count.checked_mul(std::mem::size_of::<Weak<Watch>>()))
                .and_then(|bytes| u64::try_from(bytes).ok())
                .ok_or_else(|| wasmtime::Error::msg("Memo watch allocation overflow"))?;
            Session::consume_host_work(scope.as_context_mut(), bytes + 1, &irq)?;
            let extra = capacity - entries.watches.len();
            entries.watches.try_reserve_exact(extra)
                .map_err(|_| wasmtime::Error::msg("Memo watch allocation failed"))?;
        }
        entries.watches.push(Arc::downgrade(&watch));
        Ok(Some(watch))
    }
}

// Reserve only candidates that passed receiver/site/canonical inspection.
// Every allocation and possible relocation is charged before guest mutation.
fn reserve_candidate(
    mut store: StoreContextMut<'_, ()>,
    candidates: &mut Vec<(Arc<Watch>, Receipt, bool)>,
    irq: &AtomicBool,
) -> wasmtime::Result<()> {
    Session::consume_host_work(store.as_context_mut(), 1, irq)?;
    if candidates.len() < candidates.capacity() { return Ok(()); }
    let capacity = candidates.capacity().checked_mul(2)
        .ok_or_else(|| wasmtime::Error::msg("Memo staging capacity overflow"))?.max(1);
    let bytes = capacity.checked_add(candidates.len())
        .and_then(|count| count.checked_mul(std::mem::size_of::<(Arc<Watch>, Receipt, bool)>()))
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or_else(|| wasmtime::Error::msg("Memo staging allocation overflow"))?;
    Session::consume_host_work(store.as_context_mut(), bytes, irq)?;
    candidates.try_reserve_exact(capacity - candidates.len())
        .map_err(|_| wasmtime::Error::msg("Memo staging allocation failed"))?;
    Ok(())
}

pub(super) fn install(
    store: &mut Store<()>, linker: &mut Linker<()>, runtime: Instance,
    interrupt: Arc<AtomicBool>, ledger: Arc<Ledger>,
) -> wasmtime::Result<()> {
    let setter = runtime.get_func(&mut *store, "macro-memo-field-set")
        .or_else(|| runtime.get_func(&mut *store, "object-field-set"))
        .ok_or_else(|| wasmtime::Error::msg("Missing canonical memo setter"))?;
    let ty = FuncType::new(store.engine(), [ValType::Ref(RefType::EQREF), ValType::I32,
        ValType::Ref(RefType::EQREF)], []);
    let bridge = Func::new(&mut *store, ty, move |mut caller, arguments, _results| {
        Session::consume_host_work(caller.as_context_mut(), 32, &interrupt)?;
        let site = usize::try_from(arguments[1].unwrap_i32())
            .map_err(|_| wasmtime::Error::msg("Invalid canonical memo site"))?;
        let (_, field) = bootstrap::memo_owner_slots().nth(site)
            .ok_or_else(|| wasmtime::Error::msg("Unknown canonical memo site"))?;
        ledger.sweep(caller.as_context_mut(), &interrupt)?;
        // Stage only live matching watches. Locks never span runtime inspection
        // or the setter. All fallible capacity work precedes guest mutation.
        let (authority, prototype, count) = {
            let entries = registry_lock(&ledger.entries);
            (entries.authority.clone(), entries.prototype.clone(), entries.watches.len())
        };
        let mut candidates = Vec::new();
        if let Some(authority) = authority.as_ref() {
            for index in 0..count {
                Session::consume_host_work(caller.as_context_mut(), 1, &interrupt)?;
                let watch = { registry_lock(&ledger.entries).watches[index].upgrade() };
                let Some(watch) = watch else { continue };
                if !watch.eligible_transition || watch.site != site || watch.identity != ledger.identity { continue }
                let Some(current) = watch.current(caller.as_context_mut(), &arguments[0], &interrupt)? else { continue };
                if !(nil(&caller.as_context_mut(), &current)? ||
                    watch.authenticated(caller.as_context_mut(), &current, &interrupt)?) { continue }
                let Some(bits) = bits(caller.as_context_mut(), &arguments[2], &watch.number_type)? else { continue };
                if watch.canonical(caller.as_context_mut(), runtime, authority, &interrupt)? {
                    reserve_candidate(caller.as_context_mut(), &mut candidates, &interrupt)?;
                    Session::consume_host_work(caller.as_context_mut(), std::mem::size_of::<Receipt>() as u64 + 1, &interrupt)?;
                    let value = {
                        let mut scope = RootScope::new(&mut caller);
                        own(&mut scope, arguments[2].clone(), ledger.identity, &ledger.handles)
                            .map_err(|error| wasmtime::Error::msg(error.to_string()))?
                    };
                    let source_safe=if let Some(prototype)=prototype.as_ref() {
                        watch.domain_matches(caller.as_context_mut(),runtime,authority,prototype,&interrupt,None)?
                    } else {false};
                    candidates.push((watch, Receipt { value, bits, source_safe }, false));
                }
            }
        }
        // Exact ordinary setter, including the original language exception and
        // pending-exception state. Failure publishes no staged receipt.
        setter.call(&mut caller, &[arguments[0].clone(), Val::I32(field as i32), arguments[2].clone()], &mut [])?;
        for (watch, receipt, valid) in &mut candidates {
            Session::consume_host_work(caller.as_context_mut(), 8, &interrupt)?;
            if !watch.canonical(caller.as_context_mut(), runtime, authority.as_ref().unwrap(), &interrupt)? { continue }
            let Some(current) = watch.current(caller.as_context_mut(), &arguments[0], &interrupt)? else { continue };
            if !same(&caller.as_context_mut(), &current, &arguments[2])? { continue }
            *valid = bits(caller.as_context_mut(), &current, &watch.number_type)? == Some(receipt.bits);
            if receipt.source_safe {
                receipt.source_safe=watch.domain_matches(caller.as_context_mut(),runtime,authority.as_ref().unwrap(),prototype.as_ref().unwrap(),&interrupt,Some(receipt))?;
            }
        }
        // All fallible validation/allocation completes before any publication.
        // Traps after the setter leave new state conservatively unauthenticated.
        for (watch, receipt, valid) in candidates {
            if valid { *registry_lock(&watch.receipt) = Some(receipt); }
        }
        Ok(())
    });
    linker.define(&*store, bootstrap::COMPILER_INTEGRITY_IMPORT_MODULE,
        bootstrap::MEMO_WRITE_IMPORT_NAME, bridge)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{portable::resolve::Phase, portable_session::SessionError};

    #[test]
    fn private_memo_bridge_preserves_hash_miss_hit_and_cache_field_after_gc() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = match phase {
                Phase::Runtime => Session::new_repl().unwrap(),
                Phase::Macro => Session::new_macro().unwrap(),
            };
            session.eval("(def bridge-value (list 1 2))").unwrap();
            session.enter_namespace("suss.core").unwrap();
            session
                .eval("(def hash-ordered-coll (fn [_] 719))")
                .unwrap();
            session.enter_namespace("user").unwrap();
            let first = session.eval("(hash bridge-value)").unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &first).unwrap(),
                "719"
            );
            let value = session.eval("bridge-value").unwrap();
            session.collect().unwrap();
            let fields = session.data_fields(&value).unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &fields[4]).unwrap(),
                "719"
            );
            session.enter_namespace("suss.core").unwrap();
            session
                .eval("(def hash-ordered-coll (fn [_] (throw 8801)))")
                .unwrap();
            session.enter_namespace("user").unwrap();
            let hit = session.eval("(hash bridge-value)").unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &hit).unwrap(),
                "719"
            );
        }
    }

    fn guest_setter(session: &mut Session, setter: Func) -> Func {
        use wasm_encoder::*;
        let mut module = Module::new();
        let mut types = TypeSection::new();
        types.ty().function(
            [
                ValType::Ref(RefType::EQREF),
                ValType::I32,
                ValType::Ref(RefType::EQREF),
            ],
            [],
        );
        module.section(&types);
        let mut imports = ImportSection::new();
        imports.import("test", "setter", EntityType::Function(0));
        module.section(&imports);
        let mut functions = FunctionSection::new();
        functions.function(0);
        module.section(&functions);
        let mut exports = ExportSection::new();
        exports.export("run", ExportKind::Func, 1);
        module.section(&exports);
        let mut code = CodeSection::new();
        let mut body = Function::new([]);
        for instruction in [
            Instruction::LocalGet(0),
            Instruction::LocalGet(1),
            Instruction::LocalGet(2),
            Instruction::Call(0),
            Instruction::End,
        ] {
            body.instruction(&instruction);
        }
        code.function(&body);
        module.section(&code);
        let module = wasmtime::Module::new(&session.engine, module.finish()).unwrap();
        let instance = Instance::new(&mut session.store, &module, &[setter.into()]).unwrap();
        instance.get_func(&mut session.store, "run").unwrap()
    }

    #[test]
    fn private_memo_bridge_preserves_language_exception_payload_across_nested_runtime_call() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = Session::with_options_in(Default::default(), phase).unwrap();
            let receiver = session.eval("42").unwrap();
            let bridge = session
                .linker
                .get(
                    &mut session.store,
                    bootstrap::COMPILER_INTEGRITY_IMPORT_MODULE,
                    bootstrap::MEMO_WRITE_IMPORT_NAME,
                )
                .unwrap()
                .into_func()
                .unwrap();
            let setter = session
                .runtime
                .get_func(&mut session.store, "object-field-set")
                .unwrap();
            let mut payloads = Vec::new();
            // The first route crosses Wasm -> native bridge -> runtime Wasm.
            // The ordinary setter provides the independently observed control.
            for (function, index) in [(bridge, 0), (setter, 4)] {
                let function = guest_setter(&mut session, function);
                let mut scope = wasmtime::RootScope::new(&mut session.store);
                let value = Val::AnyRef(Some(receiver.value.to_rooted(&mut scope)));
                let error = function
                    .call(
                        &mut scope,
                        &[value.clone(), Val::I32(index), value],
                        &mut [],
                    )
                    .unwrap_err();
                let error = super::super::execution_error(
                    &mut scope,
                    session.runtime,
                    error,
                    session.identity,
                    &session.handles,
                );
                let SessionError::Language(payload) = error else {
                    panic!("{error:?}")
                };
                assert!(!scope.as_context_mut().has_pending_exception());
                payloads.push(payload);
            }
            session.collect().unwrap();
            for payload in &payloads {
                session
                    .inspect(payload, |mut store, value| {
                        let error = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                        let control = payloads[1].value.to_rooted(&mut store);
                        let control = control.as_struct(&store)?.unwrap();
                        let found = error.field(&mut store, 0)?;
                        let expected = control.field(&mut store, 0)?;
                        assert!(wasmtime::Rooted::<wasmtime::AnyRef>::ref_eq(
                            &store,
                            found.unwrap_anyref().unwrap(),
                            expected.unwrap_anyref().unwrap()
                        )?);
                        let message = error
                            .field(&mut store, 1)?
                            .unwrap_anyref()
                            .unwrap()
                            .as_array(&store)?
                            .unwrap();
                        let units: Vec<_> = message
                            .elems(&mut store)?
                            .map(|unit| unit.unwrap_i32() as u16)
                            .collect();
                        assert_eq!(
                            units,
                            "Invalid nominal operation"
                                .encode_utf16()
                                .collect::<Vec<_>>()
                        );
                        Ok(())
                    })
                    .unwrap();
            }
            let recovery = session.eval("43").unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &recovery).unwrap(),
                "43"
            );
        }
    }
}

#[cfg(test)]
mod receipt_tests {
    use super::*;
    use super::super::ConstructorInputs;

    fn registered_list(session: &mut Session) -> (SessionValue, ConstructorInputs) {
        let original = session.eval("(list 17)").unwrap();
        let fields = session.data_fields(&original).unwrap();
        let class = session.eval("suss.core/List").unwrap();
        let args = fields.iter().collect::<Vec<_>>();
        let (value, inputs) = session.data_construct_with_inputs(&class, &args).unwrap();
        (value, inputs.unwrap())
    }
    fn hash(session: &mut Session, value: &SessionValue) {
        let function = session.eval("(fn [x] (hash x))").unwrap();
        drop(session.invoke(&function, &[value]).unwrap());
    }
    fn write(session: &mut Session, value: &SessionValue) {
        let function = session.eval("(fn [x] (set! (.-__hash x) 991))").unwrap();
        drop(session.invoke(&function, &[value]).unwrap());
    }
    #[test]
    fn memo_receipts_unmatched_watches_do_not_require_staging_storage() {
        let mut session = Session::new_macro().unwrap();
        let target = session.eval("(list 17)").unwrap();
        let fields = session.data_fields(&target).unwrap();
        let class = session.eval("suss.core/List").unwrap();
        let args = fields.iter().collect::<Vec<_>>();
        let mut registered = Vec::new();
        for _ in 0..128 {
            let (value, inputs) = session.data_construct_with_inputs(&class, &args).unwrap();
            let inputs = inputs.unwrap();
            assert!(inputs.memo_watch().unwrap().eligible_transition);
            registered.push((value, inputs));
        }
        assert_eq!(registry_lock(&session.memo_ledger.entries).watches.len(), 128);
        let number = session.eval("719").unwrap();
        let bridge = session.linker.get(&mut session.store,
            bootstrap::COMPILER_INTEGRITY_IMPORT_MODULE, bootstrap::MEMO_WRITE_IMPORT_NAME)
            .unwrap().into_func().unwrap();
        // Native capability exercise only: ordinary guest installation cannot
        // import this bridge. No registered watch matches this receiver.
        session.with_operation_budget(|session| {
            session.store.set_fuel(2_500)?;
            let mut scope = RootScope::new(&mut session.store);
            let receiver = Val::AnyRef(Some(target.value.to_rooted(&mut scope)));
            let number = Val::AnyRef(Some(number.value.to_rooted(&mut scope)));
            bridge.call(&mut scope, &[receiver, Val::I32(0), number], &mut [])
                .map_err(SessionError::Trap)
        }).unwrap();
        let memo = session.data_fields(&target).unwrap().remove(4);
        assert_eq!(crate::portable_repl::display(&mut session, &memo).unwrap(), "719");
        assert!(session.inspect(&memo, |mut store, value| {
            let expected = number.value.to_rooted(&mut store);
            Rooted::<AnyRef>::ref_eq(&store, value.unwrap_anyref().unwrap(), &expected)
        }).unwrap());
        for (value, inputs) in registered {
            assert!(inputs.matches(&mut session, &value).unwrap());
            assert!(registry_lock(&inputs.memo_watch().unwrap().receipt).is_none());
        }
    }
    #[test]
    fn memo_receipts_initial_non_nil_watch_cannot_certify_after_clearing() {
        let mut session = Session::new_macro().unwrap();
        let original = session.eval("(list 17)").unwrap();
        hash(&mut session, &original);
        let fields = session.data_fields(&original).unwrap();
        let class = session.eval("suss.core/List").unwrap();
        let args = fields.iter().collect::<Vec<_>>();
        let (value, inputs) = session.data_construct_with_inputs(&class, &args).unwrap();
        let inputs = inputs.unwrap();
        let watch = inputs.memo_watch().expect("non-nil memo still registers method obligation");
        assert!(!watch.eligible_transition);
        assert!(inputs.matches(&mut session, &value).unwrap());
        let clear = session.eval("(fn [x] (set! (.-__hash x) nil))").unwrap();
        drop(session.invoke(&clear, &[&value]).unwrap());
        assert!(!inputs.matches(&mut session, &value).unwrap());
        hash(&mut session, &value);
        session.collect().unwrap();
        let fields = session.data_fields(&value).unwrap();
        assert_ne!(crate::portable_repl::display(&mut session, &fields[4]).unwrap(), "nil");
        assert!(registry_lock(&watch.receipt).is_none(), "clearing cannot enable receipt minting");
        assert!(!watch.matches(&mut session).unwrap());
    }
    #[test]
    fn memo_receipts_authenticate_macro_warming_but_not_runtime_stores() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = match phase {
                Phase::Runtime => Session::new_repl().unwrap(),
                Phase::Macro => Session::new_macro().unwrap(),
            };
            let (value, inputs) = registered_list(&mut session);
            assert!(inputs.matches(&mut session, &value).unwrap());
            session.enter_namespace("suss.core").unwrap();
            session.eval("(def hash-ordered-coll (fn [_] 719))").unwrap();
            session.enter_namespace("user").unwrap();
            hash(&mut session, &value);
            session.collect().unwrap();
            let fields = session.data_fields(&value).unwrap();
            assert_eq!(crate::portable_repl::display(&mut session, &fields[4]).unwrap(), "719");
            assert_eq!(inputs.matches(&mut session, &value).unwrap(), phase == Phase::Macro);
        }
    }
    #[test]
    fn memo_receipts_reject_generic_writes_but_preserve_past_valid_hashes() {
        let mut session = Session::new_macro().unwrap();
        let (forged, forged_inputs) = registered_list(&mut session);
        write(&mut session, &forged);
        assert!(!forged_inputs.matches(&mut session, &forged).unwrap());
        let (warmed, inputs) = registered_list(&mut session);
        hash(&mut session, &warmed);
        assert!(inputs.matches(&mut session, &warmed).unwrap());
        session.enter_namespace("suss.core").unwrap();
        session.eval("(extend-type List IHash (-hash [_] 91))").unwrap();
        session.enter_namespace("user").unwrap();
        session.collect().unwrap();
        assert!(inputs.matches(&mut session, &warmed).unwrap());
        let before = session.data_fields(&warmed).unwrap().remove(4);
        let function = session.eval("(fn [x] (hash x))").unwrap();
        let result = session.invoke(&function, &[&warmed]).unwrap();
        assert_eq!(crate::portable_repl::display(&mut session, &result).unwrap(), "91");
        let after = session.data_fields(&warmed).unwrap().remove(4);
        assert!(session.inspect(&after, |mut store, value| {
            let expected = before.value.to_rooted(&mut store);
            Rooted::<AnyRef>::ref_eq(&store, value.unwrap_anyref().unwrap(), &expected)
        }).unwrap(), "replacement dispatch must preserve the authenticated memo reference");
        assert!(inputs.matches(&mut session, &warmed).unwrap());
        session.with_operation_budget(|session| {
            session.store.set_fuel(0).unwrap();
            assert!(matches!(inputs.matches(session, &warmed), Err(SessionError::Trap(ref error))
                if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
            Ok(())
        }).unwrap();
        assert!(inputs.matches(&mut session, &warmed).unwrap());
        session.with_operation_budget(|session| {
            session.interrupt_handle().interrupt();
            assert!(inputs.matches(session, &warmed).unwrap_err().is_interrupt());
            Ok(())
        }).unwrap();
        assert!(inputs.matches(&mut session, &warmed).unwrap());
        write(&mut session, &warmed);
        assert!(!inputs.matches(&mut session, &warmed).unwrap());
        let (cold, cold_inputs) = registered_list(&mut session);
        hash(&mut session, &cold);
        assert!(cold_inputs.memo_watch().unwrap().receipt.lock().unwrap().is_none(),
            "replacement cannot mint canonical receipts");
    }
    #[test]
    fn memo_receipts_reject_numeric_replacement_and_release_dead_watch_roots() {
        let mut session = Session::new_macro().unwrap();
        let ledger = session.memo_ledger.clone();
        let (value, inputs) = registered_list(&mut session);
        hash(&mut session, &value);
        assert!(inputs.matches(&mut session, &value).unwrap());
        let memo = session.data_fields(&value).unwrap().remove(4);
        let mutation = session.inspect(&memo, |mut store, value| {
            let number = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            number.set_field(&mut store, 0, Val::F64(991.0f64.to_bits()))
        });
        assert!(mutation.unwrap_err().to_string().contains("field is not mutable"));
        assert!(inputs.matches(&mut session, &value).unwrap());
        write(&mut session, &value);
        session.collect().unwrap();
        assert!(!inputs.matches(&mut session, &value).unwrap());
        assert_eq!(registry_lock(&ledger.entries).watches.len(), 1);
        drop(inputs);
        session.collect().unwrap();
        assert!(registry_lock(&ledger.entries).watches.is_empty());
        assert_eq!(ledger.handles.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
    #[test]
    fn memo_receipts_validate_with_shared_fuel_interrupt_and_reset_ownership() {
        let mut session = Session::new_macro().unwrap();
        let (value, inputs) = registered_list(&mut session);
        hash(&mut session, &value);
        assert!(inputs.matches(&mut session, &value).unwrap());
        session.with_operation_budget(|session| {
            session.store.set_fuel(0).unwrap();
            assert!(matches!(inputs.matches(session, &value), Err(SessionError::Trap(ref error))
                if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
            Ok(())
        }).unwrap();
        assert!(inputs.matches(&mut session, &value).unwrap());
        session.with_operation_budget(|session| {
            session.interrupt_handle().interrupt();
            assert!(inputs.matches(session, &value).unwrap_err().is_interrupt());
            Ok(())
        }).unwrap();
        assert!(inputs.matches(&mut session, &value).unwrap());
        session.reset().unwrap();
        assert!(inputs.matches(&mut session, &value).is_err());
    }

#[test]
fn memo_receipts_past_warming_rejects_malformed_live_hash_layouts() {
    for damage in ["missing", "nonclosure", "odd", "wrong-type"] {
        let mut session = Session::new_macro().unwrap();
        let (value, inputs) = registered_list(&mut session);
        hash(&mut session, &value);
        let watch = inputs.memo_watch().unwrap();
        assert!(registry_lock(&watch.receipt).is_some());
        assert!(inputs.matches(&mut session, &value).unwrap());
        session.eval("(extend-type List IHash (-hash [_] 91))").unwrap();
        session.collect().unwrap();
        assert!(watch.matches(&mut session).unwrap(), "past receipt / {damage}");
        assert!(inputs.matches(&mut session, &value).unwrap());
        // The replacement is acceptable ONLY for an existing receipt. Minting
        // checks continue to require the actual initial canonical installation.
        let authority = session.canonical_methods.clone().unwrap();
        let irq = session.host_work_interrupt();
        let runtime = session.runtime;
        session.with_operation_budget(|session| {
            let mut scope = RootScope::new(&mut session.store);
            assert!(!watch.canonical(scope.as_context_mut(), runtime, &authority, &irq)?);
            Ok(())
        }).unwrap();

        let handles = session.handles.clone();
        let identity = session.identity;
        // Retain the replacement's real table for restoration across GC.
        let original_table = session.inspect(&watch.descriptor, |store, descriptor| {
            let mut store = RootScope::new(store);
            let object = descriptor.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let table = object.field(&mut store, 2)?;
            own(&mut store, table, identity, &handles)
                .map_err(|e| wasmtime::Error::msg(e.to_string()))
        }).unwrap();
        let key_name = format!("key-{}", "suss.core/IHash/-hash/1".as_bytes()
            .iter().map(|b| format!("{b:02x}")).collect::<String>());
        let key = super::super::canonical_methods::binding(
            &mut session, "suss.internal.protocol-keys", &key_name, &handles).unwrap();
        let nonclosure = session.eval("991").unwrap();
        session.inspect(&watch.descriptor, |mut store, descriptor| {
            let object = descriptor.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let bad = nonclosure.value.to_rooted(&mut store);
            if damage == "wrong-type" {
                object.set_field(&mut store, 2, Val::AnyRef(Some(bad)))?;
                return Ok(());
            }
            let old = object.field(&mut store, 2)?.unwrap_anyref().unwrap()
                .as_array(&store)?.unwrap();
            let ty = old.ty(&store)?;
            let allocator = wasmtime::ArrayRefPre::new(&mut store, ty);
            let count = match damage { "missing" => 0, "odd" => 1, _ => 2 };
            let table = wasmtime::ArrayRef::new(&mut store, &allocator,
                &Val::AnyRef(None), count)?;
            if count > 0 {
                let key = key.value.to_rooted(&mut store);
                table.set(&mut store, 0, Val::AnyRef(Some(key)))?;
            }
            if count > 1 { table.set(&mut store, 1, Val::AnyRef(Some(bad)))?; }
            object.set_field(&mut store, 2, Val::AnyRef(Some(table.to_anyref())))?;
            Ok(())
        }).unwrap();
        session.collect().unwrap();
        // Require false, not just is_err(): malformed lookup language errors
        // must be converted by Watch::method_valid into failed inspection.
        assert!(!watch.matches(&mut session).unwrap(), "{damage}");
        assert!(!inputs.matches(&mut session, &value).unwrap(), "{damage}");
        assert!(registry_lock(&watch.receipt).is_some(), "no receipt replacement");
        session.inspect(&watch.descriptor, |mut store, descriptor| {
            let object = descriptor.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let table = original_table.value.to_rooted(&mut store);
            object.set_field(&mut store, 2, Val::AnyRef(Some(table)))
        }).unwrap();
        session.collect().unwrap();
        assert!(watch.matches(&mut session).unwrap(), "restored / {damage}");
        assert!(inputs.matches(&mut session, &value).unwrap());
    }
}


    #[test]
    fn memo_receipts_store_before_fuel_failure_does_not_publish_a_receipt() {
        let mut session = Session::new_macro().unwrap();
        let (value, inputs) = registered_list(&mut session);
        let watch = registry_lock(&session.memo_ledger.entries).watches[0].upgrade().unwrap();
        assert_eq!(bootstrap::memo_owner_slots().nth(watch.site), Some(("List", 4)));
        let mut watches = vec![watch.clone()];
        let fields = session.data_fields(&value).unwrap().into_iter().map(Some).collect::<Vec<_>>();
        for _ in 0..2 {
            watches.push(session.watch_constructor_memo(&watch.constructor, &watch.descriptor,
                &value, &watch.storage, &fields).unwrap().unwrap());
        }
        let number = session.eval("719").unwrap();
        let bridge = session.linker.get(&mut session.store,
            bootstrap::COMPILER_INTEGRITY_IMPORT_MODULE, bootstrap::MEMO_WRITE_IMPORT_NAME)
            .unwrap().into_func().unwrap();
        let default = session.options.fuel_per_operation;
        let mut witnessed = false;
        // The test directly exercises the bound host capability. Ordinary guest
        // installation cannot import it; this is a supplemental publication test.
        for fuel in (1..=32_768).step_by(64) {
            session.set_operation_fuel(fuel);
            let result = session.with_operation_budget(|session| {
                let mut scope = RootScope::new(&mut session.store);
                let receiver = Val::AnyRef(Some(value.value.to_rooted(&mut scope)));
                let number = Val::AnyRef(Some(number.value.to_rooted(&mut scope)));
                bridge.call(&mut scope, &[receiver, Val::I32(watch.site as i32), number], &mut [])
                    .map_err(SessionError::Trap)
            });
            session.set_operation_fuel(default);
            let fields = session.data_fields(&value).unwrap();
            let stored = session.inspect(&fields[4], |mut store, value| {
                let expected = number.value.to_rooted(&mut store);
                Rooted::<AnyRef>::ref_eq(&store, value.unwrap_anyref().unwrap(), &expected)
            }).unwrap();
            if result.is_err() && stored {
                let error = result.unwrap_err();
                assert!(matches!(error, SessionError::Trap(ref error)
                    if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
                assert!(!inputs.matches(&mut session, &value).unwrap());
                assert!(watches.iter().all(|watch| registry_lock(&watch.receipt).is_none()),
                    "post-store exhaustion cannot publish any of the matching receipts");
                witnessed = true;
                break;
            }
            assert!(result.is_err(), "receipt publication completed before a post-store trap was witnessed");
        }
        assert!(witnessed, "must witness changed storage after a real fuel failure");
        let clear = session.eval("(fn [x] (set! (.-__hash x) nil))").unwrap();
        drop(session.invoke(&clear, &[&value]).unwrap());
        hash(&mut session, &value);
        assert!(inputs.matches(&mut session, &value).unwrap());
        for watch in watches { assert!(watch.matches(&mut session).unwrap()); }
    }
}

impl Watch {
 pub(super) fn bind_source_domain(&self,domain:&Arc<super::write_domain::WriteDomain>) {
  let mut previous=registry_lock(&self.source_domain);
  if previous.is_some() {domain.invalidate();} else {*previous=Some(Arc::downgrade(domain));}
 }
 pub(super) fn is_slot(&self,mut store:StoreContextMut<'_,()>,array:&Val,index:u32,irq:&AtomicBool)->wasmtime::Result<bool> {
  Session::consume_host_work(store.as_context_mut(),2,irq)?;
  let expected=Val::AnyRef(Some(self.storage.value.to_rooted(&mut store)));
  Ok(index as usize==self.slot && same(&store,array,&expected)?)
 }
 pub(super) fn matches_receipt(&self,mut store:StoreContextMut<'_,()>,value:&Val,receipt:&Receipt,irq:&AtomicBool)->wasmtime::Result<bool> {
  Session::consume_host_work(store.as_context_mut(),4,irq)?;
  let expected=Val::AnyRef(Some(receipt.value.value.to_rooted(&mut store)));
  Ok(same(&store,value,&expected)? && bits(store.as_context_mut(),value,&self.number_type)?==Some(receipt.bits))
 }
 pub(super) fn source_authenticated(&self,mut store:StoreContextMut<'_,()>,value:&Val,irq:&AtomicBool)->wasmtime::Result<bool> {
  Session::consume_host_work(store.as_context_mut(),1,irq)?;
  let domain=registry_lock(&self.source_domain).as_ref().and_then(Weak::upgrade);
  if domain.is_none() {return Ok(false);}
  let receipt=registry_lock(&self.receipt);
  match receipt.as_ref() {Some(receipt) if receipt.source_safe=>self.matches_receipt(store,value,receipt,irq),_=>Ok(false)}
 }
 fn domain_matches(&self,store:StoreContextMut<'_,()>,runtime:Instance,authority:&CanonicalMethods,prototype:&super::prototype_origin::Authority,irq:&AtomicBool,staged:Option<&Receipt>)->wasmtime::Result<bool> {
  let domain=registry_lock(&self.source_domain).as_ref().and_then(Weak::upgrade);
  let Some(domain)=domain else{return Ok(false)};
  match domain.matches(store,runtime,self.identity,irq,authority,prototype,staged.map(|receipt|(self,receipt)),None) {
   Ok(valid)=>Ok(valid),Err(SessionError::Language(_))=>Ok(false),Err(SessionError::Host(error)|SessionError::Trap(error))=>Err(error),Err(error)=>Err(wasmtime::Error::msg(error.to_string()))
  }
 }
}

// Actual physical nominal stores in a recognized IHash dynamic extent. No
// candidate is staged without exact receiver/storage/slot and recorded facts.
pub(super) fn install_scoped(
    store: &mut Store<()>, runtime: Instance, interrupt: Arc<AtomicBool>,
    ledger: Arc<Ledger>, observer: Arc<super::dispatch_observer::Observer>,
) -> wasmtime::Result<()> {
    let raw_store = runtime.get_func(&mut *store, "macro-memo-array-store").unwrap();
    let installer = runtime.get_func(&mut *store, "macro-memo-store-install").unwrap();
    let parameter = installer.ty(&*store).params().next().unwrap();
    let wasmtime::HeapType::ConcreteFunc(ty) = parameter.unwrap_ref().heap_type() else {
        return Err(wasmtime::Error::msg("Invalid physical store callback type"));
    };
    let hook = Func::new(&mut *store, ty.clone(), move |mut caller, arguments, _| {
        Session::consume_host_work(caller.as_context_mut(), 4, &interrupt)?;
        let mut scope = RootScope::new(&mut caller);
        let mut candidates = Vec::new();
        if observer.has_scope() {
            let (authority, prototype, count) = {
                let entries = registry_lock(&ledger.entries);
                (entries.authority.clone(), entries.prototype.clone(), entries.watches.len())
            };
            if let (Some(authority), Some(prototype)) = (authority, prototype) {
                for index in 0..count {
                    Session::consume_host_work(scope.as_context_mut(), 1, &interrupt)?;
                    let watch = { registry_lock(&ledger.entries).watches[index].upgrade() };
                    let Some(watch) = watch else { continue };
                    if !watch.eligible_transition || watch.identity != ledger.identity { continue }
                    if !watch.is_slot(scope.as_context_mut(), &arguments[0], arguments[1].unwrap_i32() as u32, &interrupt)? { continue }
                    // Do not materialize a receiver root before a physical slot matches.
                    let Some(receiver) = observer.receiver(scope.as_context_mut()) else { continue };
                    let Some(current) = watch.current(scope.as_context_mut(), &receiver, &interrupt)? else { continue };
                    if !(nil(&scope.as_context_mut(), &current)? || watch.authenticated(scope.as_context_mut(), &current, &interrupt)?) { continue }
                    let Some(bits) = bits(scope.as_context_mut(), &arguments[2], &watch.number_type)? else { continue };
                    // A mere watch or numeric store is not source authority.
                    let domain = registry_lock(&watch.source_domain).as_ref().and_then(Weak::upgrade);
                    let Some(domain) = domain else { continue };
                    let descriptor = Val::AnyRef(Some(watch.descriptor.value.to_rooted(&mut scope)));
                    if !watch.domain_method(scope.as_context_mut(), runtime, &authority, &interrupt, Some(&descriptor))? { continue }
                    if !scoped_domain(&domain, scope.as_context_mut(), runtime, &ledger, &authority, &prototype, &interrupt, None, &descriptor)? { continue }
                    reserve_candidate(scope.as_context_mut(), &mut candidates, &interrupt)?;
                    Session::consume_host_work(scope.as_context_mut(), std::mem::size_of::<Receipt>() as u64 + 1, &interrupt)?;
                    let value = own(&mut scope, arguments[2].clone(), ledger.identity, &ledger.handles)
                        .map_err(scoped_error)?;
                    candidates.push((watch, Receipt { value, bits, source_safe: true }, false));
                }
                // Exact original instruction: no recursion, no exception translation.
                raw_store.call(&mut scope, arguments, &mut [])?;
                for (watch, receipt, valid) in &mut candidates {
                    Session::consume_host_work(scope.as_context_mut(), 8, &interrupt)?;
                    let Some(receiver) = observer.receiver(scope.as_context_mut()) else { continue };
                    let Some(current) = watch.current(scope.as_context_mut(), &receiver, &interrupt)? else { continue };
                    if !watch.matches_receipt(scope.as_context_mut(), &current, receipt, &interrupt)? { continue }
                    let domain = registry_lock(&watch.source_domain).as_ref().and_then(Weak::upgrade);
                    let Some(domain) = domain else { continue };
                    let descriptor = Val::AnyRef(Some(watch.descriptor.value.to_rooted(&mut scope)));
                    *valid = watch.domain_method(scope.as_context_mut(), runtime, &authority, &interrupt, Some(&descriptor))?
                        && scoped_domain(&domain, scope.as_context_mut(), runtime, &ledger, &authority, &prototype, &interrupt, Some((watch, receipt)), &descriptor)?;
                }
                // No fallible work after the first publication. No outside write
                // clears an old receipt: exact reference/domain checks decide reuse.
                for (watch, receipt, valid) in candidates {
                    if valid { *registry_lock(&watch.receipt) = Some(receipt); }
                }
                return Ok(());
            }
        }
        raw_store.call(&mut scope, arguments, &mut [])
    });
    installer.call(store, &[Val::FuncRef(Some(hook))], &mut [])
}
fn scoped_error(error: SessionError) -> wasmtime::Error {
    match error {
        SessionError::Host(error) | SessionError::Trap(error) => error,
        other => wasmtime::Error::msg(other.to_string()),
    }
}
fn scoped_domain(
    domain: &super::write_domain::WriteDomain, store: StoreContextMut<'_, ()>,
    runtime: Instance, ledger: &Ledger, authority: &CanonicalMethods,
    prototype: &super::prototype_origin::Authority, irq: &AtomicBool,
    staged: Option<(&Watch, &Receipt)>, descriptor: &Val,
) -> wasmtime::Result<bool> {
    match domain.matches(store, runtime, ledger.identity, irq, authority, prototype, staged, Some(descriptor)) {
        Ok(valid) => Ok(valid),
        Err(SessionError::Language(_)) => Ok(false),
        Err(error) => Err(scoped_error(error)),
    }
}

#[cfg(test)]
mod scoped_tests;
