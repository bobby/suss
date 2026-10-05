//! Original UTF-16 canonical string copy emission. Linear bytes are owned by
//! the canonical allocator; source strings own their units in GC storage.
use crate::runtime_abi;
use wasm_encoder::{BlockType, Function, HeapType, Instruction, MemArg};

pub(super) struct Helpers {
    pub new: u32,
    pub length: u32,
    pub unit: u32,
    pub set_unit: u32,
    pub realloc: u32,
}

pub(super) struct Locals {
    pub value: u32,
    pub pointer: u32,
    pub length: u32,
    pub index: u32,
    pub unit: u32,
}

fn memory() -> MemArg {
    MemArg {
        offset: 0,
        align: 1,
        memory_index: 0,
    }
}
fn emit(body: &mut Function, instructions: &[Instruction<'_>]) {
    for instruction in instructions {
        body.instruction(instruction);
    }
}

/// Copy a canonical incoming buffer into an independent rooted source array.
/// The caller releases incoming allocation ownership after all arguments have
/// been copied, before invoking the source closure.
pub(super) fn copy_in(body: &mut Function, locals: &Locals, helpers: &Helpers) {
    use Instruction::*;
    emit(
        body,
        &[
            LocalGet(locals.length),
            Call(helpers.new),
            LocalSet(locals.value),
            I32Const(0),
            LocalSet(locals.index),
            Block(BlockType::Empty),
            Loop(BlockType::Empty),
            LocalGet(locals.index),
            LocalGet(locals.length),
            I32GeU,
            BrIf(1),
            LocalGet(locals.value),
            LocalGet(locals.index),
            LocalGet(locals.pointer),
            LocalGet(locals.index),
            I32Const(2),
            I32Mul,
            I32Add,
            I32Load16U(memory()),
            Call(helpers.set_unit),
            Drop,
            LocalGet(locals.index),
            I32Const(1),
            I32Add,
            LocalSet(locals.index),
            Br(0),
            End,
            End,
        ],
    );
}

/// WIT strings contain Unicode text. Ordinary source strings can contain lone
/// UTF-16 surrogates, so diagnose them before allocation/canonical lifting.
/// `failure` emits an uncaught language exception and never returns normally.
pub(super) fn validate(
    body: &mut Function,
    locals: &Locals,
    helpers: &Helpers,
    mut failure: impl FnMut(&mut Function),
) {
    use Instruction::*;
    emit(
        body,
        &[
            LocalGet(locals.value),
            RefTestNonNull(HeapType::Concrete(runtime_abi::STRING)),
            I32Eqz,
            If(BlockType::Empty),
        ],
    );
    failure(body);
    emit(
        body,
        &[
            End,
            LocalGet(locals.value),
            Call(helpers.length),
            LocalSet(locals.length),
            // Match the canonical maximum byte length and prevent memory32 overflow.
            LocalGet(locals.length),
            I32Const(0x3fff_ffff),
            I32GtU,
            If(BlockType::Empty),
        ],
    );
    failure(body);
    emit(
        body,
        &[
            End,
            I32Const(0),
            LocalSet(locals.index),
            Block(BlockType::Empty),
            Loop(BlockType::Empty),
            LocalGet(locals.index),
            LocalGet(locals.length),
            I32GeU,
            BrIf(1),
            LocalGet(locals.value),
            LocalGet(locals.index),
            Call(helpers.unit),
            LocalSet(locals.unit),
            LocalGet(locals.unit),
            I32Const(0xd800),
            I32GeU,
            LocalGet(locals.unit),
            I32Const(0xdbff),
            I32LeU,
            I32And,
            If(BlockType::Empty),
            LocalGet(locals.index),
            I32Const(1),
            I32Add,
            LocalSet(locals.index),
            LocalGet(locals.index),
            LocalGet(locals.length),
            I32GeU,
            If(BlockType::Empty),
        ],
    );
    failure(body);
    emit(
        body,
        &[
            End,
            LocalGet(locals.value),
            LocalGet(locals.index),
            Call(helpers.unit),
            LocalSet(locals.unit),
            LocalGet(locals.unit),
            I32Const(0xdc00),
            I32LtU,
            LocalGet(locals.unit),
            I32Const(0xdfff),
            I32GtU,
            I32Or,
            If(BlockType::Empty),
        ],
    );
    failure(body);
    emit(
        body,
        &[
            End,
            Else,
            LocalGet(locals.unit),
            I32Const(0xdc00),
            I32GeU,
            LocalGet(locals.unit),
            I32Const(0xdfff),
            I32LeU,
            I32And,
            If(BlockType::Empty),
        ],
    );
    failure(body);
    emit(
        body,
        &[
            End,
            End,
            LocalGet(locals.index),
            I32Const(1),
            I32Add,
            LocalSet(locals.index),
            Br(0),
            End,
            End,
        ],
    );
}

/// Allocate and copy a validated source string. The caller retains the returned
/// buffer until canonical lifting completes and releases it during post-return
/// (or after asynchronous task-return has consumed the flattened result).
pub(super) fn copy_out(body: &mut Function, locals: &Locals, helpers: &Helpers) {
    use Instruction::*;
    emit(
        body,
        &[
            I32Const(0),
            I32Const(0),
            I32Const(2),
            LocalGet(locals.length),
            I32Const(2),
            I32Mul,
            Call(helpers.realloc),
            LocalSet(locals.pointer),
            I32Const(0),
            LocalSet(locals.index),
            Block(BlockType::Empty),
            Loop(BlockType::Empty),
            LocalGet(locals.index),
            LocalGet(locals.length),
            I32GeU,
            BrIf(1),
            LocalGet(locals.pointer),
            LocalGet(locals.index),
            I32Const(2),
            I32Mul,
            I32Add,
            LocalGet(locals.value),
            LocalGet(locals.index),
            Call(helpers.unit),
            I32Store16(memory()),
            LocalGet(locals.index),
            I32Const(1),
            I32Add,
            LocalSet(locals.index),
            Br(0),
            End,
            End,
        ],
    );
}
