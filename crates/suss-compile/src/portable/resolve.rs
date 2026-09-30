//! Phase-specific namespace identities. No runtime values or source replay live here.
use super::{Diagnostic, hir::Arithmetic};
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Binding {
    Cell(Global),
    Arithmetic {
        global: Global,
        operator: Arithmetic,
    },
    BootstrapLet(Global),
    BootstrapFn(Global),
}
impl Binding {
    pub fn global(&self) -> &Global {
        match self {
            Self::Cell(g)
            | Self::BootstrapLet(g)
            | Self::BootstrapFn(g)
            | Self::Arithmetic { global: g, .. } => g,
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
}
fn canonical(namespace: &str) -> &str {
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
fn valid_namespace(namespace: &str) -> Result<(), Diagnostic> {
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
            for name in ["let", "fn"] {
                let global = Global {
                    phase,
                    namespace: "suss.core".into(),
                    name: name.into(),
                };
                let binding = if name == "let" {
                    Binding::BootstrapLet(global.clone())
                } else {
                    Binding::BootstrapFn(global.clone())
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
        self.declare_namespace(phase, &global.namespace)?;
        self.bindings
            .insert(global.clone(), Binding::Cell(global.clone()));
        Ok(global)
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
    /// runtime vars do not hide macros, but lexical locals (handled by HIR) do.
    /// General compiled macro imports/expansion remain a later integration.
    pub fn resolve_bootstrap_macro(&self, phase: Phase, symbol: &Symbol) -> Option<Binding> {
        let scope = self.scope(phase);
        let name = if let Some(namespace) = &symbol.namespace {
            let namespace = scope
                .aliases
                .get(namespace)
                .map_or(namespace.as_str(), String::as_str);
            (canonical(namespace) == "suss.core" && matches!(symbol.name.as_str(), "let" | "fn"))
                .then_some(symbol.name.as_str())
        } else if let Some(global) = scope.refers.get(&symbol.name) {
            if global.namespace == "suss.core" && matches!(global.name.as_str(), "let" | "fn") {
                Some(global.name.as_str())
            } else {
                (matches!(symbol.name.as_str(), "let" | "fn")
                    && !scope.excluded_core.contains(&symbol.name))
                .then_some(symbol.name.as_str())
            }
        } else {
            (matches!(symbol.name.as_str(), "let" | "fn")
                && !scope.excluded_core.contains(&symbol.name))
            .then_some(symbol.name.as_str())
        }?;
        let global = Global {
            phase,
            namespace: "suss.core".into(),
            name: name.into(),
        };
        Some(if name == "let" {
            Binding::BootstrapLet(global)
        } else {
            Binding::BootstrapFn(global)
        })
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
