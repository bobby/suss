//! Original source closure for the resolved official exit-with-code import.
use super::diagnostic;
use crate::{
    portable::{Diagnostic, resolve::Global},
    runtime_abi,
};
use std::borrow::Cow;
use wasm_encoder::*;
use wit_parser::{
    Resolve,
    abi::{AbiVariant, WasmType},
};
const VALUE: ValType = ValType::Ref(RefType::EQREF);

pub(super) fn module(
    resolve: &Resolve,
    function: &wit_parser::Function,
    cell: &Global,
) -> Result<Vec<u8>, Diagnostic> {
    use Instruction::*;
    let abi = resolve.wasm_signature(AbiVariant::GuestImport, function);
    if abi.params != [WasmType::I32] || !abi.results.is_empty() {
        return Err(diagnostic(
            "Resolved exit-with-code ABI differs from the u8 source adapter",
        ));
    }
    let mut types = runtime_abi::prelude();
    let mut imports = ImportSection::new();
    for (index, (name, params, results)) in [
        ("binding-set", vec![VALUE, VALUE], vec![]),
        ("language-error-new", vec![VALUE], vec![VALUE]),
        ("string-new", vec![ValType::I32], vec![VALUE]),
        (
            "string-set-unit",
            vec![VALUE, ValType::I32, ValType::I32],
            vec![ValType::I32],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        types.ty().function(params, results);
        imports.import(
            "suss.runtime",
            name,
            EntityType::Function(runtime_abi::TYPE_COUNT + index as u32),
        );
    }
    types.ty().function([VALUE], []);
    imports.import(
        "suss.runtime",
        "language-exception",
        EntityType::Tag(TagType {
            kind: TagKind::Exception,
            func_type_idx: runtime_abi::TYPE_COUNT + 4,
        }),
    );
    types.ty().function([ValType::I32], []);
    imports.import(
        "suss.canonical",
        "exit",
        EntityType::Function(runtime_abi::TYPE_COUNT + 5),
    );
    imports.import(
        cell.import_module(),
        &cell.import_name(),
        EntityType::Global(GlobalType {
            val_type: runtime_abi::binding_cell_type(),
            mutable: false,
            shared: false,
        }),
    );
    types.ty().function([], [VALUE]);
    types.ty().function([], []);
    let mut functions = FunctionSection::new();
    functions.function(runtime_abi::INVOKE);
    functions.function(runtime_abi::TYPE_COUNT + 6);
    functions.function(runtime_abi::TYPE_COUNT + 7);
    let mut code = CodeSection::new();
    let mut invoke = Function::new([(1, VALUE), (1, ValType::F64)]);
    emit(
        &mut invoke,
        &[
            LocalGet(1),
            I32Const(0),
            ArrayGet(runtime_abi::ARGS),
            LocalSet(2),
            LocalGet(2),
            RefTestNonNull(HeapType::Concrete(runtime_abi::NUMBER)),
            I32Eqz,
            If(BlockType::Empty),
            Call(6),
            Throw(0),
            End,
            LocalGet(2),
            RefCastNonNull(HeapType::Concrete(runtime_abi::NUMBER)),
            StructGet {
                struct_type_index: runtime_abi::NUMBER,
                field_index: 0,
            },
            LocalSet(3),
            LocalGet(3),
            F64Const(0.0.into()),
            F64Lt,
            LocalGet(3),
            F64Const(255.0.into()),
            F64Gt,
            I32Or,
            LocalGet(3),
            LocalGet(3),
            F64Trunc,
            F64Ne,
            I32Or,
            If(BlockType::Empty),
            Call(6),
            Throw(0),
            End,
            LocalGet(3),
            I32TruncF64U,
            Call(4),
            Unreachable,
            End,
        ],
    );
    code.function(&invoke);
    let units = "exit-with-code requires an integer in [0,255]"
        .encode_utf16()
        .collect::<Vec<_>>();
    let mut error = Function::new([(1, VALUE)]);
    emit(
        &mut error,
        &[I32Const(units.len() as i32), Call(2), LocalSet(0)],
    );
    for (index, unit) in units.into_iter().enumerate() {
        emit(
            &mut error,
            &[
                LocalGet(0),
                I32Const(index as i32),
                I32Const(unit as i32),
                Call(3),
                Drop,
            ],
        );
    }
    emit(&mut error, &[LocalGet(0), Call(1), End]);
    code.function(&error);
    let mut start = Function::new([]);
    emit(
        &mut start,
        &[
            GlobalGet(0),
            I32Const(0),
            RefI31,
            RefFunc(5),
            I32Const(1),
            I32Const(1),
            I32Const(0),
            RefI31,
            StructNew(runtime_abi::CLOSURE),
            Call(0),
            End,
        ],
    );
    code.function(&start);
    let mut elements = ElementSection::new();
    elements.declared(Elements::Functions(Cow::Borrowed(&[5])));
    let mut module = Module::new();
    module
        .section(&runtime_abi::Manifest::default().section())
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&StartSection { function_index: 7 })
        .section(&elements)
        .section(&code);
    Ok(module.finish())
}
fn emit(function: &mut Function, instructions: &[Instruction<'_>]) {
    for instruction in instructions {
        function.instruction(instruction);
    }
}
