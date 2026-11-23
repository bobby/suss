//! Original exact Decimal Number.longValue low bits. Checked casts use the
//! separately specified Clojure RT.longCast(doubleValue) path.
use super::arithmetic::{field, sign};
use super::*;

pub(super) fn functions(b: &mut Builder) {
    // value0; coefficient1; signed scale2; wrapped result3 and power4.
    let mut code = field(0, 0).to_vec();
    code.push(LocalSet(1));
    code.extend(field(0, 1));
    code.extend([I64ExtendI32S, LocalSet(2)]);
    code.extend(sign(1));
    code.extend([
        I32Eqz,
        If(BlockType::Empty),
        I64Const(0),
        Return,
        End,
        LocalGet(2),
        I64Const(0),
        I64GtS,
        If(BlockType::Empty),
        LocalGet(2),
        LocalGet(1),
        Call(b.names["decimal-integer-precision"]),
        I64ExtendI32U,
        I64GeU,
        If(BlockType::Empty),
        I64Const(0),
        Return,
        End,
        LocalGet(1),
        LocalGet(2),
        Call(b.names["decimal-power-ten"]),
        Call(b.names["bigint-quot"]),
        Call(b.names["bigint-low-long"]),
        Return,
        End,
        // Every 10^k with k>=64 is zero modulo 2^64. Do not allocate its
        // enormous exact magnitude merely to obtain a primitive low word.
        LocalGet(2),
        I64Const(-64),
        I64LeS,
        If(BlockType::Empty),
        I64Const(0),
        Return,
        End,
        LocalGet(1),
        Call(b.names["bigint-low-long"]),
        LocalSet(3),
        I64Const(10),
        LocalSet(4),
        I64Const(0),
        LocalGet(2),
        I64Sub,
        LocalSet(2),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(2),
        I64Eqz,
        BrIf(1),
        LocalGet(2),
        I64Const(1),
        I64And,
        I64Eqz,
        If(BlockType::Empty),
        Else,
        LocalGet(3),
        LocalGet(4),
        I64Mul,
        LocalSet(3),
        End,
        LocalGet(2),
        I64Const(1),
        I64ShrU,
        LocalSet(2),
        LocalGet(4),
        LocalGet(4),
        I64Mul,
        LocalSet(4),
        Br(0),
        End,
        End,
        LocalGet(3),
    ]);
    b.function_with_locals(
        "decimal-low-long",
        &[VALUE],
        &[ValType::I64],
        &[(1, VALUE), (3, ValType::I64)],
        &code,
    );
}
