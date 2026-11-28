//! Explicit selected-world randomness, using the resolved generic WIT encoder.
use super::{Diagnostic, Resolve, WorldId, error};
use crate::portable::command::shape::Types;
use wasm_encoder::*;
use wit_parser::{FunctionKind, InterfaceId, Type, WorldItem};

pub(super) struct Capability {
    pub name: String,
    pub implements: Option<String>,
    pub external_id: Option<String>,
    id: InterfaceId,
}

impl Capability {
    pub fn selected(resolve: &Resolve, world: WorldId) -> Result<Option<Self>, Diagnostic> {
        let mut capability = None;
        for (key, item) in &resolve.worlds[world].imports {
            let WorldItem::Interface {
                id, external_id, ..
            } = item
            else {
                continue;
            };
            if !Self::supported(resolve, *id) {
                continue;
            }
            if capability.is_some() {
                return Err(error(
                    "Portable AOT accepts one selected randomness provider",
                ));
            }
            capability = Some(Self {
                name: resolve.name_world_key(key),
                implements: resolve.implements_value(key, item),
                external_id: external_id.clone(),
                id: *id,
            });
        }
        Ok(capability)
    }

    pub fn supported(resolve: &Resolve, id: InterfaceId) -> bool {
        // Select only the standard capability identities the native host
        // actually provides. Other imported interfaces keep their diagnostic.
        let Some(identity) = resolve.id_of(id) else {
            return false;
        };
        if !matches!(
            identity.as_str(),
            "wasi:random/random@0.2.4" | "wasi:random/random@0.3.1"
        ) {
            return false;
        }
        let interface = &resolve.interfaces[id];
        if !interface.types.is_empty() || interface.functions.len() != 2 {
            return false;
        }
        let Some(word) = interface.functions.get("get-random-u64") else {
            return false;
        };
        let Some(bytes) = interface.functions.get("get-random-bytes") else {
            return false;
        };
        let bytes_result = bytes.result.is_some_and(|ty| {
            matches!(ty,
            Type::Id(id) if matches!(resolve.types[id].kind,
                wit_parser::TypeDefKind::List(Type::U8)))
        });
        word.kind == FunctionKind::Freestanding
            && word.external_id.is_none()
            && word.params.is_empty()
            && word.result == Some(Type::U64)
            && bytes.kind == FunctionKind::Freestanding
            && bytes.external_id.is_none()
            && bytes.params.len() == 1
            && bytes.params[0].ty == Type::U64
            && bytes_result
    }

    pub fn encode(&self, resolve: &Resolve, types: &mut Types<'_>) -> Result<u32, Diagnostic> {
        let mut instance = InstanceType::new();
        for (index, (name, function)) in resolve.interfaces[self.id].functions.iter().enumerate() {
            let ty = types.function(function)?;
            instance.alias(Alias::Outer {
                kind: ComponentOuterAliasKind::Type,
                count: 1,
                index: ty,
            });
            instance.export(name, ComponentTypeRef::Func(index as u32));
        }
        let ty = types.len();
        types.section.instance(&instance);
        Ok(ty)
    }
}

/// A closed target adapter installs the lowered capability before any source
/// initializer. No GC language values or shared type indices cross this shim.
pub(super) fn wire() -> Vec<u8> {
    let mut types = TypeSection::new();
    types.ty().function([], [ValType::I64]);
    types.ty().function(
        [ValType::Ref(RefType {
            nullable: true,
            heap_type: HeapType::Concrete(0),
        })],
        [],
    );
    types.ty().function([], []);
    let mut imports = ImportSection::new();
    imports.import("suss.random", "word", EntityType::Function(0));
    imports.import(
        "suss.runtime",
        "random-source-install",
        EntityType::Function(1),
    );
    let mut functions = FunctionSection::new();
    functions.function(2);
    let mut elements = ElementSection::new();
    elements.declared(Elements::Functions(std::borrow::Cow::Borrowed(&[0])));
    let mut start = Function::new([]);
    start
        .instruction(&Instruction::RefFunc(0))
        .instruction(&Instruction::Call(1))
        .instruction(&Instruction::End);
    let mut code = CodeSection::new();
    code.function(&start);
    let mut module = Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&StartSection { function_index: 2 })
        .section(&elements)
        .section(&code);
    module.finish()
}
