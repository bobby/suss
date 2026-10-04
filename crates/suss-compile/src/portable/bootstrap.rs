//! Versioned portable bootstrap artifacts. Source analysis restores compiler facts;
//! guest initializers execute from shipped Wasm, never from a host evaluator.
use super::{
    Diagnostic, PreparedFragment,
    resolve::{Environment, Phase},
};
use crate::runtime_abi;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

pub const FORMAT_VERSION: u32 = 1;
pub const SOURCE: &str = include_str!("../../../../runtime/core-import/suss/core.sus");
const DEPENDENCIES: &[u8] = include_bytes!("../../../../runtime/core-import/manifest.json");
const TARGET: &str = "portable-wasm-gc-shared-abi";
const MAX_ARTIFACT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format_version: u32,
    pub phase: String,
    pub source_sha256: String,
    pub compiler_source_sha256: String,
    pub dependency_graph_sha256: String,
    pub runtime_abi: u32,
    pub compiler: String,
    pub wasm_tools: String,
    pub target: String,
    /// The bounded bootstrap currently has no configurable compiler options.
    pub flags: Vec<String>,
    pub wasm_sha256: String,
    pub cells: Vec<String>,
}
fn error(message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        span: 0..SOURCE.len(),
        message: message.into(),
    }
}
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Runtime => "runtime",
        Phase::Macro => "macro",
    }
}
fn manifest(phase: Phase, wasm: &[u8], cells: &[super::resolve::Global]) -> Manifest {
    let abi = runtime_abi::Manifest::default();
    Manifest {
        format_version: FORMAT_VERSION,
        phase: phase_name(phase).into(),
        source_sha256: sha256(SOURCE.as_bytes()),
        compiler_source_sha256: env!("SUSS_COMPILER_SOURCE_SHA256").into(),
        dependency_graph_sha256: sha256(DEPENDENCIES),
        runtime_abi: abi.runtime_abi,
        compiler: abi.compiler,
        wasm_tools: abi.wasm_tools,
        target: TARGET.into(),
        flags: Vec::new(),
        wasm_sha256: sha256(wasm),
        cells: cells
            .iter()
            .map(|cell| format!("{}:{}", cell.import_module(), cell.import_name()))
            .collect(),
    }
}
fn catalog(phase: Phase) -> Result<super::AnalyzedFragment, Diagnostic> {
    let forms = suss_reader::forms::read_forms(SOURCE)
        .and_then(suss_reader::forms::resolve_conditionals)
        .map_err(|failure| Diagnostic {
            span: failure.span,
            message: failure.message,
        })?;
    super::analyze_selected_fragment(
        forms,
        0..SOURCE.len(),
        &Environment::default(),
        phase,
        &mut super::NoExpansion,
        Some(&super::SourceOrigin::new(SOURCE, None)),
    )
}
/// Development generator; it does not execute source or invoke Java/Node.
pub fn generate(phase: Phase) -> Result<(Vec<u8>, Manifest), Diagnostic> {
    let analyzed = catalog(phase)?;
    let wasm = super::compile_ir(&super::ir::lower(&analyzed.hir)?)?;
    let dependencies = vec![("core-import-manifest".into(), sha256(DEPENDENCIES))];
    let wasm = super::artifact_identity::annotate_source(
        &wasm, phase, Some(&super::SourceOrigin::new(SOURCE, None)), Some(&dependencies),
    ).map_err(error)?;
    let manifest = manifest(phase, &wasm, &analyzed.cells);
    Ok((wasm, manifest))
}
/// Validate all identity inputs before a host can install or initialize the artifact.
/// Wasm validation/linking remains mandatory in the consuming host.
pub fn restore(phase: Phase, wasm: &[u8], json: &[u8]) -> Result<PreparedFragment, Diagnostic> {
    if wasm.len() > MAX_ARTIFACT_BYTES || json.len() > 1024 * 1024 {
        return Err(error("Bootstrap artifact exceeds bounded size"));
    }
    let found: Manifest = serde_json::from_slice(json)
        .map_err(|failure| error(format!("Invalid bootstrap manifest: {failure}")))?;
    let mut expected = manifest(phase, wasm, &[]);
    expected.cells = found.cells.clone();
    if found != expected {
        return Err(error(
            "Bootstrap manifest identity mismatch; regenerate the versioned artifacts",
        ));
    }
    let analyzed = catalog(phase)?;
    let expected_cells = manifest(phase, wasm, &analyzed.cells).cells;
    if found.cells != expected_cells {
        return Err(error("Bootstrap binding catalog mismatch"));
    }
    runtime_abi::verify_artifact(wasm, &runtime_abi::Manifest::default()).map_err(error)?;
    let dependencies = vec![("core-import-manifest".into(), sha256(DEPENDENCIES))];
    super::artifact_identity::verify(wasm, super::artifact_identity::Expected {
        phase: Some(phase), source: Some(SOURCE), macro_dependencies: Some(&dependencies),
    }).map_err(error)?;
    wasmparser::Validator::new()
        .validate_all(wasm)
        .map_err(|failure| error(format!("Invalid bootstrap Wasm: {failure}")))?;
    Ok(PreparedFragment {
        wasm: wasm.to_vec(),
        environment: analyzed.environment,
        cells: analyzed.cells,
        namespace_directive: analyzed.namespace_directive,
    })
}
/// Cache immutable compiler facts and bytes, never guest roots or initialized state.
/// Every Session installs and executes these artifacts in its own Store.
pub fn shipped(phase: Phase) -> Result<&'static PreparedFragment, Diagnostic> {
    static RUNTIME: OnceLock<Result<PreparedFragment, String>> = OnceLock::new();
    static MACRO: OnceLock<Result<PreparedFragment, String>> = OnceLock::new();
    let (cache, wasm, json): (_, &[u8], &[u8]) = match phase {
        Phase::Runtime => (
            &RUNTIME,
            include_bytes!("../../../../runtime/bootstrap/runtime.wasm"),
            include_bytes!("../../../../runtime/bootstrap/runtime.json"),
        ),
        Phase::Macro => (
            &MACRO,
            include_bytes!("../../../../runtime/bootstrap/macro.wasm"),
            include_bytes!("../../../../runtime/bootstrap/macro.json"),
        ),
    };
    cache
        .get_or_init(|| restore(phase, wasm, json).map_err(|failure| failure.message))
        .as_ref()
        .map_err(|message| error(message.clone()))
}
