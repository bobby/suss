//! Original Suss lowering of Clojure letfn syntax; no upstream body is copied.
use super::*;

impl Analyzer<'_> {
    /// Bootstrap macro expansion: preserve declarations and original method
    /// forms; only the primitive binding vector and core fn call are introduced.
    pub(super) fn letfn_expansion(&mut self, form: &Form) -> Result<Form, Diagnostic> {
        let span = form.span.clone();
        let mut meter = |work| self.expander.charge_expansion_work(work, span.clone());
        let mut expanded = super::super::clone_form_metered(form, &mut meter)?;
        let Kind::List(items) = &mut expanded.kind else { unreachable!() };
        let Some(declarations) = items.get_mut(1) else {
            return Err(fail(form.span.clone(), "letfn requires a binding vector"));
        };
        let Kind::Vector(entries) = &mut declarations.kind else {
            return Err(fail(declarations.span.clone(), "letfn bindings must be a vector"));
        };
        let mut bindings = Vec::with_capacity(entries.len().saturating_mul(2));
        for mut entry in std::mem::take(entries) {
            meter(3)?;
            let Kind::List(parts) = &mut entry.kind else {
                return Err(fail(entry.span.clone(), "letfn declaration requires a name and signatures"));
            };
            let Some(name) = parts.first() else {
                return Err(fail(entry.span.clone(), "letfn declaration requires a name"));
            };
            let name = super::super::clone_form_metered(name, &mut meter)?;
            parts.insert(0, Form {
                span: entry.span.clone(), metadata: Vec::new(),
                kind: Kind::Symbol(suss_reader::Symbol { namespace: Some("suss.core".into()), name: "fn".into() }),
            });
            bindings.push(name);
            bindings.push(entry);
        }
        items[1].kind = Kind::Vector(bindings);
        Ok(expanded)
    }

    pub(super) fn recursive_functions(&mut self, form: &Form, args: &[Form],
        context: super::super::AnalysisContext, tail: bool) -> Result<Hir, Diagnostic>
    {
        let Some(declarations) = args.first() else {
            return Err(fail(form.span.clone(), "letfn* requires a binding vector"));
        };
        let Kind::Vector(entries) = &declarations.kind else {
            return Err(fail(declarations.span.clone(), "letfn* bindings must be a vector"));
        };
        if entries.len() % 2 != 0 {
            return Err(fail(declarations.span.clone(), "letfn* binding has no initializer"));
        }
        let outer = self.locals.clone();
        let mut declarations = Vec::with_capacity(entries.len() / 2);
        for pair in entries.chunks_exact(2) {
            let Kind::Symbol(name) = &pair[0].kind else {
                return Err(fail(pair[0].span.clone(), "letfn binding name must be an unqualified symbol"));
            };
            if name.namespace.is_some() || name.name == "&" {
                return Err(fail(pair[0].span.clone(), "letfn binding name must be unqualified"));
            }
            self.expander.charge_expansion_work(2, pair[0].span.clone())?;
            let id = BindingId(self.next);
            self.next += 1;
            self.insert_local(&pair[0], id, Type::Value, LocalKind::LetFn, None);
            declarations.push(self.locals[&name.name].clone());
        }
        let result = (|| {
            let mut bindings = Vec::with_capacity(declarations.len());
            let mut source_bindings = Vec::with_capacity(declarations.len());
            for (mut declaration, pair) in declarations.into_iter().zip(entries.chunks_exact(2)) {
                let id = declaration.id;
                let Kind::Symbol(name) = &pair[0].kind else { unreachable!() };
                let value = self.form_in_named(&pair[1], super::super::AnalysisContext::Expression, false, Some(&pair[0]))?;
                if !value.source.as_ref().is_some_and(|source| source.callable.is_some()) {
                    return Err(fail(pair[1].span.clone(), "letfn* initializer must be a function"));
                }
                declaration.initializer = Some(std::sync::Arc::new(value.clone()));
                let current = std::sync::Arc::make_mut(&mut self.locals).get_mut(&name.name).unwrap();
                if current.id == id { current.initializer = declaration.initializer.clone(); }
                source_bindings.push(declaration);
                bindings.push(Binding { id, name: name.name.clone(), metadata: pair[0].metadata.clone(),
                    span: pair[0].span.clone(), value });
            }
            let body = self.analyzed_body(&args[1..], form.span.clone(), context.returning(), tail)?;
            *self.source_nodes.last_mut().expect("source node fact slot") = Some(std::sync::Arc::new(SourceNode::Bindings {
                is_loop: false, is_letfn: true, bindings: source_bindings.into(), body: std::sync::Arc::new(body.clone()),
            }));
            Ok(Hir { source: None, span: form.span.clone(), metadata: form.metadata.clone(), ty: body.ty,
                kind: std::sync::Arc::new(Expression::LetFn { bindings, body: Box::new(body) }) })
        })();
        self.locals = outer;
        result
    }
}
