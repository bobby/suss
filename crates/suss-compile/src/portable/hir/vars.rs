//! Stable native Vars whose compiled getters read the original live cell.
use super::*;
use suss_reader::Symbol;

impl Analyzer<'_> {
    pub(super) fn native_constant(
        &mut self,
        form: &Form,
        owner: u64,
        index: u64,
    ) -> Result<Hir, Diagnostic> {
        if self
            .origin
            .as_ref()
            .and_then(|origin| origin.native_owner())
            != Some(owner)
        {
            return Err(fail(
                form.span.clone(),
                "Native constant belongs to another compilation owner",
            ));
        }
        let cell = self.environment.native_constant(self.phase, owner, index);
        Ok(Hir {
            source: None,
            span: form.span.clone(),
            metadata: vec![],
            ty: Type::Value,
            kind: std::sync::Arc::new(Expression::Global(cell)),
        })
    }

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

    fn quote_var_metadata(&mut self, metadata: &Form) -> Result<Hir, Diagnostic> {
        // This materializes compiler-owned data, never a user expression.
        // Use original core constructors/functions, without folding a user's
        // :const replacement of Keyword, PersistentVector or another core name.
        let privileged = std::mem::replace(&mut self.privileged, true);
        let constants = std::mem::take(&mut self.constant_definitions);
        let result = self.quote_data(metadata, 0);
        self.constant_definitions = constants;
        self.privileged = privileged;
        result
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
        // Capture the actual cell, bypassing const substitution/direct links.
        // The runtime shares one getter body across all Vars, so AOT code stays bounded
        // per definition while dynamic bindings and root replacement stay live.
        let target_cell = node(Expression::GlobalCell(target.clone()));
        let getter = self.nominal(form, Nominal::BindingGetter, vec![target_cell]);
        let namespace = self.literal_form(
            form,
            Literal::String(target.namespace().encode_utf16().collect()),
        );
        let name = self.literal_form(
            form,
            Literal::String(target.name().encode_utf16().collect()),
        );
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
        let metadata = self.quote_var_metadata(
            &Form {
                span: form.span.clone(),
                metadata: vec![],
                kind: Kind::Map(pairs),
            },
        )?;
        // Metadata construction can contain its own HIR bindings. Evaluate it
        // once and refer to the local in both the initial and update paths;
        // cloning executable HIR would duplicate binding identities/effects.
        let metadata = self.fresh_binding(form, metadata);
        let metadata_value = self.local(form, metadata.id);
        let construction = self.var_core_call(
            form,
            "make-binding-var",
            vec![getter, namespace, name, metadata_value.clone()],
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
            self.var_core_call(form, "update-binding-var-meta!", vec![read, metadata_value])?
        } else {
            read
        };
        let body = self.do_hir(form, vec![initialize, result]);
        Ok(node(Expression::Let {
            bindings: vec![metadata],
            body: Box::new(body),
        }))
    }
}
