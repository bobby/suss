//! Original shared-ABI core cell initialization, reusable by native and AOT hosts.
use super::{
    resolve::{Environment, Global, Phase},
    Diagnostic,
};
use crate::runtime_abi;
use wasm_encoder::*;

const VALUE: ValType = ValType::Ref(RefType::EQREF);

pub struct CoreBindings {
    pub wasm: Vec<u8>,
    pub cells: Vec<Global>,
}

/// Emit phase-qualified canonical binding cells and initialize them once at
/// instantiation. The host supplies only the shared runtime instance.
pub fn compile(phase: Phase) -> Result<CoreBindings, Diagnostic> {
    compile_with_cells(phase, &[])
}

/// Include the unbound cells discovered by portable source preparation. AOT
/// hosts instantiate this module before the source initializer fragments.
pub fn compile_with_cells(
    phase: Phase,
    source_cells: &[Global],
) -> Result<CoreBindings, Diagnostic> {
    if source_cells.iter().any(|cell| cell.phase() != phase) {
        return Err(error(
            "Core binding artifact cannot include a foreign phase cell",
        ));
    }
    let environment = Environment::default();
    let mut bindings = environment
        .core_bindings(phase)
        .into_iter()
        .map(|(cell, export)| (cell, export.to_string()))
        .collect::<Vec<_>>();
    for (cell, operator) in environment.arithmetic_bindings(phase) {
        use super::hir::Arithmetic;
        let name = match operator {
            Arithmetic::Add => "add",
            Arithmetic::Subtract => "subtract",
            Arithmetic::Multiply => "multiply",
            Arithmetic::Divide => "divide",
            Arithmetic::Negate => return Err(error("Unexpected negate core binding")),
        };
        bindings.push((cell, format!("arithmetic-{name}")));
    }
    let class = bindings
        .iter()
        .position(|(_, export)| export == "core-exception-info-class")
        .ok_or_else(|| error("Missing exception class core binding"))?;
    // The class must be initialized before any core function captures its cell.
    let mut order = vec![class];
    order.extend((0..bindings.len()).filter(|index| *index != class));
    let mut types = runtime_abi::prelude();
    let mut imports = ImportSection::new();
    types.ty().function([VALUE, VALUE], []);
    imports.import(
        "suss.runtime",
        "binding-set",
        EntityType::Function(runtime_abi::TYPE_COUNT),
    );
    for (index, (_, export)) in bindings.iter().enumerate() {
        let mut parameters = vec![];
        if export.starts_with("core-") && export != "core-exception-info-class" {
            parameters.push(VALUE);
        }
        if self_cell(export) {
            parameters.push(VALUE);
        }
        types.ty().function(parameters, [VALUE]);
        imports.import(
            "suss.runtime",
            export,
            EntityType::Function(runtime_abi::TYPE_COUNT + 1 + index as u32),
        );
    }
    let start_type = runtime_abi::TYPE_COUNT + 1 + bindings.len() as u32;
    types.ty().function([], []);
    let mut globals = GlobalSection::new();
    let mut exports = ExportSection::new();
    let mut cells = bindings
        .iter()
        .map(|(cell, _)| cell.clone())
        .collect::<Vec<_>>();
    let base_cells = cells
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    cells.extend(
        source_cells
            .iter()
            .filter(|cell| !base_cells.contains(*cell))
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
    );
    for (index, cell) in cells.iter().enumerate() {
        globals.global(
            GlobalType {
                val_type: runtime_abi::binding_cell_type(),
                mutable: false,
                shared: false,
            },
            &ConstExpr::extended([
                Instruction::I32Const(0),
                Instruction::RefI31,
                Instruction::I32Const(0),
                Instruction::StructNew(runtime_abi::BINDING),
            ]),
        );
        exports.export(&cell.import_name(), ExportKind::Global, index as u32);
    }
    let mut start = Function::new([]);
    for index in order {
        let export = &bindings[index].1;
        if self_cell(export) {
            // The native bootstrap exposes a bound nil cell to these constructors.
            start
                .instruction(&Instruction::GlobalGet(index as u32))
                .instruction(&Instruction::I32Const(0))
                .instruction(&Instruction::RefI31)
                .instruction(&Instruction::Call(0));
        }
        start.instruction(&Instruction::GlobalGet(index as u32));
        if export.starts_with("core-") && export != "core-exception-info-class" {
            start.instruction(&Instruction::GlobalGet(class as u32));
        }
        if self_cell(export) {
            start.instruction(&Instruction::GlobalGet(index as u32));
        }
        start
            .instruction(&Instruction::Call(1 + index as u32))
            .instruction(&Instruction::Call(0));
    }
    start.instruction(&Instruction::End);
    let mut functions = FunctionSection::new();
    functions.function(start_type);
    let mut code = CodeSection::new();
    code.function(&start);
    let mut module = Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&globals)
        .section(&exports)
        .section(&StartSection {
            function_index: 1 + bindings.len() as u32,
        })
        .section(&code)
        .section(&runtime_abi::Manifest::default().section());
    let wasm = module.finish();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&wasm)
        .map_err(|failure| error(failure.to_string()))?;
    let wasm =
        super::artifact_identity::annotate_source(&wasm, phase, None, Some(&[])).map_err(error)?;
    Ok(CoreBindings { wasm, cells })
}

fn self_cell(export: &str) -> bool {
    matches!(
        export,
        "core-ex-info"
            | "primitive-bit-and-function"
            | "primitive-bit-or-function"
            | "primitive-bit-xor-function"
            | "primitive-bit-and-not-function"
    )
}
fn error(message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        span: 0..0,
        message: message.into(),
    }
}
