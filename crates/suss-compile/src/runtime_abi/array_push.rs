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
        LocalTee(2),
        Call(b.names["source-array-storage"]),
        LocalSet(3),
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        LocalSet(5),
        LocalGet(4),
        I32Const(arrays::MAX_LENGTH),
        I32GtU,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut code);
    code.extend([
        End,
        LocalGet(5),
        I32Const(arrays::MAX_LENGTH),
        LocalGet(4),
        I32Sub,
        I32GtU,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut code);
    code.extend([
        End,
        LocalGet(4),
        LocalGet(5),
        I32Add,
        LocalTee(6),
        ArrayNewDefault(ARGS),
        LocalSet(7),
        LocalGet(7),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        LocalGet(5),
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
        LocalGet(7),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(5),
        LocalGet(1),
        I32Const(1),
        LocalGet(4),
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
        LocalGet(2),
        Call(b.names["source-array-fields"]),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        LocalGet(7),
        ArraySet(ARGS),
        LocalGet(6),
        F64ConvertI32U,
        Call(b.names["number-box"]),
    ]);
    let anchored = b.count;
    b.functions.function(INVOKE);
    let mut body = Function::new([(2, VALUE), (3, ValType::I32), (1, VALUE)]);
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
