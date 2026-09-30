//! Phase-specific namespace identities. No runtime values or source replay live here.
use super::{
    Diagnostic,
    hir::{Arithmetic, ArrayOperation, Comparison},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    path::{Path, PathBuf},
};
use suss_reader::Symbol;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    Runtime,
    Macro,
}

/// A stable language identity; Wasm import indices are deliberately absent.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Global {
    phase: Phase,
    namespace: String,
    name: String,
}
impl Global {
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn namespace(&self) -> &str {
        &self.namespace
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn import_module(&self) -> &'static str {
        match self.phase {
            Phase::Runtime => "suss.bindings.runtime",
            Phase::Macro => "suss.bindings.macro",
        }
    }
    pub fn import_name(&self) -> String {
        format!("{}/{}", self.namespace, self.name)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NominalForm {
    Deftype,
    Defprotocol,
    ExtendType,
    Instance,
    Satisfies,
    Implements,
}
#[derive(Debug, Clone)]
pub(crate) struct ProtocolMethod {
    pub name: String,
    pub signatures: Vec<(usize, usize)>, // argument arity, bundle key index
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Binding {
    BootstrapComparison {
        global: Global,
        operation: Comparison,
    },
    BootstrapArray {
        global: Global,
        operation: ArrayOperation,
    },
    Core {
        global: Global,
        export: &'static str,
    },
    Cell(Global),
    InternalCell(Global),
    Arithmetic {
        global: Global,
        operator: Arithmetic,
    },
    BootstrapLet(Global),
    BootstrapLoop(Global),
    BootstrapFn(Global),
    BootstrapDefonce(Global),
    BootstrapBinding(Global),
    Nominal {
        global: Global,
        form: NominalForm,
    },
}
impl Binding {
    pub fn global(&self) -> &Global {
        match self {
            Self::BootstrapComparison { global: g, .. }
            | Self::BootstrapArray { global: g, .. }
            | Self::Core { global: g, .. }
            | Self::Cell(g)
            | Self::InternalCell(g)
            | Self::BootstrapLoop(g)
            | Self::BootstrapLet(g)
            | Self::BootstrapFn(g)
            | Self::BootstrapDefonce(g)
            | Self::BootstrapBinding(g)
            | Self::Arithmetic { global: g, .. }
            | Self::Nominal { global: g, .. } => g,
        }
    }
}
#[derive(Debug, Clone)]
struct Scope {
    namespace: String,
    aliases: BTreeMap<String, String>,
    refers: BTreeMap<String, Global>,
    excluded_core: BTreeSet<String>,
}
impl Scope {
    fn new(namespace: &str) -> Self {
        Self {
            namespace: namespace.into(),
            aliases: BTreeMap::new(),
            refers: BTreeMap::new(),
            excluded_core: BTreeSet::new(),
        }
    }
}
/// Explicit input to analysis, shared by future AOT, REPL and macro-session callers.
/// Configuration is checked before mutation. Source analysis borrows it immutably.
#[derive(Debug, Clone)]
pub struct Environment {
    namespaces: BTreeSet<(Phase, String)>,
    bindings: BTreeMap<Global, Binding>,
    scopes: BTreeMap<(Phase, String), Scope>,
    current: BTreeMap<Phase, String>,
    materialized_bootstrap: BTreeSet<Global>,
    pub(crate) protocols: BTreeMap<Global, Vec<ProtocolMethod>>,
}
pub(crate) fn canonical(namespace: &str) -> &str {
    if namespace == "cljs.core" {
        "suss.core"
    } else {
        namespace
    }
}
fn error(message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        span: 0..0,
        message: message.into(),
    }
}
pub(crate) fn valid_namespace(namespace: &str) -> Result<(), Diagnostic> {
    if namespace == "suss.bootstrap" {
        return Err(error("Compiler bootstrap namespace is reserved"));
    }
    if namespace.is_empty()
        || namespace.split('.').any(|part| {
            part.is_empty()
                || part
                    .chars()
                    .any(|c| !c.is_alphanumeric() && c != '-' && c != '_')
        })
    {
        return Err(error("Invalid namespace name"));
    }
    Ok(())
}
fn valid_name(name: &str) -> Result<(), Diagnostic> {
    use suss_reader::forms::{Kind, read_forms};
    match read_forms(name).ok().as_deref() {
        Some([form])
            if form.metadata.is_empty()
                && matches!(&form.kind, Kind::Symbol(symbol) if symbol.namespace.is_none() && symbol.name == name) =>
        {
            Ok(())
        }
        _ => Err(error("Binding name must be an unqualified symbol")),
    }
}
impl Default for Environment {
    fn default() -> Self {
        Self::new("user").expect("valid bootstrap namespace")
    }
}
impl Environment {
    pub fn new(namespace: &str) -> Result<Self, Diagnostic> {
        valid_namespace(namespace)?;
        let mut env = Self {
            namespaces: BTreeSet::new(),
            bindings: BTreeMap::new(),
            scopes: BTreeMap::new(),
            current: BTreeMap::new(),
            materialized_bootstrap: BTreeSet::new(),
            protocols: BTreeMap::new(),
        };
        for phase in [Phase::Runtime, Phase::Macro] {
            env.scopes.insert(
                (phase, canonical(namespace).into()),
                Scope::new(canonical(namespace)),
            );
            env.current.insert(phase, canonical(namespace).into());
            env.namespaces.insert((phase, canonical(namespace).into()));
            env.namespaces.insert((phase, "suss.core".into()));
            for (name, operator) in [
                ("+", Arithmetic::Add),
                ("-", Arithmetic::Subtract),
                ("*", Arithmetic::Multiply),
                ("/", Arithmetic::Divide),
            ] {
                let global = Global {
                    phase,
                    namespace: "suss.core".into(),
                    name: name.into(),
                };
                env.bindings
                    .insert(global.clone(), Binding::Arithmetic { global, operator });
            }
            for (name, export) in [
                ("<", "comparison-less-function"),
                ("<=", "comparison-less-equal-function"),
                (">", "comparison-greater-function"),
                (">=", "comparison-greater-equal-function"),
                ("==", "comparison-strict-equal-function"),
                ("array", "array-function"),
                ("array?", "array-predicate-function"),
                ("make-array", "array-make-function"),
                ("aclone", "array-clone-function"),
                ("aget", "array-get-function"),
                ("aset", "array-set-function"),
                ("alength", "array-length-function"),
                ("native-satisfies?", "native-satisfies-function"),
                ("nil?", "predicate-nil"),
                ("false?", "predicate-false"),
                ("true?", "predicate-true"),
                ("undefined?", "predicate-undefined"),
                ("number?", "predicate-number"),
                ("string?", "predicate-string"),
                ("identical?", "predicate-identical"),
                ("ExceptionInfo", "core-exception-info-class"),
                ("ex-info", "core-ex-info"),
                ("ex-data", "core-ex-data"),
                ("ex-message", "core-ex-message"),
                ("ex-cause", "core-ex-cause"),
            ] {
                let global = Global {
                    phase,
                    namespace: "suss.core".into(),
                    name: name.into(),
                };
                env.bindings
                    .insert(global.clone(), Binding::Core { global, export });
            }
            for (name, form) in [
                ("deftype", NominalForm::Deftype),
                ("defprotocol", NominalForm::Defprotocol),
                ("extend-type", NominalForm::ExtendType),
                ("instance?", NominalForm::Instance),
                ("satisfies?", NominalForm::Satisfies),
                ("implements?", NominalForm::Implements),
            ] {
                let global = Global {
                    phase,
                    namespace: "suss.core".into(),
                    name: name.into(),
                };
                env.bindings
                    .insert(global.clone(), Binding::Nominal { global, form });
            }
            for name in ["let", "loop", "fn", "defonce", "binding", "with-redefs"] {
                let global = Global {
                    phase,
                    namespace: "suss.core".into(),
                    name: name.into(),
                };
                let binding = if name == "let" {
                    Binding::BootstrapLet(global.clone())
                } else if name == "loop" {
                    Binding::BootstrapLoop(global.clone())
                } else if name == "fn" {
                    Binding::BootstrapFn(global.clone())
                } else if name == "binding" || name == "with-redefs" {
                    Binding::BootstrapBinding(global.clone())
                } else {
                    Binding::BootstrapDefonce(global.clone())
                };
                env.bindings.insert(global, binding);
            }
        }
        Ok(env)
    }
    fn scope(&self, phase: Phase) -> &Scope {
        &self.scopes[&(phase, self.current[&phase].clone())]
    }
    fn scope_mut(&mut self, phase: Phase) -> &mut Scope {
        let namespace = self.current[&phase].clone();
        self.scopes
            .get_mut(&(phase, namespace))
            .expect("declared current scope")
    }
    pub fn current_namespace(&self, phase: Phase) -> &str {
        &self.current[&phase]
    }
    /// Explicit declarations, not evidence that any source file has loaded.
    pub fn has_namespace(&self, phase: Phase, namespace: &str) -> bool {
        self.namespaces
            .contains(&(phase, canonical(namespace).into()))
    }
    pub fn cells(&self) -> Vec<Global> {
        self.bindings
            .values()
            .filter_map(|binding| match binding {
                Binding::Cell(global) | Binding::InternalCell(global) => Some(global.clone()),
                _ => None,
            })
            .chain(self.materialized_bootstrap.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub(crate) fn materialize_bootstrap(&mut self, global: Global) {
        self.materialized_bootstrap.insert(global);
    }
    /// Bootstrap cell initializers for native hosts; source values share these
    /// canonical identities and later declarations may replace their contents.
    pub fn core_bindings(&self, phase: Phase) -> Vec<(Global, &'static str)> {
        self.bindings
            .values()
            .filter_map(|binding| match binding {
                Binding::Core { global, export } if global.phase == phase => {
                    Some((global.clone(), *export))
                }
                _ => None,
            })
            .collect()
    }
    pub fn arithmetic_bindings(&self, phase: Phase) -> Vec<(Global, Arithmetic)> {
        self.bindings
            .values()
            .filter_map(|binding| match binding {
                Binding::Arithmetic { global, operator } if global.phase == phase => {
                    Some((global.clone(), *operator))
                }
                _ => None,
            })
            .collect()
    }
    pub fn declare_namespace(&mut self, phase: Phase, namespace: &str) -> Result<(), Diagnostic> {
        valid_namespace(namespace)?;
        let namespace = canonical(namespace);
        if self.scopes.iter().any(|((scope_phase, _), scope)| {
            *scope_phase == phase
                && scope
                    .aliases
                    .get(namespace)
                    .is_some_and(|target| target != namespace)
        }) {
            return Err(error(format!("Ambiguous namespace alias {namespace}")));
        }
        self.namespaces.insert((phase, namespace.into()));
        Ok(())
    }
    pub fn enter_namespace(&mut self, phase: Phase, namespace: &str) -> Result<(), Diagnostic> {
        self.declare_namespace(phase, namespace)?;
        let namespace = canonical(namespace);
        self.scopes
            .entry((phase, namespace.into()))
            .or_insert_with(|| Scope::new(namespace));
        self.current.insert(phase, namespace.into());
        Ok(())
    }
    /// An ns declaration replaces imports; entering a REPL namespace preserves them.
    pub(crate) fn reset_namespace(
        &mut self,
        phase: Phase,
        namespace: &str,
    ) -> Result<(), Diagnostic> {
        self.enter_namespace(phase, namespace)?;
        let namespace = self.current[&phase].clone();
        self.scopes
            .insert((phase, namespace.clone()), Scope::new(&namespace));
        Ok(())
    }
    pub fn declare_cell(
        &mut self,
        phase: Phase,
        namespace: &str,
        name: &str,
    ) -> Result<Global, Diagnostic> {
        valid_namespace(namespace)?;
        valid_name(name)?;
        let global = Global {
            phase,
            namespace: canonical(namespace).into(),
            name: name.into(),
        };
        if self
            .scopes
            .get(&(phase, global.namespace.clone()))
            .is_some_and(|scope| {
                scope
                    .refers
                    .get(name)
                    .is_some_and(|target| *target != global)
            })
        {
            return Err(error(format!("Ambiguous binding {name}")));
        }
        if matches!(self.bindings.get(&global), Some(Binding::InternalCell(_))) {
            return Err(error("Cannot redefine a compiler-owned binding"));
        }
        self.declare_namespace(phase, &global.namespace)?;
        self.bindings
            .insert(global.clone(), Binding::Cell(global.clone()));
        Ok(global)
    }
    pub(crate) fn protocol_key(&mut self, protocol: &Global, method: &str, arity: usize) -> Global {
        let identity = format!(
            "{}/{}/{}/{}",
            protocol.namespace, protocol.name, method, arity
        );
        let name = identity
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let global = Global {
            phase: protocol.phase,
            namespace: "suss.internal.protocol-keys".into(),
            name: format!("key-{name}"),
        };
        self.bindings
            .insert(global.clone(), Binding::InternalCell(global.clone()));
        global
    }
    pub fn alias(&mut self, phase: Phase, alias: &str, namespace: &str) -> Result<(), Diagnostic> {
        valid_namespace(alias)?;
        valid_namespace(namespace)?;
        let namespace = canonical(namespace);
        if matches!(alias, "suss.core" | "cljs.core") {
            return Err(error("Canonical core namespaces cannot be aliased away"));
        }
        if !self.namespaces.contains(&(phase, namespace.into())) {
            return Err(error(format!("Unknown {phase:?} namespace {namespace}")));
        }
        let scope = self.scope(phase);
        if scope
            .aliases
            .get(alias)
            .is_some_and(|target| target != namespace)
            || (self.namespaces.contains(&(phase, alias.into())) && alias != namespace)
        {
            return Err(error(format!("Ambiguous namespace alias {alias}")));
        }
        self.scope_mut(phase)
            .aliases
            .insert(alias.into(), namespace.into());
        Ok(())
    }
    pub fn refer(
        &mut self,
        phase: Phase,
        local: &str,
        namespace: &str,
        name: &str,
    ) -> Result<(), Diagnostic> {
        valid_name(local)?;
        valid_namespace(namespace)?;
        valid_name(name)?;
        let global = Global {
            phase,
            namespace: canonical(namespace).into(),
            name: name.into(),
        };
        if !self.bindings.contains_key(&global) {
            return Err(error(format!(
                "Unknown {phase:?} binding {}",
                global.import_name()
            )));
        }
        let scope = self.scope(phase);
        let own = Global {
            phase,
            namespace: scope.namespace.clone(),
            name: local.into(),
        };
        if scope
            .refers
            .get(local)
            .is_some_and(|target| *target != global)
            || (self.bindings.contains_key(&own) && own != global)
        {
            return Err(error(format!("Ambiguous binding {local}")));
        }
        self.scope_mut(phase).refers.insert(local.into(), global);
        Ok(())
    }
    pub fn exclude_core(&mut self, phase: Phase, name: &str) -> Result<(), Diagnostic> {
        valid_name(name)?;
        self.scope_mut(phase).excluded_core.insert(name.into());
        Ok(())
    }
    /// The bounded bootstrap macro lookup is separate from ordinary var lookup:
    /// lexical locals (handled by HIR) hide macros. User runtime definitions also
    /// hide auto-referred array/comparison/implements? macros, as observed in the pinned compiler.
    /// General compiled macro imports/expansion remain a later integration.
    pub fn resolve_bootstrap_macro(&self, phase: Phase, symbol: &Symbol) -> Option<Binding> {
        let scope = self.scope(phase);
        let name = if let Some(namespace) = &symbol.namespace {
            let namespace = scope
                .aliases
                .get(namespace)
                .map_or(namespace.as_str(), String::as_str);
            (canonical(namespace) == "suss.core"
                && matches!(
                    symbol.name.as_str(),
                    "let"
                        | "loop"
                        | "fn"
                        | "defonce"
                        | "binding"
                        | "with-redefs"
                        | "deftype"
                        | "defprotocol"
                        | "extend-type"
                        | "instance?"
                        | "satisfies?"
                        | "implements?"
                        | "array"
                        | "make-array"
                        | "alength"
                        | "aget"
                        | "aset"
                        | "<"
                        | "<="
                        | ">"
                        | ">="
                        | "=="
                ))
            .then_some(symbol.name.as_str())
        } else if let Some(global) = scope.refers.get(&symbol.name) {
            if global.namespace == "suss.core"
                && matches!(
                    global.name.as_str(),
                    "let"
                        | "loop"
                        | "fn"
                        | "defonce"
                        | "binding"
                        | "with-redefs"
                        | "deftype"
                        | "defprotocol"
                        | "extend-type"
                        | "instance?"
                        | "satisfies?"
                        | "implements?"
                        | "array"
                        | "make-array"
                        | "alength"
                        | "aget"
                        | "aset"
                        | "<"
                        | "<="
                        | ">"
                        | ">="
                        | "=="
                )
            {
                Some(global.name.as_str())
            } else {
                (matches!(
                    symbol.name.as_str(),
                    "let"
                        | "loop"
                        | "fn"
                        | "defonce"
                        | "binding"
                        | "with-redefs"
                        | "deftype"
                        | "defprotocol"
                        | "extend-type"
                        | "instance?"
                        | "satisfies?"
                        | "implements?"
                        | "array"
                        | "make-array"
                        | "alength"
                        | "aget"
                        | "aset"
                        | "<"
                        | "<="
                        | ">"
                        | ">="
                        | "=="
                ) && !scope.excluded_core.contains(&symbol.name))
                .then_some(symbol.name.as_str())
            }
        } else {
            (matches!(
                symbol.name.as_str(),
                "let"
                    | "loop"
                    | "fn"
                    | "defonce"
                    | "binding"
                    | "with-redefs"
                    | "deftype"
                    | "defprotocol"
                    | "extend-type"
                    | "instance?"
                    | "satisfies?"
                    | "implements?"
                    | "array"
                    | "make-array"
                    | "alength"
                    | "aget"
                    | "aset"
                    | "<"
                    | "<="
                    | ">"
                    | ">="
                    | "=="
            ) && !scope.excluded_core.contains(&symbol.name))
            .then_some(symbol.name.as_str())
        }?;
        // A new runtime definition hides the auto-referred array/comparison macro;
        // explicit core qualification remains independent of that user var.
        if symbol.namespace.is_none()
            && scope.namespace != "suss.core"
            && matches!(
                name,
                "implements?"
                    | "array"
                    | "make-array"
                    | "alength"
                    | "aget"
                    | "aset"
                    | "<"
                    | "<="
                    | ">"
                    | ">="
                    | "=="
            )
            && self.bindings.contains_key(&Global {
                phase,
                namespace: scope.namespace.clone(),
                name: symbol.name.clone(),
            })
        {
            return None;
        }
        let global = Global {
            phase,
            namespace: "suss.core".into(),
            name: name.into(),
        };
        Some(
            if let Some(operation) = match name {
                "<" => Some(Comparison::Less),
                "<=" => Some(Comparison::LessEqual),
                ">" => Some(Comparison::Greater),
                ">=" => Some(Comparison::GreaterEqual),
                "==" => Some(Comparison::StrictEqual),
                _ => None,
            } {
                Binding::BootstrapComparison { global, operation }
            } else if let Some(operation) = match name {
                "array" => Some(ArrayOperation::Literal),
                "make-array" => Some(ArrayOperation::Make),
                "alength" => Some(ArrayOperation::Length),
                "aget" => Some(ArrayOperation::Get),
                "aset" => Some(ArrayOperation::Set),
                _ => None,
            } {
                Binding::BootstrapArray { global, operation }
            } else if name == "let" {
                Binding::BootstrapLet(global)
            } else if name == "loop" {
                Binding::BootstrapLoop(global.clone())
            } else if name == "fn" {
                Binding::BootstrapFn(global)
            } else if name == "defonce" {
                Binding::BootstrapDefonce(global)
            } else if name == "binding" || name == "with-redefs" {
                Binding::BootstrapBinding(global)
            } else {
                Binding::Nominal {
                    global,
                    form: match name {
                        "deftype" => NominalForm::Deftype,
                        "defprotocol" => NominalForm::Defprotocol,
                        "instance?" => NominalForm::Instance,
                        "satisfies?" => NominalForm::Satisfies,
                        "implements?" => NominalForm::Implements,
                        _ => NominalForm::ExtendType,
                    },
                }
            },
        )
    }
    pub fn resolve(
        &self,
        phase: Phase,
        symbol: &Symbol,
        span: Range<usize>,
    ) -> Result<Binding, Diagnostic> {
        let scope = self.scope(phase);
        let global = if let Some(namespace) = &symbol.namespace {
            let namespace = if matches!(namespace.as_str(), "suss.core" | "cljs.core") {
                canonical(namespace)
            } else {
                scope
                    .aliases
                    .get(namespace)
                    .map_or(namespace.as_str(), String::as_str)
            };
            Global {
                phase,
                namespace: namespace.into(),
                name: symbol.name.clone(),
            }
        } else if let Some(global) = scope.refers.get(&symbol.name) {
            global.clone()
        } else {
            let own = Global {
                phase,
                namespace: scope.namespace.clone(),
                name: symbol.name.clone(),
            };
            if self.bindings.contains_key(&own) {
                own
            } else if !scope.excluded_core.contains(&symbol.name) {
                Global {
                    phase,
                    namespace: "suss.core".into(),
                    name: symbol.name.clone(),
                }
            } else {
                own
            }
        };
        self.bindings
            .get(&global)
            .filter(|binding| !matches!(binding, Binding::InternalCell(_)))
            .cloned()
            .ok_or_else(|| Diagnostic {
                span,
                message: format!("Unresolved {phase:?} name {symbol}"),
            })
    }
}

/// Locate one source across all supported extensions/roots. Never pick by search order.
/// Canonical paths deduplicate the same file reached through multiple roots.
pub fn locate_source(
    namespace: &str,
    roots: &[impl AsRef<Path>],
    span: Range<usize>,
) -> Result<PathBuf, Diagnostic> {
    valid_namespace(namespace).map_err(|mut error| {
        error.span = span.clone();
        error
    })?;
    let namespace = canonical(namespace);
    let relative = namespace.replace('.', "/").replace('-', "_");
    let mut matches = BTreeSet::new();
    for root in roots {
        for extension in ["sus", "cljs", "cljc"] {
            let path = root.as_ref().join(format!("{relative}.{extension}"));
            match path.canonicalize() {
                Ok(path) if path.is_file() => {
                    matches.insert(path);
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(Diagnostic {
                        span,
                        message: format!("Cannot resolve namespace source: {error}"),
                    });
                }
            }
        }
    }
    match matches.len() {
        1 => Ok(matches.into_iter().next().unwrap()),
        0 => Err(Diagnostic {
            span,
            message: format!("No source for namespace {namespace}"),
        }),
        _ => Err(Diagnostic {
            span,
            message: format!(
                "Ambiguous source for namespace {namespace}: {}",
                matches
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }),
    }
}
