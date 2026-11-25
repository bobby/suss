//! Original rooted dynamic frames. Entries are private cell/old/current triplets.
use super::*;
mod redefs;
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
        sentinel_value(NIL),
        sentinel_value(NIL),
        I32Const(0), // Identity UID, unassigned.
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
/// The flags of the cell on the stack.
fn flags() -> [Instruction<'static>; 2] {
    [
        Instruction::RefCastNonNull(HeapType::Concrete(5)),
        Instruction::StructGet {
            struct_type_index: 5,
            field_index: 1,
        },
    ]
}
fn check_cell(body: &mut Vec<Instruction<'static>>, check: u32) {
    body.push(Instruction::Call(check));
}
pub(super) fn lookup(b: &mut Builder) -> u32 {
    use Instruction::*;
    redefs::checks(b);
    let mut body = vec![
        LocalGet(1),
        LocalSet(2),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(2),
        sentinel_value(NIL),
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
    check_cell(&mut body, b.names["dynamic-cell-check"]);
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
        sentinel_value(NIL),
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
fn selected_binding(lookup: u32, root: Vec<Instruction<'static>>) -> Vec<Instruction<'static>> {
    use Instruction::*;
    // Only a cell a frame has held can have a frame entry.
    let mut body = vec![LocalGet(0)];
    body.extend(flags());
    body.extend([I32Const(CELL_DYNAMIC), I32And, I32Eqz, If(BlockType::Empty)]);
    body.extend(root.iter().cloned());
    body.extend([Return, End]);
    body.extend([
        LocalGet(0),
        GlobalGet(CURRENT),
        Call(lookup),
        LocalSet(2),
        LocalSet(1),
        LocalGet(2),
        I32Const(0),
        I32GeS,
        If(BlockType::Result(VALUE)),
    ]);
    array(&mut body, 1);
    body.extend([LocalGet(2), ArrayGet(ARGS), Else]);
    body.extend(root);
    body.push(End);
    body
}

pub(super) fn binding_get(b: &mut Builder, lookup: u32, root: Vec<Instruction<'static>>) {
    use Instruction::*;
    b.function_with_locals(
        "binding-get-unforced",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE), (1, ValType::I32)],
        &selected_binding(lookup, root),
    );
    // A frame saves state without dereferencing an unbound Var or forcing a
    // deferred context error. An unbound root's physical nil slot is not nil
    // as a value: preserve the compiler's undefined representation instead.
    let mut snapshot = vec![LocalGet(0)];
    snapshot.extend(flags());
    snapshot.extend([
        I32Const(CELL_BOUND), I32And, If(BlockType::Result(VALUE)),
        LocalGet(0), RefCastNonNull(HeapType::Concrete(BINDING)),
        StructGet { struct_type_index: BINDING, field_index: 0 },
        Else, sentinel_value(UNDEFINED), End,
    ]);
    b.function_with_locals(
        "binding-snapshot", &[VALUE], &[VALUE],
        &[(1, VALUE), (1, ValType::I32)], &selected_binding(lookup, snapshot),
    );
    // Unsupported native context transport is reported on the binding read,
    // never while constructing an otherwise unused dynamic frame. A private
    // cell holds the GC-owned message; ordinary Vars are nominal objects, not
    // raw binding cells. No host callback or foreign root survives the request.
    const DEFERRED_ERROR: i32 = 4;
    b.function("binding-delay-error", &[VALUE], &[VALUE], &[
        LocalGet(0), RefCastNonNull(HeapType::Concrete(STRING)),
        I32Const(DEFERRED_ERROR), StructNew(BINDING),
    ]);
    b.function("binding-force", &[VALUE], &[VALUE], &[
        LocalGet(0), RefTestNonNull(HeapType::Concrete(BINDING)), If(BlockType::Empty),
        LocalGet(0), RefCastNonNull(HeapType::Concrete(BINDING)),
        StructGet { struct_type_index: BINDING, field_index: 1 },
        I32Const(DEFERRED_ERROR), I32And, If(BlockType::Empty),
        GlobalGet(nominal::ERROR_GLOBAL),
        LocalGet(0), RefCastNonNull(HeapType::Concrete(BINDING)),
        StructGet { struct_type_index: BINDING, field_index: 0 },
        RefCastNonNull(HeapType::Concrete(STRING)),
        sentinel_value(NIL), sentinel_value(NIL), I32Const(0), RefI31,
        StructNew(LANGUAGE_ERROR), Throw(0), End, End,
        LocalGet(0),
    ]);
    for (name, select) in [("binding-get", "binding-get-unforced"), ("binding-read", "binding-snapshot")] {
        b.function(name, &[VALUE], &[VALUE], &[
            LocalGet(0), Call(b.names[select]), Call(b.names["binding-force"]),
        ]);
    }
}

fn set_binding(lookup: u32, restore: bool, check: u32) -> Vec<Instruction<'static>> {
    use Instruction::*;
    let mut body = if restore { redefs::restore_prefix(check) } else { vec![] };
    body.extend([
        LocalGet(0),
        LocalGet(2),
        Call(lookup),
        LocalSet(4),
        LocalSet(3),
        LocalGet(4),
        I32Const(0),
        I32GeS,
        If(BlockType::Empty),
    ]);
    array(&mut body, 3);
    body.extend([
        LocalGet(4),
        LocalGet(1),
        ArraySet(ARGS),
        Else,
    ]);
    if !restore {
        body.extend(root_set());
    }
    // Ordinary binding frames never mutate/restore roots. Explicit root leases
    // were handled above; their selected roots are restored on every exit.
    body.push(End);
    body
}
fn root_set() -> Vec<Instruction<'static>> {
    use Instruction::*;
    let mut body = vec![
        LocalGet(0), RefCastNonNull(HeapType::Concrete(BINDING)), LocalGet(1),
        StructSet { struct_type_index: BINDING, field_index: 0 },
        LocalGet(0), RefCastNonNull(HeapType::Concrete(BINDING)), LocalGet(0),
    ];
    body.extend(flags());
    body.extend([
        I32Const(CELL_BOUND), I32Or,
        StructSet { struct_type_index: BINDING, field_index: 1 },
    ]);
    body
}
pub(super) fn binding_set(b: &mut Builder, lookup: u32) -> u32 {
    use Instruction::*;
    let mut root = root_set();
    root.push(LocalGet(1));
    b.function("binding-root-set", &[VALUE, VALUE], &[VALUE], &root);
    let set_in = b.function_with_locals(
        "binding-set-in",
        &[VALUE, VALUE, VALUE],
        &[],
        &[(1, VALUE), (1, ValType::I32)],
        &set_binding(lookup, false, b.names["dynamic-cell-check"]),
    );
    b.function(
        "binding-set",
        &[VALUE, VALUE],
        &[],
        &[LocalGet(0), LocalGet(1), GlobalGet(CURRENT), Call(set_in)],
    );
    b.function_with_locals(
        "binding-restore-in",
        &[VALUE, VALUE, VALUE],
        &[],
        &[(1, VALUE), (2, ValType::I32)],
        &set_binding(lookup, true, b.names["dynamic-cell-check"]),
    )
}
/// Validate a whole chain, including entries hidden by a nearer binding. No
/// frame, entry, cell or root is mutated. Only the language nil sentinel ends a
/// chain; null, false and arbitrary objects are not empty contexts.
pub(super) fn switching(b: &mut Builder) {
    use Instruction::*;
    // Parameter/current cursor 0; slow cursor 1; entries 2; index 3; parity 4.
    // Floyd's check bounds malformed cyclic chains without a host registry.
    let mut body = vec![
        LocalGet(0),
        LocalSet(1),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(0),
        sentinel_value(NIL),
        RefEq,
        BrIf(1),
    ];
    guard(&mut body, 0, FRAME);
    frame(&mut body, 0, 1);
    body.push(LocalSet(2));
    shape(&mut body, 2);
    body.extend([LocalGet(2), Call(b.names["dynamic-entries-roots"]), Drop]);
    array(&mut body, 2);
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
    array(&mut body, 2);
    body.extend([LocalGet(3), ArrayGet(ARGS)]);
    check_cell(&mut body, b.names["dynamic-cell-check"]);
    body.extend([Br(0), End, End]);
    frame(&mut body, 0, 0);
    body.extend([
        LocalSet(0),
        LocalGet(4),
        I32Eqz,
        If(BlockType::Empty),
        I32Const(1),
        LocalSet(4),
        Else,
    ]);
    frame(&mut body, 1, 0);
    body.extend([
        LocalSet(1),
        I32Const(0),
        LocalSet(4),
        End,
        LocalGet(0),
        sentinel_value(NIL),
        RefEq,
        I32Eqz,
        LocalGet(0),
        LocalGet(1),
        RefEq,
        I32And,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.extend([End, Br(0), End, End]);
    let validate = b.function_with_locals(
        "dynamic-frame-check",
        &[VALUE],
        &[],
        &[(2, VALUE), (2, ValType::I32)],
        &body,
    );
    // Retain the same chain; this does not clone/isolate mutable binding entries.
    // Task creation uses dynamic-fork; a retained chain is for saving/resuming
    // one context and must remain GC-rooted by its caller while inactive.
    b.function(
        "dynamic-save",
        &[],
        &[VALUE],
        &[GlobalGet(CURRENT), Call(validate), GlobalGet(CURRENT)],
    );
    // Effective values live in triplets, not in the cells' root slots. Installing
    // a chain makes binding-get/set search that entire chain (nearest/reverse
    // entry first), with root-cell fallback. Copying values into cells, or using
    // dynamic-pop here, would overwrite caller state and destroy saved contexts.
    // Validate both chains before the single publication; even a validation fuel
    // trap leaves the old root and all effective values untouched.
    b.function_with_locals(
        "dynamic-switch",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &[
            GlobalGet(CURRENT),
            LocalSet(1),
            LocalGet(0),
            Call(validate),
            LocalGet(1),
            Call(validate),
            LocalGet(0),
            GlobalSet(CURRENT),
            LocalGet(1),
        ],
    );
    fork(b, validate);
}

/// Snapshot binding storage, not reachable language objects. Every parent frame
/// and triplet array is fresh; cells, snapshot values and current values retain
/// identity. No root/cell/source-array writes or unwind happen, even on failure.
fn fork(b: &mut Builder, validate: u32) {
    use Instruction::*;
    // GC locals: source 0, cursor 1, source-frame array 2, cloned parent 3,
    // source entries 4, copied entries 5. Integers: depth 6, index 7, length 8.
    let mut body = vec![
        GlobalGet(CURRENT),
        LocalTee(0),
        Call(validate),
        LocalGet(0),
        LocalSet(1),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(1),
        sentinel_value(NIL),
        RefEq,
        BrIf(1),
        LocalGet(6),
        I32Const(i32::MAX),
        I32Eq,
        If(BlockType::Empty),
    ];
    error(&mut body);
    body.extend([End, LocalGet(6), I32Const(1), I32Add, LocalSet(6)]);
    frame(&mut body, 1, 0);
    body.extend([
        LocalSet(1),
        Br(0),
        End,
        End,
        LocalGet(6),
        ArrayNewDefault(ARGS),
        LocalSet(2),
        LocalGet(0),
        LocalSet(1),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(7),
        LocalGet(6),
        I32Eq,
        BrIf(1),
    ]);
    array(&mut body, 2);
    body.extend([LocalGet(7), LocalGet(1), ArraySet(ARGS)]);
    frame(&mut body, 1, 0);
    body.extend([
        LocalSet(1),
        LocalGet(7),
        I32Const(1),
        I32Add,
        LocalSet(7),
        Br(0),
        End,
        End,
        sentinel_value(NIL),
        LocalSet(3),
        // Reverse the source-frame array: immutable parents must be constructed
        // outermost first. Every in-progress clone remains in a GC Wasm local.
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(7),
        I32Eqz,
        BrIf(1),
        LocalGet(7),
        I32Const(1),
        I32Sub,
        LocalSet(7),
    ]);
    array(&mut body, 2);
    body.extend([LocalGet(7), ArrayGet(ARGS), LocalSet(1)]);
    body.extend([LocalGet(1), Call(b.names["root-redefs-frame?"]), If(BlockType::Empty), Br(1), End]);
    frame(&mut body, 1, 1);
    body.push(LocalSet(4));
    array(&mut body, 4);
    body.extend([ArrayLen, LocalTee(8), ArrayNewDefault(ARGS), LocalSet(5)]);
    array(&mut body, 5);
    body.push(I32Const(0));
    array(&mut body, 4);
    body.extend([
        I32Const(0),
        LocalGet(8),
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
        LocalGet(3),
    ]);
    array(&mut body, 5);
    body.extend([StructNew(FRAME), LocalSet(3), Br(0), End, End, LocalGet(3)]);
    b.function_with_locals(
        "dynamic-fork",
        &[],
        &[VALUE],
        &[(6, VALUE), (3, ValType::I32)],
        &body,
    );
}
pub(super) fn functions(b: &mut Builder, binding_restore: u32, try_invoke: u32) -> Vec<u32> {
    use Instruction::*;
    // Validate every cell and complete storage before publishing a new frame.
    let mut body = vec![];
    shape(&mut body, 0);
    body.extend([LocalGet(0), Call(b.names["dynamic-entries-roots"]), Drop]);
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
    check_cell(&mut body, b.names["dynamic-cell-check"]);
    body.extend([Br(0), End, End]);
    // Mark every cell dynamic, so its reads search the frames from now on.
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
    body.extend([LocalGet(2), ArrayGet(ARGS), RefCastNonNull(HeapType::Concrete(5))]);
    array(&mut body, 0);
    body.extend([LocalGet(2), ArrayGet(ARGS)]);
    body.extend(flags());
    body.extend([
        I32Const(CELL_DYNAMIC),
        I32Or,
        StructSet {
            struct_type_index: 5,
            field_index: 1,
        },
        Br(0),
        End,
        End,
    ]);
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
    body.extend([LocalGet(1), Call(b.names["dynamic-entries-roots"]), Drop]);
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
    check_cell(&mut body, b.names["dynamic-cell-check"]);
    body.extend([Br(0), End, End]);
    frame(&mut body, 0, 0);
    body.push(LocalSet(2));
    body.extend([
        LocalGet(2),
        sentinel_value(NIL),
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
        Call(binding_restore),
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
    for inst in [LocalGet(0), Call(pop), sentinel_value(NIL), End] {
        body.instruction(&inst);
    }
    b.code.function(&body);
    b.count += 1;
    let root_push = redefs::push(b, push);
    for (name, enter) in [("dynamic-invoke", push), ("root-redefs-invoke", root_push)] {
        b.function_with_locals(
        name,
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &[
            LocalGet(0),
            Call(enter),
            LocalSet(2),
            LocalGet(1),
            sentinel_value(NIL),
            LocalGet(2),
            RefFunc(callback),
            I32Const(0),
            I32Const(0),
            Call(b.names["closure-new"]),
            Call(try_invoke),
        ],
    );
    }
    vec![callback]
}

/// Whether the selected cell owner is the outermost owner above a native
/// input's checkpoint. Inner user `binding`s must not change the next input's
/// analysis namespace. Frames are the runtime's private, validated chain.
pub(super) fn namespace_input_owner(b: &mut Builder) {
    use Instruction::*;
    // Parameters cell0/base1; cursor2, selected3, first4, entries5, index6.
    let mut body = vec![GlobalGet(CURRENT), LocalSet(2), sentinel_value(NIL), LocalSet(3),
        sentinel_value(NIL), LocalSet(4), Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(2), LocalGet(1), RefEq, BrIf(1), LocalGet(2), RefIsNull, BrIf(1)];
    guard(&mut body, 2, FRAME);
    frame(&mut body, 2, 1);
    body.push(LocalSet(5));
    shape(&mut body, 5);
    array(&mut body, 5);
    body.extend([ArrayLen, LocalSet(6), Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(6), I32Eqz, BrIf(1), LocalGet(6), I32Const(3), I32Sub, LocalSet(6)]);
    array(&mut body, 5);
    body.extend([LocalGet(6), ArrayGet(ARGS), LocalGet(0), RefEq, If(BlockType::Empty),
        LocalGet(3), RefIsNull, If(BlockType::Empty), LocalGet(2), LocalSet(3), End,
        LocalGet(2), LocalSet(4), End, Br(0), End, End]);
    frame(&mut body, 2, 0);
    body.extend([LocalSet(2), Br(0), End, End, LocalGet(3), RefIsNull, I32Eqz,
        LocalGet(3), LocalGet(4), RefEq, I32And]);
    b.function_with_locals("dynamic-namespace-input-owner", &[VALUE, VALUE], &[ValType::I32],
        &[(4, VALUE), (1, ValType::I32)], &body);
}
