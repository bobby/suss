//! Original scalar-key/property adapter above owned storage. The default legacy
//! prototype accessor is recognized here; general property descriptors are pending.
use super::*;
fn proto_key() -> Vec<Instruction<'static>> {
    let mut code = "__proto__"
        .encode_utf16()
        .map(|unit| Instruction::I32Const(i32::from(unit)))
        .collect::<Vec<_>>();
    code.push(Instruction::ArrayNewFixed {
        array_type_index: STRING,
        array_size: 9,
    });
    code
}
pub(super) fn functions(b: &mut Builder) {
    use Instruction::*;
    let fields = b.names["native-object-fields"];
    let check = b.names["native-object-check-chain"];
    let prototype = b.names["native-object-prototype"];
    let slot = b.names["native-object-own-slot"];
    let root = native_object_methods::ROOT;
    // A nearer data property suppresses the inherited default accessor. Its value
    // may be Undefined; slot presence, rather than truthiness, decides shadowing.
    let mut code = vec![
        LocalGet(0),
        Call(fields),
        Drop,
        LocalGet(0),
        LocalGet(1),
        Call(slot),
        Drop,
        LocalGet(0),
        Call(check),
        LocalGet(1),
    ];
    code.extend(proto_key());
    code.extend([
        Call(b.names["property-key-equal"]),
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        LocalGet(0),
        LocalSet(2),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(2),
        I32Const(0),
        RefI31,
        RefEq,
        BrIf(1),
        LocalGet(2),
        LocalGet(1),
        Call(slot),
        I32Const(0),
        I32GeS,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        LocalGet(2),
        GlobalGet(root),
        RefEq,
        If(BlockType::Empty),
        I32Const(1),
        Return,
        End,
        LocalGet(2),
        Call(prototype),
        LocalSet(2),
        Br(0),
        End,
        End,
        I32Const(0),
    ]);
    let accessor = b.function_with_locals(
        "native-object-proto-accessor",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[(1, VALUE)],
        &code,
    );
    b.function_with_locals(
        "native-object-property-get",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &[
            LocalGet(1),
            Call(b.names["coerce-string"]),
            LocalSet(2),
            LocalGet(0),
            LocalGet(2),
            Call(accessor),
            If(BlockType::Result(VALUE)),
            LocalGet(0),
            Call(prototype),
            Else,
            LocalGet(0),
            LocalGet(2),
            Call(b.names["native-object-chain-get"]),
            End,
        ],
    );
    code = vec![
        LocalGet(1),
        Call(b.names["coerce-string"]),
        LocalSet(3),
        LocalGet(0),
        LocalGet(3),
        Call(accessor),
        If(BlockType::Empty),
        LocalGet(2),
        I32Const(0),
        RefI31,
        RefEq,
        LocalGet(2),
        RefTestNonNull(HeapType::Concrete(7)),
        I32Or,
        If(BlockType::Empty),
        // The default root has an immutable prototype. Setting the same nil is
        // allowed, but replacing it must fail before any mutation.
        LocalGet(0),
        GlobalGet(root),
        RefEq,
        If(BlockType::Empty),
        LocalGet(2),
        I32Const(0),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
    ];
    nominal::error(&mut code);
    code.extend([
        End,
        End,
        LocalGet(0),
        LocalGet(2),
        Call(b.names["native-object-prototype-set"]),
        Drop,
        Else,
        // Only supported scalars are ignored. Foreign object domains/null refs
        // remain explicit language errors, not unknown successful writes.
        LocalGet(2),
        Call(b.names["coerce-string"]),
        Drop,
        End,
        LocalGet(2),
        Return,
        End,
        LocalGet(0),
        LocalGet(3),
        LocalGet(2),
        Call(b.names["native-object-own-set"]),
    ]);
    b.function_with_locals(
        "native-object-property-set",
        &[VALUE, VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &code,
    );
}
