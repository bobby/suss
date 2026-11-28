//! Original Unicode-scalar adapters. WIT char uses a scalar i32; the language
//! owns one or two UTF-16 units. No linear-memory allocation is required.
use super::boundary_error_message;
use crate::runtime_abi;
use wasm_encoder::{BlockType, Function, HeapType, Instruction};

fn emit(body: &mut Function, instructions: &[Instruction<'_>]) {
    for instruction in instructions {
        body.instruction(instruction);
    }
}

/// Consume a canonical scalar and leave an independent UTF-16 string.
pub(super) fn lift(body: &mut Function, wide: u32) {
    use Instruction::*;
    emit(
        body,
        &[
            I64ExtendI32U,
            LocalSet(wide),
            LocalGet(wide),
            I64Const(0x10000),
            I64LtU,
            If(BlockType::Result(super::VALUE)),
            LocalGet(wide),
            I32WrapI64,
            ArrayNewFixed {
                array_type_index: runtime_abi::STRING,
                array_size: 1,
            },
            Else,
            LocalGet(wide),
            I32WrapI64,
            I32Const(0x10000),
            I32Sub,
            I32Const(10),
            I32ShrU,
            I32Const(0xd800),
            I32Add,
            LocalGet(wide),
            I32WrapI64,
            I32Const(0x3ff),
            I32And,
            I32Const(0xdc00),
            I32Add,
            ArrayNewFixed {
                array_type_index: runtime_abi::STRING,
                array_size: 2,
            },
            End,
        ],
    );
}

fn unit(body: &mut Function, value: u32, index: i32) {
    use Instruction::*;
    emit(
        body,
        &[
            LocalGet(value),
            RefCastNonNull(HeapType::Concrete(runtime_abi::STRING)),
            I32Const(index),
            ArrayGetU(runtime_abi::STRING),
        ],
    );
}

fn reject(body: &mut Function, value: u32, name: &str) {
    boundary_error_message(
        body,
        value,
        &format!("WIT export {name} returned an incompatible Unicode scalar"),
    );
}

/// Validate the source representation before any cast/index, then leave its
/// scalar bits. Reject lone surrogates, reversed pairs and multiple scalars.
pub(super) fn lower(body: &mut Function, value: u32, integer: u32, name: &str) {
    use Instruction::*;
    let string = HeapType::Concrete(runtime_abi::STRING);
    emit(
        body,
        &[
            LocalGet(value),
            RefTestNonNull(string),
            I32Eqz,
            If(BlockType::Empty),
        ],
    );
    reject(body, value, name);
    emit(
        body,
        &[
            End,
            LocalGet(value),
            RefCastNonNull(string),
            ArrayLen,
            LocalSet(integer),
            LocalGet(integer),
            I32Const(1),
            I32Eq,
            If(BlockType::Result(wasm_encoder::ValType::I32)),
        ],
    );
    unit(body, value, 0);
    emit(
        body,
        &[
            LocalTee(integer),
            I32Const(0xd800),
            I32Sub,
            I32Const(0x800),
            I32LtU,
            If(BlockType::Empty),
        ],
    );
    reject(body, value, name);
    emit(
        body,
        &[
            End,
            LocalGet(integer),
            Else,
            LocalGet(integer),
            I32Const(2),
            I32Ne,
            If(BlockType::Empty),
        ],
    );
    reject(body, value, name);
    body.instruction(&End);
    unit(body, value, 0);
    emit(body, &[I32Const(0xd800), I32Sub, I32Const(0x400), I32GeU]);
    unit(body, value, 1);
    emit(
        body,
        &[
            I32Const(0xdc00),
            I32Sub,
            I32Const(0x400),
            I32GeU,
            I32Or,
            If(BlockType::Empty),
        ],
    );
    reject(body, value, name);
    body.instruction(&End);
    unit(body, value, 0);
    emit(body, &[I32Const(0xd800), I32Sub, I32Const(10), I32Shl]);
    unit(body, value, 1);
    emit(
        body,
        &[
            I32Const(0xdc00),
            I32Sub,
            I32Add,
            I32Const(0x10000),
            I32Add,
            End,
        ],
    );
}
