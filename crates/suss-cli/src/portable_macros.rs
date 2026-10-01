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
        if items.len() < 4
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
        let Kind::Vector(parameters) = &items[2].kind else {
            return Err(failure(&items[2], "Macro parameters must be a vector"));
        };
        let mut parameters = parameters.clone();
        parameters.insert(0, symbol("&form", form.span.clone()));
        let mut function = vec![
            symbol("fn", form.span.clone()),
            Form {
                span: items[2].span.clone(),
                metadata: items[2].metadata.clone(),
                kind: Kind::Vector(parameters),
            },
        ];
        function.extend_from_slice(&items[3..]);
        let definition = Form {
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            kind: Kind::List(vec![
                symbol("def", form.span.clone()),
                items[1].clone(),
                Form {
                    span: form.span.clone(),
                    metadata: vec![],
                    kind: Kind::List(function),
                },
            ]),
        };
        let value = self.session.eval_forms(vec![definition], 0..source.len())?;
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
