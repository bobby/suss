//! Original native protocol fallback: tables belong to current callable values.
use super::*;
fn error(body: &mut Vec<Instruction<'static>>) {
    nominal::error(body);
}
fn cast_array(body: &mut Vec<Instruction<'static>>, local: u32) {
    body.extend([
        Instruction::LocalGet(local),
        Instruction::RefCastNonNull(HeapType::Concrete(ARGS)),
    ]);
}
fn key(body: &mut Vec<Instruction<'static>>, index: i32) {
    use Instruction::*;
    cast_array(body, 0);
    body.extend([I32Const(index), ArrayGet(ARGS)]);
}
fn callback(b: &mut Builder, body: &[Instruction<'static>]) -> u32 {
    let index = b.count;
    b.functions.function(INVOKE);
    let mut function = Function::new([(3, VALUE)]);
    for instruction in body {
        function.instruction(instruction);
    }
    function.instruction(&Instruction::End);
    b.code.function(&function);
    b.count += 1;
    index
}

pub(super) fn functions(
    b: &mut Builder,
    invoke: u32,
    method_get: u32,
    descriptor: u32,
) -> Vec<u32> {
    use Instruction::*;
    let mut body = vec![];
    for sentinel in [0, UNDEFINED] {
        body.extend([
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
    for sentinel in [2, 4] {
        body.extend([
            LocalGet(0),
            I32Const(sentinel),
            RefI31,
            RefEq,
            If(BlockType::Empty),
            I32Const(1),
            Return,
            End,
        ]);
    }
    body.extend([
        LocalGet(0),
        Call(b.names["source-array?"]),
        If(BlockType::Empty),
        I32Const(6),
        Return,
        End,
    ]);
    for (ty, kind) in [(NUMBER, 2), (STRING, 3), (4, 4), (ARGS, 6)] {
        body.extend([
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(ty)),
            If(BlockType::Empty),
            I32Const(kind),
            Return,
            End,
        ]);
    }
    body.push(I32Const(5));
    let kind = b.function("protocol-native-kind", &[VALUE], &[ValType::I32], &body);
    let property_get = b.names["closure-property-get"];
    let property_set = b.names["closure-property-set"];
    let mut body = vec![
        LocalGet(0),
        LocalGet(1),
        I32Const(4),
        RefI31,
        Call(property_set),
        Drop,
        I32Const(1),
    ];
    b.function(
        "protocol-native-marker-set",
        &[VALUE, ValType::I32],
        &[ValType::I32],
        &body,
    );
    body = vec![
        LocalGet(2),
        RefTestNonNull(HeapType::Concrete(4)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    error(&mut body);
    body.extend([
        End,
        LocalGet(0),
        LocalGet(1),
        LocalGet(2),
        Call(property_set),
    ]);
    b.function(
        "protocol-native-method-set",
        &[VALUE, ValType::I32, VALUE],
        &[VALUE],
        &body,
    );

    // A valid protocol signature can select a plain native implementation with
    // a different fixed parameter count. The pin's native function call fills
    // absent slots with undefined and ignores surplus values (already evaluated).
    let mut body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(4)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    error(&mut body);
    body.extend([
        End,
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    for (field, local) in [(2, 2), (3, 3)] {
        body.extend([
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(4)),
            StructGet {
                struct_type_index: 4,
                field_index: field,
            },
            LocalSet(local),
        ]);
    }
    body.extend([
        LocalGet(2),
        LocalGet(3),
        I32Eq,
        If(BlockType::Empty),
        LocalGet(2),
        I32Const(0),
        I32LtS,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    body.extend([
        LocalGet(1),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        LocalSet(3),
        LocalGet(2),
        LocalGet(3),
        I32Ne,
        If(BlockType::Empty),
        I32Const(UNDEFINED),
        RefI31,
        LocalGet(2),
        ArrayNew(ARGS),
        LocalSet(4),
        LocalGet(4),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        LocalGet(1),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        LocalGet(2),
        LocalGet(3),
        I32LtU,
        If(BlockType::Result(ValType::I32)),
        LocalGet(2),
        Else,
        LocalGet(3),
        End,
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
        LocalGet(0),
        LocalGet(4),
        Call(invoke),
        Return,
        End,
        End,
        LocalGet(0),
        LocalGet(1),
        Call(invoke),
    ]);
    let native_invoke = b.function_with_locals(
        "protocol-native-invoke",
        &[VALUE, VALUE],
        &[VALUE],
        &[(2, ValType::I32), (1, VALUE)],
        &body,
    );

    // First direct object method, then CURRENT method's specific/default table.
    let mut body = vec![
        LocalGet(1),
        I32Const(0),
        ArrayGet(ARGS),
        LocalSet(2),
        LocalGet(2),
        RefTestNonNull(HeapType::Concrete(7)),
        If(BlockType::Empty),
        LocalGet(2),
        Call(descriptor),
    ];
    key(&mut body, 0);
    body.extend([
        Call(method_get),
        LocalSet(3),
        LocalGet(3),
        RefTestNonNull(HeapType::Concrete(4)),
        If(BlockType::Empty),
        LocalGet(3),
        LocalGet(1),
        Call(invoke),
        Return,
        End,
        End,
    ]);
    key(&mut body, 1);
    body.extend([
        Call(b.names["binding-get"]),
        LocalSet(4),
        LocalGet(4),
        LocalGet(2),
        Call(kind),
        Call(property_get),
        LocalSet(3),
        LocalGet(3),
        I32Const(0),
        RefI31,
        RefEq,
        If(BlockType::Empty),
        LocalGet(4),
        I32Const(7),
        Call(property_get),
        LocalSet(3),
        End,
        LocalGet(3),
        RefTestNonNull(HeapType::Concrete(4)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.extend([End, LocalGet(3), LocalGet(1), Call(native_invoke)]);
    let dispatch = callback(b, &body);
    body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(DESCRIPTOR)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    error(&mut body);
    body.push(End);
    body.extend([
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(DESCRIPTOR)),
        StructGet {
            struct_type_index: DESCRIPTOR,
            field_index: 1,
        },
        LocalSet(3),
        LocalGet(3),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.extend([
        End,
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(5)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.extend([
        End,
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        LocalSet(2),
        LocalGet(2),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    body.extend([
        LocalGet(0),
        LocalGet(1),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 2,
        },
        RefFunc(dispatch),
        LocalGet(2),
        LocalGet(2),
        Call(b.names["closure-new"]),
    ]);
    b.function_with_locals(
        "protocol-live-dispatcher-new",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, ValType::I32), (1, VALUE)],
        &body,
    );

    // The macro's object marker fast path remains separate. Native fallback
    // examines the current protocol value, not the stable syntactic marker.
    body = vec![LocalGet(0), RefIsNull, If(BlockType::Empty)];
    error(&mut body);
    body.push(End);
    for sentinel in [0, UNDEFINED] {
        body.extend([
            LocalGet(0),
            I32Const(sentinel),
            RefI31,
            RefEq,
            If(BlockType::Empty),
        ]);
        error(&mut body);
        body.push(End);
    }
    body.extend([
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(7)),
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    body.extend([
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(4)),
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        LocalGet(0),
        LocalGet(1),
        Call(kind),
        Call(property_get),
        LocalSet(2),
        LocalGet(2),
        I32Const(0),
        RefI31,
        RefEq,
        If(BlockType::Empty),
        LocalGet(0),
        I32Const(7),
        Call(property_get),
        LocalSet(2),
        End,
        LocalGet(2),
        I32Const(0),
        RefI31,
        RefEq,
        I32Eqz,
    ]);
    let satisfies = b.function_with_locals(
        "protocol-native-satisfies",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[(1, VALUE)],
        &body,
    );
    let function_body = vec![
        LocalGet(1),
        I32Const(0),
        ArrayGet(ARGS),
        LocalGet(1),
        I32Const(1),
        ArrayGet(ARGS),
        Call(satisfies),
        I32Const(2),
        I32Mul,
        I32Const(2),
        I32Add,
        RefI31,
    ];
    let function = callback(b, &function_body);
    b.function(
        "native-satisfies-function",
        &[],
        &[VALUE],
        &[
            I32Const(0),
            RefI31,
            RefFunc(function),
            I32Const(2),
            I32Const(2),
            Call(b.names["closure-new"]),
        ],
    );
    vec![dispatch, function]
}
