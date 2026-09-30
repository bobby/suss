//! Replacement source -> HIR -> verified explicit IR -> shared-ABI fragments.
//! The CLI/AOT/macro paths still use the prototype; migration remains incomplete.
//! Supports scalars, resolved cells, fixed closures/universal calls and numeric bootstrap.
mod emit;
pub mod hir;
pub mod ir;
pub mod resolve;
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
    let forms = read_forms(source)
        .and_then(resolve_conditionals)
        .map_err(|error| Diagnostic {
            span: error.span,
            message: error.message,
        })?;
    hir::analyze_in(&forms, 0..source.len(), environment, phase)
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
