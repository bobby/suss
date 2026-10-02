//! Original owner-held identity UID for adapting Closure getUid.
//! The only allocator root is a scalar counter; owner references are never global.
use super::*;
pub(super) const COUNTER: u32 = array_methods::METHOD_ROOT + 1;
const MAX_UID: i64 = 9_007_199_254_740_991;

pub(super) fn intrinsics(b: &mut Builder) {
    use Instruction::*;
    let mut body = vec![];
    for (ty, slot) in [(4, 4), (DESCRIPTOR, 4), (7, 3), (8, 4)] {
        body.extend([
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(ty)),
            If(BlockType::Empty),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(ty)),
            StructGet {
                struct_type_index: ty,
                field_index: slot,
            },
            LocalSet(1),
            LocalGet(1),
            I32Const(0),
            RefI31,
            RefEq,
            If(BlockType::Empty),
            GlobalGet(COUNTER),
            I64Const(MAX_UID),
            I64GtU,
            If(BlockType::Empty),
        ]);
        nominal::error(&mut body);
        body.extend([
            End,
            GlobalGet(COUNTER),
            F64ConvertI64U,
            Call(b.names["number-box"]),
            LocalSet(1),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(ty)),
            LocalGet(1),
            StructSet {
                struct_type_index: ty,
                field_index: slot,
            },
            GlobalGet(COUNTER),
            I64Const(1),
            I64Add,
            GlobalSet(COUNTER),
            Else,
            LocalGet(1),
            RefTestNonNull(HeapType::Concrete(NUMBER)),
            I32Eqz,
            If(BlockType::Empty),
        ]);
        nominal::error(&mut body);
        body.extend([
            End,
            LocalGet(1),
            RefCastNonNull(HeapType::Concrete(NUMBER)),
            StructGet {
                struct_type_index: NUMBER,
                field_index: 0,
            },
            LocalSet(2),
            LocalGet(2),
            F64Const(1.0.into()),
            F64Ge,
            LocalGet(2),
            F64Const((MAX_UID as f64).into()),
            F64Le,
            I32And,
            LocalGet(2),
            LocalGet(2),
            F64Trunc,
            F64Eq,
            I32And,
            I32Eqz,
            If(BlockType::Empty),
        ]);
        nominal::error(&mut body);
        body.extend([End, End, LocalGet(1), Return, End]);
    }
    nominal::error(&mut body);
    b.function_with_locals(
        "identity-uid",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE), (1, ValType::F64)],
        &body,
    );
}
