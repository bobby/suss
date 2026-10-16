//! Invocation-local query reuse within a callback-free inspection interval.
//! Such intervals may occur between constructions or during final certification.
//! Discard the audit before every construction or fallback materialization; no
//! cached query may survive a potential guest callback, including on failure.
use super::{Session, SessionError, SessionValue, canonical_methods::CanonicalMethods,
    construction_records::BridgeClassInput, prototype_origin::{Authority, PrototypeInputs}};
use std::{cmp::Ordering, sync::Arc};
use crate::portable::metered_map::{Context, TryOrderedMap};
use wasmtime::{AnyRef, RootScope, Rooted};
type Result<T> = std::result::Result<T, SessionError>;
struct Entry {
    name: Option<&'static str>,
    roots: Vec<SessionValue>,
    valid: bool,
    next: Option<usize>,
}
pub(crate) struct CertificationAudit {
    identity: u64,
    methods: Option<Arc<CanonicalMethods>>,
    origins: Option<Arc<Authority>>,
    entries: Vec<Entry>,
    heads: TryOrderedMap<u64, usize>,
}
fn invalid() -> SessionError {
    SessionError::Host(wasmtime::Error::msg("Invalid certification audit scope"))
}
fn same_registry<T>(a: &Option<Arc<T>>, b: &Option<Arc<T>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}
fn reserve<T>(session: &mut Session, values: &mut Vec<T>, extra: usize) -> Result<()> {
    session.charge_host_work(1)?;
    let needed = values.len().checked_add(extra).ok_or_else(invalid)?;
    if needed > values.capacity() {
        let capacity = values.capacity().checked_mul(2).unwrap_or(needed).max(needed);
        let bytes = capacity.checked_add(values.len())
            .and_then(|n| n.checked_mul(std::mem::size_of::<T>())).ok_or_else(invalid)?;
        session.charge_host_work(bytes as u64)?;
        values.try_reserve_exact(capacity - values.len()).map_err(|_| invalid())?;
    }
    Ok(())
}
struct Meter<'a>(&'a mut Session);
impl Context<u64> for Meter<'_> {
    type Error = SessionError;
    fn charge(&mut self, work: u64) -> Result<()> { self.0.charge_host_work(work) }
    fn compare(&mut self, a: &u64, b: &u64) -> Result<Ordering> {
        self.0.charge_host_work(1)?;
        Ok(a.cmp(b))
    }
}
fn name_bucket(session: &mut Session, name: &str) -> Result<u64> {
    session.charge_host_work(1)?;
    let mut hash = 0xcbf29ce484222325u64;
    for chunk in name.as_bytes().chunks(256) {
        session.charge_host_work((chunk.len() as u64).saturating_mul(3))?;
        for byte in chunk { hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3); }
    }
    Ok(hash)
}
impl CertificationAudit {
    pub(crate) fn new(session: &mut Session) -> Result<Self> {
        session.charge_host_work(3)?;
        Ok(Self { identity: session.identity, methods: session.canonical_methods.clone(),
            origins: session.prototype_origin.clone(), entries: Vec::new(), heads: TryOrderedMap::default() })
    }
    fn query<'a, I: Iterator<Item = &'a SessionValue> + Clone>(
        &mut self, session: &mut Session, name: Option<&'static str>, roots: I,
        bucket: u64, validate: impl FnOnce(&mut Session) -> Result<bool>,
    ) -> Result<bool> {
        let _request = session.operation_budget_scope()?;
        session.charge_host_work(3)?;
        if self.identity != session.identity { return Err(SessionError::ForeignValue); }
        if !same_registry(&self.methods, &session.canonical_methods)
            || !same_registry(&self.origins, &session.prototype_origin) { return Err(invalid()); }
        let mut count = 0usize;
        for root in roots.clone() {
            session.charge_host_work(1)?;
            session.check(root)?;
            count = count.checked_add(1).ok_or_else(invalid)?;
        }
        let head = self.heads.get(&bucket, &mut Meter(session))?.copied();
        let mut cursor = head;
        let mut tail = None;
        while let Some(index) = cursor {
            session.charge_host_work(2)?;
            let entry = &self.entries[index];
            tail = Some(index);
            cursor = entry.next;
            let same_name = match (name, entry.name) {
                (Some(a), Some(b)) => {
                    session.charge_host_work((a.len() + b.len()) as u64 + 1)?;
                    a == b
                }
                (None, None) => true,
                _ => false,
            };
            if !same_name || count != entry.roots.len() { continue; }
            let mut equal = true;
            for (root, stored) in roots.clone().zip(&entry.roots) {
                session.charge_host_work(3)?;
                session.check(stored)?;
                let mut scope = RootScope::new(&mut session.store);
                let root = root.value.to_rooted(&mut scope);
                let stored = stored.value.to_rooted(&mut scope);
                if !Rooted::<AnyRef>::ref_eq(&scope, &root, &stored)? {
                    equal = false;
                    break;
                }
            }
            if equal { return Ok(entry.valid); }
        }
        // Queries inspect authority or allocate copied data, but cannot invoke
        // callbacks or mutate that authority in this certification interval.
        let valid = validate(session)?;
        let mut owned = Vec::new();
        reserve(session, &mut owned, count)?;
        for root in roots {
            session.charge_host_work(1)?;
            owned.push(root.clone());
        }
        reserve(session, &mut self.entries, 1)?;
        // Prepay publication before changing the index. Everything after a
        // successful new-head insertion is a reserved push or exact link write.
        session.charge_host_work(3)?;
        let index = self.entries.len();
        if head.is_none() {
            self.heads.get_or_insert(bucket, index, &mut Meter(session))?;
        }
        self.entries.push(Entry { name, roots: owned, valid, next: None });
        if let Some(tail) = tail { self.entries[tail].next = Some(index); }
        Ok(valid)
    }
    pub(crate) fn class(&mut self, session: &mut Session, input: &BridgeClassInput) -> Result<bool> {
        let _request = session.operation_budget_scope()?;
        let bucket = name_bucket(session, input.name)?;
        self.query(session, Some(input.name),
            input.constructor.iter().chain(std::iter::once(&input.descriptor)), bucket,
            |session| session.initial_source_class_matches(input.name, input.constructor.as_ref(), &input.descriptor))
    }
    pub(super) fn prototype(&mut self, session: &mut Session, input: &PrototypeInputs) -> Result<bool> {
        self.query(session, None, input.expectation_fields().iter(), 0, |session| input.matches(session))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable_session::construction_records::ConstructionRecords;

    fn class(session: &mut Session, name: &'static str) -> BridgeClassInput {
        let constructor = session.eval(&format!("suss.core/{name}")).unwrap();
        let descriptor = session.data_descriptor(&constructor, true).unwrap();
        BridgeClassInput { name, constructor: Some(constructor), descriptor }
    }

    #[test]
    fn indexed_staging_fuel_sweep_preserves_heads_links_and_recovery() {
        fn query(audit: &mut CertificationAudit, session: &mut Session,
            input: &BridgeClassInput, bucket: u64) -> Result<bool> {
            audit.query(session, Some(input.name), input.constructor.iter()
                .chain(std::iter::once(&input.descriptor)), bucket,
                |session| session.initial_source_class_matches(input.name,
                    input.constructor.as_ref(), &input.descriptor))
        }
        fn seed(session: &mut Session, inputs: &[BridgeClassInput]) -> CertificationAudit {
            let _request = session.operation_budget_scope().unwrap();
            let mut audit = CertificationAudit::new(session).unwrap();
            for (index, input) in inputs.iter().enumerate() {
                assert_eq!(query(&mut audit, session, input, 0).unwrap(), index < 2);
            }
            assert_eq!(audit.entries.len(), 4);
            assert_eq!(audit.entries.capacity(), 4, "next insertion must grow storage");
            audit
        }
        let mut session = Session::new_macro().unwrap();
        let list = class(&mut session, "List");
        let cons = class(&mut session, "Cons");
        let wrong_name = BridgeClassInput { name: "Cons", constructor: list.constructor.clone(), descriptor: list.descriptor.clone() };
        let wrong_descriptor = BridgeClassInput { name: "List", constructor: list.constructor.clone(), descriptor: cons.descriptor.clone() };
        let candidate = BridgeClassInput { name: "List", constructor: None, descriptor: list.descriptor.clone() };
        let inputs = [list, cons, wrong_name, wrong_descriptor];
        for bucket in [0, 1] {
            let mut probe = seed(&mut session, &inputs);
            let cost = session.with_operation_budget(|session| {
                let before = session.operation_fuel_remaining()?;
                assert!(!query(&mut probe, session, &candidate, bucket)?);
                Ok(before - session.operation_fuel_remaining()?)
            }).unwrap();
            drop(probe);
            eprintln!("audit staging bucket {bucket} fuel sweep: {cost}");
            for allowance in 0..cost {
                let mut audit = seed(&mut session, &inputs);
                session.with_operation_budget(|session| {
                    session.store.set_fuel(allowance)?;
                    let error = query(&mut audit, session, &candidate, bucket).unwrap_err();
                    assert!(matches!(error, SessionError::Trap(ref error)
                        if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
                        "bucket {bucket}, fuel {allowance}: {error:?}");
                    assert_eq!(audit.entries.len(), 4);
                    assert_eq!(audit.heads.len(), 1);
                    for (index, entry) in audit.entries.iter().enumerate() {
                        assert_eq!(entry.next, if index < 3 { Some(index + 1) } else { None });
                    }
                    Ok(())
                }).unwrap();
                session.with_operation_budget(|session| {
                    assert_eq!(audit.heads.get(&0, &mut Meter(session))?.copied(), Some(0));
                    for (index, input) in inputs.iter().enumerate() {
                        assert_eq!(query(&mut audit, session, input, 0)?, index < 2);
                    }
                    assert!(!query(&mut audit, session, &candidate, bucket)?);
                    assert_eq!(audit.entries.len(), 5);
                    assert_eq!(audit.heads.len(), if bucket == 0 { 1 } else { 2 });
                    assert!(!query(&mut audit, session, &candidate, bucket)?);
                    assert_eq!(audit.entries.len(), 5);
                    Ok(())
                }).unwrap();
            }
        }
    }
    #[test]
    fn forced_class_and_prototype_collisions_keep_exact_bindings_and_grow() {
        fn forced(audit: &mut CertificationAudit, session: &mut Session, input: &BridgeClassInput) -> Result<bool> {
            audit.query(session, Some(input.name),
                input.constructor.iter().chain(std::iter::once(&input.descriptor)), 0,
                |session| session.initial_source_class_matches(input.name, input.constructor.as_ref(), &input.descriptor))
        }
        let mut session = Session::new_macro().unwrap();
        drop(session.eval("(list 17)").unwrap());
        let list = class(&mut session, "List");
        let cons = class(&mut session, "Cons");
        let handles = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let prototype = super::super::canonical_methods::runtime(&mut session,
            "owned-descriptor-prototype", &[&list.descriptor], &handles).unwrap();
        let origin = session.prototype_origin_inputs(list.constructor.as_ref().unwrap(),
            &list.descriptor, &prototype).unwrap().unwrap();
        let wrong = BridgeClassInput { name: "Cons", constructor: list.constructor.clone(), descriptor: list.descriptor.clone() };
        let absent = BridgeClassInput { name: "List", constructor: None, descriptor: list.descriptor.clone() };
        let mut audit = CertificationAudit::new(&mut session).unwrap();
        assert!(audit.prototype(&mut session, &origin).unwrap());
        assert!(forced(&mut audit, &mut session, &list).unwrap());
        assert!(!forced(&mut audit, &mut session, &wrong).unwrap());
        assert!(forced(&mut audit, &mut session, &cons).unwrap());
        assert!(!forced(&mut audit, &mut session, &absent).unwrap());
        assert_eq!(audit.entries.len(), 5);
        assert_eq!(audit.heads.len(), 1);
        session.collect().unwrap();
        assert!(audit.prototype(&mut session, &origin).unwrap());
        for (input, expected) in [(&absent, false), (&cons, true), (&wrong, false), (&list, true)] {
            assert_eq!(forced(&mut audit, &mut session, input).unwrap(), expected);
        }
        assert_eq!(audit.entries.len(), 5);
    }
    #[test]
    fn warm_complete_class_catalog_fits_finite_fuel() {
        let mut session = Session::new_macro().unwrap();
        let registry = session.canonical_methods.clone().unwrap();
        let mut inputs = Vec::new();
        for (name, constructor) in crate::portable_macro_data::source_class_names()
            .zip(registry.source_constructor_roots()) {
            inputs.push(BridgeClassInput { name, constructor: Some(constructor.clone()),
                descriptor: session.data_descriptor(constructor, true).unwrap() });
        }
        let array = session.data_array(&[]).unwrap();
        inputs.push(BridgeClassInput { name: "SourceArray", constructor: None,
            descriptor: session.data_descriptor(&array, false).unwrap() });
        assert_eq!(inputs.len(), 24);
        let mut audit = CertificationAudit::new(&mut session).unwrap();
        for input in &inputs { assert!(audit.class(&mut session, input).unwrap()); }
        session.collect().unwrap();
        session.set_operation_fuel(5_000);
        let operation = session.operation_budget_scope().unwrap();
        let before = session.operation_fuel_remaining().unwrap();
        for input in &inputs { assert!(audit.class(&mut session, input).unwrap()); }
        eprintln!("warm class catalog fuel spent: {}", before - session.operation_fuel_remaining().unwrap());
        assert_eq!(audit.entries.len(), 24);
        drop(operation);
    }
    #[test]
    fn audit_class_exact_binding_gc_false_and_later_owner_error() {
        let mut session = Session::new_macro().unwrap();
        let list = class(&mut session, "List");
        let cons = class(&mut session, "Cons");
        let mut audit = CertificationAudit::new(&mut session).unwrap();
        assert!(audit.class(&mut session, &list).unwrap());
        assert_eq!(audit.entries.len(), 1);
        session.collect().unwrap();
        assert!(audit.class(&mut session, &list).unwrap());
        assert_eq!(audit.entries.len(), 1, "GC must not break rooted exact hits");

        for changed in [
            BridgeClassInput { name: "Cons", constructor: list.constructor.clone(), descriptor: list.descriptor.clone() },
            BridgeClassInput { name: "List", constructor: None, descriptor: list.descriptor.clone() },
            BridgeClassInput { name: "List", constructor: cons.constructor.clone(), descriptor: list.descriptor.clone() },
            BridgeClassInput { name: "List", constructor: list.constructor.clone(), descriptor: cons.descriptor.clone() },
        ] {
            let before = audit.entries.len();
            assert!(!session.initial_source_class_matches(changed.name,
                changed.constructor.as_ref(), &changed.descriptor).unwrap());
            assert!(!audit.class(&mut session, &changed).unwrap());
            assert_eq!(audit.entries.len(), before + 1, "every changed binding must miss");
            assert!(!audit.class(&mut session, &changed).unwrap());
            assert_eq!(audit.entries.len(), before + 1, "completed false results may hit");
        }

        let mut foreign = Session::new_macro().unwrap();
        let foreign_value = foreign.eval("nil").unwrap();
        let bad = BridgeClassInput { name: "List", constructor: list.constructor.clone(), descriptor: foreign_value };
        let before = audit.entries.len();
        assert!(matches!(audit.class(&mut session, &bad), Err(SessionError::ForeignValue)));
        assert_eq!(audit.entries.len(), before);
        let unknown = BridgeClassInput { name: "AuditUnknown", constructor: list.constructor.clone(), descriptor: list.descriptor.clone() };
        let records = ConstructionRecords::with_bridge_classes(vec![unknown, bad]);
        assert!(matches!(records.matches_class_schemas_audited(&mut session, &mut audit),
            Err(SessionError::ForeignValue)), "false coverage must not suppress a later owner error");
    }

    #[test]
    fn audit_errors_do_not_publish_and_fuel_irq_recover() {
        let mut session = Session::new_macro().unwrap();
        let list = class(&mut session, "List");
        let mut probe = CertificationAudit::new(&mut session).unwrap();
        let cost = session.with_operation_budget(|session| {
            let before = session.operation_fuel_remaining()?;
            assert!(probe.class(session, &list)?);
            Ok(before - session.operation_fuel_remaining()?)
        }).unwrap();
        drop(probe);

        let mut audit = CertificationAudit::new(&mut session).unwrap();
        session.with_operation_budget(|session| {
            session.store.set_fuel(cost - 1)?;
            let error = audit.class(session, &list).unwrap_err();
            assert!(matches!(error, SessionError::Trap(ref error)
                if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
            assert!(audit.entries.is_empty(), "failure at final staging charge must not publish");
            Ok(())
        }).unwrap();
        assert!(audit.class(&mut session, &list).unwrap());
        assert_eq!(audit.entries.len(), 1);
        session.with_operation_budget(|session| {
            session.interrupt_handle().interrupt();
            assert!(audit.class(session, &list).unwrap_err().is_interrupt());
            assert_eq!(audit.entries.len(), 1);
            Ok(())
        }).unwrap();
        assert!(audit.class(&mut session, &list).unwrap());

        let mut empty = CertificationAudit::new(&mut session).unwrap();
        session.with_operation_budget(|session| {
            session.interrupt_handle().interrupt();
            assert!(empty.class(session, &list).unwrap_err().is_interrupt());
            assert!(empty.entries.is_empty());
            Ok(())
        }).unwrap();
        assert!(empty.class(&mut session, &list).unwrap());
        drop(empty);

        // Deliberate negative scope test: a changed registry must be rejected
        // before a cached true hit. Never validate under replacement authority.
        let saved = session.canonical_methods.take();
        assert!(audit.class(&mut session, &list).is_err());
        assert_eq!(audit.entries.len(), 1);
        drop(audit);
        session.canonical_methods = saved;
        let mut next = CertificationAudit::new(&mut session).unwrap();
        assert!(next.class(&mut session, &list).unwrap());
    }

    #[test]
    fn new_audit_observes_real_schema_mutation() {
        let mut session = Session::new_macro().unwrap();
        let list = class(&mut session, "List");
        let mut audit = CertificationAudit::new(&mut session).unwrap();
        assert!(audit.class(&mut session, &list).unwrap());
        drop(audit); // Mutation is forbidden inside the reuse interval.
        let poison = session.eval("\"audit-changed-schema\"").unwrap();
        session.inspect(&list.descriptor, |mut store, value| {
            let descriptor = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let schema = descriptor.field(&mut store, 1)?.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let poison = poison.value.to_rooted(&mut store);
            schema.set(&mut store, 0, wasmtime::Val::AnyRef(Some(poison)))
        }).unwrap();
        let mut next = CertificationAudit::new(&mut session).unwrap();
        assert!(!next.class(&mut session, &list).unwrap());
    }
}
