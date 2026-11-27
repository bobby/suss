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
        roots: bool,
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
        let mut last_source_value = std::collections::BTreeMap::new();
        if roots {
            for (index, pair) in pairs.chunks_exact(2).enumerate() {
                if let Kind::Symbol(symbol) = &pair[0].kind {
                    self.expander.charge_expansion_work(
                        (symbol.name.len() + symbol.namespace.as_ref().map_or(0, String::len)) as u64 + 1,
                        pair[0].span.clone())?;
                    last_source_value.insert((symbol.namespace.as_deref(), symbol.name.as_str()), index);
                }
            }
        }
        let mut source_keys = std::collections::BTreeSet::new();
        let mut bindings = Vec::new();
        for pair in pairs.chunks_exact(2) {
            // Clojure's with-redefs macro first zipmaps source (var symbol)
            // keys. An identical source key replaces its initializer before
            // analysis or execution. Ordinary binding builds a runtime map
            // instead, so all of its initializers still run.
            // Replacing an array-map value retains the first key's position.
            let value = if let (true, Kind::Symbol(symbol)) = (roots, &pair[0].kind) {
                let key = (symbol.namespace.as_deref(), symbol.name.as_str());
                if !source_keys.insert(key) { continue; }
                &pairs[last_source_value[&key] * 2 + 1]
            } else { &pair[1] };
            bindings.push((self.assignment_target(&pair[0])?, self.form(value)?));
        }
        let target = self.target.take();
        let body = self.body(&args[1..], form.span.clone(), context.returning(), false);
        self.target = target;
        let body = body?;
        if roots {
            let mut globals = std::collections::BTreeSet::new();
            // Distinct source keys may resolve to the same Var. Clojure
            // evaluates their retained values, then map construction rejects
            // the duplicate actual Var before replacing any root.
            if let Some((duplicate, _)) = bindings.iter().find(|(global, _)| !globals.insert(global.clone())) {
                let namespace = if duplicate.namespace() == "suss.core" {
                    "clojure.core"
                } else { duplicate.namespace() };
                let message = self.literal_form(form, Literal::String(
                    format!("Duplicate key: #'{namespace}/{}", duplicate.name()).encode_utf16().collect()));
                let exception = self.nominal(form, Nominal::LanguageError, vec![message]);
                let failure = Hir { source: None, span: form.span.clone(), metadata: vec![],
                    ty: Type::Value, kind: std::sync::Arc::new(Expression::Throw(Box::new(exception))) };
                let mut effects = bindings.into_iter().map(|(_, initializer)| initializer).collect::<Vec<_>>();
                effects.push(failure);
                return Ok(self.do_hir(form, effects));
            }
        }
        let kind = if self.suspendable {
            Expression::AsyncDynamicScope { roots, bindings, body: Box::new(body) }
        } else {
            let body = self.exception_region(form, vec![], body);
            Expression::DynamicScope { roots, bindings, body: Box::new(body) }
        };
        Ok(Hir {
            source: None,
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Value,
            kind: std::sync::Arc::new(kind),
        })
    }
}
