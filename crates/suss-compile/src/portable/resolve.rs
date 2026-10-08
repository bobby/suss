//! Phase-specific namespace identities. No runtime values or source replay live here.
use super::{
    Diagnostic,
    hir::{Arithmetic, ArrayOperation, Bitwise, Comparison},
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlForm {
    When,
    WhenNot,
    IfNot,
    IfLet,
    And,
    Or,
    Cond,
    Case,
    Declare,
    CachingHash,
    ThreadFirst,
    AsThread,
    Zero,
    Positive,
    Negative,
    Increment,
    Decrement,
    UncheckedGet,
    UncheckedSet,
}
impl ControlForm {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "when" => Self::When,
            "when-not" => Self::WhenNot,
            "if-not" => Self::IfNot,
            "if-let" => Self::IfLet,
            "and" => Self::And,
            "or" => Self::Or,
            "cond" => Self::Cond,
            "case" => Self::Case,
            "declare" => Self::Declare,
            "caching-hash" => Self::CachingHash,
            "->" => Self::ThreadFirst,
            "as->" => Self::AsThread,
            "zero?" => Self::Zero,
            "pos?" => Self::Positive,
            "neg?" => Self::Negative,
            "inc" => Self::Increment,
            "dec" => Self::Decrement,
            "unchecked-get" => Self::UncheckedGet,
            "unchecked-set" => Self::UncheckedSet,
            _ => return None,
        })
    }
}
#[derive(Debug, Clone)]
pub(crate) struct ProtocolMethod {
    pub name: String,
    pub signatures: Vec<(usize, usize)>, // argument arity, bundle key index
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Binding {
    BootstrapBitwise {
        global: Global,
        operation: Bitwise,
    },
    BootstrapControl {
        global: Global,
        operation: ControlForm,
    },
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
    /// Unbound reader reservation sharing a future public definition identity.
    ReaderCell(Global),
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
            Self::BootstrapBitwise { global: g, .. }
            | Self::BootstrapControl { global: g, .. }
            | Self::BootstrapComparison { global: g, .. }
            | Self::BootstrapArray { global: g, .. }
            | Self::Core { global: g, .. }
            | Self::Cell(g)
            | Self::InternalCell(g)
            | Self::ReaderCell(g)
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
    used_refers: BTreeSet<String>,
    renamed_refers: BTreeSet<String>,
    excluded_core: BTreeSet<String>,
    macro_aliases: BTreeMap<String, String>,
    macro_refers: BTreeMap<String, (String, String)>,
    used_macro_refers: BTreeSet<String>,
    renamed_macro_refers: BTreeSet<String>,
}
impl Scope {
    fn new(namespace: &str) -> Self {
        Self {
            namespace: namespace.into(),
            aliases: BTreeMap::new(),
            refers: BTreeMap::new(),
            used_refers: BTreeSet::new(),
            renamed_refers: BTreeSet::new(),
            excluded_core: BTreeSet::new(),
            macro_aliases: BTreeMap::new(),
            macro_refers: BTreeMap::new(),
            used_macro_refers: BTreeSet::new(),
            renamed_macro_refers: BTreeSet::new(),
        }
    }
}
/// Compiler declaration facts, distinct from initialized runtime cell metadata.
#[derive(Debug, Clone)]
pub struct DefinitionInfo {
    /// Actual definition syntax, retained before initializer analysis.
    pub definition_form: suss_reader::forms::Form,
    /// Analysis completion is distinct from presence or runtime evaluation of an initializer.
    pub analysis_completed: bool,
    pub declaration: suss_reader::forms::Form,
    pub docstring: Option<Vec<u16>>,
    pub origin: Option<super::SourceOrigin>,
    /// Actual initializer syntax, available before its body is analyzed.
    pub initializer_form: Option<suss_reader::forms::Form>,
    pub initializer: Option<std::sync::Arc<super::hir::Hir>>,
    /// Genuine deftype declaration facts, captured before method analysis.
    /// None is an ordinary def, not a zero-field source type.
    pub type_fields: Option<usize>,
    pub once: bool,
}
impl DefinitionInfo {
    /// Normalized def-AST export name for target selection, distinct from the raw
    /// :export metadata in namespace declaration records. The pinned def rule keeps
    /// false/nil absent, maps true to the qualified var, and retains other values.
    /// Reference: cljs/analyzer.cljc def export-as at c4295f303100bbf5afac449242d30bca1126f1a1.
    pub fn export_as(
        &self,
        global: &Global,
    ) -> Result<Option<suss_reader::forms::Form>, Diagnostic> {
        use suss_reader::forms::{Form, Kind};
        let pairs = super::hir::reader_metadata_pairs(&self.declaration)?;
        let Some(value) = pairs.chunks_exact(2).find_map(|pair| {
            matches!(&pair[0].kind, Kind::Keyword(key)
                if key.namespace.is_none() && key.name == "export")
            .then(|| pair[1].clone())
        }) else {
            return Ok(None);
        };
        Ok(match value.kind {
            Kind::Nil | Kind::Bool(false) => None,
            Kind::Bool(true) => Some(Form {
                span: value.span,
                metadata: vec![],
                kind: Kind::Symbol(Symbol {
                    namespace: Some(if global.namespace() == "suss.core" {
                        "cljs.core".to_owned()
                    } else {
                        global.namespace().to_owned()
                    }),
                    name: global.name().to_owned(),
                }),
            }),
            _ => Some(value),
        })
    }
}

/// Borrowed phase-specific source scope. Catalog entries do not certify loading.
pub struct NamespaceScope<'a> {
    pub namespace: &'a str,
    pub aliases: &'a BTreeMap<String, String>,
    pub refers: &'a BTreeMap<String, Global>,
    pub used_refers: &'a BTreeSet<String>,
    pub renamed_refers: &'a BTreeSet<String>,
    pub excluded_core: &'a BTreeSet<String>,
    pub macro_aliases: &'a BTreeMap<String, String>,
    pub macro_refers: &'a BTreeMap<String, (String, String)>,
    pub used_macro_refers: &'a BTreeSet<String>,
    pub renamed_macro_refers: &'a BTreeSet<String>,
    pub declarations: Vec<(&'a Global, &'a DefinitionInfo)>,
}
/// Explicit input to analysis, shared by future AOT, REPL and macro-session callers.
/// Configuration is checked before mutation. Source analysis borrows it immutably.
#[derive(Debug, Clone)]
pub struct Environment {
    pub(crate) reader_state: super::syntax_quote::ReaderState,
    namespaces: BTreeSet<(Phase, String)>,
    bindings: BTreeMap<Global, Binding>,
    definitions: BTreeMap<Global, std::sync::Arc<DefinitionInfo>>,
    source_generation: u64,
    scopes: BTreeMap<(Phase, String), Scope>,
    current: BTreeMap<Phase, String>,
    materialized_bootstrap: BTreeSet<Global>,
    macro_exports: BTreeSet<(Phase, String, String)>,
    macro_namespaces: BTreeSet<(Phase, String)>,
    pub(crate) protocols: BTreeMap<Global, Vec<ProtocolMethod>>,
}
/// Namespace of a qualified symbol, as the pinned analyzer's resolve-var and
/// resolve-macro-var resolve it: `clojure.core` names `cljs.core` (`suss.core`)
/// before any alias is consulted; otherwise an alias wins. Namespace
/// declarations themselves keep their own names.
fn symbol_namespace<'a>(alias: Option<&'a String>, namespace: &'a str) -> &'a str {
    if namespace == "clojure.core" {
        return "suss.core";
    }
    alias.map_or_else(|| canonical(namespace), |alias| canonical(alias))
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
            reader_state: super::syntax_quote::ReaderState::default(),
            namespaces: BTreeSet::new(),
            bindings: BTreeMap::new(),
            definitions: BTreeMap::new(),
            source_generation: 0,
            scopes: BTreeMap::new(),
            current: BTreeMap::new(),
            materialized_bootstrap: BTreeSet::new(),
            macro_exports: BTreeSet::new(),
            macro_namespaces: BTreeSet::new(),
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
                ("int", "primitive-int-function"),
                ("bit-and", "primitive-bit-and-function"),
                ("bit-or", "primitive-bit-or-function"),
                ("bit-xor", "primitive-bit-xor-function"),
                ("bit-and-not", "primitive-bit-and-not-function"),
                ("bit-not", "primitive-bit-not-function"),
                ("bit-clear", "primitive-bit-clear-function"),
                ("bit-flip", "primitive-bit-flip-function"),
                ("bit-set", "primitive-bit-set-function"),
                ("bit-test", "primitive-bit-test-function"),
                ("bit-shift-left", "primitive-bit-shift-left-function"),
                ("bit-shift-right", "primitive-bit-shift-right-function"),
                (
                    "unsigned-bit-shift-right",
                    "primitive-unsigned-bit-shift-right-function",
                ),
                ("imul", "primitive-imul-function"),
                (
                    "bit-shift-right-zero-fill",
                    "primitive-unsigned-bit-shift-right-function",
                ),
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
                ("js-fn?", "predicate-function"),
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
            for name in [
                "when",
                "when-not",
                "if-not",
                "if-let",
                "and",
                "or",
                "cond",
                "case",
                "declare",
                "caching-hash",
                "unchecked-get",
                "unchecked-set",
            ] {
                let global = Global {
                    phase,
                    namespace: "suss.core".into(),
                    name: name.into(),
                };
                env.bindings.insert(
                    global.clone(),
                    Binding::BootstrapControl {
                        global,
                        operation: ControlForm::from_name(name).unwrap(),
                    },
                );
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
    fn source_changed(&mut self) {
        self.source_generation = self.source_generation.checked_add(1).expect("compiler source generation exhausted");
    }
    pub(crate) fn source_generation(&self) -> u64 { self.source_generation }
    fn scope_mut(&mut self, phase: Phase) -> &mut Scope {
        self.source_changed();
        let namespace = self.current[&phase].clone();
        self.scopes
            .get_mut(&(phase, namespace))
            .expect("declared current scope")
    }
    /// Undo staged declaration publication without reverting unrelated macro effects/dependencies.
    pub fn restore_declarations(&mut self, before: &Self, globals: &[Global], phase: Phase) {
        for global in globals {
            match before.bindings.get(global) { Some(value) => { self.bindings.insert(global.clone(), value.clone()); }, None => { self.bindings.remove(global); } }
            match before.definitions.get(global) { Some(value) => { self.definitions.insert(global.clone(), value.clone()); }, None => { self.definitions.remove(global); } }
            let export = (phase, global.namespace.clone(), global.name.clone());
            if before.macro_exports.contains(&export) { self.macro_exports.insert(export); }
            else { self.macro_exports.remove(&export); }
        }
        if let Some(namespace) = before.current.get(&phase) { self.current.insert(phase, namespace.clone()); }
        self.source_changed();
    }
    pub fn current_namespace(&self, phase: Phase) -> &str {
        &self.current[&phase]
    }
    pub fn namespace_scope(&self, phase: Phase) -> NamespaceScope<'_> {
        let scope = self.scope(phase);
        NamespaceScope {
            namespace: &scope.namespace, aliases: &scope.aliases,
            refers: &scope.refers, excluded_core: &scope.excluded_core,
            used_refers: &scope.used_refers, renamed_refers: &scope.renamed_refers,
            macro_aliases: &scope.macro_aliases, macro_refers: &scope.macro_refers,
            used_macro_refers: &scope.used_macro_refers, renamed_macro_refers: &scope.renamed_macro_refers,
            declarations: self.definitions.iter().filter(|(global, _)|
                global.phase() == phase && global.namespace() == scope.namespace)
                .map(|(global, info)| (global, info.as_ref())).collect(),
        }
    }
    pub fn definition_info(&self, global: &Global) -> Option<&DefinitionInfo> {
        self.definitions.get(global).map(std::sync::Arc::as_ref)
    }
    /// Immutable declaration revisions retain identity through Environment clones
    /// and source snapshots. Reanalysis installs a fresh revision instead.
    pub(crate) fn shared_definition_info(&self, global: &Global) -> Option<&std::sync::Arc<DefinitionInfo>> {
        self.definitions.get(global)
    }
    pub(crate) fn record_definition(&mut self, global: Global, info: DefinitionInfo) {
        self.source_changed();
        self.definitions.insert(global, std::sync::Arc::new(info));
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
                Binding::Cell(global) | Binding::InternalCell(global) | Binding::ReaderCell(global) => Some(global.clone()),
                _ => None,
            })
            .chain(self.materialized_bootstrap.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub(crate) fn materialize_bootstrap(&mut self, global: Global) {
        if self.materialized_bootstrap.insert(global) { self.source_changed(); }
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
        self.source_changed();
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
        self.source_changed();
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
        self.source_changed();
        Ok(global)
    }
    pub(crate) fn is_hidden_cell(&self, phase: Phase, namespace: &str, name: &str) -> bool {
        let global = Global { phase, namespace: canonical(namespace).into(), name: name.into() };
        matches!(self.bindings.get(&global), Some(Binding::ReaderCell(_) | Binding::InternalCell(_)))
    }
    pub(crate) fn reader_sequence_cells(&mut self, phase: Phase) -> (Global, Global, bool) {
        let global = Global { phase, namespace: "suss.core".into(), name: "sequence".into() };
        let fallback = Global { phase, namespace: "suss.internal.reader".into(), name: "sequence".into() };
        self.bindings.entry(global.clone()).or_insert_with(|| Binding::ReaderCell(global.clone()));
        let fresh = !self.bindings.contains_key(&fallback);
        if fresh {
            self.bindings.insert(fallback.clone(), Binding::InternalCell(fallback.clone()));
            self.source_changed();
        }
        (global, fallback, fresh)
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
    /// Retain the explicit source require edge, including imports without aliases.
    /// This is a compiler catalog fact; it does not certify initialized code.
    pub(crate) fn record_requirement(
        &mut self,
        phase: Phase,
        namespace: &str,
        macros: bool,
    ) -> Result<(), Diagnostic> {
        valid_namespace(namespace)?;
        let target = canonical(namespace);
        let catalog = if macros { &self.macro_namespaces } else { &self.namespaces };
        if !catalog.contains(&(phase, target.into())) {
            return Err(error(format!("Unknown {phase:?} required namespace {namespace}")));
        }
        let scope = self.scope_mut(phase);
        let aliases = if macros { &mut scope.macro_aliases } else { &mut scope.aliases };
        if aliases.get(namespace).is_some_and(|old| old != target) {
            return Err(error(format!("Ambiguous required namespace alias {namespace}")));
        }
        aliases.insert(namespace.into(), target.into());
        Ok(())
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
        self.refer_with_role(phase, local, namespace, name, local != name)
    }
    pub(crate) fn refer_with_role(
        &mut self,
        phase: Phase,
        local: &str,
        namespace: &str,
        name: &str,
        renamed: bool,
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
        let scope = self.scope_mut(phase);
        scope.refers.insert(local.into(), global);
        if renamed { &mut scope.renamed_refers } else { &mut scope.used_refers }.insert(local.into());
        Ok(())
    }
    pub fn exclude_core(&mut self, phase: Phase, name: &str) -> Result<(), Diagnostic> {
        valid_name(name)?;
        self.scope_mut(phase).excluded_core.insert(name.into());
        Ok(())
    }
    /// Macro catalogs describe functions in the isolated host, never Runtime cells.
    pub fn declare_macro_exports(
        &mut self,
        phase: Phase,
        namespace: &str,
        names: &[String],
    ) -> Result<(), Diagnostic> {
        valid_namespace(namespace)?;
        for name in names {
            valid_name(name)?;
        }
        let namespace = canonical(namespace);
        self.macro_namespaces.insert((phase, namespace.into()));
        self.macro_exports
            .retain(|(p, ns, _)| *p != phase || ns != namespace);
        self.macro_exports.extend(
            names
                .iter()
                .map(|name| (phase, namespace.into(), name.clone())),
        );
        Ok(())
    }
    pub fn macro_alias(
        &mut self,
        phase: Phase,
        alias: &str,
        namespace: &str,
    ) -> Result<(), Diagnostic> {
        valid_namespace(alias)?;
        if matches!(alias, "suss.core" | "cljs.core") {
            return Err(error("Canonical core namespaces cannot be aliased away"));
        }
        valid_namespace(namespace)?;
        let namespace = canonical(namespace);
        if !self.macro_namespaces.contains(&(phase, namespace.into())) {
            return Err(error(format!("Unknown macro namespace {namespace}")));
        }
        if self.macro_namespaces.contains(&(phase, alias.into())) && alias != namespace {
            return Err(error(format!("Ambiguous macro namespace alias {alias}")));
        }
        let scope = self.scope_mut(phase);
        if scope
            .macro_aliases
            .get(alias)
            .is_some_and(|old| old != namespace)
        {
            return Err(error(format!("Ambiguous macro namespace alias {alias}")));
        }
        scope.macro_aliases.insert(alias.into(), namespace.into());
        Ok(())
    }
    pub fn macro_refer(
        &mut self,
        phase: Phase,
        local: &str,
        namespace: &str,
        name: &str,
    ) -> Result<(), Diagnostic> {
        self.macro_refer_with_role(phase, local, namespace, name, local != name)
    }
    pub(crate) fn macro_refer_with_role(
        &mut self,
        phase: Phase,
        local: &str,
        namespace: &str,
        name: &str,
        renamed: bool,
    ) -> Result<(), Diagnostic> {
        valid_name(local)?;
        let namespace = canonical(namespace);
        if !self
            .macro_exports
            .contains(&(phase, namespace.into(), name.into()))
        {
            return Err(error(format!("Unknown macro binding {namespace}/{name}")));
        }
        let target = (namespace.into(), name.into());
        let scope = self.scope_mut(phase);
        if scope
            .macro_refers
            .get(local)
            .is_some_and(|old| *old != target)
        {
            return Err(error(format!("Ambiguous macro binding {local}")));
        }
        scope.macro_refers.insert(local.into(), target);
        if renamed { &mut scope.renamed_macro_refers } else { &mut scope.used_macro_refers }.insert(local.into());
        Ok(())
    }
    pub fn resolve_source_macro(&self, phase: Phase, symbol: &Symbol) -> Option<(String, String)> {
        let scope = self.scope(phase);
        let target = if let Some(namespace) = &symbol.namespace {
            (
                symbol_namespace(
                    scope
                        .macro_aliases
                        .get(namespace)
                        .or_else(|| scope.aliases.get(namespace)),
                    namespace,
                )
                .into(),
                symbol.name.clone(),
            )
        } else if let Some(target) = scope.macro_refers.get(&symbol.name) {
            target.clone()
        } else if phase == Phase::Macro
            && let Some(target) = scope.refers.get(&symbol.name)
        {
            // Ordinary dependencies of compiled macro source live in this same
            // phase. Only a real registered macro export can pass the check below.
            (target.namespace.clone(), target.name.clone())
        } else {
            (scope.namespace.clone(), symbol.name.clone())
        };
        self.macro_exports
            .contains(&(phase, target.0.clone(), target.1.clone()))
            .then_some(target)
    }
    /// Identify a bounded core macro before its complete source bootstrap exists.
    /// This supplies no runtime binding/value: excluded, shadowed or non-core
    /// names continue through ordinary lookup and its diagnostics.
    pub fn resolves_bootstrap_name(&self, phase: Phase, symbol: &Symbol, name: &str) -> bool {
        let scope = self.scope(phase);
        if let Some(namespace) = &symbol.namespace {
            return symbol_namespace(scope.aliases.get(namespace), namespace) == "suss.core"
                && symbol.name == name;
        }
        if let Some(target) = scope.refers.get(&symbol.name) {
            return target.namespace == "suss.core" && target.name == name;
        }
        let own = Global {
            phase,
            namespace: scope.namespace.clone(),
            name: symbol.name.clone(),
        };
        symbol.name == name
            && !scope.excluded_core.contains(name)
            && !self.bindings.contains_key(&own)
    }
    /// The bounded bootstrap macro lookup is separate from ordinary var lookup:
    /// lexical locals (handled by HIR) hide macros. User runtime definitions also
    /// hide automatic core bootstrap macros, as observed in the pinned compiler.
    /// General compiled macro imports/expansion remain a later integration.
    pub fn resolve_bootstrap_macro(&self, phase: Phase, symbol: &Symbol) -> Option<Binding> {
        let scope = self.scope(phase);
        let name = if let Some(namespace) = &symbol.namespace {
            (symbol_namespace(scope.aliases.get(namespace), namespace) == "suss.core"
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
                        | "when"
                        | "when-not"
                        | "if-not"
                        | "if-let"
                        | "and"
                        | "or"
                        | "cond"
                        | "case"
                        | "declare"
                        | "caching-hash"
                        | "->"
                        | "as->"
                        | "zero?"
                        | "pos?"
                        | "neg?"
                        | "inc"
                        | "dec"
                        | "int"
                        | "bit-and"
                        | "bit-or"
                        | "bit-xor"
                        | "bit-and-not"
                        | "bit-not"
                        | "bit-clear"
                        | "bit-flip"
                        | "bit-set"
                        | "bit-test"
                        | "bit-shift-left"
                        | "bit-shift-right"
                        | "unsigned-bit-shift-right"
                        | "bit-shift-right-zero-fill"
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
                        | "when"
                        | "when-not"
                        | "if-not"
                        | "if-let"
                        | "and"
                        | "or"
                        | "cond"
                        | "case"
                        | "declare"
                        | "caching-hash"
                        | "->"
                        | "as->"
                        | "zero?"
                        | "pos?"
                        | "neg?"
                        | "inc"
                        | "dec"
                        | "int"
                        | "bit-and"
                        | "bit-or"
                        | "bit-xor"
                        | "bit-and-not"
                        | "bit-not"
                        | "bit-clear"
                        | "bit-flip"
                        | "bit-set"
                        | "bit-test"
                        | "bit-shift-left"
                        | "bit-shift-right"
                        | "unsigned-bit-shift-right"
                        | "bit-shift-right-zero-fill"
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
                        | "when"
                        | "when-not"
                        | "if-not"
                        | "if-let"
                        | "and"
                        | "or"
                        | "cond"
                        | "case"
                        | "declare"
                        | "caching-hash"
                        | "->"
                        | "as->"
                        | "zero?"
                        | "pos?"
                        | "neg?"
                        | "inc"
                        | "dec"
                        | "int"
                        | "bit-and"
                        | "bit-or"
                        | "bit-xor"
                        | "bit-and-not"
                        | "bit-not"
                        | "bit-clear"
                        | "bit-flip"
                        | "bit-set"
                        | "bit-test"
                        | "bit-shift-left"
                        | "bit-shift-right"
                        | "unsigned-bit-shift-right"
                        | "bit-shift-right-zero-fill"
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
                    | "when"
                    | "when-not"
                    | "if-not"
                    | "if-let"
                    | "and"
                    | "or"
                    | "cond"
                    | "case"
                    | "declare"
                    | "caching-hash"
                    | "->"
                    | "as->"
                    | "zero?"
                    | "pos?"
                    | "neg?"
                    | "inc"
                    | "dec"
                    | "int"
                    | "bit-and"
                    | "bit-or"
                    | "bit-xor"
                    | "bit-and-not"
                    | "bit-not"
                    | "bit-clear"
                    | "bit-flip"
                    | "bit-set"
                    | "bit-test"
                    | "bit-shift-left"
                    | "bit-shift-right"
                    | "unsigned-bit-shift-right"
                    | "bit-shift-right-zero-fill"
            ) && !scope.excluded_core.contains(&symbol.name))
            .then_some(symbol.name.as_str())
        }?;
        // A new runtime definition hides these automatic core bootstrap macros;
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
                    | "when"
                    | "when-not"
                    | "if-not"
                    | "if-let"
                    | "and"
                    | "or"
                    | "cond"
                    | "case"
                    | "declare"
                    | "caching-hash"
                    | "->"
                    | "as->"
                    | "zero?"
                    | "pos?"
                    | "neg?"
                    | "inc"
                    | "dec"
                    | "int"
                    | "bit-and"
                    | "bit-or"
                    | "bit-xor"
                    | "bit-and-not"
                    | "bit-not"
                    | "bit-clear"
                    | "bit-flip"
                    | "bit-set"
                    | "bit-test"
                    | "bit-shift-left"
                    | "bit-shift-right"
                    | "unsigned-bit-shift-right"
                    | "bit-shift-right-zero-fill"
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
        Some(if let Some(operation) = ControlForm::from_name(name) {
            Binding::BootstrapControl { global, operation }
        } else if let Some(operation) = Bitwise::from_name(name) {
            Binding::BootstrapBitwise { global, operation }
        } else if let Some(operation) = match name {
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
                symbol_namespace(scope.aliases.get(namespace), namespace)
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
            .filter(|binding| !matches!(binding, Binding::InternalCell(_) | Binding::ReaderCell(_)))
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
