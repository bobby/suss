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
    source::namespace(&mut forms, &mut snapshot, phase)?;
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
    mut forms: Vec<suss_reader::forms::Form>,
    span: Range<usize>,
    environment: &resolve::Environment,
    phase: resolve::Phase,
) -> Result<PreparedFragment, Diagnostic> {
    let mut snapshot = environment.clone();
    let namespace_directive = source::namespace(&mut forms, &mut snapshot, phase)?;
    let (hir, environment) = hir::prepare(&forms, span, &snapshot, phase)?;
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
