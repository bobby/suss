//! Original checked source Array.push storage adapter, preserving owner identity.
use super::*;
pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    use Instruction::*;
    let mut code = vec![
        LocalGet(1),
        ArrayLen,
        LocalTee(4),
        I32Eqz,
        If(BlockType::Empty),
    ];
    nominal::error(&mut code);
    code.extend([
        End,
        LocalGet(4),
        I32Const(1),
        I32Sub,
        LocalSet(4),
        LocalGet(1),
        I32Const(0),
        ArrayGet(ARGS),
        LocalSet(2),
        LocalGet(2),
        Call(b.names["source-array-backing"]),
        LocalSet(3),
        LocalGet(3),
        Call(b.names["source-array-sparse-length"]),
        LocalSet(5),
        I32Const(0),
        LocalSet(6),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(6),
        LocalGet(4),
        I32GeU,
        BrIf(1),
        LocalGet(2),
        LocalGet(5), I64ExtendI32U,
        LocalGet(6), I64ExtendI32U,
        I64Add, F64ConvertI64U, Call(b.names["number-box"]),
        LocalGet(1),
        LocalGet(6),
        I32Const(1),
        I32Add,
        ArrayGet(ARGS),
        Call(b.names["source-array-set"]),
        Drop,
        LocalGet(6),
        I32Const(1),
        I32Add,
        LocalSet(6),
        Br(0),
        End,
        End,
        LocalGet(5), I64ExtendI32U, LocalGet(4), I64ExtendI32U, I64Add,
        I64Const(4294967295), I64GtU, If(BlockType::Empty),
    ]);
    range_errors::invalid_length(&mut code);
    code.extend([
        End,
        LocalGet(5),
        LocalGet(4),
        I32Add,
        F64ConvertI32U,
        Call(b.names["number-box"]),
    ]);
    let anchored = b.count;
    b.functions.function(INVOKE);
    let mut body = Function::new([(2, VALUE), (3, ValType::I32)]);
    for inst in code {
        body.instruction(&inst);
    }
    body.instruction(&End);
    b.code.function(&body);
    b.count += 1;
    let detached = b.count;
    b.functions.function(INVOKE);
    let mut code = vec![];
    nominal::error(&mut code);
    let mut body = Function::new([]);
    for inst in code {
        body.instruction(&inst);
    }
    body.instruction(&End);
    b.code.function(&body);
    b.count += 1;
    b.function(
        "source-array-push-method",
        &[],
        &[VALUE],
        &[
            GlobalGet(object_methods::TAG_GLOBAL),
            I32Const(0),
            RefI31,
            RefFunc(anchored),
            I32Const(1),
            I32Const(-1),
            Call(b.names["closure-new"]),
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 1,
            },
            I32Const(0),
            RefI31,
            I32Const(0),
            RefI31,
            StructNew(7),
            RefFunc(detached),
            I32Const(0),
            I32Const(-1),
            Call(b.names["closure-new"]),
        ],
    );
    vec![anchored, detached]
}
