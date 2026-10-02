//! Original adaptation of pinned add-ifn-methods prototype call/apply entries.
//! Canonical keys are supplied by phase resolution, never inferred from layout.
use super::*;

fn guard(code: &mut Vec<Instruction<'static>>, local: u32, ty: u32) {
    use Instruction::*;
    code.extend([
        LocalGet(local),
        RefTestNonNull(HeapType::Concrete(ty)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(code);
    code.push(End);
}
fn array(code: &mut Vec<Instruction<'static>>, local: u32) {
    code.extend([
        Instruction::LocalGet(local),
        Instruction::RefCastNonNull(HeapType::Concrete(ARGS)),
    ]);
}
fn name(code: &mut Vec<Instruction<'static>>, spelling: &str) {
    use Instruction::*;
    let units: Vec<_> = spelling.encode_utf16().collect();
    code.extend(units.iter().map(|unit| I32Const(*unit as i32)));
    code.push(ArrayNewFixed {
        array_type_index: STRING,
        array_size: units.len() as u32,
    });
}
pub(super) fn functions(b: &mut Builder, equal: u32) -> Vec<u32> {
    use Instruction::*;
    let mut callbacks = vec![];
    for applying in [false, true] {
        let mut code = vec![];
        guard(&mut code, 0, ARGS);
        array(&mut code, 0);
        code.extend([ArrayLen, I32Const(2), I32Ne, If(BlockType::Empty)]);
        nominal::error(&mut code);
        code.push(End);
        array(&mut code, 0);
        code.extend([I32Const(0), ArrayGet(ARGS), LocalSet(2)]);
        array(&mut code, 0);
        code.extend([I32Const(1), ArrayGet(ARGS), LocalSet(3)]);
        guard(&mut code, 3, ARGS);
        array(&mut code, 3);
        code.extend([ArrayLen, I32Const(22), I32Ne, If(BlockType::Empty)]);
        nominal::error(&mut code);
        code.push(End);
        code.extend([LocalGet(1), ArrayLen, LocalSet(7)]);
        if applying {
            code.extend([LocalGet(7), I32Const(2), I32LtU, If(BlockType::Empty)]);
            nominal::error(&mut code);
            code.push(End);
            code.extend([
                LocalGet(1),
                I32Const(1),
                ArrayGet(ARGS),
                Call(b.names["source-array-storage"]),
                LocalSet(4),
            ]);
            array(&mut code, 4);
            code.extend([
                ArrayLen,
                LocalSet(7),
                LocalGet(7),
                I32Const(20),
                I32GtU,
                If(BlockType::Empty),
                LocalGet(7),
                I32Const(20),
                I32Sub,
                ArrayNewDefault(ARGS),
                LocalSet(5),
            ]);
            array(&mut code, 5);
            code.push(I32Const(0));
            array(&mut code, 4);
            code.extend([
                I32Const(20),
                LocalGet(7),
                I32Const(20),
                I32Sub,
                ArrayCopy {
                    array_type_index_dst: ARGS,
                    array_type_index_src: ARGS,
                },
                I32Const(21),
                ArrayNewDefault(ARGS),
                LocalSet(6),
            ]);
            array(&mut code, 6);
            code.push(I32Const(0));
            array(&mut code, 4);
            code.extend([
                I32Const(0),
                I32Const(20),
                ArrayCopy {
                    array_type_index_dst: ARGS,
                    array_type_index_src: ARGS,
                },
            ]);
            array(&mut code, 6);
            code.extend([
                I32Const(20),
                LocalGet(5),
                Call(b.names["source-array-new"]),
                ArraySet(ARGS),
                LocalGet(6),
                LocalSet(4),
                I32Const(21),
                LocalSet(7),
                End,
            ]);
        } else {
            code.extend([
                I32Const(0),
                LocalSet(8),
                LocalGet(7),
                I32Eqz,
                I32Eqz,
                If(BlockType::Empty),
                I32Const(1),
                LocalSet(8),
                End,
                LocalGet(7),
                LocalGet(8),
                I32Sub,
                LocalSet(7),
                LocalGet(7),
                ArrayNewDefault(ARGS),
                LocalSet(4),
            ]);
            array(&mut code, 4);
            code.extend([
                I32Const(0),
                LocalGet(1),
                LocalGet(8),
                LocalGet(7),
                ArrayCopy {
                    array_type_index_dst: ARGS,
                    array_type_index_src: ARGS,
                },
            ]);
        }
        code.extend([
            LocalGet(7),
            I32Const(21),
            I32GtU,
            If(BlockType::Empty),
            Call(b.names["arity-error"]),
            Return,
            End,
            LocalGet(2),
        ]);
        array(&mut code, 3);
        code.extend([
            LocalGet(7),
            ArrayGet(ARGS),
            Call(b.names["callable-bind"]),
            LocalGet(4),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            Call(b.names["invoke"]),
        ]);
        let callback = b.count;
        b.functions.function(INVOKE);
        let mut body = Function::new([(5, VALUE), (2, ValType::I32)]);
        for inst in code {
            body.instruction(&inst);
        }
        body.instruction(&End);
        b.code.function(&body);
        b.count += 1;
        callbacks.push(callback);
        b.function(
            if applying {
                "ifn-apply-method"
            } else {
                "ifn-call-method"
            },
            &[VALUE, VALUE],
            &[VALUE],
            &[
                LocalGet(0),
                LocalGet(1),
                ArrayNewFixed {
                    array_type_index: ARGS,
                    array_size: 2,
                },
                RefFunc(callback),
                I32Const(0),
                I32Const(-1),
                Call(b.names["closure-new"]),
            ],
        );
    }
    let mut code = vec![
        LocalGet(0),
        LocalGet(1),
        Call(b.names["fixed-named-property-get"]),
        LocalSet(3),
        LocalGet(3),
        I32Const(UNDEFINED),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(3),
        Return,
        End,
    ];
    guard(&mut code, 2, ARGS);
    array(&mut code, 2);
    code.extend([ArrayLen, I32Const(22), I32Ne, If(BlockType::Empty)]);
    nominal::error(&mut code);
    code.push(End);
    for (spelling, export) in [("call", "ifn-call-method"), ("apply", "ifn-apply-method")] {
        code.push(LocalGet(1));
        name(&mut code, spelling);
        code.extend([
            Call(equal),
            If(BlockType::Empty),
            LocalGet(0),
            LocalGet(2),
            Call(b.names[export]),
            Return,
            End,
        ]);
    }
    for arity in 0..=21 {
        code.push(LocalGet(1));
        name(&mut code, &format!("cljs$core$IFn$_invoke$arity${arity}"));
        code.extend([
            Call(equal),
            If(BlockType::Empty),
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(7)),
            If(BlockType::Empty),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(7)),
            StructGet {
                struct_type_index: 7,
                field_index: 0,
            },
        ]);
        array(&mut code, 2);
        code.extend([
            I32Const(arity),
            ArrayGet(ARGS),
            Call(b.names["protocol-method-get"]),
            LocalSet(3),
            LocalGet(3),
            I32Const(0),
            RefI31,
            RefEq,
            I32Eqz,
            If(BlockType::Empty),
            LocalGet(0),
        ]);
        array(&mut code, 2);
        code.extend([
            I32Const(arity),
            ArrayGet(ARGS),
            Call(b.names["callable-bind"]),
            Return,
            End,
            End,
            End,
        ]);
    }
    code.extend([I32Const(UNDEFINED), RefI31]);
    b.function_with_locals(
        "callable-property-get",
        &[VALUE, VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &code,
    );
    callbacks
}
