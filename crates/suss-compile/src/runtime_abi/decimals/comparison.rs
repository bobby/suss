//! Original Decimal comparison, scale-sensitive case equality and hasheq.
//! Compare adjusted exponents before aligning magnitudes, so extreme scales
//! never require a power-of-ten allocation merely to decide their order.
use super::arithmetic::{field, integer, sign};
use super::*;

pub(super) fn functions(b: &mut Builder) {
    compare_values(b);
    let mut code = vec![Call(b.names["decimal-context"]), LocalSet(3), LocalSet(2)];
    for (input, output) in [(0, 4), (1, 5)] {
        code.extend([
            LocalGet(input),
            LocalGet(2),
            LocalGet(3),
            Call(b.names["decimal-from-number"]),
            LocalSet(output),
        ]);
    }
    code.extend([
        LocalGet(4),
        LocalGet(5),
        Call(b.names["decimal-compare-values"]),
    ]);
    b.function_with_locals(
        "decimal-compare",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[(2, ValType::I32), (2, VALUE)],
        &code,
    );

    // Java BigDecimal.equals, used by Clojure case, preserves the scale.
    let mut code = field(0, 1).to_vec();
    code.extend(field(1, 1));
    code.extend([I32Eq, If(BlockType::Result(ValType::I32))]);
    code.extend(field(0, 0));
    code.extend(field(1, 0));
    code.extend([
        Call(b.names["bigint-compare"]),
        I32Eqz,
        Else,
        I32Const(0),
        End,
    ]);
    b.function(
        "decimal-case-equal",
        &[VALUE, VALUE],
        &[ValType::I32],
        &code,
    );
    hash(b);
}

fn compare_values(b: &mut Builder) {
    // x0,y1; coefficients2,3; signs4,5; adjusted exponents6,7.
    let mut code = field(0, 0).to_vec();
    code.push(LocalSet(2));
    code.extend(field(1, 0));
    code.push(LocalSet(3));
    code.extend(sign(2));
    code.push(LocalSet(4));
    code.extend(sign(3));
    code.push(LocalSet(5));
    code.extend([
        LocalGet(4),
        LocalGet(5),
        I32Ne,
        If(BlockType::Empty),
        LocalGet(4),
        LocalGet(5),
        I32GtS,
        LocalGet(4),
        LocalGet(5),
        I32LtS,
        I32Sub,
        Return,
        End,
        LocalGet(4),
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ]);
    for (value, coefficient, output) in [(0, 2, 6), (1, 3, 7)] {
        code.extend([
            LocalGet(coefficient),
            Call(b.names["decimal-integer-precision"]),
            I64ExtendI32U,
        ]);
        code.extend(field(value, 1));
        code.extend([I64ExtendI32S, I64Sub, LocalSet(output)]);
    }
    code.extend([
        LocalGet(6),
        LocalGet(7),
        I64Ne,
        If(BlockType::Empty),
        LocalGet(6),
        LocalGet(7),
        I64GtS,
        LocalGet(6),
        LocalGet(7),
        I64LtS,
        I32Sub,
        LocalGet(4),
        I32Mul,
        Return,
        End,
    ]);
    code.extend(field(0, 1));
    code.push(I64ExtendI32S);
    code.extend(field(1, 1));
    code.extend([
        I64ExtendI32S,
        I64Sub,
        LocalSet(6),
        LocalGet(6),
        I64Const(0),
        I64GtS,
        If(BlockType::Empty),
        LocalGet(3),
        LocalGet(6),
        Call(b.names["decimal-power-ten"]),
        Call(b.names["bigint-multiply"]),
        LocalSet(3),
        Else,
        LocalGet(6),
        I64Const(0),
        I64LtS,
        If(BlockType::Empty),
        LocalGet(2),
        I64Const(0),
        LocalGet(6),
        I64Sub,
        Call(b.names["decimal-power-ten"]),
        Call(b.names["bigint-multiply"]),
        LocalSet(2),
        End,
        End,
        LocalGet(2),
        LocalGet(3),
        Call(b.names["bigint-compare"]),
    ]);
    b.function_with_locals(
        "decimal-compare-values",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[(2, VALUE), (2, ValType::I32), (2, ValType::I64)],
        &code,
    );
}

fn hash(b: &mut Builder) {
    // hasheq is the Java hash of stripTrailingZeros, with every zero hashing0.
    // Work on owned immutable integers; never alter a Decimal or its coefficient.
    let mut code = field(0, 0).to_vec();
    code.push(LocalSet(1));
    code.extend(field(0, 1));
    code.extend([I64ExtendI32S, LocalSet(3)]);
    code.extend(sign(1));
    code.extend([
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Call(b.names["long-box-i32"]),
        Return,
        End,
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(1),
    ]);
    code.extend(integer(b, 10));
    code.extend([Call(b.names["bigint-rem"]), LocalSet(2)]);
    code.extend(sign(2));
    code.extend([
        BrIf(1),
        LocalGet(3),
        I64Const(i64::from(i32::MIN)),
        I64Eq,
        If(BlockType::Empty),
    ]);
    code.extend(numeric::error("Overflow", nominal::ERROR_GLOBAL));
    code.extend([End, LocalGet(1)]);
    code.extend(integer(b, 10));
    code.extend([
        Call(b.names["bigint-quot"]),
        LocalSet(1),
        LocalGet(3),
        I64Const(1),
        I64Sub,
        LocalSet(3),
        Br(0),
        End,
        End,
        LocalGet(1),
        Call(b.names["bigint-word-hash"]),
        Call(b.names["long-unbox"]),
        I32WrapI64,
        I32Const(31),
        I32Mul,
        LocalGet(3),
        I32WrapI64,
        I32Add,
        Call(b.names["long-box-i32"]),
    ]);
    b.function_with_locals(
        "decimal-hash",
        &[VALUE],
        &[VALUE],
        &[(2, VALUE), (1, ValType::I64)],
        &code,
    );
}
