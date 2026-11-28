//! Initial supported resource domain. Keep untracked alias/lifetime paths gated.
use super::super::{Diagnostic, error};
use std::collections::BTreeSet;
use wit_parser::{
    FunctionKind, InterfaceId, Resolve, Type, TypeDefKind, TypeId, TypeOwner, WorldId, WorldItem,
};

pub(crate) struct Resource {
    pub id: TypeId,
    pub name: String,
}
pub(crate) struct Plan {
    pub resources: Vec<Resource>,
}
fn resource_id(resolve: &Resolve, mut id: TypeId) -> Option<TypeId> {
    for _ in 0..64 {
        match resolve.types[id].kind {
            TypeDefKind::Resource => return Some(id),
            TypeDefKind::Type(Type::Id(next)) => id = next,
            _ => return None,
        }
    }
    None
}
fn has_handle(resolve: &Resolve, ty: Type, own: Option<bool>) -> bool {
    let Type::Id(id) = ty else { return false };
    let mut live = wit_parser::LiveTypes::default();
    live.add_type_id(resolve, id);
    live.iter().any(|id| match resolve.types[id].kind {
        TypeDefKind::Handle(wit_parser::Handle::Own(_)) => own != Some(false),
        TypeDefKind::Handle(wit_parser::Handle::Borrow(_)) => own != Some(true),
        _ => false,
    })
}
impl Plan {
    pub fn selected(resolve: &Resolve, world: WorldId) -> Result<Option<Self>, Diagnostic> {
        let mut resources = Vec::new();
        let mut owners = BTreeSet::<InterfaceId>::new();
        let mut ids = BTreeSet::<TypeId>::new();
        for (key, item) in &resolve.worlds[world].imports {
            let WorldItem::Interface { id, .. } = item else {
                continue;
            };
            let mut live = wit_parser::LiveTypes::default();
            live.add_interface(resolve, *id);
            let live_resources = live
                .iter()
                .filter(|id| matches!(resolve.types[*id].kind, TypeDefKind::Resource))
                .collect::<Vec<_>>();
            if live_resources.is_empty() {
                continue;
            }
            if !owners.insert(*id) {
                return Err(error(
                    "Distinct aliases of a resource provider require contextual resource identities",
                ));
            }
            for resource in live_resources {
                if resolve.types[resource].owner != TypeOwner::Interface(*id) {
                    return Err(error(
                        "Foreign resource dependencies require contextual resource identities",
                    ));
                }
                if ids.insert(resource) {
                    resources.push(Resource {
                        id: resource,
                        name: format!(
                            "{}#{}",
                            resolve.name_world_key(key),
                            resolve.types[resource]
                                .name
                                .as_deref()
                                .unwrap_or("resource")
                        ),
                    });
                }
            }
        }
        if resources.is_empty() {
            return Ok(None);
        }
        let plan = Self { resources };
        let mut functions = 0;
        for item in resolve.worlds[world].imports.values() {
            let funcs = match item {
                WorldItem::Function(function) => vec![function],
                WorldItem::Interface { id, .. } => {
                    resolve.interfaces[*id].functions.values().collect()
                }
                _ => continue,
            };
            for function in funcs {
                functions += 1;
                match function.kind {
                    FunctionKind::Freestanding => {}
                    FunctionKind::Constructor(id)
                    | FunctionKind::Method(id)
                    | FunctionKind::Static(id)
                        if plan.ordinal(resolve, id).is_some() => {}
                    _ => {
                        return Err(error(
                            "Asynchronous resource function lifetimes remain unimplemented",
                        ));
                    }
                }
                if function
                    .result
                    .is_some_and(|ty| has_handle(resolve, ty, Some(false)))
                {
                    return Err(error(
                        "Borrowed resource results require a scoped borrow adapter",
                    ));
                }
            }
        }
        if functions == 0 {
            return Err(error(
                "Imported resource-only provider lifetime adapters remain unimplemented",
            ));
        }
        for item in resolve.worlds[world].exports.values() {
            let funcs = match item {
                WorldItem::Function(function) => vec![function],
                WorldItem::Interface { id, .. } => {
                    resolve.interfaces[*id].functions.values().collect()
                }
                _ => continue,
            };
            for function in funcs {
                if function
                    .params
                    .iter()
                    .map(|p| p.ty)
                    .chain(function.result)
                    .any(|ty| has_handle(resolve, ty, None))
                {
                    return Err(error(
                        "Exported resource handles require call-scope ownership adapters",
                    ));
                }
            }
        }
        Ok(Some(plan))
    }
    pub fn ordinal(&self, resolve: &Resolve, id: TypeId) -> Option<u32> {
        let id = resource_id(resolve, id)?;
        self.resources
            .iter()
            .position(|r| r.id == id)
            .map(|i| i as u32)
    }
    pub fn declaration(&self, resolve: &Resolve, id: TypeId) -> bool {
        self.ordinal(resolve, id).is_some()
    }
}
