//! Replacement source -> HIR -> verified explicit IR -> shared-ABI fragments.
//! Native sessions use this pipeline; AOT and source macro integration remain incomplete.
//! Supports scalars, resolved cells, fixed closures/universal calls and numeric bootstrap.
mod emit;
pub mod hir;
pub mod ir;
pub mod modules;
pub mod resolve;
mod source;
use std::ops::Range;
use suss_reader::forms::{read_forms, resolve_conditionals};

#[derive(Debug, thiserror::Error)]
#[error("{message} at bytes {span:?}")]
pub struct Diagnostic {
    pub span: Range<usize>,
    pub message: String,
}

/// Expansion executes in a host's isolated compiled session, not the compiler.
pub struct ExpansionContext<'a> {
    pub environment: &'a resolve::Environment,
    pub phase: resolve::Phase,
    pub locals: &'a std::collections::HashMap<String, (hir::BindingId, hir::Type)>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroReload {
    Once,
    Reload,
    ReloadAll,
}
pub trait ExpansionHost {
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
    hir::analyze_in(&forms, 0..source.len(), &snapshot, phase)
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
    compile_ir(&ir)
}

/// Emit a normalized function only after graph/type verification and Wasm validation.
pub fn compile_ir(function: &ir::Function) -> Result<Vec<u8>, Diagnostic> {
    emit::emit(function)
}

/// A validated fragment and staged compiler state. Install cells in one shared
/// runtime before execution; reuse existing cells by Global identity. A compiler
/// error never changes the supplied Environment. This does not load source files.
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
    prepare_fragment_forms(forms, 0..source.len(), environment, phase)
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
    let forms = resolve_conditionals(forms).map_err(|error| Diagnostic {
        span: error.span,
        message: error.message,
    })?;
    prepare_selected_fragment_with_expander(forms, span, environment, phase, expander)
}
pub(crate) fn prepare_selected_fragment_with_expander(
    mut forms: Vec<suss_reader::forms::Form>,
    span: Range<usize>,
    environment: &resolve::Environment,
    phase: resolve::Phase,
    expander: &mut dyn ExpansionHost,
) -> Result<PreparedFragment, Diagnostic> {
    let mut snapshot = environment.clone();
    let namespace_directive = source::namespace(&mut forms, &mut snapshot, phase, expander)?;
    let (hir, environment) = hir::prepare_with_expander(&forms, span, &snapshot, phase, expander)?;
    let wasm = compile_ir(&ir::lower(&hir)?)?;
    let cells = environment
        .cells()
        .into_iter()
        .filter(|cell| cell.phase() == phase)
        .collect();
    Ok(PreparedFragment {
        wasm,
        environment,
        cells,
        namespace_directive,
    })
}
