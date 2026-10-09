//! Typed views of the genuine caller analysis state, never inferred from values.
//! This is a compiler interface, not an implementation of cljs.analyzer's
//! JS extern/module/warning branches or a source macro evaluator.
use super::{hir, resolve, Diagnostic, ExpansionContext};
use std::{collections::BTreeMap, ops::Range, sync::Arc};
use suss_reader::{
    forms::{Form, Kind},
    Symbol,
};

impl ExpansionContext<'_> {
    /// Current resolution catalog differs from the enclosing source snapshot:
    /// provisional definitions published during analysis must be visible here.
    pub fn resolution_catalog(&self) -> Arc<hir::SourceNamespace> {
        Arc::new(hir::SourceNamespace::capture(self.environment, self.phase))
    }

    /// All lexical entries, including unused locals and field captures. A local
    /// shadows an identically named field, exactly as the existing &env graph.
    pub fn lexical_bindings(&self) -> BTreeMap<String, hir::SourceBinding> {
        let mut bindings = BTreeMap::new();
        for (name, field) in self.fields {
            bindings.insert(
                name.clone(),
                hir::SourceBinding::Field(Arc::new(field.clone())),
            );
        }
        for (name, local) in self.locals {
            bindings.insert(
                name.clone(),
                hir::SourceBinding::Local(Arc::new(local.clone())),
            );
        }
        bindings
    }

    /// Resolve only actual portable bindings using the same aliases/refers,
    /// exclusions and phase as source analysis. Missing names remain errors.
    /// `include_locals=false` implements the macro helpers' dissoc-env-locals
    /// operation without modifying the caller or guessing a global identity.
    pub fn resolve_binding_fact(
        &self,
        symbol: &Symbol,
        span: Range<usize>,
        include_locals: bool,
    ) -> Result<hir::SourceBinding, Diagnostic> {
        if include_locals && symbol.namespace.is_none() {
            if let Some(local) = self.locals.get(&symbol.name) {
                return Ok(hir::SourceBinding::Local(Arc::new(local.clone())));
            }
            if let Some(field) = self.fields.get(&symbol.name) {
                return Ok(hir::SourceBinding::Field(Arc::new(field.clone())));
            }
        }
        let binding = self.environment.resolve(self.phase, symbol, span)?;
        let global = binding.global().clone();
        let declaration = self.environment.shared_definition_info(&global).cloned();
        Ok(hir::SourceBinding::Global {
            global,
            declaration,
        })
    }

    /// Actual protocol declarations and overloads. Bundle positions are ABI
    /// facts and must not be confused with ClojureScript fast-path mask bits.
    pub fn protocol_signature_facts(
        &self,
        symbol: &Symbol,
        span: Range<usize>,
    ) -> Result<(resolve::Global, BTreeMap<String, Vec<usize>>), Diagnostic> {
        let binding = self.environment.resolve(self.phase, symbol, span.clone())?;
        let global = binding.global().clone();
        let methods = self
            .environment
            .protocols
            .get(&global)
            .ok_or_else(|| Diagnostic {
                span,
                message: format!("Not a declared protocol: {symbol}"),
            })?;
        let signatures = methods
            .iter()
            .map(|method| {
                (
                    method.name.clone(),
                    method.signatures.iter().map(|(arity, _)| *arity).collect(),
                )
            })
            .collect();
        Ok((global, signatures))
    }
}

/// Exact six-key elision from pinned analyzer.cljc elide-reader-meta. Input is
/// normalized reader metadata; values and qualified lookalike keys are retained.
/// This helper does not elide ::analyzed or arbitrary analyzer/user metadata.
pub fn elide_reader_metadata(form: &Form) -> Result<Vec<Form>, Diagnostic> {
    let pairs = hir::reader_metadata_pairs(form)?;
    Ok(pairs.chunks_exact(2).filter(|pair| {
        !matches!(&pair[0].kind, Kind::Keyword(key) if key.namespace.is_none()
            && matches!(key.name.as_str(), "file" | "line" | "column" | "end-column" | "end-line" | "source"))
    }).flat_map(|pair| pair.iter().cloned()).collect())
}
