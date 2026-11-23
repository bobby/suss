//! Original finite-precision addition with bounded alignment.
//! A distant operand can be represented by a signed sticky digit below both
//! the other coefficient's last digit and the requested rounding position.
//! It cannot cross an existing digit boundary, but preserves tie direction,
//! directed rounding and UNNECESSARY without constructing huge powers of ten.
use super::arithmetic::{field, sign};
use super::*;

pub(super) fn functions(b: &mut Builder) {
    // x0,y1,p2,mode3; coefficients4,5; scales6,7; threshold8,high9;
    // swap coefficient10 and scale11.
    let mut code = field(0, 0).to_vec();
    code.push(LocalSet(4));
    code.extend(field(1, 0));
    code.push(LocalSet(5));
    code.extend(field(0, 1));
    code.extend([I64ExtendI32S, LocalSet(6)]);
    code.extend(field(1, 1));
    code.extend([
        I64ExtendI32S,
        LocalSet(7),
        LocalGet(6),
        LocalGet(7),
        I64GtS,
        If(BlockType::Empty),
        LocalGet(4),
        LocalSet(10),
        LocalGet(5),
        LocalSet(4),
        LocalGet(10),
        LocalSet(5),
        LocalGet(6),
        LocalSet(11),
        LocalGet(7),
        LocalSet(6),
        LocalGet(11),
        LocalSet(7),
        End,
        LocalGet(2),
        If(BlockType::Empty),
    ]);
    code.extend(sign(4));
    code.push(If(BlockType::Empty));
    code.extend([
        LocalGet(6),
        LocalGet(4),
        Call(b.names["decimal-integer-precision"]),
        I64ExtendI32U,
        I64Sub,
        LocalGet(2),
        I64ExtendI32U,
        I64Add,
        LocalSet(8),
        LocalGet(8),
        LocalGet(6),
        I64LtS,
        If(BlockType::Empty),
        LocalGet(6),
        LocalSet(8),
        End,
        LocalGet(7),
        LocalGet(5),
        Call(b.names["decimal-integer-precision"]),
        I64ExtendI32U,
        I64Sub,
        I64Const(1),
        I64Add,
        LocalSet(9),
        LocalGet(9),
        LocalGet(8),
        I64Const(2),
        I64Add,
        I64GtS,
        If(BlockType::Empty),
    ]);
    code.extend(sign(5));
    code.extend([
        I64ExtendI32S,
        Call(b.names["bigint-from-long"]),
        LocalSet(5),
        LocalGet(8),
        I64Const(3),
        I64Add,
        LocalSet(7),
        End,
        End,
        End,
    ]);
    // A zero coefficient never needs alignment storage, even without precision.
    code.extend([LocalGet(6), LocalGet(7), I64Ne, If(BlockType::Empty)]);
    code.extend(sign(4));
    code.push(If(BlockType::Empty));
    code.extend([
        LocalGet(4),
        LocalGet(7),
        LocalGet(6),
        I64Sub,
        Call(b.names["decimal-power-ten"]),
        Call(b.names["bigint-multiply"]),
        LocalSet(4),
        End,
        End,
        LocalGet(4),
        LocalGet(5),
        Call(b.names["bigint-add"]),
        LocalGet(7),
        Call(b.names["decimal-new"]),
        LocalGet(2),
        LocalGet(3),
        Call(b.names["decimal-round"]),
    ]);
    b.function_with_locals(
        "decimal-add-rounded",
        &[VALUE, VALUE, ValType::I32, ValType::I32],
        &[VALUE],
        &[(2, VALUE), (4, ValType::I64), (1, VALUE), (1, ValType::I64)],
        &code,
    );
}
