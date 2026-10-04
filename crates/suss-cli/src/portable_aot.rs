//! Source files use the same staged compilation as scripts before AOT assembly.
use crate::{
    portable_macros::CompiledMacros,
    portable_repl::{prepare_script_compiled, read_script_forms, PreparedScript},
    portable_session::{CompilationSnapshot, SessionError},
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use suss_compile::portable::{self, modules::ModuleIdentity, resolve::Phase, PreparedFragment};
use suss_reader::Symbol;

/// Resolve the selected WIT graph and compile source through isolated compiled
/// macros, without executing any Runtime initializer in the compiler host.
pub fn compile_file(
    source_path: &Path,
    wit_path: &Path,
    wit_world: Option<&str>,
    source_paths: &[PathBuf],
    mappings: &[(String, Symbol)],
) -> Result<Vec<u8>, String> {
    compile_input(
        source_path,
        wit_path,
        wit_world,
        source_paths,
        mappings,
        None,
    )
}

/// Resolve one namespace source and validate its declaration before constructing
/// a Macro session. The exact validated text is also the compiled input.
pub fn compile_namespace(
    namespace: &str,
    wit_path: &Path,
    wit_world: Option<&str>,
    source_paths: &[PathBuf],
    mappings: &[(String, Symbol)],
) -> Result<Vec<u8>, String> {
    let path = portable::resolve::locate_source(namespace, source_paths, 0..0)
        .map_err(|error| error.to_string())?;
    compile_input(
        &path,
        wit_path,
        wit_world,
        source_paths,
        mappings,
        Some(namespace),
    )
}

fn compile_input(
    source_path: &Path,
    wit_path: &Path,
    wit_world: Option<&str>,
    source_paths: &[PathBuf],
    mappings: &[(String, Symbol)],
    namespace: Option<&str>,
) -> Result<Vec<u8>, String> {
    let mut resolve = portable::aot::Resolve::new();
    let (package, _) = resolve
        .push_path(wit_path)
        .map_err(|error| format!("Failed to resolve WIT {}: {error:#}", wit_path.display()))?;
    let world = resolve
        .select_world(&[package], wit_world)
        .map_err(|error| format!("Failed to select WIT world: {error:#}"))?;
    let source = std::fs::read_to_string(source_path)
        .map_err(|error| format!("Failed to read {}: {error}", source_path.display()))?;
    let forms = read_script_forms(&source).map_err(|error| error.to_string())?;
    if let Some(namespace) = namespace {
        portable::modules::validate_namespace_source(namespace, &forms, Phase::Runtime)
            .map_err(|error| error.to_string())?;
    }
    let fragments = prepare_forms(&source, Some(source_path.to_owned()), source_paths, forms)
        .map_err(|error| error.to_string())?;
    let mappings = portable::aot::source_export_mappings(&fragments, &resolve, world, mappings)
        .map_err(|error| error.to_string())?;
    portable::aot::component(&fragments, &resolve, world, &mappings)
        .map_err(|error| error.to_string())
}

/// Prepare a complete source input and its Runtime dependencies without running
/// any Runtime initializer. Only compiled Macro-phase code executes in the host.
/// The returned catalogs are compiler facts, not reconstructed raw-Wasm metadata.
pub fn prepare_source(
    source: &str,
    path: Option<PathBuf>,
    source_paths: &[PathBuf],
) -> Result<Vec<PreparedFragment>, SessionError> {
    // Parse the entire input before constructing the effectful Macro session.
    let forms = read_script_forms(source)?;
    prepare_forms(source, path, source_paths, forms)
}

fn prepare_forms(
    source: &str,
    path: Option<PathBuf>,
    source_paths: &[PathBuf],
    forms: Vec<suss_reader::forms::Form>,
) -> Result<Vec<PreparedFragment>, SessionError> {
    let mut core = portable::bootstrap::shipped(Phase::Runtime)
        .map_err(SessionError::Compile)?
        .clone();
    core.environment
        .enter_namespace(Phase::Runtime, "user")
        .map_err(SessionError::Compile)?;
    let snapshot = CompilationSnapshot::new(
        core.environment.clone(),
        Phase::Runtime,
        source_paths.to_vec(),
        BTreeSet::from([
            ModuleIdentity::new(Phase::Runtime, "suss.core").map_err(SessionError::Compile)?
        ]),
    );
    let origin = portable::SourceOrigin::new(source, path);
    let mut macros = CompiledMacros::new()?;
    let prepared = prepare_script_compiled(snapshot, &mut macros, forms, source.len(), &origin)?;
    let mut fragments = vec![core];
    for input in prepared {
        let PreparedScript::Runtime(input) = input else {
            continue;
        };
        // All declarations are already staged by the common source pipeline.
        // Module artifacts retain their own immutable source/phase identities.
        for module in input.modules {
            fragments.push(PreparedFragment {
                wasm: module.wasm,
                environment: input.fragment.environment.clone(),
                cells: input.fragment.cells.clone(),
                namespace_directive: None,
            });
        }
        fragments.push(input.fragment);
    }
    Ok(fragments)
}
