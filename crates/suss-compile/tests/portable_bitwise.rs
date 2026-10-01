use suss_compile::portable::{
    self,
    hir::{Bitwise, Expression, Hir, Literal, Type},
    ir::Operation,
};

#[test]
fn public_bitwise_hir_and_ir_reject_bad_arity_and_result_before_emission() {
    let make = |count, ty| Hir {
        span: 4..19,
        metadata: vec![],
        ty,
        kind: Expression::Bitwise {
            operation: Bitwise::And,
            arguments: (0..count)
                .map(|_| Hir {
                    span: 7..8,
                    metadata: vec![],
                    ty: Type::Number,
                    kind: Expression::Literal(Literal::Number(7.0)),
                })
                .collect(),
        },
    };
    for (count, ty) in [
        (0, Type::Number),
        (1, Type::Number),
        (3, Type::Number),
        (2, Type::Bool),
    ] {
        let error = portable::ir::lower(&make(count, ty)).unwrap_err();
        assert_eq!(error.span, 4..19);
    }
    let valid = portable::ir::lower(&make(2, Type::Number)).unwrap();
    portable::compile_ir(&valid).unwrap();
    let mut bad = valid.clone();
    let inst = bad.blocks[0].instructions.last_mut().unwrap();
    let Operation::Bitwise { arguments, .. } = &mut inst.operation else {
        panic!("expected primitive");
    };
    arguments.pop();
    assert!(portable::compile_ir(&bad)
        .unwrap_err()
        .message
        .contains("bitwise"));
    let mut bad = valid;
    let result = bad.blocks[0].instructions.last().unwrap().result;
    bad.values[result.0].ty = Type::Bool;
    assert!(portable::compile_ir(&bad)
        .unwrap_err()
        .message
        .contains("bitwise"));
}
