//! Replacement source -> HIR -> verified explicit IR -> shared-ABI fragments.
//! The CLI/AOT/macro paths still use the prototype; migration remains incomplete.
//! This bootstrap supports scalars, lexical let/do/if and verified numeric calls.
mod emit;
pub mod hir;
pub mod ir;
use std::ops::Range;
use suss_reader::forms::{read_forms, resolve_conditionals};

#[derive(Debug, thiserror::Error)]
#[error("{message} at bytes {span:?}")]
pub struct Diagnostic {
    pub span: Range<usize>,
    pub message: String,
}

pub fn analyze(source: &str) -> Result<hir::Hir, Diagnostic> {
    let forms = read_forms(source)
        .and_then(resolve_conditionals)
        .map_err(|error| Diagnostic {
            span: error.span,
            message: error.message,
        })?;
    hir::analyze(&forms, 0..source.len())
}
pub fn compile(source: &str) -> Result<Vec<u8>, Diagnostic> {
    let hir = analyze(source)?;
    let ir = ir::lower(&hir)?;
    compile_ir(&ir)
}

/// Emit a normalized function only after graph/type verification and Wasm validation.
pub fn compile_ir(function: &ir::Function) -> Result<Vec<u8>, Diagnostic> {
    emit::emit(function)
}
