//! Original target assembly for already prepared portable source fragments.
//! This development boundary supports scalar function exports, including interfaces.
use super::{artifact_identity, core_bindings, resolve::Phase, Diagnostic, PreparedFragment};
use crate::runtime_abi;
use suss_reader::Symbol;
use wasm_encoder::*;
use wit_parser::{FunctionKind, Type, WorldItem};
pub use wit_parser::{Resolve, WorldId};

const VALUE: ValType = ValType::Ref(RefType::EQREF);
const HELPERS: u32 = 6;

struct Export {
    name: String,
    global: super::resolve::Global,
    params: Vec<(String, Boundary)>,
    result: Option<Boundary>,
    asynchronous: bool,
}
enum PublicExport {
    Function {
        name: String,
        index: u32,
    },
    Interface {
        name: String,
        implements: Option<String>,
        functions: Vec<(String, u32)>,
    },
}
#[derive(Clone, Copy)]
enum Scalar {
    Bool,
    F32,
    F64,
    U8,
    S8,
    U16,
    S16,
    U32,
    S32,
}
#[derive(Clone, Copy)]
enum Boundary {
    Scalar(Scalar),
    Option(Scalar),
}
impl Boundary {
    fn flat(self) -> Vec<ValType> {
        match self {
            Self::Scalar(ty) => vec![ty.core()],
            Self::Option(ty) => vec![ValType::I32, ty.core()],
        }
    }
    fn core_result(self) -> ValType {
        match self {
            Self::Scalar(ty) => ty.core(),
            Self::Option(_) => ValType::I32,
        }
    }
    fn component(self, types: &mut ComponentTypeSection) -> ComponentValType {
        match self {
            Self::Scalar(ty) => ty.component(),
            Self::Option(ty) => {
                let index = types.len();
                types.defined_type().option(ty.component());
                ComponentValType::Type(index)
            }
        }
    }
    fn optional(self) -> bool {
        matches!(self, Self::Option(_))
    }
}

fn boundary(resolve: &Resolve, mut ty: Type) -> Result<Boundary, Diagnostic> {
    for _ in 0..64 {
        if let Type::Id(id) = ty {
            match &resolve.types[id].kind {
                wit_parser::TypeDefKind::Type(next) => {
                    ty = *next;
                    continue;
                }
                wit_parser::TypeDefKind::Option(payload) => {
                    return scalar(resolve, *payload).map(Boundary::Option);
                }
                _ => {}
            }
        }
        return scalar(resolve, ty).map(Boundary::Scalar);
    }
    Err(error("WIT boundary alias nesting exceeds 64"))
}

impl Scalar {
    fn core(self) -> ValType {
        match self {
            Self::Bool | Self::U8 | Self::S8 | Self::U16 | Self::S16 | Self::U32 | Self::S32 => {
                ValType::I32
            }
            Self::F32 => ValType::F32,
            Self::F64 => ValType::F64,
        }
    }
    fn payload_memory(self) -> MemArg {
        // Canonical option's one-byte discriminant precedes its aligned payload.
        let (offset, align) = match self {
            Self::F64 => (8, 3),
            Self::F32 | Self::U32 | Self::S32 => (4, 2),
            Self::U16 | Self::S16 => (2, 1),
            _ => (1, 0),
        };
        MemArg {
            offset,
            align,
            memory_index: 0,
        }
    }
    fn payload_store(self) -> Instruction<'static> {
        let arg = self.payload_memory();
        match self {
            Self::F64 => Instruction::F64Store(arg),
            Self::F32 => Instruction::F32Store(arg),
            Self::U32 | Self::S32 => Instruction::I32Store(arg),
            Self::U16 | Self::S16 => Instruction::I32Store16(arg),
            _ => Instruction::I32Store8(arg),
        }
    }
    fn payload_load(self) -> Instruction<'static> {
        let arg = self.payload_memory();
        match self {
            Self::F64 => Instruction::F64Load(arg),
            Self::F32 => Instruction::F32Load(arg),
            Self::U32 | Self::S32 => Instruction::I32Load(arg),
            Self::U16 => Instruction::I32Load16U(arg),
            Self::S16 => Instruction::I32Load16S(arg),
            Self::S8 => Instruction::I32Load8S(arg),
            _ => Instruction::I32Load8U(arg),
        }
    }
    fn integer_bounds(self) -> Option<(f64, f64, bool)> {
        Some(match self {
            Self::U8 => (0.0, u8::MAX as f64, false),
            Self::S8 => (i8::MIN as f64, i8::MAX as f64, true),
            Self::U16 => (0.0, u16::MAX as f64, false),
            Self::S16 => (i16::MIN as f64, i16::MAX as f64, true),
            Self::U32 => (0.0, u32::MAX as f64, false),
            Self::S32 => (i32::MIN as f64, i32::MAX as f64, true),
            _ => return None,
        })
    }
    fn component(self) -> ComponentValType {
        PrimitiveValType::from(self).into()
    }
}
impl From<Scalar> for PrimitiveValType {
    fn from(value: Scalar) -> Self {
        match value {
            Scalar::Bool => Self::Bool,
            Scalar::F32 => Self::F32,
            Scalar::F64 => Self::F64,
            Scalar::U8 => Self::U8,
            Scalar::S8 => Self::S8,
            Scalar::U16 => Self::U16,
            Scalar::S16 => Self::S16,
            Scalar::U32 => Self::U32,
            Scalar::S32 => Self::S32,
        }
    }
}
fn boundary_description(resolve: &Resolve, ty: Type, depth: usize) -> String {
    if depth >= 64 {
        return "type nesting exceeds 64".into();
    }
    match ty {
        Type::Id(id) => match &resolve.types[id].kind {
            wit_parser::TypeDefKind::Type(next) => boundary_description(resolve, *next, depth + 1),
            wit_parser::TypeDefKind::List(next) => {
                format!("list<{}>", boundary_description(resolve, *next, depth + 1))
            }
            wit_parser::TypeDefKind::Map(key, value) => format!(
                "map<{}, {}>",
                boundary_description(resolve, *key, depth + 1),
                boundary_description(resolve, *value, depth + 1)
            ),
            kind => kind.as_str().into(),
        },
        Type::ErrorContext => "error-context".into(),
        other => format!("{other:?}").to_ascii_lowercase(),
    }
}

fn scalar(resolve: &Resolve, mut ty: Type) -> Result<Scalar, Diagnostic> {
    for _ in 0..64 {
        return match ty {
            Type::Bool => Ok(Scalar::Bool),
            Type::F32 => Ok(Scalar::F32),
            Type::F64 => Ok(Scalar::F64),
            Type::U8 => Ok(Scalar::U8),
            Type::S8 => Ok(Scalar::S8),
            Type::U16 => Ok(Scalar::U16),
            Type::S16 => Ok(Scalar::S16),
            Type::U32 => Ok(Scalar::U32),
            Type::S32 => Ok(Scalar::S32),
            Type::Id(id) => match &resolve.types[id].kind {
                wit_parser::TypeDefKind::Type(next) => {
                    ty = *next;
                    continue;
                }
                _ => Err(error(format!(
                    "Portable AOT {} boundary adapters remain unimplemented",
                    boundary_description(resolve, ty, 0)
                ))),
            },
            _ => Err(error(format!(
                "Portable AOT {} boundary adapters remain unimplemented; supported scalar types are bool/f32/f64 and small integers",
                boundary_description(resolve, ty, 0)
            ))),
        };
    }
    Err(error("WIT scalar alias nesting exceeds 64"))
}

/// Check only implemented WIT adapter capabilities, before source macro effects.
/// Source mappings, code generation and artifact validation have separate errors.
pub(crate) fn validate_boundary(resolve: &Resolve, world: WorldId) -> Result<(), Diagnostic> {
    let selected = &resolve.worlds[world];
    if !selected.imports.is_empty() {
        return Err(error(
            "Portable AOT imported WIT adapters remain unimplemented",
        ));
    }
    let validate_function = |function: &wit_parser::Function| -> Result<(), Diagnostic> {
        if function.external_id.is_some() {
            return Err(error(
                "Portable AOT function external-id adapters remain unimplemented",
            ));
        }
        if !matches!(
            function.kind,
            FunctionKind::Freestanding | FunctionKind::AsyncFreestanding
        ) {
            return Err(error(
                "Portable AOT resource function adapters remain unimplemented",
            ));
        }
        let mut flat_parameters = 0;
        for parameter in &function.params {
            let ty = boundary(resolve, parameter.ty)?;
            flat_parameters += ty.flat().len();
        }
        // Canonical lifted calls switch above sixteen flattened
        // parameters to an indirect argument area. Diagnose that missing path
        // before Macro effects rather than emitting an invalid core signature.
        if flat_parameters > 16 {
            return Err(error(
                "Portable AOT indirect parameter adapters remain unimplemented",
            ));
        }
        if let Some(result) = function.result {
            boundary(resolve, result)?;
        }
        Ok(())
    };
    for item in selected.exports.values() {
        match item {
            WorldItem::Function(function) => validate_function(function)?,
            WorldItem::Interface {
                id, external_id, ..
            } => {
                if external_id.is_some() {
                    return Err(error(
                        "Portable AOT interface external-id adapters remain unimplemented",
                    ));
                }
                let interface = &resolve.interfaces[*id];
                if !interface.types.is_empty() {
                    return Err(error(
                        "Portable AOT interface type exports remain unimplemented",
                    ));
                }
                for function in interface.functions.values() {
                    validate_function(function)?;
                }
            }
            WorldItem::Type { .. } => {
                return Err(error("Portable AOT type exports remain unimplemented"));
            }
        }
    }
    Ok(())
}

/// Fill missing selected-world mappings from retained source declaration export
/// facts for freestanding functions only (accepted design section 10). Interface
/// functions require explicit mappings; assembly still validates every mapping.
pub fn source_export_mappings(
    fragments: &[PreparedFragment],
    resolve: &Resolve,
    world: WorldId,
    explicit: &[(String, Symbol)],
) -> Result<Vec<(String, Symbol)>, Diagnostic> {
    use suss_reader::forms::Kind;
    let last = fragments
        .last()
        .ok_or_else(|| error("AOT needs a prepared source fragment"))?;
    let selected = &resolve.worlds[world];
    if !selected.imports.is_empty() {
        return Ok(explicit.to_vec());
    }
    let mut missing = Vec::new();
    for (key, item) in &selected.exports {
        let name = resolve.name_world_key(key);
        match item {
            WorldItem::Function(function) => missing.push((name, function.name.clone())),
            WorldItem::Interface { .. } | WorldItem::Type { .. } => {}
        }
    }
    missing.retain(|(path, _)| !explicit.iter().any(|(mapped, _)| mapped == path));
    let explicit_vars = explicit
        .iter()
        .map(|(_, symbol)| {
            last.environment
                .resolve(Phase::Runtime, symbol, 0..0)
                .map(|binding| binding.global().clone())
        })
        .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
    let mut candidates = std::collections::BTreeMap::<String, Vec<Symbol>>::new();
    for global in last.environment.cells() {
        if missing.is_empty() {
            break;
        }
        if global.phase() != Phase::Runtime {
            continue;
        }
        // Explicitly selected vars do not participate in shorthand inference.
        // Their reader markers may be unsupported by WIT selection; the explicit
        // path still receives all of the assembler's normal validation.
        if explicit_vars.contains(&global) {
            continue;
        }
        let Some(info) = last.environment.definition_info(&global) else {
            continue;
        };
        let Some(export) = info.export_as(&global)? else {
            continue;
        };
        let (name, exact) = match &export.kind {
            Kind::Symbol(symbol) => {
                let source_namespace = if global.namespace() == "suss.core" {
                    "cljs.core"
                } else {
                    global.namespace()
                };
                if symbol.namespace.is_some()
                    && (symbol.namespace.as_deref() != Some(source_namespace)
                        || symbol.name != global.name())
                {
                    return Err(error(format!(
                        "Qualified WIT export metadata for {} needs an explicit mapping",
                        global.import_name()
                    )));
                }
                (symbol.name.clone(), false)
            }
            Kind::String(units) => {
                let name = String::from_utf16(units).map_err(|_| {
                    error(format!(
                        "WIT export metadata for {} contains an invalid Unicode string",
                        global.import_name()
                    ))
                })?;
                if name.is_empty() {
                    return Err(error("WIT export metadata needs a nonempty name"));
                }
                let exact = name.contains('#');
                (name, exact)
            }
            _ => {
                return Err(error(format!(
                    "Unsupported WIT export metadata for {}; use an explicit export mapping",
                    global.import_name()
                )));
            }
        };
        let matched = missing
            .iter()
            .filter(|(path, short)| if exact { *path == name } else { *short == name })
            .collect::<Vec<_>>();
        if matched.len() > 1 {
            return Err(error(format!(
                "Ambiguous WIT export metadata for {} matches {}; use explicit export mappings",
                global.import_name(),
                matched
                    .iter()
                    .map(|(path, _)| path.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        if let Some((path, _)) = matched.first() {
            candidates.entry(path.clone()).or_default().push(Symbol {
                namespace: Some(global.namespace().to_owned()),
                name: global.name().to_owned(),
            });
        }
    }
    let mut mappings = explicit.to_vec();
    for (path, vars) in candidates {
        if vars.len() != 1 {
            return Err(error(format!(
                "Ambiguous WIT export mapping for {path}; use an explicit export mapping"
            )));
        }
        mappings.push((path, vars.into_iter().next().unwrap()));
    }
    Ok(mappings)
}

/// Assemble prepared source in order, with explicit WIT export-to-var mappings.
/// Source compilation and macro execution use the ordinary portable pipeline;
/// this function never evaluates user source in the compiler host.
pub fn component(
    fragments: &[PreparedFragment],
    resolve: &Resolve,
    world: WorldId,
    mappings: &[(String, Symbol)],
) -> Result<Vec<u8>, Diagnostic> {
    validate_boundary(resolve, world)?;
    let last = fragments
        .last()
        .ok_or_else(|| error("AOT needs a prepared source fragment"))?;
    let selected = &resolve.worlds[world];
    let mut exports = Vec::new();
    let mut public_exports = Vec::new();
    let mut used = std::collections::BTreeSet::new();
    let mut add_function =
        |name: String, function: &wit_parser::Function| -> Result<u32, Diagnostic> {
            let matching = mappings
                .iter()
                .filter(|(export, _)| export == &name)
                .collect::<Vec<_>>();
            if matching.len() != 1 {
                return Err(error(format!(
                    "WIT export {name} needs exactly one explicit Suss var mapping"
                )));
            }
            used.insert(name.clone());
            let global = last
                .environment
                .resolve(Phase::Runtime, &matching[0].1, 0..0)?
                .global()
                .clone();
            let index = exports.len() as u32;
            exports.push(Export {
                name,
                global,
                params: function
                    .params
                    .iter()
                    .map(|param| Ok((param.name.clone(), boundary(resolve, param.ty)?)))
                    .collect::<Result<_, Diagnostic>>()?,
                result: function
                    .result
                    .map(|ty| boundary(resolve, ty))
                    .transpose()?,
                asynchronous: function.kind == FunctionKind::AsyncFreestanding,
            });
            Ok(index)
        };
    for (key, item) in &selected.exports {
        // Upstream resolution supplies the exact selected alias or package /
        // interface / version; never reconstruct it from unqualified names.
        let name = resolve.name_world_key(key);
        match item {
            WorldItem::Function(function) => {
                let index = add_function(name.clone(), function)?;
                public_exports.push(PublicExport::Function { name, index });
            }
            WorldItem::Interface { id, .. } => {
                let interface = &resolve.interfaces[*id];
                let mut functions = Vec::new();
                for (function_name, function) in &interface.functions {
                    let index = add_function(format!("{name}#{function_name}"), function)?;
                    functions.push((function_name.clone(), index));
                }
                public_exports.push(PublicExport::Interface {
                    name,
                    implements: resolve.implements_value(key, item),
                    functions,
                });
            }
            WorldItem::Type { .. } => {
                return Err(error("Portable AOT type exports remain unimplemented"));
            }
        }
    }
    if mappings.iter().any(|(name, _)| !used.contains(name)) {
        return Err(error("Unknown WIT export mapping"));
    }
    // Reject every artifact before assembling any initialization start function.
    for fragment in fragments {
        runtime_abi::verify_artifact(&fragment.wasm, &runtime_abi::Manifest::default())
            .map_err(error)?;
        artifact_identity::verify(
            &fragment.wasm,
            artifact_identity::Expected {
                phase: Some(Phase::Runtime),
                ..Default::default()
            },
        )
        .map_err(error)?;
    }
    let cells = fragments
        .iter()
        .flat_map(|fragment| fragment.cells.iter().cloned())
        .collect::<Vec<_>>();
    let bindings = core_bindings::compile_with_cells(Phase::Runtime, &cells)?;
    let adapter = adapter(&exports, fragments.len())?;
    let asynchronous = exports
        .iter()
        .filter(|export| export.asynchronous)
        .collect::<Vec<_>>();
    let mut component = Component::new();
    let modules = std::iter::once(runtime_abi::module())
        .chain(std::iter::once(bindings.wasm))
        .chain(fragments.iter().map(|fragment| fragment.wasm.clone()))
        .chain(std::iter::once(adapter));
    for module in modules {
        component.section(&RawSection {
            id: ComponentSectionId::CoreModule.into(),
            data: &module,
        });
    }
    if !asynchronous.is_empty() {
        let bridge = async_adapter(&asynchronous);
        component.section(&RawSection {
            id: ComponentSectionId::CoreModule.into(),
            data: &bridge,
        });
    }
    let mut instances = InstanceSection::new();
    instances.instantiate(0, std::iter::empty::<(&str, ModuleArg)>());
    instances.instantiate(1, [("suss.runtime", ModuleArg::Instance(0))]);
    for index in 0..fragments.len() {
        instances.instantiate(
            2 + index as u32,
            [
                ("suss.runtime", ModuleArg::Instance(0)),
                ("suss.bindings.runtime", ModuleArg::Instance(1)),
            ],
        );
    }
    let mut args = vec![
        ("suss.runtime".to_owned(), ModuleArg::Instance(0)),
        ("suss.bindings.runtime".to_owned(), ModuleArg::Instance(1)),
    ];
    args.extend((0..fragments.len()).map(|index| {
        (
            format!("suss.fragment.{index}"),
            ModuleArg::Instance(2 + index as u32),
        )
    }));
    let adapter_instance = 2 + fragments.len() as u32;
    instances.instantiate(adapter_instance, args);
    component.section(&instances);
    let mut types = ComponentTypeSection::new();
    let mut function_types = Vec::new();
    let mut result_types = Vec::new();
    for export in &exports {
        let params = export
            .params
            .iter()
            .map(|(name, ty)| (name.as_str(), ty.component(&mut types)))
            .collect::<Vec<_>>();
        let result = export.result.map(|ty| ty.component(&mut types));
        function_types.push(types.len());
        result_types.push(result);
        types
            .function()
            .async_(export.asynchronous)
            .params(params)
            .result(result);
    }
    component.section(&types);
    // Task-return builtins precede bridge instantiation, so the bridge can call
    // the canonical completion operation directly without a mutable shim.
    let mut completion = CanonicalFunctionSection::new();
    for (index, export) in exports.iter().enumerate() {
        if export.asynchronous {
            // task.return takes the flattened result as parameters; a scalar
            // option is a tag and payload, not the adapter's return pointer.
            completion.task_return(result_types[index], []);
        }
    }
    let bridge_instance = if asynchronous.is_empty() {
        None
    } else {
        component.section(&completion);
        let mut bridge_instances = InstanceSection::new();
        bridge_instances.export_items(
            asynchronous
                .iter()
                .enumerate()
                .map(|(index, export)| (export.name.as_str(), ExportKind::Func, index as u32)),
        );
        bridge_instances.instantiate(
            3 + fragments.len() as u32,
            [
                ("suss.aot", ModuleArg::Instance(adapter_instance)),
                ("suss.completion", ModuleArg::Instance(adapter_instance + 1)),
            ],
        );
        component.section(&bridge_instances);
        Some(adapter_instance + 2)
    };
    let mut aliases = ComponentAliasSection::new();
    let mut canonical = CanonicalFunctionSection::new();
    let mut public = ComponentExportSection::new();
    let mut exported_instances = ComponentInstanceSection::new();
    let mut core_function = asynchronous.len() as u32;
    let callback = bridge_instance.map(|instance| {
        aliases.alias(Alias::CoreInstanceExport {
            instance,
            kind: ExportKind::Func,
            name: "callback",
        });
        let index = core_function;
        core_function += 1;
        index
    });
    if exports
        .iter()
        .any(|export| export.result.is_some_and(Boundary::optional))
    {
        aliases.alias(Alias::CoreInstanceExport {
            instance: adapter_instance,
            kind: ExportKind::Memory,
            name: "suss.canonical.memory",
        });
    }
    let mut asynchronous_index = 0;
    for (index, export) in exports.iter().enumerate() {
        let entry_name = if export.asynchronous {
            let name = format!("entry.{asynchronous_index}");
            asynchronous_index += 1;
            name
        } else {
            export.name.clone()
        };
        aliases.alias(Alias::CoreInstanceExport {
            instance: if export.asynchronous {
                bridge_instance.unwrap()
            } else {
                adapter_instance
            },
            kind: ExportKind::Func,
            name: &entry_name,
        });
        let options = if export.asynchronous {
            vec![
                CanonicalOption::Async,
                CanonicalOption::Callback(callback.unwrap()),
            ]
        } else if export.result.is_some_and(Boundary::optional) {
            vec![CanonicalOption::Memory(0)]
        } else {
            vec![]
        };
        canonical.lift(core_function, function_types[index], options);
        core_function += 1;
    }
    for export in public_exports {
        match export {
            PublicExport::Function { name, index } => {
                public.export(&name, ComponentExportKind::Func, index, None);
            }
            PublicExport::Interface {
                name,
                implements,
                functions,
            } => {
                let index = exported_instances.len();
                exported_instances.export_items(
                    functions.iter().map(|(name, function)| {
                        (name.as_str(), ComponentExportKind::Func, *function)
                    }),
                );
                public.export(
                    ComponentExternName {
                        name: name.as_str().into(),
                        implements: implements.as_deref().map(Into::into),
                        version_suffix: None,
                        external_id: None,
                    },
                    ComponentExportKind::Instance,
                    index,
                    None,
                );
            }
        }
    }
    component
        .section(&aliases)
        .section(&canonical)
        .section(&exported_instances)
        .section(&public);
    let bytes = component.finish();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&bytes)
        .map_err(|failure| error(format!("Invalid portable AOT component: {failure}")))?;
    Ok(bytes)
}

/// Canonical callback entrypoints for non-suspending source functions. They
/// complete the task exactly once, then report EXIT. No continuation exists:
/// suspension points must use the future rooted-continuation lowering instead.
fn async_adapter(exports: &[&Export]) -> Vec<u8> {
    let mut types = TypeSection::new();
    let mut imports = ImportSection::new();
    for (index, export) in exports.iter().enumerate() {
        types.ty().function(
            export
                .params
                .iter()
                .flat_map(|(_, ty)| ty.flat())
                .collect::<Vec<_>>(),
            export.result.map(Boundary::core_result),
        );
        types.ty().function(
            export
                .result
                .into_iter()
                .flat_map(Boundary::flat)
                .collect::<Vec<_>>(),
            [],
        );
        imports.import(
            "suss.aot",
            &export.name,
            EntityType::Function(2 * index as u32),
        );
        imports.import(
            "suss.completion",
            &export.name,
            EntityType::Function(2 * index as u32 + 1),
        );
    }
    if exports
        .iter()
        .any(|export| export.result.is_some_and(Boundary::optional))
    {
        imports.import(
            "suss.aot",
            "suss.canonical.memory",
            EntityType::Memory(MemoryType {
                minimum: 1,
                maximum: Some(1),
                memory64: false,
                shared: false,
                page_size_log2: None,
            }),
        );
    }
    let imported = 2 * exports.len() as u32;
    let mut functions = FunctionSection::new();
    let mut public = ExportSection::new();
    let mut code = CodeSection::new();
    for (index, export) in exports.iter().enumerate() {
        types.ty().function(
            export
                .params
                .iter()
                .flat_map(|(_, ty)| ty.flat())
                .collect::<Vec<_>>(),
            [ValType::I32],
        );
        functions.function(imported + index as u32);
        public.export(
            &format!("entry.{index}"),
            ExportKind::Func,
            imported + index as u32,
        );
        let flat_params = export
            .params
            .iter()
            .map(|(_, ty)| ty.flat().len())
            .sum::<usize>();
        let mut body = Function::new([(1, ValType::I32)]);
        for parameter in 0..flat_params {
            body.instruction(&Instruction::LocalGet(parameter as u32));
        }
        body.instruction(&Instruction::Call(2 * index as u32));
        if let Some(Boundary::Option(ty)) = export.result {
            body.instruction(&Instruction::LocalSet(flat_params as u32))
                .instruction(&Instruction::LocalGet(flat_params as u32))
                .instruction(&Instruction::I32Load8U(MemArg {
                    offset: 0,
                    align: 0,
                    memory_index: 0,
                }))
                .instruction(&Instruction::LocalGet(flat_params as u32))
                .instruction(&ty.payload_load());
        }
        body.instruction(&Instruction::Call(2 * index as u32 + 1))
            .instruction(&Instruction::I32Const(0))
            .instruction(&Instruction::End);
        code.function(&body);
    }
    let callback_type = imported + exports.len() as u32;
    types.ty().function([ValType::I32; 3], [ValType::I32]);
    functions.function(callback_type);
    public.export("callback", ExportKind::Func, callback_type);
    let mut callback = Function::new([]);
    callback
        .instruction(&Instruction::Unreachable)
        .instruction(&Instruction::End);
    code.function(&callback);
    let mut module = Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&public)
        .section(&code);
    module.finish()
}

fn adapter(exports: &[Export], fragment_count: usize) -> Result<Vec<u8>, Diagnostic> {
    let mut types = runtime_abi::prelude();
    let mut imports = ImportSection::new();
    let signatures = [
        ("binding-get", vec![VALUE], vec![VALUE]),
        ("number-box", vec![ValType::F64], vec![VALUE]),
        ("invoke", vec![VALUE, VALUE], vec![VALUE]),
        ("language-error-new", vec![VALUE], vec![VALUE]),
        ("string-new", vec![ValType::I32], vec![VALUE]),
        (
            "string-set-unit",
            vec![VALUE, ValType::I32, ValType::I32],
            vec![ValType::I32],
        ),
    ];
    for (index, (name, params, results)) in signatures.into_iter().enumerate() {
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
            func_type_idx: runtime_abi::TYPE_COUNT + HELPERS,
        }),
    );
    types.ty().function([], [VALUE]);
    for index in 0..fragment_count {
        imports.import(
            &format!("suss.fragment.{index}"),
            "eval",
            EntityType::Function(runtime_abi::TYPE_COUNT + HELPERS + 1),
        );
    }
    // Components forbid duplicate module/name imports. Multiple WIT exports
    // may intentionally map to one live source var, so share only its import.
    let mut global_indices = std::collections::BTreeMap::new();
    for export in exports {
        if !global_indices.contains_key(&export.global) {
            let index = global_indices.len() as u32;
            imports.import(
                export.global.import_module(),
                &export.global.import_name(),
                EntityType::Global(GlobalType {
                    val_type: runtime_abi::binding_cell_type(),
                    mutable: false,
                    shared: false,
                }),
            );
            global_indices.insert(export.global.clone(), index);
        }
    }
    let imported = HELPERS + fragment_count as u32;
    let mut functions = FunctionSection::new();
    let mut public = ExportSection::new();
    let mut code = CodeSection::new();
    for (index, export) in exports.iter().enumerate() {
        let ty = runtime_abi::TYPE_COUNT + HELPERS + 2 + index as u32;
        types.ty().function(
            export
                .params
                .iter()
                .flat_map(|(_, ty)| ty.flat())
                .collect::<Vec<_>>(),
            export.result.map(Boundary::core_result),
        );
        functions.function(ty);
        public.export(&export.name, ExportKind::Func, imported + index as u32);
        let result = export
            .params
            .iter()
            .map(|(_, ty)| ty.flat().len() as u32)
            .sum::<u32>();
        let integer = result + 1;
        let number = integer + 1;
        let mut body = Function::new([(1, VALUE), (1, ValType::I32), (1, ValType::F64)]);
        body.instruction(&Instruction::GlobalGet(global_indices[&export.global]))
            .instruction(&Instruction::Call(0));
        let mut parameter = 0;
        for (_, ty) in &export.params {
            match ty {
                Boundary::Scalar(ty) => {
                    body.instruction(&Instruction::LocalGet(parameter));
                    lift_scalar(&mut body, ty);
                    parameter += 1;
                }
                Boundary::Option(ty) => {
                    body.instruction(&Instruction::LocalGet(parameter))
                        .instruction(&Instruction::If(BlockType::Result(VALUE)))
                        .instruction(&Instruction::LocalGet(parameter + 1));
                    lift_scalar(&mut body, ty);
                    body.instruction(&Instruction::Else)
                        .instruction(&Instruction::I32Const(0))
                        .instruction(&Instruction::RefI31)
                        .instruction(&Instruction::End);
                    parameter += 2;
                }
            }
        }
        body.instruction(&Instruction::ArrayNewFixed {
            array_type_index: runtime_abi::ARGS,
            array_size: export.params.len() as u32,
        })
        .instruction(&Instruction::Call(2))
        .instruction(&Instruction::LocalSet(result));
        match export.result {
            None => {}
            Some(Boundary::Scalar(ty)) => {
                lower_scalar(&mut body, ty, result, integer, number, &export.name)
            }
            Some(Boundary::Option(ty)) => {
                // Scalar option returns use a fixed canonical return area. No
                // allocation, ownership transfer or post-return free is needed.
                body.instruction(&Instruction::LocalGet(result))
                    .instruction(&Instruction::RefTestNonNull(HeapType::I31))
                    .instruction(&Instruction::If(BlockType::Result(ValType::I32)))
                    .instruction(&Instruction::LocalGet(result))
                    .instruction(&Instruction::RefCastNonNull(HeapType::I31))
                    .instruction(&Instruction::I31GetS)
                    .instruction(&Instruction::I32Eqz)
                    .instruction(&Instruction::Else)
                    .instruction(&Instruction::I32Const(0))
                    .instruction(&Instruction::End)
                    .instruction(&Instruction::If(BlockType::Empty))
                    .instruction(&Instruction::I32Const(0))
                    .instruction(&Instruction::I32Const(0))
                    .instruction(&Instruction::I32Store8(MemArg {
                        offset: 0,
                        align: 0,
                        memory_index: 0,
                    }))
                    .instruction(&Instruction::Else)
                    .instruction(&Instruction::I32Const(0))
                    .instruction(&Instruction::I32Const(1))
                    .instruction(&Instruction::I32Store8(MemArg {
                        offset: 0,
                        align: 0,
                        memory_index: 0,
                    }))
                    .instruction(&Instruction::I32Const(0));
                lower_scalar(&mut body, ty, result, integer, number, &export.name);
                let store = ty.payload_store();
                body.instruction(&store)
                    .instruction(&Instruction::End)
                    .instruction(&Instruction::I32Const(0));
            }
        }
        body.instruction(&Instruction::End);
        code.function(&body);
    }
    let start_type = runtime_abi::TYPE_COUNT + HELPERS + 2 + exports.len() as u32;
    types.ty().function([], []);
    functions.function(start_type);
    let mut start = Function::new([]);
    for index in 0..fragment_count {
        start
            .instruction(&Instruction::Call(HELPERS + index as u32))
            .instruction(&Instruction::Drop);
    }
    start.instruction(&Instruction::End);
    code.function(&start);
    let mut module = Module::new();
    module
        .section(&runtime_abi::Manifest::default().section())
        .section(&types)
        .section(&imports)
        .section(&functions);
    if exports
        .iter()
        .any(|export| export.result.is_some_and(Boundary::optional))
    {
        let mut memory = MemorySection::new();
        memory.memory(MemoryType {
            minimum: 1,
            maximum: Some(1),
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        module.section(&memory);
        public.export("suss.canonical.memory", ExportKind::Memory, 0);
    }
    module
        .section(&public)
        .section(&StartSection {
            function_index: imported + exports.len() as u32,
        })
        .section(&code);
    Ok(module.finish())
}
fn lift_scalar(body: &mut Function, ty: &Scalar) {
    match ty {
        Scalar::Bool => {
            body.instruction(&Instruction::I32Const(2))
                .instruction(&Instruction::I32Mul)
                .instruction(&Instruction::I32Const(2))
                .instruction(&Instruction::I32Add)
                .instruction(&Instruction::RefI31);
        }
        Scalar::F32 => {
            body.instruction(&Instruction::F64PromoteF32)
                .instruction(&Instruction::Call(1));
        }
        Scalar::F64 => {
            body.instruction(&Instruction::Call(1));
        }
        Scalar::U8 | Scalar::S8 | Scalar::U16 | Scalar::S16 | Scalar::U32 | Scalar::S32 => {
            // Normalize canonical subword bits before widening to the
            // ordinary binary64 language number; u32 must stay unsigned.
            match ty {
                Scalar::U8 => {
                    body.instruction(&Instruction::I32Const(255))
                        .instruction(&Instruction::I32And);
                }
                Scalar::S8 => {
                    body.instruction(&Instruction::I32Extend8S);
                }
                Scalar::U16 => {
                    body.instruction(&Instruction::I32Const(65535))
                        .instruction(&Instruction::I32And);
                }
                Scalar::S16 => {
                    body.instruction(&Instruction::I32Extend16S);
                }
                _ => {}
            }
            body.instruction(&if ty.integer_bounds().unwrap().2 {
                Instruction::F64ConvertI32S
            } else {
                Instruction::F64ConvertI32U
            })
            .instruction(&Instruction::Call(1));
        }
    }
}

fn lower_scalar(
    body: &mut Function,
    scalar: Scalar,
    result: u32,
    integer: u32,
    number: u32,
    name: &str,
) {
    match scalar {
        Scalar::Bool => {
            body.instruction(&Instruction::LocalGet(result))
                .instruction(&Instruction::RefTestNonNull(HeapType::I31))
                .instruction(&Instruction::I32Eqz)
                .instruction(&Instruction::If(BlockType::Empty));
            boundary_error(body, result, name);
            body.instruction(&Instruction::End)
                .instruction(&Instruction::LocalGet(result))
                .instruction(&Instruction::RefCastNonNull(HeapType::I31))
                .instruction(&Instruction::I31GetS)
                .instruction(&Instruction::LocalTee(integer))
                .instruction(&Instruction::I32Const(2))
                .instruction(&Instruction::I32Eq)
                .instruction(&Instruction::LocalGet(integer))
                .instruction(&Instruction::I32Const(4))
                .instruction(&Instruction::I32Eq)
                .instruction(&Instruction::I32Or)
                .instruction(&Instruction::I32Eqz)
                .instruction(&Instruction::If(BlockType::Empty));
            boundary_error(body, result, name);
            body.instruction(&Instruction::End)
                .instruction(&Instruction::LocalGet(integer))
                .instruction(&Instruction::I32Const(4))
                .instruction(&Instruction::I32Eq);
        }
        scalar => {
            body.instruction(&Instruction::LocalGet(result))
                .instruction(&Instruction::RefTestNonNull(HeapType::Concrete(
                    runtime_abi::NUMBER,
                )))
                .instruction(&Instruction::I32Eqz)
                .instruction(&Instruction::If(BlockType::Empty));
            boundary_error(body, result, name);
            body.instruction(&Instruction::End)
                .instruction(&Instruction::LocalGet(result))
                .instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
                    runtime_abi::NUMBER,
                )))
                .instruction(&Instruction::StructGet {
                    struct_type_index: runtime_abi::NUMBER,
                    field_index: 0,
                });
            if let Some((min, max, signed)) = scalar.integer_bounds() {
                // Validate before truncation: NaN fails the integral test,
                // infinities fail bounds, and no invalid result reaches a
                // trapping numeric conversion instead of a language error.
                body.instruction(&Instruction::LocalSet(number))
                    .instruction(&Instruction::LocalGet(number))
                    .instruction(&Instruction::F64Const(min.into()))
                    .instruction(&Instruction::F64Lt)
                    .instruction(&Instruction::LocalGet(number))
                    .instruction(&Instruction::F64Const(max.into()))
                    .instruction(&Instruction::F64Gt)
                    .instruction(&Instruction::I32Or)
                    .instruction(&Instruction::LocalGet(number))
                    .instruction(&Instruction::LocalGet(number))
                    .instruction(&Instruction::F64Trunc)
                    .instruction(&Instruction::F64Ne)
                    .instruction(&Instruction::I32Or)
                    .instruction(&Instruction::If(BlockType::Empty));
                boundary_error(body, result, name);
                body.instruction(&Instruction::End)
                    .instruction(&Instruction::LocalGet(number))
                    .instruction(&if signed {
                        Instruction::I32TruncF64S
                    } else {
                        Instruction::I32TruncF64U
                    });
            } else if matches!(scalar, Scalar::F32) {
                body.instruction(&Instruction::F32DemoteF64);
            }
        }
    }
}

fn boundary_error(body: &mut Function, local: u32, name: &str) {
    let message = format!("WIT export {name} returned an incompatible scalar value")
        .encode_utf16()
        .collect::<Vec<_>>();
    body.instruction(&Instruction::I32Const(message.len() as i32))
        .instruction(&Instruction::Call(4))
        .instruction(&Instruction::LocalSet(local));
    for (index, unit) in message.into_iter().enumerate() {
        body.instruction(&Instruction::LocalGet(local))
            .instruction(&Instruction::I32Const(index as i32))
            .instruction(&Instruction::I32Const(unit as i32))
            .instruction(&Instruction::Call(5))
            .instruction(&Instruction::Drop);
    }
    body.instruction(&Instruction::LocalGet(local))
        .instruction(&Instruction::Call(3))
        .instruction(&Instruction::Throw(0));
}
fn error(message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        span: 0..0,
        message: message.into(),
    }
}
