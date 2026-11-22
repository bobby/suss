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
        I32Const(0), // BigInt; BigInteger has the same category, with plain printing.
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
    // Canonical u64 bits are an unsigned magnitude, including when the high
    // bit is set. The WIT adapter keeps smaller inputs as ordinary longs.
    b.function(
        "bigint-from-u64",
        &[ValType::I64],
        &[VALUE],
        &[
            LocalGet(0), I64Eqz, I32Eqz,
            LocalGet(0), I32WrapI64,
            LocalGet(0), I64Const(32), I64ShrU, I32WrapI64,
            ArrayNewFixed { array_type_index: LIMBS, array_size: 2 },
            I32Const(2), Call(b.names["bigint-new"]),
        ],
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

pub(super) fn arithmetic(b: &mut Builder, helper: numeric::Info) {
    // A long or BigInt as a BigInt. Source type and physical raw-local facts
    // stay separate; this is only used by runtime integer dispatch.
    b.function(
        "integer-bigint-base",
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
    negate.push(Call(b.names["bigint-new"]));
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
    // Match the measured short-outer-loop policy on either operand order.
    code.extend([
        LocalGet(4), LocalGet(5), I32GtU, If(BlockType::Empty),
        LocalGet(2), LocalGet(3), LocalSet(2), LocalSet(3),
        LocalGet(4), LocalGet(5), LocalSet(4), LocalSet(5), End,
    ]);
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
        "bigint-multiply-gc",
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
    multiplication_bridge(b, helper);
}

/// Copy private magnitudes into bounded numeric scratch and copy the complete
/// product back into GC-owned storage. No linear-memory owner can escape, and
/// no guest call or callback occurs while scratch is live.
fn multiplication_bridge(b: &mut Builder, helper: numeric::Info) {
    let array = ValType::Ref(RefType {
        nullable: true,
        heap_type: HeapType::Concrete(LIMBS),
    });
    let word = MemArg { offset: 0, align: 2, memory_index: 0 };
    let mut code = Vec::new();
    // Locals: 2/3 input arrays, 4/5 used lengths, 6 scratch base, 7 B pointer,
    // 8 output pointer, 9 capacity, 10 index, 11 normalized length, 12 result.
    for (local, target, field) in [(0, 2, 1), (1, 3, 1), (0, 4, 2), (1, 5, 2)] {
        code.extend(get(local, field));
        code.push(LocalSet(target));
    }
    code.extend([
        // Inputs plus output need 8*(la+lb) bytes. Widen before adding or
        // multiplying; numeric-reserve checks memory32 capacity before growth.
        LocalGet(4), I64ExtendI32U, LocalGet(5), I64ExtendI32U, I64Add,
        I64Const(8), I64Mul, Call(b.names["numeric-reserve"]), LocalSet(6),
        LocalGet(4), LocalGet(5), I32Add, LocalSet(9),
        LocalGet(6), LocalGet(4), I32Const(4), I32Mul, I32Add, LocalSet(7),
        LocalGet(7), LocalGet(5), I32Const(4), I32Mul, I32Add, LocalSet(8),
    ]);
    for (input, length, pointer) in [(2, 4, 6), (3, 5, 7)] {
        code.extend([
            I32Const(0), LocalSet(10), Block(BlockType::Empty), Loop(BlockType::Empty),
            LocalGet(10), LocalGet(length), I32GeU, BrIf(1),
            LocalGet(pointer), LocalGet(10), I32Const(4), I32Mul, I32Add,
            LocalGet(input), LocalGet(10), ArrayGet(LIMBS), I32Store(word),
            LocalGet(10), I32Const(1), I32Add, LocalSet(10), Br(0), End, End,
        ]);
    }
    code.extend([
        I32Const(helper.stack_top), GlobalSet(helper.stack_global),
        LocalGet(6), LocalGet(4), LocalGet(7), LocalGet(5), LocalGet(8),
        Call(helper.multiply), LocalSet(11),
        LocalGet(9), ArrayNewDefault(LIMBS), LocalSet(12),
        I32Const(0), LocalSet(10), Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(10), LocalGet(9), I32GeU, BrIf(1),
        LocalGet(12), LocalGet(10),
        LocalGet(8), LocalGet(10), I32Const(4), I32Mul, I32Add, I32Load(word),
        ArraySet(LIMBS),
        LocalGet(10), I32Const(1), I32Add, LocalSet(10), Br(0), End, End,
    ]);
    code.extend(get(0, 0));
    code.extend(get(1, 0));
    code.extend([I32Mul, LocalGet(12), LocalGet(11), Call(b.names["bigint-new"])]);
    b.function_with_locals(
        "bigint-multiply-helper", &[VALUE, VALUE], &[VALUE],
        &[(2, array), (8, ValType::I32), (1, array)], &code,
    );

    // Initial crossover from the magnitude probe: never bridge one-limb
    // products, and leave the small balanced products in GC. Production
    // wrapper measurements must validate/refine this boundary.
    let mut code = get(0, 2).to_vec();
    code.push(LocalSet(2));
    code.extend(get(1, 2));
    code.extend([
        LocalSet(3),
        LocalGet(2), I32Const(2), I32GeU,
        LocalGet(3), I32Const(2), I32GeU, I32And,
        LocalGet(2), I64ExtendI32U, LocalGet(3), I64ExtendI32U, I64Mul,
        I64Const(16), I64GeU, I32And,
        If(BlockType::Result(VALUE)),
        LocalGet(0), LocalGet(1), Call(b.names["bigint-multiply-helper"]),
        Else,
        LocalGet(0), LocalGet(1), Call(b.names["bigint-multiply-gc"]),
        End,
    ]);
    b.function_with_locals(
        "bigint-multiply", &[VALUE, VALUE], &[VALUE],
        &[(2, ValType::I32)], &code,
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

/// Integer category dispatch, low-bit conversions and Clojure hasheq.
/// The fourth field retains BigInteger's plain printing; arithmetic returns
/// BigInt. Both use the same exact integer category and magnitude storage.
pub(super) fn integer_functions(b: &mut Builder) {
    let array = ValType::Ref(RefType {
        nullable: true,
        heap_type: HeapType::Concrete(LIMBS),
    });
    b.function(
        "integer-test",
        &[VALUE],
        &[ValType::I32],
        &[
            LocalGet(0),
            Call(b.names["long-test"]),
            LocalGet(0),
            Call(b.names["bigint-test"]),
            I32Or,
        ],
    );
    let mut code = vec![
        LocalGet(0),
        Call(b.names["integer-test"]),
        I32Eqz,
        If(BlockType::Empty),
    ];
    code.extend(numeric::error(
        "bigint requires an integer operand",
        nominal::ERROR_GLOBAL,
    ));
    code.extend([
        End,
        LocalGet(0),
        Call(b.names["integer-bigint-base"]),
        LocalSet(0),
    ]);
    code.extend(get(0, 3));
    code.extend([If(BlockType::Empty)]);
    code.extend(get(0, 0));
    code.extend(get(0, 1));
    code.extend(get(0, 2));
    code.extend([Call(b.names["bigint-new"]), Return, End, LocalGet(0)]);
    b.function("integer-bigint", &[VALUE], &[VALUE], &code);
    let mut code = vec![LocalGet(0), Call(b.names["bigint-test"]), If(BlockType::Empty)];
    code.extend(get(0, 3));
    code.extend([If(BlockType::Empty), LocalGet(0), Return, End, End,
        LocalGet(0), Call(b.names["integer-bigint"]), LocalSet(0)]);
    code.extend(get(0, 0));
    code.extend(get(0, 1));
    code.extend(get(0, 2));
    code.extend([I32Const(1), StructNew(BIGINT)]);
    b.function("integer-biginteger", &[VALUE], &[VALUE], &code);
    b.function(
        "integer-compare",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[
            LocalGet(0),
            Call(b.names["integer-bigint"]),
            LocalGet(1),
            Call(b.names["integer-bigint"]),
            Call(b.names["bigint-compare"]),
        ],
    );
    let mut code = get(0, 1).to_vec();
    code.push(LocalSet(1));
    code.extend(get(0, 2));
    code.push(LocalSet(2));
    code.extend([
        LocalGet(2),
        If(BlockType::Empty),
        LocalGet(1),
        I32Const(0),
        ArrayGet(LIMBS),
        I64ExtendI32U,
        LocalSet(3),
        End,
        LocalGet(2),
        I32Const(1),
        I32GtU,
        If(BlockType::Empty),
        LocalGet(3),
        LocalGet(1),
        I32Const(1),
        ArrayGet(LIMBS),
        I64ExtendI32U,
        I64Const(32),
        I64Shl,
        I64Or,
        LocalSet(3),
        End,
    ]);
    code.extend(get(0, 0));
    code.extend([
        I32Const(0),
        I32LtS,
        If(BlockType::Result(ValType::I64)),
        I64Const(0),
        LocalGet(3),
        I64Sub,
        Else,
        LocalGet(3),
        End,
    ]);
    b.function_with_locals(
        "bigint-low-long",
        &[VALUE],
        &[ValType::I64],
        &[(1, array), (1, ValType::I32), (1, ValType::I64)],
        &code,
    );
    let mut code = get(0, 2).to_vec();
    code.extend([
        I32Const(2),
        I32GtU,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        LocalGet(0),
        Call(b.names["bigint-low-long"]),
        LocalSet(1),
    ]);
    code.extend(get(0, 0));
    code.extend([
        I32Const(0),
        I32LtS,
        If(BlockType::Result(ValType::I32)),
        LocalGet(1),
        I64Const(0),
        I64LeS,
        Else,
        LocalGet(1),
        I64Const(0),
        I64GeS,
        End,
    ]);
    b.function_with_locals(
        "bigint-fits-long",
        &[VALUE],
        &[ValType::I32],
        &[(1, ValType::I64)],
        &code,
    );
    let mut unsigned = get(0, 0).to_vec();
    unsigned.extend([I32Const(0), I32GeS]);
    unsigned.extend(get(0, 2));
    unsigned.extend([I32Const(2), I32LeU, I32And]);
    b.function("bigint-fits-u64", &[VALUE], &[ValType::I32], &unsigned);
    let mut code = vec![
        LocalGet(0),
        Call(b.names["bigint-fits-long"]),
        If(BlockType::Empty),
        LocalGet(0),
        Call(b.names["bigint-low-long"]),
        LocalSet(3),
    ];
    code.extend(longs::hash_long_in(3, 4));
    code.extend([Call(b.names["long-box-i32"]), Return, End]);
    let word_start = code.len();
    code.extend(get(0, 1));
    code.push(LocalSet(1));
    code.extend(get(0, 2));
    code.push(LocalSet(2));
    code.extend([
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(2),
        I32Eqz,
        BrIf(1),
        LocalGet(2),
        I32Const(1),
        I32Sub,
        LocalSet(2),
        LocalGet(4),
        I32Const(31),
        I32Mul,
    ]);
    code.extend(limb(1, 2));
    code.extend([I32Add, LocalSet(4), Br(0), End, End, LocalGet(4)]);
    code.extend(get(0, 0));
    code.extend([I32Mul, Call(b.names["long-box-i32"])]);
    b.function_with_locals("bigint-word-hash", &[VALUE], &[VALUE],
        &[(1, array), (1, ValType::I32), (1, ValType::I64), (1, ValType::I32)], &code[word_start..]);
    b.function_with_locals(
        "bigint-hash",
        &[VALUE],
        &[VALUE],
        &[
            (1, array),
            (1, ValType::I32),
            (1, ValType::I64),
            (1, ValType::I32),
        ],
        &code,
    );
}

/// Original binary magnitude division. Remainder scratch and quotient limbs
/// stay private until the final normalized values are published. This GC
/// reference kernel establishes exact semantics before the numeric-helper port.
pub(super) fn division(b: &mut Builder) {
    let array = ValType::Ref(RefType {
        nullable: true,
        heap_type: HeapType::Concrete(LIMBS),
    });
    // 2 a, 3 b, 4 quotient, 5 remainder; 6 la, 7 lb, 8 bit, 9 word,
    // 10 comparison, 11 x, 12 y; 13 carry/borrow, 14 digit.
    let mut code = Vec::new();
    for (object, target, field) in [(0, 2, 1), (1, 3, 1), (0, 6, 2), (1, 7, 2)] {
        code.extend(get(object, field));
        code.push(LocalSet(target));
    }
    code.extend([LocalGet(7), I32Eqz, If(BlockType::Empty)]);
    code.extend(numeric::error("Divide by zero", nominal::ERROR_GLOBAL));
    code.extend([
        End,
        LocalGet(6),
        ArrayNewDefault(LIMBS),
        LocalSet(4),
        LocalGet(7),
        I32Const(1),
        I32Add,
        ArrayNewDefault(LIMBS),
        LocalSet(5),
        LocalGet(6),
        I32Const(32),
        I32Mul,
        I32Const(1),
        I32Sub,
        LocalSet(8),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(8),
        I32Const(0),
        I32LtS,
        BrIf(1),
        // Carry starts with the next dividend bit.
        LocalGet(2),
        LocalGet(8),
        I32Const(5),
        I32ShrU,
        ArrayGet(LIMBS),
        LocalGet(8),
        I32Const(31),
        I32And,
        I32ShrU,
        I32Const(1),
        I32And,
        I64ExtendI32U,
        LocalSet(13),
        I32Const(0),
        LocalSet(9),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(9),
        LocalGet(7),
        I32GtU,
        BrIf(1),
    ]);
    code.extend(limb(5, 9));
    code.extend([
        I64ExtendI32U,
        I64Const(1),
        I64Shl,
        LocalGet(13),
        I64Or,
        LocalSet(14),
        LocalGet(5),
        LocalGet(9),
        LocalGet(14),
        I32WrapI64,
        ArraySet(LIMBS),
        LocalGet(14),
        I64Const(32),
        I64ShrU,
        LocalSet(13),
        LocalGet(9),
        I32Const(1),
        I32Add,
        LocalSet(9),
        Br(0),
        End,
        End,
        // Compare fixed-width remainder scratch with the divisor, high first.
        I32Const(0),
        LocalSet(10),
        LocalGet(7),
        I32Const(1),
        I32Add,
        LocalSet(9),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(9),
        I32Eqz,
        BrIf(1),
        LocalGet(9),
        I32Const(1),
        I32Sub,
        LocalSet(9),
    ]);
    code.extend(limb(5, 9));
    code.extend([
        LocalSet(11),
        LocalGet(9),
        LocalGet(7),
        I32LtU,
        If(BlockType::Result(ValType::I32)),
    ]);
    code.extend(limb(3, 9));
    code.extend([
        Else,
        I32Const(0),
        End,
        LocalSet(12),
        LocalGet(11),
        LocalGet(12),
        I32Ne,
        If(BlockType::Empty),
        LocalGet(11),
        LocalGet(12),
        I32GtU,
        If(BlockType::Result(ValType::I32)),
        I32Const(1),
        Else,
        I32Const(-1),
        End,
        LocalSet(10),
        Br(2),
        End,
        Br(0),
        End,
        End,
        LocalGet(10),
        I32Const(0),
        I32GeS,
        If(BlockType::Empty),
        // R >= B: subtract once, then set this quotient bit.
        I64Const(0),
        LocalSet(13),
        I32Const(0),
        LocalSet(9),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(9),
        LocalGet(7),
        I32GtU,
        BrIf(1),
    ]);
    code.extend(limb(5, 9));
    code.extend([
        I64ExtendI32U,
        LocalGet(13),
        I64Sub,
        LocalGet(9),
        LocalGet(7),
        I32LtU,
        If(BlockType::Result(ValType::I64)),
    ]);
    code.extend(limb(3, 9));
    code.extend([
        I64ExtendI32U,
        Else,
        I64Const(0),
        End,
        I64Sub,
        LocalSet(14),
        LocalGet(5),
        LocalGet(9),
        LocalGet(14),
        I32WrapI64,
        ArraySet(LIMBS),
        LocalGet(14),
        I64Const(0),
        I64LtS,
        I64ExtendI32U,
        LocalSet(13),
        LocalGet(9),
        I32Const(1),
        I32Add,
        LocalSet(9),
        Br(0),
        End,
        End,
        LocalGet(8),
        I32Const(5),
        I32ShrU,
        LocalSet(9),
        LocalGet(4),
        LocalGet(9),
    ]);
    code.extend(limb(4, 9));
    code.extend([
        I32Const(1),
        LocalGet(8),
        I32Const(31),
        I32And,
        I32Shl,
        I32Or,
        ArraySet(LIMBS),
        End,
        LocalGet(8),
        I32Const(1),
        I32Sub,
        LocalSet(8),
        Br(0),
        End,
        End,
    ]);
    code.extend(get(0, 0));
    code.extend(get(1, 0));
    code.extend([
        I32Mul,
        LocalGet(4),
        LocalGet(6),
        Call(b.names["bigint-new"]),
    ]);
    code.extend(get(0, 0));
    code.extend([
        LocalGet(5),
        LocalGet(7),
        I32Const(1),
        I32Add,
        Call(b.names["bigint-new"]),
    ]);
    let divrem = b.function_with_locals(
        "bigint-divrem",
        &[VALUE, VALUE],
        &[VALUE, VALUE],
        &[(4, array), (7, ValType::I32), (2, ValType::I64)],
        &code,
    );
    b.function(
        "bigint-quot",
        &[VALUE, VALUE],
        &[VALUE],
        &[LocalGet(0), LocalGet(1), Call(divrem), Drop],
    );
    b.function_with_locals(
        "bigint-rem",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &[
            LocalGet(0),
            LocalGet(1),
            Call(divrem),
            LocalSet(2),
            Drop,
            LocalGet(2),
        ],
    );
}
