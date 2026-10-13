//! Owned Object descriptor entry points used by Error prototype/instance
//! mutation. Implements ordered ToPropertyDescriptor and descriptor validation;
//! no guest equality, printing, or source evaluation supplies kernel results.
use super::*;
use Instruction::*;
use error_properties::{callback, text, type_error};

fn arr(code: &mut Vec<Instruction<'static>>, local: u32, index: i32) {
    code.extend([
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(index),
        ArrayGet(ARGS),
    ]);
}
fn argument(code: &mut Vec<Instruction<'static>>, index: i32) {
    code.extend([
        LocalGet(1),
        ArrayLen,
        I32Const(index),
        I32GtU,
        If(BlockType::Result(VALUE)),
    ]);
    arr(code, 1, index);
    code.extend([Else, I32Const(UNDEFINED), RefI31, End]);
}
fn put(code: &mut Vec<Instruction<'static>>, local: u32, index: i32, value: u32) {
    code.extend([
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(index),
        LocalGet(value),
        ArraySet(ARGS),
    ]);
}
fn changed(b: &Builder, code: &mut Vec<Instruction<'static>>) {
    code.push(Call(b.names["type-error-constructor"]));
    text(code, "Cannot redefine property: ");
    code.extend([
        LocalGet(3),
        Call(b.names["string-concat"]),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 1,
        },
        Call(b.names["invoke"]),
        Throw(0),
    ]);
}
pub(super) fn functions(b: &mut Builder) -> (Vec<u32>, u32) {
    let mut code = vec![];
    for sentinel in [0, 2, UNDEFINED] {
        code.extend([
            LocalGet(0),
            I32Const(sentinel),
            RefI31,
            RefEq,
            If(BlockType::Empty),
            I32Const(0),
            Return,
            End,
        ]);
    }
    code.extend([
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(STRING)),
        If(BlockType::Empty),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
        I32Eqz,
        I32Eqz,
        Return,
        End,
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(NUMBER)),
        If(BlockType::Empty),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        LocalSet(1),
        LocalGet(1),
        F64Const(0.0.into()),
        F64Ne,
        LocalGet(1),
        LocalGet(1),
        F64Eq,
        I32And,
        Return,
        End,
        I32Const(1),
    ]);
    let boolean = b.function_with_locals(
        "descriptor-js-boolean",
        &[VALUE],
        &[ValType::I32],
        &[(1, ValType::F64)],
        &code,
    );
    code = vec![
        LocalGet(0),
        LocalGet(1),
        RefEq,
        If(BlockType::Empty),
        I32Const(1),
        Return,
        End,
    ];
    code.extend([
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(NUMBER)),
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(NUMBER)),
        I32And,
        If(BlockType::Empty),
    ]);
    for (value, local) in [(0, 2), (1, 3)] {
        code.extend([
            LocalGet(value),
            RefCastNonNull(HeapType::Concrete(NUMBER)),
            StructGet {
                struct_type_index: NUMBER,
                field_index: 0,
            },
            LocalSet(local),
        ]);
    }
    code.extend([
        LocalGet(2),
        LocalGet(2),
        F64Ne,
        LocalGet(3),
        LocalGet(3),
        F64Ne,
        I32And,
        LocalGet(2),
        I64ReinterpretF64,
        LocalGet(3),
        I64ReinterpretF64,
        I64Eq,
        I32Or,
        Return,
        End,
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(STRING)),
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(STRING)),
        I32And,
        If(BlockType::Empty),
        LocalGet(0),
        LocalGet(1),
        Call(b.names["property-key-equal"]),
        Return,
        End,
        I32Const(0),
    ]);
    let same = b.function_with_locals(
        "descriptor-same-value",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[(2, ValType::F64)],
        &code,
    );
    // arguments: target, key, attributes. Source operand evaluation is complete
    // before target checking/key conversion and all ordered attribute reads.
    code = vec![];
    for (index, local) in [(0, 2), (1, 3), (2, 4)] {
        argument(&mut code, index);
        code.push(LocalSet(local));
    }
    code.extend([
        LocalGet(2),
        Call(b.names["native-object?"]),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    type_error(b, &mut code, "Object.defineProperty called on non-object");
    code.extend([
        End,
        LocalGet(3),
        Call(b.names["coerce-string"]),
        LocalSet(3),
        LocalGet(4),
        Call(b.names["native-object?"]),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    type_error(b, &mut code, "Property description must be an object");
    code.push(End);
    for _ in 0..6 {
        code.extend([I32Const(UNDEFINED), RefI31]);
    }
    code.extend([
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 6,
        },
        LocalSet(5),
        I32Const(0),
        LocalSet(7),
    ]);
    for (index, key) in [
        "enumerable",
        "configurable",
        "value",
        "writable",
        "get",
        "set",
    ]
    .into_iter()
    .enumerate()
    {
        code.push(LocalGet(4));
        text(&mut code, key);
        code.extend([
            Call(b.names["native-object-property-descriptor"]),
            I32Const(UNDEFINED),
            RefI31,
            RefEq,
            I32Eqz,
            If(BlockType::Empty),
            LocalGet(4),
        ]);
        text(&mut code, key);
        code.extend([
            Call(b.names["native-object-property-get"]),
            LocalSet(8),
            LocalGet(7),
            I32Const(1 << index),
            I32Or,
            LocalSet(7),
        ]);
        if matches!(index, 0 | 1 | 3) {
            code.extend([
                LocalGet(8),
                Call(boolean),
                I32Const(2),
                I32Mul,
                I32Const(2),
                I32Add,
                RefI31,
                LocalSet(8),
            ]);
        } else if index >= 4 {
            code.extend([
                LocalGet(8),
                RefTestNonNull(HeapType::Concrete(CLOSURE)),
                LocalGet(8),
                I32Const(UNDEFINED),
                RefI31,
                RefEq,
                I32Or,
                I32Eqz,
                If(BlockType::Empty),
            ]);
            type_error(
                b,
                &mut code,
                if index == 4 {
                    "Getter must be a function"
                } else {
                    "Setter must be a function"
                },
            );
            code.push(End);
        }
        put(&mut code, 5, index as i32, 8);
        code.push(End);
    }
    code.extend([
        LocalGet(7),
        I32Const(48),
        I32And,
        I32Const(0),
        I32Ne,
        LocalGet(7),
        I32Const(12),
        I32And,
        I32Const(0),
        I32Ne,
        I32And,
        If(BlockType::Empty),
    ]);
    type_error(
        b,
        &mut code,
        "Invalid property descriptor. Cannot both specify accessors and a value or writable attribute, #<Object>",
    );
    code.extend([
        End,
        LocalGet(2),
        LocalGet(3),
        Call(b.names["native-object-own-descriptor"]),
        LocalSet(9),
        I32Const(0),
        LocalSet(6),
        I32Const(0),
        LocalSet(10),
        LocalGet(9),
        I32Const(UNDEFINED),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    arr(&mut code, 9, 0);
    code.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        LocalSet(10),
        LocalGet(10),
        LocalSet(6),
    ]);
    arr(&mut code, 9, 1);
    code.push(LocalSet(11));
    // A requested descriptor kind differs from current iff either a data field
    // replaces an accessor or an accessor field replaces data.
    code.extend([
        LocalGet(10),
        I32Const(4),
        I32And,
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(7),
        I32Const(2),
        I32And,
        If(BlockType::Empty),
    ]);
    arr(&mut code, 5, 1);
    code.extend([Call(boolean), If(BlockType::Empty)]);
    changed(b, &mut code);
    code.extend([
        End,
        End,
        LocalGet(7),
        I32Const(1),
        I32And,
        If(BlockType::Empty),
    ]);
    arr(&mut code, 5, 0);
    code.extend([
        Call(boolean),
        LocalGet(10),
        I32Const(2),
        I32And,
        I32Eqz,
        I32Eqz,
        I32Ne,
        If(BlockType::Empty),
    ]);
    changed(b, &mut code);
    code.extend([End, End]);
    code.extend([
        LocalGet(10),
        I32Const(8),
        I32And,
        If(BlockType::Empty),
        LocalGet(7),
        I32Const(12),
        I32And,
        If(BlockType::Empty),
    ]);
    changed(b, &mut code);
    code.extend([End]);
    for (bit, index, old_index) in [(16, 4, 0), (32, 5, 1)] {
        code.extend([LocalGet(7), I32Const(bit), I32And, If(BlockType::Empty)]);
        arr(&mut code, 5, index);
        arr(&mut code, 11, old_index);
        code.extend([Call(same), I32Eqz, If(BlockType::Empty)]);
        changed(b, &mut code);
        code.extend([End, End]);
    }
    code.extend([
        Else,
        LocalGet(7),
        I32Const(48),
        I32And,
        If(BlockType::Empty),
    ]);
    changed(b, &mut code);
    code.extend([
        End,
        LocalGet(10),
        I32Const(1),
        I32And,
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(7),
        I32Const(8),
        I32And,
        If(BlockType::Empty),
    ]);
    arr(&mut code, 5, 3);
    code.extend([Call(boolean), If(BlockType::Empty)]);
    changed(b, &mut code);
    code.extend([
        End,
        End,
        LocalGet(7),
        I32Const(4),
        I32And,
        If(BlockType::Empty),
    ]);
    arr(&mut code, 5, 2);
    code.extend([LocalGet(11), Call(same), I32Eqz, If(BlockType::Empty)]);
    changed(b, &mut code);
    code.extend([End, End, End, End, End]);
    // Merge unspecified payload fields only if the requested kind is compatible.
    code.extend([
        LocalGet(10),
        I32Const(8),
        I32And,
        If(BlockType::Empty),
        LocalGet(7),
        I32Const(12),
        I32And,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    for (bit, index, old_index) in [(16, 4, 0), (32, 5, 1)] {
        code.extend([
            LocalGet(7),
            I32Const(bit),
            I32And,
            I32Eqz,
            If(BlockType::Empty),
        ]);
        arr(&mut code, 11, old_index);
        code.push(LocalSet(8));
        put(&mut code, 5, index, 8);
        code.push(End);
    }
    code.extend([
        Else,
        LocalGet(6),
        I32Const(!9),
        I32And,
        LocalSet(6),
        End,
        Else,
        LocalGet(7),
        I32Const(48),
        I32And,
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(7),
        I32Const(4),
        I32And,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    put(&mut code, 5, 2, 11);
    code.extend([
        End,
        Else,
        LocalGet(6),
        I32Const(!1),
        I32And,
        I32Const(8),
        I32Or,
        LocalSet(6),
        End,
        End,
        End,
    ]);
    code.extend([
        LocalGet(7),
        I32Const(48),
        I32And,
        If(BlockType::Empty),
        LocalGet(6),
        I32Const(!1),
        I32And,
        I32Const(8),
        I32Or,
        LocalSet(6),
        End,
    ]);
    for (presence, index, flag) in [(1, 0, 2), (2, 1, 4), (8, 3, 1)] {
        code.extend([
            LocalGet(7),
            I32Const(presence),
            I32And,
            If(BlockType::Empty),
            LocalGet(6),
            I32Const(!flag),
            I32And,
        ]);
        arr(&mut code, 5, index);
        code.extend([
            Call(boolean),
            I32Const(flag),
            I32Mul,
            I32Or,
            LocalSet(6),
            End,
        ]);
    }
    code.extend([
        LocalGet(2),
        LocalGet(3),
        LocalGet(6),
        I32Const(8),
        I32And,
        If(BlockType::Result(VALUE)),
    ]);
    arr(&mut code, 5, 4);
    arr(&mut code, 5, 5);
    code.extend([
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 2,
        },
        Else,
    ]);
    arr(&mut code, 5, 2);
    code.extend([
        End,
        LocalGet(6),
        Call(b.names["native-object-own-define"]),
        Drop,
        LocalGet(2),
    ]);
    let define = callback(
        b,
        &[
            (4, VALUE),
            (2, ValType::I32),
            (2, VALUE),
            (1, ValType::I32),
            (1, VALUE),
        ],
        &code,
    );
    code = vec![];
    for (index, local) in [(0, 2), (1, 3)] {
        argument(&mut code, index);
        code.push(LocalSet(local));
    }
    code.extend([
        LocalGet(3),
        Call(b.names["coerce-string"]),
        LocalSet(3),
        LocalGet(2),
        LocalGet(3),
        Call(b.names["native-object-own-descriptor"]),
        LocalSet(4),
        LocalGet(4),
        I32Const(UNDEFINED),
        RefI31,
        RefEq,
        If(BlockType::Empty),
        LocalGet(4),
        Return,
        End,
    ]);
    arr(&mut code, 4, 0);
    code.extend([RefCastNonNull(HeapType::I31), I31GetU, LocalSet(5)]);
    arr(&mut code, 4, 1);
    code.extend([
        LocalSet(6),
        Call(b.names["native-object-default-new"]),
        LocalSet(7),
    ]);
    for (key, index) in [
        ("get", 0),
        ("set", 1),
        ("value", 0),
        ("writable", 1),
        ("enumerable", 2),
        ("configurable", 4),
    ] {
        if matches!(key, "get" | "set" | "value" | "writable") {
            code.extend([LocalGet(5), I32Const(8), I32And]);
            if matches!(key, "value" | "writable") {
                code.push(I32Eqz);
            }
            code.push(If(BlockType::Empty));
        }
        code.push(LocalGet(7));
        text(&mut code, key);
        match key {
            "get" | "set" => arr(&mut code, 6, index),
            "value" => code.push(LocalGet(6)),
            _ => code.extend([
                LocalGet(5),
                I32Const(index),
                I32And,
                I32Eqz,
                I32Eqz,
                I32Const(2),
                I32Mul,
                I32Const(2),
                I32Add,
                RefI31,
            ]),
        }
        code.extend([I32Const(7), Call(b.names["native-object-own-define"]), Drop]);
        if matches!(key, "get" | "set" | "value" | "writable") {
            code.push(End);
        }
    }
    code.push(LocalGet(7));
    let get = callback(b, &[(3, VALUE), (1, ValType::I32), (2, VALUE)], &code);
    code = vec![];
    argument(&mut code, 0);
    code.push(Call(b.names["native-object-prototype"]));
    let get_proto = callback(b, &[], &code);
    code = vec![];
    for (index, local) in [(0, 2), (1, 3)] {
        argument(&mut code, index);
        code.push(LocalSet(local));
    }
    code.extend([
        LocalGet(2),
        Call(b.names["native-object-fields"]),
        Drop,
        LocalGet(3),
        I32Const(0),
        RefI31,
        RefEq,
        LocalGet(3),
        Call(b.names["native-object?"]),
        I32Or,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    type_error(
        b,
        &mut code,
        "Object prototype may only be an Object or null",
    );
    code.extend([
        End,
        LocalGet(3),
        Call(b.names["native-object-check-chain"]),
        LocalGet(3),
        LocalSet(4),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(4),
        I32Const(0),
        RefI31,
        RefEq,
        BrIf(1),
        LocalGet(4),
        LocalGet(2),
        RefEq,
        If(BlockType::Empty),
    ]);
    type_error(b, &mut code, "Cyclic __proto__ value");
    code.extend([
        End,
        LocalGet(4),
        Call(b.names["native-object-prototype"]),
        LocalSet(4),
        Br(0),
        End,
        End,
        LocalGet(2),
        LocalGet(3),
        Call(b.names["native-object-prototype-set"]),
        Drop,
        LocalGet(2),
    ]);
    let set_proto = callback(b, &[(3, VALUE)], &code);
    // Non-strict portable js-delete returns false for a nonconfigurable own
    // property. Rebuild the ordered owner table only after all checks complete.
    code = vec![];
    for (index, local) in [(0, 2), (1, 3)] {
        argument(&mut code, index);
        code.push(LocalSet(local));
    }
    code.extend([
        LocalGet(3),
        Call(b.names["coerce-string"]),
        LocalSet(3),
        LocalGet(2),
        LocalGet(3),
        Call(b.names["native-object-own-slot"]),
        LocalSet(4),
        LocalGet(4),
        I32Const(0),
        I32LtS,
        If(BlockType::Empty),
        I32Const(4),
        RefI31,
        Return,
        End,
        LocalGet(2),
        LocalGet(3),
        Call(b.names["native-object-own-descriptor"]),
        LocalSet(5),
    ]);
    arr(&mut code, 5, 0);
    code.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        I32Const(4),
        I32And,
        I32Eqz,
        If(BlockType::Empty),
        I32Const(2),
        RefI31,
        Return,
        End,
        LocalGet(2),
        Call(b.names["native-object-fields"]),
        LocalSet(5),
    ]);
    arr(&mut code, 5, 0);
    code.extend([
        LocalSet(6),
        LocalGet(6),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        LocalSet(7),
        LocalGet(7),
        I32Const(2),
        I32Sub,
        ArrayNewDefault(ARGS),
        LocalSet(8),
        LocalGet(8),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        LocalGet(6),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        LocalGet(4),
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
        LocalGet(8),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(4),
        LocalGet(6),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(4),
        I32Const(2),
        I32Add,
        LocalGet(7),
        LocalGet(4),
        I32Sub,
        I32Const(2),
        I32Sub,
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        LocalGet(8),
        ArraySet(ARGS),
        I32Const(4),
        RefI31,
    ]);
    let delete = callback(
        b,
        &[
            (2, VALUE),
            (1, ValType::I32),
            (2, VALUE),
            (1, ValType::I32),
            (1, VALUE),
        ],
        &code,
    );
    b.function(
        "js-delete-function",
        &[],
        &[VALUE],
        &[
            I32Const(0),
            RefI31,
            RefFunc(delete),
            I32Const(2),
            I32Const(2),
            Call(b.names["closure-new"]),
        ],
    );
    code = vec![
        LocalGet(0),
        I32Const(9),
        Call(b.names["closure-internal-property-get"]),
        I32Const(4),
        RefI31,
        RefEq,
        If(BlockType::Empty),
        LocalGet(0),
        Return,
        End,
        LocalGet(0),
        I32Const(9),
        I32Const(4),
        RefI31,
        Call(b.names["closure-internal-property-set"]),
        Drop,
    ];
    for (name, function, _arity) in [
        ("defineProperty", define, 3),
        ("getOwnPropertyDescriptor", get, 2),
        ("getPrototypeOf", get_proto, 1),
        ("setPrototypeOf", set_proto, 2),
    ] {
        code.push(LocalGet(0));
        text(&mut code, name);
        code.extend([
            I32Const(0),
            RefI31,
            RefFunc(function),
            I32Const(0),
            I32Const(-1),
            Call(b.names["closure-new"]),
            Call(b.names["fixed-named-property-set"]),
            Drop,
        ]);
    }
    code.push(LocalGet(0));
    let initialize = b.function(
        "object-static-initialize-implementation",
        &[VALUE],
        &[VALUE],
        &code,
    );
    (
        vec![define, get, get_proto, set_proto, delete, initialize],
        initialize,
    )
}
