//! Seed witnesses captured only while the verified shipped bootstrap is private.
//! Exact storage comparison conservatively rejects later descriptor changes too.
use super::{
    Session, SessionError, SessionValue, canonical_methods, storage_snapshot::StorageSnapshot,
};
use crate::portable::{bootstrap, hir::Literal};
use std::sync::{Arc, atomic::AtomicUsize};

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
                .push(&mut session, parent.clone(), Fact::Array(fact))
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
                .push(&mut session, other.clone(), Fact::Array(fact))
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
