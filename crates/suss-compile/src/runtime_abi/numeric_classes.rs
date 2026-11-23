//! Portable numeric Class values, rooted as private canonical descriptors.
//! These are language type identities, without Java objects or a host registry.
use super::*;
use Instruction::*;

pub(crate) const NAMES: [&str; 8] = [
    "java.lang.Long",
    "java.lang.Double",
    "clojure.lang.BigInt",
    "java.math.BigInteger",
    "clojure.lang.Ratio",
    "java.math.BigDecimal",
    "java.lang.Number",
    "java.lang.Class",
];
pub(super) const ROOT: u32 = r#async::STREAM_GLOBAL_BASE + streams::GLOBAL_COUNT;
pub(super) const ROOT_END: u32 = ROOT + NAMES.len() as u32;

pub(crate) fn resolve(name: &str) -> Option<u8> {
    NAMES
        .iter()
        .position(|full| *full == name)
        .or_else(|| {
            // JVM default imports include these Java classes, not BigInt/Ratio.
            [0usize, 1, 3, 5, 6, 7]
                .into_iter()
                .find(|&i| NAMES[i].rsplit('.').next() == Some(name))
        })
        .map(|i| i as u8)
}

pub(super) fn append_globals(globals: &mut GlobalSection) {
    assert_eq!(globals.len(), ROOT);
    for name in NAMES {
        let mut init = vec![
            I64Const(0),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            ArrayNewDefault(ARGS),
        ];
        init.extend(name.encode_utf16().map(|u| I32Const(i32::from(u))));
        init.extend([
            ArrayNewFixed {
                array_type_index: STRING,
                array_size: name.encode_utf16().count() as u32,
            },
            I32Const(0),
            RefI31,
            sentinel_value(NIL),
            StructNew(DESCRIPTOR),
        ]);
        globals.global(
            GlobalType {
                val_type: reference(DESCRIPTOR),
                mutable: false,
                shared: false,
            },
            &ConstExpr::extended(init),
        );
    }
}

/// Declare before nominal functions, which recognize only these exact descriptors.
pub(super) fn functions(b: &mut Builder) {
    let mut body = vec![];
    for i in 0..NAMES.len() as u32 {
        body.extend([
            LocalGet(0),
            GlobalGet(ROOT + i),
            RefEq,
            If(BlockType::Empty),
            I32Const(i as i32),
            Return,
            End,
        ]);
    }
    body.push(I32Const(-1));
    b.function("numeric-class-index", &[VALUE], &[ValType::I32], &body);
    let mut body = vec![];
    for i in 0..NAMES.len() as u32 {
        body.extend([
            LocalGet(0),
            I32Const(i as i32),
            I32Eq,
            If(BlockType::Empty),
            GlobalGet(ROOT + i),
            Return,
            End,
        ]);
    }
    nominal::error(&mut body);
    b.function("numeric-class-value", &[ValType::I32], &[VALUE], &body);
    // A BigInteger projection shares integer storage but retains its class.
    let mut body = vec![
        LocalGet(0),
        Call(b.names["long-test"]),
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(NUMBER)),
        If(BlockType::Empty),
        I32Const(1),
        Return,
        End,
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(BIGINT)),
        If(BlockType::Empty),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(BIGINT)),
        StructGet {
            struct_type_index: BIGINT,
            field_index: 3,
        },
        If(BlockType::Result(ValType::I32)),
        I32Const(3),
        Else,
        I32Const(2),
        End,
        Return,
        End,
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(RATIO)),
        If(BlockType::Empty),
        I32Const(4),
        Return,
        End,
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(DECIMAL)),
        If(BlockType::Empty),
        I32Const(5),
        Return,
        End,
        LocalGet(0),
        Call(b.names["numeric-class-index"]),
        I32Const(0),
        I32GeS,
        If(BlockType::Empty),
        I32Const(7),
        Return,
        End,
    ];
    body.push(I32Const(-1));
    b.function("numeric-value-class", &[VALUE], &[ValType::I32], &body);
}

pub(super) fn instance_function(b: &mut Builder) {
    let mut body = vec![
        LocalGet(0),
        Call(b.names["numeric-class-index"]),
        LocalTee(2),
        I32Const(0),
        I32LtS,
        If(BlockType::Empty),
        I32Const(-1),
        Return,
        End,
        LocalGet(2),
        I32Const(6),
        I32Eq,
        If(BlockType::Empty),
        LocalGet(1),
        Call(b.names["number-test"]),
        Return,
        End,
        LocalGet(2),
        I32Const(7),
        I32Eq,
        If(BlockType::Empty),
        LocalGet(1),
        Call(b.names["class-value-is"]),
        LocalGet(1),
        Call(b.names["class-value-name"]),
        RefIsNull,
        I32Eqz,
        I32Or,
        Return,
        End,
        LocalGet(1),
        Call(b.names["numeric-value-class"]),
        LocalGet(2),
        I32Eq,
    ];
    b.function_with_locals(
        "numeric-class-instance",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[(1, ValType::I32)],
        &body,
    );
}

/// A protocol receiver's actual descriptor, or nil for the existing native
/// fallback families. Keep their old shared tables as fallback, not identity.
pub(super) fn protocol_descriptor_function(b: &mut Builder) {
    let body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(OBJECT)),
        If(BlockType::Empty),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(OBJECT)),
        StructGet {
            struct_type_index: OBJECT,
            field_index: 0,
        },
        Return,
        End,
        LocalGet(0),
        Call(b.names["numeric-value-class"]),
        LocalTee(1),
        I32Const(0),
        I32GeS,
        If(BlockType::Empty),
        LocalGet(1),
        Call(b.names["numeric-class-value"]),
        Return,
        End,
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(CLOSURE)),
        If(BlockType::Empty),
        LocalGet(0),
        Call(b.names["class-value-is"]),
        LocalGet(0),
        Call(b.names["class-value-name"]),
        RefIsNull,
        I32Eqz,
        I32Or,
        If(BlockType::Empty),
        I32Const(7),
        Call(b.names["numeric-class-value"]),
        Return,
        End,
        End,
        sentinel_value(NIL),
    ];
    b.function_with_locals(
        "protocol-value-descriptor",
        &[VALUE],
        &[VALUE],
        &[(1, ValType::I32)],
        &body,
    );
}
