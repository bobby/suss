//! Native differential controls; the preserved algorithm is test-only.
use super::*;
use crate::portable_macro_graph::AnalysisGraph;

type Projector = fn(&mut Session, &[Option<Recipe>], Value) -> Result<Option<Form>>;
fn number_bits(form: &Form) -> Vec<u64> {
    let mut pending = vec![form];
    let mut bits = Vec::new();
    while let Some(form) = pending.pop() {
        pending.extend(form.metadata.iter().rev());
        match &form.kind {
            Kind::Number(n) => bits.push(n.to_bits()),
            Kind::List(v) | Kind::Vector(v) | Kind::Map(v) | Kind::Set(v)
            | Kind::Conditional(v) | Kind::SplicingConditional(v) => pending.extend(v.iter().rev()),
            Kind::Discard(v) => pending.push(v),
            Kind::Prefix { operator, target } => { pending.push(target); pending.push(operator); }
            _ => {}
        }
    }
    bits
}
fn exact(a: &Option<Form>, b: &Option<Form>) {
    // Debug includes every span, metadata edge, kind, UTF16 unit and identifier
    // field; separately compare every number's bits (including NaN payloads).
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
    if let (Some(a), Some(b)) = (a, b) { assert_eq!(number_bits(a), number_bits(b)); }
}
fn measured(session: &mut Session, recipes: &[Option<Recipe>], root: Value, f: Projector)
    -> (Option<Form>, u64)
{
    session.with_operation_budget(|session| {
        let before = session.operation_fuel_remaining()?;
        let value = f(session, recipes, root)?;
        Ok((value, before - session.operation_fuel_remaining()?))
    }).unwrap()
}
fn parsed(text: &str) -> Form {
    suss_reader::forms::read_forms(text).unwrap().pop().unwrap()
}
#[test]
fn private_project_actual_44_source_roots_and_export_match_preserved_algorithm() {
    use crate::portable::{hir::SourceNamespace, resolve::Phase};
    let mut session = Session::new_macro().unwrap();
    session.eval("(def coherent-alias-fn (fn* ([x] x) ([x y] y)))").unwrap();
    let snapshot = session.compilation_snapshot();
    let namespace = SourceNamespace::capture(&snapshot.environment, Phase::Macro);
    let (global, revision) = namespace.declarations.iter()
        .find(|(g, _)| g.name() == "coherent-alias-fn").unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut graph = AnalysisGraph::new(&bridge, &mut session);
    graph.capture_roots = true;
    let (root, roots) = {
        let _request = graph.session.operation_budget_scope().unwrap();
        let declaration = graph.declaration(global, Some(revision), 0).unwrap();
        let root = graph.recipe(Recipe::Vector(vec![declaration])).unwrap();
        graph.finish_recipes().unwrap();
        let roots: Vec<_> = graph.source_roots.roots.iter().map(|c| c.native).collect();
        (root, roots)
    };
    assert_eq!(roots.len(), 44, "same actual alias-diagnostic source closure");
    let mut total_old = 0;
    let mut total_new = 0;
    for (ordinal, id) in roots.into_iter().chain(std::iter::once(root)).enumerate() {
        // Independent pure-native comparisons, each under its default operation
        // budget. No guest construction, compiler request allowance or callback.
        let (old, old_cost) = measured(graph.session, &graph.recipes, id, project_legacy::project);
        let (new, new_cost) = measured(graph.session, &graph.recipes, id, project);
        exact(&old, &new);
        assert!(old.is_some());
        println!("private-project actual ordinal={ordinal} native={id} old={old_cost} new={new_cost}");
        total_old += old_cost;
        total_new += new_cost;
    }
    assert!(total_new < total_old, "actual removed work, not a tariff reduction");
}
#[test]
fn private_project_full_forms_declines_and_malformed_siblings_match_legacy() {
    let mut session = Session::new().unwrap();
    let cases: Vec<(Vec<Option<Recipe>>, usize)> = vec![
        (vec![Some(Recipe::Scalar(Literal::Undefined)), Some(Recipe::Vector(vec![0, 0]))], 1),
        (vec![Some(Recipe::Scalar(Literal::Undefined)), Some(Recipe::Vector(vec![0, 2])), None], 1),
        (vec![Some(Recipe::Scalar(Literal::Undefined)), Some(Recipe::Vector(vec![0, 2])), Some(Recipe::Alias(1))], 1),
        (vec![Some(Recipe::Alias(0))], 0),
        (vec![Some(Recipe::Alias(99))], 0),
        (vec![Some(Recipe::Scalar(Literal::Bool(true))), Some(Recipe::Scalar(Literal::Undefined)),
            Some(Recipe::Metadata { shape: MetadataShape::Vector, value: 0, metadata: 1, set_items: vec![] })], 2),
        (vec![Some(Recipe::Form(Form { span: 7..19, metadata: vec![],
            kind: Kind::Map(vec![parsed(":unpaired")]) }))], 0),
        (vec![Some(Recipe::Scalar(Literal::Undefined)), Some(Recipe::Scalar(Literal::Nil)),
            Some(Recipe::Metadata { shape: MetadataShape::Vector, value: 1, metadata: 1, set_items: vec![0] })], 2),
        (vec![Some(Recipe::Scalar(Literal::Undefined)), Some(Recipe::Scalar(Literal::Bool(true))),
            Some(Recipe::Metadata { shape: MetadataShape::Vector, value: 0, metadata: 1, set_items: vec![] })], 2),
        (vec![Some(Recipe::Form(parsed("^:old [1 2]"))), Some(Recipe::Form(parsed("^:on-meta {:m 17}"))),
            Some(Recipe::Metadata { shape: MetadataShape::Vector, value: 0, metadata: 1, set_items: vec![] })], 2),
        (vec![Some(Recipe::Scalar(Literal::Nil)), Some(Recipe::Form(parsed("{:m 17}"))),
            Some(Recipe::Scalar(Literal::Number(-0.0))),
            Some(Recipe::Metadata { shape: MetadataShape::Set, value: 0, metadata: 1, set_items: vec![2, 2] })], 3),
        (vec![Some(Recipe::Scalar(Literal::Number(f64::from_bits(0x7ff8_0000_0000_1234)))),
            Some(Recipe::Scalar(Literal::Number(-0.0))), Some(Recipe::Scalar(Literal::String(vec![0xd800, 0, 0xdc00]))),
            Some(Recipe::Vector(vec![0, 1, 2])), Some(Recipe::Alias(3)), Some(Recipe::ReaderMap(vec![(4, 3)]))], 5),
        (vec![Some(Recipe::Form(parsed("^:outer [^{:nested true} {:x [1 2]}]")))], 0),
    ];
    for (ordinal, (recipes, root)) in cases.iter().enumerate() {
        let old = session.with_operation_budget(|s| project_legacy::project(s, recipes, *root));
        let new = session.with_operation_budget(|s| project(s, recipes, *root));
        match (old, new) {
            (Ok(a), Ok(b)) => exact(&a, &b),
            (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string(), "case {ordinal}"),
            (a, b) => panic!("case {ordinal}: legacy={a:?}, current={b:?}"),
        }
    }
}
#[test]
fn private_project_fuel_irq_cuts_retry_and_repeated_aliases_remain_bounded() {
    let recipes = vec![Some(Recipe::Scalar(Literal::Number(-0.0))),
        Some(Recipe::Alias(0)), Some(Recipe::Vector(vec![1, 1]))];
    let mut session = Session::new().unwrap();
    for f in [project as Projector, project_legacy::project as Projector] {
        let (expected, cost) = measured(&mut session, &recipes, 2, f);
        for cut in 0..cost {
            session.set_operation_fuel(cut);
            let err = session.with_operation_budget(|s| f(s, &recipes, 2)).unwrap_err();
            assert!(matches!(err, SessionError::Trap(ref e)
                if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{err:?}");
        }
        session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        let interrupt = session.host_work_interrupt();
        let err = session.with_operation_budget(|s| {
            interrupt.store(true, std::sync::atomic::Ordering::SeqCst);
            f(s, &recipes, 2)
        }).unwrap_err();
        assert!(err.is_interrupt());
        let (retry, _) = measured(&mut session, &recipes, 2, f);
        exact(&expected, &retry);
    }
    let mut diamond = vec![Some(Recipe::Scalar(Literal::Nil))];
    for index in 0..20 { diamond.push(Some(Recipe::Vector(vec![index, index]))); }
    for f in [project as Projector, project_legacy::project as Projector] {
        let err = session.with_operation_budget(|s| f(s, &diamond, 20)).unwrap_err();
        assert!(matches!(err, SessionError::Trap(ref e)
            if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{err:?}");
    }
}
