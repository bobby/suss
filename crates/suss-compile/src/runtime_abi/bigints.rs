//! Original signed-magnitude BigInt storage and arithmetic (#338).
//!
//! Published limbs are GC-owned, little-endian u32s. Only a new result/scratch
//! array is written; zero has sign 0 and no significant limbs. The magnitude
//! capacity can exceed its normalized length. No linear-memory owner escapes.
use super::*;
use Instruction::*;

pub(super) const LIMBS: u32 = PACKED_STORAGE + 2;
fn get(local: u32, field_index: u32) -> [Instruction<'static>; 3] {
    [
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(BIGINT)),
        StructGet {
            struct_type_index: BIGINT,
            field_index,
        },
    ]
}
fn limb(array: u32, index: u32) -> [Instruction<'static>; 3] {
    [LocalGet(array), LocalGet(index), ArrayGet(LIMBS)]
}

pub(super) fn representation(b: &mut Builder) {
    b.function(
        "bigint-limbs-new",
        &[ValType::I32],
        &[VALUE],
        &[LocalGet(0), ArrayNewDefault(LIMBS)],
    );
    // bigint-new(sign, limbs, used): callers own the array. Normalize without
    // changing it, so a negative zero cannot be published.
    let code = vec![
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(2),
        I32Eqz,
        BrIf(1),
        LocalGet(1),
        LocalGet(2),
        I32Const(1),
        I32Sub,
        ArrayGet(LIMBS),
        BrIf(1),
        LocalGet(2),
        I32Const(1),
        I32Sub,
        LocalSet(2),
        Br(0),
        End,
        End,
        LocalGet(2),
        If(BlockType::Result(ValType::I32)),
        LocalGet(0),
        Else,
        I32Const(0),
        End,
        LocalGet(1),
        RefAsNonNull,
        LocalGet(2),
        StructNew(BIGINT),
    ];
    b.function(
        "bigint-new",
        &[
            ValType::I32,
            ValType::Ref(RefType {
                nullable: true,
                heap_type: HeapType::Concrete(LIMBS),
            }),
            ValType::I32,
        ],
        &[VALUE],
        &code,
    );
    b.function(
        "bigint-test",
        &[VALUE],
        &[ValType::I32],
        &[LocalGet(0), RefTestNonNull(HeapType::Concrete(BIGINT))],
    );
    // Exactly convert a long's unsigned magnitude, including Long/MIN_VALUE.
    let code = vec![
        LocalGet(0),
        I64Const(0),
        I64LtS,
        If(BlockType::Result(ValType::I32)),
        I32Const(-1),
        Else,
        LocalGet(0),
        I64Eqz,
        If(BlockType::Result(ValType::I32)),
        I32Const(0),
        Else,
        I32Const(1),
        End,
        End,
        LocalSet(1),
        LocalGet(0),
        I64Const(0),
        I64LtS,
        If(BlockType::Result(ValType::I64)),
        I64Const(0),
        LocalGet(0),
        I64Sub,
        Else,
        LocalGet(0),
        End,
        LocalSet(2),
        LocalGet(1),
        LocalGet(2),
        I32WrapI64,
        LocalGet(2),
        I64Const(32),
        I64ShrU,
        I32WrapI64,
        ArrayNewFixed {
            array_type_index: LIMBS,
            array_size: 2,
        },
        I32Const(2),
        Call(b.names["bigint-new"]),
    ];
    b.function_with_locals(
        "bigint-from-long",
        &[ValType::I64],
        &[VALUE],
        &[(1, ValType::I32), (1, ValType::I64)],
        &code,
    );
    // Round the exact magnitude once to binary64, ties to even. Inspect all
    // discarded bits: folding limbs as doubles can round twice at a halfway.
    let mut code = get(0, 1).to_vec();
    code.push(LocalSet(1));
    code.extend(get(0, 2));
    code.extend([
        LocalTee(2),
        I32Eqz,
        If(BlockType::Empty),
        F64Const(0.0.into()),
        Return,
        End,
        LocalGet(2),
        I32Const(32),
        I32GtU,
        If(BlockType::Empty),
        F64Const(f64::INFINITY.into()),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(BIGINT)),
        StructGet {
            struct_type_index: BIGINT,
            field_index: 0,
        },
        F64ConvertI32S,
        F64Mul,
        Return,
        End,
        LocalGet(2),
        I32Const(1),
        I32Sub,
        LocalSet(3),
    ]);
    code.extend(limb(1, 3));
    code.extend([
        I32Clz,
        I32Const(32),
        LocalGet(2),
        I32Mul,
        I32Sub,
        I32Const(-1),
        I32Mul,
        LocalTee(4),
        I32Const(1),
        I32Sub,
        LocalSet(3),
        Loop(BlockType::Empty),
        LocalGet(1),
        LocalGet(3),
        I32Const(5),
        I32ShrU,
        ArrayGet(LIMBS),
        LocalGet(3),
        I32Const(31),
        I32And,
        I32ShrU,
        I32Const(1),
        I32And,
        LocalSet(9),
        LocalGet(5),
        I32Const(53),
        I32LtU,
        If(BlockType::Empty),
        LocalGet(6),
        I64Const(1),
        I64Shl,
        LocalGet(9),
        I64ExtendI32U,
        I64Or,
        LocalSet(6),
        Else,
        LocalGet(5),
        I32Const(53),
        I32Eq,
        If(BlockType::Empty),
        LocalGet(9),
        LocalSet(7),
        Else,
        LocalGet(8),
        LocalGet(9),
        I32Or,
        LocalSet(8),
        End,
        End,
        LocalGet(5),
        I32Const(1),
        I32Add,
        LocalSet(5),
        LocalGet(3),
        I32Const(1),
        I32Sub,
        LocalTee(3),
        I32Const(0),
        I32GeS,
        BrIf(0),
        End,
        LocalGet(4),
        I32Const(53),
        I32LeU,
        If(BlockType::Result(ValType::F64)),
        LocalGet(6),
        F64ConvertI64U,
        Else,
        LocalGet(7),
        LocalGet(8),
        LocalGet(6),
        I32WrapI64,
        I32Const(1),
        I32And,
        I32Or,
        I32And,
        If(BlockType::Empty),
        LocalGet(6),
        I64Const(1),
        I64Add,
        LocalSet(6),
        End,
        LocalGet(6),
        I64Const(1i64 << 53),
        I64Eq,
        If(BlockType::Empty),
        LocalGet(6),
        I64Const(1),
        I64ShrU,
        LocalSet(6),
        LocalGet(4),
        I32Const(1),
        I32Add,
        LocalSet(4),
        End,
        LocalGet(4),
        I32Const(1024),
        I32GtU,
        If(BlockType::Result(ValType::F64)),
        F64Const(f64::INFINITY.into()),
        Else,
        LocalGet(4),
        I32Const(1022),
        I32Add,
        I64ExtendI32U,
        I64Const(52),
        I64Shl,
        LocalGet(6),
        I64Const((1i64 << 52) - 1),
        I64And,
        I64Or,
        F64ReinterpretI64,
        End,
        End,
    ]);
    code.extend(get(0, 0));
    code.extend([F64ConvertI32S, F64Mul]);
    b.function_with_locals(
        "bigint-double",
        &[VALUE],
        &[ValType::F64],
        &[
            (
                1,
                ValType::Ref(RefType {
                    nullable: true,
                    heap_type: HeapType::Concrete(LIMBS),
                }),
            ),
            (4, ValType::I32),
            (1, ValType::I64),
            (3, ValType::I32),
        ],
        &code,
    );
}

pub(super) fn arithmetic(b: &mut Builder) {
    // A long or BigInt as a BigInt. Source type and physical raw-local facts
    // stay separate; this is only used by runtime integer dispatch.
    b.function(
        "integer-bigint",
        &[VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            Call(b.names["bigint-test"]),
            If(BlockType::Result(VALUE)),
            LocalGet(0),
            Else,
            LocalGet(0),
            Call(b.names["long-unbox"]),
            Call(b.names["bigint-from-long"]),
            End,
        ],
    );
    // Unsigned magnitude comparison; both arrays are normalized by used length.
    let mut code = Vec::new();
    for (local, target, field) in [(0, 2, 1), (1, 3, 1), (0, 4, 2), (1, 5, 2)] {
        code.extend(get(local, field));
        code.push(LocalSet(target));
    }
    code.extend([
        LocalGet(4),
        LocalGet(5),
        I32Ne,
        If(BlockType::Empty),
        LocalGet(4),
        LocalGet(5),
        I32GtU,
        If(BlockType::Result(ValType::I32)),
        I32Const(1),
        Else,
        I32Const(-1),
        End,
        Return,
        End,
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(4),
        I32Eqz,
        BrIf(1),
        LocalGet(4),
        I32Const(1),
        I32Sub,
        LocalSet(4),
    ]);
    code.extend(limb(2, 4));
    code.push(LocalSet(6));
    code.extend(limb(3, 4));
    code.push(LocalSet(7));
    code.extend([
        LocalGet(6),
        LocalGet(7),
        I32Ne,
        If(BlockType::Empty),
        LocalGet(6),
        LocalGet(7),
        I32GtU,
        If(BlockType::Result(ValType::I32)),
        I32Const(1),
        Else,
        I32Const(-1),
        End,
        Return,
        End,
        Br(0),
        End,
        End,
        I32Const(0),
    ]);
    let array = ValType::Ref(RefType {
        nullable: true,
        heap_type: HeapType::Concrete(LIMBS),
    });
    b.function_with_locals(
        "bigint-magnitude-compare",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[(2, array), (4, ValType::I32)],
        &code,
    );
    let mut code = Vec::new();
    code.extend(get(0, 0));
    code.push(LocalSet(2));
    code.extend(get(1, 0));
    code.push(LocalSet(3));
    code.extend([
        LocalGet(2),
        LocalGet(3),
        I32Ne,
        If(BlockType::Result(ValType::I32)),
        LocalGet(2),
        LocalGet(3),
        I32GtS,
        If(BlockType::Result(ValType::I32)),
        I32Const(1),
        Else,
        I32Const(-1),
        End,
        Else,
        LocalGet(0),
        LocalGet(1),
        Call(b.names["bigint-magnitude-compare"]),
        LocalGet(2),
        I32Mul,
        End,
    ]);
    b.function_with_locals(
        "bigint-compare",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[(2, ValType::I32)],
        &code,
    );
    let mut negate = get(0, 0).to_vec();
    negate.extend([I32Const(-1), I32Mul]);
    negate.extend(get(0, 1));
    negate.extend(get(0, 2));
    negate.push(StructNew(BIGINT));
    b.function("bigint-negate", &[VALUE], &[VALUE], &negate);
    // bigint-add: locals 2/3 signs, 4/5 arrays, 6/7 lengths, 8 result,
    // 9 index, 10 capacity, 11 carry/borrow, 12 digit, 13 temporary object.
    let mut code = Vec::new();
    for (local, target, field) in [
        (0, 2, 0),
        (1, 3, 0),
        (0, 4, 1),
        (1, 5, 1),
        (0, 6, 2),
        (1, 7, 2),
    ] {
        code.extend(get(local, field));
        code.push(LocalSet(target));
    }
    code.extend([
        LocalGet(2),
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(1),
        Return,
        End,
        LocalGet(3),
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(0),
        Return,
        End,
        LocalGet(2),
        LocalGet(3),
        I32Eq,
        If(BlockType::Empty),
        LocalGet(6),
        LocalGet(7),
        LocalGet(6),
        LocalGet(7),
        I32GtU,
        Select,
        I32Const(1),
        I32Add,
        LocalTee(10),
        ArrayNewDefault(LIMBS),
        LocalSet(8),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(9),
        LocalGet(10),
        I32GeU,
        BrIf(1),
        LocalGet(11),
        LocalGet(9),
        LocalGet(6),
        I32LtU,
        If(BlockType::Result(ValType::I64)),
    ]);
    code.extend(limb(4, 9));
    code.extend([
        I64ExtendI32U,
        Else,
        I64Const(0),
        End,
        I64Add,
        LocalGet(9),
        LocalGet(7),
        I32LtU,
        If(BlockType::Result(ValType::I64)),
    ]);
    code.extend(limb(5, 9));
    code.extend([
        I64ExtendI32U,
        Else,
        I64Const(0),
        End,
        I64Add,
        LocalSet(12),
        LocalGet(8),
        LocalGet(9),
        LocalGet(12),
        I32WrapI64,
        ArraySet(LIMBS),
        LocalGet(12),
        I64Const(32),
        I64ShrU,
        LocalSet(11),
        LocalGet(9),
        I32Const(1),
        I32Add,
        LocalSet(9),
        Br(0),
        End,
        End,
        LocalGet(2),
        LocalGet(8),
        LocalGet(10),
        Call(b.names["bigint-new"]),
        Return,
        End,
        // Opposite signs: put the larger magnitude on the left.
        LocalGet(0),
        LocalGet(1),
        Call(b.names["bigint-magnitude-compare"]),
        I32Const(0),
        I32LtS,
        If(BlockType::Empty),
        LocalGet(0),
        LocalSet(13),
        LocalGet(1),
        LocalSet(0),
        LocalGet(13),
        LocalSet(1),
    ]);
    for (local, target, field) in [(0, 2, 0), (0, 4, 1), (1, 5, 1), (0, 6, 2), (1, 7, 2)] {
        code.extend(get(local, field));
        code.push(LocalSet(target));
    }
    code.extend([
        End,
        LocalGet(6),
        ArrayNewDefault(LIMBS),
        LocalSet(8),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(9),
        LocalGet(6),
        I32GeU,
        BrIf(1),
    ]);
    code.extend(limb(4, 9));
    code.extend([
        I64ExtendI32U,
        LocalGet(11),
        I64Sub,
        LocalGet(9),
        LocalGet(7),
        I32LtU,
        If(BlockType::Result(ValType::I64)),
    ]);
    code.extend(limb(5, 9));
    code.extend([
        I64ExtendI32U,
        Else,
        I64Const(0),
        End,
        I64Sub,
        LocalSet(12),
        LocalGet(8),
        LocalGet(9),
        LocalGet(12),
        I32WrapI64,
        ArraySet(LIMBS),
        LocalGet(12),
        I64Const(0),
        I64LtS,
        I64ExtendI32U,
        LocalSet(11),
        LocalGet(9),
        I32Const(1),
        I32Add,
        LocalSet(9),
        Br(0),
        End,
        End,
        LocalGet(2),
        LocalGet(8),
        LocalGet(6),
        Call(b.names["bigint-new"]),
    ]);
    b.function_with_locals(
        "bigint-add",
        &[VALUE, VALUE],
        &[VALUE],
        &[
            (2, ValType::I32),
            (2, array),
            (2, ValType::I32),
            (1, array),
            (2, ValType::I32),
            (2, ValType::I64),
            (1, VALUE),
        ],
        &code,
    );
    b.function(
        "bigint-subtract",
        &[VALUE, VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            LocalGet(1),
            Call(b.names["bigint-negate"]),
            Call(b.names["bigint-add"]),
        ],
    );
    // Original schoolbook multiplication; every digit product plus existing
    // digit and carry fits exactly in u64. Inputs stay immutable.
    // Locals: 2/3 arrays, 4/5 lengths, 6 result, 7 i, 8 j, 9 carry, 10 product.
    let mut code = Vec::new();
    for (local, target, field) in [(0, 2, 1), (1, 3, 1), (0, 4, 2), (1, 5, 2)] {
        code.extend(get(local, field));
        code.push(LocalSet(target));
    }
    code.extend([
        LocalGet(4),
        LocalGet(5),
        I32Add,
        ArrayNewDefault(LIMBS),
        LocalSet(6),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(7),
        LocalGet(4),
        I32GeU,
        BrIf(1),
        I32Const(0),
        LocalSet(8),
        I64Const(0),
        LocalSet(9),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(8),
        LocalGet(5),
        I32GeU,
        BrIf(1),
    ]);
    code.extend(limb(2, 7));
    code.push(I64ExtendI32U);
    code.extend(limb(3, 8));
    code.extend([
        I64ExtendI32U,
        I64Mul,
        LocalGet(9),
        I64Add,
        LocalGet(6),
        LocalGet(7),
        LocalGet(8),
        I32Add,
        ArrayGet(LIMBS),
        I64ExtendI32U,
        I64Add,
        LocalSet(10),
        LocalGet(6),
        LocalGet(7),
        LocalGet(8),
        I32Add,
        LocalGet(10),
        I32WrapI64,
        ArraySet(LIMBS),
        LocalGet(10),
        I64Const(32),
        I64ShrU,
        LocalSet(9),
        LocalGet(8),
        I32Const(1),
        I32Add,
        LocalSet(8),
        Br(0),
        End,
        End,
        LocalGet(6),
        LocalGet(7),
        LocalGet(5),
        I32Add,
        LocalGet(9),
        I32WrapI64,
        ArraySet(LIMBS),
        LocalGet(7),
        I32Const(1),
        I32Add,
        LocalSet(7),
        Br(0),
        End,
        End,
    ]);
    code.extend(get(0, 0));
    code.extend(get(1, 0));
    code.extend([
        I32Mul,
        LocalGet(6),
        LocalGet(4),
        LocalGet(5),
        I32Add,
        Call(b.names["bigint-new"]),
    ]);
    b.function_with_locals(
        "bigint-multiply",
        &[VALUE, VALUE],
        &[VALUE],
        &[
            (2, array),
            (2, ValType::I32),
            (1, array),
            (2, ValType::I32),
            (2, ValType::I64),
        ],
        &code,
    );
}

pub(super) fn string(b: &mut Builder) {
    // Decimal conversion by repeated division of a private magnitude copy.
    // Locals: 1 copy, 2 used, 3 digits buffer, 4 count, 5 i, 6 remainder,
    // 7 current u64, 8 quotient, 9 output, 10 negative sign.
    let array = ValType::Ref(RefType {
        nullable: true,
        heap_type: HeapType::Concrete(LIMBS),
    });
    let string = ValType::Ref(RefType {
        nullable: true,
        heap_type: HeapType::Concrete(STRING),
    });
    let mut code = get(0, 2).to_vec();
    code.extend([
        LocalTee(2),
        ArrayNewDefault(LIMBS),
        LocalSet(1),
        LocalGet(1),
        I32Const(0),
    ]);
    code.extend(get(0, 1));
    code.extend([
        I32Const(0),
        LocalGet(2),
        ArrayCopy {
            array_type_index_dst: LIMBS,
            array_type_index_src: LIMBS,
        },
        // At most ten decimal digits per 32-bit limb, plus zero.
        LocalGet(2),
        I32Const(10),
        I32Mul,
        I32Const(1),
        I32Add,
        ArrayNewDefault(STRING),
        LocalSet(3),
        Loop(BlockType::Empty),
        I64Const(0),
        LocalSet(6),
        LocalGet(2),
        LocalSet(5),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(5),
        I32Eqz,
        BrIf(1),
        LocalGet(5),
        I32Const(1),
        I32Sub,
        LocalSet(5),
        LocalGet(6),
        I64Const(32),
        I64Shl,
    ]);
    code.extend(limb(1, 5));
    code.extend([
        I64ExtendI32U,
        I64Or,
        LocalSet(7),
        LocalGet(7),
        I64Const(10),
        I64DivU,
        LocalSet(8),
        LocalGet(7),
        LocalGet(8),
        I64Const(10),
        I64Mul,
        I64Sub,
        LocalSet(6),
        LocalGet(1),
        LocalGet(5),
        LocalGet(8),
        I32WrapI64,
        ArraySet(LIMBS),
        Br(0),
        End,
        End,
        LocalGet(3),
        LocalGet(4),
        LocalGet(6),
        I32WrapI64,
        I32Const(48),
        I32Add,
        ArraySet(STRING),
        LocalGet(4),
        I32Const(1),
        I32Add,
        LocalSet(4),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(2),
        I32Eqz,
        BrIf(1),
        LocalGet(1),
        LocalGet(2),
        I32Const(1),
        I32Sub,
        ArrayGet(LIMBS),
        BrIf(1),
        LocalGet(2),
        I32Const(1),
        I32Sub,
        LocalSet(2),
        Br(0),
        End,
        End,
        LocalGet(2),
        BrIf(0),
        End,
    ]);
    code.extend(get(0, 0));
    code.extend([
        I32Const(0),
        I32LtS,
        LocalSet(10),
        LocalGet(4),
        LocalGet(10),
        I32Add,
        ArrayNewDefault(STRING),
        LocalSet(9),
        LocalGet(10),
        If(BlockType::Empty),
        LocalGet(9),
        I32Const(0),
        I32Const(45),
        ArraySet(STRING),
        End,
        I32Const(0),
        LocalSet(5),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(5),
        LocalGet(4),
        I32GeU,
        BrIf(1),
        LocalGet(9),
        LocalGet(5),
        LocalGet(10),
        I32Add,
        LocalGet(3),
        LocalGet(4),
        I32Const(1),
        I32Sub,
        LocalGet(5),
        I32Sub,
        ArrayGetU(STRING),
        ArraySet(STRING),
        LocalGet(5),
        I32Const(1),
        I32Add,
        LocalSet(5),
        Br(0),
        End,
        End,
        LocalGet(9),
    ]);
    b.function_with_locals(
        "bigint-string",
        &[VALUE],
        &[VALUE],
        &[
            (1, array),
            (1, ValType::I32),
            (1, string),
            (2, ValType::I32),
            (3, ValType::I64),
            (1, string),
            (1, ValType::I32),
        ],
        &code,
    );
}
