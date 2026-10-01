//! Original owned property storage kernel. Prototype lookup/set is a separate
//! adapter; these exports inspect/mutate OWN data properties only.
use super::*;
pub(super) const TAG_GLOBAL: u32 = string_methods::METHOD_ROOT + 1;
fn guard(body: &mut Vec<Instruction<'static>>, local: u32, ty: u32) {
    use Instruction::*;
    body.extend([
        LocalGet(local),
        RefTestNonNull(HeapType::Concrete(ty)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(body);
    body.push(End);
}
fn array(body: &mut Vec<Instruction<'static>>, local: u32) {
    body.extend([
        Instruction::LocalGet(local),
        Instruction::RefCastNonNull(HeapType::Concrete(ARGS)),
    ]);
}
pub(super) fn functions(b: &mut Builder) {
    use Instruction::*;
    b.function(
        "native-object-new",
        &[],
        &[VALUE],
        &[
            GlobalGet(TAG_GLOBAL),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            RefI31,
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 2,
            },
            I32Const(0),
            RefI31,
            StructNew(7),
        ],
    );
    let mut body = vec![];
    guard(&mut body, 0, 7);
    body.extend([
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(7)),
        StructGet {
            struct_type_index: 7,
            field_index: 0,
        },
        GlobalGet(TAG_GLOBAL),
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(7)),
        StructGet {
            struct_type_index: 7,
            field_index: 1,
        },
        LocalSet(1),
    ]);
    array(&mut body, 1);
    body.extend([ArrayLen, I32Const(2), I32Ne, If(BlockType::Empty)]);
    nominal::error(&mut body);
    body.push(End);
    array(&mut body, 1);
    body.extend([I32Const(0), ArrayGet(ARGS), LocalSet(2)]);
    guard(&mut body, 2, ARGS);
    array(&mut body, 2);
    body.extend([ArrayLen, I32Const(1), I32And, If(BlockType::Empty)]);
    nominal::error(&mut body);
    body.push(End);
    body.extend([
        I32Const(0),
        LocalSet(3),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(3),
    ]);
    array(&mut body, 2);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    array(&mut body, 2);
    body.extend([LocalGet(3), ArrayGet(ARGS), LocalSet(4)]);
    guard(&mut body, 4, STRING);
    array(&mut body, 2);
    body.extend([
        LocalGet(3),
        I32Const(1),
        I32Add,
        ArrayGet(ARGS),
        RefIsNull,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.push(End);
    body.extend([
        LocalGet(3),
        I32Const(2),
        I32Add,
        LocalSet(3),
        Br(0),
        End,
        End,
        LocalGet(1),
    ]);
    let fields = b.function_with_locals(
        "native-object-fields",
        &[VALUE],
        &[VALUE],
        &[(2, VALUE), (1, ValType::I32), (1, VALUE)],
        &body,
    );
    let mut body = vec![
        LocalGet(0),
        Call(fields),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        LocalSet(2),
    ];
    guard(&mut body, 1, STRING);
    body.extend([
        I32Const(0),
        LocalSet(3),
        I32Const(-1),
        LocalSet(4),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(3),
    ]);
    array(&mut body, 2);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    array(&mut body, 2);
    body.extend([
        LocalGet(3),
        ArrayGet(ARGS),
        LocalGet(1),
        Call(b.names["property-key-equal"]),
        If(BlockType::Empty),
        LocalGet(3),
        LocalSet(4),
        End,
        LocalGet(3),
        I32Const(2),
        I32Add,
        LocalSet(3),
        Br(0),
        End,
        End,
        LocalGet(4),
    ]);
    let slot = b.function_with_locals(
        "native-object-own-slot",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[(1, VALUE), (2, ValType::I32)],
        &body,
    );
    let mut body = vec![
        LocalGet(0),
        LocalGet(1),
        Call(slot),
        LocalSet(2),
        LocalGet(2),
        I32Const(0),
        I32GeS,
        If(BlockType::Result(VALUE)),
        LocalGet(0),
        Call(fields),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(2),
        I32Const(1),
        I32Add,
        ArrayGet(ARGS),
        Else,
        I32Const(UNDEFINED),
        RefI31,
        End,
    ];
    b.function_with_locals(
        "native-object-own-get",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, ValType::I32)],
        &body,
    );
    body = vec![
        LocalGet(0),
        LocalGet(1),
        Call(slot),
        LocalSet(3),
        LocalGet(2),
        RefIsNull,
        If(BlockType::Empty),
    ];
    nominal::error(&mut body);
    body.extend([End, LocalGet(0), Call(fields), LocalSet(4)]);
    array(&mut body, 4);
    body.extend([
        I32Const(0),
        ArrayGet(ARGS),
        LocalSet(5),
        LocalGet(3),
        I32Const(0),
        I32GeS,
        If(BlockType::Empty),
    ]);
    array(&mut body, 5);
    body.extend([
        LocalGet(3),
        I32Const(1),
        I32Add,
        LocalGet(2),
        ArraySet(ARGS),
        LocalGet(2),
        Return,
        End,
    ]);
    array(&mut body, 5);
    body.extend([
        ArrayLen,
        LocalSet(3),
        LocalGet(3),
        I32Const(arrays::MAX_LENGTH - 2),
        I32GtU,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(3),
        I32Const(2),
        I32Add,
        ArrayNewDefault(ARGS),
        LocalSet(6),
    ]);
    array(&mut body, 6);
    body.push(I32Const(0));
    array(&mut body, 5);
    body.extend([
        I32Const(0),
        LocalGet(3),
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
    ]);
    array(&mut body, 6);
    body.extend([LocalGet(3), LocalGet(1), ArraySet(ARGS)]);
    array(&mut body, 6);
    body.extend([
        LocalGet(3),
        I32Const(1),
        I32Add,
        LocalGet(2),
        ArraySet(ARGS),
    ]);
    array(&mut body, 4);
    body.extend([I32Const(0), LocalGet(6), ArraySet(ARGS), LocalGet(2)]);
    b.function_with_locals(
        "native-object-own-set",
        &[VALUE, VALUE, VALUE],
        &[VALUE],
        &[(1, ValType::I32), (3, VALUE)],
        &body,
    );
}
