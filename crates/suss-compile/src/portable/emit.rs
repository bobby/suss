//! Emit only verified IR. Operands are local value IDs, never source expressions.
use super::{
    Diagnostic,
    hir::{Arithmetic, Literal},
    ir::{self, ClosureBody, Function as IrFunction, Operation, Terminator},
};
use crate::runtime_abi;
use std::{borrow::Cow, collections::BTreeSet};
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
    let mut bodies = Vec::new();
    collect_bodies(ir, &mut bodies);
    let all_functions = std::iter::once(ir).chain(bodies.iter().map(|body| &body.function));
    let mut names = BTreeSet::new();
    let mut globals = BTreeSet::new();
    for function in all_functions {
        for block in &function.blocks {
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
                    Operation::MakeClosure { .. } => {
                        names.insert("closure-new");
                    }
                    Operation::Call { .. } => {
                        names.insert("invoke");
                    }
                    Operation::Arithmetic { operator, .. } => {
                        names.insert(arithmetic_name(*operator));
                    }
                    _ => {}
                }
            }
        }
    }
    let names = names.into_iter().collect::<Vec<_>>();
    let globals = globals.into_iter().collect::<Vec<_>>();
    let mut types = runtime_abi::prelude();
    let mut imports = ImportSection::new();
    let type_count = runtime_abi::TYPE_COUNT;
    for (i, name) in names.iter().enumerate() {
        let (params, results) = match *name {
            "number-box" => (vec![ValType::F64], vec![VALUE]),
            "closure-new" => (
                vec![
                    VALUE,
                    ValType::Ref(RefType {
                        nullable: false,
                        heap_type: HeapType::Concrete(runtime_abi::INVOKE),
                    }),
                    ValType::I32,
                    ValType::I32,
                ],
                vec![VALUE],
            ),
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
    let mut code = CodeSection::new();
    code.function(&emit_function(ir, None, &names, &globals, &bodies)?);
    for body in &bodies {
        functions.function(runtime_abi::INVOKE);
        code.function(&emit_function(
            &body.function,
            Some(body.capture_types.len()),
            &names,
            &globals,
            &bodies,
        )?);
    }
    let mut elements = ElementSection::new();
    if !bodies.is_empty() {
        let indices = (0..bodies.len())
            .map(|i| u32::try_from(names.len() + 1 + i).map_err(|_| fail("Too many functions")))
            .collect::<Result<Vec<_>, _>>()?;
        elements.declared(Elements::Functions(Cow::Owned(indices)));
    }
    let mut module = Module::new();
    module
        .section(&runtime_abi::Manifest::default().section())
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&exports);
    if !bodies.is_empty() {
        module.section(&elements);
    }
    module.section(&code);
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

fn collect_bodies<'a>(function: &'a IrFunction, bodies: &mut Vec<&'a ClosureBody>) {
    for block in &function.blocks {
        for inst in &block.instructions {
            if let Operation::MakeClosure { body, .. } = &inst.operation {
                bodies.push(body);
                collect_bodies(&body.function, bodies);
            }
        }
    }
}
fn emit_function(
    ir: &IrFunction,
    capture_count: Option<usize>,
    names: &[&str],
    globals: &[super::resolve::Global],
    bodies: &[&ClosureBody],
) -> Result<Function, Diagnostic> {
    let fail = |message: &str| Diagnostic {
        span: ir.span.clone(),
        message: message.into(),
    };
    let count = u32::try_from(ir.values.len()).map_err(|_| fail("Too many IR values"))?;
    let offset = if capture_count.is_some() { 2 } else { 0 };
    let pc = count
        .checked_add(offset)
        .ok_or_else(|| fail("Too many IR locals"))?;
    let index = |name: &str| {
        names
            .iter()
            .position(|item| *item == name)
            .expect("collected import") as u32
    };
    // Every SSA value has one GC local. The last local is the block dispatcher.
    let mut function = Function::new([(count, VALUE), (1, ValType::I32)]);
    use Instruction::*;
    if let Some(capture_count) = capture_count {
        for (index, value) in ir.blocks[0].parameters.iter().enumerate() {
            if index < capture_count {
                function
                    .instruction(&LocalGet(0))
                    .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)))
                    .instruction(&I32Const(index as i32));
            } else {
                function
                    .instruction(&LocalGet(1))
                    .instruction(&I32Const((index - capture_count) as i32));
            }
            function
                .instruction(&ArrayGet(runtime_abi::ARGS))
                .instruction(&LocalSet(value.0 as u32 + offset));
        }
    }

    function
        .instruction(&I32Const(0))
        .instruction(&LocalSet(pc))
        .instruction(&Loop(BlockType::Empty));
    for (block_id, block) in ir.blocks.iter().enumerate() {
        let block_id = i32::try_from(block_id).map_err(|_| fail("Too many IR blocks"))?;
        function
            .instruction(&LocalGet(pc))
            .instruction(&I32Const(block_id))
            .instruction(&I32Eq)
            .instruction(&If(BlockType::Empty));
        for inst in &block.instructions {
            match &inst.operation {
                Operation::Literal(Literal::Nil) => {
                    function
                        .instruction(&I32Const(0))
                        .instruction(&RefI31)
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Literal(Literal::Bool(value)) => {
                    function
                        .instruction(&I32Const(if *value { 4 } else { 2 }))
                        .instruction(&RefI31)
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Literal(Literal::Number(value)) => {
                    function
                        .instruction(&F64Const((*value).into()))
                        .instruction(&Call(index("number-box")))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Literal(Literal::String(units)) => {
                    let length = i32::try_from(units.len())
                        .map_err(|_| fail("String literal exceeds signed runtime length"))?;
                    function
                        .instruction(&I32Const(length))
                        .instruction(&Call(index("string-new")))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                    for (unit_index, unit) in units.iter().enumerate() {
                        function
                            .instruction(&LocalGet(inst.result.0 as u32 + offset))
                            .instruction(&I32Const(unit_index as i32))
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
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Call { operands } => {
                    function.instruction(&LocalGet(operands[0].0 as u32 + offset));
                    for value in &operands[1..] {
                        function.instruction(&LocalGet(value.0 as u32 + offset));
                    }
                    let size = u32::try_from(operands.len() - 1)
                        .map_err(|_| fail("Too many call arguments"))?;
                    function
                        .instruction(&ArrayNewFixed {
                            array_type_index: runtime_abi::ARGS,
                            array_size: size,
                        })
                        .instruction(&Call(index("invoke")))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::MakeClosure { body, captures } => {
                    for value in captures {
                        function.instruction(&LocalGet(value.0 as u32 + offset));
                    }
                    let size =
                        u32::try_from(captures.len()).map_err(|_| fail("Too many captures"))?;
                    let body_index = bodies
                        .iter()
                        .position(|candidate| std::ptr::eq(*candidate, body.as_ref()))
                        .expect("collected body");
                    let function_index = u32::try_from(names.len() + 1 + body_index)
                        .map_err(|_| fail("Too many functions"))?;
                    function
                        .instruction(&ArrayNewFixed {
                            array_type_index: runtime_abi::ARGS,
                            array_size: size,
                        })
                        .instruction(&RefFunc(function_index))
                        .instruction(&I32Const(body.arity as i32))
                        .instruction(&I32Const(body.arity as i32))
                        .instruction(&Call(index("closure-new")))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Arithmetic {
                    operator,
                    arguments,
                } => {
                    for argument in arguments {
                        function.instruction(&LocalGet(argument.0 as u32 + offset));
                    }
                    function
                        .instruction(&Call(index(arithmetic_name(*operator))))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
            }
        }
        match &block.terminator {
            Terminator::Return(value) => {
                function
                    .instruction(&LocalGet(value.0 as u32 + offset))
                    .instruction(&Return);
            }
            Terminator::Jump { target, arguments } => {
                // Push every replacement value before any parameter assignment.
                for argument in arguments {
                    function.instruction(&LocalGet(argument.0 as u32 + offset));
                }
                for parameter in ir.blocks[*target].parameters.iter().rev() {
                    function.instruction(&LocalSet(parameter.0 as u32 + offset));
                }
                function
                    .instruction(&I32Const(*target as i32))
                    .instruction(&LocalSet(pc))
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
                    .instruction(&LocalGet(condition.0 as u32 + offset))
                    .instruction(&I32Const(0))
                    .instruction(&RefI31)
                    .instruction(&RefEq);
                function
                    .instruction(&LocalGet(condition.0 as u32 + offset))
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
                    .instruction(&LocalSet(pc))
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
    Ok(function)
}
