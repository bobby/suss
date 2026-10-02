//! Original ABI adaptation of compiler-owned callable signatures. The pinned
//! compiler's emit-apply-to takes only the fixed prefix, preserving the rest seq.
use super::*;

impl Analyzer<'_> {
    fn signature_core_call(
        &mut self,
        form: &Form,
        name: &str,
        argument: Hir,
    ) -> Result<Hir, Diagnostic> {
        let symbol = suss_reader::Symbol {
            namespace: Some("suss.core".into()),
            name: name.into(),
        };
        let (kind, ty) = self.global_value(&symbol, form.span.clone())?;
        let callee = Hir {
            span: form.span.clone(),
            metadata: vec![],
            kind,
            ty,
        };
        Ok(Hir {
            span: form.span.clone(),
            metadata: vec![],
            ty: Type::Value,
            kind: Expression::Call {
                callee: Box::new(self.prepare_source_callee(form, callee, 1)?),
                arguments: vec![argument],
            },
        })
    }

    pub(super) fn attach_callable_signatures(
        &mut self,
        form: &Form,
        function: Hir,
    ) -> Result<Hir, Diagnostic> {
        let Expression::GeneralFunction {
            methods,
            self_binding,
            ..
        } = &function.kind
        else {
            return Ok(function);
        };
        let methods = methods.clone();
        let self_binding = self_binding.clone();
        let owner = self.fresh_binding(form, function);
        let mut bindings = vec![owner.clone()];
        if let Some(parameter) = self_binding {
            bindings.push(Binding {
                id: parameter.id,
                name: parameter.name,
                span: parameter.span,
                metadata: parameter.metadata,
                value: self.local(form, owner.id),
            });
        }
        let mut effects = vec![];
        for method in methods {
            let fixed = method.parameters.len() - usize::from(method.variadic);
            let bound = method
                .parameters
                .iter()
                .map(|parameter| parameter.id)
                .collect();
            let mut captures = BTreeSet::new();
            free_bindings(&method.body, &bound, &mut captures);
            let delegate = Hir {
                span: form.span.clone(),
                metadata: vec![],
                ty: Type::Closure(method.parameters.len()),
                kind: Expression::Function {
                    parameters: method.parameters,
                    body: method.body,
                    captures: captures.into_iter().collect(),
                },
            };
            let delegate = self.fresh_binding(form, delegate);
            bindings.push(delegate.clone());
            let key = if method.variadic {
                "cljs$core$IFn$_invoke$arity$variadic".into()
            } else {
                format!("cljs$core$IFn$_invoke$arity${fixed}")
            };
            let key = self.literal_form(form, Literal::String(key.encode_utf16().collect()));
            effects.push(self.nominal(
                form,
                Nominal::NamedSet,
                vec![
                    self.local(form, owner.id),
                    key,
                    self.local(form, delegate.id),
                ],
            ));
            if !method.variadic {
                continue;
            }
            let key = self.literal_form(
                form,
                Literal::String("cljs$lang$maxFixedArity".encode_utf16().collect()),
            );
            let count = self.literal_form(form, Literal::Number(fixed as f64));
            effects.push(self.nominal(
                form,
                Nominal::NamedSet,
                vec![self.local(form, owner.id), key, count],
            ));
            let parameter = Parameter {
                id: BindingId(self.next),
                name: "$applyToArgs".into(),
                span: form.span.clone(),
                metadata: vec![],
            };
            self.next += 1;
            let mut prefix = vec![];
            let mut arguments = vec![];
            let mut current = parameter.id;
            for i in 0..fixed {
                let value = self.signature_core_call(form, "first", self.local(form, current))?;
                let argument = self.fresh_binding(form, value);
                arguments.push(self.local(form, argument.id));
                prefix.push(argument);
                let value = self.signature_core_call(
                    form,
                    if i + 1 == fixed { "rest" } else { "next" },
                    self.local(form, current),
                )?;
                let next = self.fresh_binding(form, value);
                current = next.id;
                prefix.push(next);
            }
            let rest = if fixed == 0 {
                self.signature_core_call(form, "seq", self.local(form, current))?
            } else {
                self.local(form, current)
            };
            arguments.push(rest);
            let call = Hir {
                span: form.span.clone(),
                metadata: vec![],
                ty: Type::Value,
                kind: Expression::Call {
                    callee: Box::new(self.local(form, delegate.id)),
                    arguments,
                },
            };
            let body = Box::new(Hir {
                span: form.span.clone(),
                metadata: vec![],
                ty: Type::Value,
                kind: Expression::Let {
                    bindings: prefix,
                    body: Box::new(call),
                },
            });
            let mut captures = BTreeSet::new();
            free_bindings(&body, &BTreeSet::from([parameter.id]), &mut captures);
            let callback = Hir {
                span: form.span.clone(),
                metadata: vec![],
                ty: Type::Closure(1),
                kind: Expression::Function {
                    parameters: vec![parameter],
                    body,
                    captures: captures.into_iter().collect(),
                },
            };
            let key = self.literal_form(
                form,
                Literal::String("cljs$lang$applyTo".encode_utf16().collect()),
            );
            effects.push(self.nominal(
                form,
                Nominal::NamedSet,
                vec![self.local(form, owner.id), key, callback],
            ));
        }
        effects.push(self.local(form, owner.id));
        let body = Hir {
            span: form.span.clone(),
            metadata: vec![],
            ty: Type::Value,
            kind: Expression::Do(effects),
        };
        Ok(Hir {
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Value,
            kind: Expression::Let {
                bindings,
                body: Box::new(body),
            },
        })
    }
}
