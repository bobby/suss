//! Public dynamic HIR/IR guards, independent from native source/runtime execution.
use suss_compile::portable::{
    self,
    hir::{Expression, Hir, Literal, Type},
    ir::{self, Operation},
    resolve::{Environment, Phase},
};
#[test]
fn dynamic_scope_requires_a_fixed_body_and_complete_snapshot_value_pairs() {
    let e = Environment::default();
    let fragment = portable::prepare_fragment("(def ^:dynamic *x* 1)", &e, Phase::Runtime).unwrap();
    let hir = portable::analyze_in(
        "(binding [*x* 7] *x*)",
        &fragment.environment,
        Phase::Runtime,
    )
    .unwrap();
    let graph = ir::lower(&hir).unwrap();
    ir::verify(&graph).unwrap();
    portable::compile_ir(&graph).unwrap();
    let mut malformed = graph.clone();
    let inst = malformed
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.operation, Operation::DynamicScope { .. }))
        .unwrap();
    let Operation::DynamicScope { operands, .. } = &mut inst.operation else {
        unreachable!()
    };
    operands.pop();
    assert!(ir::verify(&malformed).is_err());
    assert!(portable::compile_ir(&malformed).is_err());
    let mut malformed = hir.clone();
    let Expression::Do(items) = &mut malformed.kind else {
        panic!("fragment")
    };
    let Expression::DynamicScope { body, .. } = &mut items[0].kind else {
        panic!("scope")
    };
    **body = Hir {
        span: 1..5,
        metadata: vec![],
        ty: Type::Nil,
        kind: Expression::Literal(Literal::Nil),
    };
    assert!(ir::lower(&malformed).is_err());
}
#[test]
fn dynamic_targets_preserve_namespace_aliases_phase_and_lexical_shadowing() {
    let mut e = Environment::default();
    e.enter_namespace(Phase::Runtime, "config").unwrap();
    let fragment = portable::prepare_fragment("(def ^:dynamic *x* 1)", &e, Phase::Runtime).unwrap();
    let mut e = fragment.environment;
    e.enter_namespace(Phase::Runtime, "user").unwrap();
    e.alias(Phase::Runtime, "cfg", "config").unwrap();
    portable::compile_in("(binding [cfg/*x* 7] config/*x*)", &e, Phase::Runtime).unwrap();
    assert!(portable::compile_in("(binding [cfg/*x* 7] config/*x*)", &e, Phase::Macro).is_err());
    portable::compile_in("(let [binding (fn [x] x)] (binding 7))", &e, Phase::Runtime).unwrap();
}
