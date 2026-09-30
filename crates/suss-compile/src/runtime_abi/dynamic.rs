//! Original rooted dynamic frames. Entries are private cell/old/current triplets.
use super::*;
pub(super) const CURRENT: u32 = nominal::SENTINEL_GLOBAL + 1;
const FRAME: u32 = 9;
fn error(body: &mut Vec<Instruction<'static>>) {
    use Instruction::*;
    let text: Vec<_> = "Invalid dynamic binding frame".encode_utf16().collect();
    body.push(GlobalGet(nominal::ERROR_GLOBAL));
    body.extend(text.iter().map(|unit| I32Const(*unit as i32)));
    body.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: text.len() as u32,
        },
        I32Const(0),
        RefI31,
        I32Const(0),
        RefI31,
        StructNew(8),
        Throw(0),
    ]);
}
fn guard(body: &mut Vec<Instruction<'static>>, local: u32, ty: u32) {
    use Instruction::*;
    body.extend([
        LocalGet(local),
        RefTestNonNull(HeapType::Concrete(ty)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    error(body);
    body.push(End);
}
fn array(body: &mut Vec<Instruction<'static>>, local: u32) {
    body.extend([
        Instruction::LocalGet(local),
        Instruction::RefCastNonNull(HeapType::Concrete(ARGS)),
    ]);
}
fn frame(body: &mut Vec<Instruction<'static>>, local: u32, field: u32) {
    body.extend([
        Instruction::LocalGet(local),
        Instruction::RefCastNonNull(HeapType::Concrete(FRAME)),
        Instruction::StructGet {
            struct_type_index: FRAME,
            field_index: field,
        },
    ]);
}
fn shape(body: &mut Vec<Instruction<'static>>, entries: u32) {
    use Instruction::*;
    guard(body, entries, ARGS);
    array(body, entries);
    body.extend([ArrayLen, I32Const(i32::MAX), I32GtU, If(BlockType::Empty)]);
    error(body);
    body.push(End);
    array(body, entries);
    body.extend([ArrayLen, I32Const(3), I32RemU, If(BlockType::Empty)]);
    error(body);
    body.push(End);
}
fn check_cell(body: &mut Vec<Instruction<'static>>) {
    use Instruction::*;
    body.extend([
        RefTestNonNull(HeapType::Concrete(5)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    error(body);
    body.push(End);
}
pub(super) fn lookup(b: &mut Builder) -> u32 {
    use Instruction::*;
    let mut body = vec![
        LocalGet(1),
        LocalSet(2),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(2),
        I32Const(0),
        RefI31,
        RefEq,
        BrIf(1),
    ];
    guard(&mut body, 2, FRAME);
    frame(&mut body, 2, 1);
    body.push(LocalSet(3));
    shape(&mut body, 3);
    array(&mut body, 3);
    body.extend([
        ArrayLen,
        LocalSet(4),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(4),
        I32Eqz,
        BrIf(1),
        LocalGet(4),
        I32Const(3),
        I32Sub,
        LocalSet(4),
    ]);
    array(&mut body, 3);
    body.extend([LocalGet(4), ArrayGet(ARGS)]);
    check_cell(&mut body);
    array(&mut body, 3);
    body.extend([
        LocalGet(4),
        ArrayGet(ARGS),
        LocalGet(0),
        RefEq,
        If(BlockType::Empty),
        LocalGet(3),
        LocalGet(4),
        I32Const(2),
        I32Add,
        Return,
        End,
        Br(0),
        End,
        End,
    ]);
    frame(&mut body, 2, 0);
    body.extend([
        LocalSet(2),
        Br(0),
        End,
        End,
        I32Const(0),
        RefI31,
        I32Const(-1),
    ]);
    b.function_with_locals(
        "dynamic-find",
        &[VALUE, VALUE],
        &[VALUE, ValType::I32],
        &[(2, VALUE), (1, ValType::I32)],
        &body,
    )
}
pub(super) fn binding_get(b: &mut Builder, lookup: u32, root: Vec<Instruction<'static>>) {
    use Instruction::*;
    let mut body = vec![
        LocalGet(0),
        GlobalGet(CURRENT),
        Call(lookup),
        LocalSet(2),
        LocalSet(1),
        LocalGet(2),
        I32Const(0),
        I32GeS,
        If(BlockType::Result(VALUE)),
    ];
    array(&mut body, 1);
    body.extend([LocalGet(2), ArrayGet(ARGS), Else]);
    body.extend(root);
    body.push(End);
    b.function_with_locals(
        "binding-get",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE), (1, ValType::I32)],
        &body,
    );
}
pub(super) fn binding_set(b: &mut Builder, lookup: u32) -> u32 {
    use Instruction::*;
    let mut body = vec![
        LocalGet(0),
        LocalGet(2),
        Call(lookup),
        LocalSet(4),
        LocalSet(3),
        LocalGet(4),
        I32Const(0),
        I32GeS,
        If(BlockType::Empty),
    ];
    array(&mut body, 3);
    body.extend([
        LocalGet(4),
        LocalGet(1),
        ArraySet(ARGS),
        Else,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(5)),
        LocalGet(1),
        StructSet {
            struct_type_index: 5,
            field_index: 0,
        },
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(5)),
        I32Const(1),
        StructSet {
            struct_type_index: 5,
            field_index: 1,
        },
        End,
    ]);
    let set_in = b.function_with_locals(
        "binding-set-in",
        &[VALUE, VALUE, VALUE],
        &[],
        &[(1, VALUE), (1, ValType::I32)],
        &body,
    );
    b.function(
        "binding-set",
        &[VALUE, VALUE],
        &[],
        &[LocalGet(0), LocalGet(1), GlobalGet(CURRENT), Call(set_in)],
    );
    set_in
}
pub(super) fn functions(b: &mut Builder, binding_set: u32, try_invoke: u32) -> Vec<u32> {
    use Instruction::*;
    // Validate every cell and complete storage before publishing a new frame.
    let mut body = vec![];
    shape(&mut body, 0);
    array(&mut body, 0);
    body.extend([
        ArrayLen,
        LocalSet(2),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(2),
        I32Eqz,
        BrIf(1),
        LocalGet(2),
        I32Const(3),
        I32Sub,
        LocalSet(2),
    ]);
    array(&mut body, 0);
    body.extend([LocalGet(2), ArrayGet(ARGS)]);
    check_cell(&mut body);
    body.extend([Br(0), End, End]);
    array(&mut body, 0);
    body.extend([ArrayLen, ArrayNewDefault(ARGS), LocalSet(1)]);
    array(&mut body, 1);
    body.push(I32Const(0));
    array(&mut body, 0);
    body.push(I32Const(0));
    array(&mut body, 0);
    body.extend([
        ArrayLen,
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
        GlobalGet(CURRENT),
    ]);
    array(&mut body, 1);
    body.extend([
        StructNew(FRAME),
        LocalTee(3),
        GlobalSet(CURRENT),
        LocalGet(3),
    ]);
    let push = b.function_with_locals(
        "dynamic-push",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE), (1, ValType::I32), (1, VALUE)],
        &body,
    );
    let mut body = vec![];
    guard(&mut body, 0, FRAME);
    body.extend([
        LocalGet(0),
        GlobalGet(CURRENT),
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    frame(&mut body, 0, 1);
    body.push(LocalSet(1));
    shape(&mut body, 1);
    array(&mut body, 1);
    body.extend([
        ArrayLen,
        LocalSet(3),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(3),
        I32Eqz,
        BrIf(1),
        LocalGet(3),
        I32Const(3),
        I32Sub,
        LocalSet(3),
    ]);
    array(&mut body, 1);
    body.extend([LocalGet(3), ArrayGet(ARGS)]);
    check_cell(&mut body);
    body.extend([Br(0), End, End]);
    frame(&mut body, 0, 0);
    body.push(LocalSet(2));
    body.extend([
        LocalGet(2),
        I32Const(0),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    guard(&mut body, 2, FRAME);
    body.push(End);
    array(&mut body, 1);
    body.extend([
        ArrayLen,
        LocalSet(3),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(3),
        I32Eqz,
        BrIf(1),
        LocalGet(3),
        I32Const(3),
        I32Sub,
        LocalSet(3),
    ]);
    array(&mut body, 1);
    body.extend([LocalGet(3), ArrayGet(ARGS)]);
    array(&mut body, 1);
    body.extend([
        LocalGet(3),
        I32Const(1),
        I32Add,
        ArrayGet(ARGS),
        LocalGet(2),
        Call(binding_set),
        Br(0),
        End,
        End,
    ]);
    // Publish the parent only after all snapshot writes have completed. A fuel
    // trap leaves this frame rooted so native recovery can retry idempotently.
    body.extend([LocalGet(2), GlobalSet(CURRENT)]);
    let pop = b.function_with_locals(
        "dynamic-pop",
        &[VALUE],
        &[],
        &[(2, VALUE), (1, ValType::I32)],
        &body,
    );
    let callback = b.count;
    b.functions.function(INVOKE);
    let mut body = Function::new([]);
    for inst in [LocalGet(0), Call(pop), I32Const(0), RefI31, End] {
        body.instruction(&inst);
    }
    b.code.function(&body);
    b.count += 1;
    b.function_with_locals(
        "dynamic-invoke",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &[
            LocalGet(0),
            Call(push),
            LocalSet(2),
            LocalGet(1),
            I32Const(0),
            RefI31,
            LocalGet(2),
            RefFunc(callback),
            I32Const(0),
            I32Const(0),
            StructNew(4),
            Call(try_invoke),
        ],
    );
    vec![callback]
}
