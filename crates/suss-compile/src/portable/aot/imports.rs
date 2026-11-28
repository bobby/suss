//! Explicit synchronous host calls through owned source/canonical value codecs.
use super::{Diagnostic, HELPERS, VALUE, composite, error, parameters};
use crate::portable::resolve::Global;
use crate::runtime_abi;
use suss_reader::Symbol;
use wasm_encoder::*;
use wit_parser::{
    FunctionKind, Resolve, WorldId, WorldItem,
    abi::{AbiVariant, WasmType},
};

pub(super) struct Import {
    pub name: String,
    pub source: Symbol,
    pub wit: wit_parser::Function,
}

/// Resolve authority before preparing source or running any source macro.
pub(super) fn selected(
    resolve: &Resolve,
    world: WorldId,
    mappings: &[(String, Symbol)],
) -> Result<Vec<Import>, Diagnostic> {
    let mut imports = Vec::new();
    let mut sources = std::collections::BTreeSet::new();
    let mut paths = std::collections::BTreeSet::new();
    for (path, source) in mappings {
        if !paths.insert(path) || !sources.insert(source.to_string()) {
            return Err(error("WIT imports require distinct paths and source vars"));
        }
        let namespace = source
            .namespace
            .as_deref()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| error("WIT import source vars must be namespace-qualified"))?;
        if source.name.is_empty() || matches!(namespace, "suss.core" | "cljs.core" | "clojure.core")
        {
            return Err(error("WIT import source vars must be outside core"));
        }
    }
    for (key, item) in &resolve.worlds[world].imports {
        match item {
            WorldItem::Type { .. } => continue,
            WorldItem::Interface { id, .. }
                if super::random::Capability::supported(resolve, *id) && mappings.is_empty() =>
            {
                continue;
            }
            WorldItem::Interface { .. } => {
                return Err(error(
                    "Synchronous WIT interface import adapters remain unimplemented",
                ));
            }
            WorldItem::Function(function) => {
                let name = resolve.name_world_key(key);
                let source = mappings
                    .iter()
                    .find(|(path, _)| path == &name)
                    .map(|(_, source)| source.clone())
                    .ok_or_else(|| {
                        error(format!(
                            "WIT import {name} needs an explicit source var mapping"
                        ))
                    })?;
                if function.kind != FunctionKind::Freestanding || function.external_id.is_some() {
                    return Err(error(
                        "Synchronous WIT imports require freestanding functions without external-id",
                    ));
                }
                let mut registry = composite::Registry::new(resolve);
                for ty in function.params.iter().map(|p| p.ty).chain(function.result) {
                    registry.add(ty)?;
                }
                if resolve
                    .wasm_signature(AbiVariant::GuestImport, function)
                    .indirect_params
                {
                    parameters::Layout::new(resolve, function)?;
                }
                imports.push(Import {
                    name,
                    source,
                    wit: function.clone(),
                });
            }
        }
    }
    if imports.len() != mappings.len() {
        return Err(error(
            "WIT import mapping does not name a selected function",
        ));
    }
    Ok(imports)
}

pub(super) fn core(ty: WasmType) -> ValType {
    match ty {
        WasmType::I32 | WasmType::Pointer | WasmType::Length => ValType::I32,
        WasmType::I64 | WasmType::PointerOrI64 => ValType::I64,
        WasmType::F32 => ValType::F32,
        WasmType::F64 => ValType::F64,
    }
}

pub(super) struct Wiring {
    pub closure_new: u32,
    pub binding_set: u32,
    pub calls: Vec<u32>,
}

/// Append imports after existing helpers/initializers, preserving helper indices.
pub(super) fn wire(
    types: &mut TypeSection,
    section: &mut ImportSection,
    first: u32,
    imports: &[Import],
    resolve: &Resolve,
) -> Wiring {
    let mut add = |module: &str, name: &str, params: Vec<ValType>, results: Vec<ValType>| {
        let index = types.len() + runtime_abi::TYPE_COUNT - 1;
        types.ty().function(params, results);
        section.import(module, name, EntityType::Function(index));
    };
    add(
        "suss.runtime",
        "closure-new",
        vec![
            VALUE,
            runtime_abi::invoke_type(),
            ValType::I32,
            ValType::I32,
        ],
        vec![VALUE],
    );
    add(
        "suss.runtime",
        "binding-root-set",
        vec![VALUE; 2],
        vec![VALUE],
    );
    let mut calls = Vec::new();
    for (i, import) in imports.iter().enumerate() {
        let signature = resolve.wasm_signature(AbiVariant::GuestImport, &import.wit);
        add(
            "suss.host",
            &import.name,
            signature.params.into_iter().map(core).collect(),
            signature.results.into_iter().map(core).collect(),
        );
        calls.push(first + 2 + i as u32);
    }
    Wiring {
        closure_new: first,
        binding_set: first + 1,
        calls,
    }
}

pub(super) fn install(
    body: &mut Function,
    imports: &[Import],
    globals: &[Global],
    indices: &std::collections::BTreeMap<Global, u32>,
    wiring: &Wiring,
    first_invoker: u32,
) {
    use Instruction::*;
    for (i, (import, global)) in imports.iter().zip(globals).enumerate() {
        for instruction in [
            GlobalGet(indices[global]),
            runtime_abi::sentinel_value(runtime_abi::NIL),
            RefFunc(first_invoker + i as u32),
            I32Const(import.wit.params.len() as i32),
            I32Const(import.wit.params.len() as i32),
            Call(wiring.closure_new),
            Call(wiring.binding_set),
            Drop,
        ] {
            body.instruction(&instruction);
        }
    }
}

fn emit(body: &mut Function, instructions: &[Instruction<'_>]) {
    for instruction in instructions {
        body.instruction(instruction);
    }
}
fn allocate(body: &mut Function, size: u32, align: u32, local: u32) {
    use Instruction::*;
    emit(
        body,
        &[
            I32Const(0),
            I32Const(0),
            I32Const(align as i32),
            I32Const(size as i32),
            Call(HELPERS + 2),
            LocalSet(local),
        ],
    );
}
fn release(body: &mut Function, local: u32, size: u32) {
    emit(
        body,
        &[
            Instruction::LocalGet(local),
            Instruction::I32Const(size as i32),
            Instruction::Call(HELPERS + 3),
        ],
    );
}

/// Each universal invoker owns its argument buffers across the host call, copies
/// the returned value into guest storage, then retires all transfer allocations.
/// A canonical host trap poisons the instance and requires Store retirement.
pub(super) fn emit_invokers(
    imports: &[Import],
    registry: &composite::Registry,
    functions: &mut FunctionSection,
    code: &mut CodeSection,
    wiring: &Wiring,
    generic_base: u32,
    root: u32,
) -> Result<(), Diagnostic> {
    use Instruction::*;
    for (ordinal, import) in imports.iter().enumerate() {
        functions.function(runtime_abi::INVOKE);
        let nodes = import
            .wit
            .params
            .iter()
            .map(|p| registry.index(p.ty))
            .collect::<Vec<_>>();
        let result = import.wit.result.map(|ty| registry.index(ty));
        let signature = registry
            .resolve
            .wasm_signature(AbiVariant::GuestImport, &import.wit);
        let layout = signature
            .indirect_params
            .then(|| parameters::Layout::new(registry.resolve, &import.wit))
            .transpose()?;
        // Params env0/ARGS1, projected2.., native result, parameter headers,
        // optional outer argument record and the result's temporary header.
        let projected = 2;
        let native = projected + nodes.len() as u32;
        let pointers = native + 1;
        let outer = pointers + nodes.len() as u32;
        let returned = outer + 1;
        let mut body = Function::new([
            (nodes.len() as u32 + 1, VALUE),
            (nodes.len() as u32 + 2, ValType::I32),
        ]);
        for (i, node) in nodes.iter().enumerate() {
            emit(
                &mut body,
                &[
                    LocalGet(1),
                    I32Const(i as i32),
                    ArrayGet(runtime_abi::ARGS),
                    LocalSet(native),
                ],
            );
            composite::schema_call(&mut body, root, *node, true, native);
            emit(
                &mut body,
                &[
                    LocalTee(projected + i as u32),
                    Call(composite::function(
                        generic_base,
                        *node,
                        composite::VALIDATE,
                    )),
                ],
            );
        }
        if let Some(layout) = &layout {
            let align = nodes
                .iter()
                .map(|node| registry.nodes[*node].align)
                .max()
                .unwrap_or(1);
            allocate(&mut body, layout.size, align, outer);
        }
        for (i, node) in nodes.iter().enumerate() {
            let pointer = pointers + i as u32;
            if let Some(layout) = &layout {
                emit(
                    &mut body,
                    &[
                        LocalGet(outer),
                        I32Const(layout.offset(i) as i32),
                        I32Add,
                        LocalSet(pointer),
                    ],
                );
            } else {
                allocate(
                    &mut body,
                    registry.nodes[*node].size,
                    registry.nodes[*node].align,
                    pointer,
                );
            }
            emit(
                &mut body,
                &[
                    LocalGet(projected + i as u32),
                    LocalGet(pointer),
                    Call(composite::function(generic_base, *node, composite::WRITE)),
                ],
            );
        }
        if let Some(node) = result {
            allocate(
                &mut body,
                registry.nodes[node].size,
                registry.nodes[node].align,
                returned,
            );
        }
        if layout.is_some() {
            body.instruction(&LocalGet(outer));
        } else {
            for (i, node) in nodes.iter().enumerate() {
                emit(
                    &mut body,
                    &[
                        LocalGet(pointers + i as u32),
                        Call(composite::function(
                            generic_base,
                            *node,
                            composite::LOAD_FLAT,
                        )),
                    ],
                );
            }
        }
        if signature.retptr {
            body.instruction(&LocalGet(returned));
        }
        body.instruction(&Call(wiring.calls[ordinal]));
        if let Some(node) = result {
            if !signature.retptr {
                emit(
                    &mut body,
                    &[
                        LocalGet(returned),
                        Call(composite::function(
                            generic_base,
                            node,
                            composite::STORE_FLAT,
                        )),
                    ],
                );
            }
            emit(
                &mut body,
                &[
                    LocalGet(returned),
                    Call(composite::function(generic_base, node, composite::COPY)),
                    LocalSet(native),
                    LocalGet(returned),
                    Call(composite::function(generic_base, node, composite::FREE)),
                ],
            );
            release(&mut body, returned, registry.nodes[node].size);
        } else {
            emit(
                &mut body,
                &[
                    runtime_abi::sentinel_value(runtime_abi::NIL),
                    LocalSet(native),
                ],
            );
        }
        for (i, node) in nodes.iter().enumerate() {
            emit(
                &mut body,
                &[
                    LocalGet(pointers + i as u32),
                    Call(composite::function(generic_base, *node, composite::FREE)),
                ],
            );
            if layout.is_none() {
                release(&mut body, pointers + i as u32, registry.nodes[*node].size);
            }
        }
        if let Some(layout) = layout {
            release(&mut body, outer, layout.size);
        }
        emit(&mut body, &[LocalGet(native), End]);
        code.function(&body);
    }
    Ok(())
}
