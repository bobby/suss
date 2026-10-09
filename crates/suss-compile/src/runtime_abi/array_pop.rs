//! Original checked source Array.pop adapter, preserving its physical owner.
use super::*;
pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    use Instruction::*;
    let mut code = vec![LocalGet(1), ArrayLen, I32Eqz, If(BlockType::Empty)];
    nominal::error(&mut code);
    code.extend([
        End,
        LocalGet(1),
        I32Const(0),
        ArrayGet(ARGS),
        LocalSet(2),
        LocalGet(2),
        Call(b.names["source-array-backing"]),
        LocalSet(3),
        LocalGet(3),
        Call(b.names["source-array-sparse-length"]),
        LocalTee(4),
        I32Eqz,
        If(BlockType::Empty),
        I32Const(UNDEFINED),
        RefI31,
        Return,
        End,
        LocalGet(4),
        I32Const(1),
        I32Sub,
        LocalSet(4),
        LocalGet(3),
        LocalGet(4),
        Call(b.names["source-array-sparse-get"]),
        LocalSet(5),
        LocalGet(3),
        LocalGet(4),
        Call(b.names["source-array-sparse-delete"]),
        Drop,
        LocalGet(3),
        LocalGet(4),
        Call(b.names["source-array-sparse-set-length"]),
        LocalGet(5),
    ]);
    let anchored = b.count;
    b.functions.function(INVOKE);
    let mut body = Function::new([(2, VALUE), (1, ValType::I32), (1, VALUE)]);
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
        "source-array-pop-method",
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
