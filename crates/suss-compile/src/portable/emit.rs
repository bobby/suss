//! Emit only verified IR. Operands are local value IDs, never source expressions.
use super::{
    hir::{Arithmetic, Literal, Nominal, Type},
    ir::{self, ClosureBody, Function as IrFunction, GeneralClosureBody, Operation, Terminator},
    Diagnostic,
};
use crate::runtime_abi;
use std::{borrow::Cow, collections::BTreeSet};
use wasm_encoder::*;
const VALUE: ValType = ValType::Ref(RefType::EQREF);
fn arithmetic_name(
    operator: Arithmetic,
    arguments: &[ir::ValueId],
    function: &IrFunction,
) -> &'static str {
    let numeric = arguments
        .iter()
        .all(|arg| function.values[arg.0].ty == Type::Number);
    match (operator, numeric) {
        (Arithmetic::Add, true) => "number-add",
        (Arithmetic::Subtract, true) => "number-subtract",
        (Arithmetic::Multiply, true) => "number-multiply",
        (Arithmetic::Divide, true) => "number-divide",
        (Arithmetic::Negate, true) => "number-negate",
        (Arithmetic::Add, false) => "value-add",
        (Arithmetic::Subtract, false) => "value-subtract",
        (Arithmetic::Multiply, false) => "value-multiply",
        (Arithmetic::Divide, false) => "value-divide",
        (Arithmetic::Negate, false) => "value-negate",
    }
}
pub fn emit(ir: &IrFunction) -> Result<Vec<u8>, Diagnostic> {
    ir::verify(ir)?;
    let fail = |message: &str| Diagnostic {
        span: ir.span.clone(),
        message: message.into(),
    };
    let mut bodies = Vec::new();
    let mut dispatchers = Vec::new();
    collect_bodies(ir, &mut bodies, &mut dispatchers);
    let all_functions = std::iter::once(ir).chain(bodies.iter().map(|body| &body.function));
    let mut names = BTreeSet::new();
    let mut throws = false;
    let mut globals = BTreeSet::new();
    for function in all_functions {
        for block in &function.blocks {
            throws |= matches!(&block.terminator, Terminator::Throw(_));
            for inst in &block.instructions {
                match &inst.operation {
                    Operation::DynamicScope {
                        globals: targets, ..
                    } => {
                        names.insert("dynamic-invoke");
                        globals.extend(targets.iter().cloned());
                    }
                    Operation::Try { .. } => {
                        names.insert("try-invoke");
                    }
                    Operation::Literal(Literal::Number(_)) => {
                        names.insert("number-box");
                    }
                    Operation::Literal(Literal::String(units)) => {
                        names.insert("string-new");
                        if !units.is_empty() {
                            names.insert("string-set-unit");
                        }
                    }
                    Operation::GlobalCell(global) => {
                        globals.insert(global.clone());
                    }
                    Operation::GlobalRead(global) => {
                        globals.insert(global.clone());
                        names.insert("binding-bound");
                        names.insert("binding-get");
                    }
                    Operation::GlobalBound(global) => {
                        globals.insert(global.clone());
                        names.insert("binding-bound");
                    }
                    Operation::GlobalWrite { global, .. } => {
                        globals.insert(global.clone());
                        names.insert("binding-set");
                    }
                    Operation::MakeGeneralClosure { body, .. } => {
                        names.insert("closure-new");
                        names.insert("arity-error");
                        if let Some(factory) = &body.rest_class {
                            globals.insert(factory.clone());
                            names.extend(["binding-get", "invoke", "source-array-new", "number-box", "constructor-descriptor", "source-constructor-new"]);
                        }
                    }
                    Operation::MakeClosure { .. } => {
                        names.insert("closure-new");
                    }
                    Operation::Call { .. } => {
                        names.insert("invoke");
                    }
                    Operation::Bitwise { operation, .. } => {
                        names.insert(operation.export());
                    }
                    Operation::Comparison { operation, .. } => {
                        names.insert(operation.export());
                    }
                    Operation::Array { operation, .. } => {
                        names.insert(operation.export());
                    }
                    Operation::Nominal { operation, .. } => match operation {
                        Nominal::Array => {}
                        Nominal::LanguageError => { names.insert("language-error-new"); }
                        Nominal::NativeObjectFactory => { names.insert("native-object-factory-function"); }
                        Nominal::NativeObjectDefaultPrototype => { names.insert("native-object-default-prototype"); }
                        Nominal::NativeObjectGet => { names.insert("native-object-property-get"); }
                        Nominal::NativeObjectSet => { names.insert("native-object-property-set"); }
                        Nominal::NativeObjectStrictSet => { names.insert("native-object-property-set-strict"); }
                        Nominal::ObjectSet => {
                            names.insert("object-method-set");
                        }
                        Nominal::ObjectInvoke => {
                            names.insert("object-method-invoke");
                        }
                        Nominal::CallableGet => { names.insert("callable-property-get"); }
                        Nominal::NamedGet => {
                            names.insert("named-property-get");
                        }
                        Nominal::NamedSet => {
                            names.insert("named-property-set");
                        }
                        Nominal::IsClosure => {}
                        Nominal::BindCallable => {
                            names.insert("callable-bind");
                        }
                        Nominal::LiveDispatcher => {
                            names.insert("protocol-live-dispatcher-new");
                        }
                        Nominal::IFnLiveDispatcher => {
                            names.insert("ifn-live-dispatcher-new");
                        }
                        Nominal::NativeMarker(_) => {
                            names.insert("protocol-native-marker-set");
                        }
                        Nominal::NativeSet(_) => {
                            names.insert("protocol-native-method-set");
                        }
                        Nominal::Descriptor => {
                            names.insert("descriptor-new");
                        }
                        Nominal::Protocol => {
                            names.insert("protocol-value-new");
                        }
                        Nominal::Class => {
                            names.insert("class-value-new");
                        }
                        Nominal::Construct => {
                            names.insert("constructor-descriptor");
                            names.insert("source-constructor-new");
                            names.insert("invoke");
                        }
                        Nominal::Instance => {
                            names.insert("constructor-descriptor");
                            names.insert("object-instance");
                        }
                        Nominal::Field(_) => {
                            names.insert("object-field-get");
                        }
                        Nominal::FieldSet(_) => {
                            names.insert("object-field-set");
                        }
                        Nominal::Key(_) => {
                            names.insert("protocol-key");
                        }
                        Nominal::Dispatcher => {
                            names.insert("protocol-dispatcher-new");
                        }
                        Nominal::Marker => {
                            names.insert("constructor-descriptor");
                            names.insert("protocol-marker-set");
                        }
                        Nominal::Set => {
                            names.insert("constructor-descriptor");
                            names.insert("protocol-method-set");
                        }
                        Nominal::NativeSatisfies => {
                            names.insert("protocol-native-satisfies");
                        }
                        Nominal::Satisfies => {
                            names.insert("protocol-marker-satisfies");
                        }
                    },
                    Operation::Arithmetic {
                        operator,
                        arguments,
                    } => {
                        names.insert(arithmetic_name(*operator, arguments, function));
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
            "protocol-live-dispatcher-new" => (vec![VALUE, VALUE], vec![VALUE]),
            "ifn-live-dispatcher-new" => (vec![VALUE, VALUE, VALUE], vec![VALUE]),
            "protocol-native-marker-set" => (vec![VALUE, ValType::I32], vec![ValType::I32]),
            "protocol-native-method-set" => (vec![VALUE, ValType::I32, VALUE], vec![VALUE]),
            "try-invoke" => (vec![VALUE, VALUE, VALUE], vec![VALUE]),
            "object-instance" | "protocol-marker-satisfies" | "protocol-native-satisfies" => {
                (vec![VALUE, VALUE], vec![ValType::I32])
            }
            "primitive-int" | "primitive-bit-not" => (vec![VALUE], vec![VALUE]),
            "primitive-bit-and"
            | "primitive-bit-or"
            | "primitive-bit-xor"
            | "primitive-bit-and-not"
            | "primitive-bit-clear"
            | "primitive-bit-flip"
            | "primitive-bit-set"
            | "primitive-bit-test"
            | "primitive-bit-shift-left"
            | "primitive-bit-shift-right"
            | "primitive-unsigned-bit-shift-right"
            | "primitive-imul" => (vec![VALUE, VALUE], vec![VALUE]),
            "comparison-less"
            | "comparison-less-equal"
            | "comparison-greater"
            | "comparison-greater-equal"
            | "comparison-strict-equal" => (vec![VALUE, VALUE], vec![VALUE]),
            "object-field-set" => (vec![VALUE, ValType::I32, VALUE], vec![]),
            "object-field-get" | "protocol-key" => (vec![VALUE, ValType::I32], vec![VALUE]),
            "protocol-marker-set" => (vec![VALUE, VALUE], vec![VALUE]),
            "protocol-method-set" => (vec![VALUE, VALUE, VALUE], vec![]),
            "descriptor-new"
            | "class-value-new"
            | "protocol-value-new"
            | "constructor-new"
            | "source-constructor-new"
            | "constructor-descriptor"
            | "protocol-dispatcher-new" => (vec![VALUE], vec![VALUE]),
            "source-array-new"
            | "source-array-make"
            | "source-array-make-literal"
            | "source-array-length-args"
            | "source-array-get-indices"
            | "source-array-set-indices" => (vec![VALUE], vec![VALUE]),
            "arity-error" | "native-object-factory-function" | "native-object-default-prototype" => (vec![], vec![VALUE]),
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
            "binding-get" | "binding-bound" | "number-negate" | "value-negate"
            | "primitive-f64-coerce" | "primitive-f64-word0" | "primitive-f64-word4"
            | "primitive-f64-floor" | "primitive-f64-ceil" | "primitive-f64-finite" | "primitive-f64-safe-integer" | "primitive-f64-time-clip" | "identity-uid" | "language-error-new" => {
                (vec![VALUE], vec![VALUE])
            }
            "binding-set" => (vec![VALUE, VALUE], vec![]),
            "callable-property-get" | "named-property-set" | "object-method-set" | "object-method-invoke" | "native-object-property-set" | "native-object-property-set-strict" => {
                (vec![VALUE, VALUE, VALUE], vec![VALUE])
            }
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
    if throws {
        types.ty().function([VALUE], []);
        imports.import(
            "suss.runtime",
            "language-exception",
            EntityType::Tag(TagType {
                kind: TagKind::Exception,
                func_type_idx: eval_type + 1,
            }),
        );
    }
    let mut functions = FunctionSection::new();
    functions.function(eval_type);
    let mut exports = ExportSection::new();
    exports.export("eval", ExportKind::Func, names.len() as u32);
    let mut code = CodeSection::new();
    code.function(&emit_function(
        ir,
        None,
        &names,
        &globals,
        &bodies,
        &dispatchers,
    )?);
    for body in &bodies {
        functions.function(runtime_abi::INVOKE);
        code.function(&emit_function(
            &body.function,
            Some(body.capture_types.len()),
            &names,
            &globals,
            &bodies,
            &dispatchers,
        )?);
    }
    for dispatcher in &dispatchers {
        functions.function(runtime_abi::INVOKE);
        code.function(&emit_dispatcher(dispatcher, &names, &globals, &bodies));
    }
    let mut elements = ElementSection::new();
    if !bodies.is_empty() || !dispatchers.is_empty() {
        let indices = (0..bodies.len() + dispatchers.len())
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
    if !bodies.is_empty() || !dispatchers.is_empty() {
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

fn collect_bodies<'a>(
    function: &'a IrFunction,
    bodies: &mut Vec<&'a ClosureBody>,
    dispatchers: &mut Vec<&'a GeneralClosureBody>,
) {
    for block in &function.blocks {
        for inst in &block.instructions {
            match &inst.operation {
                Operation::MakeClosure { body, .. } => {
                    bodies.push(body);
                    collect_bodies(&body.function, bodies, dispatchers);
                }
                Operation::MakeGeneralClosure { body, .. } => {
                    dispatchers.push(body);
                    for method in &body.methods {
                        bodies.push(method);
                        collect_bodies(&method.function, bodies, dispatchers);
                    }
                }
                _ => {}
            }
        }
    }
}
fn emit_dispatcher(
    body: &GeneralClosureBody,
    names: &[&str],
    globals: &[super::resolve::Global],
    bodies: &[&ClosureBody],
) -> Function {
    use Instruction::*;
    let mut function = Function::new([(1, ValType::I32), (2, VALUE)]);
    function
        .instruction(&LocalGet(1))
        .instruction(&ArrayLen)
        .instruction(&LocalSet(2));
    for method in body.methods.iter().filter(|method| !method.variadic) {
        let index = bodies
            .iter()
            .position(|candidate| std::ptr::eq(*candidate, method))
            .expect("collected method");
        function
            .instruction(&LocalGet(2))
            .instruction(&I32Const(method.arity as i32))
            .instruction(&I32Eq)
            .instruction(&If(BlockType::Empty))
            .instruction(&LocalGet(0))
            .instruction(&LocalGet(1))
            .instruction(&Call((names.len() + 1 + index) as u32))
            .instruction(&Return)
            .instruction(&End);
    }
    if let Some(method) = body.methods.iter().find(|method| method.variadic) {
        let minimum = method.arity - 1;
        let body_index = bodies
            .iter()
            .position(|candidate| std::ptr::eq(*candidate, method))
            .expect("collected method");
        let factory_index = globals
            .binary_search(body.rest_class.as_ref().expect("verified rest class"))
            .expect("collected rest class") as u32;
        let import = |name: &str| {
            names
                .iter()
                .position(|candidate| *candidate == name)
                .expect("collected import") as u32
        };
        function
            .instruction(&LocalGet(2))
            .instruction(&I32Const(minimum as i32))
            .instruction(&I32GeU)
            .instruction(&If(BlockType::Empty));
        // Fixed arguments and the persistent rest value form the compiled entry.
        function
            .instruction(&I32Const(method.arity as i32))
            .instruction(&ArrayNewDefault(runtime_abi::ARGS))
            .instruction(&LocalSet(3));
        function
            .instruction(&LocalGet(3))
            .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)))
            .instruction(&I32Const(0))
            .instruction(&LocalGet(1))
            .instruction(&I32Const(0))
            .instruction(&I32Const(minimum as i32))
            .instruction(&ArrayCopy {
                array_type_index_dst: runtime_abi::ARGS,
                array_type_index_src: runtime_abi::ARGS,
            });
        function
            .instruction(&LocalGet(2))
            .instruction(&I32Const(minimum as i32))
            .instruction(&I32Sub)
            .instruction(&ArrayNewDefault(runtime_abi::ARGS))
            .instruction(&LocalSet(4));
        function
            .instruction(&LocalGet(4))
            .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)))
            .instruction(&I32Const(0))
            .instruction(&LocalGet(1))
            .instruction(&I32Const(minimum as i32))
            .instruction(&LocalGet(2))
            .instruction(&I32Const(minimum as i32))
            .instruction(&I32Sub)
            .instruction(&ArrayCopy {
                array_type_index_dst: runtime_abi::ARGS,
                array_type_index_src: runtime_abi::ARGS,
            });
        // The pinned wrapper constructs the live IndexedSeq class over a fresh source array.
        // Internal Args never becomes the language rest sequence.
        function
            .instruction(&LocalGet(3))
            .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)))
            .instruction(&I32Const(minimum as i32))
            .instruction(&LocalGet(2))
            .instruction(&I32Const(minimum as i32))
            .instruction(&I32GtU)
            .instruction(&If(BlockType::Result(VALUE)))
            .instruction(&GlobalGet(factory_index))
            .instruction(&Call(import("binding-get")))
            .instruction(&Call(import("constructor-descriptor")))
            .instruction(&Call(import("source-constructor-new")))
            .instruction(&LocalGet(4))
            .instruction(&Call(import("source-array-new")))
            .instruction(&F64Const(0.0.into()))
            .instruction(&Call(import("number-box")))
            .instruction(&I32Const(0))
            .instruction(&RefI31)
            .instruction(&ArrayNewFixed {
                array_type_index: runtime_abi::ARGS,
                array_size: 3,
            })
            .instruction(&Call(import("invoke")))
            .instruction(&Else)
            .instruction(&I32Const(0))
            .instruction(&RefI31)
            .instruction(&End)
            .instruction(&ArraySet(runtime_abi::ARGS));
        function
            .instruction(&LocalGet(0))
            .instruction(&LocalGet(3))
            .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)))
            .instruction(&Call((names.len() + 1 + body_index) as u32))
            .instruction(&Return)
            .instruction(&End);
    }
    let error = names
        .iter()
        .position(|name| *name == "arity-error")
        .expect("arity helper");
    function.instruction(&Call(error as u32)).instruction(&End);
    function
}
fn emit_function(
    ir: &IrFunction,
    capture_count: Option<usize>,
    names: &[&str],
    globals: &[super::resolve::Global],
    bodies: &[&ClosureBody],
    dispatchers: &[&GeneralClosureBody],
) -> Result<Function, Diagnostic> {
    let fail = |message: &str| Diagnostic {
        span: ir.span.clone(),
        message: message.into(),
    };
    let count = u32::try_from(ir.values.len()).map_err(|_| fail("Too many IR values"))?;
    let offset = if capture_count.is_some() { 2 } else { 0 };
    let scratch = count
        .checked_add(offset)
        .ok_or_else(|| fail("Too many IR locals"))?;
    let pc = count
        .checked_add(1)
        .and_then(|count| count.checked_add(offset))
        .ok_or_else(|| fail("Too many IR locals"))?;
    let index = |name: &str| {
        names
            .iter()
            .position(|item| *item == name)
            .expect("collected import") as u32
    };
    // Every SSA value has one GC local. The last local is the block dispatcher.
    let mut function = Function::new([(count + 1, VALUE), (1, ValType::I32)]);
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
                Operation::DynamicScope {
                    globals: targets,
                    operands,
                } => {
                    for (i, target) in targets.iter().enumerate() {
                        let global = globals
                            .iter()
                            .position(|global| global == target)
                            .expect("collected dynamic cell");
                        function
                            .instruction(&GlobalGet(global as u32))
                            .instruction(&LocalGet(operands[i].0 as u32 + offset))
                            .instruction(&LocalGet(operands[targets.len() + i].0 as u32 + offset));
                    }
                    function
                        .instruction(&ArrayNewFixed {
                            array_type_index: runtime_abi::ARGS,
                            array_size: targets.len() as u32 * 3,
                        })
                        .instruction(&LocalGet(operands.last().unwrap().0 as u32 + offset))
                        .instruction(&Call(index("dynamic-invoke")))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Try { regions } => {
                    for region in regions {
                        function.instruction(&LocalGet(region.0 as u32 + offset));
                    }
                    function
                        .instruction(&Call(index("try-invoke")))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Literal(literal @ (Literal::Nil | Literal::Undefined)) => {
                    function
                        .instruction(&I32Const(if matches!(literal, Literal::Undefined) {
                            runtime_abi::UNDEFINED
                        } else {
                            0
                        }))
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
                Operation::GlobalCell(global) => {
                    let index_global = globals
                        .binary_search(global)
                        .expect("collected cell identity")
                        as u32;
                    function
                        .instruction(&GlobalGet(index_global))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::GlobalRead(global) => {
                    let index_global =
                        globals.binary_search(global).expect("collected global") as u32;
                    function
                        .instruction(&GlobalGet(index_global))
                        .instruction(&Call(index("binding-bound")))
                        .instruction(&RefCastNonNull(HeapType::I31))
                        .instruction(&I31GetU)
                        .instruction(&I32Const(4))
                        .instruction(&I32Eq)
                        .instruction(&If(BlockType::Result(VALUE)))
                        .instruction(&GlobalGet(index_global))
                        .instruction(&Call(index("binding-get")))
                        .instruction(&Else)
                        .instruction(&I32Const(runtime_abi::UNDEFINED))
                        .instruction(&RefI31)
                        .instruction(&End)
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::GlobalBound(global) => {
                    let index_global =
                        globals.binary_search(global).expect("collected global") as u32;
                    function
                        .instruction(&GlobalGet(index_global))
                        .instruction(&Call(index("binding-bound")))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::GlobalWrite { global, value } => {
                    let index_global =
                        globals.binary_search(global).expect("collected global") as u32;
                    function
                        .instruction(&GlobalGet(index_global))
                        .instruction(&LocalGet(value.0 as u32 + offset))
                        .instruction(&Call(index("binding-set")))
                        .instruction(&LocalGet(value.0 as u32 + offset))
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
                Operation::MakeGeneralClosure { body, captures } => {
                    for value in captures {
                        function.instruction(&LocalGet(value.0 as u32 + offset));
                    }
                    if body.self_capture {
                        function.instruction(&I32Const(0)).instruction(&RefI31);
                    }
                    let size = u32::try_from(captures.len() + usize::from(body.self_capture))
                        .map_err(|_| fail("Too many captures"))?;
                    let dispatcher = dispatchers
                        .iter()
                        .position(|candidate| std::ptr::eq(*candidate, body.as_ref()))
                        .expect("collected dispatcher");
                    let dispatcher_function =
                        u32::try_from(names.len() + 1 + bodies.len() + dispatcher)
                            .map_err(|_| fail("Too many functions"))?;
                    let minimum = body
                        .methods
                        .iter()
                        .map(|method| method.arity - usize::from(method.variadic))
                        .min()
                        .expect("verified methods");
                    let maximum = if body.rest_class.is_some() { -1 } else { body.methods.iter().map(|method| method.arity as i32).max().expect("verified methods") };
                    function
                        .instruction(&ArrayNewFixed {
                            array_type_index: runtime_abi::ARGS,
                            array_size: size,
                        })
                        .instruction(&LocalTee(scratch))
                        .instruction(&RefFunc(dispatcher_function))
                        .instruction(&I32Const(minimum as i32))
                        .instruction(&I32Const(maximum))
                        .instruction(&Call(index("closure-new")))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                    if body.self_capture {
                        function
                            .instruction(&LocalGet(scratch))
                            .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)))
                            .instruction(&I32Const(captures.len() as i32))
                            .instruction(&LocalGet(inst.result.0 as u32 + offset))
                            .instruction(&ArraySet(runtime_abi::ARGS));
                    }
                }
                Operation::NilTest(value) => {
                    function
                        .instruction(&LocalGet(value.0 as u32 + offset))
                        .instruction(&I32Const(0))
                        .instruction(&RefI31)
                        .instruction(&RefEq)
                        .instruction(&LocalGet(value.0 as u32 + offset))
                        .instruction(&I32Const(runtime_abi::UNDEFINED))
                        .instruction(&RefI31)
                        .instruction(&RefEq)
                        .instruction(&I32Or)
                        .instruction(&I32Const(2))
                        .instruction(&I32Mul)
                        .instruction(&I32Const(2))
                        .instruction(&I32Add)
                        .instruction(&RefI31)
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Bitwise {
                    operation,
                    arguments,
                } => {
                    for argument in arguments {
                        function.instruction(&LocalGet(argument.0 as u32 + offset));
                    }
                    function
                        .instruction(&Call(index(operation.export())))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Comparison {
                    operation,
                    arguments,
                } => {
                    for argument in arguments {
                        function.instruction(&LocalGet(argument.0 as u32 + offset));
                    }
                    function
                        .instruction(&Call(index(operation.export())))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Array {
                    operation,
                    arguments,
                } => {
                    for argument in arguments {
                        function.instruction(&LocalGet(argument.0 as u32 + offset));
                    }
                    function
                        .instruction(&ArrayNewFixed {
                            array_type_index: runtime_abi::ARGS,
                            array_size: arguments.len() as u32,
                        })
                        .instruction(&Call(index(operation.export())))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Nominal {
                    operation,
                    arguments,
                } => {
                    match operation {
                        Nominal::IsClosure => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&RefTestNonNull(HeapType::Concrete(4)));
                        }
                        Nominal::BindCallable => {
                            for argument in arguments {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function.instruction(&Call(index("callable-bind")));
                        }
                        Nominal::LiveDispatcher | Nominal::IFnLiveDispatcher => {
                            for argument in arguments {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function.instruction(&Call(index(if matches!(operation, Nominal::IFnLiveDispatcher) { "ifn-live-dispatcher-new" } else { "protocol-live-dispatcher-new" })));
                        }
                        Nominal::NativeMarker(kind) | Nominal::NativeSet(kind) => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&I32Const(kind.index()));
                            if let Some(argument) = arguments.get(1) {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function.instruction(&Call(index(
                                if matches!(operation, Nominal::NativeMarker(_)) {
                                    "protocol-native-marker-set"
                                } else {
                                    "protocol-native-method-set"
                                },
                            )));
                        }
                        Nominal::Array | Nominal::Descriptor => {
                            for argument in arguments {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function.instruction(&ArrayNewFixed {
                                array_type_index: runtime_abi::ARGS,
                                array_size: arguments.len() as u32,
                            });
                            if *operation == Nominal::Descriptor {
                                function.instruction(&Call(index("descriptor-new")));
                            }
                        }
                        Nominal::Construct => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&Call(index("constructor-descriptor")))
                                .instruction(&Call(index("source-constructor-new")));
                            for argument in &arguments[1..] {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function
                                .instruction(&ArrayNewFixed {
                                    array_type_index: runtime_abi::ARGS,
                                    array_size: arguments.len() as u32 - 1,
                                })
                                .instruction(&Call(index("invoke")));
                        }
                        Nominal::Instance | Nominal::Set | Nominal::Marker => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&Call(index("constructor-descriptor")));
                            for argument in &arguments[1..] {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function.instruction(&Call(index(
                                if *operation == Nominal::Instance {
                                    "object-instance"
                                } else if *operation == Nominal::Marker {
                                    "protocol-marker-set"
                                } else {
                                    "protocol-method-set"
                                },
                            )));
                            if *operation == Nominal::Set {
                                function.instruction(&LocalGet(arguments[2].0 as u32 + offset));
                            }
                        }
                        Nominal::FieldSet(field) => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&I32Const(*field as i32))
                                .instruction(&LocalGet(arguments[1].0 as u32 + offset))
                                .instruction(&Call(index("object-field-set")))
                                .instruction(&LocalGet(arguments[1].0 as u32 + offset));
                        }
                        Nominal::Field(field) | Nominal::Key(field) => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&I32Const(*field as i32))
                                .instruction(&Call(index(
                                    if matches!(operation, Nominal::Field(_)) {
                                        "object-field-get"
                                    } else {
                                        "protocol-key"
                                    },
                                )));
                        }
                        Nominal::ObjectInvoke => {
                            for argument in &arguments[..2] {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            for argument in &arguments[2..] {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function
                                .instruction(&ArrayNewFixed {
                                    array_type_index: runtime_abi::ARGS,
                                    array_size: arguments.len() as u32 - 2,
                                })
                                .instruction(&Call(index("object-method-invoke")));
                        }
                        Nominal::Class
                        | Nominal::LanguageError
                        | Nominal::NativeObjectFactory
                        | Nominal::NativeObjectDefaultPrototype
                        | Nominal::NativeObjectGet
                        | Nominal::NativeObjectSet
                        | Nominal::NativeObjectStrictSet
                        | Nominal::ObjectSet
                        | Nominal::NamedGet
                        | Nominal::CallableGet
                        | Nominal::NamedSet
                        | Nominal::Protocol
                        | Nominal::Dispatcher
                        | Nominal::Satisfies
                        | Nominal::NativeSatisfies => {
                            for argument in arguments {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function.instruction(&Call(index(match operation {
                                Nominal::Class => "class-value-new",
                                Nominal::LanguageError => "language-error-new",
                                Nominal::NativeObjectFactory => "native-object-factory-function",
                                Nominal::NativeObjectDefaultPrototype => "native-object-default-prototype",
                                Nominal::NativeObjectGet => "native-object-property-get",
                                Nominal::NativeObjectSet => "native-object-property-set",
                                Nominal::NativeObjectStrictSet => "native-object-property-set-strict",
                                Nominal::ObjectSet => "object-method-set",
                                Nominal::NamedGet => "named-property-get",
                            Nominal::CallableGet => "callable-property-get",
                                Nominal::NamedSet => "named-property-set",
                                Nominal::Protocol => "protocol-value-new",
                                Nominal::Dispatcher => "protocol-dispatcher-new",
                                Nominal::NativeSatisfies => "protocol-native-satisfies",
                                _ => "protocol-marker-satisfies",
                            })));
                        }
                    }
                    if operation.result() == Type::Bool {
                        function
                            .instruction(&If(BlockType::Result(ValType::I32)))
                            .instruction(&I32Const(4))
                            .instruction(&Else)
                            .instruction(&I32Const(2))
                            .instruction(&End)
                            .instruction(&RefI31);
                    }
                    function.instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Arithmetic {
                    operator,
                    arguments,
                } => {
                    for argument in arguments {
                        function.instruction(&LocalGet(argument.0 as u32 + offset));
                    }
                    function
                        .instruction(&Call(index(arithmetic_name(*operator, arguments, ir))))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
            }
        }
        match &block.terminator {
            Terminator::Throw(value) => {
                function
                    .instruction(&LocalGet(value.0 as u32 + offset))
                    .instruction(&Throw(0));
            }
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
                // Nil, false and internal undefined are falsey, including for
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
                    .instruction(&LocalGet(condition.0 as u32 + offset))
                    .instruction(&I32Const(runtime_abi::UNDEFINED))
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
