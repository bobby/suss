//! Original compiler binding facts for compiled macro environments.
//! Retain actual declarations and analyzed initializers; never reconstruct facts
//! by evaluating source or replacing unknown information with a scalar stand-in.
use super::*;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalKind {
    Let,
    Loop,
    Argument { index: usize, rest: bool },
    FunctionName,
    Catch,
}

/// Source adaptation facts, separate from physical parameter/loop lowering.
#[derive(Debug, Clone)]
pub enum SourceRole {
    Plain,
    MethodArgument {
        index: usize,
        rest: bool,
    },
    MethodThis {
        type_declaration: Form,
        /// Original explicit protocol argument; Object receivers have none.
        argument: Option<Arc<LocalBinding>>,
        /// Actual lowered receiver access; inspecting it does not read a value.
        access: Hir,
    },
}

/// Named source function scope; a definition hint does not create a lexical ID.
#[derive(Debug, Clone)]
pub struct FunctionScope {
    pub declaration: Form,
    pub origin: Option<super::super::SourceOrigin>,
    pub namespace: String,
    pub parents: Arc<[Arc<FunctionScope>]>,
    pub self_binding: Option<LocalBinding>,
    pub shadow: Option<Arc<LocalBinding>>,
    pub shadow_field: Option<Arc<FieldBinding>>,
}

#[derive(Debug, Clone)]
pub struct FieldBinding {
    pub declaration: Form,
    pub origin: Option<super::super::SourceOrigin>,
    pub index: usize,
    pub mutable: bool,
    /// Actual lowered access expression; inspecting it does not read the field.
    pub access: Hir,
}

#[derive(Debug, Clone)]
pub struct LocalBinding {
    /// Current lowered binding used for lexical lookup, including loop remapping.
    pub id: BindingId,
    pub ty: Type,
    /// Reader or expansion declaration, preserving source/call-site span and metadata.
    pub declaration: Form,
    pub origin: Option<super::super::SourceOrigin>,
    pub kind: LocalKind,
    pub source_role: SourceRole,
    /// Actual analyzed initializer; absent for parameters/self/catch bindings.
    pub initializer: Option<Arc<Hir>>,
    /// Previous lexical declaration, excluding compiler-only ID remapping.
    pub shadow: Option<Arc<LocalBinding>>,
    pub shadow_field: Option<Arc<FieldBinding>>,
}
impl LocalBinding {
    /// Portable source role without changing the actual lowered binding identity.
    pub fn source_kind(&self) -> LocalKind {
        match &self.source_role {
            SourceRole::Plain => self.kind,
            SourceRole::MethodArgument { index, rest } => LocalKind::Argument {
                index: *index,
                rest: *rest,
            },
            SourceRole::MethodThis { .. } => LocalKind::Let,
        }
    }
}
impl Analyzer<'_> {
    pub(super) fn record_method_roles(
        &mut self,
        parameters: &[Parameter],
        type_declaration: &Form,
        object_method: bool,
    ) {
        for (index, parameter) in parameters.iter().enumerate() {
            let original = self.locals[&parameter.name].clone();
            let role = if index == 0 {
                SourceRole::MethodThis {
                    type_declaration: type_declaration.clone(),
                    argument: (!object_method).then(|| Arc::new(original.clone())),
                    access: self.local(&original.declaration, parameter.id),
                }
            } else {
                SourceRole::MethodArgument {
                    index: index - usize::from(object_method),
                    rest: matches!(original.kind, LocalKind::Argument { rest: true, .. }),
                }
            };
            self.locals
                .get_mut(&parameter.name)
                .expect("method parameter")
                .source_role = role;
        }
    }

    pub(super) fn enter_function_scope(
        &mut self,
        declaration: Option<&Form>,
        self_binding: Option<LocalBinding>,
    ) {
        let Some(declaration) = declaration else {
            return;
        };
        let Kind::Symbol(name) = &declaration.kind else {
            unreachable!("function name");
        };
        let (shadow, shadow_field) = if let Some(binding) = &self_binding {
            (binding.shadow.clone(), binding.shadow_field.clone())
        } else {
            let shadow = self.locals.get(&name.name).cloned().map(Arc::new);
            let field = if shadow.is_none() {
                self.fields.get(&name.name).cloned().map(Arc::new)
            } else {
                None
            };
            (shadow, field)
        };
        self.function_scopes.push(Arc::new(FunctionScope {
            declaration: declaration.clone(),
            origin: self.origin.clone(),
            namespace: self.environment.current_namespace(self.phase).to_owned(),
            parents: self.function_scopes.clone().into(),
            self_binding,
            shadow,
            shadow_field,
        }));
    }

    pub(super) fn insert_local(
        &mut self,
        declaration: &Form,
        id: BindingId,
        ty: Type,
        kind: LocalKind,
        initializer: Option<Hir>,
    ) -> Option<LocalBinding> {
        let Kind::Symbol(name) = &declaration.kind else {
            unreachable!("validated declaration")
        };
        let shadow = self.locals.get(&name.name).cloned().map(Arc::new);
        let shadow_field = if shadow.is_none() {
            self.fields.get(&name.name).cloned().map(Arc::new)
        } else {
            None
        };
        self.locals.insert(
            name.name.clone(),
            LocalBinding {
                id,
                ty,
                declaration: declaration.clone(),
                origin: self.origin.clone(),
                kind,
                source_role: SourceRole::Plain,
                initializer: initializer.map(Arc::new),
                shadow,
                shadow_field,
            },
        )
    }
    pub(super) fn remap_local(&mut self, name: &str, id: BindingId, ty: Type) {
        let record = self
            .locals
            .get_mut(name)
            .expect("existing parameter binding");
        record.id = id;
        record.ty = ty;
    }
}
