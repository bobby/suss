//! Original canonical full-width integer transfer through exact u32 word pairs.
use crate::runtime_abi;
use wasm_encoder::{Function, Instruction};
fn emit(body: &mut Function, items: &[Instruction<'_>]) {
    for item in items {
        body.instruction(item);
    }
}
/// Operation codes and small schema operands are i31 longs.
fn small_long(body: &mut Function, value: i32) {
    emit(body, &[Instruction::I32Const(value), Instruction::RefI31]);
}
fn schema_head(body: &mut Function, root: u32, operation: i32) {
    body.instruction(&Instruction::GlobalGet(root));
    small_long(body, operation);
}
fn schema_tail(body: &mut Function) {
    emit(
        body,
        &[
            Instruction::ArrayNewFixed {
                array_type_index: runtime_abi::ARGS,
                array_size: 3,
            },
            Instruction::Call(2),
        ],
    );
}
/// `wide` is an i64 local for boxing each unsigned word as a long.
pub(super) fn lift(body: &mut Function, signed: bool, parameter: u32, root: u32, wide: u32) {
    use Instruction::*;
    schema_head(body, root, if signed { 0 } else { 1 });
    emit(body, &[LocalGet(parameter), I64Const(32), I64ShrU]);
    emit(body, &runtime_abi::long_box_in(wide));
    emit(body, &[LocalGet(parameter), I64Const(0xffff_ffff), I64And]);
    emit(body, &runtime_abi::long_box_in(wide));
    schema_tail(body);
}
/// `scratch` is a value local for reading the word, a long.
fn word(body: &mut Function, root: u32, pair: u32, operation: i32, scratch: u32) {
    use Instruction::*;
    schema_head(body, root, operation);
    body.instruction(&LocalGet(pair));
    small_long(body, 0);
    schema_tail(body);
    emit(body, &[LocalSet(scratch)]);
    emit(body, &runtime_abi::number_to_f64(scratch));
    emit(body, &[I32TruncF64U, I64ExtendI32U]);
}
pub(super) fn lower(body: &mut Function, signed: bool, value: u32, pair: u32, root: u32, scratch: u32) {
    use Instruction::*;
    schema_head(body, root, if signed { 2 } else { 3 });
    body.instruction(&LocalGet(value));
    small_long(body, 0);
    schema_tail(body);
    body.instruction(&LocalSet(pair));
    word(body, root, pair, 4, scratch);
    emit(body, &[I64Const(32), I64Shl]);
    word(body, root, pair, 5, scratch);
    body.instruction(&I64Or);
}
