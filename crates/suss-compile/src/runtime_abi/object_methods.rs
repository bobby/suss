//! Original GC-owned Object methods. Keys and unbound functions live on the
//! class descriptor; no registry retains old classes or instance owners.
use super::*;
pub(super) const TAG_GLOBAL: u32 = arrays::TAG_GLOBAL + 1;
pub(super) const DEFAULT_THIS: u32 = TAG_GLOBAL + 1;
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
fn get(body: &mut Vec<Instruction<'static>>, local: u32, ty: u32, field: u32) {
    use Instruction::*;
    body.extend([
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(ty)),
        StructGet {
            struct_type_index: ty,
            field_index: field,
        },
    ]);
}
fn array(body: &mut Vec<Instruction<'static>>, local: u32) {
    use Instruction::*;
    body.extend([LocalGet(local), RefCastNonNull(HeapType::Concrete(ARGS))]);
}
fn callback(b: &mut Builder, instructions: &[Instruction<'static>]) -> u32 {
    let index = b.count;
    b.functions.function(INVOKE);
    let mut body = Function::new([]);
    for instruction in instructions {
        body.instruction(instruction);
    }
    body.instruction(&Instruction::End);
    b.code.function(&body);
    b.count += 1;
    index
}
pub(super) fn functions(b: &mut Builder, equal: u32) -> Vec<u32> {
    use Instruction::*;
    // Validate the entire descriptor table even when an early name matches.
    let mut body = vec![];
    guard(&mut body, 0, DESCRIPTOR);
    guard(&mut body, 1, STRING);
    get(&mut body, 0, DESCRIPTOR, 2);
    body.push(LocalSet(2));
    guard(&mut body, 2, ARGS);
    array(&mut body, 2);
    body.extend([ArrayLen, I32Const(1), I32And, If(BlockType::Empty)]);
    nominal::error(&mut body);
    body.extend([
        End,
        I32Const(0),
        LocalSet(3),
        I32Const(0),
        RefI31,
        LocalSet(5),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(3),
    ]);
    array(&mut body, 2);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    array(&mut body, 2);
    body.extend([LocalGet(3), ArrayGet(ARGS), LocalSet(4)]);
    guard(&mut body, 4, DESCRIPTOR);
    get(&mut body, 4, DESCRIPTOR, 3);
    body.extend([GlobalGet(TAG_GLOBAL), RefEq, If(BlockType::Empty)]);
    get(&mut body, 4, DESCRIPTOR, 1);
    body.push(LocalSet(6));
    guard(&mut body, 6, ARGS);
    array(&mut body, 6);
    body.extend([ArrayLen, I32Const(1), I32Ne, If(BlockType::Empty)]);
    nominal::error(&mut body);
    body.push(End);
    array(&mut body, 6);
    body.extend([I32Const(0), ArrayGet(ARGS), LocalSet(6)]);
    guard(&mut body, 6, STRING);
    body.extend([
        LocalGet(6),
        LocalGet(1),
        Call(equal),
        If(BlockType::Empty),
        LocalGet(4),
        LocalSet(5),
        End,
        End,
        LocalGet(3),
        I32Const(2),
        I32Add,
        LocalSet(3),
        Br(0),
        End,
        End,
        LocalGet(5),
    ]);
    let find = b.function_with_locals(
        "object-method-key",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE), (1, ValType::I32), (3, VALUE)],
        &body,
    );

    // args are private invocation buffers; prepend the anchored physical this.
    let mut body = vec![];
    guard(&mut body, 0, 4);
    guard(&mut body, 2, ARGS);
    body.extend([
        LocalGet(0),
        Call(b.names["closure-environment"]),
        LocalSet(3),
    ]);
    body.extend([
        LocalGet(3),
        RefTestNonNull(HeapType::Concrete(7)),
        If(BlockType::Empty),
    ]);
    get(&mut body, 3, 7, 0);
    body.extend([GlobalGet(TAG_GLOBAL), RefEq, If(BlockType::Empty)]);
    get(&mut body, 3, 7, 1);
    body.push(LocalSet(3));
    guard(&mut body, 3, ARGS);
    array(&mut body, 3);
    body.extend([ArrayLen, I32Const(1), I32Ne, If(BlockType::Empty)]);
    nominal::error(&mut body);
    body.push(End);
    array(&mut body, 2);
    body.extend([
        ArrayLen,
        LocalTee(4),
        I32Const(arrays::MAX_LENGTH - 1),
        I32GtU,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(4),
        I32Const(1),
        I32Add,
        ArrayNewDefault(ARGS),
        LocalSet(5),
    ]);
    array(&mut body, 5);
    body.extend([I32Const(0), LocalGet(1), ArraySet(ARGS)]);
    array(&mut body, 5);
    body.push(I32Const(1));
    array(&mut body, 2);
    body.extend([
        I32Const(0),
        LocalGet(4),
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
    ]);
    array(&mut body, 3);
    body.extend([
        I32Const(0),
        ArrayGet(ARGS),
        LocalGet(5),
        Call(b.names["protocol-native-invoke"]),
        Return,
        End,
        End,
        LocalGet(0),
        LocalGet(2),
        Call(b.names["protocol-native-invoke"]),
    ]);
    let invoke = b.function_with_locals(
        "object-method-invoke",
        &[VALUE, VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE), (1, ValType::I32), (1, VALUE)],
        &body,
    );
    // A plain detached call uses the non-strict portable realm's implicit this.
    // Rebuild the wrapper from its already unwrapped callback environment.
    let detached_index = b.count;
    let detached = callback(
        b,
        &[
            LocalGet(0),
            RefFunc(detached_index),
            I32Const(0),
            I32Const(-1),
            Call(b.names["closure-new"]),
            GlobalGet(DEFAULT_THIS),
            LocalGet(1),
            Call(invoke),
        ],
    );

    let mut body = vec![
        LocalGet(0),
        Call(b.names["constructor-descriptor"]),
        LocalSet(3),
    ];
    guard(&mut body, 1, STRING);
    guard(&mut body, 2, 4);
    body.extend([
        LocalGet(3),
        LocalGet(1),
        Call(find),
        LocalSet(4),
        LocalGet(4),
        I32Const(0),
        RefI31,
        RefEq,
        If(BlockType::Empty),
        I64Const(0),
        LocalGet(1),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 1,
        },
        I32Const(0),
        ArrayNewDefault(ARGS),
        GlobalGet(TAG_GLOBAL),
        StructNew(DESCRIPTOR),
        LocalSet(4),
        End,
        GlobalGet(TAG_GLOBAL),
        LocalGet(2),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 1,
        },
        I32Const(0),
        RefI31,
        StructNew(7),
        RefFunc(detached),
        I32Const(0),
        I32Const(-1),
        Call(b.names["closure-new"]),
        LocalSet(5),
        LocalGet(3),
        LocalGet(4),
        LocalGet(5),
        Call(b.names["protocol-method-set"]),
        LocalGet(5),
    ]);
    b.function_with_locals(
        "object-method-set",
        &[VALUE, VALUE, VALUE],
        &[VALUE],
        &[(3, VALUE)],
        &body,
    );
    vec![detached]
}
