//! Reachable command binding types encoded from the resolved upstream graph.
use super::diagnostic;
use crate::portable::Diagnostic;
use std::collections::{BTreeMap, BTreeSet};
use wasm_encoder::{ComponentTypeSection, ComponentValType, PrimitiveValType};
use wit_parser::{Function, FunctionKind, Resolve, Type, TypeDefKind, TypeId};

/// Types omitted from a reachable command binding stay in the resolved profile;
/// this encoder does not replace them with guessed function signatures.
pub(super) struct Types<'a> {
    resolve: &'a Resolve,
    pub section: ComponentTypeSection,
    defined: BTreeMap<TypeId, u32>,
    active: BTreeSet<TypeId>,
}
impl<'a> Types<'a> {
    pub fn new(resolve: &'a Resolve) -> Self {
        Self {
            resolve,
            section: ComponentTypeSection::new(),
            defined: BTreeMap::new(),
            active: BTreeSet::new(),
        }
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
        let params = function
            .params
            .iter()
            .map(|p| Ok((p.name.as_str(), self.value(p.ty)?)))
            .collect::<Result<Vec<_>, Diagnostic>>()?;
        let result = function.result.map(|ty| self.value(ty)).transpose()?;
        let index = self.section.len();
        self.section
            .function()
            .async_(function.kind == FunctionKind::AsyncFreestanding)
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
            TypeDefKind::Type(ty) => {
                let value = self.value(ty)?;
                self.active.remove(&id);
                if let ComponentValType::Type(index) = value {
                    self.defined.insert(id, index);
                }
                return Ok(value);
            }
            TypeDefKind::List(ty) => {
                let ty = self.value(ty)?;
                let index = self.section.len();
                self.section.defined_type().list(ty);
                index
            }
            TypeDefKind::Option(ty) => {
                let ty = self.value(ty)?;
                let index = self.section.len();
                self.section.defined_type().option(ty);
                index
            }
            TypeDefKind::Result(result) => {
                let ok = result.ok.map(|ty| self.value(ty)).transpose()?;
                let err = result.err.map(|ty| self.value(ty)).transpose()?;
                let index = self.section.len();
                self.section.defined_type().result(ok, err);
                index
            }
            TypeDefKind::Tuple(tuple) => {
                let values = tuple
                    .types
                    .iter()
                    .map(|ty| self.value(*ty))
                    .collect::<Result<Vec<_>, _>>()?;
                let index = self.section.len();
                self.section.defined_type().tuple(values);
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
