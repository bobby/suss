//! Emit only verified IR. Operands are local value IDs, never source expressions.
use super::{
    Diagnostic,
    hir::{Arithmetic, Literal},
    ir::{self, Function as IrFunction, Operation, Terminator},
};
use crate::runtime_abi;
use std::collections::BTreeSet;
use wasm_encoder::*;
const VALUE: ValType = ValType::Ref(RefType::EQREF);
fn arithmetic_name(operator: Arithmetic) -> &'static str {
    match operator {
        Arithmetic::Add => "number-add",
        Arithmetic::Subtract => "number-subtract",
        Arithmetic::Multiply => "number-multiply",
        Arithmetic::Divide => "number-divide",
        Arithmetic::Negate => "number-negate",
    }
}
pub fn emit(ir: &IrFunction) -> Result<Vec<u8>, Diagnostic> {
    ir::verify(ir)?;
    let fail = |message: &str| Diagnostic {
        span: ir.span.clone(),
        message: message.into(),
    };
    let count = u32::try_from(ir.values.len()).map_err(|_| fail("Too many IR values"))?;
    let mut names = BTreeSet::new();
    let mut globals = BTreeSet::new();
    for block in &ir.blocks {
        for inst in &block.instructions {
            match &inst.operation {
                Operation::Literal(Literal::Number(_)) => {
                    names.insert("number-box");
                }
                Operation::Literal(Literal::String(units)) => {
                    names.insert("string-new");
                    if !units.is_empty() {
                        names.insert("string-set-unit");
                    }
                }
                Operation::GlobalRead(global) => {
                    globals.insert(global.clone());
                    names.insert("binding-get");
                }
                Operation::Arithmetic { operator, .. } => {
                    names.insert(arithmetic_name(*operator));
                }
                _ => {}
            }
        }
    }
    let names = names.into_iter().collect::<Vec<_>>();
    let globals = globals.into_iter().collect::<Vec<_>>();
    let index = |name: &str| names.iter().position(|item| *item == name).unwrap() as u32;
    let mut types = runtime_abi::prelude();
    let mut imports = ImportSection::new();
    let type_count = runtime_abi::TYPE_COUNT;
    for (i, name) in names.iter().enumerate() {
        let (params, results) = match *name {
            "number-box" => (vec![ValType::F64], vec![VALUE]),
            "binding-get" | "number-negate" => (vec![VALUE], vec![VALUE]),
            "string-new" => (vec![ValType::I32], vec![VALUE]),
            "string-set-unit" => (vec![VALUE, ValType::I32, ValType::I32], vec![ValType::I32]),
            _ => (vec![VALUE, VALUE], vec![VALUE]),
        };
        types.ty().function(params, results);
        imports.import(
            "suss.runtime",
            name,
            EntityType::Function(type_count + i as u32),
        );
    }
    for global in &globals {
        imports.import(
            global.import_module(),
            &global.import_name(),
            EntityType::Global(GlobalType {
                val_type: runtime_abi::binding_cell_type(),
                mutable: false,
                shared: false,
            }),
        );
    }
    let eval_type = type_count + names.len() as u32;
    types.ty().function([], [VALUE]);
    let mut functions = FunctionSection::new();
    functions.function(eval_type);
    let mut exports = ExportSection::new();
    exports.export("eval", ExportKind::Func, names.len() as u32);
    // Every SSA value has one GC local. The last local is the block dispatcher.
    let mut function = Function::new([(count, VALUE), (1, ValType::I32)]);
    use Instruction::*;
    function
        .instruction(&I32Const(0))
        .instruction(&LocalSet(count))
        .instruction(&Loop(BlockType::Empty));
    for (block_id, block) in ir.blocks.iter().enumerate() {
        let block_id = i32::try_from(block_id).map_err(|_| fail("Too many IR blocks"))?;
        function
            .instruction(&LocalGet(count))
            .instruction(&I32Const(block_id))
            .instruction(&I32Eq)
            .instruction(&If(BlockType::Empty));
        for inst in &block.instructions {
            match &inst.operation {
                Operation::Literal(Literal::Nil) => {
                    function
                        .instruction(&I32Const(0))
                        .instruction(&RefI31)
                        .instruction(&LocalSet(inst.result.0 as u32));
                }
                Operation::Literal(Literal::Bool(value)) => {
                    function
                        .instruction(&I32Const(if *value { 4 } else { 2 }))
                        .instruction(&RefI31)
                        .instruction(&LocalSet(inst.result.0 as u32));
                }
                Operation::Literal(Literal::Number(value)) => {
                    function
                        .instruction(&F64Const((*value).into()))
                        .instruction(&Call(index("number-box")))
                        .instruction(&LocalSet(inst.result.0 as u32));
                }
                Operation::Literal(Literal::String(units)) => {
                    let length = i32::try_from(units.len())
                        .map_err(|_| fail("String literal exceeds signed runtime length"))?;
                    function
                        .instruction(&I32Const(length))
                        .instruction(&Call(index("string-new")))
                        .instruction(&LocalSet(inst.result.0 as u32));
                    for (offset, unit) in units.iter().enumerate() {
                        function
                            .instruction(&LocalGet(inst.result.0 as u32))
                            .instruction(&I32Const(offset as i32))
                            .instruction(&I32Const(i32::from(*unit)))
                            .instruction(&Call(index("string-set-unit")))
                            .instruction(&Drop);
                    }
                }
                Operation::GlobalRead(global) => {
                    let index_global =
                        globals.binary_search(global).expect("collected global") as u32;
                    function
                        .instruction(&GlobalGet(index_global))
                        .instruction(&Call(index("binding-get")))
                        .instruction(&LocalSet(inst.result.0 as u32));
                }
                Operation::Arithmetic {
                    operator,
                    arguments,
                } => {
                    for argument in arguments {
                        function.instruction(&LocalGet(argument.0 as u32));
                    }
                    function
                        .instruction(&Call(index(arithmetic_name(*operator))))
                        .instruction(&LocalSet(inst.result.0 as u32));
                }
            }
        }
        match &block.terminator {
            Terminator::Return(value) => {
                function
                    .instruction(&LocalGet(value.0 as u32))
                    .instruction(&Return);
            }
            Terminator::Jump { target, arguments } => {
                // Push every replacement value before any parameter assignment.
                for argument in arguments {
                    function.instruction(&LocalGet(argument.0 as u32));
                }
                for parameter in ir.blocks[*target].parameters.iter().rev() {
                    function.instruction(&LocalSet(parameter.0 as u32));
                }
                function
                    .instruction(&I32Const(*target as i32))
                    .instruction(&LocalSet(count))
                    .instruction(&Br(1));
            }
            Terminator::Branch {
                condition,
                consequent,
                alternative,
            } => {
                // Only the nil and false ABI sentinels are falsey, including for
                // boxed numeric zero and empty UTF-16 strings.
                function
                    .instruction(&LocalGet(condition.0 as u32))
                    .instruction(&I32Const(0))
                    .instruction(&RefI31)
                    .instruction(&RefEq);
                function
                    .instruction(&LocalGet(condition.0 as u32))
                    .instruction(&I32Const(2))
                    .instruction(&RefI31)
                    .instruction(&RefEq)
                    .instruction(&I32Or);
                function
                    .instruction(&If(BlockType::Result(ValType::I32)))
                    .instruction(&I32Const(*alternative as i32))
                    .instruction(&Else)
                    .instruction(&I32Const(*consequent as i32))
                    .instruction(&End)
                    .instruction(&LocalSet(count))
                    .instruction(&Br(1));
            }
        }
        function.instruction(&End);
    }
    function
        .instruction(&Unreachable)
        .instruction(&End)
        .instruction(&Unreachable)
        .instruction(&End);
    let mut code = CodeSection::new();
    code.function(&function);
    let mut module = Module::new();
    module
        .section(&runtime_abi::Manifest::default().section())
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&exports)
        .section(&code);
    let bytes = module.finish();
    runtime_abi::verify_artifact(&bytes, &runtime_abi::Manifest::default()).map_err(|message| {
        Diagnostic {
            span: ir.span.clone(),
            message,
        }
    })?;
    wasmparser::Validator::new()
        .validate_all(&bytes)
        .map_err(|error| Diagnostic {
            span: ir.span.clone(),
            message: format!("Generated fragment failed validation: {error}"),
        })?;
    Ok(bytes)
}
