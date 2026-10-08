//! Source files use the same staged compilation as scripts before AOT assembly.
use crate::portable::{self, PreparedFragment, modules::ModuleIdentity, resolve::Phase};
use crate::{
    CompileError, CompileResult,
    portable_macros::CompiledMacros,
    portable_repl::{PreparedScript, prepare_script_compiled_batch, read_script_forms},
    portable_session::{CompilationSnapshot, SessionError},
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use suss_reader::Symbol;

/// One resolved world import exposed as a phase-qualified source var.
#[derive(Debug, Clone)]
pub struct AsyncImportMapping {
    pub wit_name: String,
    pub source: Symbol,
}
/// Explicit source mappings for AOT assembly. Empty async imports preserve the
/// existing synchronous/non-suspending component route.
#[derive(Debug, Clone, Default)]
pub struct AotOptions {
    pub exports: Vec<(String, Symbol)>,
    pub async_imports: Vec<AsyncImportMapping>,
}

/// Compile through the normal staged source pipeline with canonical imports.
/// The current transport supports one world-level scalar async import/export;
/// all source/dependency fragments still initialize in their prepared order.
pub fn compile_file_with_options(
    source_path: &Path,
    wit_path: &Path,
    wit_world: Option<&str>,
    source_paths: &[PathBuf],
    options: &AotOptions,
) -> Result<Vec<u8>, String> {
    (|| -> CompileResult<Vec<u8>> {
        let source = std::fs::read_to_string(source_path).map_err(|error| {
            CompileError::Io(format!("Failed to read {}: {error}", source_path.display()))
        })?;
        let forms = read_script_forms(&source)
            .map_err(|error| CompileError::Parse(format!("{}: {error}", source_path.display())))?;
        compile_inputs_with_options(
            &[SourceInput {
                source,
                path: Some(source_path.to_owned()),
                forms,
                namespace: None,
            }],
            wit_path,
            wit_world,
            source_paths,
            options,
        )
    })()
    .map_err(|error| error.to_string())
}

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
    compile_file_with_options(
        source_path,
        wit_path,
        wit_world,
        source_paths,
        &AotOptions {
            exports: mappings.to_vec(),
            async_imports: Vec::new(),
        },
    )
}

pub(crate) fn compile_file_typed(
    source_path: &Path,
    wit_path: &Path,
    wit_world: Option<&str>,
    source_paths: &[PathBuf],
    mappings: &[(String, Symbol)],
) -> CompileResult<Vec<u8>> {
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
    compile_main_source(
        &source,
        Some(source_path.to_owned()),
        namespace,
        source_paths,
    )
}

/// Compile source text to the same official command profile as file mode.
/// Namespace validation precedes effectful Macro-session preparation; Runtime
/// initialization remains deferred to the resulting command artifact.
pub(crate) fn compile_main_source(
    source: &str,
    path: Option<PathBuf>,
    namespace: &str,
    source_paths: &[PathBuf],
) -> Result<Vec<u8>, String> {
    let forms = read_script_forms(source).map_err(|error| error.to_string())?;
    portable::modules::validate_namespace_source(namespace, &forms, Phase::Runtime)
        .map_err(|error| error.to_string())?;
    let fragments = prepare_inputs_with_command(
        &[SourceInput {
            source: source.to_owned(),
            path,
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
    compile_namespace_typed(namespace, wit_path, wit_world, source_paths, mappings)
        .map_err(|error| error.to_string())
}

pub(crate) fn compile_namespace_typed(
    namespace: &str,
    wit_path: &Path,
    wit_world: Option<&str>,
    source_paths: &[PathBuf],
    mappings: &[(String, Symbol)],
) -> CompileResult<Vec<u8>> {
    let path = portable::resolve::locate_source(namespace, source_paths, 0..0)
        .map_err(|error| CompileError::Semantic(error.to_string()))?;
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
) -> CompileResult<Vec<u8>> {
    let source = std::fs::read_to_string(source_path).map_err(|error| {
        CompileError::Io(format!("Failed to read {}: {error}", source_path.display()))
    })?;
    let forms = read_script_forms(&source)
        .map_err(|error| CompileError::Parse(format!("{}: {error}", source_path.display())))?;
    if let Some(namespace) = namespace {
        portable::modules::validate_namespace_source(namespace, &forms, Phase::Runtime).map_err(
            |error| CompileError::Semantic(format!("{}: {error}", source_path.display())),
        )?;
    }
    compile_inputs_typed(
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

pub(crate) fn compile_inputs_typed(
    inputs: &[SourceInput],
    wit_path: &Path,
    wit_world: Option<&str>,
    source_paths: &[PathBuf],
    mappings: &[(String, Symbol)],
) -> CompileResult<Vec<u8>> {
    compile_inputs_with_options(
        inputs,
        wit_path,
        wit_world,
        source_paths,
        &AotOptions {
            exports: mappings.to_vec(),
            async_imports: Vec::new(),
        },
    )
}

fn compile_inputs_with_options(
    inputs: &[SourceInput],
    wit_path: &Path,
    wit_world: Option<&str>,
    source_paths: &[PathBuf],
    options: &AotOptions,
) -> CompileResult<Vec<u8>> {
    let mut resolve = portable::aot::Resolve::new();
    let (package, _) = resolve.push_path(wit_path).map_err(|error| {
        let message = format!("Failed to resolve WIT {}: {error:#}", wit_path.display());
        if error.downcast_ref::<std::io::Error>().is_some() {
            CompileError::Io(message)
        } else {
            CompileError::Wit(message)
        }
    })?;
    let world = resolve
        .select_world(&[package], wit_world)
        .map_err(|error| CompileError::Wit(format!("Failed to select WIT world: {error:#}")))?;
    if options.async_imports.is_empty() {
        portable::aot::validate_boundary(&resolve, world)
            .map_err(|error| CompileError::Unsupported(error.to_string()))?;
        let fragments = prepare_inputs(inputs, source_paths)
            .map_err(|error| CompileError::Semantic(error.to_string()))?;
        let mappings =
            portable::aot::source_export_mappings(&fragments, &resolve, world, &options.exports)
                .map_err(|error| CompileError::ExportMismatch(error.to_string()))?;
        return portable::aot::component(&fragments, &resolve, world, &mappings)
            .map_err(|error| CompileError::Component(error.to_string()));
    }
    // Validate the entire selected transport scope before Macro-session effects.
    let selected = &resolve.worlds[world];
    if options.async_imports.len() != 1
        || options.exports.len() != 1
        || selected.imports.len() != 1
        || selected.exports.len() != 1
    {
        return Err(CompileError::Unsupported(
            "Serial scalar async AOT requires one explicit world import and export".into(),
        ));
    }
    let import_mapping = &options.async_imports[0];
    let (export_name, export_symbol) = &options.exports[0];
    let import = selected
        .imports
        .values()
        .find_map(|item| match item {
            wit_parser::WorldItem::Function(function)
                if function.name == import_mapping.wit_name =>
            {
                Some(function)
            }
            _ => None,
        })
        .ok_or_else(|| {
            CompileError::Wit("Async import mapping does not name a selected world function".into())
        })?;
    let export = selected
        .exports
        .values()
        .find_map(|item| match item {
            wit_parser::WorldItem::Function(function) if function.name == *export_name => {
                Some(function)
            }
            _ => None,
        })
        .ok_or_else(|| {
            CompileError::ExportMismatch(
                "Async export mapping does not name a selected world function".into(),
            )
        })?;
    for function in [import, export] {
        if function.external_id.is_some() {
            return Err(CompileError::Unsupported(
                "Async external-id adapters are unsupported".into(),
            ));
        }
        portable::command::async_component::check_scalar(&resolve, function)
            .map_err(|error| CompileError::Unsupported(error.to_string()))?;
    }
    let namespace = import_mapping
        .source
        .namespace
        .as_deref()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            CompileError::Semantic("Async import source mapping must be namespace-qualified".into())
        })?;
    if export_symbol.namespace.is_none()
        || import_mapping.source.name.is_empty()
        || import_mapping.source == *export_symbol
        || matches!(namespace, "suss.core" | "cljs.core")
    {
        return Err(CompileError::Semantic(
            "Async mappings require distinct qualified source vars outside core imports".into(),
        ));
    }
    if portable::resolve::locate_source_if_present(namespace, source_paths, 0..0)
        .map_err(|error| CompileError::Semantic(error.to_string()))?
        .is_some()
    {
        return Err(CompileError::Semantic(format!(
            "Async host namespace {namespace} is owned by a source module; import mapping would hide its initialization"
        )));
    }
    let fragments = prepare_inputs_with_imports(
        inputs,
        source_paths,
        false,
        std::slice::from_ref(&import_mapping.source),
    )
    .map_err(|error| CompileError::Semantic(error.to_string()))?;
    // Retain the actual predeclared identity; final aliases must not redirect it.
    let import_global = fragments[0]
        .cells
        .iter()
        .find(|cell| {
            cell.phase() == Phase::Runtime
                && cell.namespace() == namespace
                && cell.name() == import_mapping.source.name
        })
        .ok_or_else(|| CompileError::Semantic("Predeclared async import cell is missing".into()))?;
    let export_namespace = export_symbol
        .namespace
        .as_deref()
        .expect("validated qualified export");
    let export_global = fragments
        .iter()
        .rev()
        .flat_map(|fragment| fragment.cells.iter())
        .find(|cell| {
            cell.phase() == Phase::Runtime
                && cell.namespace() == export_namespace
                && cell.name() == export_symbol.name
        })
        .ok_or_else(|| {
            CompileError::ExportMismatch(
                "Async export requires a literal source namespace definition".into(),
            )
        })?;
    portable::command::async_component::component(
        &fragments,
        &resolve,
        portable::command::async_component::ImportMapping {
            function: import,
            global: import_global,
        },
        portable::command::async_component::ExportMapping {
            function: export,
            global: export_global,
        },
    )
    .map_err(|error| CompileError::Component(error.to_string()))
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
    prepare_inputs_with_imports(inputs, source_paths, command, &[])
}

fn prepare_inputs_with_imports(
    inputs: &[SourceInput],
    source_paths: &[PathBuf],
    command: bool,
    imports: &[Symbol],
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
    for symbol in imports {
        let cell = core
            .environment
            .declare_cell(
                Phase::Runtime,
                symbol
                    .namespace
                    .as_deref()
                    .expect("validated qualified import"),
                &symbol.name,
            )
            .map_err(SessionError::Compile)?;
        core.cells.push(cell);
    }
    let mut snapshot = CompilationSnapshot::new(
        core.environment.clone(),
        Phase::Runtime,
        source_paths.to_vec(),
        BTreeSet::from([
            ModuleIdentity::new(Phase::Runtime, "suss.core").map_err(SessionError::Compile)?
        ]),
    );
    for symbol in imports {
        snapshot.provided.insert(
            ModuleIdentity::new(
                Phase::Runtime,
                symbol
                    .namespace
                    .as_deref()
                    .expect("validated qualified import"),
            )
            .map_err(SessionError::Compile)?,
        );
    }
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
