//! Lifecycle-only synthetic graphs are private test setup, not source proofs.
//! The final test also exercises a genuine analyzed declaration metadata DAG.
use super::*;
use std::rc::Weak;
fn native(session: &mut Session, value: &SessionValue, inputs: Vec<Evidence>) -> Rc<Native> {
    // Default records intentionally have NO valid certification/data claim.
    // Only allocation/release and exact GC ownership are tested by this helper.
    Native::allocate(session, value, ConstructionRecords::default(), inputs).unwrap()
}
fn chain(
    session: &mut Session,
    value: &SessionValue,
    depth: usize,
) -> (Evidence, Vec<Weak<Native>>) {
    let mut weak = Vec::new();
    let leaf = native(session, value, Vec::new());
    weak.push(Rc::downgrade(&leaf));
    let mut head = Evidence::Native(leaf);
    for _ in 0..depth {
        let mut inputs = Vec::new();
        reserve(session, &mut inputs, 1).unwrap();
        inputs.push(head);
        let parent = native(session, value, inputs);
        weak.push(Rc::downgrade(&parent));
        head = Evidence::Native(parent);
    }
    (head, weak)
}
#[test]
fn deep_native_release_after_fuel_and_interrupt_requires_no_cleanup_budget_and_retries() {
    let mut session = Session::new_macro().unwrap();
    let value = session.eval("23").unwrap();
    for interrupt in [false, true] {
        let (head, weak) = session
            .with_operation_budget(|s| Ok(chain(s, &value, 4096)))
            .unwrap();
        session.collect().unwrap();
        assert!(weak.iter().all(|node| node.upgrade().is_some()));
        session.set_operation_fuel(if interrupt {
            crate::portable::DEFAULT_OPERATION_FUEL
        } else {
            0
        });
        let operation = session.operation_budget_scope().unwrap();
        let irq = session.host_work_interrupt();
        if interrupt {
            irq.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        let error = session.charge_host_work(1).unwrap_err();
        if interrupt {
            assert!(error.is_interrupt());
        } else {
            assert!(matches!(error, SessionError::Trap(ref e)
            if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
        }
        // Final-owner release runs with the SAME trapped/interrupting request.
        // No cleanup lease, budget refill, signal clearing or retry precedes it.
        drop(head);
        assert!(weak.iter().all(|node| node.upgrade().is_none()));
        irq.store(false, std::sync::atomic::Ordering::SeqCst);
        drop(operation);
        session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        let result = session.eval("42").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        assert_eq!(
            bridge.read(&mut session, &result, 0..0).unwrap().kind,
            suss_reader::forms::read_forms("42").unwrap()[0].kind
        );
    }
}
#[test]
fn release_shared_diamond_keeps_external_owner_and_drops_final_owner_once() {
    let mut session = Session::new_macro().unwrap();
    let value = session.eval("23").unwrap();
    session
        .with_operation_budget(|s| {
            let (head, mut weak) = chain(s, &value, 2048);
            let survivor = head.retained(s)?;
            let mut inputs = Vec::new();
            reserve(s, &mut inputs, 2)?;
            inputs.push(head.retained(s)?);
            inputs.push(head);
            let outer = native(s, &value, inputs);
            let outer_weak = Rc::downgrade(&outer);
            weak.push(outer_weak.clone());
            drop(outer);
            assert!(outer_weak.upgrade().is_none());
            assert!(
                weak[..weak.len() - 1]
                    .iter()
                    .all(|node| node.upgrade().is_some())
            );
            drop(survivor);
            assert!(weak.iter().all(|node| node.upgrade().is_none()));
            Ok(())
        })
        .unwrap();
}
#[test]
fn deep_source_alias_access_and_cache_eviction_are_constant_stack_and_preserve_survivor() {
    let (_environment, global, info) = super::tests::source();
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut cache = RootCache::default();
    let _built =
        super::tests::build(&mut session, &bridge, &mut cache, &global, &info, false).unwrap();
    let expected = cache
        .data
        .first()
        .expect("genuine native expectation")
        .expected
        .clone();
    let value = session.eval("23").unwrap();
    session
        .with_operation_budget(|s| {
            let (mut actual, weak_native) = chain(s, &value, 1024);
            let mut weak_source = Vec::new();
            for _ in 0..2048 {
                // Deliberately synthetic, unvalidated wrapper ownership only. This
                // private helper cannot manufacture a Certificate or publish cache.
                let root = Root::allocate(s, &expected, actual)?;
                weak_source.push(Rc::downgrade(&root));
                actual = Evidence::Source(root);
            }
            assert!(s.values_identical(actual.value(), &value)?);
            let Evidence::Source(top) = actual else {
                unreachable!()
            };
            let survivor = top.clone();
            let mut identity_only_cache = RootCache::default();
            identity_only_cache.data.push(top); // test-only ownership container
            identity_only_cache.data.clear(); // actual last-cache eviction path
            assert!(weak_source.iter().all(|node| node.upgrade().is_some()));
            assert!(weak_native.iter().all(|node| node.upgrade().is_some()));
            assert!(s.values_identical(&survivor.value, &value)?);
            drop(survivor);
            assert!(weak_source.iter().all(|node| node.upgrade().is_none()));
            assert!(weak_native.iter().all(|node| node.upgrade().is_none()));
            Ok(())
        })
        .unwrap();
}
#[test]
fn failed_partial_build_and_native_unwind_drop_all_owned_evidence_without_recovery_allocation() {
    let mut session = Session::new_macro().unwrap();
    let value = session.eval("23").unwrap();
    let (head, weak) = session
        .with_operation_budget(|s| Ok(chain(s, &value, 2048)))
        .unwrap();
    let failure = (|| -> Result<()> {
        let _partial_build = head;
        Err(invalid("test-only later construction failure"))
    })();
    assert!(failure.is_err());
    assert!(weak.iter().all(|node| node.upgrade().is_none()));
    let (head, weak) = session
        .with_operation_budget(|s| Ok(chain(s, &value, 2048)))
        .unwrap();
    // Catch an intentional ordinary native test panic, NOT stack overflow. The
    // production release path never catches panics or retries a guest callback.
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _partial_build = head;
        panic!("test-only unwind after original construction");
    }));
    assert!(failure.is_err());
    assert!(weak.iter().all(|node| node.upgrade().is_none()));
}
#[test]
fn genuine_deep_reader_metadata_retains_original_native_evidence_and_releases_after_owner_drop() {
    use crate::portable::{
        hir,
        resolve::{Environment, Phase},
    };
    let depth = 192;
    let text = format!(
        "(def ^{{:deep {}nil{}}} watched 17)",
        "[".repeat(depth),
        "]".repeat(depth)
    );
    let forms = suss_reader::forms::read_forms(&text).unwrap();
    let environment = hir::prepare(&forms, 0..text.len(), &Environment::default(), Phase::Macro)
        .unwrap()
        .1;
    let binding = environment
        .resolve(Phase::Macro, &suss_reader::Symbol::new("watched"), 0..0)
        .unwrap();
    let global = binding.global().clone();
    let info = environment.shared_definition_info(&global).unwrap().clone();
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut cache = RootCache::default();
    let root =
        crate::portable_session::memo_leaf_test_support::with_compilation_data_fuel(&mut session, |session| super::tests::build(session, &bridge, &mut cache, &global, &info, false))
            .unwrap_or_else(|error| panic!(
                "deep reader DATA build depth={depth} remaining={:?}: {error:?}",
                session.operation_fuel_remaining()));
    let meta = super::tests::metadata(&mut session, &root, false);
    let project = session.eval("(fn [m] (loop [v (get m :deep) n 0] (if (vector? v) (recur (nth v 0) (inc n)) [n (nil? v)])))").unwrap();
    session.collect().unwrap();
    let observed = session.invoke(&project, &[&meta]).unwrap();
    assert_eq!(
        crate::portable_repl::display(&mut session, &observed).unwrap(),
        format!("[{depth} true]")
    );
    let mut pending: Vec<_> = cache.data.iter().cloned().map(Evidence::Source).collect();
    let mut native = Vec::new();
    let mut source = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    while let Some(current) = pending.pop() {
        if !seen.insert(current.identity()) {
            continue;
        }
        match current {
            Evidence::Native(node) => {
                native.push(Rc::downgrade(&node));
                pending.extend(node.inputs.iter().cloned());
            }
            Evidence::Source(node) => {
                source.push(Rc::downgrade(&node));
                pending.push(node.actual().clone());
            }
        }
    }
    assert!(
        native.len() >= depth,
        "actual recorded ReaderVector chain required"
    );
    assert!(!source.is_empty());
    // Guest roots/meta remain owned, but cannot keep native proof/data storage
    // alive. Cache teardown is the final owner of the evidence forest here.
    drop(cache);
    assert!(native.iter().all(|node| node.upgrade().is_none()));
    assert!(source.iter().all(|node| node.upgrade().is_none()));
    session.collect().unwrap();
    let after = session.invoke(&project, &[&meta]).unwrap();
    assert_eq!(
        crate::portable_repl::display(&mut session, &after).unwrap(),
        format!("[{depth} true]")
    );
}

#[test]
fn every_new_release_preallocation_fuel_cut_discards_unpublished_owners_and_retries() {
    let (_environment, global, info) = super::tests::source();
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut cache = RootCache::default();
    let _root =
        super::tests::build(&mut session, &bridge, &mut cache, &global, &info, false).unwrap();
    let expected = cache.data.first().unwrap().expected.clone();
    let prior: Vec<_> = cache.data.iter().map(Rc::as_ptr).collect();
    let value = session.eval("23").unwrap();
    for source in [false, true] {
        let prepare = |s: &mut Session| -> Result<(Evidence, Weak<Native>)> {
            let node = Native::allocate(s, &value, ConstructionRecords::default(), Vec::new())?;
            let weak = Rc::downgrade(&node);
            Ok((Evidence::Native(node), weak))
        };
        let allocate = |s: &mut Session, actual: Evidence| -> Result<Evidence> {
            if source {
                Ok(Evidence::Source(Root::allocate(s, &expected, actual)?))
            } else {
                // Input-buffer allocation belongs to caller construction and is
                // test setup here. This sweep isolates ALL new node/frame fees.
                let inputs = vec![actual];
                Ok(Evidence::Native(Native::allocate(
                    s,
                    &value,
                    ConstructionRecords::default(),
                    inputs,
                )?))
            }
        };
        let (actual, weak) = session.with_operation_budget(prepare).unwrap();
        let required = session
            .with_operation_budget(|s| {
                let before = s.operation_fuel_remaining()?;
                let node = allocate(s, actual)?;
                let used = before - s.operation_fuel_remaining()?;
                drop(node);
                Ok(used)
            })
            .unwrap();
        assert!(weak.upgrade().is_none());
        assert!(required > 0);
        for allowance in 0..=required {
            session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
            let (actual, weak) = session.with_operation_budget(prepare).unwrap();
            session.set_operation_fuel(allowance);
            let result = session.with_operation_budget(|s| {
                let result = allocate(s, actual);
                match result {
                    Ok(node) => {
                        assert_eq!(allowance, required);
                        assert_eq!(s.operation_fuel_remaining()?, 0);
                        // No more budget is available. Final drop must succeed
                        // using pre-acquired frames, including the input owner.
                        drop(node);
                        Ok(())
                    }
                    Err(error) => {
                        assert!(allowance < required);
                        assert!(matches!(error, SessionError::Trap(ref e)
                            if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
                        Err(error)
                    }
                }
            });
            assert_eq!(result.is_ok(), allowance == required);
            assert!(
                weak.upgrade().is_none(),
                "no leaked child on {source}/{allowance}"
            );
            assert_eq!(cache.data.iter().map(Rc::as_ptr).collect::<Vec<_>>(), prior);
        }
        session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        let (actual, weak) = session.with_operation_budget(prepare).unwrap();
        session
            .with_operation_budget(|s| {
                drop(allocate(s, actual)?);
                Ok(())
            })
            .unwrap();
        assert!(weak.upgrade().is_none());
    }
}
