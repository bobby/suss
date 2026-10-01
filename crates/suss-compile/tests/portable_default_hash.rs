use suss_compile::portable::{
    self,
    hir::{Expression, Hir, Literal, Nominal, Type},
};

#[test]
fn default_prototype_adapter_rejects_invalid_public_hir_and_ir() {
    let make = |count, ty| Hir {
        span: 3..23,
        metadata: vec![],
        ty,
        kind: Expression::Nominal {
            operation: Nominal::NativeObjectDefaultPrototype,
            arguments: (0..count)
                .map(|_| Hir {
                    span: 5..6,
                    metadata: vec![],
                    ty: Type::Number,
                    kind: Expression::Literal(Literal::Number(1.0)),
                })
                .collect(),
        },
    };
    for (count, ty) in [(1, Type::Value), (0, Type::Bool)] {
        assert_eq!(portable::ir::lower(&make(count, ty)).unwrap_err().span, 3..23);
    }
    let mut ir = portable::ir::lower(&make(0, Type::Value)).unwrap();
    portable::compile_ir(&ir).unwrap();
    let result = ir.blocks[0].instructions.last().unwrap().result;
    ir.values[result.0].ty = Type::Bool;
    assert!(portable::compile_ir(&ir).is_err());
}
