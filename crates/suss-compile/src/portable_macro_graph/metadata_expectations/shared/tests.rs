use super::*;
use crate::portable_macro_graph::{
    AnalysisGraph,
    raw_keys::{Interner, Plan},
    source_facts::LogicalSourceWitness,
};
fn parsed(s: &str) -> Form {
    suss_reader::forms::read_forms(s).unwrap().pop().unwrap()
}
fn exact(a: &Form, b: &Form) {
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
    fn bits(f: &Form, out: &mut Vec<u64>) {
        for f in &f.metadata {
            bits(f, out);
        }
        match &f.kind {
            Kind::Number(n) => out.push(n.to_bits()),
            Kind::List(v) | Kind::Vector(v) | Kind::Map(v) | Kind::Set(v) => {
                for f in v {
                    bits(f, out);
                }
            }
            _ => {}
        }
    }
    let mut x = Vec::new();
    let mut y = Vec::new();
    bits(a, &mut x);
    bits(b, &mut y);
    assert_eq!(x, y);
}
fn compare(session: &mut Session, recipes: &[Option<Recipe>], root: usize) {
    let old = session.with_operation_budget(|s| project(s, recipes, root));
    let new = session.with_operation_budget(|s| Builder::new(recipes).project(s, root));
    match (old, new) {
        (Ok(Some(old)), Ok(Some(new))) => {
            exact(&old, &syntax::to_form(&new));
            let a = session
                .with_operation_budget(|s| LogicalSourceWitness::from_normalized_source(s, old));
            let b = session.with_operation_budget(|s| LogicalSourceWitness::from_shared(s, new));
            match (a, b) {
                (Ok(a), Ok(b)) => assert_eq!(a.is_some(), b.is_some()),
                (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string()),
                _ => panic!("different logical admission"),
            }
        }
        (Ok(None), Ok(None)) => {}
        (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string()),
        _ => panic!("different projection outcome"),
    }
}
#[test]
fn shared_witness_projects_actual_forest_once_without_changing_any_root_or_export() {
    use crate::portable::{hir::SourceNamespace, resolve::Phase};
    let mut session = Session::new_macro().unwrap();
    session
        .eval("(def coherent-alias-fn (fn* ([x] x) ([x y] y)))")
        .unwrap();
    let snapshot = session.compilation_snapshot();
    let namespace = SourceNamespace::capture(&snapshot.environment, Phase::Macro);
    let (global, revision) = namespace
        .declarations
        .iter()
        .find(|(g, _)| g.name() == "coherent-alias-fn")
        .unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut graph = AnalysisGraph::new(&bridge, &mut session);
    graph.capture_roots = true;
    let (root, mut roots) = {
        let _request = graph.session.operation_budget_scope().unwrap();
        let decl = graph.declaration(global, Some(revision), 0).unwrap();
        let root = graph.recipe(Recipe::Vector(vec![decl])).unwrap();
        graph.finish_recipes().unwrap();
        (
            root,
            graph
                .source_roots
                .roots
                .iter()
                .map(|c| c.native)
                .collect::<Vec<_>>(),
        )
    };
    assert_eq!(roots.len(), 44);
    roots.push(root);
    let mut old = Vec::new();
    let mut old_cost = 0;
    for id in &roots {
        let (form, cost) = graph
            .session
            .with_operation_budget(|s| {
                let before = s.operation_fuel_remaining()?;
                let v = project(s, &graph.recipes, *id)?;
                Ok((v.unwrap(), before - s.operation_fuel_remaining()?))
            })
            .unwrap();
        old.push(form);
        old_cost += cost;
    }
    // ONE shared native operation, including all45roots, not45replenished ones.
    let (new, new_cost) = graph
        .session
        .with_operation_budget(|s| {
            let before = s.operation_fuel_remaining()?;
            let mut builder = Builder::new(&graph.recipes);
            let mut values = Vec::new();
            reserve(s, &mut values, roots.len())?;
            for id in &roots {
                let value = builder.project(s, *id)?.unwrap();
                push(s, &mut values, value)?;
            }
            Ok((values, before - s.operation_fuel_remaining()?))
        })
        .unwrap();
    for (old, new) in old.iter().zip(&new) {
        exact(old, &syntax::to_form(new));
    }
    assert!(new_cost < old_cost);
    println!(
        "shared-witness roots={} old_separate_native={old_cost} shared_single_native={new_cost}",
        roots.len()
    );
}
#[test]
fn shared_witness_preserves_declines_errors_metadata_replacement_and_scalar_bits() {
    let mut s = Session::new().unwrap();
    for (recipes, root) in [
        (
            vec![
                Some(Recipe::Scalar(Literal::Undefined)),
                Some(Recipe::Vector(vec![0, 2])),
                None,
            ],
            1,
        ),
        (
            vec![
                Some(Recipe::Scalar(Literal::Undefined)),
                Some(Recipe::Vector(vec![0, 2])),
                Some(Recipe::Alias(1)),
            ],
            1,
        ),
        (vec![Some(Recipe::Alias(99))], 0),
        (vec![Some(Recipe::Alias(0))], 0),
        (
            vec![
                Some(Recipe::Scalar(Literal::Nil)),
                Some(Recipe::Set(vec![0, 0])),
            ],
            1,
        ),
        (
            vec![
                Some(Recipe::Scalar(Literal::Number(0.0))),
                Some(Recipe::Scalar(Literal::Number(-0.0))),
                Some(Recipe::Set(vec![0, 1])),
            ],
            2,
        ),
        (
            vec![Some(Recipe::Form(parsed(
                "^:outer [^{:nested true} {:x [1 2]}]",
            )))],
            0,
        ),
        (
            vec![
                Some(Recipe::Form(parsed("^:old [1 2]"))),
                Some(Recipe::Form(parsed("^:on-meta {:m 17}"))),
                Some(Recipe::Metadata {
                    shape: MetadataShape::Vector,
                    value: 0,
                    metadata: 1,
                    set_items: vec![],
                }),
            ],
            2,
        ),
        (
            vec![
                Some(Recipe::Form(parsed("^:old [1 2]"))),
                Some(Recipe::Scalar(Literal::Nil)),
                Some(Recipe::Metadata {
                    shape: MetadataShape::Vector,
                    value: 0,
                    metadata: 1,
                    set_items: vec![],
                }),
            ],
            2,
        ),
        (
            vec![
                Some(Recipe::Scalar(Literal::Undefined)),
                Some(Recipe::Scalar(Literal::Bool(true))),
                Some(Recipe::Metadata {
                    shape: MetadataShape::Vector,
                    value: 0,
                    metadata: 1,
                    set_items: vec![],
                }),
            ],
            2,
        ),
        (
            vec![
                Some(Recipe::Form(parsed("{:duplicate 1 :duplicate 2}"))),
                Some(Recipe::Scalar(Literal::Nil)),
                Some(Recipe::Metadata {
                    shape: MetadataShape::Vector,
                    value: 0,
                    metadata: 1,
                    set_items: vec![],
                }),
            ],
            2,
        ),
        (
            vec![
                Some(Recipe::Scalar(Literal::Nil)),
                Some(Recipe::Form(parsed("{:m 17}"))),
                Some(Recipe::Scalar(Literal::Number(-0.0))),
                Some(Recipe::Metadata {
                    shape: MetadataShape::Set,
                    value: 0,
                    metadata: 1,
                    set_items: vec![2, 2],
                }),
            ],
            3,
        ),
        (
            vec![
                Some(Recipe::Scalar(Literal::Number(f64::from_bits(
                    0x7ff8000000001234,
                )))),
                Some(Recipe::Scalar(Literal::String(vec![0xd800, 0, 0xdc00]))),
                Some(Recipe::Vector(vec![0, 1])),
            ],
            2,
        ),
    ] {
        compare(&mut s, &recipes, root);
    }
}
#[test]
fn shared_witness_reuses_only_exact_ids_and_overlays_never_mutate_a_base() {
    let recipes = vec![
        Some(Recipe::Scalar(Literal::Nil)),
        Some(Recipe::Scalar(Literal::Nil)),
        Some(Recipe::Vector(vec![0, 0, 1])),
        Some(Recipe::Form(parsed("{:m 17}"))),
        Some(Recipe::Metadata {
            shape: MetadataShape::Vector,
            value: 2,
            metadata: 3,
            set_items: vec![],
        }),
    ];
    let mut s = Session::new().unwrap();
    s.with_operation_budget(|s| {
        let mut b = Builder::new(&recipes);
        let base = b.project(s, 2)?.unwrap();
        let overlay = b.project(s, 4)?.unwrap();
        let again = b.project(s, 2)?.unwrap();
        assert!(Rc::ptr_eq(&base, &again));
        assert!(Rc::ptr_eq(base.body(), overlay.body()));
        let Body::Sequence(_, v) = &**base.body() else {
            panic!()
        };
        assert!(Rc::ptr_eq(&v[0], &v[1]));
        assert!(!Rc::ptr_eq(&v[0], &v[2]));
        assert!(syntax::to_form(&base).metadata.is_empty());
        assert_eq!(syntax::to_form(&overlay).metadata.len(), 1);
        let other = Builder::new(&recipes).project(s, 2)?.unwrap();
        assert!(!Rc::ptr_eq(&base, &other));
        Ok(())
    })
    .unwrap();
}
fn reference(s: &mut Session, a: &Form, b: &Form) -> Result<bool> {
    let mut keys = Interner::default();
    let mut plan = Plan::default();
    let a = keys.unordered_plan(a, &mut plan, &mut |n| s.charge_host_work(n))?;
    let b = keys.unordered_plan(b, &mut plan, &mut |n| s.charge_host_work(n))?;
    Ok(plan.nodes[a].key == plan.nodes[b].key)
}
#[test]
fn shared_witness_small_and_wide_unordered_comparisons_match_independent_plans() {
    let mut s = Session::new().unwrap();
    for count in [2, 17, 33] {
        let text = format!(
            "^:outer {{{}}}",
            (0..count)
                .map(|i| format!(":k{i} ^:value [{i} -0.0] "))
                .collect::<String>()
        );
        let recipes = vec![Some(Recipe::Form(parsed(&text)))];
        let expected = s
            .with_operation_budget(|s| project(s, &recipes, 0))
            .unwrap()
            .unwrap();
        let node = s
            .with_operation_budget(|s| Builder::new(&recipes).project(s, 0))
            .unwrap()
            .unwrap();
        assert_eq!(node.small(), count <= 16);
        let mut actual = expected.clone();
        if let Kind::Map(v) = &mut actual.kind {
            let pairs = v.chunks_exact(2).map(|p| p.to_vec()).collect::<Vec<_>>();
            *v = pairs.into_iter().rev().flatten().collect();
        }
        for mutate in [false, true] {
            if mutate {
                actual.metadata.clear();
            }
            let old = s
                .with_operation_budget(|s| reference(s, &expected, &actual))
                .unwrap();
            let new = s
                .with_operation_budget(|s| syntax::matches(s, &node, &actual))
                .unwrap();
            assert_eq!(old, new);
            assert_eq!(new, !mutate);
        }
    }
}
#[test]
fn shared_witness_budget_and_irq_errors_do_not_cache_failures_and_retry_keeps_prior_roots() {
    let recipes = vec![
        Some(Recipe::Form(parsed("^{:old [1 2]} [:x :y]"))),
        Some(Recipe::Vector(vec![0, 0])),
    ];
    let mut s = Session::new().unwrap();
    let mut b = Builder::new(&recipes);
    let prior = s
        .with_operation_budget(|s| b.project(s, 0))
        .unwrap()
        .unwrap();
    let before = syntax::to_form(&prior);
    let cost = s
        .with_operation_budget(|s| {
            let start = s.operation_fuel_remaining()?;
            Builder::new(&recipes).project(s, 1)?;
            Ok(start - s.operation_fuel_remaining()?)
        })
        .unwrap();
    for cut in [0, 1, 7, cost / 2, cost - 1] {
        s.set_operation_fuel(cut);
        let e = s.with_operation_budget(|s| b.project(s, 1)).err().unwrap();
        assert!(
            matches!(e,SessionError::Trap(ref e) if e.downcast_ref::<wasmtime::Trap>()==Some(&wasmtime::Trap::OutOfFuel))
        );
        s.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        let retry = s
            .with_operation_budget(|s| b.project(s, 1))
            .unwrap()
            .unwrap();
        exact(&before, &syntax::to_form(&prior));
        assert!(retry.admitted());
        b = Builder::new(&recipes);
    }
    let irq = s.host_work_interrupt();
    let e = s
        .with_operation_budget(|s| {
            irq.store(true, std::sync::atomic::Ordering::SeqCst);
            b.project(s, 1)
        })
        .err()
        .unwrap();
    assert!(e.is_interrupt());
    assert!(
        s.with_operation_budget(|s| b.project(s, 1))
            .unwrap()
            .unwrap()
            .admitted()
    );
}
