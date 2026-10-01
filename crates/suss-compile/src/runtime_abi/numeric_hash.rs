//! Original scalar binary64 byte adapters for retained hash-double.
//! No linear-memory buffers or JavaScript typed-array objects escape this slice.
use super::*;
pub(super) fn intrinsics(b: &mut Builder) {
    use Instruction::*;
    b.function(
        "primitive-f64-coerce",
        &[VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            Call(b.names["coerce-number"]),
            Call(b.names["number-box"]),
        ],
    );
    for (name, offset) in [("primitive-f64-word0", 0), ("primitive-f64-word4", 32)] {
        let mut body = vec![
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(NUMBER)),
            I32Eqz,
            If(BlockType::Empty),
        ];
        nominal::error(&mut body);
        body.extend([
            End,
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(NUMBER)),
            StructGet {
                struct_type_index: NUMBER,
                field_index: 0,
            },
            I64ReinterpretF64,
            I64Const(offset),
            I64ShrU,
            I32WrapI64,
            LocalSet(1),
            // Float64Array stores little-endian bytes; DataView reads each
            // four-byte word as big-endian by default. Swap each word exactly.
            LocalGet(1),
            I32Const(0xff),
            I32And,
            I32Const(24),
            I32Shl,
            LocalGet(1),
            I32Const(0xff00),
            I32And,
            I32Const(8),
            I32Shl,
            I32Or,
            LocalGet(1),
            I32Const(8),
            I32ShrU,
            I32Const(0xff00),
            I32And,
            I32Or,
            LocalGet(1),
            I32Const(24),
            I32ShrU,
            I32Or,
            F64ConvertI32S,
            Call(b.names["number-box"]),
        ]);
        b.function_with_locals(name, &[VALUE], &[VALUE], &[(1, ValType::I32)], &body);
    }
}
