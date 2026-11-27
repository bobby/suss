//! Root redefinition leases share rooted unwind frames, not thread bindings.
//! A private BINDING wrapper holds the real cell and its prior bound bit. It is
//! never exposed as a language Var/value. Forks omit these globally owned leases.
use super::*;
pub(super) const ROOT: i32 = 8;

pub(super) fn checks(b: &mut Builder) {
    use Instruction::*;
    let mut body = vec![];
    guard(&mut body, 0, BINDING);
    body.push(LocalGet(0)); body.extend(flags());
    body.extend([I32Const(ROOT), I32And, If(BlockType::Empty),
        LocalGet(0), RefCastNonNull(HeapType::Concrete(BINDING)),
        StructGet { struct_type_index: BINDING, field_index: 0 }, LocalSet(1)]);
    guard(&mut body, 1, BINDING);
    body.push(LocalGet(1)); body.extend(flags());
    body.extend([I32Const(ROOT), I32And, If(BlockType::Empty)]);
    error(&mut body); body.extend([End, End]);
    let check = b.function_with_locals("dynamic-cell-check", &[VALUE], &[], &[(1, VALUE)], &body);
    // Validate the entire storage and require one uniform scope kind before any
    // publication/root writes. Empty storage has no root lease to inherit.
    let mut body = vec![];
    shape(&mut body, 0);
    array(&mut body, 0); body.extend([ArrayLen, LocalSet(2), LocalGet(2), I32Eqz,
        If(BlockType::Empty), I32Const(0), Return, End]);
    array(&mut body, 0); body.extend([I32Const(0), ArrayGet(ARGS), LocalTee(1), Call(check), LocalGet(1)]);
    body.extend(flags()); body.extend([I32Const(ROOT), I32And, LocalSet(3), Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(2), I32Eqz, BrIf(1), LocalGet(2), I32Const(3), I32Sub, LocalSet(2)]);
    array(&mut body, 0); body.extend([LocalGet(2), ArrayGet(ARGS), LocalTee(1), Call(check), LocalGet(1)]);
    body.extend(flags()); body.extend([I32Const(ROOT), I32And, LocalGet(3), I32Ne, If(BlockType::Empty)]);
    error(&mut body); body.extend([End, Br(0), End, End, LocalGet(3)]);
    let entries = b.function_with_locals("dynamic-entries-roots", &[VALUE], &[ValType::I32], &[(1, VALUE), (2, ValType::I32)], &body);
    let mut body = vec![]; guard(&mut body, 0, FRAME); frame(&mut body, 0, 1); body.push(Call(entries));
    b.function("root-redefs-frame?", &[VALUE], &[ValType::I32], &body);
}

pub(super) fn restore_prefix(check: u32) -> Vec<Instruction<'static>> {
    use Instruction::*;
    let mut body = vec![LocalGet(0), Call(check), LocalGet(0)]; body.extend(flags());
    body.extend([I32Const(ROOT), I32And, If(BlockType::Empty), LocalGet(0)]);
    body.extend(flags()); body.extend([I32Const(CELL_BOUND), I32And, LocalSet(5),
        LocalGet(0), RefCastNonNull(HeapType::Concrete(BINDING)),
        StructGet { struct_type_index: BINDING, field_index: 0 }, LocalSet(0)]);
    body.extend(root_set());
    // Restore boundness independently of nil/false/undefined, preserving the
    // real cell's dynamic bit and all unrelated prior runtime effects.
    body.extend([LocalGet(5), I32Eqz, If(BlockType::Empty),
        LocalGet(0), RefCastNonNull(HeapType::Concrete(BINDING)), LocalGet(0)]);
    body.extend(flags()); body.extend([I32Const(!CELL_BOUND), I32And,
        StructSet { struct_type_index: BINDING, field_index: 1 }, End, Return, End]);
    body
}

pub(super) fn push(b: &mut Builder, dynamic_push: u32) -> u32 {
    use Instruction::*;
    // Parameter entries0; private entries1; actual cell2; index3; length4;
    // wrapper5; installed frame6. Snapshot every root before publishing/writing.
    let mut body = vec![LocalGet(0), Call(b.names["dynamic-entries-roots"]),
        If(BlockType::Empty)]; error(&mut body); body.push(End);
    array(&mut body, 0); body.extend([ArrayLen, LocalTee(4), ArrayNewDefault(ARGS), LocalSet(1),
        Block(BlockType::Empty), Loop(BlockType::Empty), LocalGet(3), LocalGet(4), I32GeU, BrIf(1)]);
    array(&mut body, 0); body.extend([LocalGet(3), ArrayGet(ARGS), LocalSet(2)]);
    body.push(LocalGet(2)); body.push(LocalGet(2)); body.extend(flags());
    body.extend([I32Const(CELL_BOUND), I32And, I32Const(ROOT), I32Or, sentinel_value(NIL), StructNew(BINDING), LocalSet(5)]);
    array(&mut body, 1); body.extend([LocalGet(3), LocalGet(5), ArraySet(ARGS)]);
    array(&mut body, 1); body.extend([LocalGet(3), I32Const(1), I32Add,
        LocalGet(2), RefCastNonNull(HeapType::Concrete(BINDING)),
        StructGet { struct_type_index: BINDING, field_index: 0 }, ArraySet(ARGS)]);
    array(&mut body, 1); body.extend([LocalGet(3), I32Const(2), I32Add]);
    array(&mut body, 0); body.extend([LocalGet(3), I32Const(2), I32Add, ArrayGet(ARGS), ArraySet(ARGS),
        LocalGet(3), I32Const(3), I32Add, LocalSet(3), Br(0), End, End,
        LocalGet(1), Call(dynamic_push), LocalSet(6),
        I32Const(0), LocalSet(3), Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(3), LocalGet(4), I32GeU, BrIf(1)]);
    // The lease is rooted before the first root write. A fuel trap can restore
    // every old root idempotently, including writes not yet reached.
    array(&mut body, 0); body.extend([LocalGet(3), ArrayGet(ARGS)]);
    array(&mut body, 0); body.extend([LocalGet(3), I32Const(2), I32Add, ArrayGet(ARGS),
        Call(b.names["binding-root-set"]), Drop,
        LocalGet(3), I32Const(3), I32Add, LocalSet(3), Br(0), End, End, LocalGet(6)]);
    b.function_with_locals("root-redefs-push", &[VALUE], &[VALUE],
        &[(2, VALUE), (2, ValType::I32), (2, VALUE)], &body)
}
