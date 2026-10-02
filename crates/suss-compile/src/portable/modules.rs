//! Immutable dependency graph preparation. Runtime initialization belongs to the host.
use super::{
    Diagnostic, PreparedFragment, prepare_fragment, prepare_selected_fragment,
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
    /// None means a root lookup failure or an inline input require edge.
    /// Lookup failures have an empty span; inline input spans locate its text.
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
/// Exact selected forms and source text used by phase dependency discovery.
#[derive(Debug)]
pub struct SourceModule {
    pub identity: ModuleIdentity,
    pub path: PathBuf,
    pub source: String,
    pub forms: Vec<suss_reader::forms::Form>,
    pub dependencies: Vec<ModuleIdentity>,
}
struct Discovery<'a, P> {
    roots: &'a [P],
    environment: &'a Environment,
    provided: &'a BTreeSet<ModuleIdentity>,
    active: Vec<ModuleIdentity>,
    done: BTreeSet<ModuleIdentity>,
    snapshots: Vec<SourceModule>,
    phase_graph: bool,
    macro_imports: bool,
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
                .map(|module| {
                    if self.phase_graph {
                        format!("{:?}:{}", module.phase, module.namespace)
                    } else {
                        module.namespace.clone()
                    }
                })
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
        let (declared, declared_span, dependencies) = if self.phase_graph {
            source::phase_dependencies(&forms, identity.phase)
        } else {
            source::input_header_with_macros(&forms, self.macro_imports)
                .and_then(|header| {
                    header.ok_or_else(|| Diagnostic {
                        span: forms.first().map_or(0..0, |f| f.span.clone()),
                        message: "Source module requires a leading ns declaration".into(),
                    })
                })
                .map(|(name, span, edges)| {
                    (
                        name,
                        span,
                        edges
                            .into_iter()
                            .map(|(name, span)| (identity.phase, name, span))
                            .collect(),
                    )
                })
        }
        .map_err(|error| located(&identity, Some(&path), error))?;
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
        for (phase, namespace, span) in dependencies {
            let dependency = ModuleIdentity::new(phase, &namespace).map_err(|mut error| {
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
        self.snapshots.push(SourceModule {
            identity,
            path,
            source: text,
            forms,
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
    prepare_modules_with_expander(
        namespace,
        roots,
        environment,
        phase,
        provided,
        &mut super::NoExpansion,
    )
}
/// Prepare explicitly loaded/reloaded source with the same lexical macro host
/// used for input fragments; no initializer runs during preparation.
pub fn prepare_modules_with_expander<P: AsRef<Path>>(
    namespace: &str,
    roots: &[P],
    environment: &Environment,
    phase: Phase,
    provided: &BTreeSet<ModuleIdentity>,
    expander: &mut dyn super::ExpansionHost,
) -> Result<ModulePlan, ModuleDiagnostic> {
    let identity = ModuleIdentity::new(phase, namespace).map_err(|error| ModuleDiagnostic {
        namespace: namespace.into(),
        source_path: None,
        span: error.span,
        message: error.message,
    })?;
    expander.source_paths(
        &roots
            .iter()
            .map(|root| root.as_ref().to_path_buf())
            .collect::<Vec<_>>(),
    );
    let mut discovery = Discovery {
        roots,
        environment,
        provided,
        active: Vec::new(),
        done: BTreeSet::new(),
        snapshots: Vec::new(),
        phase_graph: false,
        macro_imports: expander.supports_macro_imports(),
    };
    discovery.visit(identity.clone(), None, 0..0)?;
    let (modules, mut snapshot) =
        compile_snapshots_with_expander(discovery.snapshots, environment, phase, expander)?;
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

fn compile_snapshots_with_expander(
    snapshots: Vec<SourceModule>,
    environment: &Environment,
    phase: Phase,
    expander: &mut dyn super::ExpansionHost,
) -> Result<(Vec<PreparedModule>, Environment), ModuleDiagnostic> {
    let mut snapshot = environment.clone();
    let mut modules = Vec::new();
    for unit in snapshots {
        let PreparedFragment {
            wasm, environment, ..
        } = super::prepare_fragment_forms_with_origin(
            unit.forms,
            0..unit.source.len(),
            &snapshot,
            phase,
            expander,
            Some(&super::SourceOrigin::new(unit.source.as_str(), Some(unit.path.clone()))),
        )
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
    Ok((modules, snapshot))
}

#[derive(Debug, thiserror::Error)]
pub enum InputDiagnostic {
    #[error(transparent)]
    Compile(Diagnostic),
    #[error(transparent)]
    Dependency(ModuleDiagnostic),
}
/// Dependencies and the input are compiled together before any host publication.
pub struct PreparedInput {
    pub modules: Vec<PreparedModule>,
    pub fragment: PreparedFragment,
}
/// Prepare one incremental input, discovering dependencies from its optional leading
/// ns using the same grammar as file modules. An error changes no caller state.
pub fn prepare_input<P: AsRef<Path>>(
    source_text: &str,
    roots: &[P],
    environment: &Environment,
    phase: Phase,
    provided: &BTreeSet<ModuleIdentity>,
) -> Result<PreparedInput, InputDiagnostic> {
    let forms = read_forms(source_text).map_err(|error| {
        InputDiagnostic::Compile(Diagnostic {
            span: error.span,
            message: error.message,
        })
    })?;
    prepare_input_forms_with_origin(forms, 0..source_text.len(), roots, environment, phase,
        provided, &mut super::NoExpansion, Some(&super::SourceOrigin::new(source_text, None)))
}
/// Prepare source or macro-expanded reader forms through the same staged module
/// graph and fragment pipeline, preserving metadata and caller source locations.
pub fn prepare_input_forms<P: AsRef<Path>>(
    forms: Vec<suss_reader::forms::Form>,
    span: Range<usize>,
    roots: &[P],
    environment: &Environment,
    phase: Phase,
    provided: &BTreeSet<ModuleIdentity>,
) -> Result<PreparedInput, InputDiagnostic> {
    prepare_input_forms_with_expander(
        forms,
        span,
        roots,
        environment,
        phase,
        provided,
        &mut super::NoExpansion,
    )
}
pub fn prepare_input_forms_with_expander<P: AsRef<Path>>(
    forms: Vec<suss_reader::forms::Form>,
    span: Range<usize>,
    roots: &[P],
    environment: &Environment,
    phase: Phase,
    provided: &BTreeSet<ModuleIdentity>,
    expander: &mut dyn super::ExpansionHost,
) -> Result<PreparedInput, InputDiagnostic> {
    prepare_input_forms_with_origin(forms, span, roots, environment, phase, provided, expander, None)
}
/// Supply the immutable root input origin separately from its dependency files.
pub fn prepare_input_forms_with_origin<P: AsRef<Path>>(
    forms: Vec<suss_reader::forms::Form>, span: Range<usize>, roots: &[P],
    environment: &Environment, phase: Phase, provided: &BTreeSet<ModuleIdentity>,
    expander: &mut dyn super::ExpansionHost, origin: Option<&super::SourceOrigin>,
) -> Result<PreparedInput, InputDiagnostic> {
    let forms = resolve_conditionals(forms).map_err(|error| {
        InputDiagnostic::Compile(Diagnostic {
            span: error.span,
            message: error.message,
        })
    })?;
    let header = source::input_header_with_macros(&forms, expander.supports_macro_imports())
        .map_err(InputDiagnostic::Compile)?;
    expander.source_paths(
        &roots
            .iter()
            .map(|root| root.as_ref().to_path_buf())
            .collect::<Vec<_>>(),
    );
    let mut discovery = Discovery {
        roots,
        environment,
        provided,
        active: Vec::new(),
        done: BTreeSet::new(),
        snapshots: Vec::new(),
        phase_graph: false,
        macro_imports: expander.supports_macro_imports(),
    };
    if let Some((_, _, dependencies)) = header {
        for (namespace, span) in dependencies {
            let identity =
                ModuleIdentity::new(phase, &namespace).map_err(InputDiagnostic::Compile)?;
            discovery
                .visit(identity, None, span)
                .map_err(InputDiagnostic::Dependency)?;
        }
    }
    let (modules, mut snapshot) =
        compile_snapshots_with_expander(discovery.snapshots, environment, phase, expander)
            .map_err(InputDiagnostic::Dependency)?;
    snapshot
        .enter_namespace(phase, environment.current_namespace(phase))
        .map_err(InputDiagnostic::Compile)?;
    let fragment =
        super::prepare_selected_fragment_with_origin(forms, span, &snapshot, phase, expander, origin)
            .map_err(InputDiagnostic::Compile)?;
    Ok(PreparedInput { modules, fragment })
}

/// Read one immutable, dependency-first source graph spanning Runtime and Macro
/// identities. This neither compiles nor runs initializers. A host must provide
/// actual phase catalogs and mark modules provided only after successful execution.
/// The same physical file can occur in both phases; phase identity is never erased.
pub fn discover_phase_modules<P: AsRef<Path>>(
    namespace: &str,
    roots: &[P],
    environment: &Environment,
    phase: Phase,
    provided: &BTreeSet<ModuleIdentity>,
) -> Result<Vec<SourceModule>, ModuleDiagnostic> {
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
        phase_graph: true,
        macro_imports: true,
    };
    discovery.visit(identity, None, 0..0)?;
    Ok(discovery.snapshots)
}
