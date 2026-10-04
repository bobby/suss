//! Original target assembly for already prepared portable source fragments.
//! This development boundary supports scalar function exports, including interfaces.
use super::{Diagnostic, PreparedFragment, artifact_identity, core_bindings, resolve::Phase};
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
    params: Vec<(String, Scalar)>,
    result: Option<Scalar>,
}
enum PublicExport {
    Function {
        name: String,
        index: u32,
    },
    Interface {
        name: String,
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
                _ => Err(error(
                    "Portable AOT composite boundary adapters remain unimplemented",
                )),
            },
            _ => Err(error(
                "Portable AOT adapter currently supports bool/f32/f64 and small integers only",
            )),
        };
    }
    Err(error("WIT scalar alias nesting exceeds 64"))
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
    let last = fragments
        .last()
        .ok_or_else(|| error("AOT needs a prepared source fragment"))?;
    let selected = &resolve.worlds[world];
    if !selected.imports.is_empty() {
        return Err(error(
            "Portable AOT imported WIT adapters remain unimplemented",
        ));
    }
    let mut exports = Vec::new();
    let mut public_exports = Vec::new();
    let mut used = std::collections::BTreeSet::new();
    let mut add_function =
        |name: String, function: &wit_parser::Function| -> Result<u32, Diagnostic> {
            if function.kind != FunctionKind::Freestanding {
                return Err(error(
                    "Portable AOT async/resource function adapters remain unimplemented",
                ));
            }
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
                    .map(|param| Ok((param.name.clone(), scalar(resolve, param.ty)?)))
                    .collect::<Result<_, Diagnostic>>()?,
                result: function.result.map(|ty| scalar(resolve, ty)).transpose()?,
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
                let mut functions = Vec::new();
                for (function_name, function) in &interface.functions {
                    let index = add_function(format!("{name}#{function_name}"), function)?;
                    functions.push((function_name.clone(), index));
                }
                public_exports.push(PublicExport::Interface { name, functions });
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
    let mut aliases = ComponentAliasSection::new();
    let mut types = ComponentTypeSection::new();
    let mut canonical = CanonicalFunctionSection::new();
    let mut public = ComponentExportSection::new();
    let mut exported_instances = ComponentInstanceSection::new();
    for (index, export) in exports.iter().enumerate() {
        aliases.alias(Alias::CoreInstanceExport {
            instance: adapter_instance,
            kind: ExportKind::Func,
            name: &export.name,
        });
        types
            .function()
            .params(
                export
                    .params
                    .iter()
                    .map(|(name, ty)| (name.as_str(), ty.component())),
            )
            .result(export.result.map(Scalar::component));
        canonical.lift(
            index as u32,
            index as u32,
            std::iter::empty::<CanonicalOption>(),
        );
    }
    for export in public_exports {
        match export {
            PublicExport::Function { name, index } => {
                public.export(&name, ComponentExportKind::Func, index, None);
            }
            PublicExport::Interface { name, functions } => {
                let index = exported_instances.len();
                exported_instances.export_items(
                    functions.iter().map(|(name, function)| {
                        (name.as_str(), ComponentExportKind::Func, *function)
                    }),
                );
                public.export(&name, ComponentExportKind::Instance, index, None);
            }
        }
    }
    component
        .section(&aliases)
        .section(&types)
        .section(&canonical)
        .section(&exported_instances)
        .section(&public);
    let bytes = component.finish();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&bytes)
        .map_err(|failure| error(format!("Invalid portable AOT component: {failure}")))?;
    Ok(bytes)
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
            export.params.iter().map(|(_, ty)| ty.core()),
            export.result.map(Scalar::core),
        );
        functions.function(ty);
        public.export(&export.name, ExportKind::Func, imported + index as u32);
        let result = export.params.len() as u32;
        let integer = result + 1;
        let number = integer + 1;
        let mut body = Function::new([(1, VALUE), (1, ValType::I32), (1, ValType::F64)]);
        body.instruction(&Instruction::GlobalGet(global_indices[&export.global]))
            .instruction(&Instruction::Call(0));
        for (parameter, (_, ty)) in export.params.iter().enumerate() {
            body.instruction(&Instruction::LocalGet(parameter as u32));
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
        body.instruction(&Instruction::ArrayNewFixed {
            array_type_index: runtime_abi::ARGS,
            array_size: export.params.len() as u32,
        })
        .instruction(&Instruction::Call(2))
        .instruction(&Instruction::LocalSet(result));
        match export.result {
            None => {}
            Some(Scalar::Bool) => {
                body.instruction(&Instruction::LocalGet(result))
                    .instruction(&Instruction::RefTestNonNull(HeapType::I31))
                    .instruction(&Instruction::I32Eqz)
                    .instruction(&Instruction::If(BlockType::Empty));
                boundary_error(&mut body, result, &export.name);
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
                boundary_error(&mut body, result, &export.name);
                body.instruction(&Instruction::End)
                    .instruction(&Instruction::LocalGet(integer))
                    .instruction(&Instruction::I32Const(4))
                    .instruction(&Instruction::I32Eq);
            }
            Some(scalar) => {
                body.instruction(&Instruction::LocalGet(result))
                    .instruction(&Instruction::RefTestNonNull(HeapType::Concrete(
                        runtime_abi::NUMBER,
                    )))
                    .instruction(&Instruction::I32Eqz)
                    .instruction(&Instruction::If(BlockType::Empty));
                boundary_error(&mut body, result, &export.name);
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
                    boundary_error(&mut body, result, &export.name);
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
        .section(&functions)
        .section(&public)
        .section(&StartSection {
            function_index: imported + exports.len() as u32,
        })
        .section(&code);
    Ok(module.finish())
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
