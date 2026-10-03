//! Source macro functions execute in a separate compiled phase Store.
use crate::{
    portable_macro_data::FormBridge,
    portable_macro_graph::AnalysisGraph,
    portable_session::{Session, SessionError, SessionValue},
};
use std::collections::BTreeMap;
use suss_compile::portable::{Diagnostic, ExpansionContext, ExpansionHost};
use suss_reader::forms::{Form, Kind, read_forms};
pub struct CompiledMacros {
    session: Session,
    bridge: FormBridge,
    definitions: BTreeMap<(String, String), SessionValue>,
}
fn symbol(name: &str, span: std::ops::Range<usize>) -> Form {
    Form {
        span,
        metadata: vec![],
        kind: Kind::Symbol(suss_reader::Symbol {
            namespace: None,
            name: name.into(),
        }),
    }
}
fn failure(form: &Form, message: &str) -> SessionError {
    SessionError::Compile(Diagnostic {
        span: form.span.clone(),
        message: message.into(),
    })
}
fn add_implicit_arguments(parts: &[Form], form: &Form) -> Result<Vec<Form>, SessionError> {
    let Some(parameters) = parts.first() else {
        return Err(failure(
            form,
            "Macro signature requires parameters and body",
        ));
    };
    let Kind::Vector(names) = &parameters.kind else {
        return Err(failure(parameters, "Macro parameters must be a vector"));
    };
    if parts.len() < 2 {
        return Err(failure(form, "Macro signature requires a body"));
    }
    let mut names = names.clone();
    names.insert(0, symbol("&env", form.span.clone()));
    names.insert(0, symbol("&form", form.span.clone()));
    let mut result = vec![Form {
        span: parameters.span.clone(),
        metadata: parameters.metadata.clone(),
        kind: Kind::Vector(names),
    }];
    result.extend_from_slice(&parts[1..]);
    Ok(result)
}
impl CompiledMacros {
    pub fn new() -> Result<Self, SessionError> {
        let mut session = Session::new_macro()?;
        let bridge = FormBridge::new(&mut session)?;
        Ok(Self {
            session,
            bridge,
            definitions: BTreeMap::new(),
        })
    }
    pub fn current_namespace(&self) -> &str {
        self.session.current_namespace()
    }
    pub fn enter_namespace(&mut self, namespace: &str) -> Result<(), SessionError> {
        self.session.enter_namespace(namespace)
    }
    pub fn define(&mut self, source: &str) -> Result<(), SessionError> {
        let forms = read_forms(source).map_err(|error| {
            SessionError::Compile(Diagnostic {
                span: error.span,
                message: error.message,
            })
        })?;
        if forms.len() != 1 {
            return Err(SessionError::Compile(Diagnostic {
                span: 0..source.len(),
                message: "Expected exactly one source defmacro".into(),
            }));
        }
        self.define_form_with_origin(forms.into_iter().next().unwrap(), 0..source.len(), Some(&suss_compile::portable::SourceOrigin::new(source, None)))
            .map(|_| ())
    }
    fn load_source_namespace(
        &mut self,
        namespace: &str,
        policy: suss_compile::portable::MacroReload,
    ) -> Result<Vec<String>, SessionError> {
        use suss_compile::portable::{modules, resolve::Phase};
        if matches!(namespace, "suss.core" | "cljs.core")
            && policy != suss_compile::portable::MacroReload::Once
        {
            return Err(SessionError::Compile(Diagnostic {
                span: 0..0,
                message: "The compiled macro bootstrap core cannot be source-reloaded".into(),
            }));
        }
        let mut snapshot = self.session.compilation_snapshot();
        match policy {
            suss_compile::portable::MacroReload::Once => {}
            suss_compile::portable::MacroReload::Reload => {
                snapshot.provided.remove(
                    &modules::ModuleIdentity::new(Phase::Macro, namespace)
                        .map_err(SessionError::Compile)?,
                );
            }
            suss_compile::portable::MacroReload::ReloadAll => {
                snapshot
                    .provided
                    .retain(|identity| identity.namespace() == "suss.core");
            }
        }
        let graph = modules::discover_phase_modules(
            namespace,
            &snapshot.source_paths,
            &snapshot.environment,
            Phase::Macro,
            &snapshot.provided,
        )
        .map_err(SessionError::Module)?;
        // Discovery errors preserve previous loaded identities. Once execution
        // starts, each selected phase unit must initialize before being loaded again.
        self.session.invalidate_source_modules(
            &graph
                .iter()
                .map(|unit| unit.identity.clone())
                .collect::<Vec<_>>(),
        );
        let caller = self.session.current_namespace().to_owned();
        let result = (|| {
            for unit in graph {
                let origin = suss_compile::portable::SourceOrigin::new(unit.source.as_str(), Some(unit.path.clone()));
                // Source snapshots are already selected and dependency-first. Each
                // form executes once in Macro phase; definitions compile to the
                // same native function pipeline and become available to later forms.
                for mut form in unit.forms {
                    let definition = matches!(&form.kind, Kind::List(items) if matches!(items.first().map(|head| &head.kind), Some(Kind::Symbol(head)) if self.session.resolves_macro_definition(head)));
                    if definition {
                        let Kind::List(items) = &mut form.kind else {
                            unreachable!()
                        };
                        let Kind::Symbol(head) = &mut items[0].kind else {
                            unreachable!()
                        };
                        head.namespace = Some("suss.core".into());
                        head.name = "defmacro".into();
                        let span = form.span.clone();
                        self.define_form_with_origin(form, span, Some(&origin))?;
                    } else {
                        let span = form.span.clone();
                        let prepared =
                            self.session
                                .compilation_snapshot()
                                .prepare_with_origin(vec![form], span, self, Some(&origin))?;
                        self.session.eval_prepared(prepared)?;
                    }
                }
                self.session
                    .initialized_source_namespace(unit.identity.namespace())?;
            }
            Ok(self
                .definitions
                .keys()
                .filter(|(ns, _)| ns == namespace)
                .map(|(_, name)| name.clone())
                .collect())
        })();
        self.session.enter_namespace(&caller)?;
        result
    }
    pub fn set_operation_fuel(&mut self, fuel: u64) {
        self.session.set_operation_fuel(fuel);
    }
    pub(crate) fn replacement(&self) -> Result<Self, SessionError> {
        let mut session = self.session.replacement()?;
        let bridge = FormBridge::new(&mut session)?;
        Ok(Self {
            session,
            bridge,
            definitions: BTreeMap::new(),
        })
    }
    pub(crate) fn define_form_display(
        &mut self,
        form: Form,
        span: std::ops::Range<usize>,
        origin: Option<&suss_compile::portable::SourceOrigin>,
    ) -> Result<String, SessionError> {
        let value = self.define_form_with_origin(form, span, origin)?;
        crate::portable_repl::display(&mut self.session, &value)
    }
    fn define_form_with_origin(
        &mut self, form: Form, span: std::ops::Range<usize>,
        origin: Option<&suss_compile::portable::SourceOrigin>,
    ) -> Result<SessionValue, SessionError> {
        let form = &form;
        let Kind::List(items) = &form.kind else {
            return Err(failure(form, "Expected source defmacro"));
        };
        if items.len() < 3
            || !matches!(&items[0].kind, Kind::Symbol(s) if s.name == "defmacro" && (s.namespace.is_none() || matches!(s.namespace.as_deref(), Some("suss.core" | "cljs.core"))))
        {
            return Err(failure(form, "Expected defmacro name parameters and body"));
        }
        let Kind::Symbol(name) = &items[1].kind else {
            return Err(failure(&items[1], "Macro name must be a symbol"));
        };
        if name.namespace.is_some() {
            return Err(failure(
                &items[1],
                "Macro definition name must be unqualified",
            ));
        }
        let mut name_form = items[1].clone();
        let mut docstring = None;
        let mut start = 2;
        // Pinned defmacro/defn retain leading docs/attributes before signatures.
        while let Some(item) = items.get(start) {
            match &item.kind {
                Kind::String(_) => docstring = Some(item.clone()),
                Kind::Map(_) => name_form.metadata.push(item.clone()),
                _ => break,
            }
            start += 1;
        }
        let declarations = &items[start..];
        let Some(first) = declarations.first() else {
            return Err(failure(form, "Macro requires at least one signature"));
        };
        let mut function = vec![symbol("fn", form.span.clone())];
        if matches!(first.kind, Kind::Vector(_)) {
            function.extend(add_implicit_arguments(declarations, form)?);
        } else {
            let mut signatures = declarations;
            if let Some(attributes) = signatures
                .last()
                .filter(|item| matches!(item.kind, Kind::Map(_)))
            {
                name_form.metadata.push(attributes.clone());
                signatures = &signatures[..signatures.len() - 1];
            }
            if signatures.is_empty() {
                return Err(failure(form, "Macro requires at least one signature"));
            }
            for signature in signatures {
                let Kind::List(parts) = &signature.kind else {
                    return Err(failure(signature, "Macro signature must be a list"));
                };
                function.push(Form {
                    span: signature.span.clone(),
                    metadata: signature.metadata.clone(),
                    kind: Kind::List(add_implicit_arguments(parts, signature)?),
                });
            }
        }
        let mut definition = vec![symbol("def", form.span.clone()), name_form];
        if let Some(docstring) = docstring {
            definition.push(docstring);
        }
        definition.push(Form {
            span: form.span.clone(),
            metadata: vec![],
            kind: Kind::List(function),
        });
        let definition = Form {
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            kind: Kind::List(definition),
        };
        let snapshot = self.session.compilation_snapshot();
        let prepared = snapshot.prepare_with_origin(vec![definition], span, self, origin)?;
        let value = self.session.eval_prepared(prepared)?;
        let namespace = self.session.current_namespace().to_owned();
        self.definitions
            .insert((namespace.clone(), name.name.clone()), value.clone());
        let exports = self
            .definitions
            .keys()
            .filter(|(ns, _)| ns == &namespace)
            .map(|(_, name)| name.clone())
            .collect::<Vec<_>>();
        self.session.declare_macro_exports(&namespace, &exports)?;
        Ok(value)
    }
}
impl ExpansionHost for CompiledMacros {
    fn supports_macro_imports(&self) -> bool {
        true
    }
    fn source_paths(&mut self, paths: &[std::path::PathBuf]) {
        self.session.set_source_paths(paths);
    }
    fn load_macro_namespace(
        &mut self,
        namespace: &str,
        span: std::ops::Range<usize>,
    ) -> Result<Vec<String>, Diagnostic> {
        self.load_source_namespace(namespace, suss_compile::portable::MacroReload::Once)
            .map_err(|error| Diagnostic {
                span,
                message: format!("Compiled macro namespace loading failed: {error}"),
            })
    }

    fn load_macro_namespace_with_policy(
        &mut self,
        namespace: &str,
        policy: suss_compile::portable::MacroReload,
        span: std::ops::Range<usize>,
    ) -> Result<Vec<String>, Diagnostic> {
        self.load_source_namespace(namespace, policy)
            .map_err(|error| Diagnostic {
                span,
                message: format!("Compiled macro namespace loading failed: {error}"),
            })
    }
    fn expand(
        &mut self,
        form: &Form,
        context: ExpansionContext<'_>,
    ) -> Result<Option<Form>, Diagnostic> {
        let Kind::List(items) = &form.kind else {
            return Ok(None);
        };
        let Some(Form {
            kind: Kind::Symbol(name),
            ..
        }) = items.first()
        else {
            return Ok(None);
        };
        if name.namespace.is_none()
            && matches!(
                name.name.as_str(),
                "quote"
                    | "if"
                    | "do"
                    | "let"
                    | "let*"
                    | "loop"
                    | "loop*"
                    | "recur"
                    | "fn"
                    | "fn*"
                    | "def"
                    | "defonce"
                    | "set!"
                    | "throw"
                    | "try"
                    | "new"
                    | "ns"
            )
        {
            return Ok(None);
        }
        let target = context
            .environment
            .resolve_source_macro(context.phase, name)
            .unwrap_or_else(|| {
                (
                    name.namespace
                        .as_deref()
                        .unwrap_or(context.environment.current_namespace(context.phase))
                        .into(),
                    name.name.clone(),
                )
            });
        let Some(function) = self.definitions.get(&target).cloned() else {
            return Ok(None);
        };
        let result = (|| -> Result<Form, SessionError> {
            let caller_data = context.origin.map_or_else(|| Ok(form.clone()), |origin| origin.macro_form_data(form))
                .map_err(SessionError::Compile)?;
            let caller_form = self.bridge.quote(&mut self.session, caller_data.clone())?;
            let caller_environment = AnalysisGraph::new(&self.bridge, &mut self.session)
                .expansion(context)?;
            let mut arguments = vec![caller_form, caller_environment];
            let Kind::List(caller_items) = &caller_data.kind else { unreachable!("matched macro call") };
            for argument in &caller_items[1..] {
                arguments.push(self.bridge.quote(&mut self.session, argument.clone())?);
            }
            let arguments = arguments.iter().collect::<Vec<_>>();
            let result = self.session.invoke(&function, &arguments)?;
            self.bridge
                .read(&mut self.session, &result, form.span.clone())
        })();
        result.map(Some).map_err(|error| Diagnostic {
            span: form.span.clone(),
            message: format!("Compiled macro expansion failed: {error}"),
        })
    }
}
