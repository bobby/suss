//! Original Java hashCode adapters for Clojure case, distinct from hasheq.
//!
//! Pinned Clojure 1.12.6 uses Util.hash in case's hash dispatch. In particular
//! Decimal scale, the BigInteger carrier, and a Double's signed zero matter.
//! Bytecode/primary controls and source provenance: results/m12-case-dispatch.
//! No MathContext lookup, coercion through f64, mutable hash cache or host call.
use super::*;
use Instruction::*;

pub(super) fn intrinsics(b: &mut Builder) {
    let bigint = HeapType::Concrete(BIGINT);
    let decimal = HeapType::Concrete(DECIMAL);
    let mut code = vec![
        LocalGet(0), Call(b.names["long-test"]), If(BlockType::Empty),
        LocalGet(0), Call(b.names["long-unbox"]), LocalSet(1),
    ];
    code.extend(long_hash(1));
    code.extend([Call(b.names["long-box-i32"]), Return, End,
        LocalGet(0), Call(b.names["bigint-test"]), If(BlockType::Empty),
        // BigInteger always uses its signed-magnitude word hash. BigInt
        // uses Long.hashCode when represented by its long part.
        LocalGet(0), RefCastNonNull(bigint),
        StructGet { struct_type_index: BIGINT, field_index: 3 }, I32Eqz,
        LocalGet(0), Call(b.names["bigint-fits-long"]), I32And,
        If(BlockType::Empty),
        LocalGet(0), Call(b.names["bigint-low-long"]), LocalSet(1),
    ]);
    code.extend(long_hash(1));
    code.extend([Call(b.names["long-box-i32"]), Return, End,
        LocalGet(0), Call(b.names["bigint-word-hash"]), Return, End,
        LocalGet(0), Call(b.names["ratio-test"]), If(BlockType::Empty),
        LocalGet(0), Call(b.names["ratio-hash"]), Return, End,
        LocalGet(0), Call(b.names["decimal-test"]), If(BlockType::Empty),
        LocalGet(0), RefCastNonNull(decimal),
        StructGet { struct_type_index: DECIMAL, field_index: 0 },
        Call(b.names["bigint-word-hash"]), Call(b.names["long-unbox"]), I32WrapI64,
        I32Const(31), I32Mul,
        LocalGet(0), RefCastNonNull(decimal),
        StructGet { struct_type_index: DECIMAL, field_index: 1 }, I32Add,
        Call(b.names["long-box-i32"]), Return, End,
        LocalGet(0), Call(b.names["double-test"]), If(BlockType::Empty),
        LocalGet(0), Call(b.names["number-unbox"]), LocalTee(2),
        LocalGet(2), F64Ne,
        If(BlockType::Result(ValType::I64)),
        I64Const(0x7ff8_0000_0000_0000), // Double.doubleToLongBits canonical NaN.
        Else, LocalGet(2), I64ReinterpretF64, End, LocalSet(1),
    ]);
    code.extend(long_hash(1));
    code.extend([Call(b.names["long-box-i32"]), Return, End]);
    code.extend(numeric::error("case numeric hash requires a number operand", nominal::ERROR_GLOBAL));
    b.function_with_locals("case-number-hash", &[VALUE], &[VALUE],
        &[(1, ValType::I64), (1, ValType::F64)], &code);
}

fn long_hash(local: u32) -> [Instruction<'static>; 6] {
    [LocalGet(local), LocalGet(local), I64Const(32), I64ShrU, I64Xor, I32WrapI64]
}
