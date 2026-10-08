//! Replacement source -> HIR -> verified explicit IR -> shared-ABI fragments.
//! Native sessions use this pipeline; AOT and source macro integration remain incomplete.
//! Supports scalars, resolved cells, fixed closures/universal calls and numeric bootstrap.
mod emit;
mod function_names;
pub mod hir;
pub mod ir;
pub mod modules;
pub mod resolve;
mod source;
mod stdlib;
mod origin;
pub mod syntax_quote;
pub mod bootstrap;
pub mod artifact_cache;
pub mod artifact_identity;
pub mod core_bindings;
pub mod aot;
pub mod command;
pub use origin::{SourceOrigin, SourcePosition};
use std::ops::Range;
use suss_reader::forms::{read_forms, resolve_conditionals};

#[derive(Debug, thiserror::Error)]
#[error("{message} at bytes {span:?}")]
pub struct Diagnostic {
    pub span: Range<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisContext {
    Statement,
    Expression,
    Return,
}
impl AnalysisContext {
    /// Body contexts produce a return expression without granting recur scope.
    pub(crate) fn returning(self) -> Self {
        if self == Self::Expression { Self::Return } else { self }
    }
}

/// Expansion executes in a host's isolated compiled session, not the compiler.
pub struct ExpansionContext<'a> {
    pub environment: &'a resolve::Environment,
    /// Immutable namespace at entry to the enclosing top-level source form.
    /// Resolution uses the live environment above, including provisional defs.
    pub namespace_snapshot: &'a std::sync::Arc<hir::SourceNamespace>,
    pub origin: Option<&'a SourceOrigin>,
    pub phase: resolve::Phase,
    pub context: AnalysisContext,
    pub locals: &'a std::collections::HashMap<String, hir::LocalBinding>,
    pub fields: &'a std::collections::HashMap<String, hir::FieldBinding>,
    pub function_scopes: &'a [std::sync::Arc<hir::FunctionScope>],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroReload {
    Once,
    Reload,
    ReloadAll,
}
pub trait ExpansionHost {
    /// Immutable selected macro source identities; None means unverified/unknown.
    fn artifact_dependencies(&self) -> Option<Vec<(String, String)>> { None }

    /// Analysis and macro execution have already happened. Hosts may reuse only
    /// emission bytes; staged catalogs and cells always come from this analysis.
    fn emit_fragment(
        &mut self, function: &ir::Function, phase: resolve::Phase,
        forms: &[suss_reader::forms::Form], origin: Option<&SourceOrigin>,
    ) -> Result<Vec<u8>, Diagnostic> {
        let _ = (phase, forms, origin);
        compile_ir(function)
    }

    fn supports_macro_imports(&self) -> bool {
        false
    }
    fn source_paths(&mut self, _paths: &[std::path::PathBuf]) {}
    fn load_macro_namespace(
        &mut self,
        _namespace: &str,
        span: Range<usize>,
    ) -> Result<Vec<String>, Diagnostic> {
        Err(Diagnostic { span, message: "Source macro imports require an isolated compiled macro session, not yet integrated".into() })
    }

    fn load_macro_namespace_with_policy(
        &mut self,
        namespace: &str,
        policy: MacroReload,
        span: Range<usize>,
    ) -> Result<Vec<String>, Diagnostic> {
        if policy == MacroReload::Once {
            self.load_macro_namespace(namespace, span)
        } else {
            Err(Diagnostic {
                span,
                message: "Source macro reload requires a compiled phase loading host".into(),
            })
        }
    }
    fn expand(
        &mut self,
        form: &suss_reader::forms::Form,
        context: ExpansionContext<'_>,
    ) -> Result<Option<suss_reader::forms::Form>, Diagnostic>;
}
pub(crate) struct NoExpansion;
impl ExpansionHost for NoExpansion {
    fn artifact_dependencies(&self) -> Option<Vec<(String, String)>> { Some(vec![]) }

    fn expand(
        &mut self,
        _: &suss_reader::forms::Form,
        _: ExpansionContext<'_>,
    ) -> Result<Option<suss_reader::forms::Form>, Diagnostic> {
        Ok(None)
    }
}
pub fn analyze(source: &str) -> Result<hir::Hir, Diagnostic> {
    analyze_in(
        source,
        &resolve::Environment::default(),
        resolve::Phase::Runtime,
    )
}

pub fn analyze_in(
    source: &str,
    environment: &resolve::Environment,
    phase: resolve::Phase,
) -> Result<hir::Hir, Diagnostic> {
    let mut forms = read_forms(source)
        .and_then(resolve_conditionals)
        .map_err(|error| Diagnostic {
            span: error.span,
            message: error.message,
        })?;
    let mut snapshot = environment.clone();
    source::namespace(&mut forms, &mut snapshot, phase, &mut NoExpansion)?;
    Ok(hir::prepare_with_origin(&forms, 0..source.len(), &snapshot, phase, &mut NoExpansion, Some(&SourceOrigin::new(source, None)))?.0)
}
pub fn compile(source: &str) -> Result<Vec<u8>, Diagnostic> {
    compile_in(
        source,
        &resolve::Environment::default(),
        resolve::Phase::Runtime,
    )
}

pub fn compile_in(
    source: &str,
    environment: &resolve::Environment,
    phase: resolve::Phase,
) -> Result<Vec<u8>, Diagnostic> {
    let hir = analyze_in(source, environment, phase)?;
    let ir = ir::lower(&hir)?;
    let wasm = compile_ir(&ir)?;
    artifact_identity::annotate_source(&wasm, phase, Some(&SourceOrigin::new(source, None)), Some(&[]))
        .map_err(|message| Diagnostic { span: 0..source.len(), message })
}

/// Emit a normalized function only after graph/type verification and Wasm validation.
pub fn compile_ir(function: &ir::Function) -> Result<Vec<u8>, Diagnostic> {
    let wasm = emit::emit(function)?;
    artifact_identity::annotate_ir(&wasm).map_err(|message| Diagnostic { span: function.span.clone(), message })
}

/// A validated fragment and staged compiler state. Install cells in one shared
/// runtime before execution; reuse existing cells by Global identity. A compiler
/// error never changes the supplied Environment. This does not load source files.
#[derive(Clone)]
pub struct PreparedFragment {
    pub wasm: Vec<u8>,
    pub environment: resolve::Environment,
    pub cells: Vec<resolve::Global>,
    pub namespace_directive: Option<suss_reader::forms::Form>,
}
pub fn prepare_fragment(
    source: &str,
    environment: &resolve::Environment,
    phase: resolve::Phase,
) -> Result<PreparedFragment, Diagnostic> {
    let forms = read_forms(source).map_err(|error| Diagnostic {
        span: error.span,
        message: error.message,
    })?;
    prepare_fragment_forms_with_origin(forms, 0..source.len(), environment, phase, &mut NoExpansion, Some(&SourceOrigin::new(source, None)))
}
/// Compile owned reader/expanded forms without printing or rereading them.
/// Conditional selection and namespace preparation preserve original form spans
/// and metadata. The caller supplies the enclosing source/call-site span.
pub fn prepare_fragment_forms(
    forms: Vec<suss_reader::forms::Form>,
    span: Range<usize>,
    environment: &resolve::Environment,
    phase: resolve::Phase,
) -> Result<PreparedFragment, Diagnostic> {
    let forms = resolve_conditionals(forms).map_err(|error| Diagnostic {
        span: error.span,
        message: error.message,
    })?;
    prepare_selected_fragment(forms, span, environment, phase)
}
pub(crate) fn prepare_selected_fragment(
    forms: Vec<suss_reader::forms::Form>,
    span: Range<usize>,
    environment: &resolve::Environment,
    phase: resolve::Phase,
) -> Result<PreparedFragment, Diagnostic> {
    prepare_selected_fragment_with_expander(forms, span, environment, phase, &mut NoExpansion)
}
pub fn prepare_fragment_forms_with_expander(
    forms: Vec<suss_reader::forms::Form>,
    span: Range<usize>,
    environment: &resolve::Environment,
    phase: resolve::Phase,
    expander: &mut dyn ExpansionHost,
) -> Result<PreparedFragment, Diagnostic> {
    prepare_fragment_forms_with_origin(forms, span, environment, phase, expander, None)
}
/// Compile forms with the exact source they refer to; no printing or rereading.
/// None explicitly means no origin is available for these forms.
pub fn prepare_fragment_forms_with_origin(
    forms: Vec<suss_reader::forms::Form>, span: Range<usize>,
    environment: &resolve::Environment, phase: resolve::Phase,
    expander: &mut dyn ExpansionHost, origin: Option<&SourceOrigin>,
) -> Result<PreparedFragment, Diagnostic> {
    let forms = resolve_conditionals(forms).map_err(|error| Diagnostic {
        span: error.span, message: error.message,
    })?;
    prepare_selected_fragment_with_origin(forms, span, environment, phase, expander, origin)
}
pub(crate) fn prepare_selected_fragment_with_expander(
    forms: Vec<suss_reader::forms::Form>, span: Range<usize>,
    environment: &resolve::Environment, phase: resolve::Phase,
    expander: &mut dyn ExpansionHost,
) -> Result<PreparedFragment, Diagnostic> {
    prepare_selected_fragment_with_origin(forms, span, environment, phase, expander, None)
}
pub(crate) fn prepare_selected_fragment_with_origin(
    forms: Vec<suss_reader::forms::Form>, span: Range<usize>,
    environment: &resolve::Environment, phase: resolve::Phase,
    expander: &mut dyn ExpansionHost, origin: Option<&SourceOrigin>,
) -> Result<PreparedFragment, Diagnostic> {
    let analyzed = analyze_selected_fragment(forms.clone(), span, environment, phase, expander, origin)?;
    let function = ir::lower(&analyzed.hir)?;
    let wasm = expander.emit_fragment(&function, phase, &forms, origin)?;
    let dependencies = expander.artifact_dependencies();
    let wasm = artifact_identity::annotate_source(&wasm, phase, origin, dependencies.as_deref())
        .map_err(|message| Diagnostic { span: function.span.clone(), message })?;
    Ok(PreparedFragment { wasm, environment: analyzed.environment,
        cells: analyzed.cells, namespace_directive: analyzed.namespace_directive })
}

pub(crate) struct AnalyzedFragment {
    pub hir: hir::Hir,
    pub environment: resolve::Environment,
    pub cells: Vec<resolve::Global>,
    pub namespace_directive: Option<suss_reader::forms::Form>,
}
pub(crate) fn analyze_selected_fragment(
    mut forms: Vec<suss_reader::forms::Form>, span: Range<usize>,
    environment: &resolve::Environment, phase: resolve::Phase,
    expander: &mut dyn ExpansionHost, origin: Option<&SourceOrigin>,
) -> Result<AnalyzedFragment, Diagnostic> {
    let mut snapshot = environment.clone();
    let namespace_directive = source::namespace(&mut forms, &mut snapshot, phase, expander)?;
    let (hir, environment) = hir::prepare_with_origin(&forms, span, &snapshot, phase, expander, origin)?;
    let cells = environment
        .cells()
        .into_iter()
        .filter(|cell| cell.phase() == phase)
        .collect();
    Ok(AnalyzedFragment {
        hir, environment, cells, namespace_directive,
    })
}
