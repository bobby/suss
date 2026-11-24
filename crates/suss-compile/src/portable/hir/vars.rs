//! Stable native Vars whose compiled getters read the original live cell.
use super::*;
use suss_reader::Symbol;

impl Analyzer<'_> {
    fn var_core_call(
        &mut self,
        form: &Form,
        name: &str,
        arguments: Vec<Hir>,
    ) -> Result<Hir, Diagnostic> {
        let binding = self.environment.resolve(
            self.phase,
            &Symbol::namespaced("suss.core", name),
            form.span.clone(),
        )?;
        let global = self
            .environment
            .existing_core_link(binding.global())
            .ok_or_else(|| fail(form.span.clone(), "Var operations require shipped core"))?;
        let callee = Hir {
            source: None,
            span: form.span.clone(),
            metadata: vec![],
            ty: Type::Value,
            kind: std::sync::Arc::new(Expression::Global(global)),
        };
        Ok(Hir {
            source: None,
            span: form.span.clone(),
            metadata: vec![],
            ty: Type::Value,
            kind: std::sync::Arc::new(Expression::Call {
                callee: Box::new(callee),
                arguments,
            }),
        })
    }
    pub(super) fn binding_var(
        &mut self,
        form: &Form,
        target: &Global,
        update: bool,
    ) -> Result<Hir, Diagnostic> {
        self.expander.charge_expansion_work(
            (target.namespace().len() + target.name().len()) as u64 + 1,
            form.span.clone(),
        )?;
        let cell = self.environment.var_cell(target)?;
        let node = |kind| Hir {
            source: None,
            span: form.span.clone(),
            metadata: vec![],
            ty: Type::Value,
            kind: std::sync::Arc::new(kind),
        };
        // Bypass literal substitution, inlining and core direct links: a Var
        // reads its actual public root/dynamic cell even when ordinary source
        // references were compiled with :const or direct linking.
        let getter = node(Expression::Function {
            parameters: vec![],
            captures: vec![],
            body: Box::new(node(Expression::Global(target.clone()))),
        });
        let clean = Form {
            span: form.span.clone(),
            metadata: vec![],
            kind: Kind::Nil,
        };
        let symbol =
            self.identifier_literal(&clean, Some(target.namespace()), target.name(), false)?;
        let info = self.environment.definition_info(target).cloned();
        let mut pairs = info
            .as_ref()
            .map(|info| reader_metadata_pairs(&info.declaration))
            .transpose()?
            .unwrap_or_default();
        if let Some(doc) = info.as_ref().and_then(|info| info.docstring.as_ref()) {
            pairs.push(Form {
                span: form.span.clone(),
                metadata: vec![],
                kind: Kind::Keyword(suss_reader::Keyword::new("doc")),
            });
            pairs.push(Form {
                span: form.span.clone(),
                metadata: vec![],
                kind: Kind::String(doc.clone()),
            });
        }
        let metadata = self.quote_data(
            &Form {
                span: form.span.clone(),
                metadata: vec![],
                kind: Kind::Map(pairs),
            },
            0,
        )?;
        let construction = self.var_core_call(
            form,
            "make-binding-var",
            vec![getter, symbol, metadata.clone()],
        )?;
        let initialize = node(Expression::Definition {
            result: None,
            constant: None,
            global: cell.clone(),
            name_metadata: vec![],
            name_span: form.span.clone(),
            docstring: None,
            initializer: Some(Box::new(construction)),
            once: true,
            linked: None,
        });
        let read = node(Expression::Global(cell));
        let result = if update {
            self.var_core_call(form, "update-binding-var-meta!", vec![read, metadata])?
        } else {
            read
        };
        Ok(self.do_hir(form, vec![initialize, result]))
    }
}
