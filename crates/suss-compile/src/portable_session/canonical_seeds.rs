//! Seed witnesses captured only while the verified shipped bootstrap is private.
//! Exact storage comparison conservatively rejects later descriptor changes too.
use super::{
    Session, SessionError, SessionValue, canonical_methods, storage_snapshot::StorageSnapshot,
};
use crate::portable::{bootstrap, hir::Literal};
use std::sync::{Arc, atomic::AtomicUsize};
use wasmtime::{AsContextMut, RootScope, Val};

type Result<T> = std::result::Result<T, SessionError>;
pub(crate) struct CanonicalSeed {
    storage: StorageSnapshot,
}
pub(super) struct CanonicalSeeds {
    seeds: Vec<(&'static str, Arc<CanonicalSeed>)>,
}
impl CanonicalSeeds {
    pub(super) fn capture(
        session: &mut Session,
        authority: &bootstrap::IntegrityAuthority,
    ) -> Result<Self> {
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
        #[cfg(test)] after_first_fault: Option<bool>,
    ) -> Result<Self> {
        if session.bootstrap_core
            || session.canonical_seeds.is_some()
            || !std::ptr::eq(
                authority,
                bootstrap::shipped_authority(session.phase).map_err(SessionError::Compile)?,
            )
        {
            return Err(SessionError::Host(wasmtime::Error::msg(
                "Canonical seeds require initial shipped provisioning",
            )));
        }
        let _request = session.operation_budget_scope()?;
        let key = session.data_scalar(&Literal::String("EMPTY".encode_utf16().collect()))?;
        let mut seeds = Vec::new();
        session.charge_host_work((3 * std::mem::size_of::<(&str, Arc<CanonicalSeed>)>()) as u64)?;
        seeds.try_reserve_exact(3).map_err(|_| {
            SessionError::Host(wasmtime::Error::msg("Canonical seed allocation failed"))
        })?;
        for (name, owner) in [
            ("empty-list", "List"),
            ("empty-map", "PersistentArrayMap"),
            ("empty-set", "PersistentHashSet"),
        ] {
            session.charge_host_work(1)?;
            let class = canonical_methods::binding(session, "suss.core", owner, &handles)?;
            let value = canonical_methods::runtime(
                session,
                "named-property-get",
                &[&class, &key],
                &handles,
            )?;
            let storage = session.capture_storage_accounted(&value, handles.clone())?;
            seeds.push((name, Arc::new(CanonicalSeed { storage })));
            #[cfg(test)]
            if seeds.len() == 1 {
                match after_first_fault {
                    Some(true) => session.interrupt_handle().interrupt(),
                    Some(false) => session.store.set_fuel(0)?,
                    None => {}
                }
            }
        }
        Ok(Self { seeds })
    }
}
impl CanonicalSeed {
    pub(super) fn contains_initial_reference(
        &self,
        session: &mut Session,
        value: &SessionValue,
    ) -> Result<bool> {
        session.storage_contains_reference(value, &self.storage)
    }
    pub(crate) fn matches(&self, session: &mut Session, value: &SessionValue) -> Result<bool> {
        session.storage_matches(value, &self.storage)
    }
    // The initial snapshot remains the expectation. A valid replacement in an
    // initially bound IHash implementation slot is opaque live method DATA,
    // never pristine membership, write-domain or persistent reuse authority.
    pub(crate) fn fresh_direct_status(&self, session: &mut Session, value: &SessionValue)
        -> Result<(bool, bool)> {
        session.check(value)?;
        let _request = session.operation_budget_scope()?;
        session.charge_host_work(2)?; // optional authority retain and exception flag
        let methods = session.canonical_methods.clone();
        let runtime = session.runtime;
        let identity = session.identity;
        let interrupt = session.host_work_interrupt();
        let mut used_method_data = false;
        let mut scope = RootScope::new(&mut session.store);
        let root = Val::AnyRef(Some(value.value.to_rooted(&mut scope)));
        let healthy = self.storage.matches_in_context_with_slots(scope.as_context_mut(), root,
            identity, &interrupt, |mut store, array, index, actual| {
                Session::consume_host_work(store.as_context_mut(), 2, &interrupt)?;
                let Some(methods) = &methods else { return Ok(false); };
                let valid = methods.fresh_seed_method_slot_matches(
                    canonical_methods::InspectionContext { store, identity, runtime, interrupt: &interrupt },
                    array, index, actual)?;
                if valid { used_method_data = true; }
                Ok(valid)
            })?;
        Session::consume_host_work(scope.as_context_mut(), 2, &interrupt)?;
        Ok((healthy, healthy && !used_method_data))
    }
}
impl Session {
    pub(crate) fn canonical_seed(&mut self, name: &str) -> Result<Option<Arc<CanonicalSeed>>> {
        self.charge_host_work(1)?;
        let Some(seeds) = self.canonical_seeds.clone() else {
            return Ok(None);
        };
        for (candidate, seed) in &seeds.seeds {
            self.charge_host_work(
                (name.len() as u64)
                    .saturating_add(candidate.len() as u64)
                    .saturating_add(1),
            )?;
            if *candidate == name {
                return Ok(Some(seed.clone()));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::resolve::Phase;
    use std::sync::atomic::Ordering;
    fn session(phase: Phase) -> Session {
        match phase {
            Phase::Runtime => Session::new_repl().unwrap(),
            Phase::Macro => Session::new_macro().unwrap(),
        }
    }
    // Raw ABI injection exercises physical health, not source assignment support.
    fn put_seed_field(s: &mut Session, value: &SessionValue, field: u32, replacement: &SessionValue) {
        s.inspect(value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.field(&mut store, 1)?.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let replacement = replacement.value.to_rooted(&mut store);
            fields.set(&mut store, field, Val::AnyRef(Some(replacement)))
        }).unwrap();
    }
    #[test]
    fn fresh_seed_hash_exception_does_not_excuse_prototype_or_other_protocol_poison() {
        use crate::portable_session::memo_leaf_test_support::Method;
        // Retain every extracted GC reference as a SessionValue before the
        // scope ends; subsequent collection cannot invalidate restore values.
        fn retain_path(s: &mut Session, root: &SessionValue, path: &[(bool, u32)]) -> SessionValue {
            s.charge_host_work(path.len() as u64 * 3 + 1).unwrap();
            let mut scope = RootScope::new(&mut s.store);
            let mut value = Val::AnyRef(Some(root.value.to_rooted(&mut scope)));
            for &(array, index) in path {
                let reference = value.unwrap_anyref().unwrap();
                value = if array {
                    reference.as_array(&scope).unwrap().unwrap().get(&mut scope, index).unwrap()
                } else {
                    reference.as_struct(&scope).unwrap().unwrap().field(&mut scope, index as usize).unwrap()
                };
            }
            super::super::own(&mut scope, value, s.identity, &s.handles).unwrap()
        }
        fn put(s: &mut Session, array: &SessionValue, index: u32, value: &SessionValue) {
            s.inspect(array, |mut store, root| {
                let array = root.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                let value = value.value.to_rooted(&mut store);
                array.set(&mut store, index, Val::AnyRef(Some(value)))
            }).unwrap();
        }
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut s = session(phase);
            s.eval("(def leaf-original-method nil)").unwrap();
            let original_hash = Method::capture(&mut s, "PersistentArrayMap").unwrap();
            original_hash.bind_delegate(&mut s).unwrap();
            let map = s.eval("(.-EMPTY PersistentArrayMap)").unwrap();
            let seed = s.canonical_seed("empty-map").unwrap().unwrap();
            assert!(seed.matches(&mut s, &map).unwrap());
            // Exact seed-owned route: nominal field2 properties[0] is its
            // prototype; native prototype field1 is [descriptors,parent].
            // Mutate the prototype's own parent edge, not the seed root edge.
            let prototype = retain_path(&mut s, &map, &[(false, 2), (true, 0)]);
            let prototype_storage = retain_path(&mut s, &prototype, &[(false, 1)]);
            let original_parent = retain_path(&mut s, &prototype_storage, &[(true, 1)]);
            s.inspect(&prototype_storage, |store, value| {
                assert_eq!(value.unwrap_anyref().unwrap().as_array(&store)?.unwrap().len(&store)?, 2);
                Ok(())
            }).unwrap();
            let descriptor = s.data_descriptor(&map, false).unwrap();
            let table = retain_path(&mut s, &descriptor, &[(false, 2)]);
            let handles = s.handles.clone();
            let key_name = format!("key-{}", "suss.core/ICounted/-count/1".bytes()
                .map(|b| format!("{b:02x}")).collect::<String>());
            let count_key = canonical_methods::binding(&mut s, "suss.internal.protocol-keys", &key_name, &handles).unwrap();
            let original_count = canonical_methods::runtime(&mut s, "protocol-method-get",
                &[&descriptor, &count_key], &handles).unwrap();
            // Locate the already-existing slot by exact key root, never by
            // assumed slot ordinal or textual equality of a current key.
            let count_index = s.inspect(&table, |mut store, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                let key = count_key.value.to_rooted(&mut store);
                let len = array.len(&store)?;
                assert_eq!(len % 2, 0);
                for index in (0..len).step_by(2) {
                    let found = array.get(&mut store, index)?;
                    if wasmtime::Rooted::<wasmtime::AnyRef>::ref_eq(&store, found.unwrap_anyref().unwrap(), &key)? {
                        return Ok(index + 1);
                    }
                }
                panic!("shipped PersistentArrayMap must already implement ICounted");
            }).unwrap();
            s.eval("(extend-type PersistentArrayMap IHash (-hash [self] (leaf-original-method self)))").unwrap();
            assert_eq!(seed.fresh_direct_status(&mut s, &map).unwrap(), (true, false));
            let poison = s.eval("99").unwrap();
            put(&mut s, &prototype_storage, 1, &poison);
            s.collect().unwrap();
            assert_eq!(seed.fresh_direct_status(&mut s, &map).unwrap(), (false, false));
            assert!(!seed.matches(&mut s, &map).unwrap());
            put(&mut s, &prototype_storage, 1, &original_parent);
            s.collect().unwrap();
            assert_eq!(seed.fresh_direct_status(&mut s, &map).unwrap(), (true, false));
            // A valid closure in another protocol's existing value slot must
            // remain an ordinary snapshot mismatch, even while IHash is allowed.
            let replacement_count = s.eval("(fn [_] 73)").unwrap();
            put(&mut s, &table, count_index, &replacement_count);
            s.collect().unwrap();
            assert_eq!(seed.fresh_direct_status(&mut s, &map).unwrap(), (false, false));
            assert!(!seed.matches(&mut s, &map).unwrap());
            put(&mut s, &table, count_index, &original_count);
            s.collect().unwrap();
            assert_eq!(seed.fresh_direct_status(&mut s, &map).unwrap(), (true, false));
            assert!(!seed.matches(&mut s, &map).unwrap());
            original_hash.restore(&mut s).unwrap();
            assert_eq!(seed.fresh_direct_status(&mut s, &map).unwrap(), (true, true));
            assert!(seed.matches(&mut s, &map).unwrap());
        }
    }

    #[test]
    fn fresh_seed_method_data_preserves_pristine_membership_and_rejects_payload_poison() {
        use crate::portable_session::memo_leaf_test_support::Method;
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut s = session(phase);
            s.eval("(def leaf-original-method nil)").unwrap();
            let method = Method::capture(&mut s, "PersistentArrayMap").unwrap();
            method.bind_delegate(&mut s).unwrap();
            let map = s.eval("(.-EMPTY PersistentArrayMap)").unwrap();
            let set = s.eval("(.-EMPTY PersistentHashSet)").unwrap();
            let map_seed = s.canonical_seed("empty-map").unwrap().unwrap();
            let set_seed = s.canonical_seed("empty-set").unwrap().unwrap();
            assert_eq!(map_seed.fresh_direct_status(&mut s, &map).unwrap(), (true, true));
            assert_eq!(set_seed.fresh_direct_status(&mut s, &set).unwrap(), (true, true));
            s.eval("(extend-type PersistentArrayMap IHash (-hash [self] (leaf-original-method self)))").unwrap();
            s.collect().unwrap();
            for (value, seed) in [(&map, &map_seed), (&set, &set_seed)] {
                assert!(!seed.matches(&mut s, value).unwrap());
                assert_eq!(seed.fresh_direct_status(&mut s, value).unwrap(), (true, false));
                assert!(!seed.matches(&mut s, value).unwrap(), "fresh check cannot refresh pristine authority");
                assert!(seed.contains_initial_reference(&mut s, value).unwrap());
            }
            let class = s.eval("PersistentArrayMap").unwrap();
            let fields = s.data_fields(&map).unwrap();
            let clone = s.data_construct(&class, &fields.iter().collect::<Vec<_>>()).unwrap();
            assert_eq!(map_seed.fresh_direct_status(&mut s, &clone).unwrap(), (false, false));
            assert!(!map_seed.contains_initial_reference(&mut s, &clone).unwrap());
            let bad = s.eval("99").unwrap();
            for index in [0u32, 1, 3] {
                put_seed_field(&mut s, &map, index, &bad);
                assert_eq!(map_seed.fresh_direct_status(&mut s, &map).unwrap(), (false, false));
                put_seed_field(&mut s, &map, index, &fields[index as usize]);
                assert_eq!(map_seed.fresh_direct_status(&mut s, &map).unwrap(), (true, false));
            }
            // UID is a struct slot: an array exception must never excuse it.
            let old_uid = s.inspect(&map, |mut store, value| {
                let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let old = object.field(&mut store, 3)?.unwrap_anyref().unwrap().as_i31(&store)?.unwrap().get_u32();
                let bad = wasmtime::AnyRef::from_i31(&mut store, wasmtime::I31::new_u32(42).unwrap());
                object.set_field(&mut store, 3, Val::AnyRef(Some(bad)))?;
                Ok(old)
            }).unwrap();
            assert_eq!(map_seed.fresh_direct_status(&mut s, &map).unwrap(), (false, false));
            s.inspect(&map, |mut store, value| {
                let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let old = wasmtime::AnyRef::from_i31(&mut store, wasmtime::I31::new_u32(old_uid).unwrap());
                object.set_field(&mut store, 3, Val::AnyRef(Some(old)))
            }).unwrap();
            assert_eq!(map_seed.fresh_direct_status(&mut s, &map).unwrap(), (true, false));
            method.restore(&mut s).unwrap();
            assert!(map_seed.matches(&mut s, &map).unwrap());
            assert!(set_seed.matches(&mut s, &set).unwrap());
            // Poison nested SourceArray backing through the actual source setter.
            s.eval("(extend-type PersistentArrayMap IHash (-hash [self] (leaf-original-method self)))").unwrap();
            let mutate = s.eval("(fn [a] (aset a 0 99))").unwrap();
            s.invoke(&mutate, &[&fields[2]]).unwrap();
            s.collect().unwrap();
            assert_eq!(map_seed.fresh_direct_status(&mut s, &map).unwrap(), (false, false));
            assert_eq!(set_seed.fresh_direct_status(&mut s, &set).unwrap(), (false, false));
            assert!(!map_seed.matches(&mut s, &map).unwrap());
        }
    }
    #[test]
    fn fresh_seed_method_data_fuel_irq_missing_authority_and_reset_do_not_certify() {
        use crate::portable_session::memo_leaf_test_support::Method;
        let mut s = session(Phase::Macro);
        s.eval("(def leaf-original-method nil)").unwrap();
        let method = Method::capture(&mut s, "PersistentArrayMap").unwrap();
        method.bind_delegate(&mut s).unwrap();
        let map = s.eval("(.-EMPTY PersistentArrayMap)").unwrap();
        let seed = s.canonical_seed("empty-map").unwrap().unwrap();
        s.eval("(extend-type PersistentArrayMap IHash (-hash [self] (leaf-original-method self)))").unwrap();
        let work = s.with_operation_budget(|s| {
            let before = s.operation_fuel_remaining()?;
            assert_eq!(seed.fresh_direct_status(s, &map)?, (true, false));
            Ok(before - s.operation_fuel_remaining()?)
        }).unwrap();
        let handles = s.stats().external_value_handles;
        for cut in [0, 1, 2, 8, 32, 128, 512, 2048, 8192, work / 2, work.saturating_sub(1)] {
            if cut >= work { continue; }
            s.set_operation_fuel(cut);
            let error = seed.fresh_direct_status(&mut s, &map).unwrap_err();
            assert!(matches!(error, SessionError::Trap(ref e)
                if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "cut={cut}/{work}: {error:?}");
            assert_eq!(s.stats().external_value_handles, handles);
        }
        s.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        let request = s.operation_budget_scope().unwrap();
        s.interrupt_handle().interrupt();
        assert!(seed.fresh_direct_status(&mut s, &map).unwrap_err().is_interrupt());
        drop(request);
        assert_eq!(seed.fresh_direct_status(&mut s, &map).unwrap(), (true, false));
        let authority = s.canonical_methods.take();
        assert_eq!(seed.fresh_direct_status(&mut s, &map).unwrap(), (false, false));
        s.canonical_methods = authority;
        assert_eq!(seed.fresh_direct_status(&mut s, &map).unwrap(), (true, false));
        assert!(!seed.matches(&mut s, &map).unwrap());
        method.restore(&mut s).unwrap();
        assert!(seed.matches(&mut s, &map).unwrap());
        s.reset().unwrap();
        assert!(matches!(seed.fresh_direct_status(&mut s, &map), Err(SessionError::ForeignValue)));
        let current = s.eval("(.-EMPTY PersistentArrayMap)").unwrap();
        assert!(matches!(seed.fresh_direct_status(&mut s, &current), Err(SessionError::ForeignValue)));
    }

    #[test]
    fn canonical_seeds_cover_initial_children_without_authorizing_current_mutation() {
        use super::super::construction_records::{ConstructionRecords, Fact};
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let seed = session
                .eval("(.-EMPTY suss.core/PersistentArrayMap)")
                .unwrap();
            let witness = session.canonical_seed("empty-map").unwrap().unwrap();
            let fields = session.data_fields(&seed).unwrap();
            let child = &fields[2];
            let mut records = ConstructionRecords::default();
            records
                .push(
                    &mut session,
                    seed.clone(),
                    Fact::CanonicalSeed(witness.clone()),
                )
                .unwrap();
            session.collect().unwrap();
            for field in &fields {
                assert!(
                    witness
                        .contains_initial_reference(&mut session, field)
                        .unwrap()
                );
            }
            // A seed descendant covers an input edge, never an unrecorded root.
            assert!(
                !records
                    .matches_recorded_inputs(&mut session, child)
                    .unwrap()
            );
            let (parent, fact) = session.data_array_with_inputs(&[child]).unwrap();
            records
                .push(&mut session, parent.clone(), Fact::Array(Box::new(fact)))
                .unwrap();
            assert!(
                records
                    .matches_recorded_inputs(&mut session, &parent)
                    .unwrap()
            );
            assert!(records.matches_direct_facts(&mut session).unwrap());
            let replacement = session.data_array(&[]).unwrap();
            assert!(
                !witness
                    .contains_initial_reference(&mut session, &replacement)
                    .unwrap()
            );
            let (other, fact) = session.data_array_with_inputs(&[&replacement]).unwrap();
            records
                .push(&mut session, other.clone(), Fact::Array(Box::new(fact)))
                .unwrap();
            assert!(
                !records
                    .matches_recorded_inputs(&mut session, &other)
                    .unwrap()
            );
            // Mutate a known child through the actual source-array operation.
            let mutate = session.eval("(fn [a] (aset a 0 99))").unwrap();
            let read = session.eval("(fn [a] (aget a 0))").unwrap();
            drop(session.invoke(&mutate, &[child]).unwrap());
            session.collect().unwrap();
            assert!(
                witness
                    .contains_initial_reference(&mut session, child)
                    .unwrap()
            );
            assert!(!witness.matches(&mut session, &seed).unwrap());
            assert!(!records.matches_direct_facts(&mut session).unwrap());
            // Newly attached references cannot enter the pristine membership set.
            let current = session.invoke(&read, &[child]).unwrap();
            assert!(
                !witness
                    .contains_initial_reference(&mut session, &current)
                    .unwrap()
            );
        }
    }
    #[test]
    fn canonical_seeds_partial_capture_releases_roots_before_recovery() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            // Re-enter the private initial-state conditions with the unchanged
            // pristine bootstrap for deterministic partial-capture injection.
            let initial = session.canonical_seeds.take();
            session.bootstrap_core = false;
            let authority = bootstrap::shipped_authority(phase).unwrap();
            let external = session.handles.load(Ordering::Relaxed);
            for interrupted in [false, true] {
                let pending = Arc::new(AtomicUsize::new(0));
                let error = CanonicalSeeds::capture_accounted(
                    &mut session,
                    authority,
                    pending.clone(),
                    Some(interrupted),
                )
                .err()
                .expect("fault after first complete seed capture");
                if interrupted {
                    assert!(error.is_interrupt(), "{error:?}");
                } else {
                    assert!(
                        matches!(error, SessionError::Trap(ref e) if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
                        "{error:?}"
                    );
                }
                assert_eq!(pending.load(Ordering::Relaxed), 0);
                assert_eq!(session.handles.load(Ordering::Relaxed), external);
                assert!(session.canonical_seeds.is_none());
                let recovered = CanonicalSeeds::capture_accounted(
                    &mut session,
                    authority,
                    pending.clone(),
                    None,
                )
                .unwrap();
                assert_eq!(recovered.seeds.len(), 3);
                drop(recovered);
                assert_eq!(pending.load(Ordering::Relaxed), 0);
            }
            session.canonical_seeds = initial;
            session.bootstrap_core = true;
        }
    }
    #[test]
    fn canonical_seeds_bind_initial_roots_and_survive_hash_warming_and_gc() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let handles = session.handles.load(Ordering::Relaxed);
            for name in ["empty-list", "empty-map", "empty-set", "not-a-seed"] {
                drop(session.canonical_seed(name).unwrap());
            }
            assert_eq!(session.handles.load(Ordering::Relaxed), handles);
            for (name, owner, class) in [
                ("empty-list", "List", "EmptyList"),
                ("empty-map", "PersistentArrayMap", "PersistentArrayMap"),
                ("empty-set", "PersistentHashSet", "PersistentHashSet"),
            ] {
                let value = session
                    .eval(&format!("(.-EMPTY suss.core/{owner})"))
                    .unwrap();
                let witness = session.canonical_seed(name).unwrap().unwrap();
                assert!(witness.matches(&mut session, &value).unwrap());
                let hash = session.eval("suss.core/hash").unwrap();
                drop(session.invoke(&hash, &[&value]).unwrap());
                session.collect().unwrap();
                assert!(witness.matches(&mut session, &value).unwrap());
                let class = session.eval(&format!("suss.core/{class}")).unwrap();
                let fields = session.data_fields(&value).unwrap();
                let clone = session
                    .data_construct(&class, &fields.iter().collect::<Vec<_>>())
                    .unwrap();
                assert!(
                    !witness.matches(&mut session, &clone).unwrap(),
                    "equal contents do not establish bootstrap origin"
                );
            }
            let authority = bootstrap::shipped_authority(phase).unwrap();
            assert!(CanonicalSeeds::capture(&mut session, authority).is_err());
            let witness = session.canonical_seed("empty-map").unwrap().unwrap();
            let old = session
                .eval("(.-EMPTY suss.core/PersistentArrayMap)")
                .unwrap();
            session.reset().unwrap();
            assert!(matches!(
                witness.matches(&mut session, &old),
                Err(SessionError::ForeignValue)
            ));
        }
    }
    #[test]
    fn canonical_seeds_reject_mutation_before_a_bridge_can_observe_it() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            for (name, owner) in [
                ("empty-list", "List"),
                ("empty-map", "PersistentArrayMap"),
                ("empty-set", "PersistentHashSet"),
            ] {
                let witness = session.canonical_seed(name).unwrap().unwrap();
                let value = session
                    .eval(&format!("(.-EMPTY suss.core/{owner})"))
                    .unwrap();
                // Native ABI injection, not a source property-write claim.
                let poison = session.eval("{:seedPoison 99}").unwrap();
                session
                    .inspect(&value, |mut store, value| {
                        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                        let fields = object
                            .field(&mut store, 1)?
                            .unwrap_anyref()
                            .unwrap()
                            .as_array(&store)?
                            .unwrap();
                        let poison = poison.value.to_rooted(&mut store);
                        fields.set(&mut store, 0, wasmtime::Val::AnyRef(Some(poison)))?;
                        Ok(())
                    })
                    .unwrap();
                let post_mutation = session.capture_storage(&value).unwrap();
                session.collect().unwrap();
                assert!(session.storage_matches(&value, &post_mutation).unwrap());
                assert!(!witness.matches(&mut session, &value).unwrap());
                // A later lookup cannot refresh the initial expectation.
                let later = session.canonical_seed(name).unwrap().unwrap();
                assert!(!later.matches(&mut session, &value).unwrap());
            }
        }
    }
}

impl CanonicalSeed {
    pub(super) fn write_storage(&self)->&StorageSnapshot {&self.storage}
}
