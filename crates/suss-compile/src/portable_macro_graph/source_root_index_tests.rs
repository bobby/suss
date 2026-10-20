//! Native capture/index and boundary traversal regressions. No guest source
//! certificate is minted by these synthetic recipe DAGs.
use super::*;
use crate::portable::hir::Literal;

fn owner(session: &Session) -> Owner {
    let snapshot = session.compilation_snapshot();
    Owner::Namespace(Arc::new(hir::SourceNamespace::capture(
        &snapshot.environment,
        session.phase(),
    )))
}
fn graph_roots(
    graph: &mut AnalysisGraph<'_>,
    owner: &Owner,
    roots: usize,
    padding: usize,
) -> Result<Vec<Value>> {
    for _ in 0..padding {
        graph.recipe(Recipe::Scalar(Literal::Nil))?;
    }
    let mut aliases = Vec::new();
    let mut shared = None;
    for ordinal in 0..roots {
        let native = if ordinal == 0 {
            graph.recipe(Recipe::Scalar(Literal::Bool(false)))?
        } else {
            let mut inputs = vec![
                shared.expect("diamond after root zero"),
                aliases[ordinal - 1],
            ];
            if ordinal > 1 {
                inputs.push(aliases[ordinal - 2]);
            }
            inputs.push(shared.unwrap());
            graph.recipe(Recipe::Vector(inputs))?
        };
        aliases.push(graph.capture_source_root(owner.clone(), native)?);
        if ordinal == 0 {
            // Unaliased diamond: both arms share a native node, which leads
            // to capture zero. Every later root must traverse/dedup this path.
            let stem = graph.recipe(Recipe::Vector(vec![aliases[0]]))?;
            let left = graph.recipe(Recipe::List(vec![stem]))?;
            let right = graph.recipe(Recipe::Vector(vec![stem]))?;
            shared = Some(graph.recipe(Recipe::Vector(vec![left, right]))?);
        }
    }
    Ok(aliases)
}
fn out_of_fuel(error: SessionError) {
    assert!(
        matches!(error, SessionError::Trap(ref error)
        if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
        "{error:?}"
    );
}

#[test]
fn compact_capture_indexes_preserve_zero_holes_growth_and_old_aliases() {
    assert_eq!(
        std::mem::size_of::<Option<NonZeroUsize>>(),
        std::mem::size_of::<usize>()
    );
    assert_eq!(
        std::mem::size_of::<Option<usize>>(),
        2 * std::mem::size_of::<usize>()
    );
    let mut session = Session::new_macro().unwrap();
    let owner = owner(&session);
    let bridge = crate::portable_macro_data::FormBridge::new(&mut session).unwrap();
    session
        .with_operation_budget(|session| {
            let mut graph = AnalysisGraph::new(&bridge, session);
            let mut aliases = Vec::new();
            for ordinal in 0..64 {
                for _ in 0..ordinal % 5 + 1 {
                    graph.recipe(Recipe::Scalar(Literal::Nil))?;
                }
                let native = graph.recipe(Recipe::Scalar(Literal::Bool(ordinal % 2 == 0)))?;
                let alias = graph.capture_source_root(owner.clone(), native)?;
                aliases.push(alias);
                assert_eq!(
                    graph.source_roots.index(graph.session, alias)?,
                    Some(ordinal)
                );
                assert_eq!(graph.source_roots.native(graph.session, alias)?, native);
                assert_eq!(graph.source_roots.index(graph.session, native)?, None);
                // Every growth must preserve capture zero and all old mappings.
                for (id, alias) in aliases.iter().enumerate() {
                    assert_eq!(graph.source_roots.index(graph.session, *alias)?, Some(id));
                }
            }
            assert_eq!(graph.source_roots.index(graph.session, 0)?, None);
            assert_eq!(graph.source_roots.index(graph.session, usize::MAX)?, None);
            assert_eq!(graph.source_roots.by_alias[aliases[0]].unwrap().get(), 1);
            Ok(())
        })
        .unwrap();
}

#[test]
fn boundary_generations_match_full_clears_across_rollovers_and_typed_recovery() {
    let mut session = Session::new_macro().unwrap();
    let owner = owner(&session);
    let bridge = crate::portable_macro_data::FormBridge::new(&mut session).unwrap();
    let mut graph = AnalysisGraph::new(&bridge, &mut session);
    {
        let _request = graph.session.operation_budget_scope().unwrap();
        graph_roots(&mut graph, &owner, 511, 2048).unwrap();
        graph.finish_recipes().unwrap();
    }
    let mut measure = |old: bool| -> Result<(Vec<Vec<usize>>, u64)> {
        let _request = graph.session.operation_budget_scope()?;
        let before = graph.session.operation_fuel_remaining()?;
        let layout = if old {
            graph.source_root_children_fullfill_control()?
        } else {
            graph.source_root_children()?
        };
        Ok((layout, before - graph.session.operation_fuel_remaining()?))
    };
    let (new, current) = measure(false).unwrap();
    let (old, previous) = measure(true).unwrap();
    assert_eq!(
        new, old,
        "exact child order/dedup, including two mark rollovers"
    );
    assert!(new[0].is_empty());
    assert_eq!(new[1], [0]);
    assert_eq!(new[2], [0, 1]);
    for root in 3..511 {
        assert_eq!(new[root], [0, root - 1, root - 2]);
    }
    assert!(
        current < previous,
        "actual work: current={current}, full_clear={previous}"
    );
    // Local closure ends before altering the same Session's request allowance.
    drop(measure);
    for allowance in [0, current / 4, current / 2, current - 1] {
        graph.session.set_operation_fuel(allowance);
        let error = {
            let _request = graph.session.operation_budget_scope().unwrap();
            graph.source_root_children().unwrap_err()
        };
        out_of_fuel(error);
        graph.session.set_operation_fuel(current);
        let _request = graph.session.operation_budget_scope().unwrap();
        assert_eq!(graph.source_root_children().unwrap(), new);
        assert_eq!(graph.session.operation_fuel_remaining().unwrap(), 0);
    }
    graph.session.set_operation_fuel(current);
    let irq = graph.session.host_work_interrupt();
    {
        let _request = graph.session.operation_budget_scope().unwrap();
        irq.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(graph.source_root_children().unwrap_err().is_interrupt());
        assert_eq!(graph.session.operation_fuel_remaining().unwrap(), 0);
    }
    {
        let _request = graph.session.operation_budget_scope().unwrap();
        assert_eq!(graph.source_root_children().unwrap(), new);
        assert_eq!(graph.session.operation_fuel_remaining().unwrap(), 0);
    }
    eprintln!(
        "source-boundary-generations roots=511 padding=2048 optimized={current} full_clear={previous} resets=3 versus=511 typed_fuel_and_irq_recovery=true"
    );
}

#[test]
fn boundary_generations_keep_full_cycle_guard_beneath_captured_aliases() {
    let mut session = Session::new_macro().unwrap();
    let owner = owner(&session);
    let bridge = crate::portable_macro_data::FormBridge::new(&mut session).unwrap();
    session
        .with_operation_budget(|session| {
            let mut graph = AnalysisGraph::new(&bridge, session);
            let target = graph.recipes.len();
            let cyclic = graph.recipe(Recipe::Alias(target))?;
            assert_eq!(cyclic, target);
            let captured = graph.capture_source_root(owner.clone(), cyclic)?;
            let parent = graph.recipe(Recipe::Vector(vec![captured]))?;
            graph.capture_source_root(owner, parent)?;
            let old = graph
                .source_root_children_fullfill_control()
                .unwrap_err()
                .to_string();
            let new = graph.source_root_children().unwrap_err().to_string();
            assert_eq!(old, new);
            assert!(new.contains("Cyclic native source-root graph"), "{new}");
            Ok(())
        })
        .unwrap();
}

// Dormant diamond revisits generation3 at roots2/257/512; missed clears lose child1.
#[test]
fn boundary_rollover_clears_dormant_unaliased_paths_after_byte_reuse() {
    let mut session = Session::new_macro().unwrap();
    let owner = owner(&session);
    let bridge = crate::portable_macro_data::FormBridge::new(&mut session).unwrap();
    let mut graph = AnalysisGraph::new(&bridge, &mut session);
    {
        let _request = graph.session.operation_budget_scope().unwrap();
        for _ in 0..2048 {
            graph.recipe(Recipe::Scalar(Literal::Nil)).unwrap();
        }
        let native = graph.recipe(Recipe::Scalar(Literal::Bool(false))).unwrap();
        let zero = graph.capture_source_root(owner.clone(), native).unwrap();
        let native = graph.recipe(Recipe::Vector(vec![zero])).unwrap();
        let one = graph.capture_source_root(owner.clone(), native).unwrap();
        let stem = graph.recipe(Recipe::Vector(vec![one])).unwrap();
        let left = graph.recipe(Recipe::List(vec![stem])).unwrap();
        let right = graph.recipe(Recipe::Vector(vec![stem])).unwrap();
        let dormant = graph.recipe(Recipe::Vector(vec![left, right])).unwrap();
        let mut aliases = vec![zero, one];
        for root in 2..513 {
            let mut inputs = Vec::new();
            if matches!(root, 2 | 257 | 512) {
                inputs.push(dormant);
            }
            inputs.extend([aliases[root - 1], aliases[root - 2]]);
            let native = graph.recipe(Recipe::Vector(inputs)).unwrap();
            aliases.push(graph.capture_source_root(owner.clone(), native).unwrap());
        }
    }
    let new = {
        let _request = graph.session.operation_budget_scope().unwrap();
        graph.source_root_children().unwrap()
    };
    let old = {
        let _request = graph.session.operation_budget_scope().unwrap();
        graph.source_root_children_fullfill_control().unwrap()
    };
    assert_eq!(
        new, old,
        "all513roots preserve exact full-clear child layout"
    );
    assert!(new[0].is_empty());
    assert_eq!(new[1], [0]);
    assert_eq!(new[2], [1, 0]);
    assert_eq!(new[257], [1, 256, 255]);
    assert_eq!(new[512], [1, 511, 510]);
    for root in 3..513 {
        if !matches!(root, 257 | 512) {
            assert_eq!(new[root], [root - 1, root - 2]);
        }
    }
}
