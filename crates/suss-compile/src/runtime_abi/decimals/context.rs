//! A phase-owned getter reads the ordinary dynamic Var frame on every operation.
use super::*;

fn text(value: &str) -> Vec<Instruction<'static>> {
    let mut code = value
        .encode_utf16()
        .map(|u| I32Const(i32::from(u)))
        .collect::<Vec<_>>();
    code.push(ArrayNewFixed {
        array_type_index: STRING,
        array_size: value.encode_utf16().count() as u32,
    });
    code
}
fn error(message: &str) -> Vec<Instruction<'static>> {
    numeric::error(message, nominal::ERROR_GLOBAL)
}
pub(super) fn registration(b: &mut Builder) {
    let mut code = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(CLOSURE)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    code.extend(error("Math context getter must be a function"));
    code.push(End);
    // Reloading core constructs another closure over the same dynamic Var cell.
    // Keep the current phase's getter rooted, including after source reload.
    code.extend([LocalGet(0), GlobalSet(GETTER_ROOT), LocalGet(0)]);
    b.function("decimal-context-getter-install", &[VALUE], &[VALUE], &code);
}
pub(super) fn constructor(b: &mut Builder) {
    let mut code = vec![
        LocalGet(0),
        Call(b.names["long-test"]),
        I32Eqz,
        If(BlockType::Empty),
    ];
    code.extend(error(
        "No matching ctor found for class java.math.MathContext",
    ));
    code.push(End);
    code.extend([
        LocalGet(0),
        Call(b.names["number-cast-int"]),
        Call(b.names["long-unbox"]),
        I32WrapI64,
        LocalSet(2),
        LocalGet(2),
        I32Const(0),
        I32LtS,
        If(BlockType::Empty),
    ]);
    code.extend(error("Digits < 0"));
    code.push(End);
    code.extend([
        LocalGet(1),
        Call(b.names["long-test"]),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    code.extend(error("Invalid rounding mode"));
    code.push(End);
    code.extend([
        LocalGet(1),
        Call(b.names["long-unbox"]),
        LocalTee(3),
        I64Const(7),
        I64GtU,
        If(BlockType::Empty),
    ]);
    code.extend(error("Invalid rounding mode"));
    code.push(End);
    code.extend([
        LocalGet(2),
        LocalGet(3),
        I32WrapI64,
        StructNew(MATH_CONTEXT),
    ]);
    b.function_with_locals(
        "decimal-context-new",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, ValType::I32), (1, ValType::I64)],
        &code,
    );
}
pub(super) fn reader(b: &mut Builder) {
    let mut code = vec![
        GlobalGet(GETTER_ROOT),
        RefIsNull,
        If(BlockType::Empty),
        I32Const(0),
        I32Const(4),
        Return,
        End,
        GlobalGet(GETTER_ROOT),
        I32Const(0),
        ArrayNewDefault(ARGS),
        Call(b.names["invoke"]),
        LocalSet(0),
        LocalGet(0),
        RefIsNull,
        If(BlockType::Empty),
        I32Const(0),
        I32Const(4),
        Return,
        End,
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(MATH_CONTEXT)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    code.extend(error("Math context must be a MathContext"));
    code.push(End);
    for field_index in [0, 1] {
        code.extend([
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(MATH_CONTEXT)),
            StructGet {
                struct_type_index: MATH_CONTEXT,
                field_index,
            },
        ]);
    }
    b.function_with_locals(
        "decimal-context-impl",
        &[],
        &[ValType::I32, ValType::I32],
        &[(1, VALUE)],
        &code,
    );
}
pub(super) fn string(b: &mut Builder) {
    // Locals: context0, precision text1, mode text2, output3, text length4.
    let mut code = vec![
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(MATH_CONTEXT)),
        StructGet {
            struct_type_index: MATH_CONTEXT,
            field_index: 0,
        },
        I64ExtendI32U,
        Call(b.names["bigint-from-long"]),
        Call(b.names["integer-biginteger"]),
        Call(b.names["bigint-string"]),
        LocalSet(1),
    ];
    for (mode, name) in [
        "UP",
        "DOWN",
        "CEILING",
        "FLOOR",
        "HALF_UP",
        "HALF_DOWN",
        "HALF_EVEN",
        "UNNECESSARY",
    ]
    .iter()
    .enumerate()
    {
        code.extend([
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(MATH_CONTEXT)),
            StructGet {
                struct_type_index: MATH_CONTEXT,
                field_index: 1,
            },
            I32Const(mode as i32),
            I32Eq,
            If(BlockType::Empty),
        ]);
        code.extend(text(name));
        code.extend([LocalSet(2), End]);
    }
    code.extend([
        LocalGet(1),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
        LocalSet(4),
        LocalGet(4),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
        I32Add,
        I32Const(24),
        I32Add,
        ArrayNewDefault(STRING),
        LocalSet(3),
    ]);
    for (source, offset, length) in [(0, 0, 10), (1, 10, 0), (2, 10, 14), (3, 24, 0)] {
        code.extend([LocalGet(3), RefCastNonNull(HeapType::Concrete(STRING))]);
        if source <= 1 {
            code.push(I32Const(offset));
        } else {
            code.extend([LocalGet(4), I32Const(offset), I32Add]);
        }
        match source {
            0 => code.extend(text("precision=")),
            1 => code.extend([LocalGet(1), RefCastNonNull(HeapType::Concrete(STRING))]),
            2 => code.extend(text(" roundingMode=")),
            _ => code.extend([LocalGet(2), RefCastNonNull(HeapType::Concrete(STRING))]),
        }
        code.push(I32Const(0));
        match source {
            1 => code.push(LocalGet(4)),
            3 => code.extend([
                LocalGet(2),
                RefCastNonNull(HeapType::Concrete(STRING)),
                ArrayLen,
            ]),
            _ => code.push(I32Const(length)),
        }
        code.push(ArrayCopy {
            array_type_index_dst: STRING,
            array_type_index_src: STRING,
        });
    }
    code.push(LocalGet(3));
    b.function_with_locals(
        "decimal-context-string",
        &[VALUE],
        &[VALUE],
        &[(3, VALUE), (1, ValType::I32)],
        &code,
    );
}
