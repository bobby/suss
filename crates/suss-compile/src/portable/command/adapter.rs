//! Original command adapter: owned UTF16 arguments and ordinary compiled invoke.
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
const HELPERS: u32 = 5;
fn memory(offset: u64, align: u32) -> MemArg {
    MemArg {
        offset,
        align,
        memory_index: 0,
    }
}
fn core(ty: WasmType) -> ValType {
    match ty {
        WasmType::I32 | WasmType::Pointer | WasmType::Length => ValType::I32,
        WasmType::I64 | WasmType::PointerOrI64 => ValType::I64,
        WasmType::F32 => ValType::F32,
        WasmType::F64 => ValType::F64,
    }
}
pub(super) fn module(
    resolve: &Resolve,
    arguments: &wit_parser::Function,
    run: &wit_parser::Function,
    main: &Global,
    fragments: usize,
) -> Result<Vec<u8>, Diagnostic> {
    use Instruction::*;
    let arguments_abi = resolve.wasm_signature(AbiVariant::GuestImport, arguments);
    let run_abi = resolve.wasm_signature(AbiVariant::GuestExportAsync, run);
    let completion_abi = resolve.wasm_signature(AbiVariant::GuestExport, run);
    if arguments_abi
        .params
        .iter()
        .copied()
        .map(core)
        .collect::<Vec<_>>()
        != [ValType::I32]
        || !arguments_abi.results.is_empty()
        || !arguments_abi.retptr
        || !run_abi.params.is_empty()
        || run_abi
            .results
            .iter()
            .copied()
            .map(core)
            .collect::<Vec<_>>()
            != [ValType::I32]
        || completion_abi
            .results
            .iter()
            .copied()
            .map(core)
            .collect::<Vec<_>>()
            != [ValType::I32]
    {
        return Err(diagnostic(
            "Resolved command ABI differs from the memory32 command adapter",
        ));
    }
    let mut types = runtime_abi::prelude();
    let mut imports = ImportSection::new();
    for (index, (name, params, results)) in [
        ("binding-get", vec![VALUE], vec![VALUE]),
        ("invoke", vec![VALUE, VALUE], vec![VALUE]),
        ("string-new", vec![ValType::I32], vec![VALUE]),
        (
            "string-set-unit",
            vec![VALUE, ValType::I32, ValType::I32],
            vec![ValType::I32],
        ),
        ("args-new", vec![ValType::I32], vec![VALUE]),
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
            func_type_idx: runtime_abi::TYPE_COUNT + 5,
        }),
    );
    types.ty().function([], [VALUE]);
    for index in 0..fragments {
        imports.import(
            &format!("suss.fragment.{index}"),
            "eval",
            EntityType::Function(runtime_abi::TYPE_COUNT + 6),
        );
    }
    imports.import(
        main.import_module(),
        &main.import_name(),
        EntityType::Global(GlobalType {
            val_type: runtime_abi::binding_cell_type(),
            mutable: false,
            shared: false,
        }),
    );
    imports.import(
        "suss.memory",
        "memory",
        EntityType::Memory(MemoryType {
            minimum: 1,
            maximum: Some(65536),
            memory64: false,
            shared: false,
            page_size_log2: None,
        }),
    );
    types.ty().function(
        arguments_abi.params.iter().copied().map(core),
        arguments_abi.results.iter().copied().map(core),
    );
    types.ty().function([ValType::I32; 4], [ValType::I32]);
    types.ty().function([ValType::I32; 2], []);
    types
        .ty()
        .function(completion_abi.results.iter().copied().map(core), []);
    for (index, name) in ["arguments", "realloc", "release", "complete"]
        .into_iter()
        .enumerate()
    {
        imports.import(
            "suss.canonical",
            name,
            EntityType::Function(runtime_abi::TYPE_COUNT + 7 + index as u32),
        );
    }
    let get_arguments = HELPERS + fragments as u32;
    let realloc = get_arguments + 1;
    let release = get_arguments + 2;
    let complete = get_arguments + 3;
    let imported = get_arguments + 4;
    let mut functions = FunctionSection::new();
    types.ty().function(
        run_abi.params.iter().copied().map(core),
        run_abi.results.iter().copied().map(core),
    );
    types.ty().function([ValType::I32; 2], [VALUE]);
    types.ty().function([], []);
    types.ty().function([ValType::I32; 3], [ValType::I32]);
    for ty in 11..15 {
        functions.function(runtime_abi::TYPE_COUNT + ty);
    }
    let mut public = ExportSection::new();
    public.export("entry", ExportKind::Func, imported);
    public.export("callback", ExportKind::Func, imported + 3);
    let mut code = CodeSection::new();
    // Locals return-area/list/count/index/entry/string-pointer/units, owned GC args.
    let mut entry = Function::new([(7, ValType::I32), (1, VALUE)]);
    emit(
        &mut entry,
        &[
            I32Const(0),
            I32Const(0),
            I32Const(4),
            I32Const(8),
            Call(realloc),
            LocalSet(0),
            LocalGet(0),
            Call(get_arguments),
            LocalGet(0),
            I32Load(memory(0, 2)),
            LocalSet(1),
            LocalGet(0),
            I32Load(memory(4, 2)),
            LocalSet(2),
            LocalGet(2),
            I32Const((u32::MAX / 8) as i32),
            I32GtU,
            If(BlockType::Empty),
            Unreachable,
            End,
            // argv[0] identifies the artifact; only user arguments reach -main.
            LocalGet(2),
            If(BlockType::Result(ValType::I32)),
            LocalGet(2),
            I32Const(1),
            I32Sub,
            Else,
            I32Const(0),
            End,
            Call(4),
            LocalSet(7),
            Block(BlockType::Empty),
            Loop(BlockType::Empty),
            LocalGet(3),
            LocalGet(2),
            I32GeU,
            BrIf(1),
            LocalGet(1),
            LocalGet(3),
            I32Const(8),
            I32Mul,
            I32Add,
            LocalSet(4),
            LocalGet(4),
            I32Load(memory(0, 2)),
            LocalSet(5),
            LocalGet(4),
            I32Load(memory(4, 2)),
            LocalSet(6),
            LocalGet(6),
            I32Const((u32::MAX / 2) as i32),
            I32GtU,
            If(BlockType::Empty),
            Unreachable,
            End,
            LocalGet(3),
            If(BlockType::Empty),
            LocalGet(7),
            RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
            LocalGet(3),
            I32Const(1),
            I32Sub,
            LocalGet(5),
            LocalGet(6),
            Call(imported + 1),
            ArraySet(runtime_abi::ARGS),
            End,
            LocalGet(5),
            LocalGet(6),
            I32Const(2),
            I32Mul,
            Call(release),
            LocalGet(3),
            I32Const(1),
            I32Add,
            LocalSet(3),
            Br(0),
            End,
            End,
            LocalGet(1),
            LocalGet(2),
            I32Const(8),
            I32Mul,
            Call(release),
            LocalGet(0),
            I32Const(8),
            Call(release),
            // Source exceptions become command failure, after all argument buffers
            // have been copied to rooted GC values and freed.
            Block(BlockType::Result(VALUE)),
            TryTable(
                BlockType::Empty,
                Cow::Owned(vec![wasm_encoder::Catch::One { tag: 0, label: 0 }]),
            ),
            GlobalGet(0),
            Call(0),
            LocalGet(7),
            Call(1),
            Drop,
            I32Const(0),
            Call(complete),
            I32Const(0),
            Return,
            End,
            Unreachable,
            End,
            Drop,
            I32Const(1),
            Call(complete),
            I32Const(0),
            End,
        ],
    );
    code.function(&entry);
    // Canonical UTF16 is copied into an owned language string before release.
    let mut copy = Function::new([(1, VALUE), (1, ValType::I32)]);
    emit(
        &mut copy,
        &[
            LocalGet(1),
            Call(2),
            LocalSet(2),
            Block(BlockType::Empty),
            Loop(BlockType::Empty),
            LocalGet(3),
            LocalGet(1),
            I32GeU,
            BrIf(1),
            LocalGet(2),
            LocalGet(3),
            LocalGet(0),
            LocalGet(3),
            I32Const(2),
            I32Mul,
            I32Add,
            I32Load16U(memory(0, 1)),
            Call(3),
            Drop,
            LocalGet(3),
            I32Const(1),
            I32Add,
            LocalSet(3),
            Br(0),
            End,
            End,
            LocalGet(2),
            End,
        ],
    );
    code.function(&copy);
    let mut start = Function::new([]);
    for index in 0..fragments {
        emit(&mut start, &[Call(HELPERS + index as u32), Drop]);
    }
    emit(&mut start, &[End]);
    code.function(&start);
    let mut callback = Function::new([]);
    emit(&mut callback, &[Unreachable, End]);
    code.function(&callback);
    let mut module = Module::new();
    module
        .section(&runtime_abi::Manifest::default().section())
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&public)
        .section(&StartSection {
            function_index: imported + 2,
        })
        .section(&code);
    Ok(module.finish())
}
fn emit(function: &mut Function, instructions: &[Instruction<'_>]) {
    for instruction in instructions {
        function.instruction(instruction);
    }
}

#[cfg(test)]
mod tests {
    use crate::portable::{
        self,
        resolve::{Environment, Phase},
    };
    use suss_reader::Symbol;
    use wit_parser::WorldItem;

    #[test]
    fn command_adapter_core_validates_against_resolved_official_signatures() {
        let (resolve, world) = super::super::official_profile().unwrap();
        let selected = &resolve.worlds[world];
        let arguments = selected
            .imports
            .iter()
            .find_map(|(key, item)| {
                if resolve.name_world_key(key) != "wasi:cli/environment@0.3.1" {
                    return None;
                }
                let WorldItem::Interface { id, .. } = item else {
                    return None;
                };
                resolve.interfaces[*id].functions.get("get-arguments")
            })
            .unwrap();
        let run = selected
            .exports
            .iter()
            .find_map(|(key, item)| {
                if resolve.name_world_key(key) != "wasi:cli/run@0.3.1" {
                    return None;
                }
                let WorldItem::Interface { id, .. } = item else {
                    return None;
                };
                resolve.interfaces[*id].functions.get("run")
            })
            .unwrap();
        let fragment = portable::prepare_fragment(
            "(ns app) (def -main (fn [] 73))",
            &Environment::default(),
            Phase::Runtime,
        )
        .unwrap();
        let binding = fragment
            .environment
            .resolve(Phase::Runtime, &Symbol::namespaced("app", "-main"), 0..0)
            .unwrap();
        let module = super::module(&resolve, arguments, run, binding.global(), 1).unwrap();
        wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
            .validate_all(&module)
            .unwrap();
    }
}
