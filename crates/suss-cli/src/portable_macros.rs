//! Source macro functions execute in a separate compiled phase Store.
use crate::{
    portable_macro_data::FormBridge,
    portable_session::{Session, SessionError, SessionValue},
};
use std::collections::BTreeMap;
use suss_compile::portable::{Diagnostic, ExpansionContext, ExpansionHost};
use suss_reader::forms::{read_forms, Form, Kind};
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
        let form = &forms[0];
        let Kind::List(items) = &form.kind else {
            return Err(failure(form, "Expected source defmacro"));
        };
        if items.len() < 3
            || !matches!(&items[0].kind, Kind::Symbol(s) if s.namespace.is_none() && s.name == "defmacro")
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
        let prepared = snapshot.prepare(vec![definition], 0..source.len(), self)?;
        let value = self.session.eval_prepared(prepared)?;
        self.definitions.insert(
            (self.session.current_namespace().into(), name.name.clone()),
            value,
        );
        Ok(())
    }
}
impl ExpansionHost for CompiledMacros {
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
        let namespace = name
            .namespace
            .as_deref()
            .unwrap_or(context.environment.current_namespace(context.phase));
        let Some(function) = self
            .definitions
            .get(&(namespace.into(), name.name.clone()))
            .cloned()
        else {
            return Ok(None);
        };
        let result = (|| -> Result<Form, SessionError> {
            let mut arguments = vec![self.bridge.quote(&mut self.session, form.clone())?];
            for argument in &items[1..] {
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
