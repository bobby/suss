//! Immutable dependency graph preparation. Runtime initialization belongs to the host.
use super::{
    Diagnostic, ExpansionHost, PreparedFragment, prepare_fragment, prepare_selected_fragment,
    resolve::{self, Environment, Phase},
    source,
};
use std::{
    collections::BTreeSet,
    io::Read,
    ops::Range,
    path::{Path, PathBuf},
};
use suss_reader::forms::{read_forms_metered, resolve_conditionals_metered};

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
/// Validate a selected source module's leading namespace against its requested
/// identity using the same phase-aware header grammar as dependency discovery.
/// This performs no filesystem reads, compilation or macro/Runtime effects.
pub fn validate_namespace_source(
    namespace: &str,
    forms: &[suss_reader::forms::Form],
    phase: Phase,
) -> Result<(), Diagnostic> {
    let identity = ModuleIdentity::new(phase, namespace)?;
    let (declared, span, _) = source::phase_dependencies(forms, phase)?;
    validate_declared_namespace(&identity, &declared, span)
}
/// Validated leading namespace and optional retained project world target.
/// This shares the dependency/source header grammar and performs no effects.
pub fn project_namespace_source(
    forms: &[suss_reader::forms::Form],
) -> Result<Option<(String, Option<String>)>, Diagnostic> {
    source::project_declaration(forms)
}

fn validate_declared_namespace(
    identity: &ModuleIdentity,
    declared: &str,
    span: Range<usize>,
) -> Result<(), Diagnostic> {
    if resolve::canonical(declared) != identity.namespace {
        return Err(Diagnostic {
            span,
            message: format!(
                "Declared namespace {declared} does not match requested {}",
                identity.namespace
            ),
        });
    }
    Ok(())
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
/// Native discovery work. A compiled host may debit its current Session fuel;
/// standalone discovery supplies a finite allowance without a structural cap.
pub type DiscoveryWorkMeter<'a> = dyn FnMut(u64) -> Result<(), Diagnostic> + 'a;

fn charge_discovery(
    meter: &mut DiscoveryWorkMeter<'_>,
    units: u64,
    identity: &ModuleIdentity,
    path: Option<&Path>,
    span: Range<usize>,
) -> Result<(), ModuleDiagnostic> {
    meter(units).map_err(|mut diagnostic| {
        if diagnostic.span == (0..0) {
            diagnostic.span = span;
        }
        located(identity, path, diagnostic)
    })
}
fn charge_reader(
    meter: &mut DiscoveryWorkMeter<'_>,
    work: u64,
) -> Result<(), suss_reader::ParseError> {
    meter(work).map_err(|error| suss_reader::ParseError {
        span: error.span,
        message: error.message,
        expected: Vec::new(),
    })
}
fn source_paths_metered<P: AsRef<Path>>(
    roots: &[P],
    expander: &mut dyn super::ExpansionHost,
) -> Result<Vec<PathBuf>, Diagnostic> {
    expander.charge_expansion_work(roots.len() as u64, 0..0)?;
    let mut paths = Vec::new();
    for root in roots {
        let path = root.as_ref();
        expander.charge_expansion_work(path.as_os_str().len() as u64, 0..0)?;
        paths.push(path.to_path_buf());
    }
    Ok(paths)
}
fn parse_source(
    text: &str,
    meter: &mut DiscoveryWorkMeter<'_>,
) -> Result<Vec<suss_reader::forms::Form>, Diagnostic> {
    let mut reader = |work| charge_reader(meter, work);
    read_forms_metered(text, &mut reader)
        .and_then(|forms| resolve_conditionals_metered(forms, &mut reader))
        .map_err(|error| Diagnostic {
            span: error.span,
            message: error.message,
        })
}
// Charge the header walk before its grammar parser, including forms and text
// that parsing may copy. Reading/selection and header processing are distinct work.
fn charge_header(
    forms: &[suss_reader::forms::Form],
    meter: &mut DiscoveryWorkMeter<'_>,
) -> Result<(), Diagnostic> {
    use suss_reader::forms::Kind;
    meter(forms.len() as u64)?;
    let mut pending: Vec<_> = forms.iter().collect();
    while let Some(form) = pending.pop() {
        let children = match &form.kind {
            Kind::List(x)
            | Kind::Vector(x)
            | Kind::Map(x)
            | Kind::Set(x)
            | Kind::Conditional(x)
            | Kind::SplicingConditional(x) => x.as_slice(),
            _ => &[],
        };
        let text = match &form.kind {
            Kind::String(x) => x.len(),
            Kind::Symbol(x) => x.name.len() + x.namespace.as_ref().map_or(0, String::len),
            Kind::Keyword(x) => x.name.len() + x.namespace.as_ref().map_or(0, String::len),
            _ => 0,
        };
        meter(
            1_u64
                .saturating_add(children.len() as u64)
                .saturating_add(form.metadata.len() as u64)
                .saturating_add(text as u64),
        )
        .map_err(|mut error| {
            if error.span == (0..0) {
                error.span = form.span.clone();
            }
            error
        })?;
        pending.extend(children);
        pending.extend(&form.metadata);
        match &form.kind {
            Kind::Discard(x) => pending.push(x),
            Kind::Prefix { operator, target } => {
                pending.extend([operator.as_ref(), target.as_ref()])
            }
            _ => {}
        }
    }
    Ok(())
}
struct Discovery<'a, 'm, 'f, P> {
    roots: &'a [P],
    environment: &'a Environment,
    provided: &'a BTreeSet<ModuleIdentity>,
    active: Vec<ModuleIdentity>,
    done: BTreeSet<ModuleIdentity>,
    snapshots: Vec<SourceModule>,
    phase_graph: bool,
    macro_imports: bool,
    meter: &'m mut DiscoveryWorkMeter<'f>,
}
type DependencyEdges = std::vec::IntoIter<(Phase, String, Range<usize>)>;
enum DiscoveryTask {
    Enter {
        identity: ModuleIdentity,
        origin: Option<PathBuf>,
        span: Range<usize>,
    },
    Resume {
        unit: SourceModule,
        edges: DependencyEdges,
        seen: BTreeSet<ModuleIdentity>,
    },
}
impl<P: AsRef<Path>> Discovery<'_, '_, '_, P> {
    fn visit(
        &mut self,
        identity: ModuleIdentity,
        origin: Option<&Path>,
        span: Range<usize>,
    ) -> Result<(), ModuleDiagnostic> {
        let mut pending = vec![DiscoveryTask::Enter {
            identity,
            origin: origin.map(Path::to_path_buf),
            span,
        }];
        while let Some(task) = pending.pop() {
            match task {
                DiscoveryTask::Enter {
                    identity,
                    origin,
                    span,
                } => {
                    let fail = |message| {
                        located(
                            &identity,
                            origin.as_deref(),
                            Diagnostic {
                                span: span.clone(),
                                message,
                            },
                        )
                    };
                    charge_discovery(
                        self.meter,
                        1 + identity.namespace.len() as u64,
                        &identity,
                        origin.as_deref(),
                        span.clone(),
                    )?;
                    if self.provided.contains(&identity) {
                        if !self
                            .environment
                            .has_namespace(identity.phase, &identity.namespace)
                        {
                            return Err(fail(
                                "Provided module has no matching phase declaration".into(),
                            ));
                        }
                        continue;
                    }
                    if self.done.contains(&identity) {
                        continue;
                    }
                    let mut cycle = None;
                    for (index, module) in self.active.iter().enumerate() {
                        charge_discovery(
                            self.meter,
                            1 + module.namespace.len() as u64,
                            &identity,
                            origin.as_deref(),
                            span.clone(),
                        )?;
                        if *module == identity {
                            cycle = Some(index);
                            break;
                        }
                    }
                    if let Some(index) = cycle {
                        let mut chain = Vec::new();
                        for module in self.active[index..]
                            .iter()
                            .chain(std::iter::once(&identity))
                        {
                            charge_discovery(
                                self.meter,
                                8 + module.namespace.len() as u64,
                                &identity,
                                origin.as_deref(),
                                span.clone(),
                            )?;
                            chain.push(if self.phase_graph {
                                format!("{:?}:{}", module.phase, module.namespace)
                            } else {
                                module.namespace.clone()
                            });
                        }
                        return Err(fail(format!(
                            "Cyclic namespace dependency: {}",
                            chain.join(" -> ")
                        )));
                    }
                    charge_discovery(
                        self.meter,
                        1 + self.roots.len() as u64,
                        &identity,
                        origin.as_deref(),
                        span.clone(),
                    )?;
                    for root in self.roots {
                        charge_discovery(
                            self.meter,
                            root.as_ref().as_os_str().len() as u64,
                            &identity,
                            origin.as_deref(),
                            span.clone(),
                        )?;
                    }
                    let file = resolve::locate_source_if_present(
                        &identity.namespace,
                        self.roots,
                        span.clone(),
                    )
                    .map_err(|error| located(&identity, origin.as_deref(), error))?;
                    let (path, text) = if let Some(path) = file {
                        let io_error = |error: std::io::Error| {
                            located(
                                &identity,
                                Some(&path),
                                Diagnostic {
                                    span: 0..0,
                                    message: format!("Cannot read namespace source: {error}"),
                                },
                            )
                        };
                        let mut file = std::fs::File::open(&path).map_err(io_error)?;
                        let mut bytes = Vec::new();
                        // A read buffer is not a source-size limit. Charge each
                        // chunk before retaining it; poll cancellation even at EOF.
                        let mut buffer = [0_u8; 8192];
                        loop {
                            charge_discovery(
                                self.meter,
                                1,
                                &identity,
                                Some(&path),
                                0..bytes.len(),
                            )?;
                            let count = match file.read(&mut buffer) {
                                Ok(count) => count,
                                // Match read_to_string's retry behavior, while
                                // charging and polling again before each retry.
                                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                                    continue;
                                }
                                Err(error) => return Err(io_error(error)),
                            };
                            if count == 0 {
                                break;
                            }
                            charge_discovery(
                                self.meter,
                                count as u64,
                                &identity,
                                Some(&path),
                                bytes.len()..bytes.len() + count,
                            )?;
                            bytes.extend_from_slice(&buffer[..count]);
                        }
                        let text = String::from_utf8(bytes).map_err(|_| {
                            io_error(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "stream did not contain valid UTF-8",
                            ))
                        })?;
                        (path, text)
                    } else if let Some((path, text)) =
                        super::stdlib::source(&identity.namespace, identity.phase)
                    {
                        charge_discovery(
                            self.meter,
                            text.len() as u64,
                            &identity,
                            Some(Path::new(path)),
                            0..text.len(),
                        )?;
                        (PathBuf::from(path), text.to_owned())
                    } else {
                        return Err(fail(format!(
                            "No source for namespace {}",
                            identity.namespace
                        )));
                    };
                    let forms = parse_source(&text, self.meter)
                        .map_err(|error| located(&identity, Some(&path), error))?;
                    charge_header(&forms, self.meter)
                        .map_err(|error| located(&identity, Some(&path), error))?;
                    let (declared, declared_span, edges) = if self.phase_graph {
                        source::phase_dependencies(&forms, identity.phase)
                    } else {
                        source::input_header_with_macros(&forms, self.macro_imports)
                            .and_then(|header| {
                                header.ok_or_else(|| Diagnostic {
                                    span: forms.first().map_or(0..0, |form| form.span.clone()),
                                    message: "Source module requires a leading ns declaration"
                                        .into(),
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
                    validate_declared_namespace(&identity, &declared, declared_span)
                        .map_err(|error| located(&identity, Some(&path), error))?;
                    self.active.push(identity.clone());
                    pending.push(DiscoveryTask::Resume {
                        unit: SourceModule {
                            identity,
                            path,
                            source: text,
                            forms,
                            dependencies: Vec::new(),
                        },
                        edges: edges.into_iter(),
                        seen: BTreeSet::new(),
                    });
                }
                DiscoveryTask::Resume {
                    mut unit,
                    mut edges,
                    mut seen,
                } => {
                    let mut child = None;
                    for (phase, namespace, span) in edges.by_ref() {
                        charge_discovery(
                            self.meter,
                            1 + namespace.len() as u64,
                            &unit.identity,
                            Some(&unit.path),
                            span.clone(),
                        )?;
                        let dependency =
                            ModuleIdentity::new(phase, &namespace).map_err(|mut error| {
                                error.span = span.clone();
                                located(&unit.identity, Some(&unit.path), error)
                            })?;
                        if seen.insert(dependency.clone()) {
                            charge_discovery(
                                self.meter,
                                unit.path.as_os_str().len() as u64,
                                &unit.identity,
                                Some(&unit.path),
                                span.clone(),
                            )?;
                            unit.dependencies.push(dependency.clone());
                            child = Some(DiscoveryTask::Enter {
                                identity: dependency,
                                origin: Some(unit.path.clone()),
                                span,
                            });
                            break;
                        }
                    }
                    if let Some(child) = child {
                        pending.push(DiscoveryTask::Resume { unit, edges, seen });
                        pending.push(child);
                    } else {
                        self.active.pop();
                        self.done.insert(unit.identity.clone());
                        self.snapshots.push(unit);
                    }
                }
            }
        }
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
    let mut request =
        super::ExpansionRequest::new(expander, 0..0).map_err(|error| ModuleDiagnostic {
            namespace: namespace.into(),
            source_path: None,
            span: error.span,
            message: error.message,
        })?;
    request
        .charge_expansion_work(namespace.len() as u64, 0..0)
        .map_err(|error| ModuleDiagnostic {
            namespace: namespace.into(),
            source_path: None,
            span: error.span,
            message: error.message,
        })?;
    let identity = ModuleIdentity::new(phase, namespace).map_err(|error| ModuleDiagnostic {
        namespace: namespace.into(),
        source_path: None,
        span: error.span,
        message: error.message,
    })?;
    let expander: &mut dyn super::ExpansionHost = &mut request;
    let paths =
        source_paths_metered(roots, expander).map_err(|error| located(&identity, None, error))?;
    expander.source_paths(&paths);
    let expander_supports_macros = expander.supports_macro_imports();
    let snapshots = {
        let mut meter = |work| expander.charge_expansion_work(work, 0..0);
        let mut discovery = Discovery {
            roots,
            environment,
            provided,
            active: Vec::new(),
            done: BTreeSet::new(),
            snapshots: Vec::new(),
            phase_graph: false,
            macro_imports: expander_supports_macros,
            meter: &mut meter,
        };
        discovery.visit(identity.clone(), None, 0..0)?;
        discovery.snapshots
    };
    let (modules, mut snapshot) =
        compile_snapshots_with_expander(snapshots, environment, phase, expander)?;
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
        expander
            .charge_expansion_work(unit.source.len() as u64, 0..unit.source.len())
            .map_err(|error| located(&unit.identity, Some(&unit.path), error))?;
        let PreparedFragment {
            wasm, environment, ..
        } = super::prepare_fragment_forms_with_origin(
            unit.forms,
            0..unit.source.len(),
            &snapshot,
            phase,
            expander,
            Some(&super::SourceOrigin::new(
                unit.source.as_str(),
                Some(unit.path.clone()),
            )),
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
    let mut host = super::NoExpansion;
    let mut request = super::ExpansionRequest::new(&mut host, 0..source_text.len())
        .map_err(InputDiagnostic::Compile)?;
    let forms = {
        let mut meter = |work| request.charge_expansion_work(work, 0..0);
        parse_source(source_text, &mut meter).map_err(InputDiagnostic::Compile)?
    };
    request
        .charge_expansion_work(source_text.len() as u64, 0..source_text.len())
        .map_err(InputDiagnostic::Compile)?;
    prepare_input_forms_with_origin(
        forms,
        0..source_text.len(),
        roots,
        environment,
        phase,
        provided,
        &mut request,
        Some(&super::SourceOrigin::new(source_text, None)),
    )
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
    prepare_input_forms_with_origin(
        forms,
        span,
        roots,
        environment,
        phase,
        provided,
        expander,
        None,
    )
}
/// Supply the immutable root input origin separately from its dependency files.
pub fn prepare_input_forms_with_origin<P: AsRef<Path>>(
    forms: Vec<suss_reader::forms::Form>,
    span: Range<usize>,
    roots: &[P],
    environment: &Environment,
    phase: Phase,
    provided: &BTreeSet<ModuleIdentity>,
    expander: &mut dyn super::ExpansionHost,
    origin: Option<&super::SourceOrigin>,
) -> Result<PreparedInput, InputDiagnostic> {
    let mut request =
        super::ExpansionRequest::new(expander, span.clone()).map_err(InputDiagnostic::Compile)?;
    let expander: &mut dyn super::ExpansionHost = &mut request;
    let forms = {
        let mut meter = |work| expander.charge_expansion_work(work, 0..0);
        let selected =
            resolve_conditionals_metered(forms, &mut |work| charge_reader(&mut meter, work))
                .map_err(|error| {
                    InputDiagnostic::Compile(Diagnostic {
                        span: error.span,
                        message: error.message,
                    })
                })?;
        charge_header(&selected, &mut meter).map_err(InputDiagnostic::Compile)?;
        selected
    };
    let header = source::input_header_with_macros(&forms, expander.supports_macro_imports())
        .map_err(InputDiagnostic::Compile)?;
    let paths = source_paths_metered(roots, expander).map_err(InputDiagnostic::Compile)?;
    expander.source_paths(&paths);
    let supports_macros = expander.supports_macro_imports();
    let snapshots = {
        let mut meter = |work| expander.charge_expansion_work(work, 0..0);
        let mut discovery = Discovery {
            roots,
            environment,
            provided,
            active: Vec::new(),
            done: BTreeSet::new(),
            snapshots: Vec::new(),
            phase_graph: false,
            macro_imports: supports_macros,
            meter: &mut meter,
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
        discovery.snapshots
    };
    let (modules, mut snapshot) =
        compile_snapshots_with_expander(snapshots, environment, phase, expander)
            .map_err(InputDiagnostic::Dependency)?;
    snapshot
        .enter_namespace(phase, environment.current_namespace(phase))
        .map_err(InputDiagnostic::Compile)?;
    let fragment = super::prepare_selected_fragment_with_origin(
        forms, span, &snapshot, phase, expander, origin,
    )
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
    let mut remaining = super::DEFAULT_OPERATION_FUEL;
    discover_phase_modules_metered(
        namespace,
        roots,
        environment,
        phase,
        provided,
        &mut |work| {
            remaining = remaining.checked_sub(work).ok_or_else(|| Diagnostic {
                span: 0..0,
                message: "Namespace discovery work fuel exhausted".into(),
            })?;
            Ok(())
        },
    )
}
/// Same discovery with caller-owned cumulative accounting and cancellation.
/// Passing a Session charge callback is the host's responsibility; this function
/// neither initializes a Store budget nor resets it between dependency files.
pub fn discover_phase_modules_metered<P: AsRef<Path>>(
    namespace: &str,
    roots: &[P],
    environment: &Environment,
    phase: Phase,
    provided: &BTreeSet<ModuleIdentity>,
    meter: &mut DiscoveryWorkMeter<'_>,
) -> Result<Vec<SourceModule>, ModuleDiagnostic> {
    meter(namespace.len() as u64).map_err(|error| ModuleDiagnostic {
        namespace: namespace.into(),
        source_path: None,
        span: error.span,
        message: error.message,
    })?;
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
        meter,
    };
    discovery.visit(identity, None, 0..0)?;
    Ok(discovery.snapshots)
}
