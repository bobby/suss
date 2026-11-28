//! Schema-only selected interfaces retain their declared type-provider identity.
use super::{Diagnostic, composite, error, random};
use crate::portable::command::shape::Types;
use wasm_encoder::*;
use wit_parser::{Resolve, Type, TypeOwner, WorldId, WorldItem};

pub(super) fn validate(resolve: &Resolve, world: WorldId) -> Result<(), Diagnostic> {
    for item in resolve.worlds[world].imports.values() {
        let WorldItem::Interface { id, .. } = item else {
            continue;
        };
        if random::Capability::supported(resolve, *id) {
            continue;
        }
        for id in resolve.interfaces[*id].types.values() {
            composite::Registry::new(resolve).add(Type::Id(*id))?;
        }
    }
    Ok(())
}

pub(super) fn encode(
    resolve: &Resolve,
    world: WorldId,
    component: &mut Component,
    types: &mut Types<'_>,
) -> Result<u32, Diagnostic> {
    let mut count = 0;
    for (key, item) in &resolve.worlds[world].imports {
        let WorldItem::Interface {
            id, external_id, ..
        } = item
        else {
            continue;
        };
        if random::Capability::supported(resolve, *id) {
            continue;
        }
        let interface = &resolve.interfaces[*id];
        debug_assert!(interface.functions.is_empty());
        let mut instance = Types::<InstanceType>::new_instance(resolve);
        let mut live = wit_parser::LiveTypes::default();
        live.add_interface(resolve, *id);
        for child in live.iter() {
            if let TypeOwner::Interface(owner) = resolve.types[child].owner {
                if owner != *id {
                    let outer = types.existing(child).ok_or_else(|| {
                        error("Imported WIT schema dependency must precede its user")
                    })?;
                    instance.outer_alias(child, outer);
                }
            }
            for (name, declared) in &interface.types {
                if *declared == child {
                    instance.export_named(name, child)?;
                }
            }
        }
        let ty = types.len();
        types.section.instance(&instance.section);
        types.flush(component);
        let name = resolve.name_world_key(key);
        let implements = resolve.implements_value(key, item);
        let mut imports = ComponentImportSection::new();
        imports.import(
            ComponentExternName {
                name: name.as_str().into(),
                implements: implements.as_deref().map(Into::into),
                version_suffix: None,
                external_id: external_id.as_deref().map(Into::into),
            },
            ComponentTypeRef::Instance(ty),
        );
        component.section(&imports);
        for (name, id) in &interface.types {
            types.alias_instance_value(component, count, name, *id);
        }
        count += 1;
    }
    Ok(count)
}
