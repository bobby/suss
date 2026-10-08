//! Generated serial scalar canonical async component assembly.
//!
//! This extracts the executing transport fixture into production generation,
//! without claiming concurrent invocation, arbitrary WIT shapes or full async
//! acceptance. Ordered prepared Runtime fragments, one import and one export are
//! supported. The entry rejects reentry before changing invocation roots.
//! Source initialization runs once at instantiation; callbacks pump bounded
//! source turns. The extracted driver still traps on failed terminal outcomes
//! after draining outstanding imports at completion. Host trap recovery,
//! invocation-key overflow and interrupted allocation retirement require further
//! integration before broader acceptance. Do not use this for concurrent calls.
//! Bridge resource errors propagate without dropping context or roots; the host
//! must quarantine the invocation/retire its Store. Canonical calls are not
//! replayed by this driver, and native traps cannot be caught as language errors.
use super::{async_bridge, diagnostic};
use crate::{
    portable::{
        Diagnostic, PreparedFragment, artifact_identity, core_bindings,
        resolve::{Global, Phase},
    },
    runtime_abi,
};
use wasm_encoder::*;
use wit_parser::{FunctionKind, Resolve, Type, TypeDefKind};

/// A resolved world-level async import and its predeclared Runtime binding cell.
/// Declare this cell in the source environment before preparing the fragment.
pub struct ImportMapping<'a> {
    pub function: &'a wit_parser::Function,
    pub global: &'a Global,
}
/// A resolved world-level async export and the compiled source function cell.
/// The source function must return a task future containing a u32 result.
pub struct ExportMapping<'a> {
    pub function: &'a wit_parser::Function,
    pub global: &'a Global,
}

/// Generate the bounded serial transport. Unsupported shapes are diagnostics,
/// not a fallback to synchronous invocation. Ordered bootstrap, dependency and
/// source fragments share cells and initialize once, after bridge installation.
pub fn component(
    fragments: &[PreparedFragment],
    resolve: &Resolve,
    import: ImportMapping<'_>,
    export: ExportMapping<'_>,
) -> Result<Vec<u8>, Diagnostic> {
    if fragments.is_empty() {
        return Err(diagnostic(
            "Serial scalar async assembly requires prepared fragments",
        ));
    }
    check_scalar(resolve, import.function)?;
    check_scalar(resolve, export.function)?;
    if import.global.phase() != Phase::Runtime || export.global.phase() != Phase::Runtime {
        return Err(diagnostic("Async component mappings require Runtime cells"));
    }
    if import.global == export.global {
        return Err(diagnostic(
            "Async import and export require distinct source cells",
        ));
    }
    let environment = &fragments.last().expect("nonempty fragments").environment;
    for global in [import.global, export.global] {
        let symbol = suss_reader::Symbol::namespaced(global.namespace(), global.name());
        let binding = environment.resolve(Phase::Runtime, &symbol, 0..0)?;
        if binding.global() != global {
            return Err(diagnostic(
                "Async mapping does not resolve to its source cell",
            ));
        }
    }
    // Reject a definition in any initializer, even if a later catalog overwrites
    // its source facts. Installing a bridge must never be followed by a source
    // definition that silently replaces that callable import.
    if fragments.iter().any(|fragment| {
        fragment
            .environment
            .definition_info(import.global)
            .is_some()
    }) {
        return Err(diagnostic(
            "Async import cell must not have a source definition",
        ));
    }
    let definition = environment
        .definition_info(export.global)
        .ok_or_else(|| diagnostic("Async export requires a source definition"))?;
    if !definition.analysis_completed || definition.initializer.is_none() {
        return Err(diagnostic(
            "Async export requires a completed source initializer",
        ));
    }
    let mut cells = Vec::new();
    for fragment in fragments {
        artifact_identity::verify(
            &fragment.wasm,
            artifact_identity::Expected {
                phase: Some(Phase::Runtime),
                ..Default::default()
            },
        )
        .map_err(diagnostic)?;
        cells.extend(fragment.cells.iter().cloned());
    }
    cells.extend([import.global.clone(), export.global.clone()]);
    cells.sort();
    cells.dedup();
    let bindings = core_bindings::compile_with_cells(Phase::Runtime, &cells)?;
    let bridge = async_bridge::module(resolve, import.function, import.global)?;
    let driver = driver(export.global, fragments.len());
    let count =
        u32::try_from(fragments.len()).map_err(|_| diagnostic("Too many async fragments"))?;
    let shim = 3 + count;
    let context_instance = shim + 1;
    let bridge_instance = shim + 2;
    let driver_instance = shim + 3;
    let mut component = wasm_encoder::Component::new();
    let mut types = ComponentTypeSection::new();
    for function in [import.function, export.function] {
        types
            .function()
            .async_(true)
            .params([(function.params[0].name.as_str(), PrimitiveValType::U32)])
            .result(Some(PrimitiveValType::U32.into()));
    }
    component.section(&types);
    let mut imports = ComponentImportSection::new();
    imports.import(import.function.name.as_str(), ComponentTypeRef::Func(0));
    component.section(&imports);
    let mut modules = vec![
        runtime_abi::module(),
        super::memory::module(),
        bindings.wasm,
    ];
    modules.extend(fragments.iter().map(|fragment| fragment.wasm.clone()));
    modules.extend([async_bridge::context_module(), bridge, driver]);
    for bytes in modules {
        component.section(&RawSection {
            id: ComponentSectionId::CoreModule.into(),
            data: &bytes,
        });
    }
    let mut instances = InstanceSection::new();
    instances.instantiate(0, [] as [(&str, ModuleArg); 0]);
    instances.instantiate(1, [] as [(&str, ModuleArg); 0]);
    instances.instantiate(2, [("suss.runtime", ModuleArg::Instance(0))]);
    for index in 0..count {
        instances.instantiate(
            3 + index,
            [
                ("suss.runtime", ModuleArg::Instance(0)),
                ("suss.bindings.runtime", ModuleArg::Instance(2)),
            ],
        );
    }
    component.section(&instances);
    let mut aliases = ComponentAliasSection::new();
    aliases.alias(Alias::CoreInstanceExport {
        instance: 1,
        kind: ExportKind::Memory,
        name: "memory",
    });
    for name in ["cabi_realloc", "release"] {
        aliases.alias(Alias::CoreInstanceExport {
            instance: 1,
            kind: ExportKind::Func,
            name,
        });
    }
    component.section(&aliases);
    let mut canonical = CanonicalFunctionSection::new();
    canonical.lower(0, [CanonicalOption::Async, CanonicalOption::Memory(0)]);
    canonical
        .context_get(ValType::I32, 0)
        .waitable_join()
        .subtask_drop()
        .subtask_cancel(true)
        .waitable_set_new()
        .waitable_set_drop()
        .context_set(ValType::I32, 0)
        .task_return(Some(PrimitiveValType::U32.into()), []);
    component.section(&canonical);
    let mut instances = InstanceSection::new();
    instances.export_items([
        ("call", ExportKind::Func, 2),
        ("context", ExportKind::Func, 3),
        ("join", ExportKind::Func, 4),
        ("subtask-drop", ExportKind::Func, 5),
        ("cancel", ExportKind::Func, 6),
        ("set-new", ExportKind::Func, 7),
        ("set-drop", ExportKind::Func, 8),
        ("context-set", ExportKind::Func, 9),
        ("return", ExportKind::Func, 10),
    ]);
    instances.instantiate(3 + count, [("suss.canonical", ModuleArg::Instance(shim))]);
    instances.instantiate(
        4 + count,
        [
            ("suss.runtime", ModuleArg::Instance(0)),
            ("suss.bindings.runtime", ModuleArg::Instance(2)),
            ("suss.memory", ModuleArg::Instance(1)),
            ("suss.canonical", ModuleArg::Instance(shim)),
            ("suss.context", ModuleArg::Instance(context_instance)),
        ],
    );
    let mut driver_args = vec![
        ("suss.runtime".to_owned(), ModuleArg::Instance(0)),
        ("suss.bindings.runtime".to_owned(), ModuleArg::Instance(2)),
        (
            "suss.bridge".to_owned(),
            ModuleArg::Instance(bridge_instance),
        ),
        ("suss.canonical".to_owned(), ModuleArg::Instance(shim)),
        (
            "suss.context".to_owned(),
            ModuleArg::Instance(context_instance),
        ),
    ];
    for index in 0..count {
        driver_args.push((
            format!("suss.fragment.{index}"),
            ModuleArg::Instance(3 + index),
        ));
    }
    instances.instantiate(
        5 + count,
        driver_args
            .iter()
            .map(|(name, argument)| (name.as_str(), *argument)),
    );
    component.section(&instances);
    let mut aliases = ComponentAliasSection::new();
    for name in ["entry", "callback"] {
        aliases.alias(Alias::CoreInstanceExport {
            instance: driver_instance,
            kind: ExportKind::Func,
            name,
        });
    }
    component.section(&aliases);
    let mut canonical = CanonicalFunctionSection::new();
    canonical.lift(
        11,
        1,
        [CanonicalOption::Async, CanonicalOption::Callback(12)],
    );
    component.section(&canonical);
    let mut exports = ComponentExportSection::new();
    exports.export(
        export.function.name.as_str(),
        ComponentExportKind::Func,
        1,
        None,
    );
    component.section(&exports);
    let bytes = component.finish();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&bytes)
        .map_err(|error| diagnostic(format!("Invalid generated async component: {error}")))?;
    Ok(bytes)
}
fn driver(main: &Global, fragment_count: usize) -> Vec<u8> {
    use Instruction::*;
    let value = ValType::Ref(RefType::EQREF);
    let mut types = runtime_abi::prelude();
    let mut next_type = runtime_abi::TYPE_COUNT;
    let mut imports = ImportSection::new();
    let mut index = 0;
    let mut add = |module: &str, name: &str, params: Vec<ValType>, results: Vec<ValType>| {
        let ty = next_type;
        next_type += 1;
        types.ty().function(params, results);
        imports.import(module, name, EntityType::Function(ty));
        let n = index;
        index += 1;
        n
    };
    let get = add("suss.runtime", "binding-get", vec![value], vec![value]);
    let invoke = add("suss.runtime", "invoke", vec![value, value], vec![value]);
    let number = add(
        "suss.runtime",
        "number-box",
        vec![ValType::F64],
        vec![value],
    );
    let status = add(
        "suss.runtime",
        "future-status",
        vec![value],
        vec![ValType::I32],
    );
    let result = add("suss.runtime", "future-result", vec![value], vec![value]);
    let pump = add(
        "suss.runtime",
        "async-run-invocation",
        vec![ValType::I32],
        vec![ValType::I32],
    );
    let enter_scope = add(
        "suss.runtime",
        "async-enter-invocation",
        vec![ValType::I32],
        vec![ValType::I32],
    );
    let cancel_scope = add(
        "suss.runtime",
        "async-cancel-invocation",
        vec![ValType::I32],
        vec![ValType::I32],
    );
    let scope_counts = add(
        "suss.runtime",
        "async-invocation-counts",
        vec![ValType::I32],
        vec![ValType::I32; 3],
    );
    let retiring_count = add(
        "suss.runtime",
        "async-invocation-retiring-count",
        vec![ValType::I32],
        vec![ValType::I32],
    );
    let recover_runtime = add("suss.runtime", "async-scheduler-recover", vec![], vec![]);
    let install = add("suss.bridge", "install", vec![], vec![]);
    let event = add(
        "suss.bridge",
        "complete-event",
        vec![ValType::I32; 3],
        vec![ValType::I32],
    );
    let pending = add(
        "suss.bridge",
        "pending-invocation",
        vec![ValType::I32],
        vec![ValType::I32],
    );
    let poll_invocation = add(
        "suss.bridge",
        "poll-invocation",
        vec![ValType::I32],
        vec![ValType::I32],
    );
    let resume_invocation = add(
        "suss.bridge",
        "resume-invocation",
        vec![ValType::I32],
        vec![ValType::I32],
    );
    let cancel_invocation = add(
        "suss.bridge",
        "cancel-invocation",
        vec![ValType::I32],
        vec![ValType::I32],
    );
    let quarantine_bridge = add(
        "suss.bridge",
        "quarantine-invocation",
        vec![ValType::I32],
        vec![],
    );
    let initializers: Vec<_> = (0..fragment_count)
        .map(|index| {
            add(
                &format!("suss.fragment.{index}"),
                "eval",
                vec![],
                vec![value],
            )
        })
        .collect();
    let set_new = add("suss.canonical", "set-new", vec![], vec![ValType::I32]);
    let set_drop = add("suss.canonical", "set-drop", vec![ValType::I32], vec![]);
    let context_set = add("suss.canonical", "context-set", vec![ValType::I32], vec![]);
    let register_context = add("suss.context", "register", vec![ValType::I32; 2], vec![]);
    let retire_context = add("suss.context", "retire", vec![ValType::I32], vec![]);
    let ret = add("suss.canonical", "return", vec![ValType::I32], vec![]);
    drop(add);
    let language_tag_type = next_type;
    next_type += 1;
    types.ty().function([value], []);
    imports.import(
        "suss.runtime",
        "language-exception",
        EntityType::Tag(TagType {
            kind: TagKind::Exception,
            func_type_idx: language_tag_type,
        }),
    );
    imports.import(
        main.import_module(),
        &main.import_name(),
        EntityType::Global(GlobalType {
            val_type: runtime_abi::binding_cell_type(),
            mutable: false,
            shared: false,
        }),
    );
    let mut globals = GlobalSection::new();
    globals.global(
        GlobalType {
            val_type: value,
            mutable: true,
            shared: false,
        },
        &ConstExpr::extended([I32Const(0), RefI31]),
    );
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(0),
    );
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(i32::MAX),
    );
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(0),
    );
    // Previous scope and interrupted entry scope latch.
    for _ in 0..2 {
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i32_const(0),
        );
    }
    // Permanent quarantine latch: never resume uncertain canonical operations.
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(0),
    );
    // Imported main=0, rooted task=1, set=2, monotonic invocation=3, active=4.
    let mut functions = FunctionSection::new();
    let mut code = CodeSection::new();
    let init = index;
    let ty = next_type;
    next_type += 1;
    types.ty().function([], []);
    functions.function(ty);
    let mut initialization = vec![Call(install)];
    for eval in initializers {
        initialization.extend([Call(eval), Drop]);
    }
    initialization.push(End);
    emit(&mut code, initialization);
    let step = index + 1;
    let ty = next_type;
    next_type += 1;
    types.ty().function([], [ValType::I32]);
    functions.function(ty);
    emit_with_locals(
        &mut code,
        [(1, ValType::F64), (4, ValType::I32)],
        vec![
            GlobalGet(1),
            Call(status),
            I32Eqz,
            I32Eqz,
            GlobalGet(3),
            Call(retiring_count),
            I32Eqz,
            GlobalGet(3),
            Call(scope_counts),
            Drop,
            Drop,
            I32Eqz,
            I32Eqz,
            I32And,
            I32And,
            If(BlockType::Empty),
            GlobalGet(3),
            Call(cancel_scope),
            Drop,
            End,
            GlobalGet(3),
            Call(poll_invocation),
            Drop,
            GlobalGet(3),
            Call(resume_invocation),
            Drop,
            GlobalGet(3),
            Call(pump),
            Drop,
            GlobalGet(3),
            Call(poll_invocation),
            Drop,
            GlobalGet(3),
            Call(resume_invocation),
            Drop,
            GlobalGet(1),
            Call(status),
            I32Eqz,
            If(BlockType::Empty),
            GlobalGet(3),
            Call(pump),
            LocalSet(1),
            GlobalGet(3),
            Call(poll_invocation),
            Drop,
            GlobalGet(3),
            Call(resume_invocation),
            Drop,
            LocalGet(1),
            If(BlockType::Empty),
            I32Const(1),
            Return,
            End,
            GlobalGet(2),
            I32Const(4),
            I32Shl,
            I32Const(2),
            I32Or,
            Return,
            End,
            // Retain the parent's terminal outcome while descendants unwind.
            // Each wave waits for all cancellation-latched owners to retire.
            // This protects awaited cleanup children, then cancels unawaited
            // descendants left behind by completed cleanup. Retry after a trap
            // relies on the runtime's idempotent owner cancellation requests.
            GlobalGet(3),
            Call(retiring_count),
            I32Eqz,
            GlobalGet(3),
            Call(scope_counts),
            Drop,
            Drop,
            I32Eqz,
            I32Eqz,
            I32And,
            If(BlockType::Empty),
            GlobalGet(3),
            Call(cancel_scope),
            Drop,
            End,
            GlobalGet(3),
            Call(scope_counts),
            LocalSet(4),
            LocalSet(3),
            LocalSet(2),
            LocalGet(2),
            LocalGet(3),
            I32Or,
            LocalGet(4),
            I32Or,
            If(BlockType::Empty),
            LocalGet(3),
            LocalGet(4),
            I32Or,
            If(BlockType::Empty),
            I32Const(1),
            Return,
            End,
            GlobalGet(2),
            I32Const(4),
            I32Shl,
            I32Const(2),
            I32Or,
            Return,
            End,
            // Only now cancel leftover canonical imports and drain receipts.
            GlobalGet(3),
            Call(cancel_invocation),
            Drop,
            GlobalGet(3),
            Call(resume_invocation),
            Drop,
            GlobalGet(3),
            Call(pending),
            If(BlockType::Empty),
            GlobalGet(2),
            I32Const(4),
            I32Shl,
            I32Const(2),
            I32Or,
            Return,
            End,
            GlobalGet(1),
            Call(status),
            I32Const(1),
            I32Ne,
            If(BlockType::Empty),
            Unreachable,
            End,
            GlobalGet(1),
            Call(result),
            RefCastNonNull(HeapType::Concrete(runtime_abi::NUMBER)),
            StructGet {
                struct_type_index: runtime_abi::NUMBER,
                field_index: 0,
            },
            LocalSet(0),
            LocalGet(0),
            F64Const(0.0.into()),
            F64Ge,
            LocalGet(0),
            F64Const(4294967295.0.into()),
            F64Le,
            I32And,
            LocalGet(0),
            LocalGet(0),
            F64Trunc,
            F64Eq,
            I32And,
            I32Eqz,
            If(BlockType::Empty),
            Unreachable,
            End,
            LocalGet(0),
            I32TruncF64U,
            Call(ret),
            GlobalGet(2),
            Call(set_drop),
            GlobalGet(3),
            Call(retire_context),
            I32Const(0),
            RefI31,
            GlobalSet(1),
            I32Const(0),
            GlobalSet(4),
            I32Const(0),
            End,
        ],
    );
    let entry = index + 2;
    let ty = next_type;
    next_type += 1;
    types.ty().function([ValType::I32], [ValType::I32]);
    functions.function(ty);
    emit_with_locals(
        &mut code,
        [(1, value)],
        vec![
            GlobalGet(7),
            If(BlockType::Empty),
            Unreachable,
            End,
            GlobalGet(4),
            If(BlockType::Empty),
            Unreachable,
            End,
            I32Const(1),
            GlobalSet(4),
            GlobalGet(3),
            I32Const(1),
            I32Add,
            GlobalSet(3),
            GlobalGet(3),
            Call(context_set),
            Call(set_new),
            GlobalSet(2),
            GlobalGet(3),
            GlobalGet(2),
            Call(register_context),
            GlobalGet(3),
            Call(enter_scope),
            GlobalSet(5),
            I32Const(1),
            GlobalSet(6),
            Block(BlockType::Empty),
            Block(BlockType::Result(value)),
            TryTable(
                BlockType::Empty,
                std::borrow::Cow::Owned(vec![wasm_encoder::Catch::One { tag: 0, label: 0 }]),
            ),
            GlobalGet(0),
            Call(get),
            LocalGet(0),
            F64ConvertI32U,
            Call(number),
            ArrayNewFixed {
                array_type_index: runtime_abi::ARGS,
                array_size: 1,
            },
            Call(invoke),
            GlobalSet(1),
            GlobalGet(5),
            Call(enter_scope),
            Drop,
            I32Const(0),
            GlobalSet(6),
            Br(2),
            End,
            Unreachable,
            End,
            LocalSet(1),
            GlobalGet(5),
            Call(enter_scope),
            Drop,
            I32Const(0),
            GlobalSet(6),
            LocalGet(1),
            Throw(0),
            End,
            Call(step),
            End,
        ],
    );
    let callback = index + 3;
    let ty = next_type;
    next_type += 1;
    types.ty().function([ValType::I32; 3], [ValType::I32]);
    functions.function(ty);
    emit(
        &mut code,
        vec![
            GlobalGet(7),
            If(BlockType::Empty),
            Unreachable,
            End,
            LocalGet(0),
            I32Const(1),
            I32Eq,
            If(BlockType::Empty),
            LocalGet(0),
            LocalGet(1),
            LocalGet(2),
            Call(event),
            I32Eqz,
            If(BlockType::Empty),
            Unreachable,
            End,
            Else,
            LocalGet(0),
            If(BlockType::Empty),
            Unreachable,
            End,
            End,
            Call(step),
            End,
        ],
    );
    // Host recovery is explicit after a core trap; it never replays entry.
    let recovery = index + 4;
    let ty = next_type;
    types.ty().function([], []);
    functions.function(ty);
    emit(
        &mut code,
        vec![
            GlobalGet(7),
            If(BlockType::Empty),
            Unreachable,
            End,
            Call(recover_runtime),
            GlobalGet(6),
            If(BlockType::Empty),
            GlobalGet(5),
            Call(enter_scope),
            Drop,
            I32Const(0),
            GlobalSet(6),
            End,
            End,
        ],
    );
    let quarantine = index + 5;
    let ty = next_type;
    types.ty().function([], []);
    functions.function(ty);
    emit(
        &mut code,
        vec![
            I32Const(1),
            GlobalSet(7),
            GlobalGet(4),
            If(BlockType::Empty),
            GlobalGet(3),
            Call(quarantine_bridge),
            End,
            End,
        ],
    );
    let mut exports = ExportSection::new();
    exports
        .export("quarantine", ExportKind::Func, quarantine)
        .export("entry", ExportKind::Func, entry)
        .export("callback", ExportKind::Func, callback)
        .export("recover", ExportKind::Func, recovery);
    let mut module = Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&globals)
        .section(&exports)
        .section(&StartSection {
            function_index: init,
        })
        .section(&code);
    module.finish()
}
fn emit(code: &mut CodeSection, body: Vec<Instruction<'static>>) {
    emit_with_locals(code, [], body);
}
fn emit_with_locals<const N: usize>(
    code: &mut CodeSection,
    locals: [(u32, ValType); N],
    body: Vec<Instruction<'static>>,
) {
    let mut function = Function::new(locals);
    for instruction in body {
        function.instruction(&instruction);
    }
    code.function(&function);
}
// Validate resolved scalar shapes before allocating assembly state.
pub(crate) fn check_scalar(
    resolve: &Resolve,
    function: &wit_parser::Function,
) -> Result<(), Diagnostic> {
    fn scalar(resolve: &Resolve, mut ty: Type) -> bool {
        let mut seen = std::collections::BTreeSet::new();
        while let Type::Id(id) = ty {
            if !seen.insert(id) {
                return false;
            }
            match resolve.types[id].kind {
                TypeDefKind::Type(next) => ty = next,
                _ => return false,
            }
        }
        ty == Type::U32
    }
    if function.kind != FunctionKind::AsyncFreestanding
        || function.params.len() != 1
        || !scalar(resolve, function.params[0].ty)
        || !function.result.is_some_and(|ty| scalar(resolve, ty))
    {
        return Err(diagnostic(
            "Serial async component requires async func(u32) -> u32",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "async_component_entry_tests.rs"]
mod entry_tests;

/// Native host ownership for the generated serial scalar component.
/// A concurrent component trap can poison Wasmtime's task state. This owner
/// preserves the original error and retains the Store without attempting guest
/// reentry or replay. Dispose of the owner explicitly to retire uncertain roots.
#[cfg(not(target_family = "wasm"))]
pub struct ScalarHost<T: Send + 'static> {
    store: wasmtime::Store<T>,
    run: wasmtime::component::TypedFunc<(u32,), (u32,)>,
    quarantined: bool,
}
#[cfg(not(target_family = "wasm"))]
impl<T: Send + 'static> ScalarHost<T> {
    pub fn new(
        mut store: wasmtime::Store<T>,
        instance: wasmtime::component::Instance,
        export: &str,
    ) -> wasmtime::Result<Self> {
        let run = instance.get_typed_func::<(u32,), (u32,)>(&mut store, export)?;
        Ok(Self {
            store,
            run,
            quarantined: false,
        })
    }
    pub fn is_quarantined(&self) -> bool {
        self.quarantined
    }
    pub fn data(&self) -> &T {
        self.store.data()
    }
    /// All errors retain the Store and prevent subsequent calls. In particular,
    /// OutOfFuel is preserved as OutOfFuel; core recovery tests do not authorize
    /// automatic recovery of a trapped concurrent component call.
    pub async fn call(&mut self, value: u32) -> wasmtime::Result<u32> {
        if self.quarantined {
            wasmtime::bail!("scalar component ownership is quarantined; retire this host");
        }
        // Dropping the Rust call future does not cancel a Wasmtime guest task.
        // Latch before suspension so an abandoned call cannot permit reentry.
        self.quarantined = true;
        let run = self.run;
        let result = self
            .store
            .run_concurrent(async |accessor| run.call_concurrent(accessor, (value,)).await)
            .await;
        match result {
            Ok(Ok((value,))) => {
                self.quarantined = false;
                Ok(value)
            }
            Ok(Err(error)) | Err(error) => {
                self.quarantined = true;
                Err(error)
            }
        }
    }
}
