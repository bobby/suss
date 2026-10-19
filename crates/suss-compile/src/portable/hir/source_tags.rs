// Copyright (c) Rich Hickey. All rights reserved.
// The use and distribution terms for this software are covered by the
// Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
// which can be found in the file epl-v10.html at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
// You must not remove this notice, or any other, from this software.
// SPDX-License-Identifier: EPL-1.0
//
// Source inference adaptation of cljs/analyzer.cljc get-tag, infer-if,
// infer-invoke, infer-tag and fn inferred-ret-tag, and canonicalize-type, pinned
// at c4295f303100bbf5afac449242d30bca1126f1a1 (1529–1659, 1008–1022, 2359–2364,
// 2630–2696, 3614–3649 and 4444–4465).
// Upstream analyzer.cljc SHA-256:
// 297802c627474434f1ef868e31f5f9913c290a4e80c509a40c704dced95bbf47.
// The native operation representation is original. No JS AST is fabricated and
// no inference rule consults the physical HIR storage Type.
use super::*;

pub(crate) type WorkMeter<'a> = dyn FnMut(u64) -> Result<(), Diagnostic> + 'a;

fn charge(meter: &mut WorkMeter<'_>, amount: usize, span: &Range<usize>) -> Result<(), Diagnostic> {
    meter(u64::try_from(amount).unwrap_or(u64::MAX)).map_err(|mut error| {
        error.span = span.clone();
        error
    })
}

fn with_default_budget<T>(
    span: Range<usize>,
    f: impl FnOnce(&mut WorkMeter<'_>) -> Result<T, Diagnostic>,
) -> Result<T, Diagnostic> {
    let mut remaining = crate::portable::DEFAULT_OPERATION_FUEL;
    let mut meter = |work: u64| {
        remaining = remaining.checked_sub(work).ok_or_else(|| Diagnostic {
            span: span.clone(),
            message: "Source tag computation fuel exhausted".into(),
        })?;
        Ok(())
    };
    f(&mut meter)
}

/// Local inference can reuse this record's actual tag without another Form.
/// Explicit metadata remains independently owned; no equality/interning is used.
#[derive(Debug, Clone, Default)]
enum InferredTag {
    #[default]
    None,
    FromTag,
    Explicit(Form),
}

#[derive(Debug, Clone, Default)]
pub struct SourceTags {
    /// Absence is distinct from a known portable tag (including any).
    pub tag: Option<Form>,
    /// Genuine quoted const child: outer None means unsupported/not a quote;
    /// inner None retains the analyzer's present nil child tag.
    pub quoted_const_tag: Option<Option<Form>>,
    /// None: field absent. Some(None): field present, unknown return tag.
    pub inferred_return: Option<Option<Form>>,
    /// Supported source inference result, after metadata's get-tag precedence.
    inferred: InferredTag,
}
impl SourceTags {
    pub fn inferred_ref(&self) -> Option<&Form> {
        match &self.inferred {
            InferredTag::None => None,
            InferredTag::FromTag => self.tag.as_ref(),
            InferredTag::Explicit(form) => Some(form),
        }
    }
}

fn named(name: &str) -> Form {
    let symbol = name.split_once('/').map_or_else(
        || suss_reader::Symbol::new(name),
        |(namespace, name)| suss_reader::Symbol::namespaced(namespace, name),
    );
    Form {
        span: 0..0,
        metadata: vec![],
        kind: Kind::Symbol(symbol),
    }
}
fn is_named(form: &Form, name: &str) -> bool {
    form.kind == named(name).kind
}
// Encode the existing source-tag equality policy iteratively. The root and
// ordinary collection children ignore spans/metadata; reader-prefix children
// retain derived Form equality. Sets are unordered only in the former mode.
fn same(a: Option<&Form>, b: Option<&Form>, meter: &mut WorkMeter<'_>) -> Result<bool, Diagnostic> {
    #[derive(Clone, Copy)]
    enum Mode {
        Bare,
        Derived,
    }
    enum Task<'a> {
        Visit(&'a Form, Mode),
        Finish(&'a Form, Mode, usize),
    }
    fn encode(form: &Form, meter: &mut WorkMeter<'_>) -> Result<Option<Vec<u8>>, Diagnostic> {
        let mut pending = vec![Task::Visit(form, Mode::Bare)];
        let mut done: Vec<Vec<u8>> = Vec::new();
        while let Some(task) = pending.pop() {
            match task {
                Task::Visit(node, mode) => {
                    charge(meter, 1, &node.span)?;
                    let child_count = match &node.kind {
                        Kind::List(xs)
                        | Kind::Vector(xs)
                        | Kind::Map(xs)
                        | Kind::Set(xs)
                        | Kind::Conditional(xs)
                        | Kind::SplicingConditional(xs) => xs.len(),
                        Kind::Discard(_) => 1,
                        Kind::Prefix { .. } => 2,
                        _ => 0,
                    };
                    let meta_count = usize::from(matches!(mode, Mode::Derived))
                        .saturating_mul(node.metadata.len());
                    charge(meter, child_count.saturating_add(meta_count), &node.span)?;
                    let children: Vec<(&Form, Mode)> = match (&node.kind, mode) {
                        (Kind::List(xs) | Kind::Vector(xs) | Kind::Map(xs), Mode::Bare) => {
                            xs.iter().map(|x| (x, Mode::Bare)).collect()
                        }
                        (Kind::Set(xs), Mode::Bare) => xs.iter().map(|x| (x, Mode::Bare)).collect(),
                        (Kind::Conditional(xs) | Kind::SplicingConditional(xs), Mode::Bare) => {
                            xs.iter().map(|x| (x, Mode::Derived)).collect()
                        }
                        (Kind::Discard(x), Mode::Bare) => vec![(x, Mode::Derived)],
                        (Kind::Prefix { operator, target }, Mode::Bare) => {
                            vec![(operator, Mode::Derived), (target, Mode::Derived)]
                        }
                        (
                            Kind::List(xs)
                            | Kind::Vector(xs)
                            | Kind::Set(xs)
                            | Kind::Map(xs)
                            | Kind::Conditional(xs)
                            | Kind::SplicingConditional(xs),
                            Mode::Derived,
                        ) => xs.iter().map(|x| (x, Mode::Derived)).collect(),
                        (Kind::Discard(x), Mode::Derived) => vec![(x, Mode::Derived)],
                        (Kind::Prefix { operator, target }, Mode::Derived) => {
                            vec![(operator, Mode::Derived), (target, Mode::Derived)]
                        }
                        _ => vec![],
                    };
                    pending.push(Task::Finish(node, mode, children.len() + meta_count));
                    pending.extend(children.iter().rev().map(|(x, mode)| Task::Visit(x, *mode)));
                    if matches!(mode, Mode::Derived) {
                        pending.extend(
                            node.metadata
                                .iter()
                                .rev()
                                .map(|x| Task::Visit(x, Mode::Derived)),
                        );
                    }
                }
                Task::Finish(node, mode, child_count) => {
                    let child_start = done
                        .len()
                        .checked_sub(child_count)
                        .expect("comparison children");
                    let mut parts = done.split_off(child_start);
                    if matches!(&node.kind, Kind::Set(_)) && matches!(mode, Mode::Bare) {
                        let comparisons = parts.len().saturating_mul(
                            usize::BITS as usize - parts.len().leading_zeros() as usize,
                        );
                        let bytes = parts
                            .iter()
                            .fold(0usize, |sum, part| sum.saturating_add(part.len()));
                        charge(meter, bytes.saturating_mul(comparisons), &node.span)?;
                        parts.sort_unstable();
                    }
                    let own = match &node.kind {
                        Kind::Nil | Kind::Discard(_) | Kind::Prefix { .. } => 1,
                        Kind::Bool(_) => 2,
                        Kind::Number(_) => 9,
                        Kind::String(v) => 9usize.saturating_add(v.len().saturating_mul(2)),
                        Kind::Symbol(v) => identifier_size(v.namespace.as_deref(), &v.name),
                        Kind::Keyword(v) => identifier_size(v.namespace.as_deref(), &v.name),
                        Kind::List(_)
                        | Kind::Vector(_)
                        | Kind::Map(_)
                        | Kind::Set(_)
                        | Kind::Conditional(_)
                        | Kind::SplicingConditional(_) => 9,
                    };
                    let framing = parts.iter().fold(0usize, |sum, part| {
                        sum.saturating_add(8).saturating_add(part.len())
                    });
                    let derived = if matches!(mode, Mode::Derived) { 24 } else { 0 };
                    charge(
                        meter,
                        own.saturating_add(framing).saturating_add(derived),
                        &node.span,
                    )?;
                    let mut out = Vec::new();
                    if matches!(mode, Mode::Derived) {
                        out.extend_from_slice(&(node.span.start as u64).to_le_bytes());
                        out.extend_from_slice(&(node.span.end as u64).to_le_bytes());
                        out.extend_from_slice(&(node.metadata.len() as u64).to_le_bytes());
                        let metadata_count = node.metadata.len();
                        let metadata = parts.drain(..metadata_count).collect::<Vec<_>>();
                        append_parts(&mut out, metadata, meter, &node.span)?;
                    }
                    match &node.kind {
                        Kind::Nil => out.push(0),
                        Kind::Bool(v) => out.extend([1, u8::from(*v)]),
                        Kind::Number(v) => {
                            if v.is_nan() {
                                return Ok(None);
                            }
                            out.push(2);
                            out.extend_from_slice(
                                &(if *v == 0.0 { 0 } else { v.to_bits() }).to_le_bytes(),
                            );
                        }
                        Kind::String(v) => {
                            out.push(3);
                            out.extend_from_slice(&(v.len() as u64).to_le_bytes());
                            for chunk in v.chunks(4096) {
                                charge(meter, 0, &node.span)?;
                                for x in chunk {
                                    out.extend_from_slice(&x.to_le_bytes());
                                }
                            }
                        }
                        Kind::Symbol(v) => {
                            out.push(4);
                            append_identifier(&mut out, v.namespace.as_deref(), &v.name, meter, &node.span)?;
                        }
                        Kind::Keyword(v) => {
                            out.push(5);
                            append_identifier(&mut out, v.namespace.as_deref(), &v.name, meter, &node.span)?;
                        }
                        Kind::List(v) => {
                            out.push(6);
                            out.extend_from_slice(&(v.len() as u64).to_le_bytes());
                        }
                        Kind::Vector(v) => {
                            out.push(7);
                            out.extend_from_slice(&(v.len() as u64).to_le_bytes());
                        }
                        Kind::Map(v) => {
                            out.push(8);
                            out.extend_from_slice(&(v.len() as u64).to_le_bytes());
                        }
                        Kind::Set(v) => {
                            out.push(9);
                            out.extend_from_slice(&(v.len() as u64).to_le_bytes());
                        }
                        Kind::Conditional(v) => {
                            out.push(10);
                            out.extend_from_slice(&(v.len() as u64).to_le_bytes());
                        }
                        Kind::SplicingConditional(v) => {
                            out.push(11);
                            out.extend_from_slice(&(v.len() as u64).to_le_bytes());
                        }
                        Kind::Discard(_) => out.push(12),
                        Kind::Prefix { .. } => out.push(13),
                    }
                    append_parts(&mut out, parts, meter, &node.span)?;
                    done.push(out);
                }
            }
        }
        Ok(done.pop())
    }
    // Bytes have already been charged before allocation/copy. Polling with
    // zero work keeps cancellation responsive without charging them twice.
    fn append_bytes(out: &mut Vec<u8>, bytes: &[u8], meter: &mut WorkMeter<'_>, span: &Range<usize>) -> Result<(), Diagnostic> {
        for chunk in bytes.chunks(4096) {
            charge(meter, 0, span)?;
            out.extend_from_slice(chunk);
        }
        Ok(())
    }
    fn append_parts(out: &mut Vec<u8>, parts: Vec<Vec<u8>>, meter: &mut WorkMeter<'_>, span: &Range<usize>) -> Result<(), Diagnostic> {
        for part in parts {
            out.extend_from_slice(&(part.len() as u64).to_le_bytes());
            append_bytes(out, &part, meter, span)?;
        }
        Ok(())
    }
    fn append_identifier(out: &mut Vec<u8>, namespace: Option<&str>, name: &str, meter: &mut WorkMeter<'_>, span: &Range<usize>) -> Result<(), Diagnostic> {
        out.push(u8::from(namespace.is_some()));
        if let Some(namespace) = namespace {
            out.extend_from_slice(&(namespace.len() as u64).to_le_bytes());
            append_bytes(out, namespace.as_bytes(), meter, span)?;
        }
        out.extend_from_slice(&(name.len() as u64).to_le_bytes());
        append_bytes(out, name.as_bytes(), meter, span)
    }
    fn identifier_size(namespace: Option<&str>, name: &str) -> usize {
        2usize
            .saturating_add(8)
            .saturating_add(name.len())
            .saturating_add(namespace.map_or(0, |ns| 8usize.saturating_add(ns.len())))
    }
    match (a, b) {
        (None, None) => Ok(true),
        (Some(a), Some(b)) => Ok(match (encode(a, meter)?, encode(b, meter)?) {
            (Some(left), Some(right)) => {
                charge(meter, left.len().min(right.len()).saturating_add(1), &a.span)?;
                if left.len() != right.len() {
                    false
                } else {
                    let mut equal = true;
                    for (left, right) in left.chunks(4096).zip(right.chunks(4096)) {
                        charge(meter, 0, &a.span)?;
                        if left != right {
                            equal = false;
                            break;
                        }
                    }
                    equal
                }
            }
            _ => false,
        }),
        _ => Ok(false),
    }
}
fn metadata(
    form: &Form,
    name: &str,
    meter: &mut WorkMeter<'_>,
) -> Result<Option<Form>, Diagnostic> {
    let pairs = super::quotes::reader_metadata_view_metered(form, meter)?;
    for pair in pairs {
        charge(meter, 1 + name.len(), &form.span)?;
        if matches!(&pair.key.as_ref().kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == name)
        {
            return Ok(Some(super::super::clone_form_metered(pair.value.as_ref(), meter)?));
        }
    }
    Ok(None)
}
fn hint(form: &Form, meter: &mut WorkMeter<'_>) -> Result<Option<Form>, Diagnostic> {
    Ok(metadata(form, "tag", meter)?.filter(|tag| !matches!(tag.kind, Kind::Nil)))
}
fn constant(form: &Form) -> Option<Form> {
    Some(named(match &form.kind {
        Kind::Nil => "clj-nil",
        Kind::Bool(_) => "boolean",
        Kind::Number(_) => "number",
        Kind::String(_) => "string",
        Kind::Keyword(_) => "cljs.core/Keyword",
        Kind::Symbol(_) => "cljs.core/Symbol",
        Kind::Vector(_) => "cljs.core/IVector",
        Kind::Map(_) => "cljs.core/IMap",
        Kind::Set(_) => "cljs.core/ISet",
        Kind::List(_) => "cljs.core/IList",
        _ => return None,
    }))
}
// analyze-const reads the tag after quoted analysis, including with-meta.
// Reader/analyzer bookkeeping alone does not create that wrapper. A meaningful
// metadata wrapper has no inferred tag unless get-tag supplies a non-nil hint.
fn quoted_constant_tag(
    form: &Form,
    meter: &mut WorkMeter<'_>,
) -> Result<Option<Option<Form>>, Diagnostic> {
    let Some(tag) = constant(form) else {
        return Ok(None);
    };
    let pairs = reader_metadata_pairs_metered(form, meter)?;
    let mut meaningful = false;
    for pair in pairs.chunks_exact(2) {
        charge(meter, 1, &form.span)?;
        if !matches!(&pair[0].kind, Kind::Keyword(key)
            if (key.namespace.is_none()
                && matches!(key.name.as_str(), "file" | "line" | "column" | "end-column" | "end-line" | "source"))
            || (key.namespace.as_deref() == Some("cljs.analyzer") && key.name == "analyzed"))
        {
            meaningful = true;
            break;
        }
    }
    Ok(Some(if meaningful {
        hint(form, meter)?
    } else {
        Some(tag)
    }))
}
fn callable(hir: &Hir) -> Option<&SourceCallable> {
    hir.source.as_ref()?.callable.as_deref()
}
/// prepare_source_callee may put the actual analyzed callee in a private
/// binding to perform IFn dispatch once. Inspect that retained initializer,
/// without attributing source facts to the compiler-only wrapper itself.
fn callable_before_dispatch(hir: &Hir) -> Option<&SourceCallable> {
    if let Some(methods) = callable(hir) {
        return Some(methods);
    }
    if hir.source.is_none() {
        if let Expression::Let { bindings, body } = &hir.kind {
            if bindings.len() == 1 {
                if let Expression::If {
                    condition,
                    alternative,
                    ..
                } = &body.kind
                {
                    if matches!(
                        condition.kind,
                        Expression::Nominal {
                            operation: Nominal::IsClosure,
                            ..
                        }
                    ) && matches!(
                        alternative.kind,
                        Expression::Nominal {
                            operation: Nominal::BindCallable,
                            ..
                        }
                    ) {
                        return callable(&bindings[0].value);
                    }
                }
            }
        }
    }
    None
}
fn truthy_constant(form: &Form) -> bool {
    match &form.kind {
        Kind::Number(_) | Kind::String(_) | Kind::Keyword(_) | Kind::Bool(true) => true,
        Kind::List(items) if items.is_empty() => true,
        Kind::List(items)
            if items.len() == 2
                && matches!(&items[0].kind, Kind::Symbol(head) if head.namespace.is_none() && head.name == "quote") =>
        {
            !matches!(items[1].kind, Kind::Nil | Kind::Bool(false))
        }
        _ => false,
    }
}
pub fn local_tag(binding: &LocalBinding) -> Result<Option<Form>, Diagnostic> {
    with_default_budget(binding.declaration.span.clone(), |meter: &mut WorkMeter<'_>| {
        local_tag_metered(binding, meter)
    })
}

pub(crate) fn local_tag_metered(
    binding: &LocalBinding,
    meter: &mut WorkMeter<'_>,
) -> Result<Option<Form>, Diagnostic> {
    if let Some(tag) = hint(&binding.declaration, meter)? {
        return Ok(Some(tag));
    }
    binding
        .initializer
        .as_ref()
        .map_or(Ok(None), |init| inferred(init, meter))
}
fn truthy(form: &Form) -> bool {
    !matches!(form.kind, Kind::Nil | Kind::Bool(false))
}
// Each calculation retains one normalized view of this exact immutable
// declaration. Initialization is lazy so existing short-circuit paths keep
// their first metadata validation point; nothing survives the calculation.
struct DeclarationMetadata<'a> {
    form: &'a Form,
    view: std::cell::OnceCell<Vec<super::quotes::MetadataEntry<'a>>>,
}
impl<'a> DeclarationMetadata<'a> {
    fn new(form: &'a Form) -> Self { Self { form, view: std::cell::OnceCell::new() } }
    fn get(&self, name: &str, meter: &mut WorkMeter<'_>) -> Result<Option<&Form>, Diagnostic> {
        if self.view.get().is_none() {
            let view = super::quotes::reader_metadata_view_metered(self.form, meter)?;
            // This local view is inaccessible to meter callbacks; no other
            // path can initialize it while normalization is in progress.
            if self.view.set(view).is_err() { unreachable!("single declaration calculation") }
        }
        for pair in self.view.get().expect("initialized declaration metadata") {
            charge(meter, 1 + name.len(), &self.form.span)?;
            if matches!(&pair.key.as_ref().kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == name) {
                return Ok(Some(pair.value.as_ref()));
            }
        }
        Ok(None)
    }
}
fn owned_metadata_value(value: Option<&Form>, meter: &mut WorkMeter<'_>) -> Result<Option<Form>, Diagnostic> {
    value.map(|value| crate::portable::clone_form_metered(value, meter)).transpose()
}
fn provisional(
    info: &super::super::resolve::DefinitionInfo,
    metadata: &DeclarationMetadata<'_>,
    meter: &mut WorkMeter<'_>,
) -> Result<bool, Diagnostic> {
    Ok(!info.analysis_completed
        || (info.initializer_form.is_some()
            && metadata.get("declared", meter)?.is_some_and(truthy)))
}
fn completed_callable<'a>(
    info: &'a super::super::resolve::DefinitionInfo,
    metadata: &DeclarationMetadata<'_>,
    meter: &mut WorkMeter<'_>,
) -> Result<Option<&'a SourceCallable>, Diagnostic> {
    Ok(if provisional(info, metadata, meter)? {
        None
    } else {
        info.initializer.as_ref().and_then(|init| callable(init))
    })
}
// Pinned analyzer.cljc2092–2175 merges raw metadata before callable top-fn
// overlays and computed return fields. Original Rust policy; never treat a
// provisional initializer as a published callable declaration.
fn top_function_property<'a>(
    metadata: &'a DeclarationMetadata<'_>,
    name: &str,
    meter: &mut WorkMeter<'_>,
) -> Result<Option<&'a Form>, Diagnostic> {
    let Some(top) = metadata.get("top-fn", meter)? else {
        return Ok(None);
    };
    let pairs = match &top.kind {
        Kind::Nil => return Ok(None),
        Kind::Map(items) => items,
        Kind::Vector(items) if items.len() == 2 => items,
        Kind::List(items) | Kind::Set(items) if items.is_empty() => return Ok(None),
        Kind::String(units) if units.is_empty() => return Ok(None),
        _ => {
            return Err(fail(
                top.span.clone(),
                "Function declaration top-fn requires mergeable reader data",
            ));
        }
    };
    for pair in pairs.chunks_exact(2).rev() {
        charge(meter, name.len() + 1, &top.span)?;
        if matches!(&pair[0].kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == name)
        {
            return Ok(Some(&pair[1]));
        }
    }
    Ok(None)
}
pub fn declaration_tag(
    info: &super::super::resolve::DefinitionInfo,
) -> Result<Option<Form>, Diagnostic> {
    with_default_budget(info.declaration.span.clone(), |meter: &mut WorkMeter<'_>| {
        declaration_tag_metered(info, meter)
    })
}

pub(crate) fn declaration_tag_metered(
    info: &super::super::resolve::DefinitionInfo,
    meter: &mut WorkMeter<'_>,
) -> Result<Option<Form>, Diagnostic> {
    let metadata = DeclarationMetadata::new(&info.declaration);
    let raw = metadata.get("tag", meter)?;
    // parse-type merges source metadata over its default function tag,
    // including explicit nil/false. Type declarations precede method analysis.
    if info.type_fields.is_some() {
        return match raw { Some(_) => owned_metadata_value(raw, meter), None => Ok(Some(named("function"))) };
    }
    if provisional(info, &metadata, meter)? {
        return owned_metadata_value(raw, meter);
    }
    // Function vars retain their raw/overlaid tag, including present nil and
    // false. Dynamic affects scalar inference, not function var-reference tags.
    if completed_callable(info, &metadata, meter)?.is_some() {
        return owned_metadata_value(top_function_property(&metadata, "tag", meter)?.or(raw), meter);
    }
    if raw.is_some_and(truthy) {
        return owned_metadata_value(raw, meter);
    }
    if metadata.get("dynamic", meter)?.is_some_and(truthy) {
        return Ok(Some(named("any")));
    }
    let computed = info
        .initializer
        .as_ref()
        .map(|init| inferred(init, meter))
        .transpose()?
        .flatten();
    match computed { Some(tag) => Ok(Some(tag)), None => owned_metadata_value(raw, meter) }
}
fn declaration_return_tag(
    info: &super::super::resolve::DefinitionInfo,
    meter: &mut WorkMeter<'_>,
) -> Result<Option<Form>, Diagnostic> {
    let metadata = DeclarationMetadata::new(&info.declaration);
    let methods = completed_callable(info, &metadata, meter)?;
    let fn_var = if methods.is_some() {
        // Keep map_or's eager :macro default query even when top-fn supplies
        // fn-var. These lookups can still validate or exhaust the operation.
        top_function_property(&metadata, "fn-var", meter)?
            .map_or(!metadata.get("macro", meter)?.is_some_and(truthy), truthy)
    } else {
        metadata.get("fn-var", meter)?.is_some_and(truthy)
            || (!provisional(info, &metadata, meter)?
                && metadata.get("declared", meter)?.is_some_and(truthy)
                && metadata.get("arglists", meter)?.is_some_and(truthy))
    };
    if !fn_var {
        return Ok(None);
    }
    let raw = metadata.get("ret-tag", meter)?;
    let tag = if let Some(methods) = methods {
        let hint = metadata.get("tag", meter)?.filter(|tag| truthy(tag));
        // Preserve both original eager .or(...) evaluations and their order.
        let computed = return_tag(methods, meter)?;
        let overlay = top_function_property(&metadata, "ret-tag", meter)?;
        if hint.is_some() { owned_metadata_value(hint, meter)? }
        else if computed.is_some() { computed }
        else { owned_metadata_value(overlay.or(raw), meter)? }
    } else {
        owned_metadata_value(raw, meter)?
    };
    // infer-invoke uses if-some, preserving false but falling through nil.
    Ok(tag.filter(|tag| !matches!(tag.kind, Kind::Nil)))
}
fn resolved_tag(
    binding: Option<&SourceBinding>,
    meter: &mut WorkMeter<'_>,
) -> Result<Option<Form>, Diagnostic> {
    match binding {
        Some(SourceBinding::JavaScript { .. }) => Ok(Some(named("js"))),
        Some(SourceBinding::Local(binding)) => local_tag_metered(binding, meter),
        Some(SourceBinding::Field(binding)) => hint(&binding.declaration, meter),
        Some(SourceBinding::Global {
            declaration: Some(info),
            ..
        }) => declaration_tag_metered(info, meter),
        _ => Ok(None),
    }
}
fn resolved_callable(binding: Option<&SourceBinding>) -> Option<&SourceCallable> {
    match binding {
        Some(SourceBinding::Local(binding)) => callable(binding.initializer.as_ref()?),
        _ => None,
    }
}
fn return_tag(
    methods: &SourceCallable,
    meter: &mut WorkMeter<'_>,
) -> Result<Option<Form>, Diagnostic> {
    charge(
        meter,
        methods.methods.len(),
        &methods
            .methods
            .first()
            .map_or(0..0, |method| method.body.span.clone()),
    )?;
    let mut result = None;
    for (index, method) in methods.methods.iter().enumerate() {
        let tag = inferred(&method.body, meter)?;
        if index == 0 {
            result = tag;
        } else if !same(result.as_ref(), tag.as_ref(), meter)? {
            return Ok(None);
        }
    }
    Ok(result)
}
fn union(
    then: Option<Form>,
    otherwise: Option<Form>,
    meter: &mut WorkMeter<'_>,
) -> Result<Option<Form>, Diagnostic> {
    if same(then.as_ref(), otherwise.as_ref(), meter)?
        || otherwise
            .as_ref()
            .is_some_and(|tag| is_named(tag, "ignore"))
    {
        return Ok(then);
    }
    if then.as_ref().is_some_and(|tag| is_named(tag, "ignore")) {
        return Ok(otherwise);
    }
    if [&then, &otherwise].iter().all(|tag| {
        tag.as_ref()
            .is_some_and(|tag| is_named(tag, "clj") || is_named(tag, "not-native"))
    }) {
        return Ok(Some(named("clj")));
    }
    if [&then, &otherwise].iter().all(|tag| {
        tag.as_ref()
            .is_some_and(|tag| is_named(tag, "boolean") || is_named(tag, "seq"))
    }) {
        return Ok(Some(named("seq")));
    }
    let mut items: Vec<Form> = Vec::new();
    for branch in [then, otherwise] {
        let branch = branch.unwrap_or(Form {
            span: 0..0,
            metadata: vec![],
            kind: Kind::Nil,
        });
        let values: &[Form] = match &branch.kind {
            Kind::Set(values) => values.as_slice(),
            _ => std::slice::from_ref(&branch),
        };
        for value in values {
            let mut duplicate = false;
            for previous in &items {
                if same(Some(previous), Some(value), meter)? {
                    duplicate = true;
                    break;
                }
            }
            if !duplicate {
                items.push(crate::portable::clone_form_metered(value, meter)?);
            }
        }
    }
    charge(meter, items.len(), &(0..0))?;
    if items.iter().any(|tag| is_named(tag, "any")) {
        return Ok(Some(named("any")));
    }
    if items.iter().any(|tag| is_named(tag, "seq")) {
        charge(meter, items.len(), &(0..0))?;
        items.retain(|tag| !is_named(tag, "clj-nil"));
    }
    if items.len() == 1 {
        Ok(items.pop())
    } else {
        Ok(Some(Form {
            span: 0..0,
            metadata: vec![],
            kind: Kind::Set(items),
        }))
    }
}
pub(crate) fn inferred(hir: &Hir, meter: &mut WorkMeter<'_>) -> Result<Option<Form>, Diagnostic> {
    let mut current = hir;
    loop {
        charge(meter, 1, &current.span)?;
        if let Some(source) = &current.source {
            return source
                .tags
                .inferred_ref()
                .map(|tag| crate::portable::clone_form_metered(tag, meter))
                .transpose();
        }
        match &current.kind {
            Expression::Literal(value) => {
                return Ok(Some(named(match value {
                    Literal::Nil | Literal::Undefined => "clj-nil",
                    Literal::Bool(_) => "boolean",
                    Literal::Number(_) => "number",
                    Literal::String(_) => "string",
                })));
            }
            Expression::Do(items) => match items.last() {
                Some(item) => current = item,
                None => return Ok(Some(named("clj-nil"))),
            },
            Expression::Let { body, .. }
            | Expression::Loop { body, .. }
            | Expression::DynamicScope { body, .. }
            | Expression::AsyncDynamicScope { body, .. }
            | Expression::AsyncTry { body, .. }
            | Expression::Function { body, .. } => current = body,
            _ => return Ok(None),
        };
    }
}
pub(super) fn source_tags(
    form: &Form,
    resolved: Option<&SourceBinding>,
    methods: Option<&SourceCallable>,
    hir: &Hir,
) -> Result<SourceTags, Diagnostic> {
    with_default_budget(form.span.clone(), |meter: &mut WorkMeter<'_>| {
        source_tags_metered(form, resolved, methods, hir, meter)
    })
}

pub(super) fn source_tags_metered(
    form: &Form,
    resolved: Option<&SourceBinding>,
    methods: Option<&SourceCallable>,
    hir: &Hir,
    meter: &mut WorkMeter<'_>,
) -> Result<SourceTags, Diagnostic> {
    charge(meter, 1, &form.span)?;
    let quoted_const_tag = if let Kind::List(items) = &form.kind
        && items.len() == 2
        && matches!(&items[0].kind, Kind::Symbol(head)
            if head.namespace.is_none() && head.name == "quote")
    {
        quoted_constant_tag(&items[1], meter)?
    } else {
        None
    };
    let inferred_return = methods
        .map(|methods| return_tag(methods, meter))
        .transpose()?;
    let tag = if methods.is_some() {
        Some(named("function"))
    } else {
        match &form.kind {
            Kind::Nil
            | Kind::Bool(_)
            | Kind::Number(_)
            | Kind::String(_)
            | Kind::Keyword(_)
            | Kind::Vector(_)
            | Kind::Map(_)
            | Kind::Set(_) => constant(form),
            Kind::Symbol(_) => resolved_tag(resolved, meter)?,
            Kind::List(items) if items.is_empty() => constant(form),
            Kind::List(items)
                if matches!(items.first().map(|head| &head.kind), Some(Kind::Symbol(head)) if head.namespace.is_none() && head.name == "quote")
                    && items.len() == 2 =>
            {
                match quoted_const_tag.as_ref() {
                    Some(Some(tag)) => Some(crate::portable::clone_form_metered(tag, meter)?),
                    Some(None) => Some(named("any")),
                    None => None,
                }
            }
            _ => match &hir.kind {
                Expression::Arithmetic { .. } | Expression::Bitwise { .. } => Some(named("number")),
                Expression::Comparison { .. } | Expression::NilTest(_) => Some(named("boolean")),
                Expression::Throw(_) | Expression::Recur { .. } => Some(named("ignore")),
                Expression::Do(_)
                | Expression::Let { .. }
                | Expression::Loop { .. }
                | Expression::DynamicScope { .. }
                | Expression::AsyncDynamicScope { .. }
                | Expression::AsyncTry { .. } => inferred(hir, meter)?,
                Expression::Try { regions } => match regions.first() {
                    Some(region) => inferred(region, meter)?,
                    None => None,
                },
                Expression::If {
                    condition,
                    consequent,
                    alternative,
                } => {
                    let truthy_constant = condition
                        .source
                        .as_ref()
                        .is_some_and(|source| truthy_constant(&source.form));
                    let then = inferred(consequent, meter)?;
                    if truthy_constant {
                        then
                    } else {
                        union(then, inferred(alternative, meter)?, meter)?
                    }
                }
                Expression::Definition { initializer, .. } => initializer
                    .as_ref()
                    .map_or(Ok(None), |init| inferred(init, meter))?,
                Expression::Assign { .. } => None,
                Expression::Call { callee, arguments } => {
                    charge(meter, 1, &callee.span)?;
                    let methods =
                        callable_before_dispatch(callee).or_else(|| resolved_callable(resolved));
                    if let Some(methods) = methods {
                        charge(meter, methods.methods.len(), &callee.span)?;
                    }
                    let method = methods.and_then(|methods| {
                        methods.methods.iter().find(|method| {
                            method.variadic || method.parameters.len() == arguments.len()
                        })
                    });
                    let method_tag = method
                        .map(|method| inferred(&method.body, meter))
                        .transpose()?
                        .flatten();
                    // Global vars retain an aggregate return tag, not their
                    // initializer's individual methods (analyzer def parse).
                    // Local/direct functions retain per-method information.
                    let global_tag = match resolved {
                        Some(SourceBinding::JavaScript { .. }) => Some(named("js")),
                        Some(SourceBinding::Global {
                            declaration: Some(info),
                            ..
                        }) => declaration_return_tag(info, meter)?,
                        _ => None,
                    };
                    method_tag.or(global_tag).or(Some(named("any")))
                }
                Expression::Nominal {
                    operation: Nominal::Construct,
                    arguments,
                } => {
                    // parse-new derives the result from source resolution,
                    // not the physical runtime constructor/descriptor value.
                    // Resolution on an invocation belongs to its callee, not
                    // the value of the constructor expression. Only reference
                    // ASTs carry the :info read by pinned parse-new.
                    let class = arguments
                        .first()
                        .and_then(|class| class.source.as_ref())
                        .filter(|class| matches!(class.form.kind, Kind::Symbol(_)));
                    let class_tag = class
                        .map(|class| resolved_tag(class.resolved.as_ref(), meter))
                        .transpose()?
                        .flatten();
                    let name = match class.and_then(|class| class.resolved.as_ref()) {
                        Some(SourceBinding::JavaScript { global })
                        | Some(SourceBinding::Global { global, .. }) => {
                            charge(
                                meter,
                                global.namespace().len().saturating_add(global.name().len()),
                                &form.span,
                            )?;
                            Some(named(&format!("{}/{}", global.namespace(), global.name())))
                        }
                        Some(SourceBinding::Local(binding)) => Some(
                            crate::portable::clone_form_metered(&binding.declaration, meter)?,
                        ),
                        Some(SourceBinding::Field(binding)) => Some(
                            crate::portable::clone_form_metered(&binding.declaration, meter)?,
                        ),
                        _ => None,
                    };
                    let primitive = |tag: &Form| {
                        if let Kind::Symbol(symbol) = &tag.kind
                            && symbol.namespace.as_deref() == Some("js")
                        {
                            match symbol.name.as_str() {
                                "Object" => Some("object"),
                                "String" => Some("string"),
                                "Array" => Some("array"),
                                "Number" => Some("number"),
                                "Function" => Some("function"),
                                "Boolean" => Some("boolean"),
                                _ => None,
                            }
                        } else {
                            None
                        }
                    };
                    let js_tag = class_tag.as_ref().is_some_and(|tag| {
                        matches!(&tag.kind,
                        Kind::Symbol(symbol) if symbol.namespace.as_deref() == Some("js")
                            || (symbol.namespace.is_none() && symbol.name == "js"))
                    });
                    if js_tag && class_tag.as_ref().and_then(primitive).is_none() {
                        Some(named("js"))
                    } else {
                        Some(
                            name.and_then(|name| primitive(&name).map(named).or(Some(name)))
                                .unwrap_or(Form {
                                    kind: Kind::Nil,
                                    span: 0..0,
                                    metadata: vec![],
                                }),
                        )
                    }
                }
                _ => None,
            },
        }
    };
    // get-tag/infer-tag use if-some. A present nil AST tag remains present
    // in the source view, but does not become an inferred return type.
    let inferred = match hint(form, meter)? {
        Some(hint) => InferredTag::Explicit(hint),
        None if tag.as_ref().is_some_and(|tag| !matches!(tag.kind, Kind::Nil)) => InferredTag::FromTag,
        None => InferredTag::None,
    };
    Ok(SourceTags {
        tag,
        quoted_const_tag,
        inferred_return,
        inferred,
    })
}

#[cfg(test)]
mod quoted_tag_tests {
    use super::*;

    fn hir(kind: Expression) -> Hir {
        Hir {
            source: None,
            span: 0..1,
            metadata: vec![],
            ty: Type::Value,
            kind,
        }
    }

    fn callable_declaration_fixture() -> super::super::super::resolve::DefinitionInfo {
        let source = "(def subject (fn* [x] x))";
        let forms = suss_reader::forms::read_forms(source).unwrap();
        let (_, environment) = super::super::prepare(&forms, 0..source.len(), &Environment::default(), Phase::Runtime).unwrap();
        let binding = environment.resolve(Phase::Runtime, &suss_reader::Symbol::new("subject"), 0..0).unwrap();
        environment.definition_info(binding.global()).unwrap().clone()
    }

    #[test]
    fn declaration_metadata_reuses_one_view_and_retains_each_query_charge() {
        let mut form = suss_reader::forms::read_forms("^string ^:private ^{:declared false :dynamic false :macro false :arglists [x] :ret-tag number} subject").unwrap().remove(0);
        form.span = 0..150;
        let names = ["declared", "declared", "dynamic", "macro", "arglists", "ret-tag", "tag", "top-fn"];
        let shared = DeclarationMetadata::new(&form);
        let mut shared_work = 0;
        let mut separate_work = 0;
        for name in names {
            let result = shared.get(name, &mut |work| { shared_work += work; Ok(()) }).unwrap();
            let separate = DeclarationMetadata::new(&form);
            let expected = separate.get(name, &mut |work| { separate_work += work; Ok(()) }).unwrap();
            assert_eq!(result, expected);
        }
        assert!(shared_work < separate_work, "actual repeated normalization/allocation is eliminated");
        let entries = shared.view.get().unwrap();
        let expected_query_work = entries.iter().take_while(|entry| !matches!(&entry.key.as_ref().kind,
            Kind::Keyword(key) if key.namespace.is_none() && key.name == "tag")).count() + 1;
        let owner = entries.as_ptr();
        let mut repeat_work = 0;
        let value = shared.get("tag", &mut |work| { repeat_work += work; Ok(()) }).unwrap().unwrap();
        assert_eq!(repeat_work, expected_query_work as u64 * 4);
        assert_eq!(shared.view.get().unwrap().as_ptr(), owner);
        assert!(matches!(&value.kind, Kind::Symbol(symbol) if symbol.name == "string"));
    }

    #[test]
    fn declaration_query_reuse_preserves_lazy_validation_late_invalid_prefix_and_top_fn_errors() {
        let read = |text| suss_reader::forms::read_forms(text).unwrap().remove(0);
        let mut info = callable_declaration_fixture();
        info.declaration.metadata = vec![read("42"), read("{:tag number :fn-var true}")];
        info.analysis_completed = false;
        let view = DeclarationMetadata::new(&info.declaration);
        assert!(provisional(&info, &view, &mut |_| panic!("provisional short circuit must not normalize metadata")).unwrap());
        assert!(view.view.get().is_none());
        for return_tag in [false, true] {
            let error = if return_tag { declaration_return_tag(&info, &mut |_| Ok(())).unwrap_err() }
                else { declaration_tag_metered(&info, &mut |_| Ok(())).unwrap_err() };
            assert_eq!(error.message, "Invalid metadata: expected a map, keyword, symbol or string");
            assert_eq!(error.span, info.declaration.metadata[0].span);
        }
        info.analysis_completed = true;
        info.declaration.metadata = vec![read("{:top-fn 42 :tag number}")];
        for return_tag in [false, true] {
            let error = if return_tag { declaration_return_tag(&info, &mut |_| Ok(())).unwrap_err() }
                else { declaration_tag_metered(&info, &mut |_| Ok(())).unwrap_err() };
            assert_eq!(error.message, "Function declaration top-fn requires mergeable reader data");
            let Kind::Map(pairs) = &info.declaration.metadata[0].kind else { unreachable!() };
            assert_eq!(error.span, pairs[1].span);
        }
    }

    #[test]
    fn declaration_query_reuse_returns_fresh_overlaid_tags_and_keeps_failure_retry_and_presence() {
        let read = |text: &str| suss_reader::forms::read_forms(text).unwrap().remove(0);
        let mut info = callable_declaration_fixture();
        info.declaration.metadata = vec![read("{:tag raw :top-fn {:tag \"overlaid\" :fn-var false :ret-tag ignored} :macro true}")];
        let original = info.declaration.clone();
        let mut calls = 0;
        let selected = declaration_tag_metered(&info, &mut |_| { calls += 1; Ok(()) }).unwrap().unwrap();
        let view = DeclarationMetadata::new(&info.declaration);
        let expected = top_function_property(&view, "tag", &mut |_| Ok(())).unwrap().unwrap();
        assert_eq!(&selected, expected);
        let (Kind::String(copy), Kind::String(source)) = (&selected.kind, &expected.kind) else { unreachable!() };
        assert_ne!(copy.as_ptr(), source.as_ptr());
        assert!(declaration_return_tag(&info, &mut |_| Ok(())).unwrap().is_none(), "present false fn-var must survive");
        for stop in 0..calls {
            let mut current = 0;
            let error = declaration_tag_metered(&info, &mut |_| {
                let call = current; current += 1;
                if call == stop { Err(fail(0..0, "declaration query cancelled")) } else { Ok(()) }
            }).unwrap_err();
            assert_eq!(error.message, "declaration query cancelled");
            assert_eq!(info.declaration, original);
            assert_eq!(declaration_tag_metered(&info, &mut |_| Ok(())).unwrap().unwrap(), selected);
        }
        // Presence is distinct from truthiness for provisional/type tags.
        for literal in ["nil", "false"] {
            let mut info = callable_declaration_fixture();
            info.analysis_completed = false;
            info.declaration.metadata = vec![read(&format!("{{:tag {literal}}}"))];
            let Kind::Map(pairs) = &info.declaration.metadata[0].kind else { unreachable!() };
            assert_eq!(declaration_tag_metered(&info, &mut |_| Ok(())).unwrap().as_ref(), Some(&pairs[1]));
            info.type_fields = Some(0);
            assert_eq!(declaration_tag_metered(&info, &mut |_| Ok(())).unwrap().as_ref(), Some(&pairs[1]));
        }
    }

    #[test]
    fn metadata_lookup_clones_only_selected_winner_and_returns_fresh_owned_value() {
        let read = |text| suss_reader::forms::read_forms(text).unwrap().remove(0);
        let build = |size| {
            let mut form = read("value");
            form.metadata = vec![Form { span: 2..30, metadata: vec![], kind: Kind::Map(vec![
                read(":tag"), read("\"selected\""), read(":unrelated"),
                Form { span: 9..20, metadata: vec![read(":nested")], kind: Kind::String(vec![65; size]) },
            ]) }];
            form
        };
        let mut totals = Vec::new();
        for size in [32, 32768] {
            let form = build(size);
            let mut work = 0;
            let mut selected = metadata(&form, "tag", &mut |units| { work += units; Ok(()) }).unwrap().unwrap();
            let Kind::Map(pairs) = &form.metadata[0].kind else { unreachable!() };
            let (Kind::String(copy), Kind::String(original)) = (&selected.kind, &pairs[1].kind) else { unreachable!() };
            assert_eq!(copy, original);
            assert_ne!(copy.as_ptr(), original.as_ptr());
            if let Kind::String(value) = &mut selected.kind { value[0] = 66; }
            assert_eq!(pairs[1], read("\"selected\""));
            totals.push(work);
        }
        assert_eq!(totals[0], totals[1], "unselected value bytes are never copied or traversed as keys");
    }

    #[test]
    fn metadata_lookup_validates_all_prefixes_and_arbitrary_keys_before_selecting() {
        let read = |text| suss_reader::forms::read_forms(text).unwrap().remove(0);
        let mut form = read("value");
        // Reversed prefix processing first sees a perfectly valid :tag. The
        // later invalid prefix must still reject lookup of that existing tag.
        form.metadata = vec![read("42"), read("{:tag number}")];
        let error = metadata(&form, "tag", &mut |_| Ok(())).unwrap_err();
        let owned_error = reader_metadata_pairs_metered(&form, &mut |_| Ok(())).unwrap_err();
        assert_eq!(error.message, "Invalid metadata: expected a map, keyword, symbol or string");
        assert_eq!(error.message, owned_error.message);
        assert_eq!(error.span, owned_error.span);
        let malformed_key = Form { span: 51..60, metadata: vec![], kind: Kind::Map(vec![read(":unpaired")]) };
        form.metadata = vec![Form { span: 1..80, metadata: vec![], kind: Kind::Map(vec![
            read(":tag"), read("number"), malformed_key.clone(), read("1"), malformed_key, read("2"),
        ]) }];
        let error = metadata(&form, "tag", &mut |_| Ok(())).unwrap_err();
        let owned_error = reader_metadata_pairs_metered(&form, &mut |_| Ok(())).unwrap_err();
        assert_eq!(error.message, "Metadata key map requires paired entries");
        assert_eq!(error.message, owned_error.message);
        assert_eq!(error.span, owned_error.span);
    }

    #[test]
    fn metadata_lookup_completes_structural_dedup_and_retries_every_fallible_boundary() {
        let read = |text| suss_reader::forms::read_forms(text).unwrap().remove(0);
        let mut form = read("value");
        // List/vector reader-data equality and canonical duplicate map/set
        // contents must be normalized even when looking up a different key.
        form.metadata = vec![read("{:tag [winning {:nested true}] [1 2] 17 {:a #{1}} 23}"),
            read("{:tag discarded (1 2) 19 {:a #{1 1}} 29}")];
        let original = form.clone();
        let mut calls = 0;
        let selected = metadata(&form, "tag", &mut |_| { calls += 1; Ok(()) }).unwrap().unwrap();
        // Compare against the actual winning source node, including every
        // nested span; separately reading its text starts locations at zero.
        let Kind::Map(outer_pairs) = &form.metadata[0].kind else { unreachable!() };
        assert_eq!(selected, outer_pairs[1]);
        assert_eq!(super::super::quotes::reader_metadata_view_metered(&form, &mut |_| Ok(())).unwrap().len(), 3);
        for stop in 0..calls {
            let mut current = 0;
            let error = metadata(&form, "tag", &mut |_| {
                let call = current; current += 1;
                if call == stop { Err(Diagnostic { span: 5..8, message: "metadata lookup cancelled".into() }) } else { Ok(()) }
            }).unwrap_err();
            assert_eq!(error.message, "metadata lookup cancelled");
            assert_eq!(form, original);
            assert_eq!(metadata(&form, "tag", &mut |_| Ok(())).unwrap().unwrap(), selected);
        }
    }

    #[test]
    fn inferred_relation_reuses_tag_and_preserves_explicit_nil_and_false_rules() {
        let form = suss_reader::forms::read_forms("42").unwrap().remove(0);
        let literal = hir(Expression::Literal(Literal::Number(42.0)));
        let tags = source_tags_metered(&form, None, None, &literal, &mut |_| Ok(())).unwrap();
        assert!(std::ptr::eq(tags.tag.as_ref().unwrap(), tags.inferred_ref().unwrap()));
        assert!(is_named(tags.inferred_ref().unwrap(), "number"));
        let copied = tags.clone();
        assert!(std::ptr::eq(copied.tag.as_ref().unwrap(), copied.inferred_ref().unwrap()));
        let hinted = hir(Expression::Do(vec![hir(Expression::Literal(Literal::Number(42.0)))]));
        for (source, expected) in [("^string (do 42)", named("string")),
            ("^{:tag false} (do 42)", Form { kind: Kind::Bool(false), span: 0..0, metadata: vec![] })]
        {
            let form = suss_reader::forms::read_forms(source).unwrap().remove(0);
            let tags = source_tags_metered(&form, None, None, &hinted, &mut |_| Ok(())).unwrap();
            assert!(matches!(tags.inferred, InferredTag::Explicit(_)));
            assert!(!std::ptr::eq(tags.tag.as_ref().unwrap(), tags.inferred_ref().unwrap()));
            assert!(same(tags.inferred_ref(), Some(&expected), &mut |_| Ok(())).unwrap());
            assert!(is_named(tags.tag.as_ref().unwrap(), "number"));
        }
        for phase in [Phase::Runtime, Phase::Macro] {
            for (source, nil) in [("(def ^{:tag nil} f (fn* [x] x)) f", true),
                ("(def ^{:tag false} f (fn* [x] x)) f", false)]
            {
                let analyzed = crate::portable::analyze_in(source, &Environment::default(), phase).unwrap();
                let Expression::Do(items) = &analyzed.kind else { unreachable!() };
                let tags = &items.last().unwrap().source.as_ref().unwrap().tags;
                assert!(matches!(tags.tag.as_ref().unwrap().kind, Kind::Nil) == nil);
                if nil { assert!(tags.inferred_ref().is_none()); }
                else {
                    assert!(matches!(tags.inferred_ref().unwrap().kind, Kind::Bool(false)));
                    assert!(std::ptr::eq(tags.tag.as_ref().unwrap(), tags.inferred_ref().unwrap()));
                }
            }
        }
    }

    #[test]
    fn inferred_relation_keeps_required_hint_and_parent_copy_meter_errors() {
        let form = suss_reader::forms::read_forms("^string (do 42)").unwrap().remove(0);
        let expected = form.clone();
        let literal = hir(Expression::Do(vec![hir(Expression::Literal(Literal::Number(42.0)))]));
        let mut count = 0;
        source_tags_metered(&form, None, None, &literal, &mut |_| { count += 1; Ok(()) }).unwrap();
        for stop in 0..count {
            let mut call = 0;
            let error = source_tags_metered(&form, None, None, &literal, &mut |_| {
                let current = call; call += 1;
                if current == stop { Err(fail(0..0, "tag interrupted")) } else { Ok(()) }
            }).unwrap_err();
            assert_eq!(error.message, "tag interrupted");
            assert_eq!(form, expected);
        }
        // Reading a child's retained inferred tag still makes a real owned
        // propagation copy and must not bypass the existing meter.
        let analyzed = crate::portable::analyze("42").unwrap();
        let Expression::Do(items) = &analyzed.kind else { unreachable!() };
        let error = inferred(&items[0], &mut |_| Err(fail(0..0, "parent interrupted"))).unwrap_err();
        assert_eq!(error.message, "parent interrupted");
        assert!(is_named(&inferred(&items[0], &mut |_| Ok(())).unwrap().unwrap(), "number"));
    }

    #[test]
    fn inference_walks_deep_finite_hir_and_meter_failure_is_recoverable() {
        let mut deep = hir(Expression::Literal(Literal::Number(1.0)));
        for _ in 0..512 {
            deep = hir(Expression::Do(vec![deep]));
        }

        let mut charged = 0u64;
        let mut meter = |work: u64| {
            charged += work;
            Ok(())
        };
        let tag = inferred(&deep, &mut meter).unwrap().unwrap();
        assert!(is_named(&tag, "number"));
        assert!(charged >= 513);

        let mut remaining = 32u64;
        let mut exhausted = |work: u64| {
            remaining = remaining
                .checked_sub(work)
                .ok_or_else(|| fail(0..1, "budget exhausted"))?;
            Ok(())
        };
        let error = inferred(&deep, &mut exhausted).unwrap_err();
        assert_eq!(error.message, "budget exhausted");

        let mut recovered = 0u64;
        let mut meter_again = |work: u64| {
            recovered += work;
            Ok(())
        };
        assert!(is_named(
            &inferred(&deep, &mut meter_again).unwrap().unwrap(),
            "number"
        ));
        assert_eq!(recovered, charged);
    }

    #[test]
    fn large_tag_encoding_polls_cancellation_and_next_comparison_recovers() {
        let value = Form {
            span: 42..45,
            metadata: vec![],
            kind: Kind::String(vec![97; 16_384]),
        };
        let mut cancel_at_poll = |work| {
            if work == 0 {
                Err(Diagnostic { span: 0..0, message: "cancelled".into() })
            } else {
                Ok(())
            }
        };
        let error = same(Some(&value), Some(&value), &mut cancel_at_poll).unwrap_err();
        assert_eq!(error.message, "cancelled");
        assert_eq!(error.span, 42..45);
        assert!(same(Some(&value), Some(&value), &mut |_| Ok(())).unwrap());
    }

    #[test]
    fn tag_equality_keeps_numeric_and_unordered_set_policy_iteratively() {
        let number = |value| Form {
            span: 0..1,
            metadata: vec![],
            kind: Kind::Number(value),
        };
        let left = Form {
            span: 0..1,
            metadata: vec![],
            kind: Kind::Set(vec![number(0.0), number(2.0)]),
        };
        let right = Form {
            span: 9..10,
            metadata: vec![],
            kind: Kind::Set(vec![number(2.0), number(-0.0)]),
        };
        let nan = Form {
            span: 0..1,
            metadata: vec![],
            kind: Kind::Number(f64::NAN),
        };
        let mut meter = |_| Ok(());
        assert!(same(Some(&left), Some(&right), &mut meter).unwrap());
        assert!(!same(Some(&nan), Some(&nan), &mut meter).unwrap());
        // Pinned ClojureScript confirms unordered set equality with signed
        // zero here. Rust debug-string sorting could pair the wrong elements.
        let left = Form {
            span: 0..1,
            metadata: vec![],
            kind: Kind::Set(vec![number(-0.0), number(-0.1)]),
        };
        let right = Form {
            span: 9..10,
            metadata: vec![],
            kind: Kind::Set(vec![number(0.0), number(-0.1)]),
        };
        assert!(same(Some(&left), Some(&right), &mut meter).unwrap());
    }

    #[test]
    fn scalar_quote_preserves_outer_source_instead_of_private_literal_source() {
        for phase in [Phase::Runtime, Phase::Macro] {
            for source in [
                "(quote nil)",
                "(quote false)",
                "(quote 42)",
                "(quote \"hello\")",
            ] {
                let hir =
                    crate::portable::analyze_in(source, &Environment::default(), phase).unwrap();
                let Expression::Do(forms) = &hir.kind else {
                    panic!("fragment source forms")
                };
                assert_eq!(forms.len(), 1);
                assert!(matches!(&forms[0].kind, Expression::Literal(_)));
                let retained = forms[0]
                    .source
                    .as_ref()
                    .expect("actual quoted source record");
                let expected = suss_reader::forms::read_forms(source).unwrap().remove(0);
                assert_eq!(
                    retained.form, expected,
                    "enclosing source quote in {phase:?}"
                );
                assert!(retained.tags.quoted_const_tag.is_some());
            }
        }
    }

    #[test]
    fn quoted_child_tag_retains_nil_false_hints_and_elides_only_bookkeeping() {
        for (source, expected) in [
            ("word", Some(named("cljs.core/Symbol"))),
            ("^{:purpose :kept} word", None),
            ("^{:tag nil} word", None),
            (
                "^{:tag false} word",
                Some(Form {
                    kind: Kind::Bool(false),
                    span: 0..0,
                    metadata: vec![],
                }),
            ),
            ("^number word", Some(named("number"))),
            (
                "^{:line 1 :column 2 :file \"probe.sus\" :end-column 3 :end-line 1 :source \"word\" :cljs.analyzer/analyzed true} word",
                Some(named("cljs.core/Symbol")),
            ),
            ("^{:purpose :kept} [word]", None),
            ("^{:purpose :kept} (word)", None),
            ("^{:purpose :kept} {:a word}", None),
            ("^{:purpose :kept} #{word}", None),
            ("^{:line 1 :other/analyzed true} word", None),
        ] {
            let form = suss_reader::forms::read_forms(source).unwrap().remove(0);
            let mut remaining = crate::portable::DEFAULT_OPERATION_FUEL;
            let mut meter = |work: u64| {
                remaining = remaining
                    .checked_sub(work)
                    .ok_or_else(|| fail(form.span.clone(), "test fuel exhausted"))?;
                Ok(())
            };
            let actual = quoted_constant_tag(&form, &mut meter)
                .unwrap()
                .expect("supported source constant");
            assert!(
                same(actual.as_ref(), expected.as_ref(), &mut meter).unwrap(),
                "quoted datum {source}: {actual:?}"
            );
        }
    }
}

// analyze-wrap-meta has no own :tag; get-tag may still read a source hint.
pub(super) fn metadata_wrapper_tags(form: &Form) -> Result<SourceTags, Diagnostic> {
    with_default_budget(form.span.clone(), |meter: &mut WorkMeter<'_>| {
        metadata_wrapper_tags_metered(form, meter)
    })
}

pub(super) fn metadata_wrapper_tags_metered(
    form: &Form,
    meter: &mut WorkMeter<'_>,
) -> Result<SourceTags, Diagnostic> {
    Ok(SourceTags {
        inferred: hint(form, meter)?.map_or(InferredTag::None, InferredTag::Explicit),
        ..Default::default()
    })
}
