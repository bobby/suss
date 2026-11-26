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
        let direct = self.origin.as_ref().is_some_and(|origin| origin.native_direct_constants())
            && self.native_function_pools.len()==1;
        let cell = if direct { self.environment.native_value(self.phase,owner,index) } else {
            let pool=*self.native_function_pools.last().ok_or_else(|| fail(form.span.clone(),"Missing native function constant pool"))?;
            let site=self.environment.native_function_site(self.phase,owner,index,pool);
            let initializer=self.environment.native_function_initializer(&site);
            let callee=Hir { source: None, span: form.span.clone(), metadata: vec![], ty: Type::Value,
                kind: std::sync::Arc::new(Expression::Global(initializer)) };
            self.native_pool_initializers.last_mut().unwrap().entry(site.clone()).or_insert_with(|| Hir {
                source: None, span: form.span.clone(), metadata: vec![], ty: Type::Value,
                kind: std::sync::Arc::new(Expression::Call {callee: Box::new(callee), arguments: vec![]}) });
            site
        };
        Ok(Hir {
            source: None,
            span: form.span.clone(),
            metadata: vec![],
            ty: Type::Value,
            kind: std::sync::Arc::new(Expression::Global(cell)),
        })
    }

    pub(super) fn initialize_native_pool(value: Hir, initializers: BTreeMap<Global,Hir>) -> Hir {
        if initializers.is_empty() { return value; }
        let mut items=initializers.into_values().collect::<Vec<_>>();
        Hir { source: value.source.clone(), span: value.span.clone(), metadata: value.metadata.clone(), ty: value.ty,
            kind: std::sync::Arc::new(Expression::Do({items.push(value);items})) }
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

    fn evaluate_var_metadata(&mut self, form: &Form) -> Result<Hir, Diagnostic> {
        self.expander.charge_expansion_work(1, form.span.clone())?;
        stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
            let items = match &form.kind {
                Kind::Vector(items) | Kind::Map(items) | Kind::Set(items) => items,
                Kind::Keyword(_) | Kind::Tagged { .. } => return self.quote_var_metadata(form),
                Kind::List(items) if items.is_empty() => return self.quote_var_metadata(form),
                _ => return self.form(form),
            };
            // Evaluate expressions against the user's current bindings. Only
            // the compiler's literal containers use its original core roots.
            let values = items.iter().map(|item| self.evaluate_var_metadata(item))
                .collect::<Result<Vec<_>, _>>()?;
            let privileged = std::mem::replace(&mut self.privileged, true);
            let constants = std::mem::take(&mut self.constant_definitions);
            let result = match &form.kind {
                Kind::Vector(_) => self.vector_values(form, values),
                Kind::Map(_) => self.map_values(form, items, values),
                Kind::Set(_) => self.set_values(form, items, values),
                _ => unreachable!(),
            };
            self.constant_definitions = constants;
            self.privileged = privileged;
            result
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
        let metadata_form = Form {
            span: form.span.clone(), metadata: vec![], kind: Kind::Map(pairs),
        };
        let metadata = if update {
            self.evaluate_var_metadata(&metadata_form)?
        } else { self.quote_var_metadata(&metadata_form)? };
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
            compiler_root: false,
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
    /// Direct source loading constructs generic Vars, not canonical native
    /// root owners. Its own metadata remains authoritative. Entering the core
    /// namespace must not grant access to a compiler-owned getter's flag.
    pub(super) fn ordinary_var_macro_adapter(
        &mut self, form: &Form, operation: Nominal, mut operands: Vec<Hir>,
    ) -> Hir {
        let node = |kind, ty| Hir {
            source: None, span: form.span.clone(), metadata: form.metadata.clone(),
            ty, kind: std::sync::Arc::new(kind),
        };
        if operation == Nominal::BindingGetterMacro {
            // Evaluate the getter expression once before consulting the
            // generic Var's metadata; no native cell is read.
            operands.push(self.literal_form(form, Literal::Bool(true)));
            return node(Expression::Do(operands), Type::Bool);
        }
        debug_assert_eq!(operation, Nominal::BindingGetterMacroSet);
        // Match the private adapter's operand order and getter result without
        // writing a native flag. Even source named suss.core has no authority.
        let getter = self.fresh_binding(form, operands.remove(0));
        let mut result = self.local(form, getter.id);
        result.ty = getter.value.ty;
        let ty = result.ty;
        operands.push(result);
        let body = node(Expression::Do(operands), ty);
        node(Expression::Let { bindings: vec![getter], body: Box::new(body) }, ty)
    }

}
