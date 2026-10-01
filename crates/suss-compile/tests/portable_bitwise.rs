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

#[test]
fn private_binary64_storage_hir_and_ir_check_unary_result_contract() {
    for operation in [Bitwise::F64Coerce, Bitwise::F64Word0, Bitwise::F64Word4] {
        let make = |count, ty| Hir {
            span: 3..21, metadata: vec![], ty,
            kind: Expression::Bitwise { operation, arguments: (0..count).map(|_| Hir {
                span: 5..6, metadata: vec![], ty: Type::Number,
                kind: Expression::Literal(Literal::Number(-0.0)),
            }).collect() },
        };
        for (count, ty) in [(0, Type::Number), (2, Type::Number), (1, Type::Bool)] {
            assert_eq!(portable::ir::lower(&make(count, ty)).unwrap_err().span, 3..21);
        }
        let valid = portable::ir::lower(&make(1, Type::Number)).unwrap();
        portable::compile_ir(&valid).unwrap();
        let mut bad = valid.clone();
        let inst = bad.blocks[0].instructions.last_mut().unwrap();
        let Operation::Bitwise { arguments, .. } = &mut inst.operation else { panic!("primitive"); };
        arguments.clear();
        assert!(portable::compile_ir(&bad).is_err());
        let mut bad = valid;
        let result = bad.blocks[0].instructions.last().unwrap().result;
        bad.values[result.0].ty = Type::Bool;
        assert!(portable::compile_ir(&bad).is_err());
    }
}

#[test]
fn numeric_hash_boundary_hir_ir_preserve_boolean_and_number_contracts() {
    for (operation, arity, ty) in [
        (Bitwise::F64Floor, 1, Type::Number),
        (Bitwise::F64Finite, 1, Type::Bool),
        (Bitwise::F64SafeInteger, 1, Type::Bool),
        (Bitwise::SafeIntegerRemainder, 2, Type::Number),
    ] {
        let make = |count, ty| Hir {
            span: 3..21, metadata: vec![], ty,
            kind: Expression::Bitwise { operation, arguments: (0..count).map(|_| Hir {
                span: 5..6, metadata: vec![], ty: Type::Number,
                kind: Expression::Literal(Literal::Number(3.0)),
            }).collect() },
        };
        let wrong = if ty == Type::Bool { Type::Number } else { Type::Bool };
        for (count, result) in [(0, ty), (arity + 1, ty), (arity, wrong)] {
            assert_eq!(portable::ir::lower(&make(count, result)).unwrap_err().span, 3..21);
        }
        let valid = portable::ir::lower(&make(arity, ty)).unwrap();
        portable::compile_ir(&valid).unwrap();
        let mut bad = valid.clone();
        let Operation::Bitwise { arguments, .. } = &mut bad.blocks[0].instructions.last_mut().unwrap().operation else { panic!("primitive"); };
        arguments.clear();
        assert!(portable::compile_ir(&bad).is_err());
        let mut bad = valid;
        let result = bad.blocks[0].instructions.last().unwrap().result;
        bad.values[result.0].ty = wrong;
        assert!(portable::compile_ir(&bad).is_err());
    }
}
