//! Owned expression artifacts prepared by the common compiled source pipeline.
//! Execution consumes prepared modules; it never reparses or replays source.
use crate::{
    portable::{self, PreparedFragment, modules::ModuleIdentity, resolve::Phase},
    portable_macros::CompiledMacros,
    portable_repl::{PreparedScript, prepare_script_compiled_batch, read_script_forms},
    portable_session::{CompilationSnapshot, Session, SessionError, SessionValue},
};
use std::{collections::BTreeSet, path::PathBuf};

/// Host-owned compiled inputs, including their bootstrap and dependency plans.
/// This is not a standalone core module: ABI2 fragments import a shared runtime.
pub struct ExpressionArtifact {
    pub(crate) core: PreparedFragment,
    pub(crate) inputs: Vec<portable::modules::PreparedInput>,
}

impl std::fmt::Debug for ExpressionArtifact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExpressionArtifact")
            .field("modules", &self.modules().count())
            .finish()
    }
}

impl ExpressionArtifact {
    /// Inspect every emitted module in initialization order, without source text.
    pub fn modules(&self) -> impl Iterator<Item = &[u8]> {
        std::iter::once(self.core.wasm.as_slice()).chain(self.inputs.iter().flat_map(|input| {
            input
                .modules
                .iter()
                .map(|module| module.wasm.as_slice())
                .chain(std::iter::once(input.fragment.wasm.as_slice()))
        }))
    }

    /// Initialize in an empty Runtime session, retaining rooted results and state.
    /// A session that already contains fragments is rejected before any effects;
    /// subsequent interactive inputs should use its incremental compilation API.
    pub fn execute(self, session: &mut Session) -> Result<Option<SessionValue>, SessionError> {
        self.execute_with_core(session, |_| Ok(()))
            .map(|(value, ())| value)
    }

    /// Register host observations of canonical core values before user code can
    /// redefine them. The complete artifact is validated before this callback.
    /// Host callback effects, like preceding language effects, are not rolled back
    /// if a later user initializer fails.
    pub fn execute_with_core<T>(
        self,
        session: &mut Session,
        core_ready: impl FnOnce(&mut Session) -> Result<T, SessionError>,
    ) -> Result<(Option<SessionValue>, T), SessionError> {
        session.execute_expression_artifact(self, core_ready)
    }
}

pub(crate) fn prepare(
    source: &str,
    source_paths: &[PathBuf],
) -> Result<ExpressionArtifact, SessionError> {
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
    let mut macros = CompiledMacros::new()?;
    let origin = portable::SourceOrigin::new(source, None);
    let (prepared, _) =
        prepare_script_compiled_batch(snapshot, &mut macros, forms, source.len(), &origin)?;
    let inputs = prepared
        .into_iter()
        .filter_map(|input| match input {
            PreparedScript::Runtime(input) => Some(input),
            _ => None,
        })
        .collect();
    Ok(ExpressionArtifact { core, inputs })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expression_artifact_invalid_last_module_rejects_entire_bundle() {
        let mut artifact = prepare("(def seen (atom 0)) (swap! seen inc)", &[]).unwrap();
        artifact.inputs.last_mut().unwrap().fragment.wasm.push(0xff);
        let mut session = Session::new().unwrap();
        let before = session.stats().resident_fragments;
        assert!(matches!(
            artifact.execute(&mut session),
            Err(SessionError::Host(_))
        ));
        assert_eq!(session.stats().resident_fragments, before);
        assert!(
            session.eval("seen").is_err(),
            "no initializer or binding was published"
        );
        session.eval("42").unwrap();
    }
}
