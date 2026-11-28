//! Test-only observation of Suss's current owned canonical allocator.
//! The original component is an exact prefix; only a read-only observer is added.
//! Restricted to the checked allocator module and canonical alignments <=16.
use wasm_encoder::*;
use wasmparser::{Instance, Parser, Payload};

pub fn instrument(bytes: &[u8]) -> Vec<u8> {
    let types = wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(bytes)
        .unwrap();
    let counts = types.as_ref();
    let mut depth = 0;
    let mut modules = 0;
    let mut instances = 0;
    let mut allocator_module = None;
    let mut allocator_instance = None;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.unwrap() {
            Payload::Version { .. } => depth += 1,
            Payload::End(_) => depth -= 1,
            Payload::ModuleSection {
                unchecked_range, ..
            } if depth == 1 => {
                let mut names = Vec::new();
                let mut imports = 0;
                let mut functions = 0;
                let mut memories = 0;
                let mut globals = 0;
                for p in Parser::new(0).parse_all(
                    &bytes[usize::try_from(unchecked_range.start).unwrap()
                        ..usize::try_from(unchecked_range.end).unwrap()],
                ) {
                    match p.unwrap() {
                        Payload::ExportSection(exports) => {
                            for e in exports {
                                names.push(e.unwrap().name.to_owned());
                            }
                        }
                        Payload::ImportSection(section) => imports += section.count(),
                        Payload::FunctionSection(section) => functions += section.count(),
                        Payload::MemorySection(section) => memories += section.count(),
                        Payload::GlobalSection(section) => globals += section.count(),
                        _ => {}
                    }
                }
                names.sort();
                if imports == 0
                    && ((functions == 5 && globals == 2)
                        || ((functions == 6 || functions == 7) && globals == 4))
                    && memories == 1
                    && names == ["cabi_realloc", "memory", "release"]
                {
                    assert!(
                        allocator_module.replace(modules).is_none(),
                        "ambiguous allocator"
                    );
                }
                modules += 1;
            }
            Payload::InstanceSection(section) if depth == 1 => {
                for instance in section {
                    if let Instance::Instantiate { module_index, .. } = instance.unwrap() {
                        if Some(module_index) == allocator_module {
                            assert!(allocator_instance.replace(instances).is_none());
                        }
                    }
                    instances += 1;
                }
            }
            _ => {}
        }
    }
    assert_eq!(modules, counts.module_count());
    assert_eq!(instances, counts.core_instance_count());
    let allocator = allocator_instance.expect("checked canonical allocator instance");
    let mut result = bytes.to_vec();
    ModuleSection(&observer()).append_to_component(&mut result);
    let mut section = InstanceSection::new();
    section.instantiate(modules, [("env", ModuleArg::Instance(allocator))]);
    section.append_to_component(&mut result);
    let mut aliases = ComponentAliasSection::new();
    aliases.alias(Alias::CoreInstanceExport {
        instance: instances,
        kind: ExportKind::Func,
        name: "stats",
    });
    aliases.append_to_component(&mut result);
    let mut definitions = ComponentTypeSection::new();
    definitions
        .function()
        .params([] as [(&str, ComponentValType); 0])
        .result(Some(PrimitiveValType::U64.into()));
    definitions.append_to_component(&mut result);
    let mut canonical = CanonicalFunctionSection::new();
    canonical.lift(counts.function_count(), counts.component_type_count(), []);
    canonical.append_to_component(&mut result);
    let mut exports = ComponentExportSection::new();
    exports.export(
        "canonical-stats",
        ComponentExportKind::Func,
        counts.component_function_count(),
        None,
    );
    exports.append_to_component(&mut result);
    assert!(result.starts_with(bytes));
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&result)
        .unwrap();
    result
}
fn observer() -> Module {
    use Instruction::*;
    let mut types = TypeSection::new();
    types.ty().function([], [ValType::I64]);
    let mut imports = ImportSection::new();
    imports.import(
        "env",
        "memory",
        EntityType::Memory(MemoryType {
            minimum: 1,
            maximum: Some(65536),
            memory64: false,
            shared: false,
            page_size_log2: None,
        }),
    );
    let mut functions = FunctionSection::new();
    functions.function(0);
    let mut exports = ExportSection::new();
    exports.export("stats", ExportKind::Func, 0);
    let mut body = Function::new([(6, ValType::I32), (3, ValType::I64)]);
    let memory = |offset| MemArg {
        offset,
        align: 2,
        memory_index: 0,
    };
    for op in [
        I32Const(16),
        LocalSet(0),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(0),
        I64ExtendI32U,
        I64Const(16),
        I64Add,
        MemorySize(0),
        I64ExtendI32U,
        I64Const(16),
        I64Shl,
        I64GtU,
        BrIf(1),
        LocalGet(0),
        I32Load(memory(4)),
        LocalTee(2),
        I32Eqz,
        BrIf(1),
        LocalGet(0),
        I32Load(memory(0)),
        LocalGet(1),
        I32Ne,
        If(BlockType::Empty),
        Unreachable,
        End,
        LocalGet(0),
        I32Load(memory(12)),
        LocalTee(3),
        I32Const(1),
        I32GtU,
        If(BlockType::Empty),
        Unreachable,
        End,
        LocalGet(0),
        I32Load(memory(8)),
        LocalTee(4),
        LocalGet(2),
        I32GtU,
        // Inactive headers now link the free list through their length slot.
        LocalGet(3),
        I32And,
        If(BlockType::Empty),
        Unreachable,
        End,
        LocalGet(3),
        If(BlockType::Empty),
        LocalGet(6),
        I64Const(1),
        I64Add,
        LocalSet(6),
        LocalGet(7),
        LocalGet(4),
        I64ExtendI32U,
        I64Add,
        LocalSet(7),
        End,
        LocalGet(0),
        I32Const(16),
        I32Add,
        LocalTee(1),
        I64ExtendI32U,
        LocalGet(2),
        I64ExtendI32U,
        I64Add,
        I64Const(15),
        I64Add,
        I64Const(-16),
        I64And,
        LocalTee(8),
        MemorySize(0),
        I64ExtendI32U,
        I64Const(16),
        I64Shl,
        I64GtU,
        If(BlockType::Empty),
        Unreachable,
        End,
        LocalGet(8),
        LocalGet(0),
        I64ExtendI32U,
        I64LeU,
        If(BlockType::Empty),
        Unreachable,
        End,
        LocalGet(8),
        I32WrapI64,
        LocalSet(0),
        Br(0),
        End,
        End,
        LocalGet(7),
        I64Const(u32::MAX as i64),
        I64GtU,
        If(BlockType::Empty),
        Unreachable,
        End,
        LocalGet(6),
        I64Const(32),
        I64Shl,
        LocalGet(7),
        I64Or,
        End,
    ] {
        body.instruction(&op);
    }
    let mut code = CodeSection::new();
    code.function(&body);
    let mut module = Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&exports)
        .section(&code);
    module
}
