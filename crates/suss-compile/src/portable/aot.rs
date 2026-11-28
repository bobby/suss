//! Original target assembly for already prepared portable source fragments.
//! This development boundary supports scalar function exports, including interfaces.
use super::{Diagnostic, PreparedFragment, artifact_identity, core_bindings, resolve::Phase};
use crate::runtime_abi;
use suss_reader::Symbol;
use wasm_encoder::*;
use wit_parser::{FunctionKind, Type, WorldItem};
pub use wit_parser::{Resolve, WorldId};

mod characters;
mod composite;
mod imports;
mod list_schema;
mod lists;
#[cfg(suss_host)]
mod optimize;
mod parameters;
mod provenance;
mod random;
mod resources;
mod strings;

/// Recover complete verified-input manifests retained by the assembler. These
/// describe source inputs, not a signature of an optimizer's rewritten output.
pub fn input_source_manifests(
    component: &[u8],
) -> Result<Vec<artifact_identity::Manifest>, String> {
    provenance::read(component)
}

const VALUE: ValType = ValType::Ref(RefType::EQREF);
const NUMBER_UNBOX: u32 = 6;
const BIGINT_FROM_U64: u32 = 7;
const BIGINT_FITS_U64: u32 = 8;
const BIGINT_LOW_LONG: u32 = 9;
const HELPERS: u32 = 10;

struct Export {
    name: String,
    wit: wit_parser::Function,
    global: super::resolve::Global,
    params: Vec<(String, Boundary)>,
    result: Option<Boundary>,
    asynchronous: bool,
    indirect_params: Option<parameters::Layout>,
}
impl Export {
    fn flat_params(&self, resolve: &Resolve) -> Vec<ValType> {
        self.params
            .iter()
            .flat_map(|(_, ty)| ty.flat(resolve))
            .collect()
    }
    fn core_params(&self, resolve: &Resolve) -> Vec<ValType> {
        if self.indirect_params.is_some() {
            vec![ValType::I32]
        } else {
            self.flat_params(resolve)
        }
    }
    fn uses_memory(&self) -> bool {
        self.indirect_params.is_some()
            || self.params.iter().any(|(_, ty)| ty.uses_memory())
            || self.result.is_some_and(Boundary::uses_memory)
    }
}
enum PublicExport {
    Function {
        name: String,
        index: u32,
    },
    Interface {
        name: String,
        implements: Option<String>,
        external_id: Option<String>,
        types: Vec<(String, wit_parser::TypeId)>,
        functions: Vec<(String, u32)>,
    },
}

/// WIT annotations are host-defined metadata; public paths remain unchanged.
fn component_name<'a>(
    name: &'a str,
    implements: Option<&'a str>,
    external_id: Option<&'a str>,
) -> ComponentExternName<'a> {
    ComponentExternName {
        name: name.into(),
        implements: implements.map(Into::into),
        version_suffix: None,
        external_id: external_id.map(Into::into),
    }
}
#[derive(Clone, Copy)]
enum Scalar {
    Bool,
    Char,
    F32,
    F64,
    U8,
    S8,
    U16,
    S16,
    U32,
    S32,
    U64,
    S64,
}
#[derive(Clone, Copy)]
enum ListElement {
    Scalar(Scalar),
    String,
}

#[derive(Clone, Copy)]
enum Boundary {
    Scalar(Scalar),
    Option(Scalar),
    String,
    StringOption,
    List(ListElement),
    Generic(wit_parser::TypeId),
}
impl Boundary {
    fn flat(self, resolve: &Resolve) -> Vec<ValType> {
        match self {
            Self::Scalar(ty) => vec![ty.core()],
            Self::Option(ty) => vec![ValType::I32, ty.core()],
            Self::String | Self::List(_) => vec![ValType::I32, ValType::I32],
            Self::StringOption => vec![ValType::I32; 3],
            Self::Generic(id) => composite::flat(resolve, Type::Id(id)),
        }
    }
    fn core_result(self, resolve: &Resolve) -> Option<ValType> {
        Some(match self {
            Self::Scalar(ty) => ty.core(),
            Self::Generic(_) => match self.flat(resolve).as_slice() {
                [] => return None,
                [ty] => *ty,
                _ => ValType::I32,
            },
            _ => ValType::I32,
        })
    }
    fn string(self) -> bool {
        matches!(self, Self::String)
    }
    fn uses_strings(self, resolve: &Resolve) -> bool {
        match self {
            Self::Generic(id) => {
                let mut registry = composite::Registry::new(resolve).permit_resources();
                let index = registry.add(Type::Id(id)).expect("validated WIT value");
                registry
                    .nodes
                    .iter()
                    .take(index + 1)
                    .any(|node| matches!(node.shape, composite::Shape::String))
            }
            _ => matches!(
                self,
                Self::String | Self::StringOption | Self::List(ListElement::String)
            ),
        }
    }
    fn uses_memory(self) -> bool {
        matches!(
            self,
            Self::String | Self::StringOption | Self::List(_) | Self::Generic(_)
        )
    }
    fn needs_post(self, resolve: &Resolve) -> bool {
        self.uses_memory() && (!matches!(self, Self::Generic(_)) || self.flat(resolve).len() > 1)
    }
    fn list(self) -> Option<ListElement> {
        match self {
            Self::List(element) => Some(element),
            _ => None,
        }
    }
    fn string_option(self) -> bool {
        matches!(self, Self::StringOption)
    }
    fn indirect_result(self, resolve: &Resolve) -> bool {
        self.optional()
            || self.string()
            || self.list().is_some()
            || matches!(self, Self::Generic(_)) && self.flat(resolve).len() > 1
    }
    fn optional(self) -> bool {
        matches!(self, Self::Option(_) | Self::StringOption)
    }
}

fn boundary(resolve: &Resolve, mut ty: Type, resources: bool) -> Result<Boundary, Diagnostic> {
    for _ in 0..64 {
        if let Type::Id(id) = ty {
            match &resolve.types[id].kind {
                wit_parser::TypeDefKind::Type(next) => {
                    ty = *next;
                    continue;
                }
                wit_parser::TypeDefKind::List(payload) => {
                    let mut payload = *payload;
                    for _ in 0..64 {
                        if payload == Type::String {
                            return Ok(Boundary::List(ListElement::String));
                        }
                        if let Type::Id(id) = payload {
                            if let wit_parser::TypeDefKind::Type(next) = &resolve.types[id].kind {
                                payload = *next;
                                continue;
                            }
                        }
                        return match scalar(resolve, payload) {
                            Ok(ty) => Ok(Boundary::List(ListElement::Scalar(ty))),
                            Err(_) => generic_boundary(resolve, ty, resources),
                        };
                    }
                    return Err(error("Portable AOT list element alias depth exceeds64"));
                }
                wit_parser::TypeDefKind::Option(payload) => {
                    let mut payload = *payload;
                    for _ in 0..64 {
                        if payload == Type::String {
                            return Ok(Boundary::StringOption);
                        }
                        if let Type::Id(id) = payload {
                            if let wit_parser::TypeDefKind::Type(next) = &resolve.types[id].kind {
                                payload = *next;
                                continue;
                            }
                        }
                        return match scalar(resolve, payload) {
                            Ok(ty) => Ok(Boundary::Option(ty)),
                            Err(_) => generic_boundary(resolve, ty, resources),
                        };
                    }
                    return Err(error("WIT option payload alias nesting exceeds 64"));
                }
                _ => return generic_boundary(resolve, ty, resources),
            }
        }
        if ty == Type::String {
            return Ok(Boundary::String);
        }
        return scalar(resolve, ty).map(Boundary::Scalar);
    }
    Err(error("WIT boundary alias nesting exceeds 64"))
}

fn generic_boundary(resolve: &Resolve, ty: Type, resources: bool) -> Result<Boundary, Diagnostic> {
    let Type::Id(id) = ty else {
        return Err(error("Expected a resolved WIT composite type"));
    };
    let mut registry = composite::Registry::new(resolve);
    if resources {
        registry = registry.permit_resources();
    }
    registry.add(ty)?;
    Ok(Boundary::Generic(id))
}

impl Scalar {
    fn core(self) -> ValType {
        match self {
            Self::Bool
            | Self::Char
            | Self::U8
            | Self::S8
            | Self::U16
            | Self::S16
            | Self::U32
            | Self::S32 => ValType::I32,
            Self::F32 => ValType::F32,
            Self::F64 => ValType::F64,
            Self::U64 | Self::S64 => ValType::I64,
        }
    }
    fn payload_memory(self) -> MemArg {
        // Canonical option's one-byte discriminant precedes its aligned payload.
        let (offset, align) = match self {
            Self::F64 | Self::U64 | Self::S64 => (8, 3),
            Self::Char | Self::F32 | Self::U32 | Self::S32 => (4, 2),
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
            Self::U64 | Self::S64 => Instruction::I64Store(arg),
            Self::F32 => Instruction::F32Store(arg),
            Self::Char | Self::U32 | Self::S32 => Instruction::I32Store(arg),
            Self::U16 | Self::S16 => Instruction::I32Store16(arg),
            _ => Instruction::I32Store8(arg),
        }
    }
    fn payload_load(self) -> Instruction<'static> {
        let arg = self.payload_memory();
        match self {
            Self::F64 => Instruction::F64Load(arg),
            Self::U64 | Self::S64 => Instruction::I64Load(arg),
            Self::F32 => Instruction::F32Load(arg),
            Self::Char | Self::U32 | Self::S32 => Instruction::I32Load(arg),
            Self::U16 => Instruction::I32Load16U(arg),
            Self::S16 => Instruction::I32Load16S(arg),
            Self::S8 => Instruction::I32Load8S(arg),
            _ => Instruction::I32Load8U(arg),
        }
    }
    /// The range of longs an integer type accepts. U64's BigInt range is
    /// checked separately before extracting its unsigned bits.
    fn integer_bounds(self) -> Option<(i64, i64)> {
        Some(match self {
            Self::U8 => (0, u8::MAX.into()),
            Self::S8 => (i8::MIN.into(), i8::MAX.into()),
            Self::U16 => (0, u16::MAX.into()),
            Self::S16 => (i16::MIN.into(), i16::MAX.into()),
            Self::U32 => (0, u32::MAX.into()),
            Self::S32 => (i32::MIN.into(), i32::MAX.into()),
            Self::U64 => (0, i64::MAX),
            Self::S64 => (i64::MIN, i64::MAX),
            _ => return None,
        })
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
            Type::Char => Ok(Scalar::Char),
            Type::F32 => Ok(Scalar::F32),
            Type::F64 => Ok(Scalar::F64),
            Type::U8 => Ok(Scalar::U8),
            Type::S8 => Ok(Scalar::S8),
            Type::U16 => Ok(Scalar::U16),
            Type::S16 => Ok(Scalar::S16),
            Type::U32 => Ok(Scalar::U32),
            Type::S32 => Ok(Scalar::S32),
            Type::U64 => Ok(Scalar::U64),
            Type::S64 => Ok(Scalar::S64),
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
                "Portable AOT {} boundary adapters remain unimplemented; supported scalar types are bool, integers and floats",
                boundary_description(resolve, ty, 0)
            ))),
        };
    }
    Err(error("WIT scalar alias nesting exceeds 64"))
}

// World-local aliases appear as type imports in the resolved WIT graph.
// Supported value types introduce no runtime capability import; their adapters
// use the resolved underlying type. Function/interface/resource imports remain
// subject to the existing unsupported-feature diagnostic.
fn has_unsupported_imports(resolve: &Resolve, world: WorldId) -> bool {
    resolve.worlds[world]
        .imports
        .values()
        .any(|item| match item {
            WorldItem::Type { .. } => false,
            WorldItem::Interface { id, .. } => {
                !random::Capability::supported(resolve, *id)
                    && !resolve.interfaces[*id].functions.is_empty()
            }
            WorldItem::Function(_) => true,
        })
}

pub(crate) fn resource_api_symbols(
    resolve: &Resolve,
    world: WorldId,
) -> Result<Vec<Symbol>, Diagnostic> {
    Ok(if resources::Plan::selected(resolve, world)?.is_some() {
        resources::API_OPERATIONS
            .iter()
            .map(|name| Symbol::namespaced("suss.resource", *name))
            .collect()
    } else {
        Vec::new()
    })
}

/// Check explicit host mappings without executing source or granting Macro authority.
pub(crate) fn validate_import_boundary(
    resolve: &Resolve,
    world: WorldId,
    mappings: &[(String, Symbol)],
) -> Result<(), Diagnostic> {
    if mappings.is_empty() {
        return validate_boundary(resolve, world);
    }
    imports::selected(resolve, world, mappings)?;
    let resources = resources::Plan::selected(resolve, world)?;
    let mut values = resolve.clone();
    values.worlds[world]
        .imports
        .retain(|_, item| matches!(item, WorldItem::Type { .. }));
    validate_boundary_with_resources(&values, world, resources.as_ref())
}

/// Check only implemented WIT adapter capabilities, before source macro effects.
/// Source mappings, code generation and artifact validation have separate errors.
pub(crate) fn validate_boundary(resolve: &Resolve, world: WorldId) -> Result<(), Diagnostic> {
    let resources = resources::Plan::selected(resolve, world)?;
    validate_boundary_with_resources(resolve, world, resources.as_ref())
}
fn validate_boundary_with_resources(
    resolve: &Resolve,
    world: WorldId,
    resources: Option<&resources::Plan>,
) -> Result<(), Diagnostic> {
    let selected = &resolve.worlds[world];
    random::Capability::selected(resolve, world)?;
    for item in selected.imports.values() {
        if let WorldItem::Type { id, .. } = item {
            if !resources.is_some_and(|plan| plan.declaration(resolve, *id)) {
                boundary(resolve, Type::Id(*id), resources.is_some())?;
            }
        }
    }
    if has_unsupported_imports(resolve, world) {
        return Err(error(
            "Portable AOT imported WIT adapters remain unimplemented",
        ));
    }
    // Schema-only interfaces grant no host call authority. Validate their
    // complete selected types before source effects, including unused types.
    imports::selected(resolve, world, &[])?;
    let validate_function = |function: &wit_parser::Function| -> Result<(), Diagnostic> {
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
            let ty = boundary(resolve, parameter.ty, resources.is_some())?;
            flat_parameters += ty.flat(resolve).len();
        }
        if flat_parameters > Resolve::MAX_FLAT_PARAMS {
            // Check the resolver's memory32 layout before any source macros.
            parameters::Layout::new(resolve, function)?;
        }
        if let Some(result) = function.result {
            boundary(resolve, result, resources.is_some())?;
        }
        Ok(())
    };
    for item in selected.exports.values() {
        match item {
            WorldItem::Function(function) => validate_function(function)?,
            WorldItem::Interface { id, .. } => {
                let interface = &resolve.interfaces[*id];
                for ty in interface.types.values() {
                    if !resources.is_some_and(|plan| plan.declaration(resolve, *ty)) {
                        boundary(resolve, Type::Id(*ty), resources.is_some())?;
                    }
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
    if has_unsupported_imports(resolve, world) {
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
    component_with(fragments, resolve, world, mappings, false)
}
/// `component`, optionally merged and optimized with Binaryen (ADR-0005).
pub fn component_with(
    fragments: &[PreparedFragment],
    resolve: &Resolve,
    world: WorldId,
    mappings: &[(String, Symbol)],
    optimize: bool,
) -> Result<Vec<u8>, Diagnostic> {
    component_with_imports(fragments, resolve, world, mappings, &[], optimize)
}

/// Assemble explicitly mapped synchronous host calls with owned value codecs.
pub fn component_with_imports(
    fragments: &[PreparedFragment],
    resolve: &Resolve,
    world: WorldId,
    mappings: &[(String, Symbol)],
    import_mappings: &[(String, Symbol)],
    optimize: bool,
) -> Result<Vec<u8>, Diagnostic> {
    validate_import_boundary(resolve, world, import_mappings)?;
    let host_imports = if import_mappings.is_empty() {
        Vec::new()
    } else {
        imports::selected(resolve, world, import_mappings)?
    };
    let resource_plan = resources::Plan::selected(resolve, world)?;
    if (!host_imports.is_empty() || resource_plan.is_some()) && optimize {
        return Err(error(
            "Optimized synchronous import adapters remain unimplemented",
        ));
    }
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
            let params = function
                .params
                .iter()
                .map(|param| {
                    Ok((
                        param.name.clone(),
                        boundary(resolve, param.ty, resource_plan.is_some())?,
                    ))
                })
                .collect::<Result<Vec<_>, Diagnostic>>()?;
            let indirect_params = if params
                .iter()
                .map(|(_, ty)| ty.flat(resolve).len())
                .sum::<usize>()
                > Resolve::MAX_FLAT_PARAMS
            {
                Some(parameters::Layout::new(resolve, function)?)
            } else {
                None
            };
            exports.push(Export {
                name,
                wit: function.clone(),
                global,
                params,
                indirect_params,
                result: function
                    .result
                    .map(|ty| boundary(resolve, ty, resource_plan.is_some()))
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
            WorldItem::Interface {
                id, external_id, ..
            } => {
                let interface = &resolve.interfaces[*id];
                let mut functions = Vec::new();
                for (function_name, function) in &interface.functions {
                    let index = add_function(format!("{name}#{function_name}"), function)?;
                    functions.push((function_name.clone(), index));
                }
                public_exports.push(PublicExport::Interface {
                    name,
                    implements: resolve.implements_value(key, item),
                    external_id: external_id.clone(),
                    types: interface
                        .types
                        .iter()
                        .map(|(name, ty)| (name.clone(), *ty))
                        .collect(),
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
    let mut registry = composite::Registry::new(resolve);
    if resource_plan.is_some() {
        registry = registry.permit_resources();
    }
    for export in &exports {
        for ty in export.params.iter().map(|(_, ty)| *ty).chain(export.result) {
            if let Boundary::Generic(id) = ty {
                registry.add(Type::Id(id))?;
            }
        }
    }
    let import_globals = host_imports
        .iter()
        .map(|import| {
            last.environment
                .resolve(Phase::Runtime, &import.source, 0..0)
                .map(|value| value.global().clone())
        })
        .collect::<Result<Vec<_>, Diagnostic>>()?;
    let resource_bindings = if resource_plan.is_some() {
        resources::API_OPERATIONS
            .iter()
            .map(|name| {
                last.environment
                    .resolve(
                        Phase::Runtime,
                        &Symbol::namespaced("suss.resource", *name),
                        0..0,
                    )
                    .map(|value| value.global().clone())
            })
            .collect::<Result<Vec<_>, Diagnostic>>()?
    } else {
        vec![]
    };
    let resource_count = resource_plan
        .as_ref()
        .map_or(0, |plan| plan.resources.len() as u32);
    for import in &host_imports {
        for ty in import
            .wit
            .params
            .iter()
            .map(|p| p.ty)
            .chain(import.wit.result)
        {
            registry.add(ty)?;
        }
    }
    let generic_schema = !registry.nodes.is_empty();
    // Compile the design's tagged option schema through the ordinary source
    // pipeline, preserving nominal persistent-vector and keyword identities.
    // This fragment has no definitions or user macro expansion.
    let option_schema = exports.iter().any(|export| {
        export.params.iter().any(|(_, ty)| ty.optional())
            || export.result.is_some_and(Boundary::optional)
    });
    let list_schema = exports.iter().any(|export| {
        export.params.iter().any(|(_, ty)| ty.list().is_some())
            || export.result.is_some_and(|ty| ty.list().is_some())
    });
    let mut fragments = fragments.to_vec();
    if option_schema {
        fragments.push(super::prepare_privileged_fragment(
            r#"(let [identical suss.core/identical?
                        make-array suss.core/array get-array suss.core/aget
                        vector-class suss.core/PersistentVector
                        keyword-class suss.core/Keyword
                        root (.-EMPTY_NODE suss.core/PersistentVector)
                        none [:none] some-tag :some]
                (fn [lifting value]
                    (if lifting
                        (if (identical nil value) none
                            (new vector-class nil 2 5 root (make-array some-tag value) nil))
                        (if (suss.core/instance? vector-class value)
                            (let [length (.-cnt value) tail (.-tail value)]
                                (if (if (identical length 1) true (identical length 2))
                                    (let [tag (get-array tail 0)]
                                        (if (if (suss.core/instance? keyword-class tag)
                                                (identical nil (.-ns tag)) false)
                                            (if (identical length 1)
                                                (if (identical (.-name tag) "none") nil
                                                    (throw "WIT option requires [:none] or [:some value]"))
                                                (if (identical (.-name tag) "some")
                                                    (let [payload (get-array tail 1)]
                                                        (if (identical nil payload)
                                                            (throw "WIT scalar option payload cannot be nil")
                                                            payload))
                                                    (throw "WIT option requires [:none] or [:some value]")))
                                            (throw "WIT option requires [:none] or [:some value]")))
                                    (throw "WIT option requires [:none] or [:some value]")))
                            (throw "WIT option requires [:none] or [:some value]")))))"#,
            &fragments[0].environment,
            Phase::Runtime,
        )?);
    }
    if list_schema {
        fragments.push(super::prepare_privileged_fragment(
            list_schema::SOURCE,
            &fragments[0].environment,
            Phase::Runtime,
        )?);
    }
    if generic_schema {
        fragments.push(super::prepare_privileged_fragment(
            &registry.source(),
            &fragments[0].environment,
            Phase::Runtime,
        )?);
    }
    // Reject every artifact before assembling any initialization start function.
    let mut source_manifests = Vec::with_capacity(fragments.len());
    for fragment in &mut fragments {
        runtime_abi::verify_artifact(&fragment.wasm, &runtime_abi::Manifest::default())
            .map_err(error)?;
        source_manifests.push(
            artifact_identity::verify(
                &fragment.wasm,
                artifact_identity::Expected {
                    phase: Some(Phase::Runtime),
                    ..Default::default()
                },
            )
            .map_err(error)?,
        );
        fragment.wasm = artifact_identity::without_identity(&fragment.wasm).map_err(error)?;
    }
    let source_provenance = provenance::encode(&source_manifests).map_err(error)?;
    let cells = fragments
        .iter()
        .flat_map(|fragment| fragment.cells.iter().cloned())
        .collect::<Vec<_>>();
    let bindings = core_bindings::compile_with_cells(Phase::Runtime, &cells)?;
    let has_host_adapter = !host_imports.is_empty() || resource_plan.is_some();
    let has_memory = has_host_adapter || exports.iter().any(Export::uses_memory);
    let initialization = super::initialization::Plan::new(
        &fragments,
        usize::from(option_schema) + usize::from(list_schema) + usize::from(generic_schema),
    )?;
    let load_count = u32::from(initialization.module.is_some());
    let adapter = adapter(
        &exports,
        &initialization.names,
        option_schema,
        list_schema,
        &registry,
        &host_imports,
        &import_globals,
        resource_plan.as_ref(),
        &resource_bindings,
    )?;
    let asynchronous = exports
        .iter()
        .filter(|export| export.asynchronous)
        .collect::<Vec<_>>();
    // Direct links (#360): a fragment imports each link's closure global and
    // entries from module `suss.link.n`, and a var's slot from `suss.slot.s`,
    // which the earlier fragment that exports `link.n.code` or `slot.s`
    // provides. Fragment by fragment, each such import module and its definer.
    let mut link_definers = std::collections::BTreeMap::new();
    for (index, fragment) in fragments.iter().enumerate() {
        for payload in wasmparser::Parser::new(0).parse_all(&fragment.wasm) {
            if let Ok(wasmparser::Payload::ExportSection(section)) = payload {
                for export in section.into_iter().flatten() {
                    if export.kind == wasmparser::ExternalKind::Global {
                        if export.name.starts_with("slot.") {
                            link_definers.insert(format!("suss.{}", export.name), index);
                        } else if let Some(definition) = export
                            .name
                            .strip_prefix("link.")
                            .and_then(|name| name.strip_suffix(".code"))
                        {
                            link_definers.insert(format!("suss.link.{definition}"), index);
                        }
                    }
                }
            }
        }
    }
    let mut fragment_links = Vec::new();
    for fragment in &fragments {
        let mut modules = std::collections::BTreeMap::new();
        for payload in wasmparser::Parser::new(0).parse_all(&fragment.wasm) {
            if let Ok(wasmparser::Payload::ImportSection(section)) = payload {
                for import in section.into_imports().flatten() {
                    if import.module.starts_with("suss.link.")
                        || import.module.starts_with("suss.slot.")
                    {
                        let definer =
                            *link_definers.get(import.module).ok_or_else(|| Diagnostic {
                                span: 0..0,
                                message: format!(
                                    "Direct link {} has no defining fragment",
                                    import.module
                                ),
                            })?;
                        modules.insert(import.module.to_owned(), definer);
                    }
                }
            }
        }
        fragment_links.push(modules);
    }
    let mut component = Component::new();
    let randomness = random::Capability::selected(resolve, world)?;
    let random_count = u32::from(randomness.is_some());
    if !host_imports.is_empty() && randomness.is_some() {
        return Err(error(
            "Mixed automatic randomness and explicit host import mappings remain unimplemented",
        ));
    }
    let host_count = host_imports.len() as u32;
    let component_function_base = random_count + host_count;
    let mut resolved_types = super::command::shape::Types::new(resolve);
    let (interface_imports, mut host_functions, mut next_host_function) =
        imports::encode_interfaces(
            resolve,
            world,
            &host_imports,
            &mut component,
            &mut resolved_types,
        )?;
    let imported_instance_count = random_count + interface_imports;
    let random_type = randomness
        .as_ref()
        .map(|capability| capability.encode(resolve, &mut resolved_types))
        .transpose()?;
    // Publish nominal root values before defining their parents/functions:
    // component type exports introduce new identities, not just names.
    let mut live = wit_parser::LiveTypes::default();
    let mut imported_types = wit_parser::LiveTypes::default();
    for import in &host_imports {
        imported_types.add_func(resolve, &import.wit);
        live.add_func(resolve, &import.wit);
    }
    let mut root_names = std::collections::BTreeMap::new();
    for (key, item) in &selected.imports {
        if let WorldItem::Type { id, .. } = item {
            root_names.insert(*id, resolve.name_world_key(key));
            live.add_type_id(resolve, *id);
        }
    }
    for item in selected.exports.values() {
        if let WorldItem::Function(function) = item {
            live.add_func(resolve, function);
        }
    }
    let reserved = root_names
        .values()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut public_names = selected
        .exports
        .keys()
        .map(|key| resolve.name_world_key(key))
        .collect::<std::collections::BTreeSet<_>>();
    for id in live.iter() {
        if !root_names.contains_key(&id) && resolved_types.existing(id).is_some() {
            continue;
        }
        use wit_parser::TypeDefKind;
        if !root_names.contains_key(&id)
            && !matches!(
                resolve.types[id].kind,
                TypeDefKind::Record(_)
                    | TypeDefKind::Variant(_)
                    | TypeDefKind::Enum(_)
                    | TypeDefKind::Flags(_)
            )
        {
            continue;
        }
        let preferred = root_names.get(&id);
        let mut name = preferred
            .cloned()
            .or_else(|| resolve.types[id].name.clone())
            .unwrap_or_else(|| format!("value-{}", id.index()));
        while public_names.contains(&name) || (reserved.contains(&name) && preferred != Some(&name))
        {
            name.push_str("-value");
        }
        public_names.insert(name.clone());
        if imported_types.contains(id) {
            resolved_types.import_value(&mut component, &name, id)?;
        } else {
            resolved_types.export_value(&mut component, &name, id)?;
        }
    }
    let mut public_types = std::collections::BTreeMap::new();
    for export in &public_exports {
        if let PublicExport::Interface { types, .. } = export {
            for (_, id) in types {
                public_types.insert(*id, resolved_types.named(*id)?);
            }
        }
    }
    let mut function_types = Vec::new();
    let mut result_types = Vec::new();
    for export in &exports {
        result_types.push(
            export
                .wit
                .result
                .map(|ty| resolved_types.value(ty))
                .transpose()?,
        );
        function_types.push(resolved_types.function(&export.wit)?);
    }
    let host_types = host_imports
        .iter()
        .filter(|import| import.interface.is_none())
        .map(|import| resolved_types.function(&import.wit))
        .collect::<Result<Vec<_>, Diagnostic>>()?;
    resolved_types.flush(&mut component);
    if let (Some(capability), Some(ty)) = (&randomness, random_type) {
        let mut imports = ComponentImportSection::new();
        imports.import(
            component_name(
                &capability.name,
                capability.implements.as_deref(),
                capability.external_id.as_deref(),
            ),
            ComponentTypeRef::Instance(ty),
        );
        component.section(&imports);
        let mut aliases = ComponentAliasSection::new();
        aliases.alias(Alias::InstanceExport {
            instance: interface_imports,
            kind: ComponentExportKind::Func,
            name: "get-random-u64",
        });
        component.section(&aliases);
        let mut lower = CanonicalFunctionSection::new();
        lower.lower(0, std::iter::empty::<CanonicalOption>());
        component.section(&lower);
    }
    if !host_imports.is_empty() {
        let mut imports = ComponentImportSection::new();
        for ((ordinal, import), ty) in host_imports
            .iter()
            .enumerate()
            .filter(|(_, import)| import.interface.is_none())
            .zip(host_types)
        {
            imports.import(
                component_name(&import.name, None, import.wit.external_id.as_deref()),
                ComponentTypeRef::Func(ty),
            );
            host_functions[ordinal] = Some(next_host_function);
            next_host_function += 1;
        }
        component.section(&imports);
    }
    debug_assert_eq!(next_host_function, host_count);
    let mut modules = std::iter::once(("suss.runtime".to_owned(), runtime_abi::module()))
        .chain(std::iter::once((
            "suss.bindings.runtime".to_owned(),
            bindings.wasm,
        )))
        .chain(
            fragments
                .iter()
                .enumerate()
                .map(|(index, fragment)| (format!("suss.fragment.{index}"), fragment.wasm.clone())),
        )
        .chain(
            initialization
                .module
                .iter()
                .map(|module| (super::initialization::MODULE.to_owned(), module.clone())),
        )
        .chain(std::iter::once(("suss.aot".to_owned(), adapter)))
        .collect::<Vec<_>>();
    // ADR-0005 (#357): optimizing merges these into one module, which keeps
    // only the adapter's exports and imports only the allocator, if any.
    if optimize {
        #[cfg(suss_host)]
        {
            // A merged module has one name, so a fragment's link and slot
            // imports name their definer's fragment instead.
            for (index, links) in fragment_links.iter().enumerate() {
                if !links.is_empty() {
                    let module = &mut modules[2 + index].1;
                    *module = optimize::rename_import_modules(module, |name| {
                        links
                            .get(name)
                            .map(|definer| format!("suss.fragment.{definer}"))
                    })?;
                }
            }
            if randomness.is_some() {
                // wasm-merge preserves this instantiation/start order.
                modules.insert(1, ("suss.random.wire".to_owned(), random::wire()));
            }
            modules = vec![("suss.aot".to_owned(), optimize::merge(&modules)?)];
        }
        #[cfg(not(suss_host))]
        return Err(error("--optimize needs the native compiler host"));
    }
    // Module and instance indices of the two layouts. Unoptimized: runtime,
    // bindings, fragments, adapter, then the allocator and bridge modules;
    // the allocator's instance precedes the adapter's. Optimized: the merged
    // module stands for all of the first group.
    let separate = modules.len() > 1;
    let adapter_module = modules.len() as u32 - 1;
    let wire_module = if separate && randomness.is_some() {
        let index = modules.len() as u32;
        modules.push(("suss.random.wire".to_owned(), random::wire()));
        Some(index)
    } else {
        None
    };
    let memory_module = modules.len() as u32;
    let bridge_module = memory_module + u32::from(has_memory);
    for (_, module) in &modules {
        component.section(&RawSection {
            id: ComponentSectionId::CoreModule.into(),
            data: module,
        });
    }
    if has_memory {
        let memory = super::command::memory::module();
        component.section(&RawSection {
            id: ComponentSectionId::CoreModule.into(),
            data: &memory,
        });
    }
    if !asynchronous.is_empty() {
        let bridge = async_adapter(&asynchronous, has_memory, &registry);
        component.section(&RawSection {
            id: ComponentSectionId::CoreModule.into(),
            data: &bridge,
        });
    }
    let mut instances = InstanceSection::new();
    let mut args = vec![];
    if randomness.is_some() {
        instances.export_items([("word", ExportKind::Func, 0)]);
    }
    // Imported provider instance, followed by the runtime and its installer.
    let runtime_instance = random_count;
    let bindings_instance = 1 + 2 * random_count;
    let fragment_instance = 2 + 2 * random_count;
    if separate {
        instances.instantiate(0, std::iter::empty::<(&str, ModuleArg)>());
        if let Some(wire) = wire_module {
            instances.instantiate(
                wire,
                [
                    ("suss.random", ModuleArg::Instance(0)),
                    ("suss.runtime", ModuleArg::Instance(runtime_instance)),
                ],
            );
        }
        instances.instantiate(1, [("suss.runtime", ModuleArg::Instance(runtime_instance))]);
        for (index, links) in fragment_links.iter().enumerate() {
            let mut args = vec![
                (
                    "suss.runtime".to_owned(),
                    ModuleArg::Instance(runtime_instance),
                ),
                (
                    "suss.bindings.runtime".to_owned(),
                    ModuleArg::Instance(bindings_instance),
                ),
            ];
            args.extend(links.iter().map(|(module, definer)| {
                (
                    module.clone(),
                    ModuleArg::Instance(fragment_instance + *definer as u32),
                )
            }));
            instances.instantiate(2 + index as u32, args);
        }
        if load_count != 0 {
            let load = 2 + fragments.len() as u32;
            instances.instantiate(
                load,
                initialization.arguments(runtime_instance, bindings_instance, fragment_instance),
            );
            args.push((
                super::initialization::MODULE.to_owned(),
                ModuleArg::Instance(fragment_instance + fragments.len() as u32),
            ));
        }
        args.push((
            "suss.runtime".to_owned(),
            ModuleArg::Instance(runtime_instance),
        ));
        args.push((
            "suss.bindings.runtime".to_owned(),
            ModuleArg::Instance(bindings_instance),
        ));
        args.extend((0..fragments.len()).map(|index| {
            (
                format!("suss.fragment.{index}"),
                ModuleArg::Instance(fragment_instance + index as u32),
            )
        }));
    }
    if !separate && randomness.is_some() {
        args.push(("suss.random".to_owned(), ModuleArg::Instance(0)));
    }
    let first_own = if separate {
        fragment_instance + fragments.len() as u32 + load_count
    } else {
        random_count
    };
    if has_memory {
        instances.instantiate(memory_module, std::iter::empty::<(&str, ModuleArg)>());
        args.push(("suss.memory".to_owned(), ModuleArg::Instance(first_own)));
    }
    let adapter_instance = first_own + u32::from(has_memory) + u32::from(has_host_adapter);
    if !has_host_adapter {
        instances.instantiate(adapter_module, args);
        component.section(&instances);
        // Memory must be aliased before canonical task-return uses Unicode results.
        // Alias realloc first so its core function index is stable across sync/async.
        let mut memory_aliases = ComponentAliasSection::new();
        if has_memory
            || exports
                .iter()
                .any(|export| export.result.is_some_and(|ty| ty.indirect_result(resolve)))
        {
            memory_aliases.alias(Alias::CoreInstanceExport {
                instance: adapter_instance,
                kind: ExportKind::Memory,
                name: "suss.canonical.memory",
            });
        }
        if has_memory {
            memory_aliases.alias(Alias::CoreInstanceExport {
                instance: adapter_instance - 1,
                kind: ExportKind::Func,
                name: "cabi_realloc",
            });
        }
        if !memory_aliases.is_empty() {
            component.section(&memory_aliases);
        }
    } else {
        // Canon lower needs memory/realloc before the adapter start function
        // can call a provider during guest source initialization.
        component.section(&instances);
        let mut aliases = ComponentAliasSection::new();
        aliases.alias(Alias::CoreInstanceExport {
            instance: first_own,
            kind: ExportKind::Memory,
            name: "memory",
        });
        aliases.alias(Alias::CoreInstanceExport {
            instance: first_own,
            kind: ExportKind::Func,
            name: "cabi_realloc",
        });
        component.section(&aliases);
        let mut lowered = CanonicalFunctionSection::new();
        for function in &host_functions {
            lowered.lower(
                function.expect("resolved host function index"),
                [
                    CanonicalOption::Memory(0),
                    CanonicalOption::Realloc(random_count),
                    CanonicalOption::UTF16,
                ],
            );
        }
        component.section(&lowered);
        let mut drops = CanonicalFunctionSection::new();
        if let Some(plan) = &resource_plan {
            for resource in &plan.resources {
                drops.resource_drop(
                    resolved_types
                        .existing(resource.id)
                        .ok_or_else(|| error("Resource provider type identity is missing"))?,
                );
            }
        }
        if resource_count != 0 {
            component.section(&drops);
        }
        let mut installed = InstanceSection::new();
        let mut provider_functions = host_imports
            .iter()
            .enumerate()
            .map(|(index, import)| {
                (
                    import.name.clone(),
                    ExportKind::Func,
                    random_count + 1 + index as u32,
                )
            })
            .collect::<Vec<_>>();
        provider_functions.extend((0..resource_count).map(|i| {
            (
                resources::drop_name(i as usize),
                ExportKind::Func,
                random_count + 1 + host_count + i,
            )
        }));
        installed.export_items(provider_functions);
        args.push((
            "suss.host".to_owned(),
            ModuleArg::Instance(adapter_instance - 1),
        ));
        installed.instantiate(adapter_module, args);
        component.section(&installed);
    }
    let completion_base = random_count + host_count + resource_count + u32::from(has_memory);
    // Task-return builtins precede bridge instantiation, so the bridge can call
    // the canonical completion operation directly without a mutable shim.
    let mut completion = CanonicalFunctionSection::new();
    for (index, export) in exports.iter().enumerate() {
        if export.asynchronous {
            // task.return takes the flattened result as parameters; a scalar
            // option is a tag and payload, not the adapter's return pointer.
            let string_boundary = export.params.iter().any(|(_, ty)| ty.uses_strings(resolve))
                || export.result.is_some_and(|ty| ty.uses_strings(resolve));
            // Pinned task-return checks encoding against the lifted export even
            // for scalar/void results whose inputs required Unicode lowering.
            let mut options = vec![];
            if string_boundary {
                options.push(CanonicalOption::UTF16);
            }
            if export.result.is_some_and(Boundary::uses_memory) {
                options.push(CanonicalOption::Memory(0));
            }
            completion.task_return(result_types[index], options);
        }
    }
    let bridge_instance = if asynchronous.is_empty() {
        None
    } else {
        component.section(&completion);
        let mut bridge_instances = InstanceSection::new();
        bridge_instances.export_items(asynchronous.iter().enumerate().map(|(index, export)| {
            (
                export.name.as_str(),
                ExportKind::Func,
                completion_base + index as u32,
            )
        }));
        bridge_instances.instantiate(
            bridge_module,
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
    let mut core_function = completion_base + asynchronous.len() as u32;
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
        let entry = core_function;
        core_function += 1;
        let string_boundary = export.params.iter().any(|(_, ty)| ty.uses_strings(resolve))
            || export.result.is_some_and(|ty| ty.uses_strings(resolve));
        let mut options = vec![];
        if export.uses_memory() {
            options.extend([
                CanonicalOption::Memory(0),
                CanonicalOption::Realloc(random_count),
            ]);
            if string_boundary {
                options.push(CanonicalOption::UTF16);
            }
        } else if !export.asynchronous
            && export.result.is_some_and(|ty| ty.indirect_result(resolve))
        {
            options.push(CanonicalOption::Memory(0));
        }
        if export.asynchronous {
            options.extend([
                CanonicalOption::Async,
                CanonicalOption::Callback(callback.unwrap()),
            ]);
        } else if export.result.is_some_and(|ty| ty.needs_post(resolve)) {
            let post_name = format!("post.{}", export.name);
            aliases.alias(Alias::CoreInstanceExport {
                instance: adapter_instance,
                kind: ExportKind::Func,
                name: &post_name,
            });
            options.push(CanonicalOption::PostReturn(core_function));
            core_function += 1;
        }
        canonical.lift(entry, function_types[index], options);
    }
    for export in public_exports {
        match export {
            PublicExport::Function { name, index } => {
                public.export(
                    component_name(
                        &name,
                        None,
                        exports[index as usize].wit.external_id.as_deref(),
                    ),
                    ComponentExportKind::Func,
                    component_function_base + index,
                    None,
                );
            }
            PublicExport::Interface {
                name,
                implements,
                external_id,
                types,
                functions,
            } => {
                let index = imported_instance_count + exported_instances.len();
                exported_instances.export_items(
                    types
                        .iter()
                        .map(|(name, id)| {
                            (
                                component_name(name, None, None),
                                ComponentExportKind::Type,
                                public_types[id],
                            )
                        })
                        .chain(functions.iter().map(|(name, function)| {
                            (
                                component_name(
                                    name,
                                    None,
                                    exports[*function as usize].wit.external_id.as_deref(),
                                ),
                                ComponentExportKind::Func,
                                component_function_base + *function,
                            )
                        }))
                        .collect::<Vec<_>>(),
                );
                public.export(
                    component_name(&name, implements.as_deref(), external_id.as_deref()),
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
    component.section(&CustomSection {
        name: std::borrow::Cow::Borrowed(provenance::SECTION),
        data: std::borrow::Cow::Owned(source_provenance),
    });
    let bytes = component.finish();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&bytes)
        .map_err(|failure| error(format!("Invalid portable AOT component: {failure}")))?;
    Ok(bytes)
}

/// Canonical callback entrypoints for non-suspending source functions. They
/// complete the task exactly once, then report EXIT. No continuation exists:
/// suspension points must use the future rooted-continuation lowering instead.
fn async_adapter(
    exports: &[&Export],
    dynamic_memory: bool,
    registry: &composite::Registry,
) -> Vec<u8> {
    let resolve = registry.resolve;
    let mut types = TypeSection::new();
    let mut imports = ImportSection::new();
    for (index, export) in exports.iter().enumerate() {
        types.ty().function(
            export.core_params(resolve),
            export.result.and_then(|ty| ty.core_result(resolve)),
        );
        let result_flat = export
            .result
            .into_iter()
            .flat_map(|ty| ty.flat(resolve))
            .collect::<Vec<_>>();
        let completion_params = if result_flat.len() > Resolve::MAX_FLAT_PARAMS {
            vec![ValType::I32]
        } else {
            result_flat
        };
        types.ty().function(completion_params, []);
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
        .any(|export| export.result.is_some_and(|ty| ty.indirect_result(resolve)))
    {
        imports.import(
            "suss.aot",
            "suss.canonical.memory",
            EntityType::Memory(MemoryType {
                minimum: 1,
                maximum: Some(if dynamic_memory { 65536 } else { 1 }),
                memory64: false,
                shared: false,
                page_size_log2: None,
            }),
        );
    }
    let mut loaders = std::collections::BTreeMap::new();
    for (index, export) in exports.iter().enumerate() {
        if let Some(Boundary::Generic(id)) = export.result {
            let flat = Boundary::Generic(id).flat(resolve);
            if flat.len() > 1 && flat.len() <= Resolve::MAX_FLAT_PARAMS {
                let ty = types.len();
                types.ty().function([ValType::I32], flat);
                let function = 2 * exports.len() as u32 + loaders.len() as u32;
                imports.import(
                    "suss.aot",
                    &format!("flat.{}", export.name),
                    EntityType::Function(ty),
                );
                loaders.insert(index, function);
            }
        }
    }
    let mut posts = std::collections::BTreeMap::new();
    if exports
        .iter()
        .any(|export| export.result.is_some_and(|ty| ty.needs_post(resolve)))
    {
        let ty = types.len();
        types.ty().function([ValType::I32], []);
        for (index, export) in exports
            .iter()
            .enumerate()
            .filter(|(_, export)| export.result.is_some_and(|ty| ty.needs_post(resolve)))
        {
            let function = 2 * exports.len() as u32 + loaders.len() as u32 + posts.len() as u32;
            let name = format!("post.{}", export.name);
            imports.import("suss.aot", &name, EntityType::Function(ty));
            posts.insert(index, function);
        }
    }
    let imported = 2 * exports.len() as u32 + loaders.len() as u32 + posts.len() as u32;
    let mut functions = FunctionSection::new();
    let mut public = ExportSection::new();
    let mut code = CodeSection::new();
    for (index, export) in exports.iter().enumerate() {
        let entry_type = types.len();
        types
            .ty()
            .function(export.core_params(resolve), [ValType::I32]);
        functions.function(entry_type);
        public.export(
            &format!("entry.{index}"),
            ExportKind::Func,
            imported + index as u32,
        );
        let flat_params = export.core_params(resolve).len();
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
        if let Some(Boundary::Generic(id)) = export.result {
            let flat = Boundary::Generic(id).flat(resolve);
            if flat.len() > 1 {
                body.instruction(&Instruction::LocalSet(flat_params as u32))
                    .instruction(&Instruction::LocalGet(flat_params as u32));
                if let Some(loader) = loaders.get(&index) {
                    body.instruction(&Instruction::Call(*loader));
                }
            }
        } else if export.result.is_some_and(|ty| ty.needs_post(resolve)) {
            body.instruction(&Instruction::LocalSet(flat_params as u32));
            let optional = export.result.is_some_and(Boundary::string_option);
            if optional {
                body.instruction(&Instruction::LocalGet(flat_params as u32))
                    .instruction(&Instruction::I32Load8U(MemArg {
                        offset: 0,
                        align: 0,
                        memory_index: 0,
                    }));
            }
            body.instruction(&Instruction::LocalGet(flat_params as u32))
                .instruction(&Instruction::I32Load(MemArg {
                    offset: if optional { 4 } else { 0 },
                    align: 2,
                    memory_index: 0,
                }))
                .instruction(&Instruction::LocalGet(flat_params as u32))
                .instruction(&Instruction::I32Load(MemArg {
                    offset: if optional { 8 } else { 4 },
                    align: 2,
                    memory_index: 0,
                }));
        }
        body.instruction(&Instruction::Call(2 * index as u32 + 1));
        if let Some(post) = posts.get(&index) {
            // task-return consumes canonical bytes before the non-suspending
            // completion call returns. Release transfer ownership exactly once.
            body.instruction(&Instruction::LocalGet(flat_params as u32))
                .instruction(&Instruction::Call(*post));
        }
        body.instruction(&Instruction::I32Const(0))
            .instruction(&Instruction::End);
        code.function(&body);
    }
    let callback_type = types.len();
    types.ty().function([ValType::I32; 3], [ValType::I32]);
    functions.function(callback_type);
    public.export(
        "callback",
        ExportKind::Func,
        imported + exports.len() as u32,
    );
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

fn adapter(
    exports: &[Export],
    initializers: &[String],
    option_schema: bool,
    list_schema: bool,
    registry: &composite::Registry,
    host_imports: &[imports::Import],
    import_globals: &[super::resolve::Global],
    resource_plan: Option<&resources::Plan>,
    resource_bindings: &[super::resolve::Global],
) -> Result<Vec<u8>, Diagnostic> {
    let resolve = registry.resolve;
    let generic_schema = !registry.nodes.is_empty();
    let fragment_count = initializers.len();
    // Resource-only providers still need lifetime closures and canonical drop
    // functions, even when no provider function is mapped into source.
    let has_host_adapter = !host_imports.is_empty() || resource_plan.is_some();
    let has_memory = has_host_adapter || exports.iter().any(Export::uses_memory);
    let helpers = if has_memory { HELPERS + 4 } else { HELPERS }
        + if generic_schema {
            4
        } else if list_schema {
            2
        } else {
            0
        };
    let mut types = runtime_abi::prelude();
    let mut imports = ImportSection::new();
    let mut signatures = vec![
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
        ("number-unbox", vec![VALUE], vec![ValType::F64]),
        ("bigint-from-u64", vec![ValType::I64], vec![VALUE]),
        ("bigint-fits-u64", vec![VALUE], vec![ValType::I32]),
        ("bigint-low-long", vec![VALUE], vec![ValType::I64]),
    ];
    if has_memory {
        signatures.extend([
            ("string-length", vec![VALUE], vec![ValType::I32]),
            ("string-unit", vec![VALUE, ValType::I32], vec![ValType::I32]),
        ]);
    }
    for (index, (name, params, results)) in signatures.into_iter().enumerate() {
        types.ty().function(params, results);
        imports.import(
            "suss.runtime",
            name,
            EntityType::Function(runtime_abi::TYPE_COUNT + index as u32),
        );
    }
    if has_memory {
        for (index, (name, params, results)) in [
            ("cabi_realloc", vec![ValType::I32; 4], vec![ValType::I32]),
            ("release", vec![ValType::I32; 2], vec![]),
        ]
        .into_iter()
        .enumerate()
        {
            types.ty().function(params, results);
            imports.import(
                "suss.memory",
                name,
                EntityType::Function(runtime_abi::TYPE_COUNT + HELPERS + 2 + index as u32),
            );
        }
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
    }
    if list_schema || generic_schema {
        for (index, (name, params)) in [
            ("object-array-new", vec![VALUE]),
            ("array-set", vec![VALUE; 3]),
        ]
        .into_iter()
        .enumerate()
        {
            types.ty().function(params, [VALUE]);
            imports.import(
                "suss.runtime",
                name,
                EntityType::Function(runtime_abi::TYPE_COUNT + HELPERS + 4 + index as u32),
            );
        }
    }
    if generic_schema {
        for (index, (name, params)) in [
            ("array-get", vec![VALUE; 2]),
            ("packed-array-length", vec![VALUE]),
        ]
        .into_iter()
        .enumerate()
        {
            let ty = types.len() + runtime_abi::TYPE_COUNT - 1;
            types.ty().function(params, [VALUE]);
            imports.import("suss.runtime", name, EntityType::Function(ty));
            debug_assert_eq!(ty, runtime_abi::TYPE_COUNT + HELPERS + 6 + index as u32);
        }
    }
    types.ty().function([VALUE], []);
    imports.import(
        "suss.runtime",
        "language-exception",
        EntityType::Tag(TagType {
            kind: TagKind::Exception,
            func_type_idx: runtime_abi::TYPE_COUNT + helpers,
        }),
    );
    types.ty().function([], [VALUE]);
    for name in initializers {
        imports.import(
            name,
            "eval",
            EntityType::Function(runtime_abi::TYPE_COUNT + helpers + 1),
        );
    }
    let host_wiring = has_host_adapter.then(|| {
        imports::wire(
            &mut types,
            &mut imports,
            helpers + fragment_count as u32,
            host_imports,
            resolve,
        )
    });
    let resource_first = helpers
        + fragment_count as u32
        + if host_wiring.is_some() {
            2 + host_imports.len() as u32
        } else {
            0
        };
    let resource_wiring =
        resource_plan.map(|plan| resources::wire(&mut types, &mut imports, resource_first, plan));
    let resource_types = resource_plan.map(|_| resources::Types::declare(&mut types));
    // Components forbid duplicate module/name imports. Multiple WIT exports
    // may intentionally map to one live source var, so share only its import.
    let mut global_indices = std::collections::BTreeMap::new();
    for global in exports
        .iter()
        .map(|export| &export.global)
        .chain(import_globals)
        .chain(resource_bindings)
    {
        if !global_indices.contains_key(global) {
            let index = global_indices.len() as u32;
            imports.import(
                global.import_module(),
                &global.import_name(),
                EntityType::Global(GlobalType {
                    val_type: runtime_abi::binding_cell_type(),
                    mutable: false,
                    shared: false,
                }),
            );
            global_indices.insert(global.clone(), index);
        }
    }
    // False, true and undefined follow the binding cells; schema roots follow them.
    let sentinels = global_indices.len() as u32;
    for (name, _) in runtime_abi::SENTINEL_EXPORTS {
        imports.import(
            "suss.runtime",
            name,
            EntityType::Global(GlobalType {
                val_type: runtime_abi::sentinel_type(),
                mutable: false,
                shared: false,
            }),
        );
    }
    let schema_base = sentinels + runtime_abi::SENTINEL_EXPORTS.len() as u32;
    let generic_root = schema_base + u32::from(option_schema) + u32::from(list_schema);
    let imported = helpers
        + fragment_count as u32
        + if host_wiring.is_some() {
            2 + host_imports.len() as u32
        } else {
            0
        }
        + resource_plan.map_or(0, |plan| 3 + plan.resources.len() as u32);
    let posts = exports
        .iter()
        .filter(|export| export.result.is_some_and(|ty| ty.needs_post(resolve)))
        .count() as u32;
    let generic_base = imported + exports.len() as u32 + 1 + posts;
    let resource_base =
        composite::function(generic_base, registry.nodes.len(), 0) + host_imports.len() as u32;
    let resource_descriptor = generic_root + u32::from(generic_schema);
    let resource_carrier = resource_types.map(|types| resources::Carrier {
        types,
        helpers: resources::Helpers {
            object_new: resource_wiring.as_ref().unwrap().object_new,
            string_new: 4,
            string_set: 5,
            error_new: 3,
            error_tag: 0,
        },
        descriptor: resource_descriptor,
        scope: resource_descriptor + 1,
        borrow_scope: resource_descriptor + 2,
        check_function: resource_base + 1,
        close_function: resource_base + 2,
    });
    let resource_codec = resource_plan.map(|plan| resources::Codec {
        plan,
        resolve,
        drop_calls: &resource_wiring.as_ref().unwrap().drop_calls,
        make_function: resource_base,
        make_borrow_function: resource_base + 9,
        check_function: resource_base + 1,
        state_type: resource_types.unwrap().state,
    });
    let mut functions = FunctionSection::new();
    let mut public = ExportSection::new();
    let mut code = CodeSection::new();
    for (index, export) in exports.iter().enumerate() {
        let ty = types.len() + runtime_abi::TYPE_COUNT - 1;
        types.ty().function(
            export.core_params(resolve),
            export.result.and_then(|ty| ty.core_result(resolve)),
        );
        functions.function(ty);
        public.export(&export.name, ExportKind::Func, imported + index as u32);
        let parameter_base = u32::from(export.indirect_params.is_some());
        let result = parameter_base + export.flat_params(resolve).len() as u32;
        let integer = result + 1;
        // integer + 1 is an unused f64 local.
        let pointer = integer + 2;
        let length = pointer + 1;
        let position = length + 1;
        let unit = position + 1;
        let flat_locals = if export.indirect_params.is_some() {
            export
                .flat_params(resolve)
                .into_iter()
                .map(|ty| (1, ty))
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        let mut locals = flat_locals;
        locals.extend([
            (1, VALUE),
            (1, ValType::I32),
            (1, ValType::F64),
            (4, ValType::I32),
            (2, VALUE),
            (4, ValType::I32),
            (1, ValType::I64),
            (1, VALUE),
        ]);
        if generic_schema {
            locals.push((export.params.len() as u32, ValType::I32));
        }
        let context_base = unit
            + 9
            + if generic_schema {
                export.params.len() as u32
            } else {
                0
            };
        let result_moves = resource_plan
            .filter(|plan| plan.owned_result(resolve, &export.wit))
            .map(|_| {
                locals.push((1, VALUE));
                context_base
            });
        let borrow_context = resource_plan
            .filter(|plan| plan.borrowed_parameters(resolve, &export.wit))
            .map(|_| {
                let first = context_base + u32::from(result_moves.is_some());
                locals.push((5, VALUE));
                let returned = export
                    .result
                    .and_then(|ty| ty.core_result(resolve))
                    .map(|ty| {
                        locals.push((1, ty));
                        first + 5
                    });
                resources::BorrowContext {
                    previous: first,
                    token: first + 1,
                    error: first + 2,
                    cursor: first + 3,
                    state: first + 4,
                    returned,
                }
            });
        let mut body = Function::new(locals);
        if let Some(context) = &borrow_context {
            resource_carrier
                .as_ref()
                .unwrap()
                .begin_borrow(&mut body, context);
        }
        if let Some(layout) = &export.indirect_params {
            layout.read(&mut body, &export.params, resolve);
        }
        let wide = unit + 7;
        let scratch = unit + 8;
        let generic_inputs = scratch + 1;
        let string_locals = strings::Locals {
            value: result,
            pointer,
            length,
            index: position,
            unit,
        };
        let string_helpers = strings::Helpers {
            new: 4,
            length: HELPERS,
            unit: HELPERS + 1,
            set_unit: 5,
            realloc: HELPERS + 2,
        };
        let list_locals = lists::Locals {
            value: result,
            container: unit + 1,
            array: unit + 2,
            pointer: unit + 3,
            length: unit + 4,
            index: unit + 5,
            output: unit + 6,
            integer,
            sentinels,
            wide,
            scratch,
        };
        let list_root = schema_base + u32::from(option_schema);
        body.instruction(&Instruction::GlobalGet(global_indices[&export.global]))
            .instruction(&Instruction::Call(0));
        let mut parameter = parameter_base;
        for (ordinal, (_, ty)) in export.params.iter().enumerate() {
            match ty {
                Boundary::Generic(id) => {
                    let node = registry.index(Type::Id(*id));
                    if let Some(layout) = &export.indirect_params {
                        body.instruction(&Instruction::LocalGet(0))
                            .instruction(&Instruction::I32Const(layout.offset(ordinal) as i32))
                            .instruction(&Instruction::I32Add);
                    } else {
                        let local = generic_inputs + ordinal as u32;
                        body.instruction(&Instruction::I32Const(0))
                            .instruction(&Instruction::I32Const(0))
                            .instruction(&Instruction::I32Const(registry.nodes[node].align as i32))
                            .instruction(&Instruction::I32Const(registry.nodes[node].size as i32))
                            .instruction(&Instruction::Call(HELPERS + 2))
                            .instruction(&Instruction::LocalSet(local));
                        for field in 0..ty.flat(resolve).len() {
                            body.instruction(&Instruction::LocalGet(parameter + field as u32));
                        }
                        body.instruction(&Instruction::LocalGet(local))
                            .instruction(&Instruction::Call(composite::function(
                                generic_base,
                                node,
                                composite::STORE_FLAT,
                            )))
                            .instruction(&Instruction::LocalGet(local));
                    }
                    body.instruction(&Instruction::Call(composite::function(
                        generic_base,
                        node,
                        composite::COPY,
                    )));
                    parameter += ty.flat(resolve).len() as u32;
                }
                Boundary::List(element) => {
                    lists::copy_in(
                        &mut body,
                        *element,
                        parameter,
                        &list_locals,
                        &string_locals,
                        &string_helpers,
                        list_root,
                        &export.name,
                    );
                    parameter += 2;
                }
                Boundary::String => {
                    body.instruction(&Instruction::LocalGet(parameter))
                        .instruction(&Instruction::LocalSet(pointer))
                        .instruction(&Instruction::LocalGet(parameter + 1))
                        .instruction(&Instruction::LocalSet(length));
                    strings::copy_in(&mut body, &string_locals, &string_helpers);
                    body.instruction(&Instruction::LocalGet(result));
                    parameter += 2;
                }
                Boundary::Scalar(ty) => {
                    body.instruction(&Instruction::LocalGet(parameter));
                    lift_scalar(&mut body, ty, sentinels, wide);
                    parameter += 1;
                }
                Boundary::StringOption => {
                    body.instruction(&Instruction::LocalGet(parameter))
                        .instruction(&Instruction::If(BlockType::Result(VALUE)))
                        .instruction(&Instruction::LocalGet(parameter + 1))
                        .instruction(&Instruction::LocalSet(pointer))
                        .instruction(&Instruction::LocalGet(parameter + 2))
                        .instruction(&Instruction::LocalSet(length));
                    strings::copy_in(&mut body, &string_locals, &string_helpers);
                    body.instruction(&Instruction::LocalGet(result))
                        .instruction(&Instruction::Else)
                        .instruction(&runtime_abi::sentinel_value(runtime_abi::NIL))
                        .instruction(&Instruction::End)
                        .instruction(&Instruction::LocalSet(result))
                        .instruction(&Instruction::GlobalGet(schema_base))
                        .instruction(&runtime_abi::sentinel_in(runtime_abi::TRUE, sentinels))
                        .instruction(&Instruction::LocalGet(result))
                        .instruction(&Instruction::ArrayNewFixed {
                            array_type_index: runtime_abi::ARGS,
                            array_size: 2,
                        })
                        .instruction(&Instruction::Call(2));
                    parameter += 3;
                }
                Boundary::Option(ty) => {
                    body.instruction(&Instruction::LocalGet(parameter))
                        .instruction(&Instruction::If(BlockType::Result(VALUE)))
                        .instruction(&Instruction::LocalGet(parameter + 1));
                    lift_scalar(&mut body, ty, sentinels, wide);
                    body.instruction(&Instruction::Else)
                        .instruction(&runtime_abi::sentinel_value(runtime_abi::NIL))
                        .instruction(&Instruction::End);
                    // Consume the lifted scalar exactly once, then form its
                    // [:none]/[:some value] source representation.
                    body.instruction(&Instruction::LocalSet(result))
                        .instruction(&Instruction::GlobalGet(schema_base))
                        .instruction(&runtime_abi::sentinel_in(runtime_abi::TRUE, sentinels))
                        .instruction(&Instruction::LocalGet(result))
                        .instruction(&Instruction::ArrayNewFixed {
                            array_type_index: runtime_abi::ARGS,
                            array_size: 2,
                        })
                        .instruction(&Instruction::Call(2));
                    parameter += 2;
                }
            }
        }
        body.instruction(&Instruction::ArrayNewFixed {
            array_type_index: runtime_abi::ARGS,
            array_size: export.params.len() as u32,
        });
        // Every source argument now owns its GC value; release canonical input
        // buffers before invoking code that can throw a language exception.
        let mut parameter = parameter_base;
        for (ordinal, (_, ty)) in export.params.iter().enumerate() {
            if let Boundary::Generic(id) = ty {
                let node = registry.index(Type::Id(*id));
                if let Some(layout) = &export.indirect_params {
                    body.instruction(&Instruction::LocalGet(0))
                        .instruction(&Instruction::I32Const(layout.offset(ordinal) as i32))
                        .instruction(&Instruction::I32Add)
                        .instruction(&Instruction::Call(composite::function(
                            generic_base,
                            node,
                            composite::FREE,
                        )));
                } else {
                    let local = generic_inputs + ordinal as u32;
                    body.instruction(&Instruction::LocalGet(local))
                        .instruction(&Instruction::Call(composite::function(
                            generic_base,
                            node,
                            composite::FREE,
                        )))
                        .instruction(&Instruction::LocalGet(local))
                        .instruction(&Instruction::I32Const(registry.nodes[node].size as i32))
                        .instruction(&Instruction::Call(HELPERS + 3));
                }
            }
            if let Some(element) = ty.list() {
                lists::release_input(&mut body, element, parameter, &list_locals);
            }
            if ty.string() {
                body.instruction(&Instruction::LocalGet(parameter))
                    .instruction(&Instruction::LocalGet(parameter + 1))
                    .instruction(&Instruction::I32Const(2))
                    .instruction(&Instruction::I32Mul)
                    .instruction(&Instruction::Call(HELPERS + 3));
            }
            if ty.string_option() {
                body.instruction(&Instruction::LocalGet(parameter))
                    .instruction(&Instruction::If(BlockType::Empty))
                    .instruction(&Instruction::LocalGet(parameter + 1))
                    .instruction(&Instruction::LocalGet(parameter + 2))
                    .instruction(&Instruction::I32Const(2))
                    .instruction(&Instruction::I32Mul)
                    .instruction(&Instruction::Call(HELPERS + 3))
                    .instruction(&Instruction::End);
            }
            parameter += ty.flat(resolve).len() as u32;
        }
        if let Some(layout) = &export.indirect_params {
            body.instruction(&Instruction::LocalGet(0))
                .instruction(&Instruction::I32Const(layout.size as i32))
                .instruction(&Instruction::Call(HELPERS + 3));
        }
        body.instruction(&Instruction::Call(2))
            .instruction(&Instruction::LocalSet(result));
        match export.result {
            None => {}
            Some(Boundary::Generic(id)) => {
                let node = registry.index(Type::Id(id));
                composite::schema_call(&mut body, generic_root, node, true, result);
                body.instruction(&Instruction::LocalSet(result))
                    .instruction(&Instruction::LocalGet(result))
                    .instruction(&Instruction::Call(composite::function(
                        generic_base,
                        node,
                        composite::VALIDATE,
                    )));
                if let Some(queue) = result_moves {
                    body.instruction(&Instruction::LocalGet(result))
                        .instruction(&Instruction::RefNull(HeapType::Abstract {
                            shared: false,
                            ty: AbstractHeapType::Eq,
                        }))
                        .instruction(&Instruction::Call(resource_base + 10 + node as u32))
                        .instruction(&Instruction::LocalSet(queue));
                }
                body.instruction(&Instruction::I32Const(0))
                    .instruction(&Instruction::I32Const(0))
                    .instruction(&Instruction::I32Const(registry.nodes[node].align as i32))
                    .instruction(&Instruction::I32Const(registry.nodes[node].size as i32))
                    .instruction(&Instruction::Call(HELPERS + 2))
                    .instruction(&Instruction::LocalSet(pointer))
                    .instruction(&Instruction::LocalGet(result))
                    .instruction(&Instruction::LocalGet(pointer))
                    .instruction(&Instruction::Call(composite::function(
                        generic_base,
                        node,
                        composite::WRITE,
                    )))
                    .instruction(&Instruction::LocalGet(pointer));
                if Boundary::Generic(id).flat(resolve).len() <= 1 {
                    body.instruction(&Instruction::Call(composite::function(
                        generic_base,
                        node,
                        composite::LOAD_FLAT,
                    )))
                    .instruction(&Instruction::LocalGet(pointer))
                    .instruction(&Instruction::Call(composite::function(
                        generic_base,
                        node,
                        composite::FREE,
                    )))
                    .instruction(&Instruction::LocalGet(pointer))
                    .instruction(&Instruction::I32Const(registry.nodes[node].size as i32))
                    .instruction(&Instruction::Call(HELPERS + 3));
                }
            }
            Some(Boundary::List(element)) => {
                lists::copy_out(
                    &mut body,
                    element,
                    &list_locals,
                    &string_locals,
                    &string_helpers,
                    list_root,
                    &export.name,
                );
            }
            Some(Boundary::String) => {
                strings::validate(&mut body, &string_locals, &string_helpers, |body| {
                    boundary_error_message(
                        body,
                        result,
                        &format!(
                            "WIT export {} returned an incompatible Unicode string",
                            export.name
                        ),
                    );
                });
                strings::copy_out(&mut body, &string_locals, &string_helpers);
                body.instruction(&Instruction::I32Const(0))
                    .instruction(&Instruction::LocalGet(pointer))
                    .instruction(&Instruction::I32Store(MemArg {
                        offset: 0,
                        align: 2,
                        memory_index: 0,
                    }))
                    .instruction(&Instruction::I32Const(0))
                    .instruction(&Instruction::LocalGet(length))
                    .instruction(&Instruction::I32Store(MemArg {
                        offset: 4,
                        align: 2,
                        memory_index: 0,
                    }))
                    .instruction(&Instruction::I32Const(0));
            }
            Some(Boundary::StringOption) => {
                body.instruction(&Instruction::GlobalGet(schema_base))
                    .instruction(&runtime_abi::sentinel_in(runtime_abi::FALSE, sentinels))
                    .instruction(&Instruction::LocalGet(result))
                    .instruction(&Instruction::ArrayNewFixed {
                        array_type_index: runtime_abi::ARGS,
                        array_size: 2,
                    })
                    .instruction(&Instruction::Call(2))
                    .instruction(&Instruction::LocalSet(result));
                // A nil result is WIT option none.
                body.instruction(&Instruction::LocalGet(result))
                    .instruction(&Instruction::RefIsNull)
                    .instruction(&Instruction::If(BlockType::Empty));
                for (offset, align) in [(0, 0), (4, 2), (8, 2)] {
                    body.instruction(&Instruction::I32Const(0))
                        .instruction(&Instruction::I32Const(0));
                    if offset == 0 {
                        body.instruction(&Instruction::I32Store8(MemArg {
                            offset,
                            align,
                            memory_index: 0,
                        }));
                    } else {
                        body.instruction(&Instruction::I32Store(MemArg {
                            offset,
                            align,
                            memory_index: 0,
                        }));
                    }
                }
                body.instruction(&Instruction::Else);
                strings::validate(&mut body, &string_locals, &string_helpers, |body| {
                    boundary_error_message(
                        body,
                        result,
                        &format!(
                            "WIT export {} returned an incompatible Unicode string option payload",
                            export.name
                        ),
                    );
                });
                strings::copy_out(&mut body, &string_locals, &string_helpers);
                body.instruction(&Instruction::I32Const(0))
                    .instruction(&Instruction::I32Const(1))
                    .instruction(&Instruction::I32Store8(MemArg {
                        offset: 0,
                        align: 0,
                        memory_index: 0,
                    }))
                    .instruction(&Instruction::I32Const(0))
                    .instruction(&Instruction::LocalGet(pointer))
                    .instruction(&Instruction::I32Store(MemArg {
                        offset: 4,
                        align: 2,
                        memory_index: 0,
                    }))
                    .instruction(&Instruction::I32Const(0))
                    .instruction(&Instruction::LocalGet(length))
                    .instruction(&Instruction::I32Store(MemArg {
                        offset: 8,
                        align: 2,
                        memory_index: 0,
                    }))
                    .instruction(&Instruction::End)
                    .instruction(&Instruction::I32Const(0));
            }
            Some(Boundary::Scalar(ty)) => lower_scalar(
                &mut body,
                ty,
                result,
                integer,
                wide,
                &export.name,
                sentinels,
            ),
            Some(Boundary::Option(ty)) => {
                debug_assert!(option_schema);
                body.instruction(&Instruction::GlobalGet(schema_base))
                    .instruction(&runtime_abi::sentinel_in(runtime_abi::FALSE, sentinels))
                    .instruction(&Instruction::LocalGet(result))
                    .instruction(&Instruction::ArrayNewFixed {
                        array_type_index: runtime_abi::ARGS,
                        array_size: 2,
                    })
                    .instruction(&Instruction::Call(2))
                    .instruction(&Instruction::LocalSet(result));
                // Scalar option returns use a fixed canonical return area. No
                // allocation, ownership transfer or post-return free is needed.
                // A nil result is WIT option none.
                body.instruction(&Instruction::LocalGet(result))
                    .instruction(&Instruction::RefIsNull)
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
                lower_scalar(
                    &mut body,
                    ty,
                    result,
                    integer,
                    wide,
                    &export.name,
                    sentinels,
                );
                let store = ty.payload_store();
                body.instruction(&store)
                    .instruction(&Instruction::End)
                    .instruction(&Instruction::I32Const(0));
            }
        }
        if let Some(context) = &borrow_context {
            resource_carrier
                .as_ref()
                .unwrap()
                .finish_borrow(&mut body, context);
        }
        if let Some(queue) = result_moves {
            body.instruction(&Instruction::LocalGet(queue))
                .instruction(&Instruction::Call(resource_base + 5));
        }
        body.instruction(&Instruction::End);
        code.function(&body);
    }
    let start_type = types.len() + runtime_abi::TYPE_COUNT - 1;
    types.ty().function([], []);
    functions.function(start_type);
    let mut start = Function::new([(1, VALUE)]);
    let schema_count =
        usize::from(option_schema) + usize::from(list_schema) + usize::from(generic_schema);
    for index in 0..fragment_count - schema_count {
        start
            .instruction(&Instruction::Call(helpers + index as u32))
            .instruction(&Instruction::Drop);
        if option_schema && index == 0 {
            // The first fragment provides compiled Runtime core. Capture schema
            // closures and constructors before any user initializer can redefine
            // those public cells. This private root is never exported.
            start
                .instruction(&Instruction::Call(
                    helpers + (fragment_count - schema_count) as u32,
                ))
                .instruction(&Instruction::GlobalSet(schema_base));
        }
        if list_schema && index == 0 {
            start
                .instruction(&Instruction::Call(
                    helpers + fragment_count as u32 - 1 - u32::from(generic_schema),
                ))
                .instruction(&Instruction::GlobalSet(
                    schema_base + u32::from(option_schema),
                ));
        }
        if generic_schema && index == 0 {
            start
                .instruction(&Instruction::Call(helpers + fragment_count as u32 - 1))
                .instruction(&Instruction::GlobalSet(generic_root));
        }
        if index == 0 {
            if let Some(carrier) = &resource_carrier {
                resources::initialize_descriptor(
                    &mut start,
                    carrier.helpers,
                    resource_wiring.as_ref().unwrap().descriptor_new,
                    resource_descriptor,
                );
                for (binding, invoker) in resource_bindings
                    .iter()
                    .zip([resource_base + 3, resource_base + 8])
                {
                    resources::install_operation(
                        &mut start,
                        global_indices[binding],
                        invoker,
                        host_wiring.as_ref().expect("resource host wiring"),
                    );
                }
            }
        }
        if let Some(wiring) = &host_wiring {
            if index == 0 {
                imports::install(
                    &mut start,
                    host_imports,
                    import_globals,
                    &global_indices,
                    wiring,
                    composite::function(generic_base, registry.nodes.len(), 0),
                );
            }
        }
    }
    start.instruction(&Instruction::End);
    code.function(&start);
    for export in exports
        .iter()
        .filter(|export| export.result.is_some_and(|ty| ty.needs_post(resolve)))
    {
        // TypeSection counts the shared recursive group as one entry, while
        // core type indices count all TYPE_COUNT members of that group.
        let ty = types.len() + runtime_abi::TYPE_COUNT - 1;
        types.ty().function([ValType::I32], []);
        let index = imported + functions.len();
        functions.function(ty);
        public.export(&format!("post.{}", export.name), ExportKind::Func, index);
        let mut post = Function::new([(3, ValType::I32)]);
        if let Some(Boundary::Generic(id)) = export.result {
            let node = registry.index(Type::Id(id));
            post.instruction(&Instruction::LocalGet(0))
                .instruction(&Instruction::Call(composite::function(
                    generic_base,
                    node,
                    composite::FREE,
                )))
                .instruction(&Instruction::LocalGet(0))
                .instruction(&Instruction::I32Const(registry.nodes[node].size as i32))
                .instruction(&Instruction::Call(HELPERS + 3))
                .instruction(&Instruction::End);
            code.function(&post);
            continue;
        }
        if let Some(element) = export.result.and_then(Boundary::list) {
            lists::post_return(&mut post, element);
            post.instruction(&Instruction::End);
            code.function(&post);
            continue;
        }
        let optional = export.result.is_some_and(Boundary::string_option);
        if optional {
            post.instruction(&Instruction::LocalGet(0))
                .instruction(&Instruction::I32Load8U(MemArg {
                    offset: 0,
                    align: 0,
                    memory_index: 0,
                }))
                .instruction(&Instruction::If(BlockType::Empty));
        }
        post.instruction(&Instruction::LocalGet(0))
            .instruction(&Instruction::I32Load(MemArg {
                offset: if optional { 4 } else { 0 },
                align: 2,
                memory_index: 0,
            }))
            .instruction(&Instruction::LocalGet(0))
            .instruction(&Instruction::I32Load(MemArg {
                offset: if optional { 8 } else { 4 },
                align: 2,
                memory_index: 0,
            }))
            .instruction(&Instruction::I32Const(2))
            .instruction(&Instruction::I32Mul)
            .instruction(&Instruction::Call(HELPERS + 3));
        if optional {
            post.instruction(&Instruction::End);
        }
        post.instruction(&Instruction::End);
        code.function(&post);
    }
    if generic_schema {
        debug_assert_eq!(imported + functions.len(), generic_base);
        registry.emit(
            &mut types,
            &mut functions,
            &mut code,
            generic_base,
            generic_root,
            sentinels,
            resource_codec.as_ref(),
        );
        for export in exports {
            if let Some(Boundary::Generic(id)) = export.result {
                let node = registry.index(Type::Id(id));
                public.export(
                    &format!("flat.{}", export.name),
                    ExportKind::Func,
                    composite::function(generic_base, node, composite::LOAD_FLAT),
                );
            }
        }
    }
    if let Some(wiring) = &host_wiring {
        imports::emit_invokers(
            host_imports,
            registry,
            &mut functions,
            &mut code,
            wiring,
            generic_base,
            generic_root,
            resource_plan.map(|_| (resource_base + 10, resource_base + 5)),
        )?;
    }
    if let Some(carrier) = &resource_carrier {
        debug_assert_eq!(imported + functions.len(), resource_base);
        for (ty, body) in [
            (carrier.types.make, carrier.make()),
            (carrier.types.check, carrier.check()),
            (carrier.types.close, carrier.close()),
            (runtime_abi::INVOKE, carrier.invoke_close()),
            (carrier.types.reserve, carrier.reserve()),
            (carrier.types.commit, carrier.commit()),
            (carrier.types.close, carrier.cleanup_scope()),
            (
                runtime_abi::INVOKE,
                carrier.invoke_scope_cleanup(resource_base + 6),
            ),
            (
                runtime_abi::INVOKE,
                carrier.invoke_scope(
                    resource_base + 7,
                    resource_wiring.as_ref().unwrap().try_invoke,
                    host_wiring.as_ref().unwrap().closure_new,
                ),
            ),
            (carrier.types.make, carrier.make_borrow()),
        ] {
            functions.function(ty);
            code.function(&body);
        }
        registry.emit_resource_arguments(
            &mut functions,
            &mut code,
            carrier.types.gather,
            resource_base + 10,
            resource_base + 4,
            resource_codec.as_ref().unwrap(),
        );
    }
    let mut module = Module::new();
    module
        .section(&runtime_abi::Manifest::default().section())
        .section(&types)
        .section(&imports)
        .section(&functions);
    if !has_memory
        && exports
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
    }
    if has_memory
        || exports
            .iter()
            .any(|export| export.result.is_some_and(Boundary::optional))
    {
        public.export("suss.canonical.memory", ExportKind::Memory, 0);
    }
    if option_schema || list_schema || generic_schema || resource_plan.is_some() {
        let mut globals = GlobalSection::new();
        for _ in 0..schema_count + 3 * usize::from(resource_plan.is_some()) {
            globals.global(
                GlobalType {
                    val_type: VALUE,
                    mutable: true,
                    shared: false,
                },
                // Schema helper roots hold nil until initialized.
                &ConstExpr::extended([runtime_abi::sentinel_value(runtime_abi::NIL)]),
            );
        }
        module.section(&globals);
    }
    module.section(&public).section(&StartSection {
        function_index: imported + exports.len() as u32,
    });
    if host_wiring.is_some() {
        let first = composite::function(generic_base, registry.nodes.len(), 0);
        let mut elements = ElementSection::new();
        let mut refs = (0..host_imports.len())
            .map(|i| first + i as u32)
            .collect::<Vec<_>>();
        if let Some(wiring) = &resource_wiring {
            refs.extend(wiring.drop_calls.iter().copied());
            refs.extend([resource_base + 3, resource_base + 7, resource_base + 8]);
        }
        elements.declared(Elements::Functions(std::borrow::Cow::Owned(refs)));
        module.section(&elements);
    }
    module.section(&code);
    Ok(module.finish())
}
/// `wide` is an `i64` scratch local for boxing a long.
fn lift_scalar(body: &mut Function, ty: &Scalar, sentinels: u32, wide: u32) {
    match ty {
        Scalar::Char => characters::lift(body, wide),
        Scalar::Bool => {
            for instruction in runtime_abi::boolean_in(sentinels) {
                body.instruction(&instruction);
            }
        }
        Scalar::F32 => {
            body.instruction(&Instruction::F64PromoteF32)
                .instruction(&Instruction::Call(1));
        }
        Scalar::F64 => {
            body.instruction(&Instruction::Call(1));
        }
        Scalar::U64 => {
            // Preserve unsigned canonical bits as a positive exact magnitude.
            body.instruction(&Instruction::LocalTee(wide))
                .instruction(&Instruction::I64Const(0))
                .instruction(&Instruction::I64LtS)
                .instruction(&Instruction::If(BlockType::Result(VALUE)))
                .instruction(&Instruction::LocalGet(wide))
                .instruction(&Instruction::Call(BIGINT_FROM_U64))
                .instruction(&Instruction::Else)
                .instruction(&Instruction::LocalGet(wide));
            for instruction in runtime_abi::long_box_in(wide) {
                body.instruction(&instruction);
            }
            body.instruction(&Instruction::End);
        }
        Scalar::S64 => {
            for instruction in runtime_abi::long_box_in(wide) {
                body.instruction(&instruction);
            }
        }
        Scalar::U8 | Scalar::S8 | Scalar::U16 | Scalar::S16 | Scalar::U32 | Scalar::S32 => {
            // Normalize canonical subword bits before widening to a long;
            // u32 must stay unsigned.
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
            body.instruction(&if matches!(ty, Scalar::U8 | Scalar::U16 | Scalar::U32) {
                Instruction::I64ExtendI32U
            } else {
                Instruction::I64ExtendI32S
            });
            for instruction in runtime_abi::long_box_in(wide) {
                body.instruction(&instruction);
            }
        }
    }
}

/// `integer` is an `i32` scratch local and `wide` an `i64` one.
fn lower_scalar(
    body: &mut Function,
    scalar: Scalar,
    result: u32,
    integer: u32,
    wide: u32,
    name: &str,
    sentinels: u32,
) {
    match scalar {
        Scalar::Char => characters::lower(body, result, integer, name),
        Scalar::Bool => {
            // A WIT bool must be false or true.
            body.instruction(&Instruction::LocalGet(result))
                .instruction(&runtime_abi::sentinel_in(runtime_abi::TRUE, sentinels))
                .instruction(&Instruction::RefEq)
                .instruction(&Instruction::LocalTee(integer))
                .instruction(&Instruction::LocalGet(result))
                .instruction(&runtime_abi::sentinel_in(runtime_abi::FALSE, sentinels))
                .instruction(&Instruction::RefEq)
                .instruction(&Instruction::I32Or)
                .instruction(&Instruction::I32Eqz)
                .instruction(&Instruction::If(BlockType::Empty));
            boundary_error(body, result, name);
            body.instruction(&Instruction::End)
                .instruction(&Instruction::LocalGet(integer));
        }
        Scalar::F32 | Scalar::F64 => {
            // Any number, as a double; f32 rounds.
            for instruction in runtime_abi::number_test(result) {
                body.instruction(&instruction);
            }
            body.instruction(&Instruction::I32Eqz)
                .instruction(&Instruction::If(BlockType::Empty));
            boundary_error(body, result, name);
            body.instruction(&Instruction::End);
            if matches!(scalar, Scalar::F32) {
                // A long rounds once, as Clojure's float does, not through
                // a double first.
                for instruction in runtime_abi::long_test(result) {
                    body.instruction(&instruction);
                }
                body.instruction(&Instruction::If(BlockType::Result(ValType::F32)));
                for instruction in runtime_abi::long_to_i64(result) {
                    body.instruction(&instruction);
                }
                body.instruction(&Instruction::F32ConvertI64S)
                    .instruction(&Instruction::Else);
                body.instruction(&Instruction::LocalGet(result))
                    .instruction(&Instruction::Call(NUMBER_UNBOX));
                body.instruction(&Instruction::F32DemoteF64)
                    .instruction(&Instruction::End);
            } else {
                body.instruction(&Instruction::LocalGet(result))
                    .instruction(&Instruction::Call(NUMBER_UNBOX));
            }
        }
        Scalar::U64 => {
            for instruction in runtime_abi::long_test(result) {
                body.instruction(&instruction);
            }
            body.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
            for instruction in runtime_abi::long_to_i64(result) {
                body.instruction(&instruction);
            }
            body.instruction(&Instruction::LocalTee(wide))
                .instruction(&Instruction::I64Const(0))
                .instruction(&Instruction::I64LtS)
                .instruction(&Instruction::If(BlockType::Empty));
            boundary_error(body, result, name);
            body.instruction(&Instruction::End)
                .instruction(&Instruction::LocalGet(wide))
                .instruction(&Instruction::Else)
                .instruction(&Instruction::LocalGet(result))
                .instruction(&Instruction::RefTestNonNull(HeapType::Concrete(
                    runtime_abi::BIGINT,
                )))
                .instruction(&Instruction::I32Eqz)
                .instruction(&Instruction::If(BlockType::Empty));
            boundary_error(body, result, name);
            body.instruction(&Instruction::End)
                .instruction(&Instruction::LocalGet(result))
                .instruction(&Instruction::Call(BIGINT_FITS_U64))
                .instruction(&Instruction::I32Eqz)
                .instruction(&Instruction::If(BlockType::Empty));
            boundary_error(body, result, name);
            body.instruction(&Instruction::End)
                .instruction(&Instruction::LocalGet(result))
                .instruction(&Instruction::Call(BIGINT_LOW_LONG))
                .instruction(&Instruction::End);
        }
        scalar => {
            // A long within the type's range; a double is not converted.
            let (min, max) = scalar.integer_bounds().unwrap();
            for instruction in runtime_abi::long_test(result) {
                body.instruction(&instruction);
            }
            body.instruction(&Instruction::I32Eqz)
                .instruction(&Instruction::If(BlockType::Empty));
            boundary_error(body, result, name);
            body.instruction(&Instruction::End);
            for instruction in runtime_abi::long_to_i64(result) {
                body.instruction(&instruction);
            }
            body.instruction(&Instruction::LocalTee(wide))
                .instruction(&Instruction::I64Const(min))
                .instruction(&Instruction::I64LtS)
                .instruction(&Instruction::LocalGet(wide))
                .instruction(&Instruction::I64Const(max))
                .instruction(&Instruction::I64GtS)
                .instruction(&Instruction::I32Or)
                .instruction(&Instruction::If(BlockType::Empty));
            boundary_error(body, result, name);
            body.instruction(&Instruction::End)
                .instruction(&Instruction::LocalGet(wide));
            if !matches!(scalar, Scalar::U64 | Scalar::S64) {
                body.instruction(&Instruction::I32WrapI64);
            }
        }
    }
}

fn boundary_error(body: &mut Function, local: u32, name: &str) {
    boundary_error_message(
        body,
        local,
        &format!("WIT export {name} returned an incompatible scalar value"),
    );
}
fn boundary_error_message(body: &mut Function, local: u32, message: &str) {
    let message = message.encode_utf16().collect::<Vec<_>>();
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
