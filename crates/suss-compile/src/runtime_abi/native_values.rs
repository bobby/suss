//! Private optional operations on authenticated native value endpoints. Pure
//! components install no callback and retain their ordinary storage paths.
use super::*;
use Instruction::*;

pub(crate) const FUNCTION: i32 = 1;
pub(crate) const DATA: i32 = 2;
pub(crate) const VAR: i32 = 3;
pub(crate) const NAMESPACE: i32 = 4;
pub(crate) const OBJECT_ARRAY: i32 = 5;
pub(crate) const FIELD_GET: i32 = 0;
pub(crate) const FIELD_SET: i32 = 1;
pub(crate) const GETTER_MACRO: i32 = 2;
pub(crate) const GETTER_MACRO_SET: i32 = 3;
pub(crate) const ARRAY_LENGTH: i32 = 4;
pub(crate) const ARRAY_GET: i32 = 5;
pub(crate) const ARRAY_SET: i32 = 6;
pub(crate) const ARRAY_CLONE: i32 = 7;
pub(crate) const ARRAY_ELEMENTS: i32 = 8;
pub(crate) const ARRAY_COPY: i32 = 9;
const ROOT: u32 = MAP_ENTRY_DESCRIPTOR_GLOBAL + 8;

pub(super) fn declare(b: &mut Builder) -> u32 {
    let source = b.next_type;
    b.types
        .ty()
        .function([VALUE, ValType::I32, ValType::I32, VALUE, VALUE], [VALUE]);
    b.next_type += 1;
    let reference = RefType {
        nullable: true,
        heap_type: HeapType::Concrete(source),
    };
    b.function(
        "native-value-install",
        &[ValType::Ref(reference)],
        &[],
        &[LocalGet(0), GlobalSet(ROOT)],
    );
    let mut body = vec![GlobalGet(ROOT), RefIsNull, If(BlockType::Empty)];
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(0),
        Call(b.names["native-identity-capability"]),
        LocalTee(5),
        RefIsNull,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(5),
        LocalGet(1),
        LocalGet(2),
        LocalGet(3),
        LocalGet(4),
        GlobalGet(ROOT),
        RefAsNonNull,
        CallRef(source),
    ]);
    b.function_with_locals(
        "native-value-operation",
        &[VALUE, ValType::I32, ValType::I32, VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &body,
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

/// Called only on a storage slow path. The kind is authenticated by the same
/// host callback that authenticates identity, never by a user field/tag/name.
pub(super) fn kind(b: &Builder, receiver: u32, expected: i32) -> Vec<Instruction<'static>> {
    vec![
        LocalGet(receiver),
        Call(b.names["native-value-kind"]),
        I32Const(expected),
        I32Eq,
    ]
}
pub(super) fn operation(
    b: &Builder,
    receiver: u32,
    op: i32,
    index: Vec<Instruction<'static>>,
    first: Vec<Instruction<'static>>,
    second: Vec<Instruction<'static>>,
) -> Vec<Instruction<'static>> {
    let mut body = vec![LocalGet(receiver), I32Const(op)];
    body.extend(index);
    body.extend(first);
    body.extend(second);
    body.push(Call(b.names["native-value-operation"]));
    body
}
