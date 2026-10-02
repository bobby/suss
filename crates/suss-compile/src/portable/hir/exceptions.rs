//! Original source handler regions, informed by pinned analyzer.cljc, not copied.
use super::*;

impl Analyzer<'_> {
    pub(super) fn exception_region(&self, form: &Form, parameters: Vec<Parameter>, body: Hir) -> Hir {
        let bound = parameters.iter().map(|parameter| parameter.id).collect();
        let mut free = BTreeSet::new();
        free_bindings(&body, &bound, &mut free);
        Hir {
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
        let result = self.try_regions(form, args, context.returning());
        self.locals = locals;
        self.target = target;
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
        let body = self.body(&args[..body_end], form.span.clone(), context, false)?;
        let body = self.exception_region(form, vec![], body);
        let handler = if catches.is_empty() {
            self.literal_form(form, Literal::Nil)
        } else {
            let id = BindingId(self.next);
            self.next += 1;
            let parameter = Parameter {
                id,
                name: format!("$exception{}", id.0),
                metadata: Vec::new(),
                span: form.span.clone(),
            };
            let payload = self.local(form, id);
            let mut selected = Hir {
                span: form.span.clone(),
                metadata: Vec::new(),
                ty: Type::Value,
                kind: Expression::Throw(Box::new(payload.clone())),
            };
            // Analyze in source order so declarations/resolution stay deterministic.
            let mut analyzed = Vec::new();
            for (catch, items, default) in catches {
                let test = if default {
                    None
                } else {
                    let class = self.form(&items[0])?;
                    Some(self.nominal(catch, Nominal::Instance, vec![class, payload.clone()]))
                };
                let Kind::Symbol(name) = &items[1].kind else {
                    unreachable!()
                };
                let previous = self.insert_local(&items[1], id, Type::Value, LocalKind::Catch, None);
                let body = self.body(&items[2..], catch.span.clone(), context, false);
                if let Some(previous) = previous {
                    self.locals.insert(name.name.clone(), previous);
                } else {
                    self.locals.remove(&name.name);
                }
                analyzed.push((catch, test, body?));
            }
            for (catch, test, body) in analyzed.into_iter().rev() {
                selected = if let Some(test) = test {
                    Hir {
                        span: catch.span.clone(),
                        metadata: Vec::new(),
                        ty: Type::Value,
                        kind: Expression::If {
                            condition: Box::new(test),
                            consequent: Box::new(body),
                            alternative: Box::new(selected),
                        },
                    }
                } else {
                    body
                };
            }
            self.exception_region(form, vec![parameter], selected)
        };
        let cleanup = if let Some((cleanup, forms)) = cleanup {
            let body = self.body(forms, cleanup.span.clone(), super::super::AnalysisContext::Statement, false)?;
            self.exception_region(cleanup, vec![], body)
        } else {
            self.literal_form(form, Literal::Nil)
        };
        Ok(Hir {
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Value,
            kind: Expression::Try {
                regions: [Box::new(body), Box::new(handler), Box::new(cleanup)],
            },
        })
    }
}
