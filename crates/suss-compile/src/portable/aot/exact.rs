//! Original canonical full-width integer transfer through exact u32 word pairs.
use crate::runtime_abi;
use wasm_encoder::{Function, Instruction};
fn emit(body: &mut Function, items: &[Instruction<'_>]) {
    for item in items {
        body.instruction(item);
    }
}
fn box_number(body: &mut Function) {
    body.instruction(&Instruction::Call(1));
}
fn schema_head(body: &mut Function, root: u32, operation: f64) {
    emit(
        body,
        &[
            Instruction::GlobalGet(root),
            Instruction::F64Const(operation.into()),
        ],
    );
    box_number(body);
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
pub(super) fn lift(body: &mut Function, signed: bool, parameter: u32, root: u32) {
    use Instruction::*;
    schema_head(body, root, if signed { 0.0 } else { 1.0 });
    emit(
        body,
        &[
            LocalGet(parameter),
            I64Const(32),
            I64ShrU,
            I32WrapI64,
            F64ConvertI32U,
        ],
    );
    box_number(body);
    emit(body, &[LocalGet(parameter), I32WrapI64, F64ConvertI32U]);
    box_number(body);
    schema_tail(body);
}
/// `scratch` is a value local for reading the word, a long.
fn word(body: &mut Function, root: u32, pair: u32, operation: f64, scratch: u32) {
    use Instruction::*;
    schema_head(body, root, operation);
    emit(body, &[LocalGet(pair), F64Const(0.0.into())]);
    box_number(body);
    schema_tail(body);
    emit(body, &[LocalSet(scratch)]);
    emit(body, &runtime_abi::number_to_f64(scratch));
    emit(body, &[I32TruncF64U, I64ExtendI32U]);
}
pub(super) fn lower(body: &mut Function, signed: bool, value: u32, pair: u32, root: u32, scratch: u32) {
    use Instruction::*;
    schema_head(body, root, if signed { 2.0 } else { 3.0 });
    emit(body, &[LocalGet(value), F64Const(0.0.into())]);
    box_number(body);
    schema_tail(body);
    body.instruction(&LocalSet(pair));
    word(body, root, pair, 4.0, scratch);
    emit(body, &[I64Const(32), I64Shl]);
    word(body, root, pair, 5.0, scratch);
    body.instruction(&I64Or);
}
