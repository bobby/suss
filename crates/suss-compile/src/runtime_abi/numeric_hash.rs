//! Original scalar binary64 byte adapters for retained hash-double.
//! No linear-memory buffers or JavaScript typed-array objects escape this slice.
use super::*;
pub(super) fn intrinsics(b: &mut Builder) {
    use Instruction::*;
    // Original numeric milliseconds adapter for portable Date storage.
    // ECMAScript TimeClip: reject nonfinite/out-of-range, truncate, normalize zero.
    let mut time_clip = vec![LocalGet(0), RefTestNonNull(HeapType::Concrete(NUMBER)),
        I32Eqz, If(BlockType::Empty)];
    nominal::error(&mut time_clip);
    time_clip.extend([End, LocalGet(0), RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet { struct_type_index: NUMBER, field_index: 0 }, LocalSet(1),
        LocalGet(1), F64Abs, F64Const(8640000000000000.0.into()), F64Le,
        If(BlockType::Result(ValType::F64)),
        LocalGet(1), F64Trunc, F64Const(0.0.into()), F64Add,
        Else, F64Const(f64::NAN.into()), End, Call(b.names["number-box"])]);
    b.function_with_locals("primitive-f64-time-clip", &[VALUE], &[VALUE],
        &[(1, ValType::F64)], &time_clip);
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
    b.function(
        "primitive-f64-floor",
        &[VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            Call(b.names["coerce-number"]),
            F64Floor,
            Call(b.names["number-box"]),
        ],
    );
    b.function(
        "primitive-f64-finite",
        &[VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            Call(b.names["coerce-number"]),
            F64Abs,
            F64Const(f64::INFINITY.into()),
            F64Lt,
            I32Const(1),
            I32Shl,
            I32Const(2),
            I32Add,
            RefI31,
        ],
    );
    // Number.isSafeInteger performs no coercion. Other owned Value kinds
    // return false; finite integral Number values must fit +/- (2^53 - 1).
    b.function_with_locals(
        "primitive-f64-safe-integer",
        &[VALUE],
        &[VALUE],
        &[(1, ValType::F64)],
        &[
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(NUMBER)),
            If(BlockType::Result(VALUE)),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(NUMBER)),
            StructGet {
                struct_type_index: NUMBER,
                field_index: 0,
            },
            LocalSet(1),
            LocalGet(1),
            LocalGet(1),
            F64Trunc,
            F64Eq,
            LocalGet(1),
            F64Abs,
            F64Const(9007199254740991.0.into()),
            F64Le,
            I32And,
            I32Const(1),
            I32Shl,
            I32Const(2),
            I32Add,
            RefI31,
            Else,
            I32Const(2),
            RefI31,
            End,
        ],
    );
    // This private adapter is only for the integral safe-number branch of
    // retained hash. Exact i64 remainder avoids rounded division/subtraction.
    let mut body = Vec::new();
    for (input, slot) in [(0, 2), (1, 3)] {
        body.extend([
            LocalGet(input),
            Call(b.names["coerce-number"]),
            LocalSet(slot),
            LocalGet(slot),
            LocalGet(slot),
            F64Trunc,
            F64Ne,
            LocalGet(slot),
            F64Abs,
            F64Const(9007199254740991.0.into()),
            F64Le,
            I32Eqz,
            I32Or,
            If(BlockType::Empty),
        ]);
        nominal::error(&mut body);
        body.push(End);
    }
    body.extend([
        LocalGet(3),
        F64Const(0.0.into()),
        F64Eq,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(2),
        I64TruncF64S,
        LocalGet(3),
        I64TruncF64S,
        I64RemS,
        F64ConvertI64S,
        LocalGet(2),
        F64Copysign,
        Call(b.names["number-box"]),
    ]);
    b.function_with_locals(
        "primitive-safe-integer-remainder",
        &[VALUE, VALUE],
        &[VALUE],
        &[(2, ValType::F64)],
        &body,
    );
}
