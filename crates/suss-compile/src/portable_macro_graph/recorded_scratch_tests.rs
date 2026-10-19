//! Current-ad6 comparisons. Historical f56 scene adapted to explicit recording
//! route only. No automatic sparse selection; ordinary materialize stays dense.
use super::*;
fn read(source: &str) -> Form {
    suss_reader::forms::read_forms(source).unwrap().remove(0)
}
fn indexed_value(graph: &mut AnalysisGraph<'_>, root: Value, bridge: &FormBridge,
    sparse: bool) -> Result<SessionValue> {
    // FORCE each generic implementation, independent of production threshold.
    // Sparse construction/adapter fees match the explicit sparse adapter in v2.
    if sparse {
        graph.session.charge_host_work(1)?;
        let index = materialization_index::Sparse::new(&mut |n| graph.session.charge_host_work(n))?;
        graph.materialize_indexed(root, bridge, None, index)
    } else { graph.materialize_using(root, bridge) }
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
        for route in 0..2 {
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
                for _ in 0..20_000 {
                    graph.recipe(Recipe::Scalar(Literal::Nil))?;
                }
                graph.finish_recipes()?;
                let before = graph.session.operation_fuel_remaining()?;
                let result = indexed_value(&mut graph, root, &bridge, route == 1)?;
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


/// R can approach N for arbitrary reader data. Exercise progressively wide,
/// genuinely reached reader maps with and without an unreachable graph tail.
/// Sparse is NOT required to win; costs and a finite-budget counterexample are
/// printed and retained rather than hiding its index overhead.
#[test]
fn wide_reader_metadata_reports_dense_sparse_cost_and_exhaustion_tradeoff() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // Cost calibration only, not default-budget acceptance. Preserve all
        // wide data, forced routes and finite-budget adverse comparisons.
        session.set_operation_fuel(200_000_000);
        eprintln!("recorded-scratch-wide diagnostic-only allowance=200000000");
        let bridge = FormBridge::new(&mut session).unwrap();
        for width in [32usize, 256] {
            let body = (0..width).map(|n| format!(":k{n} {n}")).collect::<Vec<_>>().join(" ");
            let form = read(&format!("^{{:wide true}} {{{body}}}"));
            for padding in [0usize, 20_000] {
                let mut costs = Vec::new();
                let mut results = Vec::new();
                for sparse in [false, true] {
                    let value = session.with_operation_budget(|session| {
                        let mut graph = AnalysisGraph::new(&bridge, session);
                        let root = graph.form(&form, 0)?;
                        for _ in 0..padding { graph.recipe(Recipe::Scalar(Literal::Nil))?; }
                        graph.finish_recipes()?;
                        let n = graph.recipes.len();
                        let before = graph.session.operation_fuel_remaining()?;
                        let value = indexed_value(&mut graph, root, &bridge, sparse)?;
                        let cost = before - graph.session.operation_fuel_remaining()?;
                        eprintln!("recorded-scratch-wide width={width} padding={padding} n={n} sparse={sparse} cost={cost}");
                        costs.push(cost);
                        Ok(value)
                    }).unwrap();
                    session.collect().unwrap();
                    results.push(bridge.read(&mut session, &value, 0..1).unwrap());
                }
                assert_eq!(results[0], results[1]);
                // Same built graph at the cheaper measured materialization
                // allowance: preserve any new exhaustion of the other route.
                let scope = session.operation_budget_scope().unwrap();
                let mut graph = AnalysisGraph::new(&bridge, &mut session);
                let root = graph.form(&form, 0).unwrap();
                for _ in 0..padding { graph.recipe(Recipe::Scalar(Literal::Nil)).unwrap(); }
                graph.finish_recipes().unwrap();
                let allowance = costs[0].min(costs[1]);
                // Graph preparation is outside each measured materialization
                // budget. Only this test changes allowances, never production.
                drop(scope);
                for (route, cost) in costs.iter().enumerate() {
                    graph.session.set_operation_fuel(allowance);
                    let request = graph.session.operation_budget_scope().unwrap();
                    let result = indexed_value(&mut graph, root, &bridge, route == 1);
                    if *cost == allowance { assert!(result.is_ok(), "route={route} {result:?}"); }
                    else {
                        let error = result.unwrap_err();
                        assert!(matches!(error, SessionError::Trap(ref e)
                            if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{error:?}");
                    }
                    drop(request);
                }
                graph.session.set_operation_fuel(200_000_000);
                drop(graph);
            }
        }
    }
}

#[test]
fn recorded_sparse_typed_fuel_interrupt_and_fresh_retry_preserve_source() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let bridge = FormBridge::new(&mut session).unwrap();
        let source = session.eval("[71 72]").unwrap();
        let original = bridge.read(&mut session, &source, 0..1).unwrap();
        // Fresh native graph per attempt: no replay after failed guest effects.
        let build = |session: &mut Session, sparse| -> Result<SessionValue> {
            let mut graph = AnalysisGraph::new(&bridge, session);
            let number = graph.recipe(Recipe::Scalar(Literal::Number(23.0)))?;
            let root = graph.recipe(Recipe::Vector(vec![number, number]))?;
            indexed_value(&mut graph, root, &bridge, sparse)
        };
        for sparse in [false, true] {
            let cold = session.with_operation_budget(|session| {
                let before = session.operation_fuel_remaining()?;
                drop(build(session, sparse)?);
                Ok(before - session.operation_fuel_remaining()?)
            }).unwrap();
            // Record first and subsequent request costs independently.
            // Neither is an exact trap threshold: Wasmtime checks in batches.
            // The final-unit case below checks that boundary explicitly.
            let full = session.with_operation_budget(|session| {
                let before = session.operation_fuel_remaining()?;
                drop(build(session, sparse)?);
                Ok(before - session.operation_fuel_remaining()?)
            }).unwrap();
            eprintln!("recorded-scratch-fuel phase={:?} sparse={sparse} cold={cold} warm={full}", session.phase());
            let cuts = [0, 1, 64.min(full - 1), 128.min(full - 1), full / 2, full - 1];
            for allowance in cuts {
                session.set_operation_fuel(allowance);
                let request = session.operation_budget_scope().unwrap();
                let boundary = match build(&mut session, sparse) {
                    Err(error) => {
                        assert!(matches!(error, SessionError::Trap(ref e)
                            if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{error:?}");
                        None
                    }
                    Ok(value) => {
                        // Wasmtime batches fuel checks; a final block can
                        // return at zero without a return-time trap check.
                        // Require this only at the last-unit probe, and prove
                        // that positive host work cannot continue the request.
                        let remaining = session.operation_fuel_remaining().unwrap();
                        eprintln!("recorded-scratch-boundary phase={:?} sparse={sparse} allowance={allowance} cold={cold} warm={full} remaining={remaining}", session.phase());
                        assert_eq!(allowance, full - 1);
                        assert_eq!(remaining, 0);
                        let error = session.charge_host_work(1).unwrap_err();
                        assert!(matches!(error, SessionError::Trap(ref e)
                            if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{error:?}");
                        Some(value)
                    }
                };
                drop(request);
                session.set_operation_fuel(10_000_000);
                session.collect().unwrap();
                assert_eq!(bridge.read(&mut session, &source, 0..1).unwrap(), original);
                let value = session.with_operation_budget(|session| build(session, sparse)).unwrap();
                for value in boundary.iter().chain(std::iter::once(&value)) {
                    let actual = bridge.read(&mut session, value, 0..1).unwrap();
                    let Kind::Vector(items) = &actual.kind else { panic!("vector recovery value"); };
                    assert_eq!(items.len(), 2);
                    for item in items {
                        assert_eq!(item.kind, Kind::Number(23.0));
                        assert_eq!(item.span, 0..1, "bridge inspection range, not parsed syntax coordinates");
                        assert!(item.metadata.is_empty());
                    }
                }
            }
            let request = session.operation_budget_scope().unwrap();
            session.interrupt_handle().interrupt();
            assert!(build(&mut session, sparse).unwrap_err().is_interrupt());
            drop(request);
            session.with_operation_budget(|session| build(session, sparse)).unwrap();
        }
    }
}

#[test]
fn dense_sparse_reader_callback_failure_is_not_replayed_and_source_stays_live() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def scratch-fail-events []) (def scratch-fail-old-hash suss.core/hash)").unwrap();
        session.enter_namespace("suss.core").unwrap();
        session.eval("(def hash (fn [key] (do (set! user/scratch-fail-events (conj user/scratch-fail-events key)) (if (= key :e) (throw \"scratch-stop\") (user/scratch-fail-old-hash key)))))").unwrap();
        session.enter_namespace("user").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let form = read("{:a 1 :b 2 :c 3 :d 4 :e 5 :f 6 :g 7 :h 8 :i 9}");
        let mut observations = Vec::new();
        for sparse in [false, true] {
            session.eval("(set! scratch-fail-events [])").unwrap();
            let error = session.with_operation_budget(|session| {
                let mut graph = AnalysisGraph::new(&bridge, session);
                let root = graph.form(&form, 0)?;
                indexed_value(&mut graph, root, &bridge, sparse)
            }).unwrap_err();
            let SessionError::Language(thrown) = error else { panic!("expected original language exception: {error:?}"); };
            session.collect().unwrap();
            assert_eq!(bridge.read(&mut session, &thrown, 0..1).unwrap().kind,
                Kind::String("scratch-stop".encode_utf16().collect()));
            let events = session.eval("scratch-fail-events").unwrap();
            let observed = bridge.read(&mut session, &events, 0..1).unwrap();
            assert_eq!(crate::portable_repl::display(&mut session, &events).unwrap(),
                "[:a :b :c :d :e]", "callback prefix must execute exactly once");
            observations.push(observed);
        }
        assert_eq!(observations[0], observations[1]);
        // Recovery is an explicit new request after restoring the live binding;
        // production never catches and replays a failed sparse invocation.
        session.enter_namespace("suss.core").unwrap();
        session.eval("(def hash user/scratch-fail-old-hash)").unwrap();
        session.enter_namespace("user").unwrap();
        // A bridge captures constructor inputs, including hash. A fresh request
        // must capture the restored binding rather than retain the throwing one.
        let bridge = FormBridge::new(&mut session).unwrap();
        let expected = session.eval("{:a 1 :b 2 :c 3 :d 4 :e 5 :f 6 :g 7 :h 8 :i 9}").unwrap();
        let equal = session.eval("(fn [a b] (= a b))").unwrap();
        session.with_operation_budget(|session| {
            let mut graph = AnalysisGraph::new(&bridge, session);
            let root = graph.form(&form, 0)?;
            let value = indexed_value(&mut graph, root, &bridge, true)?;
            let same = graph.session.invoke(&equal, &[&value, &expected])?;
            assert_eq!(bridge.read(graph.session, &same, 0..1)?.kind, Kind::Bool(true),
                "recovered map matches the full original data, independent of map iteration order");
            Ok(())
        }).unwrap();
    }
}
