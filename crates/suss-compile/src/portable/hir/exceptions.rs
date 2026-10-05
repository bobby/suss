//! Original source handler regions, informed by pinned analyzer.cljc, not copied.
use super::*;

impl Analyzer<'_> {
    pub(super) fn exception_region(
        &self,
        form: &Form,
        parameters: Vec<Parameter>,
        body: Hir,
    ) -> Hir {
        let bound = parameters.iter().map(|parameter| parameter.id).collect();
        let mut free = BTreeSet::new();
        free_bindings(&body, &bound, &mut free);
        Hir {
            source: None,
            span: form.span.clone(),
            metadata: Vec::new(),
            ty: Type::Closure(parameters.len()),
            kind: Expression::Function {
                parameters,
                captures: free.into_iter().collect(),
                body: Box::new(body),
            },
        }
    }
    pub(super) fn try_form(
        &mut self,
        form: &Form,
        args: &[Form],
        context: super::super::AnalysisContext,
    ) -> Result<Hir, Diagnostic> {
        let locals = self.locals.clone();
        let target = self.target.take();
        let aliases = self.catch_aliases.clone();
        let result = self.try_regions(form, args, context);
        self.locals = locals;
        self.target = target;
        self.catch_aliases = aliases;
        result
    }
    fn try_regions(
        &mut self,
        form: &Form,
        args: &[Form],
        context: super::super::AnalysisContext,
    ) -> Result<Hir, Diagnostic> {
        fn clause(form: &Form) -> Option<(&str, &[Form])> {
            let Kind::List(items) = &form.kind else {
                return None;
            };
            let Some(Form {
                kind: Kind::Symbol(name),
                ..
            }) = items.first()
            else {
                return None;
            };
            (name.namespace.is_none() && matches!(name.name.as_str(), "catch" | "finally"))
                .then_some((name.name.as_str(), &items[1..]))
        }
        let body_end = args
            .iter()
            .position(|arg| clause(arg).is_some())
            .unwrap_or(args.len());
        let mut catches = Vec::new();
        let mut cleanup = None;
        let mut saw_default = false;
        for argument in &args[body_end..] {
            match clause(argument) {
                Some(("catch", items)) if cleanup.is_none() && !saw_default => {
                    if items.len() < 2 {
                        return Err(fail(
                            argument.span.clone(),
                            "catch requires a type and local name",
                        ));
                    }
                    let default = matches!(&items[0].kind, Kind::Keyword(name) if name.namespace.is_none() && name.name == "default");
                    let Kind::Symbol(name) = &items[1].kind else {
                        return Err(fail(
                            items[1].span.clone(),
                            "catch local requires an unqualified symbol",
                        ));
                    };
                    if name.namespace.is_some() || name.name == "&" {
                        return Err(fail(
                            items[1].span.clone(),
                            "catch local requires an unqualified symbol",
                        ));
                    }
                    saw_default = default;
                    catches.push((argument, items, default));
                }
                Some(("finally", items)) if cleanup.is_none() => {
                    cleanup = Some((argument, items));
                }
                _ => {
                    return Err(fail(
                        argument.span.clone(),
                        "Invalid form or ordering after try handlers",
                    ));
                }
            }
        }
        // Pinned analyzer parse-try visits finally, catches, then body during
        // macro expansion. Runtime regions remain body/handler/cleanup below.
        // A bare try preserves expression context; handlers introduce return.
        let context = if catches.is_empty() && cleanup.is_none() {
            context
        } else {
            context.returning()
        };
        // Build and analyze genuine source regions before packaging closures.
        // The generated forms are originals; no source tree is recovered from
        // a runtime handler or re-analyzed after lowering.
        fn call(name: &str, arguments: Vec<Form>, span: Range<usize>) -> Form {
            let mut items = vec![Form {
                kind: Kind::Symbol(name.split_once('/').map_or_else(
                    || suss_reader::Symbol::new(name),
                    |(namespace, name)| suss_reader::Symbol::namespaced(namespace, name),
                )),
                span: span.clone(),
                metadata: vec![],
            }];
            items.extend(arguments);
            Form {
                kind: Kind::List(items),
                span,
                metadata: vec![],
            }
        }
        let source_cleanup = if let Some((cleanup, forms)) = cleanup {
            Some(self.analyzed_body(
                forms,
                cleanup.span.clone(),
                super::super::AnalysisContext::Statement,
                false,
            )?)
        } else {
            None
        };
        let cleanup = if let Some(body) = &source_cleanup {
            self.exception_region(form, vec![], body.clone())
        } else {
            self.literal_form(form, Literal::Nil)
        };
        let payload = if catches.is_empty() {
            None
        } else {
            let id = BindingId(self.next);
            self.next += 1;
            let mut hidden_name = format!("$exception{}", id.0);
            while self.locals.contains_key(&hidden_name)
                || self.fields.contains_key(&hidden_name)
                || catches.iter().any(|(_, items, _)| {
                    matches!(&items[1].kind,
                    Kind::Symbol(name) if name.name == hidden_name)
                })
            {
                hidden_name.push('$');
            }
            let declaration = Form {
                span: form.span.clone(),
                metadata: vec![],
                kind: Kind::Symbol(suss_reader::Symbol::new(&hidden_name)),
            };
            self.insert_local(&declaration, id, Type::Value, LocalKind::Catch, None);
            self.locals.get_mut(&hidden_name).unwrap().source_role = SourceRole::PrivateCatch {
                anchor: form.span.clone(),
            };
            Some(std::sync::Arc::new(self.locals[&hidden_name].clone()))
        };
        let exception = payload.as_ref().map_or_else(
            || Form {
                kind: Kind::Nil,
                span: form.span.clone(),
                metadata: vec![],
            },
            |payload| payload.declaration.clone(),
        );
        let mut handler_form = call("throw", vec![exception.clone()], form.span.clone());
        for (catch, items, default) in catches.into_iter().rev() {
            let hidden = payload.as_ref().expect("catch payload").clone();
            self.catch_aliases.push((items[1].clone(), hidden));
            let bindings = Form {
                kind: Kind::Vector(vec![items[1].clone(), exception.clone()]),
                span: catch.span.clone(),
                metadata: vec![],
            };
            let mut forms = vec![bindings];
            forms.extend_from_slice(&items[2..]);
            let body = call("let*", forms, catch.span.clone());
            handler_form = if default {
                body
            } else {
                let test = call(
                    "cljs.core/instance?",
                    vec![items[0].clone(), exception.clone()],
                    catch.span.clone(),
                );
                call("if", vec![test, body, handler_form], catch.span.clone())
            };
        }
        let source_handler = self.form_in(&handler_form, context, false)?;
        let handler = if let Some(payload) = &payload {
            let Kind::Symbol(name) = &payload.declaration.kind else {
                unreachable!()
            };
            self.locals.remove(&name.name);
            let parameter = Parameter {
                id: payload.id,
                name: name.name.clone(),
                metadata: vec![],
                span: payload.declaration.span.clone(),
            };
            self.exception_region(form, vec![parameter], source_handler.clone())
        } else {
            self.literal_form(form, Literal::Nil)
        };
        let source_body =
            self.analyzed_body(&args[..body_end], form.span.clone(), context, false)?;
        let body = self.exception_region(form, vec![], source_body.clone());
        *self.source_nodes.last_mut().expect("source node fact slot") =
            Some(std::sync::Arc::new(SourceNode::Try {
                body: std::sync::Arc::new(source_body),
                handler: std::sync::Arc::new(source_handler),
                cleanup: source_cleanup.map(std::sync::Arc::new),
                payload,
            }));
        Ok(Hir {
            source: None,
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Value,
            kind: Expression::Try {
                regions: [Box::new(body), Box::new(handler), Box::new(cleanup)],
            },
        })
    }
}
