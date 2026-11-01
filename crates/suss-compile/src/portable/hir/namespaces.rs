//! Original Suss runtime namespace publication (#309). Each fragment publishes
//! its namespace's changed scope and new declarations to suss.core's registry,
//! and runs with *ns* bound to that namespace, as Clojure's load binds it.
use super::*;
use super::super::resolve;
use std::collections::{BTreeMap, BTreeSet};
use suss_reader::{Keyword, Symbol};

/// The resolution facts syntax quote reads, in a comparable form.
#[derive(PartialEq)]
struct Scope {
    aliases: BTreeMap<String, String>,
    macro_aliases: BTreeMap<String, String>,
    refers: BTreeMap<String, (String, String)>,
    macro_refers: BTreeMap<String, (String, String)>,
    excluded: BTreeSet<String>,
}
impl Scope {
    fn of(scope: resolve::NamespaceScope<'_>) -> Self {
        Self {
            aliases: scope.aliases.clone(),
            macro_aliases: scope.macro_aliases.clone(),
            refers: scope
                .refers
                .iter()
                .map(|(name, global)| (name.clone(), (global.namespace().to_owned(), global.name().to_owned())))
                .collect(),
            macro_refers: scope.macro_refers.clone(),
            excluded: scope.excluded_core.clone(),
        }
    }
}

fn synthetic(span: &Range<usize>, kind: Kind) -> Form {
    Form { span: span.clone(), metadata: Vec::new(), kind }
}
fn symbol(span: &Range<usize>, symbol: Symbol) -> Form {
    synthetic(span, Kind::Symbol(symbol))
}
/// Lists longer than this travel as bucketed name tables.
const SHORT_NAME_LIST: usize = 32;
/// Core's `name-table-buckets` and `name-table-bucket`: lookups search only
/// the bucket a name's length and first and last code units select.
const NAME_TABLE_BUCKETS: usize = 61;
fn name_table_bucket(name: &[u16]) -> usize {
    (31 * name.len() + usize::from(name[0]) + 7 * usize::from(name[name.len() - 1])) % NAME_TABLE_BUCKETS
}
/// A name list for core's namespace registry. A short list is one string of
/// names separated by spaces (symbol names contain no spaces). A long one is a
/// vector of " a b c " bucket strings, which core searches with the runtime's
/// string search instead of splitting; one string constant per bucket is far
/// smaller than a constant per symbol.
fn names(span: &Range<usize>, names: impl IntoIterator<Item = String>) -> Form {
    let mut names: Vec<Vec<u16>> = names
        .into_iter()
        .filter(|name| !name.is_empty())
        .map(|name| name.encode_utf16().collect())
        .collect();
    names.sort();
    names.dedup();
    if names.len() <= SHORT_NAME_LIST {
        return synthetic(span, Kind::String(names.join(&[32u16][..])));
    }
    let mut buckets = vec![vec![32u16]; NAME_TABLE_BUCKETS];
    for name in names {
        let bucket = &mut buckets[name_table_bucket(&name)];
        bucket.extend(name);
        bucket.push(32);
    }
    let buckets = buckets.into_iter().map(|units| synthetic(span, Kind::String(units))).collect();
    synthetic(span, Kind::Vector(buckets))
}
fn aliases(span: &Range<usize>, aliases: &BTreeMap<String, String>) -> Form {
    synthetic(
        span,
        Kind::Map(
            aliases
                .iter()
                .flat_map(|(alias, target)| [symbol(span, Symbol::new(alias)), symbol(span, Symbol::new(target))])
                .collect(),
        ),
    )
}
/// Refers keyed by name string, which core looks up more cheaply than a symbol.
fn refers(span: &Range<usize>, refers: &BTreeMap<String, (String, String)>) -> Form {
    synthetic(
        span,
        Kind::Map(
            refers
                .iter()
                .flat_map(|(local, (namespace, name))| {
                    [
                        synthetic(span, Kind::String(local.encode_utf16().collect())),
                        symbol(span, Symbol::namespaced(namespace, name)),
                    ]
                })
                .collect(),
        ),
    )
}
fn quoted(span: &Range<usize>, form: Form) -> Form {
    synthetic(span, Kind::List(vec![symbol(span, Symbol::new("quote")), form]))
}
fn entry(span: &Range<usize>, key: &str, value: Form) -> [Form; 2] {
    [synthetic(span, Kind::Keyword(Keyword::new(key))), value]
}

impl Analyzer<'_> {
    /// Wrap a fragment's body so that it publishes its namespace when needed
    /// and binds *ns*. `before` is the environment before the fragment's ns
    /// directive. The namespace object lives in an internal cell, so a
    /// fragment that publishes nothing only reads that cell. Sessions without
    /// core have no runtime namespaces and are unchanged.
    pub(super) fn namespace_frame(&mut self, before: &Environment, body: Hir) -> Result<Hir, Diagnostic> {
        let span = body.span.clone();
        let publish = Symbol::namespaced("suss.core", "publish-namespace!");
        if self.environment.resolve(self.phase, &publish, span.clone()).is_err() {
            return Ok(body);
        }
        let namespace = self.environment.current_namespace(self.phase).to_owned();
        let (cell, fresh) = self.environment.namespace_cell(self.phase, &namespace);
        let scope = Scope::of(self.environment.namespace_scope(self.phase));
        let changed = fresh
            || before
                .namespace_scope_for(self.phase, &namespace)
                .is_none_or(|previous| Scope::of(previous) != scope);
        let declared = self.environment.declared_names(self.phase, &namespace);
        let core = namespace == "suss.core";
        // A namespace's own declaration changes resolution only when the name
        // would otherwise resolve elsewhere, so other namespaces publish only
        // those; an ordinary def publishes nothing. Core publishes everything.
        let reader_names = super::super::syntax_quote::core_names::CORE_READER_NAMES;
        let relevant = |name: &String| {
            core || self.environment.declares(self.phase, "suss.core", name)
                || resolve::BOOTSTRAP_MACROS.contains(&name.as_str())
                || reader_names.binary_search(&name.as_str()).is_ok()
                || scope.macro_refers.contains_key(name)
        };
        // With a changed scope or a namespace new to the runtime, the list
        // replaces what the runtime has; otherwise it adds new names.
        let added: Vec<_> = if changed {
            declared.iter().filter(|name| relevant(name)).cloned().collect()
        } else {
            let previous = before.declared_names(self.phase, &namespace);
            declared.difference(&previous).filter(|name| relevant(name)).cloned().collect()
        };
        let read = |span: &Range<usize>| Hir {
            source: None,
            span: span.clone(),
            metadata: Vec::new(),
            ty: Type::Value,
            kind: Expression::Global(cell.clone()),
        };
        let defining = before.resolve(self.phase, &publish, span.clone()).is_err();
        let mut effects = Vec::new();
        if changed || !added.is_empty() {
            let scope_form = if changed {
                let mut entries = Vec::new();
                entries.extend(entry(&span, "aliases", aliases(&span, &scope.aliases)));
                entries.extend(entry(&span, "macro-aliases", aliases(&span, &scope.macro_aliases)));
                entries.extend(entry(&span, "refers", refers(&span, &scope.refers)));
                entries.extend(entry(&span, "macro-refers", refers(&span, &scope.macro_refers)));
                entries.extend(entry(&span, "excluded", names(&span, scope.excluded.iter().cloned())));
                if core {
                    // Syntax quote's fallbacks after ordinary resolution.
                    // Core's own declarations are checked first with the
                    // same answer, so these lists omit them.
                    let undeclared = |names: &[&str]| {
                        names
                            .iter()
                            .filter(|name| !declared.contains(**name))
                            .map(|name| (*name).to_owned())
                            .collect::<Vec<_>>()
                    };
                    entries.extend(entry(&span, "bootstrap-macros", names(&span, undeclared(resolve::BOOTSTRAP_MACROS))));
                    entries.extend(entry(
                        &span,
                        "reader-names",
                        names(&span, undeclared(super::super::syntax_quote::core_names::CORE_READER_NAMES)),
                    ));
                }
                quoted(&span, synthetic(&span, Kind::Map(entries)))
            } else {
                synthetic(&span, Kind::Nil)
            };
            let added = if added.is_empty() {
                synthetic(&span, Kind::Nil)
            } else {
                names(&span, added)
            };
            let name = quoted(&span, symbol(&span, Symbol::new(namespace)));
            let call = synthetic(
                &span,
                Kind::List(vec![symbol(&span, publish.clone()), name.clone(), scope_form, added]),
            );
            let call = if defining {
                // Core's own name lists are large and only syntax-quote
                // resolution reads them, so core defers building them until
                // its first resolution instead of paying at every startup.
                synthetic(
                    &span,
                    Kind::List(vec![
                        symbol(&span, Symbol::new("do")),
                        synthetic(
                            &span,
                            Kind::List(vec![
                                symbol(&span, Symbol::new("set!")),
                                symbol(&span, Symbol::namespaced("suss.core", "pending-publication")),
                                synthetic(
                                    &span,
                                    Kind::List(vec![
                                        symbol(&span, Symbol::new("fn")),
                                        synthetic(&span, Kind::Vector(Vec::new())),
                                        call,
                                    ]),
                                ),
                            ]),
                        ),
                        synthetic(
                            &span,
                            Kind::List(vec![symbol(&span, Symbol::namespaced("suss.core", "intern-namespace")), name]),
                        ),
                    ]),
                )
            } else {
                call
            };
            let call = self.form(&call)?;
            effects.push(Hir {
                source: None,
                span: span.clone(),
                metadata: Vec::new(),
                ty: Type::Value,
                kind: Expression::Definition {
                    global: cell.clone(),
                    name_metadata: Vec::new(),
                    name_span: span.clone(),
                    docstring: None,
                    initializer: Some(Box::new(call)),
                    once: false,
                    // An internal namespace cell, not a core definition.
                    linked: None,
                },
            });
        }
        if defining {
            // The fragment that defines the registry and *ns* (core's
            // bootstrap) publishes after its body and loads under *ns*'s root.
            let mut items = vec![body];
            items.extend(effects);
            return Ok(Hir { source: None, span, metadata: Vec::new(), ty: Type::Value, kind: Expression::Do(items) });
        }
        let target = symbol(&span, Symbol::namespaced("suss.core", "*ns*"));
        let global = self.assignment_target(&target)?;
        let body = self.exception_region(&target, vec![], body);
        effects.push(Hir {
            source: None,
            span: span.clone(),
            metadata: Vec::new(),
            ty: Type::Value,
            kind: Expression::DynamicScope { bindings: vec![(global, read(&span))], body: Box::new(body) },
        });
        Ok(Hir { source: None, span, metadata: Vec::new(), ty: Type::Value, kind: Expression::Do(effects) })
    }
}
