//! One preparation/publication path for source and evaluated macro declarations.
use super::*;

pub(super) struct PendingMacroDefinition {
    pub prepared: crate::portable::modules::PreparedInput,
    pub template: Form,
    binding: crate::portable::resolve::Global,
    name: String,
    namespace: String,
    reads_environment: bool,
    reads_caller_form: bool,
    compiler_catalog: Option<CompilerCatalogMode>,
    provenance: String,
}
impl MacroHost<'_> {
    pub(super) fn prepare_macro_definition(
        &mut self,
        form: &Form,
        span: std::ops::Range<usize>,
        origin: Option<&crate::portable::SourceOrigin>,
    ) -> Result<PendingMacroDefinition, SessionError> {
        let Kind::List(items) = &form.kind else {
            return Err(failure(form, "Expected source defmacro"));
        };
        if items.len() < 3
            || !matches!(&items[0].kind, Kind::Symbol(s) if s.name == "defmacro" && (s.namespace.is_none() || matches!(s.namespace.as_deref(), Some("suss.core" | "cljs.core" | "clojure.core"))))
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
        let mut name_form = items[1].clone();
        let mut docstring = None;
        let mut start = 2;
        // Pinned defmacro/defn retain leading docs/attributes before signatures.
        while let Some(item) = items.get(start) {
            match &item.kind {
                Kind::String(_) => docstring = Some(item.clone()),
                Kind::Map(_) => name_form.metadata.push(item.clone()),
                _ => break,
            }
            start += 1;
        }
        let declarations = &items[start..];
        let Some(first) = declarations.first() else {
            return Err(failure(form, "Macro requires at least one signature"));
        };
        let mut function = vec![symbol("fn", form.span.clone())];
        if matches!(first.kind, Kind::Vector(_)) {
            function.extend(add_implicit_arguments(declarations, form)?);
        } else {
            let mut signatures = declarations;
            if let Some(attributes) = signatures
                .last()
                .filter(|item| matches!(item.kind, Kind::Map(_)))
            {
                name_form.metadata.push(attributes.clone());
                signatures = &signatures[..signatures.len() - 1];
            }
            if signatures.is_empty() {
                return Err(failure(form, "Macro requires at least one signature"));
            }
            for signature in signatures {
                let Kind::List(parts) = &signature.kind else {
                    return Err(failure(signature, "Macro signature must be a list"));
                };
                function.push(Form {
                    span: signature.span.clone(),
                    metadata: signature.metadata.clone(),
                    kind: Kind::List(add_implicit_arguments(parts, signature)?),
                });
            }
        }
        let metadata =
            crate::portable_macro_data::normalized_reader_metadata(&mut self.session, &name_form)?;
        let mut compiler_catalog = None;
        for pair in metadata.chunks_exact(2) {
            if matches!(&pair[0].kind, Kind::Keyword(key) if key.namespace.as_deref() == Some("suss") && key.name == "compiler-catalog")
            {
                match &pair[1].kind {
                    Kind::Bool(enabled) => {
                        compiler_catalog = enabled.then_some(CompilerCatalogMode::Full)
                    }
                    Kind::Nil => compiler_catalog = None,
                    Kind::Keyword(key)
                        if key.namespace.is_none() && key.name == "record-bindings" =>
                    {
                        compiler_catalog = Some(CompilerCatalogMode::RecordBindings)
                    }
                    _ => {
                        return Err(failure(
                            &pair[1],
                            "Compiler catalog transport metadata must be Boolean or nil",
                        ));
                    }
                }
            }
        }
        name_form.metadata.push(Form {
            span: form.span.clone(),
            metadata: vec![],
            kind: Kind::Map(vec![
                Form {
                    span: form.span.clone(),
                    metadata: vec![],
                    kind: Kind::Keyword(suss_reader::Keyword::new("macro")),
                },
                Form {
                    span: form.span.clone(),
                    metadata: vec![],
                    kind: Kind::Bool(true),
                },
            ]),
        });
        let mut definition = vec![symbol("def", form.span.clone()), name_form];
        if let Some(docstring) = docstring {
            definition.push(docstring);
        }
        // The Runtime publication needs only the declaration prefix. Keep the
        // macro body exclusively in its one compiled Macro preparation.
        let mut definition = Form {
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            kind: Kind::List(definition),
        };
        let template = crate::portable::clone_form_metered(&definition, &mut |work| {
            self.session
                .charge_host_work(work)
                .map_err(|error| Diagnostic {
                    span: form.span.clone(),
                    message: error.to_string(),
                })
        })
        .map_err(SessionError::Compile)?;
        let Kind::List(parts) = &mut definition.kind else {
            unreachable!()
        };
        parts.push(Form {
            span: form.span.clone(),
            metadata: vec![],
            kind: Kind::List(function),
        });
        // A macro observes &env only through a lexical reference to its
        // implicit parameter: written in its definition, or produced by a source
        // macro expanded while its body compiles (through any rename, alias or
        // form rewrite). Compiler-implemented forms never introduce one. A macro
        // with neither cannot observe &env, so it receives nil.
        let expansions = catalog(&self.catalog).source_expansions;
        let snapshot = self.session.compilation_snapshot();
        let definition_origin = origin
            .cloned()
            .unwrap_or_else(|| crate::portable::SourceOrigin::new("", None))
            .for_macro_definition(self.session.native_owner());
        let prepared =
            snapshot.prepare_with_origin(vec![definition], span, self, Some(&definition_origin))?;
        let reads_environment =
            mentions_environment(form, &mut |work| self.session.charge_host_work(work))
                .map_err(|error| declaration_observation_context(error, &form.span, "&env"))?
                || catalog(&self.catalog).source_expansions != expansions;
        let reads_caller_form = mentions_parameter(form, "&form", &mut |work| {
            self.session.charge_host_work(work)
        })
        .map_err(|error| declaration_observation_context(error, &form.span, "&form"))?
            || catalog(&self.catalog).source_expansions != expansions;
        // Complete all fallible provenance work before publishing this
        // replacement. Effects already executed during preparation are retained.
        let provenance = declaration_provenance_metered(form, origin, &mut |work| {
            self.session.charge_host_work(work)
        })
        .map_err(|error| declaration_provenance_context(error, &form.span, origin))?;
        let binding = prepared
            .fragment
            .definition
            .as_ref()
            .ok_or_else(|| {
                failure(
                    form,
                    "Macro declaration did not retain its binding identity",
                )
            })?
            .global
            .clone();
        Ok(PendingMacroDefinition {
            prepared,
            template,
            binding,
            name: name.name.clone(),
            namespace: self.session.current_namespace().to_owned(),
            reads_environment,
            reads_caller_form,
            compiler_catalog,
            provenance,
        })
    }
    pub(super) fn commit_macro_definition(
        &mut self,
        pending: PendingMacroDefinition,
    ) -> Result<(SessionValue, SessionValue), SessionError> {
        let PendingMacroDefinition {
            prepared,
            binding,
            name,
            namespace,
            reads_environment,
            reads_caller_form,
            compiler_catalog,
            provenance,
            ..
        } = pending;
        let variable = self.session.eval_prepared(prepared)?;
        let value = self.session.binding_value(&binding)?;
        let key = (namespace.clone(), name.clone());
        if reads_caller_form {
            catalog(&self.catalog).caller_form_free.remove(&key);
        } else {
            catalog(&self.catalog).caller_form_free.insert(key.clone());
        }
        if reads_environment || compiler_catalog.is_some() {
            catalog(&self.catalog).environment_free.remove(&key);
        } else {
            catalog(&self.catalog).environment_free.insert(key.clone());
        }
        if let Some(mode) = compiler_catalog {
            catalog(&self.catalog)
                .compiler_catalog_macros
                .insert(key.clone(), mode);
        } else {
            catalog(&self.catalog).compiler_catalog_macros.remove(&key);
        }
        catalog(&self.catalog)
            .definitions
            .insert(key, value.clone());
        catalog(&self.catalog)
            .declaration_sources
            .insert((namespace.clone(), name.clone()), provenance);
        let exports = catalog(&self.catalog)
            .definitions
            .keys()
            .filter(|(ns, _)| ns == &namespace)
            .map(|(_, name)| name.clone())
            .collect::<Vec<_>>();
        self.session.declare_macro_exports(&namespace, &exports)?;
        Ok((value, variable))
    }
}
