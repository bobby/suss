//! Compiled continuation plumbing. No source forms or interpreted instructions
//! cross this ABI: the resume closure dispatches native Wasm states.
use super::{Function, FutureBody, Instruction::*, runtime_abi};
use crate::portable::ir::{
    ValueId,
    r#async::{Continuation, Exit},
};
use wasm_encoder::{BlockType, HeapType};

pub const IMPORTS: &[&str] = &[
    "future-pending-new",
    "dynamic-fork",
    "dynamic-save",
    "dynamic-push",
    "dynamic-pop",
    "closure-new",
    "async-continuation-new",
    "async-continuation-snapshot",
    "async-continuation-commit",
    "async-registration-new",
    "async-task-start",
    "async-task-complete",
];
pub struct Locals {
    pub scratch: u32,
    pub snapshot: u32,
    pub cont: u32,
    pub live: u32,
    pub resume: u32,
    pub pc: u32,
    pub offset: u32,
    pub stack: u32,
    pub frame: u32,
    pub saved: u32,
    pub outcome: u32,
    pub auxiliary: u32,
}
fn nil(f: &mut Function) {
    f.instruction(&I32Const(0)).instruction(&RefI31);
}
fn int(f: &mut Function, value: usize) {
    f.instruction(&I32Const(value as i32)).instruction(&RefI31);
}
fn array(f: &mut Function, size: usize) {
    f.instruction(&ArrayNewFixed {
        array_type_index: runtime_abi::ARGS,
        array_size: size as u32,
    });
}
pub fn item(f: &mut Function, local: u32, index: usize) {
    f.instruction(&LocalGet(local))
        .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)))
        .instruction(&I32Const(index as i32))
        .instruction(&ArrayGet(runtime_abi::ARGS));
}
fn empty_slots(f: &mut Function, size: usize, local: u32) {
    nil(f);
    f.instruction(&I32Const(size as i32))
        .instruction(&ArrayNew(runtime_abi::ARGS))
        .instruction(&LocalSet(local));
}
fn put_local(f: &mut Function, array: u32, index: usize, value: u32) {
    f.instruction(&LocalGet(array))
        .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)))
        .instruction(&I32Const(index as i32))
        .instruction(&LocalGet(value))
        .instruction(&ArraySet(runtime_abi::ARGS));
}

pub fn create(
    f: &mut Function,
    body: &FutureBody,
    captures: &[ValueId],
    result: ValueId,
    function_index: u32,
    l: &Locals,
    import: &impl Fn(&str) -> u32,
) {
    // Separate creation temporaries: a nested future must not overwrite the
    // currently executing task's continuation, snapshot or resume closure.
    let env = l.scratch + 5;
    let cont = l.scratch + 6;
    let live = l.scratch + 7;
    let resume = l.scratch + 8;
    nil(f);
    array(f, 1);
    f.instruction(&LocalSet(env));
    f.instruction(&LocalGet(env))
        .instruction(&RefFunc(function_index))
        .instruction(&I32Const(1))
        .instruction(&I32Const(1))
        .instruction(&Call(import("closure-new")))
        .instruction(&LocalSet(resume));
    put_local(f, env, 0, resume);
    f.instruction(&Call(import("future-pending-new")))
        .instruction(&LocalSet(result.0 as u32 + l.offset));
    empty_slots(f, body.continuation.function.values.len(), live);
    for (capture, parameter) in captures
        .iter()
        .zip(&body.continuation.function.blocks[0].parameters)
    {
        put_local(f, live, parameter.0, capture.0 as u32 + l.offset);
    }
    int(f, 0);
    f.instruction(&LocalGet(live));
    array(f, 0); // initially empty unwind stack
    f.instruction(&Call(import("dynamic-fork")))
        .instruction(&LocalGet(result.0 as u32 + l.offset));
    int(f, 0);
    int(f, 0);
    nil(f);
    array(f, 8);
    f.instruction(&Call(import("async-continuation-new")))
        .instruction(&LocalSet(cont))
        .instruction(&LocalGet(cont))
        .instruction(&LocalGet(resume))
        .instruction(&Call(import("async-task-start")))
        .instruction(&Drop);
}

pub fn initialize(
    f: &mut Function,
    plan: &Continuation,
    l: &Locals,
    import: &impl Fn(&str) -> u32,
) {
    // Universal INVOKE receives an environment and one argument, the record.
    item(f, 0, 0);
    f.instruction(&LocalSet(l.resume));
    item(f, 1, 0);
    f.instruction(&LocalTee(l.cont))
        .instruction(&Call(import("async-continuation-snapshot")))
        .instruction(&LocalSet(l.snapshot));
    item(f, l.snapshot, 1);
    f.instruction(&LocalSet(l.live));
    for id in 0..plan.function.values.len() {
        item(f, l.live, id);
        f.instruction(&LocalSet(id as u32 + l.offset));
    }
    item(f, l.snapshot, 0);
    f.instruction(&RefCastNonNull(HeapType::I31))
        .instruction(&I31GetU)
        .instruction(&LocalSet(l.pc));
    item(f, l.snapshot, 6);
    f.instruction(&RefCastNonNull(HeapType::I31))
        .instruction(&I31GetU)
        .instruction(&I32Const(2))
        .instruction(&I32GeU)
        .instruction(&If(BlockType::Empty))
        .instruction(&I32Const(plan.states.len() as i32))
        .instruction(&LocalSet(l.pc))
        .instruction(&End);
}

pub fn resume_result(f: &mut Function, plan: &Continuation, state: usize, l: &Locals) {
    if let Some(result) = plan
        .states
        .iter()
        .find_map(|previous| match &previous.exit {
            Exit::Suspend { resume, result, .. } if *resume == state => Some(*result),
            _ => None,
        })
    {
        item(f, l.snapshot, 7);
        f.instruction(&LocalSet(result.0 as u32 + l.offset));
    }
}
fn prepare_checkpoint(
    f: &mut Function,
    resume: usize,
    spill: Option<&[ValueId]>,
    l: &Locals,
    value_count: usize,
    import: &impl Fn(&str) -> u32,
) {
    // Prepare a fresh live array and snapshot. No published entry array is
    // changed before the runtime's validated single-pointer commit.
    empty_slots(f, value_count, l.live);
    if let Some(spill) = spill {
        for value in spill {
            put_local(f, l.live, value.0, value.0 as u32 + l.offset);
        }
    } else {
        // A cooperative backedge retains the complete SSA local space, after
        // simultaneous edge parameter assignment, including region live roots.
        for id in 0..value_count {
            put_local(f, l.live, id, id as u32 + l.offset);
        }
    }
    item(f, l.snapshot, 5);
    f.instruction(&RefCastNonNull(HeapType::I31))
        .instruction(&I31GetU)
        .instruction(&I32Const(0x7fffffff))
        .instruction(&I32Eq)
        .instruction(&If(BlockType::Empty))
        .instruction(&Unreachable)
        .instruction(&End);
    f.instruction(&LocalGet(l.cont));
    int(f, resume);
    f.instruction(&LocalGet(l.live));
    item(f, l.snapshot, 2);
    f.instruction(&Call(import("dynamic-save")));
    item(f, l.snapshot, 4);
    item(f, l.snapshot, 5);
    f.instruction(&RefCastNonNull(HeapType::I31))
        .instruction(&I31GetU)
        .instruction(&I32Const(1))
        .instruction(&I32Add)
        .instruction(&RefI31);
    int(f, 0);
    nil(f);
    array(f, 8);
    // Keep the prepared generation locally before publication/registration.
    // A registration language error must unwind using N+1, not stale N.
    f.instruction(&LocalTee(l.snapshot));
    f.instruction(&Call(import("async-continuation-commit")))
        .instruction(&Drop);
}
pub fn suspend(
    f: &mut Function,
    future: ValueId,
    resume: usize,
    spill: &[ValueId],
    l: &Locals,
    value_count: usize,
    import: &impl Fn(&str) -> u32,
) {
    prepare_checkpoint(f, resume, Some(spill), l, value_count, import);
    f.instruction(&LocalGet(l.cont))
        .instruction(&LocalGet(future.0 as u32 + l.offset))
        .instruction(&LocalGet(l.resume))
        .instruction(&Call(import("async-registration-new")))
        .instruction(&Drop);
    nil(f);
    f.instruction(&Return);
}
/// A scheduling quantum ends at the first traversed backedge. This queues native
/// code for a later turn; fuel exhaustion is not the scheduling mechanism.
pub fn yield_turn(
    f: &mut Function,
    target: usize,
    l: &Locals,
    value_count: usize,
    import: &impl Fn(&str) -> u32,
) {
    prepare_checkpoint(f, target, None, l, value_count, import);
    f.instruction(&LocalGet(l.cont))
        .instruction(&LocalGet(l.resume))
        .instruction(&Call(import("async-task-yield")))
        .instruction(&Drop);
    nil(f);
    f.instruction(&Return);
}
fn publish_current(f: &mut Function, l: &Locals, import: &impl Fn(&str) -> u32) {
    f.instruction(&Call(import("dynamic-save")))
        .instruction(&LocalSet(l.auxiliary));
    put_local(f, l.snapshot, 3, l.auxiliary);
    f.instruction(&LocalGet(l.cont))
        .instruction(&LocalGet(l.snapshot))
        .instruction(&Call(import("async-continuation-commit")))
        .instruction(&Drop);
}
pub fn complete(
    f: &mut Function,
    kind: usize,
    value: ValueId,
    l: &Locals,
    import: &impl Fn(&str) -> u32,
) {
    publish_current(f, l, import);
    f.instruction(&LocalGet(l.cont));
    f.instruction(&I32Const(kind as i32));
    f.instruction(&LocalGet(value.0 as u32 + l.offset))
        .instruction(&Call(import("async-task-complete")))
        .instruction(&Drop);
    nil(f);
    f.instruction(&Return);
}

fn set_item_local(f: &mut Function, local: u32, index: usize, value: u32) {
    put_local(f, local, index, value);
}
fn set_item_int(f: &mut Function, local: u32, index: usize, value: usize) {
    f.instruction(&LocalGet(local))
        .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)))
        .instruction(&I32Const(index as i32));
    int(f, value);
    f.instruction(&ArraySet(runtime_abi::ARGS));
}
fn copy_array(f: &mut Function, source: u32, destination: u32, len: usize) {
    empty_slots(f, len, destination);
    f.instruction(&LocalGet(destination))
        .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)))
        .instruction(&I32Const(0))
        .instruction(&LocalGet(source))
        .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)))
        .instruction(&I32Const(0))
        .instruction(&I32Const(len as i32))
        .instruction(&ArrayCopy {
            array_type_index_dst: runtime_abi::ARGS,
            array_type_index_src: runtime_abi::ARGS,
        });
}
fn restore_saved(f: &mut Function, l: &Locals, count: usize) {
    item(f, l.frame, 5);
    f.instruction(&LocalSet(l.saved));
    for id in 0..count {
        item(f, l.saved, id);
        f.instruction(&LocalSet(id as u32 + l.offset));
    }
}
/// Unwind node: [parent,phase,pending-kind,pending-value,dynamic,saved-locals,
/// handler-pc,cleanup-pc,exit-pc]. Published nodes are immutable: phase changes
/// allocate a replacement. Phase 0 body, 1 handler, 2 cleanup.
pub fn region_push(
    f: &mut Function,
    handler: Option<usize>,
    cleanup: Option<usize>,
    end: usize,
    dynamic: &[ValueId],
    result: ValueId,
    plan: &Continuation,
    l: &Locals,
) {
    empty_slots(f, plan.function.values.len(), l.saved);
    // Conservatively retain all initialized GC locals, including a partially
    // evaluated surrounding expression. Unassigned Wasm GC locals are null;
    // these are storage roots, not reads of undefined SSA operands.
    for id in 0..plan.function.values.len() {
        put_local(f, l.saved, id, id as u32 + l.offset);
    }
    item(f, l.snapshot, 2);
    int(f, 0);
    int(f, 0);
    nil(f);
    if let Some(frame) = dynamic.first() {
        f.instruction(&LocalGet(frame.0 as u32 + l.offset));
    } else {
        nil(f);
    }
    f.instruction(&LocalGet(l.saved));
    int(
        f,
        handler.map_or(0x7fffffff, |block| plan.block_entries[block]),
    );
    int(
        f,
        cleanup.map_or(0x7fffffff, |block| plan.block_entries[block]),
    );
    int(f, plan.block_entries[end]);
    array(f, 9);
    f.instruction(&LocalSet(l.frame));
    set_item_local(f, l.snapshot, 2, l.frame);
    int(f, 2); // portable false: normal path never selects a latent exception edge
    f.instruction(&LocalSet(result.0 as u32 + l.offset));
}
pub fn event(f: &mut Function, kind: usize, value: u32, l: &Locals, unwind_pc: usize) {
    set_item_int(f, l.snapshot, 6, kind);
    set_item_local(f, l.snapshot, 7, value);
    f.instruction(&I32Const(unwind_pc as i32))
        .instruction(&LocalSet(l.pc));
}
pub fn region_exit(f: &mut Function, value: ValueId, cleanup: bool, l: &Locals, unwind_pc: usize) {
    if cleanup {
        item(f, l.snapshot, 2);
        f.instruction(&LocalSet(l.frame));
        item(f, l.frame, 2);
        f.instruction(&LocalSet(l.auxiliary));
        set_item_local(f, l.snapshot, 6, l.auxiliary);
        item(f, l.frame, 3);
        f.instruction(&LocalSet(l.outcome));
        set_item_local(f, l.snapshot, 7, l.outcome);
        f.instruction(&I32Const(unwind_pc as i32))
            .instruction(&LocalSet(l.pc));
    } else {
        event(f, 1, value.0 as u32 + l.offset, l, unwind_pc);
    }
}
fn phase_is(f: &mut Function, l: &Locals, value: usize) {
    item(f, l.frame, 1);
    int(f, value);
    f.instruction(&RefEq);
}
fn target_exists(f: &mut Function, l: &Locals, slot: usize) {
    item(f, l.frame, slot);
    int(f, 0x7fffffff);
    f.instruction(&RefEq).instruction(&I32Eqz);
}
fn select_region(f: &mut Function, l: &Locals, slot: usize, phase: usize, count: usize) {
    // Preserve the outcome before cleanup; a finally await must not overwrite it.
    copy_array(f, l.frame, l.stack, 9);
    set_item_int(f, l.stack, 1, phase);
    if phase == 2 {
        item(f, l.snapshot, 6);
        f.instruction(&LocalSet(l.auxiliary));
        set_item_local(f, l.stack, 2, l.auxiliary);
        item(f, l.snapshot, 7);
        f.instruction(&LocalSet(l.outcome));
        set_item_local(f, l.stack, 3, l.outcome);
    }
    set_item_local(f, l.snapshot, 2, l.stack);
    restore_saved(f, l, count);
    item(f, l.frame, slot);
    f.instruction(&RefCastNonNull(HeapType::I31))
        .instruction(&I31GetU)
        .instruction(&LocalSet(l.pc));
    // The caller's dispatcher resumes this native handler/cleanup state.
}
pub fn unwind(f: &mut Function, l: &Locals, count: usize, import: &impl Fn(&str) -> u32) {
    f.instruction(&wasm_encoder::Instruction::Block(BlockType::Empty))
        .instruction(&Loop(BlockType::Empty));
    item(f, l.snapshot, 2);
    f.instruction(&LocalSet(l.frame));
    f.instruction(&LocalGet(l.frame))
        .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)))
        .instruction(&ArrayLen)
        .instruction(&I32Eqz)
        .instruction(&If(BlockType::Empty));
    publish_current(f, l, import);
    f.instruction(&LocalGet(l.cont));
    item(f, l.snapshot, 6);
    f.instruction(&RefCastNonNull(HeapType::I31))
        .instruction(&I31GetU);
    item(f, l.snapshot, 7);
    f.instruction(&Call(import("async-task-complete")))
        .instruction(&Drop);
    nil(f);
    f.instruction(&Return).instruction(&End);
    phase_is(f, l, 0);
    item(f, l.snapshot, 6);
    int(f, 2);
    f.instruction(&RefEq).instruction(&I32And);
    target_exists(f, l, 6);
    f.instruction(&I32And).instruction(&If(BlockType::Empty));
    select_region(f, l, 6, 1, count);
    f.instruction(&Br(2)).instruction(&End);
    // A failure thrown by cleanup never re-enters that same cleanup.
    phase_is(f, l, 2);
    f.instruction(&I32Eqz);
    target_exists(f, l, 7);
    f.instruction(&I32And).instruction(&If(BlockType::Empty));
    select_region(f, l, 7, 2, count);
    f.instruction(&Br(2)).instruction(&End);
    // Pop dynamic storage only after all inner finally states have completed.
    item(f, l.frame, 4);
    nil(f);
    f.instruction(&RefEq)
        .instruction(&I32Eqz)
        .instruction(&If(BlockType::Empty));
    item(f, l.frame, 4);
    f.instruction(&Call(import("dynamic-pop")))
        .instruction(&End);
    item(f, l.frame, 0);
    f.instruction(&LocalSet(l.stack));
    set_item_local(f, l.snapshot, 2, l.stack);
    item(f, l.snapshot, 6);
    int(f, 1);
    f.instruction(&RefEq).instruction(&If(BlockType::Empty));
    restore_saved(f, l, count);
    item(f, l.frame, 8);
    f.instruction(&RefCastNonNull(HeapType::I31))
        .instruction(&I31GetU)
        .instruction(&LocalSet(l.pc))
        .instruction(&Br(2))
        .instruction(&End)
        .instruction(&Br(0))
        .instruction(&End)
        .instruction(&End);
}
