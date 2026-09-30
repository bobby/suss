//! Public HIR/IR exception guards; source/GC semantics execute in native session tests.
use suss_compile::portable::{
    self,
    hir::{self, Expression, Hir, Literal, Type},
    ir::{self, Operation, Terminator, ValueId},
};

#[test]
fn handler_regions_require_fixed_shapes_and_throw_edges_require_defined_values() {
    let hir =
        portable::analyze("(try (throw 7) (catch :default error error) (finally 9))").unwrap();
    let graph = ir::lower(&hir).unwrap();
    ir::verify(&graph).unwrap();
    portable::compile_ir(&graph).unwrap();
    let mut malformed = graph.clone();
    let instruction = malformed
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find(|instruction| matches!(instruction.operation, Operation::Try { .. }))
        .unwrap();
    let Operation::Try { regions } = &mut instruction.operation else {
        unreachable!()
    };
    regions[1] = regions[0];
    assert!(ir::verify(&malformed).is_err());
    assert!(portable::compile_ir(&malformed).is_err());

    let hir = portable::analyze("(throw 7)").unwrap();
    let mut malformed = ir::lower(&hir).unwrap();
    malformed.blocks[0].terminator = Terminator::Throw(ValueId(usize::MAX));
    assert!(ir::verify(&malformed).is_err());
    assert!(portable::compile_ir(&malformed).is_err());

    let nil = Hir {
        span: 1..5,
        metadata: vec![],
        ty: Type::Nil,
        kind: Expression::Literal(Literal::Nil),
    };
    let malformed = Hir {
        span: 0..9,
        metadata: vec![],
        ty: Type::Value,
        kind: Expression::Try {
            regions: [Box::new(nil.clone()), Box::new(nil.clone()), Box::new(nil)],
        },
    };
    assert!(ir::lower(&malformed).is_err());
}

#[test]
fn public_hir_cannot_recur_across_a_handler_region() {
    let mut hir = portable::analyze("(loop [x 0] (try 42 (finally 7)))").unwrap();
    let Expression::Do(items) = &mut hir.kind else {
        panic!("source fragment")
    };
    let Expression::Loop { target, body, .. } = &mut items[0].kind else {
        panic!("loop")
    };
    let target = *target;
    let Expression::Do(items) = &mut body.kind else {
        panic!("loop body")
    };
    let Expression::Try { regions } = &mut items[0].kind else {
        panic!("try")
    };
    let Expression::Function { body, .. } = &mut regions[2].kind else {
        panic!("cleanup")
    };
    body.kind = Expression::Recur {
        target,
        arguments: vec![Hir {
            span: 1..5,
            metadata: vec![],
            ty: Type::Number,
            kind: Expression::Literal(Literal::Number(1.0)),
        }],
    };
    body.ty = hir::Type::Value;
    assert!(ir::lower(&hir).is_err());
}
