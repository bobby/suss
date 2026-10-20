use suss_compile::portable::{
    self,
    hir::{Bitwise, Expression, Hir, Literal, Type},
    ir::Operation,
};

#[test]
fn cached_integer_bits_keep_escaping_box_and_remove_only_repeated_coercions() {
    let number = |value| Hir {
        source: None, span: 0..1, metadata: vec![], ty: Type::Number,
        kind: Expression::Literal(Literal::Number(value)),
    };
    let inner = Hir {
        source: None, span: 0..1, metadata: vec![], ty: Type::Number,
        kind: Expression::Bitwise { operation: Bitwise::Int, arguments: vec![number(7.0)] },
    };
    let mut ir = portable::ir::lower(&Hir {
        source: None, span: 0..1, metadata: vec![], ty: Type::Number,
        kind: Expression::Bitwise { operation: Bitwise::Xor, arguments: vec![inner, number(3.0)] },
    }).unwrap();
    let producer = ir.blocks[0].instructions.iter().find(|instruction|
        matches!(instruction.operation, Operation::Bitwise { operation: Bitwise::Int, .. }))
        .unwrap().result;
    for instruction in &mut ir.blocks[0].instructions {
        if let Operation::Bitwise { operation: Bitwise::Xor, arguments } = &mut instruction.operation {
            *arguments = vec![producer, producer];
        }
    }
    // Insert a real zero-argument closure call after the producer, before its
    // cached consumers. The body returns nil, adding no numeric box/coercion.
    let mut closure_ir = portable::ir::lower(&Hir {
        source: None, span: 0..1, metadata: vec![], ty: Type::Closure(0),
        kind: Expression::Function {
            parameters: vec![], captures: vec![],
            body: Box::new(Hir {
                source: None, span: 0..1, metadata: vec![], ty: Type::Nil,
                kind: Expression::Literal(Literal::Nil),
            }),
        },
    }).unwrap();
    let closure_operation = closure_ir.blocks[0].instructions.remove(0).operation;
    assert!(matches!(&closure_operation, Operation::MakeClosure { .. }));
    let closure = portable::ir::ValueId(ir.values.len());
    ir.values.push(portable::ir::Value { ty: Type::Closure(0), span: 0..1 });
    let called = portable::ir::ValueId(ir.values.len());
    ir.values.push(portable::ir::Value { ty: Type::Value, span: 0..1 });
    let after_producer = ir.blocks[0].instructions.iter()
        .position(|instruction| instruction.result == producer).unwrap() + 1;
    ir.blocks[0].instructions.splice(after_producer..after_producer, [
        portable::ir::Instruction { result: closure, operation: closure_operation, span: 0..1 },
        portable::ir::Instruction { result: called,
            operation: Operation::Call { operands: vec![closure] }, span: 0..1 },
    ]);
    // The original box also escapes through the return, independently of the
    // repeated integer uses. It must still be allocated at the producer.
    ir.blocks[0].terminator = portable::ir::Terminator::Return(producer);
    let wasm = portable::compile_ir(&ir).unwrap();
    let mut names = Vec::new();
    let mut boxes = 0;
    let mut coercions = 0;
    let mut invokes = 0;
    for payload in wasmparser::Parser::new(0).parse_all(&wasm) {
        match payload.unwrap() {
            wasmparser::Payload::ImportSection(imports) => {
                for import in imports.into_imports() {
                    let import = import.unwrap();
                    if matches!(import.ty, wasmparser::TypeRef::Func(_)) { names.push(import.name.to_owned()); }
                }
            }
            wasmparser::Payload::CodeSectionEntry(body) => {
                for operator in body.get_operators_reader().unwrap() {
                    if let wasmparser::Operator::Call { function_index } = operator.unwrap() {
                        match names.get(function_index as usize).map(String::as_str) {
                            Some("number-box") => boxes += 1,
                            Some("coerce-int32") => coercions += 1,
                            Some("invoke") => invokes += 1,
                            _ => {},
                        }
                    }
                }
            }
            _ => {},
        }
    }
    assert_eq!(invokes, 1, "intervening real call remains emitted");
    assert_eq!(boxes, 4, "two literals, retained producer and final xor box");
    assert_eq!(coercions, 1, "producer conversion remains; both repeated uses reuse bits");
}

#[test]
fn integer_chain_emission_omits_the_unobserved_intermediate_number_box() {
    let number = |value| Hir {
        source: None, span: 0..1, metadata: vec![], ty: Type::Number,
        kind: Expression::Literal(Literal::Number(value)),
    };
    let inner = Hir {
        source: None, span: 0..1, metadata: vec![], ty: Type::Number,
        kind: Expression::Bitwise { operation: Bitwise::Int, arguments: vec![number(7.0)] },
    };
    let chain = Hir {
        source: None, span: 0..1, metadata: vec![], ty: Type::Number,
        kind: Expression::Bitwise { operation: Bitwise::Xor, arguments: vec![inner, number(3.0)] },
    };
    let wasm = portable::compile_ir(&portable::ir::lower(&chain).unwrap()).unwrap();
    let mut function_imports = 0;
    let mut number_box = None;
    let mut boxes = 0;
    for payload in wasmparser::Parser::new(0).parse_all(&wasm) {
        match payload.unwrap() {
            wasmparser::Payload::ImportSection(imports) => {
                for import in imports.into_imports() {
                    let import = import.unwrap();
                    if matches!(import.ty, wasmparser::TypeRef::Func(_)) {
                        if import.name == "number-box" { number_box = Some(function_imports); }
                        function_imports += 1;
                    }
                }
            }
            wasmparser::Payload::CodeSectionEntry(body) => {
                for operator in body.get_operators_reader().unwrap() {
                    if let wasmparser::Operator::Call { function_index } = operator.unwrap() {
                        if Some(function_index) == number_box { boxes += 1; }
                    }
                }
            }
            _ => {}
        }
    }
    assert!(number_box.is_some());
    // Two source literals and the exposed final result require boxes. The
    // intermediate Int result must stay in integer scratch, rather than adding
    // a fourth allocation. Runtime value/effect tests execute this same route.
    assert_eq!(boxes, 3);
}

#[test]
fn public_bitwise_hir_and_ir_reject_bad_arity_and_result_before_emission() {
    let make = |count, ty| Hir {
        source: None,
        span: 4..19,
        metadata: vec![],
        ty,
        kind: Expression::Bitwise {
            operation: Bitwise::And,
            arguments: (0..count)
                .map(|_| Hir {
                    source: None,
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
    for operation in [Bitwise::F64Coerce, Bitwise::F64Word0, Bitwise::F64Word4, Bitwise::IdentityUid] {
        let make = |count, ty| Hir {
            source: None,
            span: 3..21, metadata: vec![], ty,
            kind: Expression::Bitwise { operation, arguments: (0..count).map(|_| Hir {
                source: None,
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
        (Bitwise::F64TimeClip, 1, Type::Number),
        (Bitwise::SafeIntegerRemainder, 2, Type::Number),
    ] {
        let make = |count, ty| Hir {
            source: None,
            span: 3..21, metadata: vec![], ty,
            kind: Expression::Bitwise { operation, arguments: (0..count).map(|_| Hir {
                source: None,
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
