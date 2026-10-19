//! Optional private-runtime origin witnesses, not memo/UID or retained-root grants.
use super::{Session, SessionError, SessionValue, canonical_methods, own, storage_snapshot::StorageSnapshot};
use std::sync::{Arc, atomic::AtomicUsize};
use wasmtime::{AnyRef, AsContextMut, RootScope, Rooted, StoreContextMut, Val};
type Result<T> = std::result::Result<T, SessionError>;
pub(super) struct Authority {
    parent: SessionValue,
    parent_storage: StorageSnapshot,
    handles: Arc<AtomicUsize>,
}
#[derive(Debug)]
pub(super) struct PrototypeInputs { fields: Vec<SessionValue> }
fn invalid(message: &'static str) -> SessionError { SessionError::Host(wasmtime::Error::msg(message)) }
fn runtime(session: &mut Session, name: &str, args: &[&SessionValue], handles: &Arc<AtomicUsize>) -> Result<SessionValue> {
    canonical_methods::runtime(session, name, args, handles)
}
fn nil(store: &StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<bool> {
    Ok(matches!(value, Val::AnyRef(Some(value)) if value.as_i31(store)?.is_some_and(|v| v.get_u32() == 0)))
}
fn same(store: &StoreContextMut<'_, ()>, a: &Val, b: &Val) -> wasmtime::Result<bool> {
    match (a,b) {
        (Val::AnyRef(Some(a)), Val::AnyRef(Some(b))) => Rooted::<AnyRef>::ref_eq(store, a, b),
        _ => Ok(false),
    }
}
fn array_fields(store: &mut StoreContextMut<'_, ()>, value: &Val, count: usize, interrupt: &std::sync::atomic::AtomicBool) -> wasmtime::Result<Option<Vec<Val>>> {
    Session::consume_host_work(store.as_context_mut(), count as u64 + 1, interrupt)?;
    let Some(value) = value.anyref().flatten() else { return Ok(None); };
    let Some(array) = value.as_array(&*store)? else { return Ok(None); };
    if array.len(&*store)? as usize != count { return Ok(None); }
    Session::consume_host_work(store.as_context_mut(), (count * std::mem::size_of::<Val>()) as u64, interrupt)?;
    let mut result = Vec::new();
    result.try_reserve_exact(count).map_err(|_| wasmtime::Error::msg("Prototype input allocation failed"))?;
    for i in 0..count {
        Session::consume_host_work(store.as_context_mut(), 1, interrupt)?;
        result.push(array.get(&mut *store, i as u32)?);
    }
    Ok(Some(result))
}
impl Session {
    pub(super) fn begin_prototype_origins(&mut self) -> Result<()> {
        if self.bootstrap_core || self.prototype_origin.is_some() { return Err(invalid("Prototype provisioning already finished")); }
        let _request = self.operation_budget_scope()?;
        let handles = Arc::new(AtomicUsize::new(0));
        drop(runtime(self, "prototype-origin-begin", &[], &handles)?);
        Ok(())
    }
    pub(super) fn seal_prototype_origins(&mut self, methods: &canonical_methods::CanonicalMethods) -> Result<Authority> {
        let _request = self.operation_budget_scope()?;
        let handles = Arc::new(AtomicUsize::new(0));
        let count = crate::portable_macro_data::source_class_names().len();
        if count != 23 { return Err(invalid("Prototype origin constructor catalog mismatch")); }
        self.charge_host_work(count as u64 + 1)?;
        for constructor in methods.source_constructor_roots() {
            let descriptor = runtime(self, "constructor-descriptor", &[constructor], &handles)?;
            if !methods.source_constructor_matches(self, constructor, &descriptor)? {
                return Err(invalid("Changed constructor before prototype sealing"));
            }
        }
        let roots = {
            let mut scope = RootScope::new(&mut self.store);
            let function = self.runtime.get_func(&mut scope, "args-new").unwrap();
            let mut result = [Val::null_any_ref()];
            function.call(&mut scope, &[Val::I32(count as i32)], &mut result)?;
            let array = result[0].unwrap_anyref().unwrap().as_array(&scope)?.unwrap();
            let mut observed = 0;
            for (index, constructor) in methods.source_constructor_roots().enumerate() {
                Session::consume_host_work(scope.as_context_mut(), 1, &self.interrupt)?;
                let value = constructor.value.to_rooted(&mut scope);
                array.set(&mut scope, index as u32, Val::AnyRef(Some(value)))?;
                observed += 1;
            }
            if observed != count { return Err(invalid("Incomplete prototype constructor catalog")); }
            own(&mut scope, result[0].clone(), self.identity, &handles)?
        };
        drop(runtime(self, "prototype-origin-seal", &[&roots], &handles)?);
        let parent = runtime(self, "prototype-origin-default-parent", &[], &handles)?;
        let parent_storage = self.capture_storage_accounted(&parent, handles.clone())?;
        Ok(Authority { parent, parent_storage, handles })
    }
    pub(super) fn prototype_origin_inputs(&mut self, constructor: &SessionValue, descriptor: &SessionValue, prototype: &SessionValue) -> Result<Option<PrototypeInputs>> {
        let _request = self.operation_budget_scope()?;
        let Some(authority) = self.prototype_origin.clone() else { return Ok(None); };
        let Some(methods) = self.canonical_methods.clone() else { return Ok(None); };
        if !methods.source_constructor_matches(self, constructor, descriptor)? { return Ok(None); }
        let witness = runtime(self, "prototype-origin-query", &[constructor, descriptor, prototype], &authority.handles)?;
        let interrupt = self.host_work_interrupt();
        let mut scope = RootScope::new(&mut self.store);
        let witness = Val::AnyRef(Some(witness.value.to_rooted(&mut scope)));
        let Some(values) = array_fields(&mut scope.as_context_mut(), &witness, 12, &interrupt)? else { return Ok(None); };
        Session::consume_host_work(scope.as_context_mut(), (12 * std::mem::size_of::<SessionValue>()) as u64, &interrupt)?;
        let mut fields = Vec::new();
        fields.try_reserve_exact(12).map_err(|_| invalid("Prototype witness allocation failed"))?;
        for value in values {
            Session::consume_host_work(scope.as_context_mut(), 1, &interrupt)?;
            fields.push(own(&mut scope, value, self.identity, &authority.handles)?);
        }
        Ok(Some(PrototypeInputs { fields }))
    }
}
impl PrototypeInputs {
    pub(super) fn expectation_fields(&self) -> &[SessionValue] { &self.fields }
    pub(super) fn matches(&self, session: &mut Session) -> Result<bool> {
        let _request = session.operation_budget_scope()?;
        let Some(authority) = session.prototype_origin.clone() else { return Ok(false); };
        // Fresh query checks the existing cache and latest bounded private slot.
        let Some(current) = session.prototype_origin_inputs(&self.fields[0], &self.fields[1], &self.fields[2])? else { return Ok(false); };
        let interrupt = session.host_work_interrupt();
        session.charge_host_work((24 * std::mem::size_of::<Val>()) as u64)?;
        let mut scope = RootScope::new(&mut session.store);
        let mut f = Vec::with_capacity(12);
        for (old, new) in self.fields.iter().zip(&current.fields) {
            Session::consume_host_work(scope.as_context_mut(), 1, &interrupt)?;
            let old = Val::AnyRef(Some(old.value.to_rooted(&mut scope)));
            let new = Val::AnyRef(Some(new.value.to_rooted(&mut scope)));
            if !same(&scope.as_context_mut(), &old, &new)? { return Ok(false); }
            f.push(old);
        }
        let prototype = f[2].unwrap_anyref().unwrap().as_struct(&scope)?.ok_or_else(|| invalid("Prototype witness needs object"))?;
        Session::consume_host_work(scope.as_context_mut(), 4, &interrupt)?;
        let tag = prototype.field(&mut scope, 0)?;
        let storage = prototype.field(&mut scope, 1)?;
        let properties = prototype.field(&mut scope, 2)?;
        let uid = prototype.field(&mut scope, 3)?;
        if !same(&scope.as_context_mut(), &tag, &f[3])?
            || !same(&scope.as_context_mut(), &storage, &f[4])?
            || !nil(&scope.as_context_mut(), &properties)?
            || !nil(&scope.as_context_mut(), &uid)? { return Ok(false); }
        let Some(fields) = array_fields(&mut scope.as_context_mut(), &f[4], 2, &interrupt)? else { return Ok(false); };
        let Some(pairs) = array_fields(&mut scope.as_context_mut(), &f[5], 2, &interrupt)? else { return Ok(false); };
        let Some(property) = array_fields(&mut scope.as_context_mut(), &f[8], 2, &interrupt)? else { return Ok(false); };
        if !same(&scope.as_context_mut(), &fields[0], &f[5])? || !same(&scope.as_context_mut(), &fields[1], &f[6])?
            || !same(&scope.as_context_mut(), &pairs[0], &f[7])? || !same(&scope.as_context_mut(), &pairs[1], &f[8])?
            || !same(&scope.as_context_mut(), &property[1], &f[0])?
            || !same(&scope.as_context_mut(), &property[1], &f[10])? { return Ok(false); }
        Session::consume_host_work(scope.as_context_mut(), 3, &interrupt)?;
        if !matches!(&property[0], Val::AnyRef(Some(v)) if v.as_i31(&scope)?.is_some_and(|v|v.get_u32()==5))
            || !nil(&scope.as_context_mut(), &f[11])? { return Ok(false); }
        // The original STRING reference is not a spelling authority: its UTF16
        // elements are mutable. Compare against independent factory literal units.
        let Some(key) = f[7].unwrap_anyref().unwrap().as_array(&scope)? else { return Ok(false); };
        if key.len(&scope)? != 11 { return Ok(false); }
        for (index, unit) in "constructor".encode_utf16().enumerate() {
            Session::consume_host_work(scope.as_context_mut(), 1, &interrupt)?;
            if !matches!(key.get(&mut scope, index as u32)?, Val::I32(found) if found == i32::from(unit)) { return Ok(false); }
        }
        let initial_parent = Val::AnyRef(Some(authority.parent.value.to_rooted(&mut scope)));
        if !same(&scope.as_context_mut(), &f[6], &initial_parent)? { return Ok(false); }
        drop(scope);
        // This complete snapshot was made ONLY inside initial provisioning. It
        // conservatively rejects default Object-prototype mutations, not source
        // descriptor protocol changes behind the constructor backlink cut above.
        session.storage_matches(&authority.parent, &authority.parent_storage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::resolve::Phase;
    fn session(phase: Phase) -> Session {
        match phase { Phase::Runtime => Session::new_repl().unwrap(), Phase::Macro => Session::new_macro().unwrap() }
    }
    fn call_native(session: &mut Session, name: &str, constructor: &SessionValue, write: Option<&SessionValue>) -> Result<SessionValue> {
        let _request = session.operation_budget_scope()?;
        session.check(constructor)?;
        if let Some(value) = write { session.check(value)?; }
        session.charge_host_work(4)?;
        let mut scope = RootScope::new(&mut session.store);
        let function = session.runtime.get_func(&mut scope, name).unwrap();
        let mut args = vec![Val::AnyRef(Some(constructor.value.to_rooted(&mut scope))), Val::I32(8)];
        if let Some(value) = write { args.push(Val::AnyRef(Some(value.value.to_rooted(&mut scope)))); }
        super::super::call(&mut scope, session.runtime, function, &args, session.identity, &session.handles)
    }
    fn force_cache_absent(session: &mut Session, constructor: &SessionValue) {
        let nil = session.eval("nil").unwrap();
        drop(call_native(session, "closure-internal-property-set", constructor, Some(&nil)).unwrap());
    }
    fn getter(session: &mut Session, constructor: &SessionValue) -> Result<SessionValue> {
        session.with_operation_budget(|session| {
            let handles = session.prototype_origin.as_ref().map(|a|a.handles.clone()).unwrap_or_else(||Arc::new(AtomicUsize::new(0)));
            runtime(session, "owned-constructor-prototype", &[constructor], &handles)
        })
    }
    #[test]
    fn prototype_origin_is_default_off_and_custom_direct_facts_remain_usable() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut local = Session::with_options_in(Default::default(), phase).unwrap();
            assert!(local.prototype_origin.is_none());
            let value = local.eval("17").unwrap();
            let handles = Arc::new(AtomicUsize::new(0));
            let query = local.with_operation_budget(|s| runtime(s, "prototype-origin-query", &[&value,&value,&value], &handles)).unwrap();
            assert!(local.inspect(&query, |store,v|nil(&store,&v)).unwrap());
            let mut session = session(phase);
            session.eval("(deftype UnregisteredPrototype [a])").unwrap();
            let class = session.eval("UnregisteredPrototype").unwrap();
            let value = session.eval("17").unwrap();
            let (object, inputs) = session.data_construct_with_inputs(&class, &[&value]).unwrap();
            let inputs = inputs.unwrap();
            assert!(inputs.matches(&mut session,&object).unwrap());
            assert!(!inputs.matches_prototype_origin(&mut session).unwrap());
        }
    }
    #[test]
    fn prototype_origin_lazy_creation_and_copied_query_survive_gc_without_mint_export() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let registry = session.canonical_methods.clone().unwrap();
            let mut lazy = None;
            for constructor in registry.source_constructor_roots() {
                let carrier = call_native(&mut session,"closure-internal-property-get",constructor,None).unwrap();
                if session.inspect(&carrier, |store,v|nil(&store,&v)).unwrap() { lazy = Some(constructor.clone()); break; }
            }
            let constructor = lazy.expect("initial bootstrap leaves a source prototype lazy");
            let descriptor = session.data_descriptor(&constructor,true).unwrap();
            let prototype = getter(&mut session,&constructor).unwrap();
            let inputs = session.prototype_origin_inputs(&constructor,&descriptor,&prototype).unwrap().unwrap();
            session.collect().unwrap();
            assert!(inputs.matches(&mut session).unwrap());
            let handles = session.prototype_origin.as_ref().unwrap().handles.clone();
            let copy = session.with_operation_budget(|s|runtime(s,"prototype-origin-query",&[&constructor,&descriptor,&prototype],&handles)).unwrap();
            session.inspect(&copy, |mut store,v| {
                let a=v.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                assert_eq!(a.len(&store)?,12);
                a.set(&mut store,0,Val::null_any_ref())
            }).unwrap();
            session.collect().unwrap();
            assert!(inputs.matches(&mut session).unwrap(),"query copy cannot mutate private record");
            let module=wasmtime::Module::new(&session.engine,crate::runtime_abi::module()).unwrap();
            assert_eq!(module.imports().len(),0);
            let names=module.exports().filter(|e|e.name().starts_with("prototype-origin-")).map(|e|e.name().to_owned()).collect::<std::collections::BTreeSet<_>>();
            assert_eq!(names,["prototype-origin-begin","prototype-origin-seal","prototype-origin-query","prototype-origin-default-parent"].into_iter().map(str::to_owned).collect());
            assert!(session.begin_prototype_origins().is_err());
            assert!(session.seal_prototype_origins(&registry).is_err());
        }
    }
    #[test]
    fn prototype_origin_binds_creator_descriptor_before_restore_and_generic_writes_do_not_mint() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session=session(phase);
            let constructor=session.eval("suss.core/ArrayChunk").unwrap();
            let descriptor=session.data_descriptor(&constructor,true).unwrap();
            let other=session.eval("suss.core/Keyword").unwrap();
            let fake=session.data_descriptor(&other,true).unwrap();
            force_cache_absent(&mut session,&constructor);
            session.inspect(&constructor,|mut store,v| {
                let c=v.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let owner=c.field(&mut store,0)?.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let fields=owner.field(&mut store,1)?.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                let fake=fake.value.to_rooted(&mut store);
                fields.set(&mut store,0,Val::AnyRef(Some(fake)))
            }).unwrap();
            let prototype=getter(&mut session,&constructor).unwrap();
            session.inspect(&constructor,|mut store,v| {
                let c=v.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let owner=c.field(&mut store,0)?.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let fields=owner.field(&mut store,1)?.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                let original=descriptor.value.to_rooted(&mut store);
                fields.set(&mut store,0,Val::AnyRef(Some(original)))
            }).unwrap();
            session.collect().unwrap();
            assert!(session.prototype_origin_inputs(&constructor,&descriptor,&prototype).unwrap().is_none(),"restoring creator descriptor does not bless fake creation");
            force_cache_absent(&mut session,&constructor);
            let prototype=getter(&mut session,&constructor).unwrap();
            let inputs=session.prototype_origin_inputs(&constructor,&descriptor,&prototype).unwrap().unwrap();
            assert!(inputs.matches(&mut session).unwrap());
            // Own-property injection preserves the identity but changes storage.
            let key=session.eval(r#""origin-poison""#).unwrap();
            let value=session.eval("99").unwrap();
            let mutate=session.eval("(fn [p] (aset p \"origin-poison\" 99))").unwrap();
            drop(session.invoke(&mutate,&[&prototype]).unwrap());
            assert!(!inputs.matches(&mut session).unwrap());
            drop((key,value));
        }
    }
    #[test]
    fn prototype_origin_fuel_failures_recover_without_eager_getter_or_reconfiguration() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session=session(phase);
            let constructor=session.eval("suss.core/ArrayChunk").unwrap();
            let descriptor=session.data_descriptor(&constructor,true).unwrap();
            force_cache_absent(&mut session,&constructor);
            let default=session.options.fuel_per_operation;
            let mut failures=0;
            let mut result=None;
            for fuel in [1,16,64,256,1024,4096,16384,65536] {
                session.set_operation_fuel(fuel);
                match getter(&mut session,&constructor) {
                    Ok(value)=>{result=Some(value);break;},
                    Err(error)=>{
                        assert!(matches!(error,SessionError::Trap(ref e) if e.downcast_ref::<wasmtime::Trap>()==Some(&wasmtime::Trap::OutOfFuel)),"{error}");
                        failures+=1;
                    }
                }
            }
            session.set_operation_fuel(default);
            let prototype=result.expect("finite factory succeeds below default allowance");
            assert!(failures>0);
            let inputs=session.prototype_origin_inputs(&constructor,&descriptor,&prototype).unwrap().unwrap();
            session.collect().unwrap();
            assert!(inputs.matches(&mut session).unwrap());
            session.with_operation_budget(|s| {
                s.interrupt_handle().interrupt();
                assert!(inputs.matches(s).unwrap_err().is_interrupt());
                Ok(())
            }).unwrap();
            assert!(inputs.matches(&mut session).unwrap());
            session.reset().unwrap();
            assert!(inputs.matches(&mut session).is_err());
        }
    }
}

impl Authority {
    pub(super) fn parent_matches_in_context(&self,mut context: canonical_methods::InspectionContext<'_>) -> Result<bool> {
        let mut scope=RootScope::new(context.store.as_context_mut());
        let root=Val::AnyRef(Some(self.parent.value.to_rooted(&mut scope)));
        self.parent_storage.matches_in_context(scope.as_context_mut(),root,context.identity,context.interrupt)
    }
}
impl PrototypeInputs {
    pub(super) fn write_cuts(&self)->[&SessionValue;3] { [&self.fields[0],&self.fields[3],&self.fields[6]] }
}

#[cfg(test)]
mod certification_audit_tests {
    use super::*;
    use crate::portable_session::certification_audit::CertificationAudit;

    fn real_list_origin(session: &mut Session) -> PrototypeInputs {
        drop(session.eval("(list 17)").unwrap());
        let constructor = session.eval("suss.core/List").unwrap();
        let descriptor = session.data_descriptor(&constructor, true).unwrap();
        let handles = Arc::new(AtomicUsize::new(0));
        let prototype = runtime(session, "owned-descriptor-prototype", &[&descriptor], &handles).unwrap();
        session.prototype_origin_inputs(&constructor, &descriptor, &prototype).unwrap().unwrap()
    }

    #[test]
    fn audit_prototype_uses_all_twelve_fields_and_survives_gc() {
        let mut session = Session::new_macro().unwrap();
        let origin = real_list_origin(&mut session);
        assert_eq!(origin.fields.len(), 12);
        assert!(origin.matches(&mut session).unwrap());
        let mut audit = CertificationAudit::new(&mut session).unwrap();
        assert!(audit.prototype(&mut session, &origin).unwrap());
        session.collect().unwrap();
        assert!(audit.prototype(&mut session, &origin).unwrap());
        // New rooted identity cannot equal any field of the existing witness.
        // data_array is a native allocation, not a constructor/prototype mutation.
        let different = session.data_array(&[]).unwrap();
        for index in 0..12 {
            let mut fields = origin.fields.clone();
            fields[index] = different.clone();
            let changed = PrototypeInputs { fields };
            let direct = changed.matches(&mut session);
            let cached = audit.prototype(&mut session, &changed);
            match (direct, cached) {
                (Ok(expected), Ok(actual)) => {
                    assert!(!expected, "changed witness field {index} must reject");
                    assert_eq!(actual, expected, "field {index} must not hit original true entry");
                }
                (Err(_), Err(_)) => {}, // Genuine malformed-query errors must propagate.
                _ => panic!("audited and direct prototype checks disagree at field {index}"),
            }
        }
        assert!(audit.prototype(&mut session, &origin).unwrap());
        drop(audit);

        // Complete validation must run again in a new interval. Mutate actual
        // factory prototype constructor-key UTF16 storage, not the expectation.
        session.inspect(&origin.fields[7], |mut store, value| {
            value.unwrap_anyref().unwrap().as_array(&store)?.unwrap()
                .set(&mut store, 0, Val::I32(i32::from(b'X')))
        }).unwrap();
        let mut next = CertificationAudit::new(&mut session).unwrap();
        assert!(!next.prototype(&mut session, &origin).unwrap());
    }
}
