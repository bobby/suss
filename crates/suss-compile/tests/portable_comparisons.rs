use suss_compile::portable::{
    self,
    hir::{Comparison, Expression, Hir, Literal, Type},
    ir,
};
#[test]
fn comparison_hir_ir_guards_require_binary_boolean_and_dominating_operands() {
    let value = Hir {
        source: None,
        span: 2..3,
        metadata: vec![],
        ty: Type::Number,
        kind: std::sync::Arc::new(Expression::Literal(Literal::Number(1.0))),
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
                source: None,
                span: 0..7,
                metadata: vec![],
                ty,
                kind: std::sync::Arc::new(Expression::Comparison {
                    operation,
                    arguments: vec![value.clone(); arity],
                }),
            };
            assert!(ir::lower(&hir).is_err());
        }
        let hir = Hir {
            source: None,
            span: 0..7,
            metadata: vec![],
            ty: Type::Bool,
            kind: std::sync::Arc::new(Expression::Comparison {
                operation,
                arguments: vec![value.clone(), value.clone()],
            }),
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
fn wide_finite_comparison_executes_without_a_fixed_arity_resource_cap() {
    use suss_compile::portable_session::Session;
    for mut session in [Session::new().unwrap(), Session::new_macro().unwrap()] {
        for (values, expected) in [
            ((0..257).map(|n| n.to_string()).collect::<Vec<_>>(), 42),
            (std::iter::repeat_n("1".to_owned(), 257).collect::<Vec<_>>(), 0),
        ] {
            let source = format!("(if (< {}) 42 0)", values.join(" "));
            let result = session.eval(&source).unwrap();
            let number = session.inspect(&result, |store, value| {
                suss_compile::portable_session::runtime_number(store, &value)
            }).unwrap();
            assert_eq!(number, Some(suss_compile::portable_session::RuntimeNumber::Long(expected)));
        }
    }
}
