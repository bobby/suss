use suss_compile::portable::{
    self,
    hir::{ArrayOperation, Expression, Hir, Literal, Type},
    ir,
};
#[test]
fn array_hir_ir_reject_bad_arity_type_and_non_dominating_operands() {
    let value = Hir {
        span: 2..3,
        metadata: vec![],
        ty: Type::Nil,
        kind: Expression::Literal(Literal::Nil),
    };
    for operation in [
        ArrayOperation::Make,
        ArrayOperation::MakeLiteral,
        ArrayOperation::Length,
        ArrayOperation::Get,
        ArrayOperation::Set,
    ] {
        let invalid = Hir {
            span: 0..7,
            metadata: vec![],
            ty: Type::Value,
            kind: Expression::Array {
                operation,
                arguments: vec![],
            },
        };
        assert!(ir::lower(&invalid).is_err(), "{operation:?}");
    }
    let mut hir = Hir {
        span: 0..7,
        metadata: vec![],
        ty: Type::Bool,
        kind: Expression::Array {
            operation: ArrayOperation::Literal,
            arguments: vec![value],
        },
    };
    assert!(ir::lower(&hir).is_err());
    hir.ty = Type::Value;
    let graph = ir::lower(&hir).unwrap();
    portable::compile_ir(&graph).unwrap();
    for malformed in 0..3 {
        let mut graph = graph.clone();
        let instruction = graph.blocks[0].instructions.last_mut().unwrap();
        match malformed {
            0 => graph.values[instruction.result.0].ty = Type::Number,
            1 => {
                if let ir::Operation::Array { operation, .. } = &mut instruction.operation {
                    *operation = ArrayOperation::Set;
                }
            }
            _ => {
                if let ir::Operation::Array { arguments, .. } = &mut instruction.operation {
                    arguments[0] = instruction.result;
                }
            }
        }
        assert!(ir::verify(&graph).is_err());
        assert!(portable::compile_ir(&graph).is_err());
    }
}
