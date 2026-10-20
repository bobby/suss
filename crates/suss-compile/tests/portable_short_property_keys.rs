use suss_compile::portable::{
    self,
    hir::{Expression, Hir, Literal, Nominal, Type},
    ir::{self, Operation, Terminator},
};

fn literal(value: Literal, ty: Type) -> Hir {
    Hir {
        source: None,
        span: 0..1,
        metadata: vec![],
        ty,
        kind: Expression::Literal(value),
    }
}
fn getter(units: Vec<u16>) -> ir::Function {
    ir::lower(&Hir {
        source: None,
        span: 0..1,
        metadata: vec![],
        ty: Type::Value,
        kind: Expression::Nominal {
            operation: Nominal::NamedGet,
            arguments: vec![
                literal(Literal::Nil, Type::Nil),
                literal(Literal::String(units), Type::String),
            ],
        },
    })
    .unwrap()
}
fn emission(ir: &ir::Function) -> (usize, usize, usize) {
    let bytes = portable::compile_ir(ir).unwrap();
    let mut names = Vec::new();
    let (mut arrays, mut packed, mut ordinary) = (0, 0, 0);
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        match payload.unwrap() {
            wasmparser::Payload::ImportSection(imports) => {
                for import in imports.into_imports() {
                    let import = import.unwrap();
                    if matches!(import.ty, wasmparser::TypeRef::Func(_)) {
                        names.push(import.name.to_owned());
                    }
                }
            }
            wasmparser::Payload::CodeSectionEntry(body) => {
                for op in body.get_operators_reader().unwrap() {
                    match op.unwrap() {
                        wasmparser::Operator::ArrayNewFixed { .. } => arrays += 1,
                        wasmparser::Operator::Call { function_index } => {
                            match names.get(function_index as usize).map(String::as_str) {
                                Some("short-literal-property-probe") => packed += 1,
                                Some("named-property-get") => ordinary += 1,
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    (arrays, packed, ordinary)
}
fn assert_probe_fallback(ir: &ir::Function) {
    let key = ir.blocks[0]
        .instructions
        .iter()
        .find(|i| matches!(&i.operation, Operation::Literal(Literal::String(_))))
        .unwrap()
        .result;
    let wasm = portable::compile_ir(ir).unwrap();
    let mut names = Vec::new();
    let mut proved = false;
    for payload in wasmparser::Parser::new(0).parse_all(&wasm) {
        match payload.unwrap() {
            wasmparser::Payload::ImportSection(imports) => {
                for import in imports.into_imports() {
                    let import = import.unwrap();
                    if matches!(import.ty, wasmparser::TypeRef::Func(_)) {
                        names.push(import.name.to_owned());
                    }
                }
            }
            wasmparser::Payload::CodeSectionEntry(body) => {
                let ops = body
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap();
                let probe=ops.iter().position(|op|matches!(op,wasmparser::Operator::Call {function_index}
                    if names.get(*function_index as usize).map(String::as_str)==Some("short-literal-property-probe"))).unwrap();
                assert!(
                    matches!(ops[probe+1],wasmparser::Operator::LocalSet {local_index} if local_index==key.0 as u32)
                );
                assert!(matches!(ops[probe + 2], wasmparser::Operator::If { .. }));
                assert!(
                    matches!(ops[probe+3],wasmparser::Operator::LocalGet {local_index} if local_index==key.0 as u32)
                );
                assert!(matches!(ops[probe + 4], wasmparser::Operator::Else));
                let array = ops
                    .iter()
                    .position(|op| matches!(op, wasmparser::Operator::ArrayNewFixed { .. }))
                    .unwrap();
                assert!(
                    array > probe + 4,
                    "no key allocation before probe or in the hit arm"
                );
                assert!(
                    matches!(ops[array+1],wasmparser::Operator::LocalSet {local_index} if local_index==key.0 as u32)
                );
                assert!(
                    matches!(ops[array+3],wasmparser::Operator::LocalGet {local_index} if local_index==key.0 as u32)
                );
                assert!(
                    matches!(ops[array+4],wasmparser::Operator::Call {function_index}
                    if names.get(function_index as usize).map(String::as_str)==Some("named-property-get"))
                );
                assert!(matches!(ops[array + 5], wasmparser::Operator::End));
                proved = true;
            }
            _ => {}
        }
    }
    assert!(proved);
}

#[test]
fn adjacent_short_key_probe_keeps_original_allocation_only_on_miss() {
    for units in [
        vec![],
        vec![0],
        vec![0xd800, 0],
        vec![0x6e, 0x61, 0x6d, 0x65],
    ] {
        let ir = getter(units);
        assert_eq!(emission(&ir), (1, 1, 1));
        assert_probe_fallback(&ir);
    }
    assert_eq!(emission(&getter(vec![0x61; 5])), (1, 0, 1));
}
#[test]
fn key_escape_in_return_and_closure_capture_keeps_original_allocation() {
    let mut returned = getter(vec![0x6e, 0x73]);
    let key = returned.blocks[0]
        .instructions
        .iter()
        .find(|i| matches!(&i.operation, Operation::Literal(Literal::String(_))))
        .unwrap()
        .result;
    returned.blocks[0].terminator = Terminator::Return(key);
    assert_eq!(emission(&returned), (1, 0, 1));

    let mut captured = getter(vec![0x6e, 0x73]);
    let key = captured.blocks[0]
        .instructions
        .iter()
        .find(|i| matches!(&i.operation, Operation::Literal(Literal::String(_))))
        .unwrap()
        .result;
    // A real closure signature/body with one capture. The capture use is after
    // the getter, so adjacency alone would incorrectly omit the key's root.
    let closure = Hir {
        source: None,
        span: 0..1,
        metadata: vec![],
        ty: Type::Closure(0),
        kind: Expression::Function {
            parameters: vec![],
            captures: vec![],
            body: Box::new(literal(Literal::Nil, Type::Nil)),
        },
    };
    let mut closure_ir = ir::lower(&closure).unwrap();
    let mut operation = closure_ir.blocks[0].instructions.remove(0).operation;
    if let Operation::MakeClosure { captures, body } = &mut operation {
        captures.push(key);
        body.capture_types.push(Type::String);
        let parameter = ir::ValueId(body.function.values.len());
        body.function.values.push(ir::Value {
            ty: Type::String,
            span: 0..1,
        });
        body.function.blocks[0].parameters.push(parameter);
    } else {
        panic!("closure operation");
    }
    let id = ir::ValueId(captured.values.len());
    captured.values.push(ir::Value {
        ty: Type::Closure(0),
        span: 0..1,
    });
    captured.blocks[0].instructions.push(ir::Instruction {
        result: id,
        operation,
        span: 0..1,
    });
    // Capture vector's actual use must be counted even if the closure body does
    // not read it. This exercises allocation retention, not invocation arity.
    assert_eq!(
        emission(&captured),
        (2, 0, 1),
        "key plus closure capture array stay allocated"
    );
}
