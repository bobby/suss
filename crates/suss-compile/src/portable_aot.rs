//! Source files use the same staged compilation as scripts before AOT assembly.
use crate::{
    portable_macros::CompiledMacros,
    portable_repl::{PreparedScript, prepare_script_compiled_batch, read_script_forms},
    portable_session::{CompilationSnapshot, SessionError},
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use crate::portable::{self, PreparedFragment, modules::ModuleIdentity, resolve::Phase};
use suss_reader::Symbol;

/// An immutable, already selected and validated project input.
pub(crate) struct SourceInput {
    pub(crate) source: String,
    pub(crate) path: Option<PathBuf>,
    pub(crate) forms: Vec<suss_reader::forms::Form>,
    pub(crate) namespace: Option<String>,
}

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

/// Compile a validated entry namespace to the pinned official command profile.
pub fn compile_main(
    source_path: &Path,
    namespace: &str,
    source_paths: &[PathBuf],
) -> Result<Vec<u8>, String> {
    let source = std::fs::read_to_string(source_path)
        .map_err(|error| format!("Failed to read {}: {error}", source_path.display()))?;
    let forms = read_script_forms(&source).map_err(|error| error.to_string())?;
    portable::modules::validate_namespace_source(namespace, &forms, Phase::Runtime)
        .map_err(|error| error.to_string())?;
    let fragments = prepare_inputs_with_command(
        &[SourceInput {
            source,
            path: Some(source_path.to_owned()),
            forms,
            namespace: Some(namespace.to_owned()),
        }],
        source_paths,
        true,
    )
    .map_err(|error| error.to_string())?;
    portable::command::component_with_exit(
        &fragments,
        &Symbol::namespaced(namespace, "-main"),
        &Symbol::namespaced("wasi.cli", "exit-with-code"),
    )
    .map_err(|error| error.to_string())
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
    let source = std::fs::read_to_string(source_path)
        .map_err(|error| format!("Failed to read {}: {error}", source_path.display()))?;
    let forms = read_script_forms(&source).map_err(|error| error.to_string())?;
    if let Some(namespace) = namespace {
        portable::modules::validate_namespace_source(namespace, &forms, Phase::Runtime)
            .map_err(|error| error.to_string())?;
    }
    compile_inputs(
        &[SourceInput {
            source,
            path: Some(source_path.to_owned()),
            forms,
            namespace: namespace.map(str::to_owned),
        }],
        wit_path,
        wit_world,
        source_paths,
        mappings,
    )
}

pub(crate) fn compile_inputs(
    inputs: &[SourceInput],
    wit_path: &Path,
    wit_world: Option<&str>,
    source_paths: &[PathBuf],
    mappings: &[(String, Symbol)],
) -> Result<Vec<u8>, String> {
    let mut resolve = portable::aot::Resolve::new();
    let (package, _) = resolve
        .push_path(wit_path)
        .map_err(|error| format!("Failed to resolve WIT {}: {error:#}", wit_path.display()))?;
    let world = resolve
        .select_world(&[package], wit_world)
        .map_err(|error| format!("Failed to select WIT world: {error:#}"))?;
    portable::aot::validate_boundary(&resolve, world).map_err(|error| error.to_string())?;
    let fragments = prepare_inputs(inputs, source_paths).map_err(|error| error.to_string())?;
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
    // Parse the complete input before constructing the effectful Macro session.
    let forms = read_script_forms(source)?;
    prepare_forms(source, path, source_paths, forms)
}

fn prepare_forms(
    source: &str,
    path: Option<PathBuf>,
    source_paths: &[PathBuf],
    forms: Vec<suss_reader::forms::Form>,
) -> Result<Vec<PreparedFragment>, SessionError> {
    prepare_inputs(
        &[SourceInput {
            source: source.into(),
            path,
            forms,
            namespace: None,
        }],
        source_paths,
    )
}

fn prepare_inputs(
    inputs: &[SourceInput],
    source_paths: &[PathBuf],
) -> Result<Vec<PreparedFragment>, SessionError> {
    prepare_inputs_with_command(inputs, source_paths, false)
}

fn prepare_inputs_with_command(
    inputs: &[SourceInput],
    source_paths: &[PathBuf],
    command: bool,
) -> Result<Vec<PreparedFragment>, SessionError> {
    let mut core = portable::bootstrap::shipped(Phase::Runtime)
        .map_err(SessionError::Compile)?
        .clone();
    core.environment
        .enter_namespace(Phase::Runtime, "user")
        .map_err(SessionError::Compile)?;
    if command {
        let exit = core
            .environment
            .declare_cell(Phase::Runtime, "wasi.cli", "exit-with-code")
            .map_err(SessionError::Compile)?;
        core.cells.push(exit);
    }
    let mut snapshot = CompilationSnapshot::new(
        core.environment.clone(),
        Phase::Runtime,
        source_paths.to_vec(),
        BTreeSet::from([
            ModuleIdentity::new(Phase::Runtime, "suss.core").map_err(SessionError::Compile)?
        ]),
    );
    let mut macros = CompiledMacros::new()?;
    if command {
        snapshot.provided.insert(
            ModuleIdentity::new(Phase::Runtime, "wasi.cli").map_err(SessionError::Compile)?,
        );
    }
    let mut fragments = vec![core];
    // Only sources prepared during this batch suppress a later selected root.
    // Bootstrap provisioning does not mean an explicitly selected file ran.
    let mut prepared_sources = BTreeSet::new();
    for source in inputs {
        let identity = source
            .namespace
            .as_deref()
            .map(|namespace| {
                ModuleIdentity::new(Phase::Runtime, namespace).map_err(SessionError::Compile)
            })
            .transpose()?;
        if identity
            .as_ref()
            .is_some_and(|identity| prepared_sources.contains(identity))
        {
            continue;
        }
        let origin = portable::SourceOrigin::new(source.source.as_str(), source.path.clone());
        let (prepared, completed) = prepare_script_compiled_batch(
            snapshot,
            &mut macros,
            source.forms.clone(),
            source.source.len(),
            &origin,
        )?;
        snapshot = completed;
        if let Some(identity) = identity {
            snapshot.provided.insert(identity.clone());
            prepared_sources.insert(identity);
        }
        for input in prepared {
            let PreparedScript::Runtime(input) = input else {
                continue;
            };
            // All declarations are already staged by the common source pipeline.
            // Module artifacts retain their own immutable source/phase identities.
            for module in input.modules {
                if inputs.iter().any(|selected| {
                    selected.path.as_ref() == Some(&module.source_path)
                        && selected.source != module.source
                }) {
                    return Err(SessionError::Compile(portable::Diagnostic {
                        span: 0..0,
                        message: format!(
                            "Selected project source changed during compilation: {}",
                            module.source_path.display()
                        ),
                    }));
                }
                prepared_sources.insert(module.identity.clone());
                fragments.push(PreparedFragment {
                    wasm: module.wasm,
                    environment: input.fragment.environment.clone(),
                    cells: input.fragment.cells.clone(),
                    namespace_directive: None,
                });
            }
            fragments.push(input.fragment);
        }
    }
    Ok(fragments)
}
