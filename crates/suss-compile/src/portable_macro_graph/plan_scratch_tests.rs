use super::*;

fn read(source: &str) -> Form {
    suss_reader::forms::read_forms(source).unwrap().remove(0)
}

fn indexed_value(graph: &mut AnalysisGraph<'_>, root: Value, bridge: &FormBridge,
    sparse: bool) -> Result<SessionValue> {
    graph.finish_recipes()?;
    if sparse {
        graph.session.charge_host_work(std::mem::size_of::<materialization_index::Sparse>() as u64 + 2)?;
        graph.materialize_indexed(root, bridge, None, None,
            materialization_index::Sparse::default(), false)
    } else {
        graph.materialize_indexed(root, bridge, None, None,
            materialization_index::Dense, false)
    }
}

#[test]
fn dense_and_sparse_materialization_preserve_reader_effects_metadata_sharing_and_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def workspace-hash-log []) (def workspace-original-hash suss.core/hash)").unwrap();
        session.enter_namespace("suss.core").unwrap();
        session.eval("(def hash (fn [key] (do (set! user/workspace-hash-log (conj user/workspace-hash-log key)) (user/workspace-original-hash key))))").unwrap();
        session.enter_namespace("user").unwrap();
        let inspect = session.eval("(fn [v] [(identical? (nth v 2) (nth v 3)) (identical? (nth v 0) (nth v 1))])").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let mut observations = Vec::new();
        let mut costs = Vec::new();
        for route in 0..3 {
            session.eval("(set! workspace-hash-log [])").unwrap();
            let value = session.with_operation_budget(|session| {
                let mut graph = AnalysisGraph::new(&bridge, session);
                let reader = graph.form(&read("^{:mark 17} {:a 1 :b 2 :c 3 :d 4 :e 5 :f 6 :g 7 :h 8 :i 9}"), 0)?;
                let key = graph.keyword("payload")?;
                let number = graph.number(17)?;
                let shared = graph.recipe(Recipe::Map(vec![(key, number)]))?;
                let root = graph.recipe(Recipe::Vector(vec![reader, reader, shared, shared]))?;
                // Unreachable recipes cannot change reader occurrences or the
                // shared compiler map, but used to size all temporary arrays.
                for _ in 0..materialization_index::SPARSE_THRESHOLD + 1 {
                    graph.recipe(Recipe::Scalar(Literal::Nil))?;
                }
                graph.finish_recipes()?;
                let before = graph.session.operation_fuel_remaining()?;
                let result = if route == 2 { graph.materialize(root)? }
                    else { indexed_value(&mut graph, root, &bridge, route == 1)? };
                costs.push(before - graph.session.operation_fuel_remaining()?);
                Ok(result)
            }).unwrap();
            session.collect().unwrap();
            let count = session.eval("(count workspace-hash-log)").unwrap();
            assert_eq!(bridge.read(&mut session, &count, 0..1).unwrap().kind, Kind::Number(18.0));
            let effects = session.eval("workspace-hash-log").unwrap();
            let effects = bridge.read(&mut session, &effects, 0..1).unwrap();
            let decoded = bridge.read(&mut session, &value, 0..1).unwrap();
            let Kind::Vector(items) = &decoded.kind else { panic!("vector root"); };
            assert!(!items[0].metadata.is_empty());
            let aliases = session.invoke(&inspect, &[&value]).unwrap();
            let aliases = bridge.read(&mut session, &aliases, 0..1).unwrap();
            let Kind::Vector(flags) = &aliases.kind else { panic!("alias flags"); };
            assert_eq!(flags.iter().map(|v| &v.kind).collect::<Vec<_>>(),
                vec![&Kind::Bool(true), &Kind::Bool(false)]);
            observations.push((decoded, effects));
        }
        assert_eq!(observations[0], observations[1]);
        assert_eq!(observations[0], observations[2]);
        assert!(costs[2] < costs[0], "production sparse choice: {costs:?}");
        assert!(costs[1] < costs[0], "reachable scratch must remove actual work: {costs:?}");
    }
}

#[test]
fn dense_and_sparse_materialization_reject_cycles_and_recover_after_interrupts() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let bridge = FormBridge::new(&mut session).unwrap();
        for sparse in [false, true] {
            let scope = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            let root = graph.recipes.len();
            graph.recipe(Recipe::Alias(root)).unwrap();
            let error = indexed_value(&mut graph, root, &bridge, sparse).unwrap_err();
            assert!(error.to_string().contains("Cyclic compiler analysis graph"));
            drop(scope);
            graph.recipes[root] = Some(Recipe::Scalar(Literal::Number(23.0)));
            let scope = graph.session.operation_budget_scope().unwrap();
            graph.session.interrupt_handle().interrupt();
            assert!(indexed_value(&mut graph, root, &bridge, sparse).unwrap_err().is_interrupt());
            drop(scope);
            let scope = graph.session.operation_budget_scope().unwrap();
            let value = indexed_value(&mut graph, root, &bridge, sparse).unwrap();
            graph.session.collect().unwrap();
            assert_eq!(bridge.read(graph.session, &value, 0..1).unwrap().kind, Kind::Number(23.0));
            drop(scope);
        }
    }
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
