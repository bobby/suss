//! Original bounded constructor identity storage for retained source types.
//! A private descriptor key roots each constructor in its own type descriptor;
//! no global owner registry, source name lookup, or recursive ABI layout change.
use super::*;

pub(super) const KEY_GLOBAL: u32 = r#async::STREAM_GLOBAL_BASE + streams::GLOBAL_COUNT;

pub(super) fn append_globals(globals: &mut GlobalSection) {
    use Instruction::*;
    globals.global(
        GlobalType {
            val_type: reference(DESCRIPTOR),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            I64Const(0),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            RefI31,
            I32Const(0),
            RefI31,
            StructNew(DESCRIPTOR),
        ]),
    );
}

pub(super) fn functions(b: &mut Builder) {
    use Instruction::*;
    let mut body = vec![];
    for (ty, constructor) in [
        (NUMBER, "number-constructor"),
        (STRING, "string-constructor"),
    ] {
        body.extend([
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(ty)),
            If(BlockType::Empty),
            Call(b.names[constructor]),
            Return,
            End,
        ]);
    }
    body.extend([
        LocalGet(0),
        I32Const(2),
        RefI31,
        RefEq,
        LocalGet(0),
        I32Const(4),
        RefI31,
        RefEq,
        I32Or,
        If(BlockType::Empty),
        Call(b.names["boolean-constructor"]),
        Return,
        End,
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(7)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    // Array/function constructors and native-object constructor properties
    // remain an explicit unsupported boundary, never kind markers.
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(7)),
        StructGet {
            struct_type_index: 7,
            field_index: 0,
        },
        GlobalGet(KEY_GLOBAL),
        Call(b.names["protocol-method-get"]),
        LocalSet(1),
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(4)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.extend([End, LocalGet(1)]);
    b.function_with_locals(
        "value-constructor",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &body,
    );
}
