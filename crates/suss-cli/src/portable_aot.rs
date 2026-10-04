//! Source files use the same staged compilation as scripts before AOT assembly.
use crate::{
    portable_macros::CompiledMacros,
    portable_repl::{prepare_script_compiled, read_script_forms, PreparedScript},
    portable_session::{CompilationSnapshot, SessionError},
};
use std::{collections::BTreeSet, path::PathBuf};
use suss_compile::portable::{self, modules::ModuleIdentity, resolve::Phase, PreparedFragment};

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
