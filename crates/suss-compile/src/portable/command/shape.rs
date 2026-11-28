//! Reachable command binding types encoded from the resolved upstream graph.
use super::diagnostic;
use crate::portable::Diagnostic;
use std::collections::{BTreeMap, BTreeSet};
use wasm_encoder::{ComponentTypeSection, ComponentValType, PrimitiveValType};
use wit_parser::{Function, FunctionKind, Resolve, Type, TypeDefKind, TypeId};

/// Types omitted from a reachable command binding stay in the resolved profile;
/// this encoder does not replace them with guessed function signatures.
pub(crate) trait TypeSink {
    fn count(&self) -> u32;
    fn encoder(&mut self) -> wasm_encoder::ComponentTypeEncoder<'_>;
}
impl TypeSink for ComponentTypeSection {
    fn count(&self) -> u32 {
        self.len()
    }
    fn encoder(&mut self) -> wasm_encoder::ComponentTypeEncoder<'_> {
        self.ty()
    }
}
impl TypeSink for wasm_encoder::InstanceType {
    fn count(&self) -> u32 {
        self.type_count()
    }
    fn encoder(&mut self) -> wasm_encoder::ComponentTypeEncoder<'_> {
        self.ty()
    }
}

pub(crate) struct Types<'a, S: TypeSink = ComponentTypeSection> {
    resolve: &'a Resolve,
    base: u32,
    pub section: S,
    defined: BTreeMap<TypeId, u32>,
    active: BTreeSet<TypeId>,
}
impl<'a> Types<'a> {
    pub fn new(resolve: &'a Resolve) -> Self {
        Self {
            resolve,
            base: 0,
            section: ComponentTypeSection::new(),
            defined: BTreeMap::new(),
            active: BTreeSet::new(),
        }
    }

    pub fn flush(&mut self, component: &mut wasm_encoder::Component) {
        if !self.section.is_empty() {
            self.base += self.section.len();
            component.section(&self.section);
            self.section = ComponentTypeSection::new();
        }
    }

    /// Export creates a new public identity. Later parent definitions and
    /// function signatures must refer to that identity, including aliases.
    pub fn export_value(
        &mut self,
        component: &mut wasm_encoder::Component,
        name: &str,
        id: TypeId,
    ) -> Result<(), Diagnostic> {
        let referenced = self.named(id)?;
        self.flush(component);
        let created = self.base;
        self.base += 1;
        let mut exports = wasm_encoder::ComponentExportSection::new();
        exports.export(
            name,
            wasm_encoder::ComponentExportKind::Type,
            referenced,
            None,
        );
        component.section(&exports);
        for index in self.defined.values_mut() {
            if *index == referenced {
                *index = created;
            }
        }
        Ok(())
    }

    /// Structural type imports carry schema identity and require no runtime
    /// provider. They let declared host functions refer to nominal values.
    pub fn import_value(
        &mut self,
        component: &mut wasm_encoder::Component,
        name: &str,
        id: TypeId,
    ) -> Result<(), Diagnostic> {
        let referenced = self.named(id)?;
        self.flush(component);
        let created = self.base;
        self.base += 1;
        let mut imports = wasm_encoder::ComponentImportSection::new();
        imports.import(
            name,
            wasm_encoder::ComponentTypeRef::Type(wasm_encoder::TypeBounds::Eq(referenced)),
        );
        component.section(&imports);
        for index in self.defined.values_mut() {
            if *index == referenced {
                *index = created;
            }
        }
        Ok(())
    }

    pub fn alias_instance_value(
        &mut self,
        component: &mut wasm_encoder::Component,
        instance: u32,
        name: &str,
        id: TypeId,
    ) {
        self.flush(component);
        let created = self.base;
        self.base += 1;
        let mut aliases = wasm_encoder::ComponentAliasSection::new();
        aliases.alias(wasm_encoder::Alias::InstanceExport {
            instance,
            kind: wasm_encoder::ComponentExportKind::Type,
            name,
        });
        component.section(&aliases);
        self.defined.insert(id, created);
    }
}

impl<'a, S: TypeSink> Types<'a, S> {
    pub fn len(&self) -> u32 {
        self.base + self.section.count()
    }
    pub fn existing(&self, id: TypeId) -> Option<u32> {
        self.defined.get(&id).copied()
    }
    pub fn function(&mut self, function: &Function) -> Result<u32, Diagnostic> {
        if !matches!(
            function.kind,
            FunctionKind::Freestanding | FunctionKind::AsyncFreestanding
        ) {
            return Err(diagnostic(
                "Command resource binding needs a resource adapter",
            ));
        }
        self.resolved_function(function)
    }

    /// Encode the resolved signature after a caller has selected its resource
    /// lifetime adapter. Resolution already supplies implicit method receivers.
    pub fn resolved_function(&mut self, function: &Function) -> Result<u32, Diagnostic> {
        let params = function
            .params
            .iter()
            .map(|p| Ok((p.name.as_str(), self.value(p.ty)?)))
            .collect::<Result<Vec<_>, Diagnostic>>()?;
        let result = function.result.map(|ty| self.value(ty)).transpose()?;
        let index = self.len();
        self.section
            .encoder()
            .function()
            .async_(function.kind.is_async())
            .params(params)
            .result(result);
        Ok(index)
    }

    pub fn value(&mut self, ty: Type) -> Result<ComponentValType, Diagnostic> {
        let primitive = match ty {
            Type::Bool => PrimitiveValType::Bool,
            Type::U8 => PrimitiveValType::U8,
            Type::S8 => PrimitiveValType::S8,
            Type::U16 => PrimitiveValType::U16,
            Type::S16 => PrimitiveValType::S16,
            Type::U32 => PrimitiveValType::U32,
            Type::S32 => PrimitiveValType::S32,
            Type::U64 => PrimitiveValType::U64,
            Type::S64 => PrimitiveValType::S64,
            Type::F32 => PrimitiveValType::F32,
            Type::F64 => PrimitiveValType::F64,
            Type::Char => PrimitiveValType::Char,
            Type::String => PrimitiveValType::String,
            Type::ErrorContext => PrimitiveValType::ErrorContext,
            Type::Id(id) => return self.defined(id),
        };
        Ok(primitive.into())
    }

    /// A public interface type needs an actual type index, including aliases
    /// of primitives. Reuse that same identity in its function signatures.
    pub fn named(&mut self, id: TypeId) -> Result<u32, Diagnostic> {
        match self.value(Type::Id(id))? {
            ComponentValType::Type(index) => Ok(index),
            ComponentValType::Primitive(primitive) => {
                let index = self.len();
                self.section.encoder().defined_type().primitive(primitive);
                self.defined.insert(id, index);
                Ok(index)
            }
        }
    }

    fn defined(&mut self, id: TypeId) -> Result<ComponentValType, Diagnostic> {
        if let Some(index) = self.defined.get(&id) {
            return Ok(ComponentValType::Type(*index));
        }
        if !self.active.insert(id) {
            return Err(diagnostic(
                "Recursive command value type needs a recursive adapter",
            ));
        }
        let kind = self.resolve.types[id].kind.clone();
        let index = match kind {
            TypeDefKind::Handle(handle) => {
                let resource = match handle {
                    wit_parser::Handle::Own(id) | wit_parser::Handle::Borrow(id) => {
                        self.named(id)?
                    }
                };
                let index = self.len();
                match handle {
                    wit_parser::Handle::Own(_) => {
                        self.section.encoder().defined_type().own(resource)
                    }
                    wit_parser::Handle::Borrow(_) => {
                        self.section.encoder().defined_type().borrow(resource)
                    }
                }
                index
            }
            TypeDefKind::Type(ty) => {
                let value = self.value(ty)?;
                self.active.remove(&id);
                if let ComponentValType::Type(index) = value {
                    self.defined.insert(id, index);
                }
                return Ok(value);
            }
            TypeDefKind::Map(key, value) => {
                let key = self.value(key)?;
                let value = self.value(value)?;
                let index = self.len();
                self.section.encoder().defined_type().map(key, value);
                index
            }
            TypeDefKind::FixedLengthList(element, length) => {
                let element = self.value(element)?;
                let index = self.len();
                self.section
                    .encoder()
                    .defined_type()
                    .fixed_length_list(element, length);
                index
            }
            TypeDefKind::List(ty) => {
                let ty = self.value(ty)?;
                let index = self.len();
                self.section.encoder().defined_type().list(ty);
                index
            }
            TypeDefKind::Option(ty) => {
                let ty = self.value(ty)?;
                let index = self.len();
                self.section.encoder().defined_type().option(ty);
                index
            }
            TypeDefKind::Result(result) => {
                let ok = result.ok.map(|ty| self.value(ty)).transpose()?;
                let err = result.err.map(|ty| self.value(ty)).transpose()?;
                let index = self.len();
                self.section.encoder().defined_type().result(ok, err);
                index
            }
            TypeDefKind::Tuple(tuple) => {
                let values = tuple
                    .types
                    .iter()
                    .map(|ty| self.value(*ty))
                    .collect::<Result<Vec<_>, _>>()?;
                let index = self.len();
                self.section.encoder().defined_type().tuple(values);
                index
            }
            TypeDefKind::Enum(enumeration) => {
                let index = self.len();
                self.section
                    .encoder()
                    .defined_type()
                    .enum_type(enumeration.cases.iter().map(|case| case.name.as_str()));
                index
            }
            TypeDefKind::Flags(flags) => {
                let index = self.len();
                self.section
                    .encoder()
                    .defined_type()
                    .flags(flags.flags.iter().map(|flag| flag.name.as_str()));
                index
            }
            TypeDefKind::Variant(variant) => {
                let cases = variant
                    .cases
                    .iter()
                    .map(|case| {
                        Ok((
                            case.name.as_str(),
                            case.ty.map(|ty| self.value(ty)).transpose()?,
                        ))
                    })
                    .collect::<Result<Vec<_>, Diagnostic>>()?;
                let index = self.len();
                self.section.encoder().defined_type().variant(cases);
                index
            }
            TypeDefKind::Record(record) => {
                let fields = record
                    .fields
                    .iter()
                    .map(|field| Ok((field.name.as_str(), self.value(field.ty)?)))
                    .collect::<Result<Vec<_>, Diagnostic>>()?;
                let index = self.len();
                self.section.encoder().defined_type().record(fields);
                index
            }
            _ => {
                return Err(diagnostic(
                    "Reachable command value requires a composite/resource adapter",
                ));
            }
        };
        self.active.remove(&id);
        self.defined.insert(id, index);
        Ok(ComponentValType::Type(index))
    }
}

impl<'a> Types<'a, wasm_encoder::InstanceType> {
    pub fn new_instance(resolve: &'a Resolve) -> Self {
        Self {
            resolve,
            base: 0,
            section: wasm_encoder::InstanceType::new(),
            defined: BTreeMap::new(),
            active: BTreeSet::new(),
        }
    }
    pub fn outer_alias(&mut self, id: TypeId, index: u32) {
        let local = self.len();
        self.section.alias(wasm_encoder::Alias::Outer {
            kind: wasm_encoder::ComponentOuterAliasKind::Type,
            count: 1,
            index,
        });
        self.defined.insert(id, local);
    }
    pub fn export_named(&mut self, name: &str, id: TypeId) -> Result<(), Diagnostic> {
        if matches!(self.resolve.types[id].kind, TypeDefKind::Resource) {
            let created = self.len();
            self.section.export(
                name,
                wasm_encoder::ComponentTypeRef::Type(wasm_encoder::TypeBounds::SubResource),
            );
            self.defined.insert(id, created);
            return Ok(());
        }
        let referenced = self.named(id)?;
        let created = self.len();
        self.section.export(
            name,
            wasm_encoder::ComponentTypeRef::Type(wasm_encoder::TypeBounds::Eq(referenced)),
        );
        for index in self.defined.values_mut() {
            if *index == referenced {
                *index = created;
            }
        }
        Ok(())
    }
}

#[cfg(all(test, suss_host, not(suss_generator)))]
mod resource_tests;
