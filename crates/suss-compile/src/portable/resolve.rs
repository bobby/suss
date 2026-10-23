//! Phase-specific namespace identities. No runtime values or source replay live here.
#[cfg(test)]
const BOOTSTRAP_GLOBAL_NAMES: &[&str] = &["->", "<", "<=", "==", ">", ">=", "aget", "alength", "and", "array", "as->", "aset", "binding", "bit-and", "bit-and-not", "bit-clear", "bit-flip", "bit-not", "bit-or", "bit-set", "bit-shift-left", "bit-shift-right", "bit-shift-right-zero-fill", "bit-test", "bit-xor", "caching-hash", "case", "cond", "dec", "declare", "defonce", "defprotocol", "defrecord*", "deftype", "deftype*", "extend-type", "fn", "if-let", "if-not", "implements?", "inc", "instance?", "int", "let", "loop", "make-array", "neg?", "or", "pos?", "satisfies?", "unsigned-bit-shift-right", "when", "when-not", "with-redefs", "zero?"];

use super::{
    Diagnostic,
    hir::{Arithmetic, ArrayOperation, Bitwise, Comparison},
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    hash::{BuildHasherDefault, Hash, Hasher},
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};
use suss_reader::Symbol;

mod type_key_history;
pub use type_key_history::TypeKeyHistory;
mod completed_values;
mod namespace_snapshot;
mod namespace_index;
pub use completed_values::CompletedDeclarationSource;
pub(crate) use completed_values::{TypeValuePlan, TypeSourceValue, VALUE_FIELDS, own_definition_form};
pub(crate) use type_key_history::{TypeKey, KeyOrigin as TypeKeyOrigin, LiteralSite as TypeKeyLiteralSite};
pub(crate) use type_key_history::{RetainedKey, RetainedSite};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Phase {
    Runtime,
    Macro,
}

/// A stable language identity; Wasm import indices are deliberately absent.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Global(std::sync::Arc<GlobalIdentity>);
/// Immutable owned payload. Active key queries use borrowed GlobalKey views.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GlobalIdentity {
    phase: Phase,
    namespace: String,
    name: String,
}
impl std::borrow::Borrow<GlobalIdentity> for Global {
    fn borrow(&self) -> &GlobalIdentity { &self.0 }
}
impl std::ops::Deref for Global {
    type Target = GlobalIdentity;
    fn deref(&self) -> &GlobalIdentity { &self.0 }
}
impl PartialEq<GlobalIdentity> for Global {
    fn eq(&self, other: &GlobalIdentity) -> bool { self.0.as_ref() == other }
}
impl std::fmt::Debug for Global {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Global").field("phase", &self.phase)
            .field("namespace", &self.namespace).field("name", &self.name).finish()
    }
}
#[cfg(test)]
thread_local! { static GLOBAL_OWNER_ALLOCATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
type OwnerMeter<'a> = dyn FnMut(u64) -> Result<(), Diagnostic> + 'a;
fn owner_charge(meter: &mut OwnerMeter<'_>, units: usize) -> Result<(), Diagnostic> {
    meter(u64::try_from(units).map_err(|_| error("Global ownership work overflow"))?)
}
fn owner_text(value: &str, meter: &mut OwnerMeter<'_>) -> Result<String, Diagnostic> {
    owner_charge(meter, value.len())?;
    let mut result = String::new();
    result.try_reserve_exact(value.len()).map_err(|_| error("Unable to reserve Global identity text"))?;
    result.push_str(value);
    Ok(result)
}
// Convenience APIs create a separate, finite native setup operation. Compiler
// preparation calls the metered variants with its CURRENT ExpansionRequest;
// it must never replenish that request by entering a setup wrapper.
fn setup_meter() -> impl FnMut(u64) -> Result<(), Diagnostic> {
    let mut remaining = super::DEFAULT_OPERATION_FUEL;
    move |work| {
        remaining = remaining.checked_sub(work).ok_or_else(|| error("Global setup work fuel exhausted"))?;
        Ok(())
    }
}
impl GlobalIdentity {
    fn metered(phase: Phase, namespace: &str, name: &str, meter: &mut OwnerMeter<'_>) -> Result<Self, Diagnostic> {
        Ok(Self { phase, namespace: owner_text(namespace, meter)?, name: owner_text(name, meter)? })
    }
    fn import_name(&self) -> String { format!("{}/{}", self.namespace, self.name) }
}
impl Global {
    fn own(identity: GlobalIdentity, meter: &mut OwnerMeter<'_>) -> Result<Self, Diagnostic> {
        let bytes = std::mem::size_of::<GlobalIdentity>().checked_add(2 * std::mem::size_of::<usize>())
            .ok_or_else(|| error("Global ownership work overflow"))?;
        owner_charge(meter, bytes)?;
        // Established generic Arc allocation/OOM policy; no second text copy.
        let owner = Self(std::sync::Arc::new(identity));
        #[cfg(test)]
        GLOBAL_OWNER_ALLOCATIONS.with(|count| count.set(count.get() + 1));
        Ok(owner)
    }
    fn new_metered(phase: Phase, namespace: &str, name: &str, meter: &mut OwnerMeter<'_>) -> Result<Self, Diagnostic> {
        Self::own(GlobalIdentity::metered(phase, namespace, name, meter)?, meter)
    }
    pub(crate) fn retain_metered(&self, meter: &mut OwnerMeter<'_>) -> Result<Self, Diagnostic> {
        meter(1)?;
        Ok(self.clone())
    }
    pub fn phase(&self) -> Phase { self.phase }
    pub fn namespace(&self) -> &str { &self.namespace }
    pub fn name(&self) -> &str { &self.name }
    pub fn import_module(&self) -> &'static str {
        match self.phase { Phase::Runtime => "suss.bindings.runtime", Phase::Macro => "suss.bindings.macro" }
    }
    pub fn import_name(&self) -> String { self.0.import_name() }
}

// A borrowed stack query cannot acquire a Global or text allocation. The
// BTreeMap remains value-ordered; the query only supplies fallible work debits.
trait GlobalKey {
    fn parts(&self) -> (Phase, &str, &str);
    fn paid_cmp(&self, _other: &dyn GlobalKey) -> Option<std::cmp::Ordering> { None }
}
impl GlobalKey for Global {
    fn parts(&self) -> (Phase, &str, &str) { (self.phase(), self.namespace(), self.name()) }
}
// Owned keys and borrowed queries hash the same identity parts, so a hashed
// table finds the Global by value without acquiring an owner or String.
impl Hash for Global {
    fn hash<H: Hasher>(&self, state: &mut H) { self.parts().hash(state) }
}
impl Hash for GlobalIdentity {
    fn hash<H: Hasher>(&self, state: &mut H) { (self.phase, self.namespace.as_str(), self.name.as_str()).hash(state) }
}
impl Hash for dyn GlobalKey + '_ {
    fn hash<H: Hasher>(&self, state: &mut H) { self.parts().hash(state) }
}
/// Fixed-key SipHash keeps table layout, probing and therefore metered
/// comparison sequences deterministic across processes.
pub(crate) type BindingTable = HashMap<Global, Binding, BuildHasherDefault<std::collections::hash_map::DefaultHasher>>;
impl<'a> std::borrow::Borrow<dyn GlobalKey + 'a> for Global {
    fn borrow(&self) -> &(dyn GlobalKey + 'a) { self }
}
impl PartialEq for dyn GlobalKey + '_ {
    fn eq(&self, other: &Self) -> bool { self.cmp(other).is_eq() }
}
impl Eq for dyn GlobalKey + '_ {}
impl PartialOrd for dyn GlobalKey + '_ {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) }
}
impl Ord for dyn GlobalKey + '_ {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if let Some(order) = self.paid_cmp(other) { return order; }
        if let Some(order) = other.paid_cmp(self) { return order.reverse(); }
        self.parts().cmp(&other.parts())
    }
}
struct Query<'a> {
    phase: Phase, namespace: &'a str, name: &'a str,
    compare: Option<&'a dyn Fn(Phase, &str, &str) -> std::cmp::Ordering>,
}
impl GlobalKey for Query<'_> {
    fn parts(&self) -> (Phase, &str, &str) { (self.phase, self.namespace, self.name) }
    fn paid_cmp(&self, other: &dyn GlobalKey) -> Option<std::cmp::Ordering> {
        let compare = self.compare?;
        let (phase, namespace, name) = other.parts();
        Some(compare(phase, namespace, name))
    }
}
fn lookup_global<'s, V>(map: &'s HashMap<Global, V, impl std::hash::BuildHasher>, phase: Phase, namespace: &str,
    name: &str, meter: &mut OwnerMeter<'_>) -> Result<Option<(&'s Global, &'s V)>, Diagnostic>
{
    meter(12)?; // stack query/state/closure setup, result selection and bounded retirement
    // One keyed hash over the phase and both texts, plus its fixed finalization,
    // before the table is probed. Equality checks below are paid separately.
    let hash = namespace.len().checked_add(name.len()).and_then(|n| n.checked_add(16))
        .ok_or_else(|| error("Global hash work overflow"))?;
    owner_charge(meter, hash)?;
    let state = std::cell::RefCell::new((meter, None));
    let compare = |other_phase: Phase, other_namespace: &str, other_name: &str| {
        let mut state = state.borrow_mut();
        if state.1.is_some() { return std::cmp::Ordering::Equal; }
        let result = (|| {
            (state.0)(4)?;
            let order = phase.cmp(&other_phase);
            if !order.is_eq() { return Ok(order); }
            let mut text = |a: &str, b: &str| -> Result<std::cmp::Ordering, Diagnostic> {
                let work = a.len().checked_add(b.len()).and_then(|n| n.checked_add(2))
                    .ok_or_else(|| error("Global comparison work overflow"))?;
                owner_charge(state.0, work)?;
                Ok(a.cmp(b))
            };
            let order = text(namespace, other_namespace)?;
            if !order.is_eq() { return Ok(order); }
            text(name, other_name)
        })();
        match result {
            Ok(order) => order,
            // Read-only lookup stops on a dummy equality after the first
            // failure. It MUST discard the result and return this exact error.
            Err(error) => { state.1 = Some(error); std::cmp::Ordering::Equal }
        }
    };
    let query = Query { phase, namespace, name, compare: Some(&compare) };
    let result = map.get_key_value(&query as &dyn GlobalKey);
    let failure = state.borrow_mut().1.take();
    match failure { Some(error) => Err(error), None => Ok(result) }
}
fn lookup_global_setup<'s, V>(map: &'s HashMap<Global, V, impl std::hash::BuildHasher>, phase: Phase,
    namespace: &str, name: &str) -> Option<(&'s Global, &'s V)>
{
    // Existing infallible predicates acquire no new owner or String. Active
    // metered preparation uses lookup_global instead.
    let query = Query { phase, namespace, name, compare: None };
    map.get_key_value(&query as &dyn GlobalKey)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NominalForm {
    Deftype,
    DeftypeInternal,
    DefrecordInternal,
    Defprotocol,
    ExtendType,
    Instance,
    Satisfies,
    Implements,
    Reify,
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
/// Source snapshots retain these maps; a later scope edit copies on write.
struct Scope {
    namespace: String,
    aliases: Arc<BTreeMap<String, String>>,
    refers: Arc<BTreeMap<String, Global>>,
    used_refers: Arc<BTreeSet<String>>,
    renamed_refers: Arc<BTreeSet<String>>,
    excluded_core: Arc<BTreeSet<String>>,
    macro_aliases: Arc<BTreeMap<String, String>>,
    macro_refers: Arc<BTreeMap<String, (String, String)>>,
    used_macro_refers: Arc<BTreeSet<String>>,
    renamed_macro_refers: Arc<BTreeSet<String>>,
}
impl Scope {
    fn new(namespace: &str) -> Self {
        Self {
            namespace: namespace.into(),
            aliases: Default::default(),
            refers: Default::default(),
            used_refers: Default::default(),
            renamed_refers: Default::default(),
            excluded_core: Default::default(),
            macro_aliases: Default::default(),
            macro_refers: Default::default(),
            used_macro_refers: Default::default(),
            renamed_macro_refers: Default::default(),
        }
    }
}
/// Compiler declaration facts, distinct from initialized runtime cell metadata.
#[derive(Debug, Clone)]
pub struct DefinitionInfo {
    /// Actual definition syntax, retained before initializer analysis.
    pub definition_form: std::sync::Arc<suss_reader::forms::Form>,
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
    /// The intrinsic declaration kind, independent of user metadata overrides.
    pub type_is_record: bool,
    /// Bounded native key facts captured before type metadata is rebuilt.
    pub type_key_history: Option<std::sync::Arc<TypeKeyHistory>>,
    /// Private genuine completed ordinary projection owner; no predecessor chain.
    pub completed_values: Option<std::sync::Arc<CompletedDeclarationSource>>,
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
                span: value.span.clone(),
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
    bindings: BindingTable,
    definitions: BTreeMap<Global, std::sync::Arc<DefinitionInfo>>,
    /// Number of binding values carrying each cell Global, for identity indexing.
    cell_refs: HashMap<Global, usize, BuildHasherDefault<std::collections::hash_map::DefaultHasher>>,
    namespace_index: namespace_index::NamespaceIndexes,
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
        Self::new_metered(namespace, &mut setup_meter())
    }
    pub fn new_metered(namespace: &str, meter: &mut OwnerMeter<'_>) -> Result<Self, Diagnostic> {
        valid_namespace(namespace)?;
        let mut env = Self {
            reader_state: super::syntax_quote::ReaderState::default(),
            namespaces: BTreeSet::new(),
            bindings: BindingTable::default(),
            definitions: BTreeMap::new(),
            cell_refs: Default::default(),
            namespace_index: Default::default(),
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
                let global = Global::new_metered(phase, "suss.core", name, meter)?;
                meter(2)?; // prepaid key/Binding retains before insertion
                env.set_binding(global.clone(), Binding::Arithmetic { global, operator });
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
                ("js-delete", "js-delete-function"),
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
                let global = Global::new_metered(phase, "suss.core", name, meter)?;
                meter(2)?; // prepaid key/Binding retains before insertion
                env.set_binding(global.clone(), Binding::Core { global, export });
            }
            // Explicit owned host bindings, not declarations of a loaded JS namespace.
            for (name, export) in [
                ("Error", "error-constructor"),
                ("RangeError", "range-error-constructor"),
                ("undefined", "undefined-value"),
                ("TypeError", "type-error-constructor"),
                ("Object", "object-constructor"),
                ("NaN", "nan-value"),
            ] {
                let global = Global::new_metered(phase, "js", name, meter)?;
                meter(2)?; // prepaid key/Binding retains before insertion
                env.set_binding(global.clone(), Binding::Core { global, export });
            }
            // Private callable adapters used by the bundled compiled async library.
            for (name, export) in [
                ("pending", "async-source-pending-function"),
                ("resolve!", "async-source-resolve!-function"),
                ("reject!", "async-source-reject!-function"),
                ("cancel!", "async-source-cancel!-function"),
                ("status", "async-source-status-function"),
                ("result", "async-source-result-function"),
                ("stream-pair", "async-source-stream-pair-function"),
                (
                    "stream-pair-reader",
                    "async-source-stream-pair-reader-function",
                ),
                (
                    "stream-pair-writer",
                    "async-source-stream-pair-writer-function",
                ),
                (
                    "stream-read-limit",
                    "async-source-stream-read-limit-function",
                ),
                (
                    "stream-write-limit",
                    "async-source-stream-write-limit-function",
                ),
                (
                    "stream-begin-read",
                    "async-source-stream-begin-read-function",
                ),
                (
                    "stream-begin-write",
                    "async-source-stream-begin-write-function",
                ),
                (
                    "stream-operation-future",
                    "async-source-stream-operation-future-function",
                ),
                (
                    "stream-operation-retire",
                    "async-source-stream-operation-retire-function",
                ),
                ("stream-chunk-new", "async-source-stream-chunk-new-function"),
                ("stream-chunk-set", "async-source-stream-chunk-set-function"),
                (
                    "stream-chunk-count",
                    "async-source-stream-chunk-count-function",
                ),
                ("stream-chunk-nth", "async-source-stream-chunk-nth-function"),
                ("stream-eof?", "async-source-stream-eof?-function"),
                ("stream-close!", "async-source-stream-close!-function"),
                ("stream-fail!", "async-source-stream-fail!-function"),
            ] {
                let global = Global::new_metered(phase, "suss.internal.async", name, meter)?;
                meter(2)?; // prepaid key/Binding retains before insertion
                env.set_binding(global.clone(), Binding::Core { global, export });
            }
            for (name, form) in [
                ("deftype", NominalForm::Deftype),
                ("deftype*", NominalForm::DeftypeInternal),
                ("defrecord*", NominalForm::DefrecordInternal),
                ("defprotocol", NominalForm::Defprotocol),
                ("extend-type", NominalForm::ExtendType),
                ("instance?", NominalForm::Instance),
                ("satisfies?", NominalForm::Satisfies),
                ("implements?", NominalForm::Implements),
                ("reify", NominalForm::Reify),
            ] {
                let global = Global::new_metered(phase, "suss.core", name, meter)?;
                meter(2)?; // prepaid key/Binding retains before insertion
                env.set_binding(global.clone(), Binding::Nominal { global, form });
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
                let global = Global::new_metered(phase, "suss.core", name, meter)?;
                meter(2)?; // prepaid key/Binding retains before insertion
                env.set_binding(
                    global.clone(),
                    Binding::BootstrapControl {
                        global,
                        operation: ControlForm::from_name(name).unwrap(),
                    },
                );
            }
            for name in ["let", "loop", "fn", "defonce", "binding", "with-redefs"] {
                let global = Global::new_metered(phase, "suss.core", name, meter)?;
                meter(2)?; // prepaid key/Binding retains before insertion
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
                env.set_binding(global, binding);
            }
        }
        Ok(env)
    }
    fn scope(&self, phase: Phase) -> &Scope {
        &self.scopes[&(phase, self.current[&phase].clone())]
    }
    fn source_changed(&mut self) {
        self.source_generation = self
            .source_generation
            .checked_add(1)
            .expect("compiler source generation exhausted");
    }
    pub(crate) fn source_generation(&self) -> u64 {
        self.source_generation
    }
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
            match before.bindings.get(global) {
                Some(value) => {
                    self.set_binding(global.clone(), value.clone());
                }
                None => {
                    self.remove_binding(global);
                }
            }
            match before.definitions.get(global) {
                Some(value) => {
                    self.set_definition(global.clone(), value.clone());
                }
                None => {
                    self.remove_definition(global);
                }
            }
            let export = (phase, global.namespace.clone(), global.name.clone());
            if before.macro_exports.contains(&export) {
                self.macro_exports.insert(export);
            } else {
                self.macro_exports.remove(&export);
            }
        }
        if let Some(namespace) = before.current.get(&phase) {
            self.current.insert(phase, namespace.clone());
        }
        self.source_changed();
    }
    pub fn current_namespace(&self, phase: Phase) -> &str {
        &self.current[&phase]
    }
    pub fn namespace_scope(&self, phase: Phase) -> NamespaceScope<'_> {
        self.namespace_scope_for(phase, self.current_namespace(phase))
            .expect("declared current scope")
    }
    /// Read an actual declared scope without changing the current namespace or
    /// source generation. A missing scope stays missing, never a fallback def.
    pub fn namespace_scope_for(&self, phase: Phase, namespace: &str) -> Option<NamespaceScope<'_>> {
        let scope = self.scopes.get(&(phase, canonical(namespace).into()))?;
        Some(NamespaceScope {
            namespace: &scope.namespace, aliases: &scope.aliases,
            refers: &scope.refers, excluded_core: &scope.excluded_core,
            used_refers: &scope.used_refers, renamed_refers: &scope.renamed_refers,
            macro_aliases: &scope.macro_aliases, macro_refers: &scope.macro_refers,
            used_macro_refers: &scope.used_macro_refers, renamed_macro_refers: &scope.renamed_macro_refers,
            declarations: self.definitions.iter().filter(|(global, _)|
                global.phase() == phase && global.namespace() == scope.namespace)
                .map(|(global, info)| (global, info.as_ref())).collect(),
        })
    }
    /// Phase-specific declared namespaces; declaration does not certify loading.
    pub fn declared_namespaces(&self, phase: Phase) -> impl Iterator<Item = &str> {
        self.namespaces.iter().filter_map(move |(entry_phase, namespace)|
            (*entry_phase == phase).then_some(namespace.as_str()))
    }
    pub fn definition_info(&self, global: &Global) -> Option<&DefinitionInfo> {
        self.definitions.get(global).map(std::sync::Arc::as_ref)
    }
    /// Immutable declaration revisions retain identity through Environment clones
    /// and source snapshots. Reanalysis installs a fresh revision instead.
    pub(crate) fn shared_definition_info(
        &self,
        global: &Global,
    ) -> Option<&std::sync::Arc<DefinitionInfo>> {
        self.definitions.get(global)
    }
    pub(crate) fn record_definition(&mut self, global: Global, info: DefinitionInfo) {
        self.record_definition_metered(global, info, &mut |_| Ok(())).expect("unmetered definition record");
    }
    pub(crate) fn record_definition_metered(&mut self, global: Global, info: DefinitionInfo,
        meter: &mut OwnerMeter<'_>) -> Result<(), Diagnostic> {
        self.record_definition_shared_metered(global, info, meter).map(drop)
    }
    /// Publish and retain this exact immutable revision without a second lookup.
    /// The caller meters the allocation and retained reference before calling.
    pub(crate) fn record_definition_shared(&mut self, global: Global, info: DefinitionInfo) -> std::sync::Arc<DefinitionInfo> {
        self.record_definition_shared_metered(global, info, &mut |_| Ok(())).expect("unmetered definition record")
    }
    /// The namespace revision's structural update is prepaid before publication.
    pub(crate) fn record_definition_shared_metered(&mut self, global: Global, info: DefinitionInfo,
        meter: &mut OwnerMeter<'_>) -> Result<std::sync::Arc<DefinitionInfo>, Diagnostic> {
        meter(self.definition_update_work(&global))?;
        let owner = std::sync::Arc::new(info);
        self.source_changed();
        self.set_definition(global, owner.clone());
        Ok(owner)
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
                Binding::Cell(global)
                | Binding::InternalCell(global)
                | Binding::ReaderCell(global) => Some(global.clone()),
                _ => None,
            })
            .chain(self.materialized_bootstrap.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    #[cfg(test)]
    pub(crate) fn clear_bindings(&mut self) {
        let keys: Vec<_> = self.bindings.keys().cloned().collect();
        for key in keys { self.remove_binding(&key); }
    }
    pub(crate) fn materialize_bootstrap(&mut self, global: Global) {
        self.materialize_bootstrap_metered(global, &mut |_| Ok(())).expect("unmetered bootstrap materialization");
    }
    pub(crate) fn materialize_bootstrap_metered(&mut self, global: Global, meter: &mut OwnerMeter<'_>) -> Result<(), Diagnostic> {
        if !self.materialized_bootstrap.contains(&global) {
            meter(self.bootstrap_update_work(&global))?;
            self.materialized_bootstrap.insert(global.clone());
            self.source_changed();
            self.index_bootstrap(&global);
        }
        Ok(())
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
            .collect::<BTreeMap<_, _>>()
            .into_iter()
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
            .collect::<BTreeMap<_, _>>()
            .into_iter()
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
    pub fn declare_cell(&mut self, phase: Phase, namespace: &str, name: &str) -> Result<Global, Diagnostic> {
        self.declare_cell_metered(phase, namespace, name, &mut setup_meter())
    }
    pub(crate) fn declare_cell_metered(&mut self, phase: Phase, namespace: &str, name: &str, meter: &mut OwnerMeter<'_>) -> Result<Global, Diagnostic> {
        valid_namespace(namespace)?;
        valid_name(name)?;
        let namespace = canonical(namespace);
        owner_charge(meter, namespace.len())?; // existing scope-key copy before acquisition
        let scope = self.scopes.get(&(phase, namespace.to_owned()));
        if let Some(target) = scope.and_then(|scope| scope.refers.get(name)) {
            let work = target.namespace().len().checked_add(namespace.len())
                .and_then(|n| n.checked_add(target.name().len()))
                .and_then(|n| n.checked_add(name.len())).and_then(|n| n.checked_add(4))
                .ok_or_else(|| error("Global comparison work overflow"))?;
            owner_charge(meter, work)?;
            if target.phase() != phase || target.namespace() != namespace || target.name() != name {
                return Err(error(format!("Ambiguous binding {name}")));
            }
        }
        let existing = lookup_global(&self.bindings, phase, namespace, name, meter)?;
        if matches!(existing, Some((_, Binding::InternalCell(_)))) {
            return Err(error("Cannot redefine a compiler-owned binding"));
        }
        let global = if let Some((key, _)) = existing { key.retain_metered(meter)? }
            else { Global::new_metered(phase, namespace, name, meter)? };
        meter(2)?;
        meter(self.binding_update_work(&global, Some(&Binding::Cell(global.clone()))))?;
        self.declare_namespace(phase, global.namespace())?;
        self.set_binding(global.clone(), Binding::Cell(global.clone()));
        self.source_changed();
        Ok(global)
    }
    /// A rollback key does not declare/rebind a source cell or consult caller
    /// aliases/refers. Preserve the old fresh-environment key's phase/name
    /// validation while sharing an existing identity when one is present.
    pub(crate) fn checkpoint_global_metered(&self, phase: Phase, namespace: &str, name: &str, meter: &mut OwnerMeter<'_>) -> Result<Global, Diagnostic> {
        valid_namespace(namespace)?;
        valid_name(name)?;
        let namespace = canonical(namespace);
        if let Some((key, _)) = lookup_global(&self.bindings, phase, namespace, name, meter)? {
            key.retain_metered(meter)
        } else { Global::new_metered(phase, namespace, name, meter) }
    }
    pub(crate) fn is_hidden_cell(&self, phase: Phase, namespace: &str, name: &str) -> bool {
        let global = Query { phase, namespace: canonical(namespace), name, compare: None };
        matches!(
            self.bindings.get(&global as &dyn GlobalKey),
            Some(Binding::ReaderCell(_) | Binding::InternalCell(_))
        )
    }
    pub(crate) fn reader_sequence_cells(&mut self, phase: Phase) -> (Global, Global, bool) {
        self.reader_sequence_cells_metered(phase, &mut setup_meter()).expect("bounded reader binding setup")
    }
    pub(crate) fn reader_sequence_cells_metered(&mut self, phase: Phase, meter: &mut OwnerMeter<'_>) -> Result<(Global, Global, bool), Diagnostic> {
        let reader = lookup_global(&self.bindings, phase, "suss.core", "sequence", meter)?;
        let new_global = reader.is_none();
        let global = if let Some((key, _)) = reader { key.retain_metered(meter)? }
            else { Global::new_metered(phase, "suss.core", "sequence", meter)? };
        let private = lookup_global(&self.bindings, phase, "suss.internal.reader", "sequence", meter)?;
        let fresh = private.is_none();
        let fallback = if let Some((key, _)) = private { key.retain_metered(meter)? }
            else { Global::new_metered(phase, "suss.internal.reader", "sequence", meter)? };
        meter(2 * usize::from(new_global) as u64 + 2 * usize::from(fresh) as u64)?;
        // Each bound covers its own update; the second's index grows by at most one.
        let mut work = 0;
        if new_global { work += self.binding_update_work(&global, Some(&Binding::ReaderCell(global.clone()))); }
        if fresh { work += self.binding_update_work(&fallback, Some(&Binding::InternalCell(fallback.clone()))) + 64; }
        meter(work)?;
        if new_global { self.set_binding(global.clone(), Binding::ReaderCell(global.clone())); }
        if fresh {
            self.set_binding(fallback.clone(), Binding::InternalCell(fallback.clone()));
            self.source_changed();
        }
        Ok((global, fallback, fresh))
    }
    pub(crate) fn reify_class_key_metered(&mut self, phase: Phase, generation: u64, site: usize, meter: &mut OwnerMeter<'_>) -> Result<Global, Diagnostic> {
        let digits = |n: u64| if n == 0 { 1 } else { n.ilog10() as usize + 1 };
        let length = 9usize.checked_add(digits(generation)).and_then(|n| n.checked_add(digits(site as u64)))
            .ok_or_else(|| error("Global identity size overflow"))?;
        owner_charge(meter, length)?;
        let mut name = String::new();
        name.try_reserve_exact(length).map_err(|_| error("Unable to reserve reify Global name"))?;
        use std::fmt::Write;
        write!(&mut name, "t-reify-{generation}-{site}").expect("String formatting");
        let existing = lookup_global(&self.bindings, phase, "suss.internal.reify", &name, meter)?;
        let fresh = existing.is_none();
        let global = if let Some((key, _)) = existing { key.retain_metered(meter)? }
            else { Global::own(GlobalIdentity { phase, namespace: owner_text("suss.internal.reify", meter)?, name }, meter)? };
        if fresh {
            meter(2)?;
            meter(self.binding_update_work(&global, Some(&Binding::InternalCell(global.clone()))))?;
            self.set_binding(global.clone(), Binding::InternalCell(global.clone()));
            self.source_changed();
        }
        Ok(global)
    }
    pub(crate) fn protocol_key(&mut self, protocol: &Global, method: &str, arity: usize) -> Global {
        self.protocol_key_metered(protocol, method, arity, &mut setup_meter()).expect("bounded protocol binding setup")
    }
    pub(crate) fn protocol_key_metered(&mut self, protocol: &Global, method: &str, arity: usize, meter: &mut OwnerMeter<'_>) -> Result<Global, Diagnostic> {
        let digits = if arity == 0 { 1 } else { arity.ilog10() as usize + 1 };
        let length = protocol.namespace.len().checked_add(protocol.name.len())
            .and_then(|n| n.checked_add(method.len())).and_then(|n| n.checked_add(digits))
            .and_then(|n| n.checked_add(3)).ok_or_else(|| error("Global identity size overflow"))?;
        owner_charge(meter, length)?;
        let mut identity = String::new();
        identity.try_reserve_exact(length).map_err(|_| error("Unable to reserve protocol identity"))?;
        use std::fmt::Write;
        write!(&mut identity, "{}/{}/{}/{}", protocol.namespace, protocol.name, method, arity).expect("String formatting");
        let name_len = length.checked_mul(2).and_then(|n| n.checked_add(4)).ok_or_else(|| error("Global identity size overflow"))?;
        owner_charge(meter, name_len)?;
        let mut name = String::new();
        name.try_reserve_exact(name_len).map_err(|_| error("Unable to reserve protocol Global name"))?;
        name.push_str("key-");
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for byte in identity.bytes() { name.push(HEX[(byte >> 4) as usize] as char); name.push(HEX[(byte & 15) as usize] as char); }
        let global = if let Some((key, _)) = lookup_global(&self.bindings, protocol.phase, "suss.internal.protocol-keys", &name, meter)? {
            key.retain_metered(meter)?
        } else { Global::own(GlobalIdentity { phase: protocol.phase, namespace: owner_text("suss.internal.protocol-keys", meter)?, name }, meter)? };
        meter(2)?;
        meter(self.binding_update_work(&global, Some(&Binding::InternalCell(global.clone()))))?;
        self.set_binding(global.clone(), Binding::InternalCell(global.clone()));
        Ok(global)
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
        let catalog = if macros {
            &self.macro_namespaces
        } else {
            &self.namespaces
        };
        if !catalog.contains(&(phase, target.into())) {
            return Err(error(format!(
                "Unknown {phase:?} required namespace {namespace}"
            )));
        }
        let scope = self.scope_mut(phase);
        let aliases = Arc::make_mut(if macros {
            &mut scope.macro_aliases
        } else {
            &mut scope.aliases
        });
        if aliases.get(namespace).is_some_and(|old| old != target) {
            return Err(error(format!(
                "Ambiguous required namespace alias {namespace}"
            )));
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
        Arc::make_mut(&mut self.scope_mut(phase).aliases)
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
    pub(crate) fn has_binding(&self, phase: Phase, namespace: &str, name: &str) -> bool {
        lookup_global_setup(&self.bindings, phase, canonical(namespace), name).is_some()
    }
    pub(crate) fn refer_with_role(
        &mut self,
        phase: Phase,
        local: &str,
        namespace: &str,
        name: &str,
        renamed: bool,
    ) -> Result<(), Diagnostic> {
        self.refer_with_role_metered(phase, local, namespace, name, renamed, &mut setup_meter())
    }
    pub(crate) fn refer_with_role_metered(&mut self, phase: Phase, local: &str, namespace: &str,
        name: &str, renamed: bool, meter: &mut OwnerMeter<'_>) -> Result<(), Diagnostic>
    {
        valid_name(local)?;
        valid_namespace(namespace)?;
        valid_name(name)?;
        let namespace = canonical(namespace);
        let Some((target, _)) = lookup_global(&self.bindings, phase, namespace, name, meter)? else {
            return Err(error(format!("Unknown {phase:?} binding {namespace}/{name}")));
        };
        let scope = self.scope(phase);
        if let Some(old) = scope.refers.get(local) {
            owner_charge(meter, old.namespace().len().checked_add(namespace.len())
                .and_then(|n| n.checked_add(old.name().len()))
                .and_then(|n| n.checked_add(name.len())).and_then(|n| n.checked_add(4))
                .ok_or_else(|| error("Global comparison work overflow"))?)?;
            if old != target { return Err(error(format!("Ambiguous binding {local}"))); }
        }
        if let Some((own, _)) = lookup_global(&self.bindings, phase, &scope.namespace, local, meter)? {
            let work = own.namespace().len().checked_add(target.namespace().len())
                .and_then(|n| n.checked_add(own.name().len()))
                .and_then(|n| n.checked_add(target.name().len())).and_then(|n| n.checked_add(4))
                .ok_or_else(|| error("Global comparison work overflow"))?;
            owner_charge(meter, work)?;
            if own != target { return Err(error(format!("Ambiguous binding {local}"))); }
        }
        let global = target.retain_metered(meter)?;
        let scope = self.scope_mut(phase);
        Arc::make_mut(&mut scope.refers).insert(local.into(), global);
        Arc::make_mut(if renamed { &mut scope.renamed_refers } else { &mut scope.used_refers })
            .insert(local.into());
        Ok(())
    }
    pub fn exclude_core(&mut self, phase: Phase, name: &str) -> Result<(), Diagnostic> {
        valid_name(name)?;
        Arc::make_mut(&mut self.scope_mut(phase).excluded_core).insert(name.into());
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
        Arc::make_mut(&mut scope.macro_aliases).insert(alias.into(), namespace.into());
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
        Arc::make_mut(&mut scope.macro_refers).insert(local.into(), target);
        Arc::make_mut(if renamed {
            &mut scope.renamed_macro_refers
        } else {
            &mut scope.used_macro_refers
        })
        .insert(local.into());
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
        let own = Query { phase, namespace: &scope.namespace, name: &symbol.name, compare: None };
        symbol.name == name
            && !scope.excluded_core.contains(name)
            && !self.bindings.contains_key(&own as &dyn GlobalKey)
    }
    /// The bounded bootstrap macro lookup is separate from ordinary var lookup:
    /// lexical locals (handled by HIR) hide macros. User runtime definitions also
    /// hide automatic core bootstrap macros, as observed in the pinned compiler.
    /// General compiled macro imports/expansion remain a later integration.
    fn selected_bootstrap_name<'a>(&'a self, phase: Phase, symbol: &'a Symbol,
        meter: &mut OwnerMeter<'_>) -> Result<Option<&'a str>, Diagnostic> {
        meter(2)?;
        let mut failure = None;
        let selected = (|| {
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
                    | "deftype*"
                    | "defrecord*"
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
                    | "deftype*"
                    | "defrecord*"
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
                    | "deftype*"
                    | "defrecord*"
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
                    | "deftype*"
                    | "defrecord*"
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
            && match lookup_global(&self.bindings, phase, &scope.namespace, &symbol.name, meter) {
                Ok(value) => value.is_some(),
                Err(error) => { failure = Some(error); return None; }
            }
        {
            return None;
        }
        Some(name)
            })();
        match failure { Some(error) => Err(error), None => Ok(selected) }
    }
    /// A syntax-quote predicate needs no Global owner or fallible allocation.
    pub(crate) fn has_bootstrap_macro(&self, phase: Phase, symbol: &Symbol) -> bool {
        self.selected_bootstrap_name(phase, symbol, &mut setup_meter()).expect("bounded bootstrap predicate").is_some()
    }
    pub(crate) fn has_bootstrap_macro_metered(&self, phase: Phase, symbol: &Symbol,
        meter: &mut OwnerMeter<'_>) -> Result<bool, Diagnostic> {
        Ok(self.selected_bootstrap_name(phase, symbol, meter)?.is_some())
    }
    /// Legacy setup convenience: selection is bounded to fixed compiler names.
    /// Preparation uses the fallible metered variant, never hides meter errors.
    pub fn resolve_bootstrap_macro(&self, phase: Phase, symbol: &Symbol) -> Option<Binding> {
        self.resolve_bootstrap_macro_metered(phase, symbol, &mut setup_meter()).expect("bounded bootstrap Global setup")
    }
    pub(crate) fn resolve_bootstrap_macro_metered(&self, phase: Phase, symbol: &Symbol, meter: &mut OwnerMeter<'_>) -> Result<Option<Binding>, Diagnostic> {
        let Some(name) = self.selected_bootstrap_name(phase, symbol, meter)? else { return Ok(None) };
        let global = if let Some((key, _)) = lookup_global(&self.bindings, phase, "suss.core", name, meter)? {
            key.retain_metered(meter)?
        } else { Global::new_metered(phase, "suss.core", name, meter)? };
        // Preserve the original bootstrap VARIANT, independent of live Binding.
        Ok(Some(if let Some(operation) = ControlForm::from_name(name) {
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
            Binding::BootstrapLoop(global)
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
                    "deftype*" => NominalForm::DeftypeInternal,
                    "defrecord*" => NominalForm::DefrecordInternal,
                    "defprotocol" => NominalForm::Defprotocol,
                    "instance?" => NominalForm::Instance,
                    "satisfies?" => NominalForm::Satisfies,
                    "implements?" => NominalForm::Implements,
                    _ => NominalForm::ExtendType,
                },
            }
        }))
    }
    // Native compiler interface catalogs select an actual fully qualified
    // cell, not a user alias or a fabricated declaration. No source capability.
    pub(crate) fn native_binding_metered(&self, phase: Phase, namespace: &str, name: &str,
        meter: &mut OwnerMeter<'_>) -> Result<Option<Binding>, Diagnostic>
    {
        let Some((_, binding)) = lookup_global(&self.bindings, phase, namespace, name, meter)? else { return Ok(None) };
        if matches!(binding, Binding::InternalCell(_) | Binding::ReaderCell(_)) { return Ok(None); }
        meter(1)?;
        Ok(Some(binding.clone()))
    }
    pub fn resolve(&self, phase: Phase, symbol: &Symbol, span: Range<usize>) -> Result<Binding, Diagnostic> {
        self.resolve_metered(phase, symbol, span, &mut setup_meter())
    }
    pub(crate) fn resolve_metered(&self, phase: Phase, symbol: &Symbol, span: Range<usize>,
        meter: &mut OwnerMeter<'_>) -> Result<Binding, Diagnostic>
    {
        let result = (|| {
            meter(4)?;
            meter(4)?; // borrowed hit slot setup/store, selection and retirement
            let mut current_hit = None;
            let scope = self.scope(phase);
            let (namespace, name) = if let Some(namespace) = &symbol.namespace {
                let namespace = if namespace == "js" { "js" }
                    else if matches!(namespace.as_str(), "suss.core" | "cljs.core") { canonical(namespace) }
                    else { symbol_namespace(scope.aliases.get(namespace), namespace) };
                (namespace, symbol.name.as_str())
            } else if let Some(global) = scope.refers.get(&symbol.name) {
                (global.namespace(), global.name())
            } else {
                current_hit = lookup_global(&self.bindings, phase, &scope.namespace, &symbol.name, meter)?;
                let namespace = if current_hit.is_some()
                    || scope.excluded_core.contains(&symbol.name) { scope.namespace.as_str() }
                    else { "suss.core" };
                (namespace, symbol.name.as_str())
            };
            // A current hit already selected this exact immutable binding.
            // Keep private-cell rejection below: a hidden hit cannot fall back
            // to core. Misses retain their original second lookup.
            let selected = match current_hit {
                Some(hit) => Some(hit),
                None => lookup_global(&self.bindings, phase, namespace, name, meter)?,
            };
            let binding = selected
                .map(|(_, value)| value)
                .filter(|binding| !matches!(binding, Binding::InternalCell(_) | Binding::ReaderCell(_)))
                .ok_or_else(|| Diagnostic { span: span.clone(), message: format!("Unresolved {phase:?} name {symbol}") })?;
            meter(1)?; // exactly one Global retained by this Binding clone
            Ok(binding.clone())
        })();
        result.map_err(|mut error: Diagnostic| { error.span = span; error })
    }
}

/// Locate one source across all supported extensions/roots. Never pick by search order.
/// Canonical paths deduplicate the same file reached through multiple roots.
pub fn locate_source(
    namespace: &str,
    roots: &[impl AsRef<Path>],
    span: Range<usize>,
) -> Result<PathBuf, Diagnostic> {
    locate_source_if_present(namespace, roots, span.clone())?.ok_or_else(|| Diagnostic {
        span,
        message: format!("No source for namespace {}", canonical(namespace)),
    })
}

/// Absence alone permits an embedded-library fallback. Ambiguity, invalid names
/// and filesystem failures remain errors rather than becoming library successes.
pub(crate) fn locate_source_if_present(
    namespace: &str,
    roots: &[impl AsRef<Path>],
    span: Range<usize>,
) -> Result<Option<PathBuf>, Diagnostic> {
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
                Ok(path) => {
                    return Err(Diagnostic {
                        span,
                        message: format!("Namespace source is not a file: {}", path.display()),
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    match std::fs::symlink_metadata(&path) {
                        Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => {
                            // A missing leaf beneath a broken directory link is
                            // a configured source error, not absent source.
                            for ancestor in path.ancestors().skip(1) {
                                if let Ok(metadata) = std::fs::symlink_metadata(ancestor) {
                                    if metadata.file_type().is_symlink() {
                                        if let Err(error) = ancestor.canonicalize() {
                                            return Err(Diagnostic {
                                                span,
                                                message: format!(
                                                    "Cannot resolve namespace source: {error}"
                                                ),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        _ => {
                            return Err(Diagnostic {
                                span,
                                message: format!("Cannot resolve namespace source: {error}"),
                            });
                        }
                    }
                }
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
        1 => Ok(Some(matches.into_iter().next().unwrap())),
        0 => Ok(None),
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


#[cfg(test)]
mod global_ownership_tests {
    use super::*;
    use std::{borrow::Borrow, sync::Arc};

    fn allocations() -> usize { GLOBAL_OWNER_ALLOCATIONS.with(|count| count.get()) }
    fn same_owner(a: &Global, b: &Global) -> bool { Arc::ptr_eq(&a.0, &b.0) }

    #[test]
    fn hir_callers_propagate_every_interruption_and_recover_without_publishing() {
        struct InterruptHost {
            calls: usize,
            reject: Option<usize>,
            remaining: u64,
        }
        impl super::super::ExpansionHost for InterruptHost {
            fn expansion_work_fuel(&self) -> u64 { self.remaining }
            fn charge_expansion_work(&mut self, work: u64, span: Range<usize>) -> Result<(), Diagnostic> {
                let call = self.calls;
                self.calls += 1;
                if self.reject == Some(call) {
                    return Err(Diagnostic { span, message: "HIR selector interruption".into() });
                }
                self.remaining = self.remaining.checked_sub(work)
                    .ok_or_else(|| error("Test exhausted the default preparation budget"))?;
                Ok(())
            }
            fn expand(&mut self, _: &suss_reader::forms::Form, _: super::super::ExpansionContext<'_>)
                -> Result<Option<suss_reader::forms::Form>, Diagnostic> { Ok(None) }
        }
        let host = |reject| InterruptHost {
            calls: 0, reject, remaining: super::super::DEFAULT_OPERATION_FUEL,
        };
        for phase in [Phase::Runtime, Phase::Macro] {
            // let/loop exercise the primitive fallback caller; inc is an
            // initially absent selector reaching the later control caller.
            for source in ["(let [x 7] x)", "(loop [x 7] x)", "(inc 7)"] {
                let env = Environment::default();
                let generation = env.source_generation();
                let forms = suss_reader::forms::read_forms(source).unwrap();
                let mut complete = host(None);
                super::super::hir::prepare_with_expander(
                    &forms, 0..source.len(), &env, phase, &mut complete,
                ).unwrap();
                assert!(complete.calls > 0);
                // Reject every charged boundary, including both resolver
                // callers. A swallowed Err or fallback to ordinary resolution
                // would either succeed or replace this exact diagnostic.
                for reject in 0..complete.calls {
                    let mut interrupted = host(Some(reject));
                    let failure = super::super::hir::prepare_with_expander(
                        &forms, 0..source.len(), &env, phase, &mut interrupted,
                    ).unwrap_err();
                    assert_eq!(failure.message, "HIR selector interruption", "{phase:?} {source} boundary {reject}");
                    assert_eq!(interrupted.calls, reject + 1);
                    assert_eq!(env.source_generation(), generation);
                }
                super::super::hir::prepare_with_expander(
                    &forms, 0..source.len(), &env, phase, &mut host(None),
                ).unwrap();
            }
        }
    }

    #[test]
    fn borrowed_queries_aliases_and_bootstrap_selection_allocate_no_identity_owners() {
        let mut env = Environment::new("lookup.owner").unwrap();
        let global = env.declare_cell(Phase::Runtime, "target", "value").unwrap();
        env.alias(Phase::Runtime, "t", "target").unwrap();
        env.refer(Phase::Runtime, "renamed", "target", "value").unwrap();
        let count = allocations();
        let strong = Arc::strong_count(&global.0);
        for _ in 0..20 {
            for symbol in [Symbol::namespaced("t", "value"), Symbol::new("renamed")] {
                let binding = env.resolve(Phase::Runtime, &symbol, 0..0).unwrap();
                assert!(same_owner(&global, binding.global()));
                let query: &GlobalIdentity = global.borrow();
                assert!(same_owner(&global, env.bindings.get_key_value(query).unwrap().0));
            }
            for phase in [Phase::Runtime, Phase::Macro] {
                for name in BOOTSTRAP_GLOBAL_NAMES {
                    let symbol = Symbol::namespaced("cljs.core", *name);
                    assert!(env.has_bootstrap_macro(phase, &symbol), "{name}");
                    if env.has_binding(phase, "suss.core", name) {
                        assert!(env.resolve_bootstrap_macro_metered(phase, &symbol, &mut |_| Ok(())).unwrap().is_some());
                    }
                }
            }
        }
        assert_eq!(allocations(), count);
        assert_eq!(Arc::strong_count(&global.0), strong);
        // The ordinary live Binding changes; the explicit bootstrap selector
        // still returns its original VARIANT while retaining the same key.
        let core = env.resolve(Phase::Runtime, &Symbol::namespaced("cljs.core", "<"), 0..0).unwrap().global().clone();
        let replacement = env.declare_cell(Phase::Runtime, "suss.core", "<").unwrap();
        assert!(same_owner(&core, &replacement));
        assert!(matches!(env.resolve(Phase::Runtime, &Symbol::namespaced("cljs.core", "<"), 0..0).unwrap(), Binding::Cell(_)));
        let selected = env.resolve_bootstrap_macro(Phase::Runtime, &Symbol::namespaced("cljs.core", "<")).unwrap();
        assert!(matches!(&selected, Binding::BootstrapComparison { .. }));
        assert!(same_owner(&core, selected.global()));
    }

    #[test]
    fn all_bootstrap_variants_meter_absent_owners_and_restore_without_publishing_fallbacks() {
        let mut env = Environment::new("restoration").unwrap();
        for phase in [Phase::Runtime, Phase::Macro] {
            for name in BOOTSTRAP_GLOBAL_NAMES {
                let symbol = Symbol::namespaced("cljs.core", *name);
                let existing = env.has_binding(phase, "suss.core", name);
                let before = allocations();
                let mut charges = Vec::new();
                let selected = env.resolve_bootstrap_macro_metered(phase, &symbol, &mut |work| { charges.push(work); Ok(()) }).unwrap().unwrap();
                assert_eq!(allocations() - before, usize::from(!existing));
                assert_eq!(selected.global().namespace(), "suss.core");
                assert_eq!(selected.global().name(), *name);
                assert_eq!(env.has_binding(phase, "suss.core", name), existing);
                for reject in 0..charges.len() {
                    let mut calls = 0;
                    let result = env.resolve_bootstrap_macro_metered(phase, &symbol, &mut |_| {
                        let current = calls; calls += 1;
                        if current == reject { Err(error("selector interruption")) } else { Ok(()) }
                    });
                    assert_eq!(result.unwrap_err().message, "selector interruption");
                    assert_eq!(calls, reject + 1);
                }
            }
            assert!(!env.has_binding(phase, "suss.core", "->"));
            let prior = env.clone();
            let symbol = Symbol::namespaced("cljs.core", "->");
            let absent = env.resolve_bootstrap_macro_metered(phase, &symbol, &mut |_| Ok(())).unwrap().unwrap();
            let declared = env.declare_cell(phase, "suss.core", "->").unwrap();
            let selected = env.resolve_bootstrap_macro_metered(phase, &symbol, &mut |_| Ok(())).unwrap().unwrap();
            assert!(matches!(&selected, Binding::BootstrapControl { operation: ControlForm::ThreadFirst, .. }));
            assert!(same_owner(&declared, selected.global()));
            env.restore_declarations(&prior, &[declared], phase);
            assert!(!env.has_binding(phase, "suss.core", "->"));
            let count = allocations();
            let restored = env.resolve_bootstrap_macro_metered(phase, &symbol, &mut |_| Ok(())).unwrap().unwrap();
            assert_eq!(allocations(), count + 1);
            assert_eq!(absent, restored);
            assert!(!same_owner(absent.global(), restored.global()));
        }
    }

    #[test]
    fn identity_value_order_debug_and_imports_do_not_depend_on_arc_address() {
        let a = Global::new_metered(Phase::Runtime, "app", "value", &mut |_| Ok(())).unwrap();
        let b = Global::new_metered(Phase::Runtime, "app", "value", &mut |_| Ok(())).unwrap();
        let other_phase = Global::new_metered(Phase::Macro, "app", "value", &mut |_| Ok(())).unwrap();
        assert!(!same_owner(&a, &b));
        assert_eq!(a, b);
        assert!(a < other_phase);
        assert_eq!(format!("{a:?}"), "Global { phase: Runtime, namespace: \"app\", name: \"value\" }");
        assert_eq!(a.import_module(), "suss.bindings.runtime");
        assert_eq!(other_phase.import_module(), "suss.bindings.macro");
        assert_eq!(a.import_name(), "app/value");
        let mut map = BTreeMap::new();
        map.insert(a.clone(), 17);
        map.insert(b.clone(), 19);
        let query = GlobalIdentity { phase: Phase::Runtime, namespace: "app".into(), name: "value".into() };
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(&query), Some(&19));
        let count = Arc::strong_count(&a.0);
        assert!(a.retain_metered(&mut |_| Err(error("retain cancellation"))).is_err());
        assert_eq!(Arc::strong_count(&a.0), count);
        let mut charged = 0;
        let retained = a.retain_metered(&mut |work| { charged += work; Ok(()) }).unwrap();
        assert_eq!(charged, 1);
        assert!(same_owner(&a, &retained));
    }

    #[test]
    fn creation_and_binding_publication_prepay_all_work_and_retry_without_partial_state() {
        let mut charges = Vec::new();
        let before = allocations();
        let owner = Global::new_metered(Phase::Runtime, "fresh", "value", &mut |work| { charges.push(work); Ok(()) }).unwrap();
        assert_eq!(charges, vec![5, 5, (std::mem::size_of::<GlobalIdentity>() + 2 * std::mem::size_of::<usize>()) as u64]);
        assert_eq!(allocations(), before + 1);
        for reject in 0..charges.len() {
            let before = allocations();
            let mut calls = 0;
            let result = Global::new_metered(Phase::Runtime, "fresh", "value", &mut |_| {
                let current = calls; calls += 1;
                if current == reject { Err(error("owner interruption")) } else { Ok(()) }
            });
            assert_eq!(result.unwrap_err().message, "owner interruption");
            assert_eq!(allocations(), before);
        }
        let env = Environment::new("publication").unwrap();
        let mut trial = env.clone();
        let mut charges = Vec::new();
        trial.declare_cell_metered(Phase::Runtime, "publication", "fresh", &mut |work| { charges.push(work); Ok(()) }).unwrap();
        for reject in 0..charges.len() {
            let mut trial = env.clone();
            let generation = trial.source_generation();
            let mut calls = 0;
            let result = trial.declare_cell_metered(Phase::Runtime, "publication", "fresh", &mut |_| {
                let current = calls; calls += 1;
                if current == reject { Err(error("publication interruption")) } else { Ok(()) }
            });
            assert_eq!(result.unwrap_err().message, "publication interruption");
            assert_eq!(trial.source_generation(), generation);
            assert_eq!(trial.bindings, env.bindings);
            let created = trial.declare_cell_metered(Phase::Runtime, "publication", "fresh", &mut |_| Ok(())).unwrap();
            let generation = trial.source_generation();
            let rebound = trial.declare_cell_metered(Phase::Runtime, "publication", "fresh", &mut |_| Ok(())).unwrap();
            assert!(same_owner(&created, &rebound));
            assert!(trial.source_generation() > generation);
        }
        drop(owner);
        assert!(Environment::new_metered("publication", &mut |_| Err(error("initialization interruption"))).is_err());
    }

    #[test]
    fn reader_pair_staging_and_readonly_checkpoint_identity_do_not_publish_partial_cells() {
        let env = Environment::default();
        let mut trial = env.clone();
        let mut charges = Vec::new();
        trial.reader_sequence_cells_metered(Phase::Runtime, &mut |work| { charges.push(work); Ok(()) }).unwrap();
        for reject in 0..charges.len() {
            let mut trial = env.clone();
            let mut calls = 0;
            assert!(trial.reader_sequence_cells_metered(Phase::Runtime, &mut |_| {
                let current = calls; calls += 1;
                if current == reject { Err(error("reader interruption")) } else { Ok(()) }
            }).is_err());
            assert_eq!(trial.bindings, env.bindings);
            assert_eq!(trial.source_generation(), env.source_generation());
            let (reader, fallback, fresh) = trial.reader_sequence_cells_metered(Phase::Runtime, &mut |_| Ok(())).unwrap();
            assert!(fresh);
            let (again, private, fresh) = trial.reader_sequence_cells_metered(Phase::Runtime, &mut |_| Ok(())).unwrap();
            assert!(!fresh);
            assert!(same_owner(&reader, &again) && same_owner(&fallback, &private));
        }
        let generation = env.source_generation();
        let count = env.bindings.len();
        let key = env.checkpoint_global_metered(Phase::Macro, "new.namespace", "future", &mut |_| Ok(())).unwrap();
        assert_eq!(key.import_name(), "new.namespace/future");
        assert_eq!(env.bindings.len(), count);
        assert_eq!(env.source_generation(), generation);
        let mut bound = env.clone();
        let cell = bound.declare_cell(Phase::Macro, "new.namespace", "future").unwrap();
        let before = allocations();
        let checkpoint = bound.checkpoint_global_metered(Phase::Macro, "new.namespace", "future", &mut |_| Ok(())).unwrap();
        assert!(same_owner(&cell, &checkpoint));
        assert_eq!(allocations(), before);
    }

    #[test]
    fn redefinition_shares_language_identity_but_preserves_distinct_definition_revisions() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let first = suss_reader::forms::read_forms("(def chosen 1)").unwrap();
            let (_, env) = super::super::hir::prepare_with_expander(&first, 0..14, &Environment::default(), phase, &mut super::super::NoExpansion).unwrap();
            let before = env.resolve(phase, &Symbol::new("chosen"), 0..0).unwrap().global().clone();
            let old_revision = env.shared_definition_info(&before).unwrap().clone();
            let second = suss_reader::forms::read_forms("(def chosen 2)").unwrap();
            let (_, revised) = super::super::hir::prepare_with_expander(&second, 0..14, &env, phase, &mut super::super::NoExpansion).unwrap();
            let after = revised.resolve(phase, &Symbol::new("chosen"), 0..0).unwrap().global().clone();
            assert!(same_owner(&before, &after));
            assert!(!Arc::ptr_eq(&old_revision, revised.shared_definition_info(&after).unwrap()));
            assert!(Arc::ptr_eq(&old_revision, env.shared_definition_info(&before).unwrap()));
        }
    }

    #[test]
    fn borrowed_metered_queries_pay_each_comparison_and_never_expose_failed_results() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut env = Environment::new("borrowed.lookup").unwrap();
            let original = env.declare_cell(phase, "borrowed.target", "actual").unwrap();
            env.alias(phase, "t", "borrowed.target").unwrap();
            env.refer(phase, "selected", "borrowed.target", "actual").unwrap();
            let current = env.declare_cell(phase, "borrowed.lookup", "local").unwrap();
            let before = allocations();
            let generation = env.source_generation();
            let bindings = env.bindings.clone();
            for (symbol, expected) in [
                (Symbol::namespaced("t", "actual"), &original),
                (Symbol::new("selected"), &original),
                (Symbol::new("local"), &current),
            ] {
                let strong = Arc::strong_count(&expected.0);
                let mut one_search = Vec::new();
                lookup_global(&env.bindings, phase, expected.namespace(), expected.name(),
                    &mut |work| { one_search.push(work); Ok(()) }).unwrap().unwrap();
                let mut charges = Vec::new();
                let selected = env.resolve_metered(phase, &symbol, 11..19,
                    &mut |work| { charges.push(work); Ok(()) }).unwrap();
                assert!(same_owner(expected, selected.global()));
                assert_eq!(Arc::strong_count(&expected.0), strong + 1);
                let mut expected_charges = vec![4, 4];
                expected_charges.extend(one_search);
                expected_charges.push(1);
                assert_eq!(charges, expected_charges, "exactly one search and one retained result for {symbol}");
                drop(selected);
                assert_eq!(allocations(), before, "lookup hit allocates no identity owner");
                assert!(charges.len() > 2, "real hash, comparisons and retained result are charged");
                for reject in 0..charges.len() {
                    let mut calls = 0;
                    let result = env.resolve_metered(phase, &symbol, 11..19, &mut |_| {
                        let call = calls; calls += 1;
                        if call == reject { Err(error("borrowed comparison cancellation")) } else { Ok(()) }
                    });
                    let failure = result.unwrap_err();
                    assert_eq!(failure.message, "borrowed comparison cancellation");
                    assert_eq!(failure.span, 11..19);
                    assert_eq!(calls, reject + 1, "do not continue charging or use an apparent hit after failure");
                    assert_eq!(env.bindings, bindings);
                    assert_eq!(env.source_generation(), generation);
                    assert_eq!(allocations(), before);
                    assert_eq!(Arc::strong_count(&expected.0), strong);
                }
                let retry = env.resolve_metered(phase, &symbol, 11..19, &mut |_| Ok(())).unwrap();
                assert!(same_owner(expected, retry.global()));
            }
            let other = Global::new_metered(phase, "borrowed.target", "actual", &mut |_| Ok(())).unwrap();
            assert_eq!(original, other);
            assert!(!same_owner(&original, &other), "value equality never becomes owner authority");
            // Force the comparison trampoline itself to fail: a matching
            // BTreeMap key is NOT a success when the comparison debit failed.
            let mut calls = 0;
            let failure = lookup_global(&env.bindings, phase, original.namespace(), original.name(),
                &mut |_| { let call = calls; calls += 1;
                    if call == 1 { Err(error("no lookup budget")) } else { Ok(()) }
                }).unwrap_err();
            assert_eq!(failure.message, "no lookup budget");
            assert_eq!(calls, 2);

            // A singleton fixes the hashed lookup path: setup, keyed hash of
            // "one"/"hit" plus finalization, then phase, namespace and name of
            // the one probed candidate, then the single clone.
            let mut singleton = Environment::new("one").unwrap();
            singleton.clear_bindings();
            let owner = singleton.declare_cell(phase, "one", "hit").unwrap();
            let before = allocations();
            let mut charges = Vec::new();
            let selected = singleton.resolve_metered(phase, &Symbol::new("hit"), 11..19,
                &mut |work| { charges.push(work); Ok(()) }).unwrap();
            assert_eq!(charges, vec![4, 4, 12, 22, 4, 8, 8, 1]);
            assert_eq!(charges.iter().sum::<u64>(), 63);
            assert!(same_owner(&owner, selected.global()));
            assert_eq!(allocations(), before);
        }
    }

    #[test]
    fn borrowed_current_hits_preserve_core_precedence_exclusions_and_private_cell_errors() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut env = Environment::new("current.lookup").unwrap();
            let core = env.declare_cell(phase, "suss.core", "collision").unwrap();
            let current = env.declare_cell(phase, "current.lookup", "collision").unwrap();
            let selected = env.resolve_metered(phase, &Symbol::new("collision"), 23..31,
                &mut |_| Ok(())).unwrap();
            assert!(same_owner(&current, selected.global()), "current beats core before exclusion");
            drop(selected);
            env.exclude_core(phase, "collision").unwrap();
            let fallback = env.declare_cell(phase, "suss.core", "fallback").unwrap();
            env.declare_cell(phase, "suss.core", "excluded").unwrap();
            env.exclude_core(phase, "excluded").unwrap();
            for (symbol, owner) in [
                (Symbol::new("collision"), &current),
                (Symbol::namespaced("cljs.core", "collision"), &core),
                (Symbol::new("fallback"), &fallback),
            ] {
                let selected = env.resolve_metered(phase, &symbol, 23..31, &mut |_| Ok(())).unwrap();
                assert!(same_owner(owner, selected.global()));
            }
            for name in ["excluded", "missing"] {
                let symbol = Symbol::new(name);
                let mut expected_charges = vec![4, 4];
                for namespace in ["current.lookup", if name == "excluded" { "current.lookup" } else { "suss.core" }] {
                    assert!(lookup_global(&env.bindings, phase, namespace, name,
                        &mut |work| { expected_charges.push(work); Ok(()) }).unwrap().is_none());
                }
                let mut charges = Vec::new();
                let failure = env.resolve_metered(phase, &symbol, 23..31,
                    &mut |work| { charges.push(work); Ok(()) }).unwrap_err();
                assert_eq!(failure.message, format!("Unresolved {phase:?} name {symbol}"));
                assert_eq!(failure.span, 23..31);
                assert_eq!(charges, expected_charges, "misses keep both searches and never clone");
            }

            for reader in [false, true] {
                // Deliberately collide a hidden current binding with a public
                // core binding. Visibility filtering must not retry in core.
                let name = if reader { "hidden-reader" } else { "hidden-internal" };
                env.declare_cell(phase, "suss.core", name).unwrap();
                let hidden = env.declare_cell(phase, "current.lookup", name).unwrap();
                env.set_binding(hidden.clone(), if reader {
                    Binding::ReaderCell(hidden.clone())
                } else { Binding::InternalCell(hidden.clone()) });
                let bindings = env.bindings.clone();
                let generation = env.source_generation();
                let before = allocations();
                let strong = Arc::strong_count(&hidden.0);
                let symbol = Symbol::new(name);
                let mut expected_charges = vec![4, 4];
                lookup_global(&env.bindings, phase, "current.lookup", name,
                    &mut |work| { expected_charges.push(work); Ok(()) }).unwrap().unwrap();
                let mut charges = Vec::new();
                let failure = env.resolve_metered(phase, &symbol, 23..31,
                    &mut |work| { charges.push(work); Ok(()) }).unwrap_err();
                assert_eq!(failure.message, format!("Unresolved {phase:?} name {symbol}"));
                assert_eq!(failure.span, 23..31);
                assert_eq!(charges, expected_charges, "one hidden hit, no core search or clone");
                for reject in 0..charges.len() {
                    let mut calls = 0;
                    let failure = env.resolve_metered(phase, &symbol, 23..31, &mut |_| {
                        let call = calls; calls += 1;
                        if call == reject { Err(error("hidden lookup cancellation")) } else { Ok(()) }
                    }).unwrap_err();
                    assert_eq!(failure.message, "hidden lookup cancellation");
                    assert_eq!(failure.span, 23..31);
                    assert_eq!(calls, reject + 1);
                }
                assert_eq!(env.bindings, bindings);
                assert_eq!(env.source_generation(), generation);
                assert_eq!(allocations(), before);
                assert_eq!(Arc::strong_count(&hidden.0), strong);
                assert!(env.resolve_metered(phase, &symbol, 23..31, &mut |_| Ok(())).is_err());
            }
        }
    }


    #[test]
    fn native_interface_catalog_uses_actual_binding_and_propagates_current_meter_failure() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let env = Environment::default();
            let legacy = super::super::compiler_facts::CompilerNamespaceCatalog::capture(&env, phase);
            let mut charges = Vec::new();
            let actual = super::super::compiler_facts::CompilerNamespaceCatalog::capture_metered(
                &env, phase, &mut |work| { charges.push(work); Ok(()) }).unwrap();
            assert_eq!(actual.native_interfaces, legacy.native_interfaces);
            assert_eq!(actual.current, legacy.current);
            assert_eq!(actual.namespaces.keys().collect::<Vec<_>>(), legacy.namespaces.keys().collect::<Vec<_>>());
            assert_eq!(actual.native_interfaces.len(), 1);
            for reject in 0..charges.len() {
                let mut calls = 0;
                let error = super::super::compiler_facts::CompilerNamespaceCatalog::capture_metered(
                    &env, phase, &mut |_| { let call = calls; calls += 1;
                        if call == reject { Err(error("catalog ownership interruption")) } else { Ok(()) }
                    }).unwrap_err();
                assert_eq!(error.message, "catalog ownership interruption");
                assert_eq!(calls, reject + 1);
            }
            let mut replaced = env.clone();
            replaced.declare_cell(phase, "js", "Object").unwrap();
            let actual = super::super::compiler_facts::CompilerNamespaceCatalog::capture_metered(
                &replaced, phase, &mut |_| Ok(())).unwrap();
            assert!(actual.native_interfaces.is_empty(), "same name cannot replace actual Core export proof");
        }
    }

}
