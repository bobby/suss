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
        LocalTee(2),
        Call(b.names["source-array-storage"]),
        LocalSet(3),
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
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
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(4),
        ArrayGet(ARGS),
        LocalSet(6),
        LocalGet(4),
        ArrayNewDefault(ARGS),
        LocalSet(5),
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        LocalGet(4),
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
        LocalGet(2),
        Call(b.names["source-array-fields"]),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        LocalGet(5),
        ArraySet(ARGS),
        LocalGet(6),
    ]);
    let anchored = b.count;
    b.functions.function(INVOKE);
    let mut body = Function::new([(2, VALUE), (1, ValType::I32), (2, VALUE)]);
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
