// Copyright (c) Nicola Mometto, Rich Hickey & contributors.
// The use and distribution terms for this software are covered by the
// Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
// which can be found in runtime/core-import/epl-v10.html in this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
// You must not remove this notice, or any other, from this software.
// SPDX-License-Identifier: EPL-1.0

//! Adaptation of pinned tools.reader syntax quote, reader.clj598–753.
//! Source SHA256: 614e857a4d92e22edbaf38e4e87c3f499e8703fb7f930c23e1df136535cc4597.
//! Pin: c4295f303100bbf5afac449242d30bca1126f1a1.
//! Namespace resolution uses the supplied phase catalog, not lexical locals.
//! Generated expression constructors have a separate bootstrap lowering;
//! quoted reader data retains its actual pinned constructor names and structure.
//! Private sequence coercion adapts core.cljs4403–4406. Full declaration SHA256:
//! 748c1e1a78ed4ddfac684f99f973501ceb9366e7fba1cb9074726d27759f8756.
use super::{
    Diagnostic,
    resolve::{Environment, Phase},
};
use std::{collections::BTreeMap, ops::Range};
use suss_reader::{
    Symbol,
    forms::{Form, Kind},
};
mod core_names;

/// Deterministic state to carry across successful reader inputs. Failed expansion
/// does not advance it. Each nested backquote has its own gensym map.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReaderState {
    next_gensym: u64,
}

impl ReaderState {
    /// Expand resolved reader forms, including templates inside quoted data.
    /// The caller must supply original reader spans, not synthetic macro spans.
    pub fn expand(
        &mut self,
        form: &Form,
        environment: &Environment,
        phase: Phase,
    ) -> Result<Form, Diagnostic> {
        self.expand_with_origin(form, environment, phase, None)
    }
    pub(crate) fn expand_with_origin(
        &mut self,
        form: &Form,
        environment: &Environment,
        phase: Phase,
        origin: Option<&super::SourceOrigin>,
    ) -> Result<Form, Diagnostic> {
        let mut remaining = super::DEFAULT_OPERATION_FUEL;
        self.expand_with_origin_metered(form, environment, phase, origin, &mut |work| {
            remaining = remaining.checked_sub(work).ok_or_else(|| error(form, "Syntax quote computation fuel exhausted"))?;
            Ok(())
        })
    }

    /// Expand reader forms while charging traversal, copied text, and source
    /// provenance work to the caller's operation budget.
    pub(crate) fn expand_with_origin_metered(
        &mut self,
        form: &Form,
        environment: &Environment,
        phase: Phase,
        origin: Option<&super::SourceOrigin>,
        meter: &mut impl FnMut(u64) -> Result<(), Diagnostic>,
    ) -> Result<Form, Diagnostic> {
        let mut staged = self.clone();
        let result = Pass {
            state: &mut staged,
            environment,
            phase,
            origin,
            meter,
        }
        .read(form, 0)?;
        *self = staged;
        Ok(result)
    }
}

fn error(form: &Form, message: &str) -> Diagnostic {
    Diagnostic {
        span: form.span.clone(),
        message: message.into(),
    }
}
fn data(span: &Range<usize>, kind: Kind) -> Form {
    Form {
        span: span.clone(),
        metadata: vec![],
        kind,
    }
}
fn symbol(span: &Range<usize>, spelling: &str) -> Form {
    data(span, Kind::Symbol(Symbol::parse(spelling)))
}
fn call(span: &Range<usize>, spelling: &str, args: Vec<Form>) -> Form {
    let mut items = vec![symbol(span, spelling)];
    items.extend(args);
    data(span, Kind::List(items))
}
fn prefix<'a>(form: &'a Form, name: &str, width: usize) -> Option<&'a Form> {
    let Kind::List(items) = &form.kind else {
        return None;
    };
    let [head, target] = items.as_slice() else {
        return None;
    };
    // Conditional selection retains the original reader operator token span.
    // A textual call with the same name has a longer span and is ordinary code.
    (head.span.start >= form.span.start
        && head.span.end <= target.span.start
        && head.span.len() == width
        && matches!(&head.kind, Kind::Symbol(s) if s.namespace.is_none() && s.name == name))
    .then_some(target)
}
fn unquote(form: &Form, splice: bool) -> Option<&Form> {
    prefix(
        form,
        if splice {
            "unquote-splicing"
        } else {
            "unquote"
        },
        if splice { 2 } else { 1 },
    )
    .or_else(|| {
        let Kind::List(items) = &form.kind else {
            return None;
        };
        let [head, target] = items.as_slice() else {
            return None;
        };
        matches!(&head.kind, Kind::Symbol(s) if s.namespace.as_deref() == Some("clojure.core")
                && s.name == if splice { "unquote-splicing" } else { "unquote" })
        .then_some(target)
    })
}

struct Pass<'a, M> {
    state: &'a mut ReaderState,
    environment: &'a Environment,
    phase: Phase,
    origin: Option<&'a super::SourceOrigin>,
    meter: &'a mut M,
}
impl<M: FnMut(u64) -> Result<(), Diagnostic>> Pass<'_, M> {
    fn check(&mut self, form: &Form) -> Result<(), Diagnostic> {
        let units = match &form.kind {
            Kind::String(units) => units.len(),
            Kind::Symbol(s) => s.name.len() + s.namespace.as_ref().map_or(0, String::len),
            Kind::Keyword(k) => k.name.len() + k.namespace.as_ref().map_or(0, String::len),
            _ => 0,
        };
        let cost = units
            .checked_add(1)
            .and_then(|cost| u64::try_from(cost).ok())
            .ok_or_else(|| error(form, "Syntax quote work count overflowed"))?;
        (self.meter)(cost)
    }
    fn read(&mut self, form: &Form, depth: usize) -> Result<Form, Diagnostic> {
        stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || self.read_inner(form, depth))
    }
    fn read_inner(&mut self, form: &Form, depth: usize) -> Result<Form, Diagnostic> {
        self.check(form)?;
        if let Some(target) = prefix(form, "syntax-quote", 1) {
            // Actual reader recursion expands nested templates before the outer
            // quote walks their generated constructors. Never share their map.
            // Quote all meaningful metadata, including original indexing fields.
            // Ordinary compiler forms outside the template remain unenriched.
            let located = self
                .origin
                .map(|origin| origin.macro_form_data_metered(target, &mut *self.meter))
                .transpose()?;
            let target = self.read(located.as_ref().unwrap_or(target), depth + 1)?;
            let mut result = self.quote(&target, &mut BTreeMap::new(), depth + 1)?;
            // A caret before backquote annotates the generated form, while a
            // caret after it belongs to the template's quoted value.
            let metadata = form
                .metadata
                .iter()
                .map(|f| self.read(f, depth + 1))
                .collect::<Result<Vec<_>, _>>()?;
            result.metadata.splice(0..0, metadata);
            result.span = form.span.clone();
            return Ok(result);
        }
        // Walk before cloning so every child and copied atom is charged.
        let mut result = data(&form.span, Kind::Nil);
        result.metadata = form
            .metadata
            .iter()
            .map(|f| self.read(f, depth + 1))
            .collect::<Result<_, _>>()?;
        match &form.kind {
            Kind::List(items) | Kind::Vector(items) | Kind::Map(items) | Kind::Set(items) => {
                let items = items
                    .iter()
                    .map(|f| self.read(f, depth + 1))
                    .collect::<Result<_, _>>()?;
                result.kind = match form.kind {
                    Kind::List(_) => Kind::List(items),
                    Kind::Vector(_) => Kind::Vector(items),
                    Kind::Map(_) => Kind::Map(items),
                    _ => Kind::Set(items),
                };
            }
            Kind::Conditional(_)
            | Kind::SplicingConditional(_)
            | Kind::Discard(_)
            | Kind::Prefix { .. } => {
                return Err(error(
                    form,
                    "Syntax quote requires resolved reader conditionals",
                ));
            }
            _ => result.kind = clone_kind_metered(form, &mut *self.meter)?,
        }
        Ok(result)
    }
    fn resolve(&mut self, symbol: &Symbol, form: &Form) -> Result<Symbol, Diagnostic> {
        if symbol.namespace.is_none() && symbol.name.contains('.') {
            return Ok(symbol.clone());
        }
        let scope = self.environment.namespace_scope(self.phase);
        if let Some(namespace) = &symbol.namespace {
            let namespace = scope
                .aliases
                .get(namespace)
                .or_else(|| scope.macro_aliases.get(namespace))
                .map_or(namespace.as_str(), String::as_str);
            return Ok(Symbol::namespaced(
                if matches!(namespace, "suss.core" | "clojure.core") {
                    "cljs.core"
                } else {
                    namespace
                },
                &symbol.name,
            ));
        }
        let mut meter_error = None;
        let resolved = self.environment.resolve_metered(self.phase, symbol, form.span.clone(), &mut |work| {
            (self.meter)(work).map_err(|error| { meter_error = Some(error.clone()); error })
        });
        if let Some(error) = meter_error { return Err(error); }
        if let Ok(binding) = resolved {
            let global = binding.global();
            return Ok(Symbol::namespaced(
                if global.namespace() == "suss.core" {
                    "cljs.core"
                } else {
                    global.namespace()
                },
                global.name(),
            ));
        }
        if let Some((namespace, name)) = scope.macro_refers.get(&symbol.name) {
            return Ok(Symbol::namespaced(
                if namespace == "suss.core" {
                    "cljs.core"
                } else {
                    namespace
                },
                name,
            ));
        }
        if self
            .environment
            .has_bootstrap_macro_metered(self.phase, symbol, &mut *self.meter)?
        {
            return Ok(Symbol::namespaced("cljs.core", &symbol.name));
        }
        // Reader resolution knows core declarations before they are loaded in
        // this bounded runtime. Ordinary HIR resolution still requires actual
        // bindings; this catalog never fabricates runtime support.
        if !scope.excluded_core.contains(&symbol.name)
            && core_names::CORE_READER_NAMES
                .binary_search(&symbol.name.as_str())
                .is_ok()
        {
            return Ok(Symbol::namespaced("cljs.core", &symbol.name));
        }
        Ok(Symbol::namespaced(scope.namespace, &symbol.name))
    }
    fn quote(
        &mut self,
        form: &Form,
        gensyms: &mut BTreeMap<String, Symbol>,
        depth: usize,
    ) -> Result<Form, Diagnostic> {
        stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
            self.quote_inner(form, gensyms, depth)
        })
    }
    fn quote_inner(
        &mut self,
        form: &Form,
        gensyms: &mut BTreeMap<String, Symbol>,
        depth: usize,
    ) -> Result<Form, Diagnostic> {
        self.check(form)?;
        let span = &form.span;
        let result = if let Some(target) = unquote(form, false) {
            super::clone_form_metered(target, &mut *self.meter)?
        } else if unquote(form, true).is_some() {
            return Err(error(
                form,
                "Unquote splicing requires a collection element",
            ));
        } else {
            match &form.kind {
                Kind::Symbol(s) => {
                    let s = if s.namespace.is_none() && special(&s.name) {
                        s.clone()
                    } else if s.namespace.is_none() && s.name.ends_with('#') {
                        if let Some(s) = gensyms.get(&s.name) {
                            s.clone()
                        } else {
                            let id = self.state.next_gensym.checked_add(1).ok_or_else(|| {
                                error(form, "Syntax quote gensym counter exhausted")
                            })?;
                            self.state.next_gensym = id;
                            // Partition IDs by Store phase to avoid collisions
                            // between independently initialized reader states.
                            let printed_id = id
                                .checked_mul(2)
                                .and_then(|id| {
                                    id.checked_add(if self.phase == Phase::Macro { 1 } else { 0 })
                                })
                                .ok_or_else(|| {
                                    error(form, "Syntax quote gensym counter exhausted")
                                })?;
                            let generated = Symbol::new(format!(
                                "{}__{printed_id}__auto__",
                                &s.name[..s.name.len() - 1]
                            ));
                            gensyms.insert(s.name.clone(), generated.clone());
                            generated
                        }
                    } else if s.namespace.is_none() && s.name.starts_with('.') {
                        s.clone()
                    } else {
                        self.resolve(s, form)?
                    };
                    call(span, "quote", vec![data(span, Kind::Symbol(s))])
                }
                Kind::List(items) if items.is_empty() => call(span, "clojure.core/list", vec![]),
                Kind::List(items) => self.collection(form, items, gensyms, depth + 1)?,
                Kind::Vector(items) => {
                    let sequence = self.collection(form, items, gensyms, depth + 1)?;
                    call(span, "clojure.core/vec", vec![sequence])
                }
                Kind::Map(items) | Kind::Set(items) => {
                    let constructor = if matches!(form.kind, Kind::Set(_)) {
                        "clojure.core/hash-set"
                    } else if items.len() >= 32 {
                        "clojure.core/hash-map"
                    } else {
                        "clojure.core/array-map"
                    };
                    let sequence = self.collection(form, items, gensyms, depth + 1)?;
                    call(
                        span,
                        "clojure.core/apply",
                        vec![symbol(span, constructor), sequence],
                    )
                }
                Kind::Nil
                | Kind::Bool(_)
                | Kind::Number(_)
                | Kind::String(_)
                | Kind::Keyword(_) => data(span, clone_kind_metered(form, &mut *self.meter)?),
                _ => return Err(error(form, "Unsupported unresolved syntax quote data")),
            }
        };
        charge_metadata_tree(form, &mut *self.meter)?;
        let metadata = super::hir::reader_metadata_pairs_metered(form, self.meter)?;
        let meaningful = metadata.chunks_exact(2).any(|pair| {
            !matches!(&pair[0].kind,
            Kind::Keyword(k) if k.namespace.is_none() && matches!(k.name.as_str(),
                "line" | "column" | "end-line" | "end-column" | "file" | "source"))
        });
        if meaningful {
            let metadata = data(span, Kind::Map(metadata));
            let metadata = self.quote(&metadata, gensyms, depth + 1)?;
            Ok(call(span, "clojure.core/with-meta", vec![result, metadata]))
        } else {
            Ok(result)
        }
    }
    fn collection(
        &mut self,
        form: &Form,
        items: &[Form],
        gensyms: &mut BTreeMap<String, Symbol>,
        depth: usize,
    ) -> Result<Form, Diagnostic> {
        let mut chunks = Vec::with_capacity(items.len());
        for item in items {
            let chunk = if let Some(target) = unquote(item, true) {
                super::clone_form_metered(target, &mut *self.meter)?
            } else {
                let item = if let Some(target) = unquote(item, false) {
                    super::clone_form_metered(target, &mut *self.meter)?
                } else {
                    self.quote(item, gensyms, depth + 1)?
                };
                call(&form.span, "clojure.core/list", vec![item])
            };
            chunks.push(chunk);
        }
        let concat = call(&form.span, "clojure.core/concat", chunks);
        let seq = call(&form.span, "clojure.core/seq", vec![concat]);
        Ok(call(&form.span, "clojure.core/sequence", vec![seq]))
    }
}

// tools.reader uses clojure.core/special-symbol?, not the analyzer's larger set.
fn special(name: &str) -> bool {
    matches!(
        name,
        "if" | "let*"
            | "letfn*"
            | "do"
            | "fn*"
            | "quote"
            | "var"
            | "def"
            | "loop*"
            | "recur"
            | "throw"
            | "try"
            | "catch"
            | "finally"
            | "new"
            | "set!"
            | "."
            | "monitor-enter"
            | "monitor-exit"
            | "&"
    )
}

pub(crate) fn has_template(form: &Form) -> Result<bool, Diagnostic> {
    has_template_metered(form, &mut |_| Ok(()))
}

pub(crate) fn has_template_metered(
    form: &Form,
    meter: &mut impl FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<bool, Diagnostic> {
    let mut pending = vec![form];
    while let Some(form) = pending.pop() {
        charge_form(form, meter)?;
        if prefix(form, "syntax-quote", 1).is_some() {
            return Ok(true);
        }
        pending.extend(&form.metadata);
        match &form.kind {
            Kind::List(items) | Kind::Vector(items) | Kind::Map(items) | Kind::Set(items) => {
                pending.extend(items)
            }
            _ => (),
        }
    }
    Ok(false)
}

/// Lower only executable constructors after reader expansion. Literal quote
/// payloads remain exactly the reader's data. No public sequence binding is
/// introduced: its transducer arities remain unimplemented dependencies.
pub(crate) fn lower_generated(form: &Form) -> Result<Form, Diagnostic> {
    lower_generated_metered(form, &mut |_| Ok(()))
}

pub(crate) fn lower_generated_metered(
    form: &Form,
    meter: &mut impl FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<Form, Diagnostic> {
    fn lower<M: FnMut(u64) -> Result<(), Diagnostic>>(
        form: &Form,
        meter: &mut M,
    ) -> Result<Form, Diagnostic> {
        stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || lower_inner(form, meter))
    }
    fn lower_inner<M: FnMut(u64) -> Result<(), Diagnostic>>(
        form: &Form,
        meter: &mut M,
    ) -> Result<Form, Diagnostic> {
        charge_form(form, meter)?;
        if let Kind::List(items) = &form.kind {
            if matches!(items.first().map(|f| &f.kind), Some(Kind::Symbol(s))
                if s.name == "quote" && (s.namespace.is_none() || matches!(s.namespace.as_deref(), Some("cljs.core" | "clojure.core" | "suss.core"))))
            {
                let mut result = super::clone_form_metered(form, meter)?;
                if let Kind::List(items) = &mut result.kind {
                    if let Some(Form {
                        kind: Kind::Symbol(s),
                        ..
                    }) = items.first_mut()
                    {
                        if s.namespace.as_deref() == Some("clojure.core") {
                            s.namespace = Some("cljs.core".into());
                        }
                    }
                }
                return Ok(result);
            }
        }

        let mut result = data(&form.span, Kind::Nil);
        result.metadata = form
            .metadata
            .iter()
            .map(|item| super::clone_form_metered(item, meter))
            .collect::<Result<_, _>>()?;
        result.kind = match &form.kind {
            Kind::List(items) | Kind::Vector(items) | Kind::Map(items) | Kind::Set(items) => {
                let items = items
                    .iter()
                    .map(|f| lower(f, meter))
                    .collect::<Result<_, _>>()?;
                match form.kind {
                    Kind::List(_) => Kind::List(items),
                    Kind::Vector(_) => Kind::Vector(items),
                    Kind::Map(_) => Kind::Map(items),
                    _ => Kind::Set(items),
                }
            }
            Kind::Symbol(s) if s.namespace.as_deref() == Some("clojure.core") => {
                Kind::Symbol(Symbol::namespaced("cljs.core", &s.name))
            }
            _ => clone_kind_metered(form, meter)?,
        };
        Ok(result)
    }
    lower(form, meter)
}

fn charge_form<M: FnMut(u64) -> Result<(), Diagnostic>>(
    form: &Form,
    meter: &mut M,
) -> Result<(), Diagnostic> {
    let units = match &form.kind {
        Kind::String(units) => units.len(),
        Kind::Symbol(s) => s.name.len() + s.namespace.as_ref().map_or(0, String::len),
        Kind::Keyword(k) => k.name.len() + k.namespace.as_ref().map_or(0, String::len),
        _ => 0,
    };
    let cost = units
        .checked_add(1)
        .and_then(|cost| u64::try_from(cost).ok())
        .ok_or_else(|| error(form, "Syntax quote work count overflowed"))?;
    meter(cost)
}

fn clone_kind_metered<M: FnMut(u64) -> Result<(), Diagnostic>>(
    form: &Form,
    meter: &mut M,
) -> Result<Kind, Diagnostic> {
    let mut cloned = super::clone_form_metered(form, meter)?;
    cloned.metadata.clear();
    Ok(cloned.take_kind())
}

fn charge_metadata_tree<M: FnMut(u64) -> Result<(), Diagnostic>>(
    form: &Form,
    meter: &mut M,
) -> Result<(), Diagnostic> {
    let mut pending = form.metadata.iter().collect::<Vec<_>>();
    while let Some(item) = pending.pop() {
        charge_form(item, meter)?;
        pending.extend(&item.metadata);
        if let Kind::List(items) | Kind::Vector(items) | Kind::Map(items) | Kind::Set(items) =
            &item.kind
        {
            pending.extend(items);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

/// Select the private coercion only after provisional definitions and macro
/// expansion are visible. The caller selects an initialized public cell or the
/// private default without marking the public source binding initialized.
pub(crate) fn reader_sequence_initializer(
    form: &Form,
    _environment: &Environment,
    _phase: Phase,
    origin: Option<&super::SourceOrigin>,
    meter: &mut impl FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<Option<Form>, Diagnostic> {
    let Kind::List(items) = &form.kind else {
        return Ok(None);
    };
    let [head, _argument] = items.as_slice() else {
        return Ok(None);
    };
    let Kind::Symbol(symbol) = &head.kind else {
        return Ok(None);
    };
    // Lowered reader output names cljs.core; reader data that passed through a
    // macro (for example a defn body) still carries the reader's clojure.core.
    if !matches!(
        symbol.namespace.as_deref(),
        Some("cljs.core" | "clojure.core")
    ) || symbol.name != "sequence"
        || head.span.start < form.span.start
        || head.span.end != form.span.end
    {
        return Ok(None);
    }
    let Some(origin) = origin else {
        return Ok(None);
    };
    let reader = origin.macro_form_data_metered(form, meter)?;
    if !matches!(&reader.kind, Kind::List(items) if matches!(items.first().map(|f| &f.kind),
        Some(Kind::Symbol(s)) if s.namespace.as_deref() == Some("clojure.core") && s.name == "sequence"))
    {
        return Ok(None);
    }
    // Exact one-argument sequence body, core.cljs4403–4406, qualified to avoid
    // caller-local shadowing. No public sequence binding is manufactured.
    const INITIALIZER: &str = "(fn* [reader_coll] (if (cljs.core/seq? reader_coll) reader_coll (cljs.core/or (cljs.core/seq reader_coll) ())))";
    let mut reader_meter = |work| meter(work).map_err(|failure| suss_reader::ParseError {
        span: failure.span, message: failure.message, expected: vec![],
    });
    let mut function = suss_reader::forms::read_forms_metered(INITIALIZER, &mut reader_meter)
        .and_then(|forms| suss_reader::forms::resolve_conditionals_metered(forms, &mut reader_meter))
        .map_err(|failure| Diagnostic { span: form.span.clone(), message: failure.message })?
        .remove(0);
    fn relocate(form: &mut Form, span: &Range<usize>) {
        form.span = span.clone();
        match &mut form.kind {
            Kind::List(items) | Kind::Vector(items) => {
                for item in items {
                    relocate(item, span);
                }
            }
            _ => (),
        }
    }
    relocate(&mut function, &form.span);
    meter(form.metadata.len() as u64)?;
    let metadata = form.metadata.iter().map(|item| super::clone_form_metered(item, meter))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(Form {
        span: form.span.clone(),
        metadata,
        kind: function.take_kind(),
    }))
}
