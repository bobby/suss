//! Original GC-owned Array slice adapter. Copies storage; never retains its owner.
use super::*;
pub(super) const METHOD_ROOT: u32 = native_objects::TAG_GLOBAL + 2;
pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    use Instruction::*;
    // ToIntegerOrInfinity then relative-index clamping to [0, length].
    let mut body = vec![
        LocalGet(0),
        Call(b.names["coerce-number"]),
        F64Trunc,
        LocalSet(2),
        LocalGet(2),
        LocalGet(2),
        F64Ne,
        If(BlockType::Empty),
        F64Const(0.0.into()),
        LocalSet(2),
        End,
        LocalGet(2),
        F64Const(0.0.into()),
        F64Lt,
        If(BlockType::Empty),
        LocalGet(2),
        LocalGet(1),
        F64ConvertI32U,
        F64Add,
        LocalSet(2),
        End,
        LocalGet(2),
        F64Const(0.0.into()),
        F64Max,
        LocalGet(1),
        F64ConvertI32U,
        F64Min,
        I32TruncF64U,
    ];
    let index = b.function_with_locals(
        "source-array-slice-index",
        &[VALUE, ValType::I32],
        &[ValType::I32],
        &[(1, ValType::F64)],
        &body,
    );
    // Receiver and optional bounds, already evaluated in member-call order.
    body = vec![
        LocalGet(1),
        I32Const(0),
        ArrayGet(ARGS),
        Call(b.names["source-array-storage"]),
        LocalSet(2),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        LocalSet(3),
        I32Const(0),
        LocalSet(4),
        LocalGet(3),
        LocalSet(5),
        LocalGet(1),
        ArrayLen,
        I32Const(1),
        I32GtU,
        If(BlockType::Empty),
        LocalGet(1),
        I32Const(1),
        ArrayGet(ARGS),
        LocalGet(3),
        Call(index),
        LocalSet(4),
        End,
        LocalGet(1),
        ArrayLen,
        I32Const(2),
        I32GtU,
        If(BlockType::Empty),
        // Explicit undefined end defaults to length, as does an omitted end.
        // nil is numeric zero; only undefined uses the omitted default.
        LocalGet(1),
        I32Const(2),
        ArrayGet(ARGS),
        I32Const(UNDEFINED),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(1),
        I32Const(2),
        ArrayGet(ARGS),
        LocalGet(3),
        Call(index),
        LocalSet(5),
        End,
        End,
        LocalGet(5),
        LocalGet(4),
        I32LtU,
        If(BlockType::Empty),
        LocalGet(4),
        LocalSet(5),
        End,
        LocalGet(5),
        LocalGet(4),
        I32Sub,
        ArrayNewDefault(ARGS),
        LocalSet(6),
        LocalGet(6),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(4),
        LocalGet(5),
        LocalGet(4),
        I32Sub,
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
        LocalGet(6),
        Call(b.names["source-array-new"]),
    ];
    let anchored = b.count;
    b.functions.function(INVOKE);
    let mut f = Function::new([(1, VALUE), (3, ValType::I32), (1, VALUE)]);
    for i in body {
        f.instruction(&i);
    }
    f.instruction(&End);
    b.code.function(&f);
    b.count += 1;
    let detached = b.count;
    b.functions.function(INVOKE);
    let mut body = vec![];
    nominal::error(&mut body);
    let mut f = Function::new([]);
    for i in body {
        f.instruction(&i);
    }
    f.instruction(&End);
    b.code.function(&f);
    b.count += 1;
    b.function(
        "source-array-slice-method",
        &[],
        &[VALUE],
        &[
            GlobalGet(METHOD_ROOT),
            I32Const(0),
            RefI31,
            RefEq,
            If(BlockType::Empty),
            GlobalGet(object_methods::TAG_GLOBAL),
            I32Const(0),
            RefI31,
            RefFunc(anchored),
            I32Const(1),
            I32Const(3),
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
            GlobalSet(METHOD_ROOT),
            End,
            GlobalGet(METHOD_ROOT),
        ],
    );
    vec![anchored, detached]
}
