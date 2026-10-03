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
    FunctionName {
        variadic: bool,
        /// Absent on the initial self binding; present only after parameter staging.
        methods: Option<Arc<SourceFunctionParameters>>,
    },
    PrivateCatch {
        anchor: std::ops::Range<usize>,
    },
    CatchBinding {
        hidden: Arc<LocalBinding>,
        access: Hir,
    },
    MethodArgument {
        index: usize,
        rest: bool,
    },
    MethodThis {
        type_declaration: Form,
        namespace: String,
        protocol_receiver: bool,
        /// Actual source receiver form, independent of a same-named last formal.
        receiver_declaration: Form,
        /// Visible source argument shadow; Object receivers may shadow a user arg.
        argument: Option<Arc<LocalBinding>>,
        /// Actual lowered receiver access; inspecting it does not read a value.
        access: Hir,
    },
}

/// Named source function scope; a definition hint does not create a lexical ID.
#[derive(Debug, Clone)]
pub struct FunctionScope {
    /// Actual function syntax before any method body expansion or lowering.
    pub function_form: Form,
    pub declaration: Form,
    pub origin: Option<super::super::SourceOrigin>,
    pub namespace: String,
    pub declaration_context: super::super::AnalysisContext,
    /// Actual declaration environment, before the new self name is installed.
    pub scope: Arc<SourceNamespace>,
    pub namespace_snapshot: Arc<SourceNamespace>,
    pub locals: Arc<HashMap<String, LocalBinding>>,
    pub fields: Arc<HashMap<String, FieldBinding>>,
    pub phase: Phase,
    pub parents: Arc<[Arc<FunctionScope>]>,
    pub self_binding: Option<LocalBinding>,
    pub shadow: Option<Arc<LocalBinding>>,
    pub shadow_field: Option<Arc<FieldBinding>>,
}

#[derive(Debug, Clone)]
pub struct FieldBinding {
    /// Shared declaration identity across immutable snapshot clones.
    pub identity: Arc<()>,
    pub declaration: Form,
    pub origin: Option<super::super::SourceOrigin>,
    pub index: usize,
    pub mutable: bool,
    /// Actual lowered access expression; inspecting it does not read the field.
    pub access: Hir,
}

#[derive(Debug, Clone)]
pub struct LocalBinding {
    /// Shared declaration identity; lowering remaps do not invent declarations.
    pub identity: Arc<()>,
    /// Current lowered binding used for lexical lookup, including loop remapping.
    pub id: BindingId,
    pub ty: Type,
    /// Reader or expansion declaration, preserving source/call-site span and metadata.
    pub declaration: Form,
    pub origin: Option<super::super::SourceOrigin>,
    pub kind: LocalKind,
    pub declaration_context: super::super::AnalysisContext,
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
            SourceRole::Plain
            | SourceRole::FunctionName { .. }
            | SourceRole::PrivateCatch { .. } => self.kind,
            SourceRole::CatchBinding { .. } => LocalKind::Let,
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
        declarations: &[Form],
        type_declaration: &Form,
        object_method: bool,
        receiver_argument: Option<LocalBinding>,
    ) {
        fn object_argument_shadow(
            shadow: Option<Arc<LocalBinding>>,
            parameters: &[Parameter],
        ) -> Option<Arc<LocalBinding>> {
            let mut binding = (*shadow?).clone();
            if binding.id == parameters[0].id {
                return object_argument_shadow(binding.shadow, parameters);
            }
            if let Some(index) = parameters
                .iter()
                .position(|parameter| parameter.id == binding.id)
            {
                binding.source_role = SourceRole::MethodArgument {
                    index: index - 1,
                    rest: matches!(binding.kind, LocalKind::Argument { rest: true, .. }),
                };
                binding.shadow = object_argument_shadow(binding.shadow, parameters);
            }
            Some(Arc::new(binding))
        }
        // Last same-named user parameter wins, then source this-as binds the
        // receiver outside the body. Do not overwrite that let with an arg role.
        for (index, parameter) in parameters.iter().enumerate().skip(1) {
            let binding = self
                .locals
                .get_mut(&parameter.name)
                .expect("method parameter");
            binding.source_role = SourceRole::MethodArgument {
                index: index - usize::from(object_method),
                rest: matches!(binding.kind, LocalKind::Argument { rest: true, .. }),
            };
            if object_method {
                binding.shadow = object_argument_shadow(binding.shadow.take(), parameters);
            }
        }
        let receiver = &parameters[0];
        let repeated = parameters[1..]
            .iter()
            .any(|parameter| parameter.name == receiver.name);
        let argument = if repeated {
            let mut argument = receiver_argument.expect("visible receiver-name argument");
            let LocalKind::Argument { index, rest } = argument.kind else {
                unreachable!()
            };
            argument.source_role = SourceRole::MethodArgument {
                index: index - usize::from(object_method),
                rest,
            };
            // Object's physical receiver is not a source formal in its fn.
            // Preserve any true outer shadow behind that implicit parameter.
            if object_method {
                argument.shadow = object_argument_shadow(argument.shadow.take(), parameters);
            }
            Some(Arc::new(argument))
        } else if !object_method {
            let mut argument = self.locals[&receiver.name].clone();
            argument.source_role = SourceRole::Plain;
            Some(Arc::new(argument))
        } else {
            None
        };
        let role = SourceRole::MethodThis {
            type_declaration: type_declaration.clone(),
            namespace: self.environment.current_namespace(self.phase).to_owned(),
            protocol_receiver: !object_method,
            receiver_declaration: declarations[0].clone(),
            argument,
            access: self.local(&declarations[0], receiver.id),
        };
        self.locals
            .get_mut(&receiver.name)
            .expect("method receiver")
            .source_role = role;
    }

    pub(super) fn enter_function_scope(
        &mut self,
        function_form: &Form,
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
        let mut locals = self.locals.clone();
        if let Some(binding) = &self_binding {
            if let Some(shadow) = &binding.shadow {
                locals.insert(name.name.clone(), (**shadow).clone());
            } else {
                locals.remove(&name.name);
            }
        }
        let scope = self.capture_source_namespace();
        self.function_scopes.push(Arc::new(FunctionScope {
            function_form: function_form.clone(),
            declaration: declaration.clone(),
            origin: self.origin.clone(),
            namespace: self.environment.current_namespace(self.phase).to_owned(),
            declaration_context: self
                .analysis_contexts
                .last()
                .copied()
                .expect("source analysis context"),
            scope,
            namespace_snapshot: self.namespace_snapshot.as_ref().expect("top-level source snapshot").clone(),
            locals: Arc::new(locals),
            fields: Arc::new(self.fields.clone()),
            phase: self.phase,
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
                identity: Arc::new(()),
                id,
                ty,
                declaration: declaration.clone(),
                origin: self.origin.clone(),
                kind,
                declaration_context: self
                    .analysis_contexts
                    .last()
                    .copied()
                    .expect("source analysis context"),
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
