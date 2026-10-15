//! Constructor-owned prototype objects, rooted in closure property storage.
//! Typed immutable function roots break the runtime build-order dependency;
//! there is no host registry or source-name based ancestry decision.
use super::*;
use Instruction::*;

const ROOT: u32 = array_join::ACTIVE + 4;
pub(super) const TYPE_ERROR_GLOBAL: u32 = ROOT + 7;
pub(super) const OBJECT_DESCRIPTOR: u32 = ROOT + 6;
pub(super) const OBJECT_CONSTRUCTOR: u32 = ROOT + 8;
pub(super) const LAST_GLOBAL: u32 = ROOT + 10;
const PROTOTYPE_KEY: i32 = 8;

#[derive(Clone, Copy)]
pub(super) struct Types {
    getter: u32,
    descriptor: u32,
    ancestry: u32,
    instance: u32,
    inherited: u32,
}

pub(super) fn declare(b: &mut Builder) -> Types {
    let types = Types {
        getter: b.next_type,
        descriptor: b.next_type + 1,
        ancestry: b.next_type + 2,
        instance: b.next_type + 3,
        inherited: b.next_type + 4,
    };
    for _ in 0..2 {
        b.types.ty().function([VALUE], [VALUE]);
    }
    b.types.ty().function([VALUE], [ValType::I32]);
    b.types.ty().function([VALUE, VALUE], [ValType::I32]);
    b.types.ty().function([VALUE, VALUE], [VALUE]);
    b.next_type += 5;
    for (name, ty, root) in [
        ("owned-constructor-prototype", types.getter, ROOT + 1),
        ("owned-descriptor-prototype", types.descriptor, ROOT + 2),
    ] {
        b.function(
            name,
            &[VALUE],
            &[VALUE],
            &[LocalGet(0), GlobalGet(root), CallRef(ty)],
        );
    }
    b.function(
        "owned-error-ancestry",
        &[VALUE],
        &[ValType::I32],
        &[LocalGet(0), GlobalGet(ROOT + 3), CallRef(types.ancestry)],
    );
    b.function(
        "owned-prototype-instance",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[
            LocalGet(0),
            LocalGet(1),
            GlobalGet(ROOT + 4),
            CallRef(types.instance),
        ],
    );
    b.function(
        "owned-instance-property-get",
        &[VALUE, VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            LocalGet(1),
            GlobalGet(ROOT + 5),
            CallRef(types.inherited),
        ],
    );
    b.function(
        "object-static-initialize",
        &[VALUE],
        &[VALUE],
        &[LocalGet(0), GlobalGet(ROOT + 9), CallRef(types.getter)],
    );
    b.function(
        "owned-type-error",
        &[VALUE],
        &[VALUE],
        &[LocalGet(0), GlobalGet(ROOT + 10), CallRef(types.getter)],
    );
    types
}

fn text(code: &mut Vec<Instruction<'static>>, name: &str) {
    code.extend(name.encode_utf16().map(|unit| I32Const(i32::from(unit))));
    code.push(ArrayNewFixed {
        array_type_index: STRING,
        array_size: name.encode_utf16().count() as u32,
    });
}

pub(super) fn functions(b: &mut Builder) -> [u32; 5] {
    let mut code = vec![
        // Only genuine source/native constructors have descriptor environments.
        LocalGet(0),
        Call(b.names["constructor-descriptor"]),
        LocalSet(1),
        LocalGet(1),
        GlobalGet(OBJECT_DESCRIPTOR),
        RefEq,
        If(BlockType::Empty),
        Call(b.names["native-object-default-prototype"]),
        Return,
        End,
        LocalGet(0),
        I32Const(PROTOTYPE_KEY),
        Call(b.names["closure-internal-property-get"]),
        LocalSet(2),
        LocalGet(2),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        If(BlockType::Empty),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        I32Const(1),
        I32Ne,
        If(BlockType::Empty),
    ];
    nominal::error(&mut code);
    code.extend([
        End,
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        Return,
        End,
        // Allocate once, then retain the exact prototype on its constructor.
        Call(b.names["native-object-default-new"]),
        LocalSet(2),
        LocalGet(1),
        GlobalGet(range_errors::DESCRIPTOR_GLOBAL),
        RefEq,
        LocalGet(1),
        GlobalGet(TYPE_ERROR_GLOBAL),
        RefEq,
        I32Or,
        If(BlockType::Empty),
        LocalGet(2),
        Call(b.names["error-constructor"]),
        Call(b.names["owned-constructor-prototype"]),
        Call(b.names["native-object-prototype-set"]),
        Drop,
        End,
        LocalGet(2),
    ]);
    text(&mut code, "constructor");
    code.extend([
        LocalGet(0),
        I32Const(5),
        Call(b.names["native-object-own-define"]),
        Drop,
    ]);
    for (descriptor, name) in [
        (nominal::ERROR_GLOBAL, "Error"),
        (range_errors::DESCRIPTOR_GLOBAL, "RangeError"),
        (TYPE_ERROR_GLOBAL, "TypeError"),
    ] {
        code.extend([
            LocalGet(1),
            GlobalGet(descriptor),
            RefEq,
            If(BlockType::Empty),
            LocalGet(2),
        ]);
        text(&mut code, "name");
        text(&mut code, name);
        code.extend([
            I32Const(5),
            Call(b.names["native-object-own-define"]),
            Drop,
            LocalGet(2),
        ]);
        text(&mut code, "message");
        text(&mut code, "");
        code.extend([
            I32Const(5),
            Call(b.names["native-object-own-define"]),
            Drop,
            End,
        ]);
    }
    code.extend([
        LocalGet(1),
        GlobalGet(nominal::ERROR_GLOBAL),
        RefEq,
        If(BlockType::Empty),
        LocalGet(2),
    ]);
    text(&mut code, "toString");
    code.extend([
        Call(b.names["error-to-string-method"]),
        I32Const(5),
        Call(b.names["native-object-own-define"]),
        Drop,
        End,
    ]);
    code.extend([
        LocalGet(0),
        I32Const(PROTOTYPE_KEY),
        LocalGet(2),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 1,
        },
        Call(b.names["closure-internal-property-set"]),
        Drop,
        LocalGet(2),
    ]);
    let getter = b.function_with_locals(
        "owned-constructor-prototype-implementation",
        &[VALUE],
        &[VALUE],
        &[(2, VALUE)],
        &code,
    );
    // Kernel-only descriptors without a source constructor have no public
    // prototype. They are not guessed from descriptor IDs or source names.
    let code = [
        LocalGet(0),
        GlobalGet(range_errors::DESCRIPTOR_GLOBAL),
        RefEq,
        If(BlockType::Empty),
        Call(b.names["range-error-constructor"]),
        Call(getter),
        Return,
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(DESCRIPTOR)),
        StructGet {
            struct_type_index: DESCRIPTOR,
            field_index: 2,
        },
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        RefI31,
        Return,
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(DESCRIPTOR)),
        StructGet {
            struct_type_index: DESCRIPTOR,
            field_index: 2,
        },
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        I32Const(1),
        I32And,
        If(BlockType::Empty),
        I32Const(0),
        RefI31,
        Return,
        End,
        LocalGet(0),
        GlobalGet(type_values::KEY_GLOBAL),
        Call(b.names["protocol-method-get"]),
        LocalSet(1),
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(CLOSURE)),
        If(BlockType::Result(VALUE)),
        LocalGet(1),
        Call(getter),
        Else,
        I32Const(0),
        RefI31,
        End,
    ];
    let descriptor = b.function_with_locals(
        "owned-descriptor-prototype-implementation",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &code,
    );
    // Ancestry follows actual owner-held prototype references. Numeric/kernel
    // errors retain their separate canonical representation; they are not used
    // to classify arbitrary source objects as derived errors.
    let mut code = vec![
        LocalGet(0),
        Call(b.names["native-object?"]),
        If(BlockType::Empty),
        LocalGet(0),
        Call(b.names["native-object-prototype"]),
        LocalSet(1),
        Else,
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(7)),
        If(BlockType::Empty),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(7)),
        StructGet {
            struct_type_index: 7,
            field_index: 2,
        },
        LocalSet(1),
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        If(BlockType::Empty),
        LocalGet(1),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        I32Const(1),
        I32Ne,
        If(BlockType::Empty),
    ];
    nominal::error(&mut code);
    code.extend([
        End,
        LocalGet(1),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        LocalSet(1),
        Else,
        I32Const(0),
        Return,
        End,
        Else,
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(8)),
        If(BlockType::Empty),
        LocalGet(0),
        Call(b.names["language-error-canonical?"]),
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(8)),
        StructGet {
            struct_type_index: 8,
            field_index: 0,
        },
        GlobalGet(range_errors::DESCRIPTOR_GLOBAL),
        RefEq,
        If(BlockType::Result(VALUE)),
        Call(b.names["range-error-constructor"]),
        Else,
        Call(b.names["error-constructor"]),
        End,
        Call(getter),
        LocalSet(1),
        Else,
        I32Const(0),
        Return,
        End,
        End,
        End,
        LocalGet(1),
        I32Const(0),
        RefI31,
        RefEq,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        LocalGet(1),
        Call(b.names["native-object-check-chain"]),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(1),
        I32Const(0),
        RefI31,
        RefEq,
        BrIf(1),
        LocalGet(1),
        LocalGet(2),
        RefEq,
        If(BlockType::Empty),
        I32Const(1),
        Return,
        End,
        // The whole chain was validated above. This search has no callbacks
        // or writes, so advance through the already-checked backing directly.
        LocalGet(1),
        Call(b.names["owned-error-backing"]),
        LocalTee(3),
        I32Const(0),
        RefI31,
        RefEq,
        If(BlockType::Result(VALUE)),
        LocalGet(1),
        Else,
        LocalGet(3),
        End,
        RefCastNonNull(HeapType::Concrete(7)),
        StructGet { struct_type_index: 7, field_index: 1 },
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),
        ArrayGet(ARGS),
        LocalSet(1),
        Br(0),
        End,
        End,
        I32Const(0),
    ]);
    let instance_code = code;
    // This implementation accepts the actual target prototype, so inherited
    // source Error subclasses also satisfy intermediate source-class catches.
    let instance_code = instance_code
        .into_iter()
        .map(|instruction| match instruction {
            LocalGet(1) => LocalGet(2),
            LocalSet(1) => LocalSet(2),
            LocalGet(2) => LocalGet(1),
            LocalSet(2) => LocalSet(1),
            other => other,
        })
        .collect::<Vec<_>>();
    let instance = b.function_with_locals(
        "owned-prototype-instance-implementation",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[(2, VALUE)],
        &instance_code,
    );
    let ancestry = b.function(
        "owned-error-ancestry-implementation",
        &[VALUE],
        &[ValType::I32],
        &[
            LocalGet(0),
            Call(b.names["error-constructor"]),
            Call(getter),
            Call(instance),
        ],
    );
    let mut inherited_code = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(8)),
        If(BlockType::Empty),
        LocalGet(0),
        Call(b.names["language-error-canonical?"]),
        I32Eqz,
        If(BlockType::Empty),
    ];
    nominal::error(&mut inherited_code);
    inherited_code.extend([
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(8)),
        StructGet {
            struct_type_index: 8,
            field_index: 0,
        },
        GlobalGet(range_errors::DESCRIPTOR_GLOBAL),
        RefEq,
        If(BlockType::Result(VALUE)),
        Call(b.names["range-error-constructor"]),
        Else,
        Call(b.names["error-constructor"]),
        End,
        Call(getter),
        LocalSet(2),
        Else,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(7)),
        StructGet {
            struct_type_index: 7,
            field_index: 2,
        },
        LocalSet(2),
        LocalGet(2),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        If(BlockType::Empty),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        I32Const(1),
        I32Ne,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut inherited_code);
    inherited_code.extend([
        End,
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        LocalSet(2),
        Else,
        LocalGet(2),
        I32Const(0),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut inherited_code);
    inherited_code.extend([
        End,
        // Without a per-instance carrier, inherit from the descriptor's
        // constructor prototype. An explicit carrier (including [null]) above
        // remains authoritative; kernel-only descriptors still resolve to
        // the absent-prototype sentinel.
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(7)),
        StructGet {
            struct_type_index: 7,
            field_index: 0,
        },
        Call(b.names["owned-descriptor-prototype"]),
        LocalSet(2),
        End,
        End,
        LocalGet(2),
        I32Const(0),
        RefI31,
        RefEq,
        If(BlockType::Empty),
        I32Const(6),
        RefI31,
        Return,
        End,
        // Presence is transported separately from the value: an inherited
        // undefined property must not become an absent/unsupported property.
        LocalGet(2),
        LocalGet(1),
        Call(b.names["native-object-property-descriptor"]),
        I32Const(6),
        RefI31,
        RefEq,
        If(BlockType::Empty),
        I32Const(6),
        RefI31,
        Return,
        End,
        LocalGet(2),
        LocalGet(0),
        LocalGet(1),
        Call(b.names["native-object-property-get-with-receiver"]),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 1,
        },
    ]);
    let inherited = b.function_with_locals(
        "owned-instance-property-get-implementation",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &inherited_code,
    );
    [getter, descriptor, ancestry, instance, inherited]
}

pub(super) fn append_globals(
    globals: &mut GlobalSection,
    types: Types,
    functions: [u32; 5],
    object_initialize: u32,
    type_error_throw: u32,
) {
    for (ty, function) in [
        types.getter,
        types.descriptor,
        types.ancestry,
        types.instance,
        types.inherited,
    ]
    .into_iter()
    .zip(functions)
    {
        globals.global(
            GlobalType {
                val_type: reference(ty),
                mutable: false,
                shared: false,
            },
            &ConstExpr::extended([RefFunc(function)]),
        );
    }
    for identity in [11, 12] {
        globals.global(
            GlobalType {
                val_type: reference(DESCRIPTOR),
                mutable: false,
                shared: false,
            },
            &ConstExpr::extended([
                I64Const(identity),
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
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: true,
            shared: false,
        },
        &ConstExpr::extended([I32Const(0), RefI31]),
    );
    globals.global(
        GlobalType {
            val_type: reference(types.getter),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([RefFunc(object_initialize)]),
    );
    globals.global(
        GlobalType {
            val_type: reference(types.getter),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([RefFunc(type_error_throw)]),
    );
}
