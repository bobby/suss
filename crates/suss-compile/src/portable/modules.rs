//! Immutable dependency graph preparation. Runtime initialization belongs to the host.
use super::{
    Diagnostic, PreparedFragment, prepare_fragment,
    resolve::{self, Environment, Phase},
    source,
};
use std::{
    collections::BTreeSet,
    ops::Range,
    path::{Path, PathBuf},
};
use suss_reader::forms::{read_forms, resolve_conditionals};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModuleIdentity {
    phase: Phase,
    namespace: String,
}
impl ModuleIdentity {
    pub fn new(phase: Phase, namespace: &str) -> Result<Self, Diagnostic> {
        resolve::valid_namespace(namespace)?;
        Ok(Self {
            phase,
            namespace: resolve::canonical(namespace).into(),
        })
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn namespace(&self) -> &str {
        &self.namespace
    }
}
#[derive(Debug, thiserror::Error)]
#[error("{namespace} ({source_path:?}): {message} at bytes {span:?}")]
pub struct ModuleDiagnostic {
    pub namespace: String,
    /// Location of the source containing the diagnostic, including require edges.
    /// None means the root source could not be located; no byte location is invented.
    pub source_path: Option<PathBuf>,
    pub span: Range<usize>,
    pub message: String,
}
fn located(
    identity: &ModuleIdentity,
    path: Option<&Path>,
    diagnostic: Diagnostic,
) -> ModuleDiagnostic {
    ModuleDiagnostic {
        namespace: identity.namespace.clone(),
        source_path: path.map(Path::to_path_buf),
        span: diagnostic.span,
        message: diagnostic.message,
    }
}
#[derive(Debug)]
pub struct PreparedModule {
    pub identity: ModuleIdentity,
    pub source_path: PathBuf,
    /// Exact immutable text used to discover and compile this module.
    pub source: String,
    pub dependencies: Vec<ModuleIdentity>,
    pub wasm: Vec<u8>,
}
#[derive(Debug)]
pub struct ModulePlan {
    /// Dependency-first, preserving require order; each identity occurs once.
    pub modules: Vec<PreparedModule>,
    pub environment: Environment,
    pub cells: Vec<resolve::Global>,
}
struct Snapshot {
    identity: ModuleIdentity,
    path: PathBuf,
    source: String,
    dependencies: Vec<ModuleIdentity>,
}
struct Discovery<'a, P> {
    roots: &'a [P],
    environment: &'a Environment,
    provided: &'a BTreeSet<ModuleIdentity>,
    active: Vec<ModuleIdentity>,
    done: BTreeSet<ModuleIdentity>,
    snapshots: Vec<Snapshot>,
}
impl<P: AsRef<Path>> Discovery<'_, P> {
    fn visit(
        &mut self,
        identity: ModuleIdentity,
        origin: Option<&Path>,
        span: Range<usize>,
    ) -> Result<(), ModuleDiagnostic> {
        let fail = |message: String| {
            located(
                &identity,
                origin,
                Diagnostic {
                    span: span.clone(),
                    message,
                },
            )
        };
        if self.provided.contains(&identity) {
            if !self
                .environment
                .has_namespace(identity.phase, &identity.namespace)
            {
                return Err(fail(
                    "Provided module has no matching phase declaration".into(),
                ));
            }
            return Ok(());
        }
        if self.done.contains(&identity) {
            return Ok(());
        }
        if let Some(index) = self.active.iter().position(|module| *module == identity) {
            let chain = self.active[index..]
                .iter()
                .chain(std::iter::once(&identity))
                .map(|module| module.namespace.as_str())
                .collect::<Vec<_>>()
                .join(" -> ");
            return Err(fail(format!("Cyclic namespace dependency: {chain}")));
        }
        if self.active.len() >= 64 {
            return Err(fail("Namespace dependency nesting exceeds 64".into()));
        }
        let path = resolve::locate_source(&identity.namespace, self.roots, span.clone())
            .map_err(|error| located(&identity, origin, error))?;
        let text = std::fs::read_to_string(&path).map_err(|error| {
            located(
                &identity,
                Some(&path),
                Diagnostic {
                    span: 0..0,
                    message: format!("Cannot read namespace source: {error}"),
                },
            )
        })?;
        let forms = read_forms(&text)
            .and_then(resolve_conditionals)
            .map_err(|error| {
                located(
                    &identity,
                    Some(&path),
                    Diagnostic {
                        span: error.span,
                        message: error.message,
                    },
                )
            })?;
        let (declared, declared_span, dependencies) =
            source::dependencies(&forms).map_err(|error| located(&identity, Some(&path), error))?;
        if resolve::canonical(&declared) != identity.namespace {
            return Err(located(
                &identity,
                Some(&path),
                Diagnostic {
                    span: declared_span,
                    message: format!(
                        "Declared namespace {declared} does not match requested {}",
                        identity.namespace
                    ),
                },
            ));
        }
        self.active.push(identity.clone());
        let mut ordered = Vec::new();
        let mut seen = BTreeSet::new();
        for (namespace, span) in dependencies {
            let dependency =
                ModuleIdentity::new(identity.phase, &namespace).map_err(|mut error| {
                    error.span = span.clone();
                    located(&identity, Some(&path), error)
                })?;
            if seen.insert(dependency.clone()) {
                self.visit(dependency.clone(), Some(&path), span)?;
                ordered.push(dependency);
            }
        }
        self.active.pop();
        self.done.insert(identity.clone());
        self.snapshots.push(Snapshot {
            identity,
            path,
            source: text,
            dependencies: ordered,
        });
        Ok(())
    }
}
/// Discover and compile the entire source graph without mutating the caller or
/// executing initializers. `provided` is explicit host authority: already initialized
/// modules (or the bounded bootstrap core) whose catalog/cells the host supplies.
/// Merely declaring a namespace never makes it loaded. Mark new identities provided
/// only after successful initialization; reuse their cells on subsequent plans.
pub fn prepare_modules<P: AsRef<Path>>(
    namespace: &str,
    roots: &[P],
    environment: &Environment,
    phase: Phase,
    provided: &BTreeSet<ModuleIdentity>,
) -> Result<ModulePlan, ModuleDiagnostic> {
    let identity = ModuleIdentity::new(phase, namespace).map_err(|error| ModuleDiagnostic {
        namespace: namespace.into(),
        source_path: None,
        span: error.span,
        message: error.message,
    })?;
    let mut discovery = Discovery {
        roots,
        environment,
        provided,
        active: Vec::new(),
        done: BTreeSet::new(),
        snapshots: Vec::new(),
    };
    discovery.visit(identity.clone(), None, 0..0)?;
    let mut snapshot = environment.clone();
    let mut modules = Vec::new();
    for unit in discovery.snapshots {
        let PreparedFragment {
            wasm, environment, ..
        } = prepare_fragment(&unit.source, &snapshot, phase)
            .map_err(|error| located(&unit.identity, Some(&unit.path), error))?;
        snapshot = environment;
        modules.push(PreparedModule {
            identity: unit.identity,
            source_path: unit.path,
            source: unit.source,
            dependencies: unit.dependencies,
            wasm,
        });
    }
    snapshot
        .enter_namespace(phase, &identity.namespace)
        .map_err(|error| located(&identity, None, error))?;
    let cells = snapshot
        .cells()
        .into_iter()
        .filter(|cell| cell.phase() == phase)
        .collect();
    Ok(ModulePlan {
        modules,
        environment: snapshot,
        cells,
    })
}
