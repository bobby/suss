//! Original descriptor-aware scalar properties and legacy prototype accessors.
use super::*;
pub(super) fn primitives(b: &mut Builder) {
    use Instruction::*;
    let mut code = vec![
        LocalGet(0),
        Call(b.names["native-object-fields"]),
        Drop,
        LocalGet(1),
        I32Const(0),
        RefI31,
        RefEq,
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(7)),
        I32Or,
        If(BlockType::Empty),
        LocalGet(0),
        GlobalGet(native_object_methods::ROOT),
        RefEq,
        If(BlockType::Empty),
        LocalGet(1),
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
        LocalGet(1),
        Call(b.names["native-object-prototype-set"]),
        Drop,
        Else,
        LocalGet(1),
        Call(b.names["coerce-string"]),
        Drop,
        End,
        I32Const(UNDEFINED),
        RefI31,
    ]);
    b.function(
        "native-object-proto-write",
        &[VALUE, VALUE],
        &[VALUE],
        &code,
    );
}
pub(super) fn functions(b: &mut Builder) {
    use Instruction::*;
    let mut code = vec![
        LocalGet(0),
        LocalGet(1),
        Call(b.names["native-object-own-slot"]),
        Drop,
        LocalGet(0),
        Call(b.names["native-object-check-chain"]),
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
        Call(b.names["native-object-own-descriptor"]),
        LocalSet(3),
        LocalGet(3),
        I32Const(UNDEFINED),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(3),
        Return,
        End,
        LocalGet(2),
        Call(b.names["native-object-prototype"]),
        LocalSet(2),
        Br(0),
        End,
        End,
        I32Const(UNDEFINED),
        RefI31,
    ];
    let resolve = b.function_with_locals(
        "native-object-property-descriptor",
        &[VALUE, VALUE],
        &[VALUE],
        &[(2, VALUE)],
        &code,
    );
    // Preserved internal diagnostic helper now queries a real descriptor.
    b.function_with_locals(
        "native-object-proto-accessor",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[(1, VALUE)],
        &[
            LocalGet(0),
            LocalGet(1),
            Call(resolve),
            LocalSet(2),
            LocalGet(2),
            I32Const(UNDEFINED),
            RefI31,
            RefEq,
            If(BlockType::Result(ValType::I32)),
            I32Const(0),
            Else,
            LocalGet(2),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            I32Const(0),
            ArrayGet(ARGS),
            RefCastNonNull(HeapType::I31),
            I31GetU,
            I32Const(8),
            I32And,
            End,
        ],
    );
    code = vec![
        LocalGet(1),
        Call(b.names["coerce-string"]),
        LocalSet(2),
        LocalGet(0),
        LocalGet(2),
        Call(resolve),
        LocalSet(3),
        LocalGet(3),
        I32Const(UNDEFINED),
        RefI31,
        RefEq,
        If(BlockType::Empty),
        LocalGet(3),
        Return,
        End,
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::I31),
        I31GetU,
        I32Const(8),
        I32And,
        If(BlockType::Empty),
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        LocalSet(4),
        LocalGet(4),
        I32Const(UNDEFINED),
        RefI31,
        RefEq,
        If(BlockType::Empty),
        LocalGet(4),
        Return,
        End,
        LocalGet(4),
        LocalGet(0),
        I32Const(0),
        ArrayNewDefault(ARGS),
        Call(b.names["object-method-invoke"]),
        Return,
        End,
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),
        ArrayGet(ARGS),
    ];
    b.function_with_locals(
        "native-object-property-get",
        &[VALUE, VALUE],
        &[VALUE],
        &[(3, VALUE)],
        &code,
    );
    for strict in [false, true] {
        code = vec![LocalGet(2), RefIsNull, If(BlockType::Empty)];
        nominal::error(&mut code);
        code.extend([
            End,
            LocalGet(1),
            Call(b.names["coerce-string"]),
            LocalSet(3),
            LocalGet(0),
            LocalGet(3),
            Call(resolve),
            LocalSet(4),
            LocalGet(4),
            I32Const(UNDEFINED),
            RefI31,
            RefEq,
            I32Eqz,
            If(BlockType::Empty),
            LocalGet(4),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            I32Const(0),
            ArrayGet(ARGS),
            RefCastNonNull(HeapType::I31),
            I31GetU,
            LocalSet(5),
            LocalGet(5),
            I32Const(8),
            I32And,
            If(BlockType::Empty),
            LocalGet(4),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            I32Const(1),
            ArrayGet(ARGS),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            I32Const(1),
            ArrayGet(ARGS),
            LocalSet(4),
            LocalGet(4),
            I32Const(UNDEFINED),
            RefI31,
            RefEq,
            I32Eqz,
            If(BlockType::Empty),
            LocalGet(4),
            LocalGet(0),
            LocalGet(2),
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 1,
            },
            Call(b.names["object-method-invoke"]),
            Drop,
            Else,
        ]);
        if strict {
            nominal::error(&mut code);
        }
        code.extend([
            End,
            LocalGet(2),
            Return,
            End,
            LocalGet(5),
            I32Const(1),
            I32And,
            I32Eqz,
            If(BlockType::Empty),
        ]);
        if strict {
            nominal::error(&mut code);
        }
        code.extend([
            LocalGet(2),
            Return,
            End,
            LocalGet(0),
            LocalGet(3),
            Call(b.names["native-object-own-slot"]),
            I32Const(0),
            I32GeS,
            If(BlockType::Empty),
            LocalGet(0),
            LocalGet(3),
            LocalGet(2),
            LocalGet(5),
            Call(b.names["native-object-own-define"]),
            Return,
            End,
            End,
            LocalGet(0),
            LocalGet(3),
            LocalGet(2),
            Call(b.names["native-object-own-set"]),
        ]);
        b.function_with_locals(
            if strict {
                "native-object-property-set-strict"
            } else {
                "native-object-property-set"
            },
            &[VALUE, VALUE, VALUE],
            &[VALUE],
            &[(2, VALUE), (1, ValType::I32)],
            &code,
        );
    }
    legacy_functions(b, resolve);
    for writing in [false, true] {
        let mut code = vec![
            LocalGet(1),
            RefTestNonNull(HeapType::Concrete(STRING)),
            I32Eqz,
            If(BlockType::Empty),
        ];
        nominal::error(&mut code);
        code.extend([
            End,
            LocalGet(0),
            Call(b.names["native-object?"]),
            If(BlockType::Result(VALUE)),
            LocalGet(0),
            LocalGet(1),
        ]);
        if writing {
            code.push(LocalGet(2));
        }
        code.extend([
            Call(
                b.names[if writing {
                    "native-object-property-set"
                } else {
                    "native-object-property-get"
                }],
            ),
            Else,
            LocalGet(0),
            LocalGet(1),
        ]);
        if writing {
            code.push(LocalGet(2));
        }
        code.extend([
            Call(
                b.names[if writing {
                    "fixed-named-property-set"
                } else {
                    "fixed-named-property-get"
                }],
            ),
            End,
        ]);
        b.function(
            if writing {
                "named-property-set"
            } else {
                "named-property-get"
            },
            if writing {
                &[VALUE, VALUE, VALUE]
            } else {
                &[VALUE, VALUE]
            },
            &[VALUE],
            &code,
        );
    }
}

// Original bounded legacy accessor operations over owned objects and scalar keys.
fn legacy_functions(b: &mut Builder, resolve: u32) {
    use Instruction::*;
    let mut code = vec![
        LocalGet(0),
        Call(b.names["native-object-fields"]),
        Drop,
        LocalGet(2),
        RefTestNonNull(HeapType::Concrete(4)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    nominal::error(&mut code);
    code.extend([End, LocalGet(3), I32Const(1), I32GtU, If(BlockType::Empty)]);
    nominal::error(&mut code);
    code.extend([
        End,
        LocalGet(1),
        Call(b.names["coerce-string"]),
        LocalSet(4),
        I32Const(UNDEFINED),
        RefI31,
        LocalSet(6),
        LocalGet(0),
        LocalGet(4),
        Call(b.names["native-object-own-descriptor"]),
        LocalSet(5),
        LocalGet(5),
        I32Const(UNDEFINED),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::I31),
        I31GetU,
        LocalSet(7),
        LocalGet(7),
        I32Const(4),
        I32And,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut code);
    code.extend([
        End,
        LocalGet(7),
        I32Const(8),
        I32And,
        If(BlockType::Empty),
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),
        LocalGet(3),
        I32Sub,
        ArrayGet(ARGS),
        LocalSet(6),
        End,
        End,
        LocalGet(0),
        LocalGet(4),
        LocalGet(3),
        I32Eqz,
        If(BlockType::Result(VALUE)),
        LocalGet(2),
        Else,
        LocalGet(6),
        End,
        LocalGet(3),
        I32Eqz,
        If(BlockType::Result(VALUE)),
        LocalGet(6),
        Else,
        LocalGet(2),
        End,
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 2,
        },
        I32Const(14),
        Call(b.names["native-object-own-define"]),
        Drop,
        I32Const(UNDEFINED),
        RefI31,
    ]);
    b.function_with_locals(
        "native-object-legacy-define",
        &[VALUE, VALUE, VALUE, ValType::I32],
        &[VALUE],
        &[(3, VALUE), (1, ValType::I32)],
        &code,
    );
    code = vec![
        LocalGet(0),
        Call(b.names["native-object-fields"]),
        Drop,
        LocalGet(2),
        I32Const(1),
        I32GtU,
        If(BlockType::Empty),
    ];
    nominal::error(&mut code);
    code.extend([
        End,
        LocalGet(0),
        LocalGet(1),
        Call(b.names["coerce-string"]),
        Call(resolve),
        LocalSet(3),
        LocalGet(3),
        I32Const(UNDEFINED),
        RefI31,
        RefEq,
        If(BlockType::Empty),
        LocalGet(3),
        Return,
        End,
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::I31),
        I31GetU,
        I32Const(8),
        I32And,
        If(BlockType::Result(VALUE)),
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(2),
        ArrayGet(ARGS),
        Else,
        I32Const(UNDEFINED),
        RefI31,
        End,
    ]);
    b.function_with_locals(
        "native-object-legacy-lookup",
        &[VALUE, VALUE, ValType::I32],
        &[VALUE],
        &[(1, VALUE)],
        &code,
    );
}
