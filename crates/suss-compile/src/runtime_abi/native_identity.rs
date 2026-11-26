//! Optional native identity for authenticated cross-Store value capabilities.
//! The callback and capability type live outside the shared ABI prelude. Pure
//! components retain ordinary reference identity without requiring a host import.
use super::*;
use Instruction::*;

// Four numeric roots, then entropy and compiled evaluation. Checked at append.
const ROOT: u32 = MAP_ENTRY_DESCRIPTOR_GLOBAL + 7;

pub(super) fn declare(b: &mut Builder) -> u32 {
    let capability = b.next_type;
    b.types.ty().struct_([FieldType {
        element_type: StorageType::Val(ValType::EXTERNREF),
        mutable: false,
    }]);
    b.next_type += 1;
    let cap_test = RefTestNonNull(HeapType::Concrete(capability));
    let mut body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(UNIVERSAL)),
        If(BlockType::Empty),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(UNIVERSAL)),
        StructGet {
            struct_type_index: UNIVERSAL,
            field_index: UNIVERSAL_ENVIRONMENT,
        },
        LocalTee(1),
        cap_test.clone(),
        If(BlockType::Empty),
        LocalGet(1),
        Return,
        End,
        End,
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(PACKED)),
        If(BlockType::Empty),
        LocalGet(0), RefCastNonNull(HeapType::Concrete(PACKED)),
        StructGet { struct_type_index: PACKED, field_index: 0 },
        LocalTee(1), cap_test.clone(), If(BlockType::Empty), LocalGet(1), Return, End, End,
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(OBJECT)),
        If(BlockType::Empty),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(OBJECT)),
        StructGet {
            struct_type_index: OBJECT,
            field_index: 2,
        },
        LocalTee(1),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        If(BlockType::Empty),
        LocalGet(1),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        I32Const(2),
        I32Eq,
        If(BlockType::Empty),
        LocalGet(1),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),
        ArrayGet(ARGS),
        LocalTee(1),
        cap_test,
        If(BlockType::Empty),
        LocalGet(1),
        Return,
        End,
        End,
        End,
        End,
        sentinel_value(NIL),
    ];
    let extract = b.function_with_locals(
        "native-identity-capability",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &body,
    );
    let source = b.next_type;
    b.types.ty().function([VALUE, VALUE], [ValType::I32]);
    b.next_type += 1;
    let reference = RefType {
        nullable: true,
        heap_type: HeapType::Concrete(source),
    };
    b.function(
        "native-identity-install",
        &[ValType::Ref(reference)],
        &[],
        &[LocalGet(0), GlobalSet(ROOT)],
    );
    body = vec![
        GlobalGet(ROOT),
        RefIsNull,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        LocalGet(0),
        Call(extract),
        LocalTee(2),
        sentinel_value(NIL),
        RefEq,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        LocalGet(1),
        Call(extract),
        LocalTee(3),
        sentinel_value(NIL),
        RefEq,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        LocalGet(2),
        LocalGet(3),
        GlobalGet(ROOT),
        RefAsNonNull,
        CallRef(source),
    ];
    b.function_with_locals(
        "native-identical-test",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[(2, VALUE)],
        &body,
    );
    // A null second argument is the private kind query. Identity calls always
    // pass two non-null extracted capabilities. The host authenticates the
    // actual endpoint before returning its operation family.
    b.function_with_locals("native-value-kind", &[VALUE], &[ValType::I32], &[(1, VALUE)], &[
        GlobalGet(ROOT), RefIsNull, If(BlockType::Empty), I32Const(0), Return, End,
        LocalGet(0), Call(extract), LocalTee(1), RefIsNull,
        If(BlockType::Empty), I32Const(0), Return, End,
        LocalGet(1), RefNull(HeapType::Abstract { shared: false, ty: AbstractHeapType::Eq }),
        GlobalGet(ROOT), RefAsNonNull, CallRef(source),
    ]);
    // Ordinary source instances have one prototype element. A second private
    // element is allowed only when the installed host authenticates the actual
    // receiver's capability; arbitrary two-element arrays remain invalid.
    b.function_with_locals(
        "native-prototype-carrier-valid",
        &[VALUE],
        &[ValType::I32],
        &[(1, VALUE)],
        &[
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(OBJECT)),
            I32Eqz,
            If(BlockType::Empty),
            I32Const(0),
            Return,
            End,
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(OBJECT)),
            StructGet {
                struct_type_index: OBJECT,
                field_index: 2,
            },
            LocalTee(1),
            RefTestNonNull(HeapType::Concrete(ARGS)),
            I32Eqz,
            If(BlockType::Empty),
            I32Const(0),
            Return,
            End,
            LocalGet(1),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            ArrayLen,
            I32Const(1),
            I32Eq,
            If(BlockType::Empty),
            I32Const(1),
            Return,
            End,
            LocalGet(1),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            ArrayLen,
            I32Const(2),
            I32Eq,
            If(BlockType::Result(ValType::I32)),
            LocalGet(0),
            LocalGet(0),
            Call(b.names["native-identical-test"]),
            Else,
            I32Const(0),
            End,
        ],
    );
    source
}

pub(super) fn append(globals: &mut GlobalSection, ty: u32) {
    assert_eq!(globals.len(), ROOT);
    let reference = RefType {
        nullable: true,
        heap_type: HeapType::Concrete(ty),
    };
    globals.global(
        GlobalType {
            val_type: ValType::Ref(reference),
            mutable: true,
            shared: false,
        },
        &ConstExpr::ref_null(reference.heap_type),
    );
}
