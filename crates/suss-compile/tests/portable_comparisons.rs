use suss_compile::portable::{
    self,
    hir::{Comparison, Expression, Hir, Literal, Type},
    ir,
};
#[test]
fn comparison_hir_ir_guards_require_binary_boolean_and_dominating_operands() {
    let value = Hir {
        span: 2..3,
        metadata: vec![],
        ty: Type::Number,
        kind: Expression::Literal(Literal::Number(1.0)),
    };
    for operation in [
        Comparison::Less,
        Comparison::LessEqual,
        Comparison::Greater,
        Comparison::GreaterEqual,
        Comparison::StrictEqual,
    ] {
        for (arity, ty) in [
            (0, Type::Bool),
            (1, Type::Bool),
            (3, Type::Bool),
            (2, Type::Number),
        ] {
            let hir = Hir {
                span: 0..7,
                metadata: vec![],
                ty,
                kind: Expression::Comparison {
                    operation,
                    arguments: vec![value.clone(); arity],
                },
            };
            assert!(ir::lower(&hir).is_err());
        }
        let hir = Hir {
            span: 0..7,
            metadata: vec![],
            ty: Type::Bool,
            kind: Expression::Comparison {
                operation,
                arguments: vec![value.clone(), value.clone()],
            },
        };
        let graph = ir::lower(&hir).unwrap();
        portable::compile_ir(&graph).unwrap();
        for malformed in 0..3 {
            let mut graph = graph.clone();
            let instruction = graph.blocks[0].instructions.last_mut().unwrap();
            match malformed {
                0 => graph.values[instruction.result.0].ty = Type::Number,
                1 => {
                    if let ir::Operation::Comparison { arguments, .. } = &mut instruction.operation
                    {
                        arguments.pop();
                    }
                }
                _ => {
                    if let ir::Operation::Comparison { arguments, .. } = &mut instruction.operation
                    {
                        arguments[0] = instruction.result;
                    }
                }
            }
            assert!(ir::verify(&graph).is_err());
            assert!(portable::compile_ir(&graph).is_err());
        }
    }
}

#[test]
fn comparison_macro_resource_limit_is_located_before_recursive_expansion() {
    let source = format!(
        "(< {})",
        std::iter::repeat_n("1", 257).collect::<Vec<_>>().join(" ")
    );
    let error = portable::compile(&source).unwrap_err();
    assert!(error.message.contains("expansion limit"));
    assert_eq!(error.span, 0..source.len());
}
