use super::*;

fn read(source: &str) -> Form {
    suss_reader::forms::read_forms(source).unwrap().remove(0)
}

#[test]
fn empty_plan_capacity_preserves_keys_metadata_and_failed_build_recovery() {
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    session.with_operation_budget(|session| {
        let mut graph = AnalysisGraph::new(&bridge, session);
        let first = read("^{:root 7} [foo {:a #{1 2}} (quote (unresolved 17))]");
        let original = first.clone();
        let value = graph.form(&first, 0)?;
        let capacity = graph.form_plan.nodes.capacity();
        assert!(capacity > 0);
        assert!(graph.form_plan.nodes.is_empty());
        assert_eq!(value, graph.form(&first, 0)?);
        assert_eq!(capacity, graph.form_plan.nodes.capacity());
        let other = read("^{:root 8} [foo {:a #{1 3}} (quote (unresolved 17))]");
        assert_ne!(value, graph.form(&other, 0)?);
        assert_eq!(first, original);
        assert!(graph.form_plan.nodes.is_empty());
        // This reaches the genuine builder's scalar-metadata rejection after
        // planning, unlike a guard failure before taking the scratch buffer.
        let bad = Form { span: 0..1, metadata: vec![read(":bad")], kind: Kind::Nil };
        assert!(graph.form_data(&bad, 0).is_err());
        assert!(graph.form_plan.nodes.is_empty());
        assert_eq!(graph.form_plan.nodes.capacity(), 0);
        assert_eq!(value, graph.form(&first, 0)?);
        assert!(graph.form_plan.nodes.is_empty());
        let actual = graph.materialize(value)?;
        let decoded = bridge.read(graph.session, &actual, 0..1)?;
        assert!(matches!(decoded.kind, Kind::Vector(_)));
        Ok(())
    }).unwrap();
}

#[test]
fn retained_plan_capacity_reduces_actual_work_at_same_finite_allowance() {
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let source = format!("[{}]", (0..129).map(|n| format!("item-{n}")).collect::<Vec<_>>().join(" "));
    let form = read(&source);
    let measure = |session: &mut Session, retain: bool| {
        session.with_operation_budget(|session| {
            let before = session.operation_fuel_remaining()?;
            let mut graph = AnalysisGraph::new(&bridge, session);
            let mut previous = None;
            for _ in 0..6 {
                if !retain { graph.form_plan = Plan::default(); }
                let value = graph.form(&form, 0)?;
                if let Some(previous) = previous { assert_eq!(previous, value); }
                previous = Some(value);
                assert!(graph.form_plan.nodes.is_empty());
            }
            Ok(before - graph.session.operation_fuel_remaining()?)
        })
    };
    let retained = measure(&mut session, true).unwrap();
    let discarded = measure(&mut session, false).unwrap();
    assert!(retained < discarded);
    session.set_operation_fuel(retained);
    assert_eq!(measure(&mut session, true).unwrap(), retained);
    let error = measure(&mut session, false).unwrap_err();
    assert!(matches!(error, SessionError::Trap(ref e)
        if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{error:?}");
    assert_eq!(measure(&mut session, true).unwrap(), retained);
    eprintln!("plan-capacity-cost retained={retained} discarded={discarded} same-allowance={retained}");
}

#[test]
fn plan_reset_all_fuel_cuts_and_irq_keep_empty_capacity_recoverable() {
    let mut session = Session::new_macro().unwrap();
    let form = read("[0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16]");
    let mut interner = Interner::default();
    let mut plan = Plan::default();
    session.with_operation_budget(|session| {
        interner.plan(&form, &mut plan, &mut |n| session.charge_host_work(n))?;
        Ok(())
    }).unwrap();
    let count = plan.nodes.len();
    let capacity = plan.nodes.capacity();
    let cost = count as u64 * 4;
    for allowance in 0..cost {
        // Build genuine nodes using the unchanged interner before testing reset.
        session.set_operation_fuel(10_000_000);
        plan.nodes.clear();
        session.with_operation_budget(|session| {
            interner.plan(&form, &mut plan, &mut |n| session.charge_host_work(n))?;
            Ok(())
        }).unwrap();
        session.set_operation_fuel(allowance);
        let error = session.with_operation_budget(|session| {
            plan.clear_metered(&mut |n| session.charge_host_work(n))
        }).unwrap_err();
        assert!(matches!(error, SessionError::Trap(ref e)
            if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{error:?}");
        assert_eq!(plan.nodes.len(), count - (allowance / 4) as usize);
        session.set_operation_fuel(cost);
        session.with_operation_budget(|session| {
            plan.clear_metered(&mut |n| session.charge_host_work(n))
        }).unwrap();
        assert!(plan.nodes.is_empty());
        assert_eq!(plan.nodes.capacity(), capacity);
    }
    session.set_operation_fuel(10_000_000);
    session.with_operation_budget(|session| {
        interner.plan(&form, &mut plan, &mut |n| session.charge_host_work(n))?;
        Ok(())
    }).unwrap();
    let irq = session.host_work_interrupt();
    let mut completed = 0;
    let error = session.with_operation_budget(|session| {
        plan.clear_metered(&mut |n| {
            session.charge_host_work(n)?;
            if n != 0 {
                completed += 1;
                irq.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            Ok(())
        })
    }).unwrap_err();
    assert!(error.is_interrupt());
    assert_eq!(completed, 1);
    assert_eq!(plan.nodes.len(), count - 1);
    irq.store(false, std::sync::atomic::Ordering::SeqCst);
    session.with_operation_budget(|session| plan.clear_metered(&mut |n| session.charge_host_work(n))).unwrap();
    assert!(plan.nodes.is_empty());
    assert_eq!(plan.nodes.capacity(), capacity);
}
