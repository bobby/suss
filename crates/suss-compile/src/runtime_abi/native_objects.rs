//! Original owned property storage and raw prototype-chain kernel.
//! Source accessors and default Object prototype methods are a separate adapter.
use super::*;
pub(super) const TAG_GLOBAL: u32 = string_methods::METHOD_ROOT + 1;
fn guard(body: &mut Vec<Instruction<'static>>, local: u32, ty: u32) {
    use Instruction::*;
    body.extend([
        LocalGet(local),
        RefTestNonNull(HeapType::Concrete(ty)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(body);
    body.push(End);
}
fn array(body: &mut Vec<Instruction<'static>>, local: u32) {
    body.extend([
        Instruction::LocalGet(local),
        Instruction::RefCastNonNull(HeapType::Concrete(ARGS)),
    ]);
}
pub(super) fn functions(b: &mut Builder) {
    use Instruction::*;
    let descriptor = descriptor_functions(b);
    b.function(
        "native-object?",
        &[VALUE],
        &[ValType::I32],
        &[
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(7)),
            If(BlockType::Result(ValType::I32)),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(7)),
            StructGet {
                struct_type_index: 7,
                field_index: 0,
            },
            GlobalGet(TAG_GLOBAL),
            RefEq,
            Else,
            I32Const(0),
            End,
        ],
    );
    b.function(
        "native-object-new",
        &[],
        &[VALUE],
        &[
            GlobalGet(TAG_GLOBAL),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            RefI31,
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 2,
            },
            I32Const(0),
            RefI31,
            I32Const(0), RefI31, StructNew(7),
        ],
    );
    let mut body = vec![];
    guard(&mut body, 0, 7);
    body.extend([
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(7)),
        StructGet {
            struct_type_index: 7,
            field_index: 0,
        },
        GlobalGet(TAG_GLOBAL),
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(7)),
        StructGet {
            struct_type_index: 7,
            field_index: 1,
        },
        LocalSet(1),
    ]);
    array(&mut body, 1);
    body.extend([ArrayLen, I32Const(2), I32Ne, If(BlockType::Empty)]);
    nominal::error(&mut body);
    body.push(End);
    array(&mut body, 1);
    body.extend([I32Const(0), ArrayGet(ARGS), LocalSet(2)]);
    guard(&mut body, 2, ARGS);
    array(&mut body, 2);
    body.extend([ArrayLen, I32Const(1), I32And, If(BlockType::Empty)]);
    nominal::error(&mut body);
    body.push(End);
    body.extend([
        I32Const(0),
        LocalSet(3),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(3),
    ]);
    array(&mut body, 2);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    array(&mut body, 2);
    body.extend([LocalGet(3), ArrayGet(ARGS), LocalSet(4)]);
    guard(&mut body, 4, STRING);
    array(&mut body, 2);
    body.extend([
        LocalGet(3),
        I32Const(1),
        I32Add,
        ArrayGet(ARGS),
        Call(descriptor),
        Drop,
    ]);
    body.extend([
        LocalGet(3),
        I32Const(2),
        I32Add,
        LocalSet(3),
        Br(0),
        End,
        End,
        LocalGet(1),
    ]);
    let fields = b.function_with_locals(
        "native-object-fields",
        &[VALUE],
        &[VALUE],
        &[(2, VALUE), (1, ValType::I32), (1, VALUE)],
        &body,
    );
    let mut body = vec![
        LocalGet(0),
        Call(fields),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        LocalSet(2),
    ];
    guard(&mut body, 1, STRING);
    body.extend([
        I32Const(0),
        LocalSet(3),
        I32Const(-1),
        LocalSet(4),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(3),
    ]);
    array(&mut body, 2);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    array(&mut body, 2);
    body.extend([
        LocalGet(3),
        ArrayGet(ARGS),
        LocalGet(1),
        Call(b.names["property-key-equal"]),
        If(BlockType::Empty),
        LocalGet(3),
        LocalSet(4),
        End,
        LocalGet(3),
        I32Const(2),
        I32Add,
        LocalSet(3),
        Br(0),
        End,
        End,
        LocalGet(4),
    ]);
    let slot = b.function_with_locals(
        "native-object-own-slot",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[(1, VALUE), (2, ValType::I32)],
        &body,
    );
    let mut body = vec![
        LocalGet(0),
        LocalGet(1),
        Call(slot),
        LocalSet(2),
        LocalGet(2),
        I32Const(0),
        I32GeS,
        If(BlockType::Result(VALUE)),
        LocalGet(0),
        Call(fields),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(2),
        I32Const(1),
        I32Add,
        ArrayGet(ARGS),
        Else,
        I32Const(UNDEFINED),
        RefI31,
        End,
    ];
    b.function_with_locals(
        "native-object-own-descriptor",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, ValType::I32)],
        &body,
    );
    b.function_with_locals(
        "native-object-own-get",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &[
            LocalGet(0),
            LocalGet(1),
            Call(b.names["native-object-own-descriptor"]),
            LocalSet(2),
            LocalGet(2),
            I32Const(UNDEFINED),
            RefI31,
            RefEq,
            If(BlockType::Result(VALUE)),
            LocalGet(2),
            Else,
            LocalGet(2),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            I32Const(1),
            ArrayGet(ARGS),
            End,
        ],
    );
    body = vec![
        LocalGet(0),
        LocalGet(1),
        Call(slot),
        LocalSet(3),
        LocalGet(2),
        Call(descriptor),
        Drop,
    ];
    body.extend([LocalGet(0), Call(fields), LocalSet(4)]);
    array(&mut body, 4);
    body.extend([
        I32Const(0),
        ArrayGet(ARGS),
        LocalSet(5),
        LocalGet(3),
        I32Const(0),
        I32GeS,
        If(BlockType::Empty),
    ]);
    array(&mut body, 5);
    body.extend([
        LocalGet(3),
        I32Const(1),
        I32Add,
        LocalGet(2),
        ArraySet(ARGS),
        LocalGet(2),
        Return,
        End,
    ]);
    array(&mut body, 5);
    body.extend([
        ArrayLen,
        LocalSet(3),
        LocalGet(3),
        I32Const(arrays::MAX_LENGTH - 2),
        I32GtU,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(3),
        I32Const(2),
        I32Add,
        ArrayNewDefault(ARGS),
        LocalSet(6),
    ]);
    array(&mut body, 6);
    body.push(I32Const(0));
    array(&mut body, 5);
    body.extend([
        I32Const(0),
        LocalGet(3),
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
    ]);
    array(&mut body, 6);
    body.extend([LocalGet(3), LocalGet(1), ArraySet(ARGS)]);
    array(&mut body, 6);
    body.extend([
        LocalGet(3),
        I32Const(1),
        I32Add,
        LocalGet(2),
        ArraySet(ARGS),
    ]);
    array(&mut body, 4);
    body.extend([I32Const(0), LocalGet(6), ArraySet(ARGS), LocalGet(2)]);
    b.function_with_locals(
        "native-object-own-store",
        &[VALUE, VALUE, VALUE],
        &[VALUE],
        &[(1, ValType::I32), (3, VALUE)],
        &body,
    );
    let store = b.names["native-object-own-store"];
    b.function_with_locals(
        "native-object-own-define",
        &[VALUE, VALUE, VALUE, ValType::I32],
        &[VALUE],
        &[],
        &{
            let mut code = vec![LocalGet(3), I32Const(15), I32GtU, If(BlockType::Empty)];
            nominal::error(&mut code);
            code.push(End);
            code.extend([
                LocalGet(0),
                LocalGet(1),
                LocalGet(3),
                RefI31,
                LocalGet(2),
                ArrayNewFixed {
                    array_type_index: ARGS,
                    array_size: 2,
                },
                Call(store),
                Drop,
                LocalGet(2),
            ]);
            code
        },
    );
    b.function(
        "native-object-own-set",
        &[VALUE, VALUE, VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            LocalGet(1),
            LocalGet(2),
            I32Const(7),
            Call(b.names["native-object-own-define"]),
        ],
    );
    prototype_functions(b, fields, slot);
}

// These operations manipulate raw internal prototypes. Source __proto__ accessor
// semantics and the default Object prototype are implemented above this layer.
fn prototype_functions(b: &mut Builder, fields: u32, slot: u32) {
    use Instruction::*;
    let mut body = vec![
        LocalGet(0),
        Call(fields),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),
        ArrayGet(ARGS),
        LocalSet(1),
        LocalGet(1),
        I32Const(0),
        RefI31,
        RefEq,
        If(BlockType::Empty),
        LocalGet(1),
        Return,
        End,
        LocalGet(1),
        Call(fields),
        Drop,
        LocalGet(1),
    ];
    let prototype = b.function_with_locals(
        "native-object-prototype",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &body,
    );
    let step = b.function(
        "native-object-prototype-step",
        &[VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            I32Const(0),
            RefI31,
            RefEq,
            If(BlockType::Result(VALUE)),
            LocalGet(0),
            Else,
            LocalGet(0),
            Call(prototype),
            End,
        ],
    );
    // Floyd's algorithm rejects even host-forged cycles without an arbitrary
    // semantic depth cap, recursion, or a process-side registry.
    body = vec![
        LocalGet(0),
        LocalSet(1),
        LocalGet(0),
        LocalSet(2),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(2),
        I32Const(0),
        RefI31,
        RefEq,
        BrIf(1),
        LocalGet(1),
        Call(step),
        LocalSet(1),
        LocalGet(2),
        Call(step),
        Call(step),
        LocalSet(2),
        LocalGet(2),
        I32Const(0),
        RefI31,
        RefEq,
        BrIf(1),
        LocalGet(1),
        LocalGet(2),
        RefEq,
        If(BlockType::Empty),
    ];
    nominal::error(&mut body);
    body.extend([End, Br(0), End, End]);
    let check = b.function_with_locals(
        "native-object-check-chain",
        &[VALUE],
        &[],
        &[(2, VALUE)],
        &body,
    );
    body = vec![
        LocalGet(0),
        Call(fields),
        LocalSet(2),
        LocalGet(1),
        Call(check),
        LocalGet(1),
        LocalSet(3),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(3),
        I32Const(0),
        RefI31,
        RefEq,
        BrIf(1),
        LocalGet(3),
        LocalGet(0),
        RefEq,
        If(BlockType::Empty),
    ];
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(3),
        Call(prototype),
        LocalSet(3),
        Br(0),
        End,
        End,
    ]);
    array(&mut body, 2);
    body.extend([I32Const(1), LocalGet(1), ArraySet(ARGS), LocalGet(1)]);
    b.function_with_locals(
        "native-object-prototype-set",
        &[VALUE, VALUE],
        &[VALUE],
        &[(2, VALUE)],
        &body,
    );
    body = vec![LocalGet(0), Call(fields), Drop];
    guard(&mut body, 1, STRING);
    body.extend([
        LocalGet(0),
        Call(check),
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
        LocalGet(2),
        LocalGet(1),
        Call(b.names["native-object-own-get"]),
        Return,
        End,
        LocalGet(2),
        Call(prototype),
        LocalSet(2),
        Br(0),
        End,
        End,
        I32Const(UNDEFINED),
        RefI31,
    ]);
    b.function_with_locals(
        "native-object-chain-get",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &body,
    );
}

// Private GC-owned descriptors: [attribute flags, data value/accessor pair].
// Flags: writable1, enumerable2, configurable4, accessor8. The shared array
// type is reused; no new type group or process registry is introduced.
fn descriptor_functions(b: &mut Builder) -> u32 {
    use Instruction::*;
    let mut code = vec![];
    guard(&mut code, 0, ARGS);
    array(&mut code, 0);
    code.extend([ArrayLen, I32Const(2), I32Ne, If(BlockType::Empty)]);
    nominal::error(&mut code);
    code.push(End);
    array(&mut code, 0);
    code.extend([I32Const(0), ArrayGet(ARGS), LocalSet(1)]);
    code.extend([
        LocalGet(1),
        RefTestNonNull(HeapType::I31),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut code);
    code.push(End);
    code.extend([
        LocalGet(1),
        RefCastNonNull(HeapType::I31),
        I31GetU,
        LocalSet(2),
        LocalGet(2),
        I32Const(15),
        I32GtU,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut code);
    code.push(End);
    array(&mut code, 0);
    code.extend([
        I32Const(1),
        ArrayGet(ARGS),
        LocalSet(1),
        LocalGet(1),
        RefIsNull,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut code);
    code.extend([End, LocalGet(2), I32Const(8), I32And, If(BlockType::Empty)]);
    code.extend([LocalGet(2), I32Const(1), I32And, If(BlockType::Empty)]);
    nominal::error(&mut code);
    code.push(End);
    guard(&mut code, 1, ARGS);
    array(&mut code, 1);
    code.extend([ArrayLen, I32Const(2), I32Ne, If(BlockType::Empty)]);
    nominal::error(&mut code);
    code.push(End);
    for index in [0, 1] {
        array(&mut code, 1);
        code.extend([
            I32Const(index),
            ArrayGet(ARGS),
            LocalSet(3),
            LocalGet(3),
            RefTestNonNull(HeapType::Concrete(4)),
            LocalGet(3),
            I32Const(UNDEFINED),
            RefI31,
            RefEq,
            I32Or,
            I32Eqz,
            If(BlockType::Empty),
        ]);
        nominal::error(&mut code);
        code.push(End);
    }
    code.extend([End, LocalGet(0)]);
    b.function_with_locals(
        "native-object-descriptor-check",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE), (1, ValType::I32), (1, VALUE)],
        &code,
    )
}
