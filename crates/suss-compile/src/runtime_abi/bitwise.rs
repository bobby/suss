//! Original scalar ToInt32 primitive for retained hashing dependencies.
//! Public core forms and object-to-primitive conversion are separate work.
use super::*;

pub(super) fn intrinsics(b: &mut Builder) {
    use Instruction::*;
    let convert = b.names["coerce-number"];
    b.function_with_locals(
        "coerce-int32",
        &[VALUE],
        &[ValType::I32],
        &[(1, ValType::F64)],
        &[
            LocalGet(0),
            Call(convert),
            F64Trunc,
            LocalSet(1),
            // NaN and either infinity must return zero before a Wasm integer
            // conversion. Signed zero/subnormal inputs also become zero.
            LocalGet(1),
            F64Abs,
            F64Const(f64::INFINITY.into()),
            F64Lt,
            If(BlockType::Result(ValType::I32)),
            // All finite f64 values with magnitude >= 2^84 are multiples of
            // 2^32. Below that threshold these power-of-two operations retain
            // the exact integral remainder, including negative inputs.
            LocalGet(1),
            LocalGet(1),
            F64Const(4294967296.0.into()),
            F64Div,
            F64Floor,
            F64Const(4294967296.0.into()),
            F64Mul,
            F64Sub,
            I32TruncF64U,
            Else,
            I32Const(0),
            End,
        ],
    );
}
