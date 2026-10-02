//! Original bounded binding/with-redefs bootstrap, informed by the pinned macros.
use super::*;
impl Analyzer<'_> {
    pub(super) fn assignment_target(&mut self, form: &Form) -> Result<Global, Diagnostic> {
        let Kind::Symbol(name) = &form.kind else {
            return Err(fail(
                form.span.clone(),
                "Binding target requires a resolved var",
            ));
        };
        if name.namespace.is_none()
            && (self.locals.contains_key(&name.name) || self.fields.contains_key(&name.name))
        {
            return Err(fail(
                form.span.clone(),
                "Cannot assign a local or immutable field",
            ));
        }
        match self
            .environment
            .resolve(self.phase, name, form.span.clone())?
        {
            ResolvedBinding::Cell(global) => Ok(global),
            ResolvedBinding::Arithmetic { global, .. } | ResolvedBinding::Core { global, .. } => {
                self.environment.materialize_bootstrap(global.clone());
                Ok(global)
            }
            _ => Err(fail(
                form.span.clone(),
                "Binding target is not a materialized runtime var",
            )),
        }
    }
    pub(super) fn dynamic_scope(
        &mut self,
        form: &Form,
        args: &[Form],
        context: super::super::AnalysisContext,
    ) -> Result<Hir, Diagnostic> {
        let Some(Form {
            kind: Kind::Vector(pairs),
            ..
        }) = args.first()
        else {
            return Err(fail(
                form.span.clone(),
                "binding/with-redefs requires a binding vector",
            ));
        };
        if pairs.len() % 2 != 0 {
            return Err(fail(args[0].span.clone(), "Binding has no initializer"));
        }
        let mut bindings = Vec::new();
        for pair in pairs.chunks_exact(2) {
            bindings.push((self.assignment_target(&pair[0])?, self.form(&pair[1])?));
        }
        let target = self.target.take();
        let body = self.body(&args[1..], form.span.clone(), context.returning(), false);
        self.target = target;
        let body = self.exception_region(form, vec![], body?);
        Ok(Hir {
            source: None,
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Value,
            kind: Expression::DynamicScope {
                bindings,
                body: Box::new(body),
            },
        })
    }
}
