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
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            sentinel_value(NIL), StructNew(DESCRIPTOR),
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
            // A long is a number too.
            if ty == NUMBER {
                Call(b.names["number-test"])
            } else {
                RefTestNonNull(HeapType::Concrete(ty))
            },
            If(BlockType::Empty),
            Call(b.names[constructor]),
            Return,
            End,
        ]);
    }
    body.extend([
        LocalGet(0),
        sentinel_value(FALSE),
        RefEq,
        LocalGet(0),
        sentinel_value(TRUE),
        RefEq,
        I32Or,
        If(BlockType::Empty),
        Call(b.names["boolean-constructor"]),
        Return,
        End,
    ]);
    // A packed primitive array's type value, by its storage (#336).
    for (kind, (name, _, _, _)) in packed_arrays::KINDS.iter().enumerate() {
        body.extend([
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(PACKED)),
            If(BlockType::Empty),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(PACKED)),
            StructGet {
                struct_type_index: PACKED,
                field_index: 0,
            },
            RefTestNonNull(HeapType::Concrete(packed_arrays::storage(kind))),
            If(BlockType::Empty),
            Call(b.names[&format!("packed-array-type-{name}")]),
            Return,
            End,
            End,
        ]);
    }
    body.extend([
        LocalGet(0),
        Call(b.names["source-array?"]),
        If(BlockType::Empty),
    ]);
    // The pinned type form reads the live constructor property. An owned Array
    // can shadow the canonical constructor with any value, even nil/false.
    // Do not replace that property read with an immutable kind comparison.
    fn constructor_name(body: &mut Vec<Instruction<'static>>) {
        for unit in "constructor".encode_utf16() {
            body.push(Instruction::I32Const(i32::from(unit)));
        }
        body.push(Instruction::ArrayNewFixed {
            array_type_index: STRING,
            array_size: 11,
        });
    }
    body.push(LocalGet(0));
    constructor_name(&mut body);
    body.extend([
        Call(b.names["source-array-property-has"]),
        If(BlockType::Empty),
        LocalGet(0),
    ]);
    constructor_name(&mut body);
    body.extend([
        Call(b.names["source-array-property-get"]),
        Return,
        End,
        Call(b.names["array-constructor"]),
        Return,
        End,
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(8)),
        If(BlockType::Empty),
        LocalGet(0),
        Call(b.names["language-error-canonical?"]),
        If(BlockType::Empty),
        LocalGet(0),
    ]);
    constructor_name(&mut body);
    body.extend([
        Call(b.names["named-property-get"]),
        Return,
        End,
        End,
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(7)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    // Function and native-object constructor properties
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
    body.extend([End, LocalGet(0)]);
    constructor_name(&mut body);
    body.push(Call(b.names["named-property-get"]));
    b.function_with_locals(
        "value-constructor",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &body,
    );
}
