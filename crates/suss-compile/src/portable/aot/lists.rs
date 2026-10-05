//! Original canonical list copy emission. Source storage is owned GC data;
//! canonical element and nested-string buffers have explicit transfer ownership.
use super::{boundary_error_message, lift_scalar, lower_scalar, strings, ListElement, Scalar};
use crate::runtime_abi;
use wasm_encoder::{BlockType, Function, HeapType, Instruction, MemArg};

pub(super) struct Locals {
    pub value: u32,
    pub container: u32,
    pub array: u32,
    pub pointer: u32,
    pub length: u32,
    pub index: u32,
    pub output: u32,
    pub integer: u32,
    pub number: u32,
}
fn emit(body: &mut Function, instructions: &[Instruction<'_>]) {
    for instruction in instructions {
        body.instruction(instruction);
    }
}
fn size(element: ListElement) -> i32 {
    match element {
        ListElement::String | ListElement::Scalar(Scalar::F64) => 8,
        ListElement::Scalar(Scalar::Bool | Scalar::U8 | Scalar::S8) => 1,
        ListElement::Scalar(Scalar::U16 | Scalar::S16) => 2,
        ListElement::Scalar(_) => 4,
    }
}
fn alignment(element: ListElement) -> i32 {
    match element {
        ListElement::String => 4,
        _ => size(element),
    }
}
fn memory(offset: u64, align: u32) -> MemArg {
    MemArg {
        offset,
        align,
        memory_index: 0,
    }
}
fn load(ty: Scalar) -> Instruction<'static> {
    use Instruction::*;
    match ty {
        Scalar::Bool | Scalar::U8 => I32Load8U(memory(0, 0)),
        Scalar::S8 => I32Load8S(memory(0, 0)),
        Scalar::U16 => I32Load16U(memory(0, 1)),
        Scalar::S16 => I32Load16S(memory(0, 1)),
        Scalar::U32 | Scalar::S32 => I32Load(memory(0, 2)),
        Scalar::F32 => F32Load(memory(0, 2)),
        Scalar::F64 => F64Load(memory(0, 3)),
    }
}
fn store(ty: Scalar) -> Instruction<'static> {
    use Instruction::*;
    match ty {
        Scalar::Bool | Scalar::U8 | Scalar::S8 => I32Store8(memory(0, 0)),
        Scalar::U16 | Scalar::S16 => I32Store16(memory(0, 1)),
        Scalar::U32 | Scalar::S32 => I32Store(memory(0, 2)),
        Scalar::F32 => F32Store(memory(0, 2)),
        Scalar::F64 => F64Store(memory(0, 3)),
    }
}
fn address(body: &mut Function, pointer: u32, index: u32, element: ListElement) {
    use Instruction::*;
    emit(
        body,
        &[
            LocalGet(pointer),
            LocalGet(index),
            I32Const(size(element)),
            I32Mul,
            I32Add,
        ],
    );
}
fn begin(body: &mut Function, length: u32, index: u32) {
    use Instruction::*;
    emit(
        body,
        &[
            I32Const(0),
            LocalSet(index),
            Block(BlockType::Empty),
            Loop(BlockType::Empty),
            LocalGet(index),
            LocalGet(length),
            I32GeU,
            BrIf(1),
        ],
    );
}
fn end(body: &mut Function, index: u32) {
    use Instruction::*;
    emit(
        body,
        &[
            LocalGet(index),
            I32Const(1),
            I32Add,
            LocalSet(index),
            Br(0),
            End,
            End,
        ],
    );
}
fn schema(body: &mut Function, root: u32, operation: f64, value: u32, index: Option<u32>) {
    use Instruction::*;
    emit(
        body,
        &[
            GlobalGet(root),
            F64Const(operation.into()),
            Call(1),
            LocalGet(value),
        ],
    );
    if let Some(index) = index {
        emit(body, &[LocalGet(index), F64ConvertI32U]);
    } else {
        body.instruction(&F64Const(0.0.into()));
    }
    emit(
        body,
        &[
            Call(1),
            ArrayNewFixed {
                array_type_index: runtime_abi::ARGS,
                array_size: 3,
            },
            Call(2),
        ],
    );
}
fn numeric_length(body: &mut Function, root: u32, locals: &Locals) {
    use Instruction::*;
    schema(body, root, 3.0, locals.container, None);
    emit(
        body,
        &[
            RefCastNonNull(HeapType::Concrete(runtime_abi::NUMBER)),
            StructGet {
                struct_type_index: runtime_abi::NUMBER,
                field_index: 0,
            },
            I32TruncF64U,
            LocalSet(locals.length),
        ],
    );
}

pub(super) fn copy_in(
    body: &mut Function,
    element: ListElement,
    parameter: u32,
    locals: &Locals,
    text: &strings::Locals,
    helpers: &strings::Helpers,
    root: u32,
    export: &str,
) {
    use Instruction::*;
    emit(
        body,
        &[
            LocalGet(parameter),
            LocalSet(locals.pointer),
            LocalGet(parameter + 1),
            LocalSet(locals.length),
            LocalGet(locals.length),
            I32Const(1_000_000),
            I32GtU,
            If(BlockType::Empty),
        ],
    );
    boundary_error_message(
        body,
        locals.value,
        &format!("WIT export {export} list input exceeds bounded source storage"),
    );
    emit(
        body,
        &[
            End,
            LocalGet(locals.length),
            ArrayNewDefault(runtime_abi::ARGS),
            Call(10),
            LocalSet(locals.array),
        ],
    );
    begin(body, locals.length, locals.index);
    emit(
        body,
        &[
            LocalGet(locals.array),
            LocalGet(locals.index),
            F64ConvertI32U,
            Call(1),
        ],
    );
    match element {
        ListElement::Scalar(ty) => {
            address(body, locals.pointer, locals.index, element);
            body.instruction(&load(ty));
            lift_scalar(body, &ty);
        }
        ListElement::String => {
            address(body, locals.pointer, locals.index, element);
            emit(body, &[I32Load(memory(0, 2)), LocalSet(text.pointer)]);
            address(body, locals.pointer, locals.index, element);
            emit(body, &[I32Load(memory(4, 2)), LocalSet(text.length)]);
            strings::copy_in(body, text, helpers);
            body.instruction(&LocalGet(locals.value));
        }
    }
    emit(body, &[Call(11), Drop]);
    end(body, locals.index);
    schema(body, root, 0.0, locals.array, None);
}

fn release(body: &mut Function, element: ListElement, locals: &Locals) {
    use Instruction::*;
    if matches!(element, ListElement::String) {
        begin(body, locals.length, locals.index);
        address(body, locals.pointer, locals.index, element);
        body.instruction(&I32Load(memory(0, 2)));
        address(body, locals.pointer, locals.index, element);
        emit(body, &[I32Load(memory(4, 2)), I32Const(2), I32Mul, Call(9)]);
        end(body, locals.index);
    }
    emit(
        body,
        &[
            LocalGet(locals.pointer),
            LocalGet(locals.length),
            I32Const(size(element)),
            I32Mul,
            Call(9),
        ],
    );
}
pub(super) fn release_input(
    body: &mut Function,
    element: ListElement,
    parameter: u32,
    locals: &Locals,
) {
    use Instruction::*;
    emit(
        body,
        &[
            LocalGet(parameter),
            LocalSet(locals.pointer),
            LocalGet(parameter + 1),
            LocalSet(locals.length),
        ],
    );
    release(body, element, locals);
}

pub(super) fn copy_out(
    body: &mut Function,
    element: ListElement,
    locals: &Locals,
    text: &strings::Locals,
    helpers: &strings::Helpers,
    root: u32,
    export: &str,
) {
    use Instruction::*;
    emit(body, &[LocalGet(locals.value), LocalSet(locals.container)]);
    schema(body, root, 1.0, locals.container, None);
    body.instruction(&LocalSet(locals.container));
    numeric_length(body, root, locals);
    // Check every value before allocating any output buffer. Private indexing
    // has no user callbacks or mutation between validation and copying.
    begin(body, locals.length, locals.index);
    schema(body, root, 2.0, locals.container, Some(locals.index));
    body.instruction(&LocalSet(locals.value));
    match element {
        ListElement::Scalar(ty) => {
            lower_scalar(
                body,
                ty,
                locals.value,
                locals.integer,
                locals.number,
                export,
            );
            body.instruction(&Drop);
        }
        ListElement::String => strings::validate(body, text, helpers, |body| {
            boundary_error_message(
                body,
                locals.value,
                &format!(
                    "WIT export {export} returned an incompatible Unicode string list element"
                ),
            )
        }),
    }
    end(body, locals.index);
    emit(
        body,
        &[
            I32Const(0),
            I32Const(0),
            I32Const(alignment(element)),
            LocalGet(locals.length),
            I32Const(size(element)),
            I32Mul,
            Call(8),
            LocalSet(locals.output),
        ],
    );
    begin(body, locals.length, locals.index);
    schema(body, root, 2.0, locals.container, Some(locals.index));
    body.instruction(&LocalSet(locals.value));
    match element {
        ListElement::Scalar(ty) => {
            address(body, locals.output, locals.index, element);
            lower_scalar(
                body,
                ty,
                locals.value,
                locals.integer,
                locals.number,
                export,
            );
            body.instruction(&store(ty));
        }
        ListElement::String => {
            emit(
                body,
                &[
                    LocalGet(locals.value),
                    Call(helpers.length),
                    LocalSet(text.length),
                ],
            );
            strings::copy_out(body, text, helpers);
            address(body, locals.output, locals.index, element);
            emit(body, &[LocalGet(text.pointer), I32Store(memory(0, 2))]);
            address(body, locals.output, locals.index, element);
            emit(body, &[LocalGet(text.length), I32Store(memory(4, 2))]);
        }
    }
    end(body, locals.index);
    emit(
        body,
        &[
            I32Const(0),
            LocalGet(locals.output),
            I32Store(memory(0, 2)),
            I32Const(0),
            LocalGet(locals.length),
            I32Store(memory(4, 2)),
            I32Const(0),
        ],
    );
}

pub(super) fn post_return(body: &mut Function, element: ListElement) {
    use Instruction::*;
    emit(
        body,
        &[
            LocalGet(0),
            I32Load(memory(0, 2)),
            LocalSet(1),
            LocalGet(0),
            I32Load(memory(4, 2)),
            LocalSet(2),
        ],
    );
    let locals = Locals {
        value: 0,
        container: 0,
        array: 0,
        pointer: 1,
        length: 2,
        index: 3,
        output: 0,
        integer: 0,
        number: 0,
    };
    release(body, element, &locals);
}
