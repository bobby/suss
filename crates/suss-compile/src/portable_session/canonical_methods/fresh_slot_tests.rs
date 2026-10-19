//! Bounded no-authority initial-slot tests adapted from c74eb63.
//! Historical results are not current execution evidence.
use super::*;
use crate::portable::resolve::Phase;
fn session(phase: Phase) -> Session {
    match phase {
        Phase::Runtime => Session::new_repl().unwrap(),
        Phase::Macro => Session::new_macro().unwrap(),
    }
}
fn fresh_slot_probe(
    session: &mut Session,
    methods: &CanonicalMethods,
    owner: &str,
    offset: u32,
    foreign: bool,
) -> Result<bool> {
    let _request = session.operation_budget_scope()?;
    let method = methods
        .methods
        .iter()
        .find(|method| method.owner == owner)
        .unwrap();
    let identity = session.identity;
    let runtime = session.runtime;
    let interrupt = session.host_work_interrupt();
    let mut scope = RootScope::new(&mut session.store);
    let table = method.table.value.to_rooted(&mut scope);
    let array = table.as_array(&scope)?.unwrap();
    let actual = array.get(&mut scope, method.implementation_index)?;
    methods.fresh_seed_method_slot_matches(
        InspectionContext {
            store: scope.as_context_mut(),
            identity: if foreign {
                identity.wrapping_add(1)
            } else {
                identity
            },
            runtime,
            interrupt: &interrupt,
        },
        Val::AnyRef(Some(table)),
        method.implementation_index + offset,
        &actual,
    )
}
#[test]
fn fresh_seed_slot_preserves_strict_method_authority_and_exact_binding() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = session(phase);
        let methods = session.canonical_methods.clone().unwrap();
        let descriptor = methods
            .methods
            .iter()
            .find(|m| m.owner == "PersistentArrayMap")
            .unwrap()
            .descriptor
            .clone();
        assert!(fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, false).unwrap());
        session
            .eval("(extend-type PersistentArrayMap IHash (-hash [self] 91))")
            .unwrap();
        session.collect().unwrap();
        assert!(fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, false).unwrap());
        assert!(
            !methods
                .matches(&mut session, "PersistentArrayMap", &descriptor)
                .unwrap()
        );
        assert!(!fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 1, false).unwrap());
        assert!(matches!(
            fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, true),
            Err(SessionError::ForeignValue)
        ));
        // A number cannot be treated as an opaque closure implementation.
        let number = session.eval("17").unwrap();
        let method = methods
            .methods
            .iter()
            .find(|m| m.owner == "PersistentArrayMap")
            .unwrap();
        session
            .inspect(&method.table, |mut store, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                let number = number.value.to_rooted(&mut store);
                array.set(
                    &mut store,
                    method.implementation_index,
                    Val::AnyRef(Some(number)),
                )
            })
            .unwrap();
        assert!(!fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, false).unwrap());
    }
}
#[test]
fn fresh_seed_slot_rejects_changed_key_owner_table_and_supplied_method() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = session(phase);
        let methods = session.canonical_methods.clone().unwrap();
        let method = methods
            .methods
            .iter()
            .find(|m| m.owner == "PersistentArrayMap")
            .unwrap();
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
            assert!(
                !methods
                    .fresh_seed_method_slot_matches(
                        InspectionContext {
                            store: scope.as_context_mut(),
                            identity,
                            runtime,
                            interrupt: &interrupt,
                        },
                        Val::AnyRef(Some(table)),
                        method.implementation_index,
                        &wrong
                    )
                    .unwrap()
            );
            let wrong_table = Val::AnyRef(Some(nil.value.to_rooted(&mut scope)));
            assert!(
                !methods
                    .fresh_seed_method_slot_matches(
                        InspectionContext {
                            store: scope.as_context_mut(),
                            identity,
                            runtime,
                            interrupt: &interrupt,
                        },
                        wrong_table,
                        method.implementation_index,
                        &wrong
                    )
                    .unwrap()
            );
        }
        session
            .inspect(&method.table, |mut store, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                let nil = nil.value.to_rooted(&mut store);
                array.set(
                    &mut store,
                    method.implementation_index - 1,
                    Val::AnyRef(Some(nil)),
                )
            })
            .unwrap();
        session.collect().unwrap();
        assert!(!fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, false).unwrap());
        session
            .inspect(&method.table, |mut store, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                let key = methods.key.value.to_rooted(&mut store);
                array.set(
                    &mut store,
                    method.implementation_index - 1,
                    Val::AnyRef(Some(key)),
                )
            })
            .unwrap();
        // The retained table alone does not authorize a changed owner table.
        session
            .inspect(&method.descriptor, |mut store, value| {
                let descriptor = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let other_table = other.table.value.to_rooted(&mut store);
                descriptor.set_field(&mut store, 2, Val::AnyRef(Some(other_table)))
            })
            .unwrap();
        session.collect().unwrap();
        assert!(!fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, false).unwrap());
        session
            .inspect(&method.descriptor, |mut store, value| {
                let descriptor = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let table = method.table.value.to_rooted(&mut store);
                descriptor.set_field(&mut store, 2, Val::AnyRef(Some(table)))
            })
            .unwrap();
        assert!(fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, false).unwrap());
    }
}
#[test]
fn fresh_seed_slot_every_direct_fuel_cut_is_typed_and_retryable() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = session(phase);
        let methods = session.canonical_methods.clone().unwrap();
        session
            .eval("(extend-type PersistentArrayMap IHash (-hash [self] 91))")
            .unwrap();
        let cost = session
            .with_operation_budget(|session| {
                let before = session.store.get_fuel()?;
                assert!(fresh_slot_probe(
                    session,
                    &methods,
                    "PersistentArrayMap",
                    0,
                    false
                )?);
                Ok(before - session.store.get_fuel()?)
            })
            .unwrap();
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
            assert!(
                fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, false).unwrap(),
                "full retry after allowance={allowance}/{cost}"
            );
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
    assert!(
        fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, false)
            .unwrap_err()
            .is_interrupt()
    );
    session.interrupt.store(false, Ordering::SeqCst);
    drop(request);
    assert!(fresh_slot_probe(&mut session, &methods, "PersistentArrayMap", 0, false).unwrap());
}
