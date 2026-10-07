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
    pub inferred: Option<Form>,
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
fn same(a: &Option<Form>, b: &Option<Form>) -> bool {
    fn bare(form: &Form) -> Form {
        let mut form = form.clone();
        form.span = 0..0;
        form.metadata.clear();
        match &mut form.kind {
            Kind::List(items) | Kind::Vector(items) | Kind::Map(items) | Kind::Set(items) => {
                for item in items {
                    *item = bare(item);
                }
            }
            _ => {}
        }
        if let Kind::Set(items) = &mut form.kind {
            items.sort_by_key(|item| format!("{item:?}"));
        }
        form
    }
    a.as_ref().map(bare) == b.as_ref().map(bare)
}
fn metadata(form: &Form, name: &str) -> Result<Option<Form>, Diagnostic> {
    Ok(reader_metadata_pairs(form)?.chunks_exact(2).find_map(|pair| {
        matches!(&pair[0].kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == name).then(|| pair[1].clone())
    }))
}
fn hint(form: &Form) -> Result<Option<Form>, Diagnostic> {
    Ok(metadata(form, "tag")?.filter(|tag| !matches!(tag.kind, Kind::Nil)))
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
fn quoted_constant_tag(form: &Form) -> Result<Option<Option<Form>>, Diagnostic> {
    let Some(tag) = constant(form) else {
        return Ok(None);
    };
    let meaningful = reader_metadata_pairs(form)?.chunks_exact(2).any(|pair| {
        !matches!(&pair[0].kind, Kind::Keyword(key)
            if (key.namespace.is_none()
                && matches!(key.name.as_str(), "file" | "line" | "column" | "end-column" | "end-line" | "source"))
            || (key.namespace.as_deref() == Some("cljs.analyzer") && key.name == "analyzed"))
    });
    Ok(Some(if meaningful { hint(form)? } else { Some(tag) }))
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
                if let Expression::If { condition, alternative, .. } = &body.kind {
                    if matches!(condition.kind, Expression::Nominal { operation: Nominal::IsClosure, .. })
                        && matches!(alternative.kind, Expression::Nominal { operation: Nominal::BindCallable, .. })
                    {
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
    if let Some(tag) = hint(&binding.declaration)? {
        return Ok(Some(tag));
    }
    binding
        .initializer
        .as_ref()
        .map_or(Ok(None), |init| inferred(init, 0))
}
fn truthy(form: &Form) -> bool {
    !matches!(form.kind, Kind::Nil | Kind::Bool(false))
}
fn provisional(info: &super::super::resolve::DefinitionInfo) -> Result<bool, Diagnostic> {
    Ok(!info.analysis_completed || (info.initializer_form.is_some()
        && metadata(&info.declaration, "declared")?.as_ref().is_some_and(truthy)))
}
fn completed_callable(info: &super::super::resolve::DefinitionInfo) -> Result<Option<&SourceCallable>, Diagnostic> {
    Ok(if provisional(info)? { None } else {
        info.initializer.as_ref().and_then(|init| callable(init))
    })
}
// Pinned analyzer.cljc2092–2175 merges raw metadata before callable top-fn
// overlays and computed return fields. Original Rust policy; never treat a
// provisional initializer as a published callable declaration.
fn top_function_property(info: &super::super::resolve::DefinitionInfo, name: &str) -> Result<Option<Form>, Diagnostic> {
    let Some(top) = metadata(&info.declaration, "top-fn")? else { return Ok(None) };
    let pairs = match &top.kind {
        Kind::Nil => return Ok(None),
        Kind::Map(items) => items,
        Kind::Vector(items) if items.len() == 2 => items,
        Kind::List(items) | Kind::Set(items) if items.is_empty() => return Ok(None),
        Kind::String(units) if units.is_empty() => return Ok(None),
        _ => return Err(fail(top.span.clone(), "Function declaration top-fn requires mergeable reader data")),
    };
    Ok(pairs.chunks_exact(2).rev().find_map(|pair| {
        matches!(&pair[0].kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == name)
            .then(|| pair[1].clone())
    }))
}
pub fn declaration_tag(
    info: &super::super::resolve::DefinitionInfo,
) -> Result<Option<Form>, Diagnostic> {
    let raw = metadata(&info.declaration, "tag")?;
    // parse-type merges source metadata over its default function tag,
    // including explicit nil/false. Type declarations precede method analysis.
    if info.type_fields.is_some() { return Ok(raw.or_else(|| Some(named("function")))) }
    if provisional(info)? { return Ok(raw) }
    // Function vars retain their raw/overlaid tag, including present nil and
    // false. Dynamic affects scalar inference, not function var-reference tags.
    if completed_callable(info)?.is_some() {
        return Ok(top_function_property(info, "tag")?.or(raw));
    }
    if raw.as_ref().is_some_and(truthy) { return Ok(raw) }
    if metadata(&info.declaration, "dynamic")?.as_ref().is_some_and(truthy) {
        return Ok(Some(named("any")));
    }
    let computed = info.initializer.as_ref().map(|init| inferred(init, 0)).transpose()?.flatten();
    Ok(computed.or(raw))
}
fn declaration_return_tag(info: &super::super::resolve::DefinitionInfo) -> Result<Option<Form>, Diagnostic> {
    let methods = completed_callable(info)?;
    let fn_var = if methods.is_some() {
        top_function_property(info, "fn-var")?.as_ref().map_or(
            !metadata(&info.declaration, "macro")?.as_ref().is_some_and(truthy), truthy)
    } else {
        metadata(&info.declaration, "fn-var")?.as_ref().is_some_and(truthy)
            || (!provisional(info)?
                && metadata(&info.declaration, "declared")?.as_ref().is_some_and(truthy)
                && metadata(&info.declaration, "arglists")?.as_ref().is_some_and(truthy))
    };
    if !fn_var { return Ok(None) }
    let raw = metadata(&info.declaration, "ret-tag")?;
    let tag = if let Some(methods) = methods {
        let computed = metadata(&info.declaration, "tag")?.filter(truthy).or(return_tag(methods)?);
        computed.or(top_function_property(info, "ret-tag")?).or(raw)
    } else { raw };
    // infer-invoke uses if-some, preserving false but falling through nil.
    Ok(tag.filter(|tag| !matches!(tag.kind, Kind::Nil)))
}
fn resolved_tag(binding: Option<&SourceBinding>) -> Result<Option<Form>, Diagnostic> {
    match binding {
        Some(SourceBinding::Local(binding)) => local_tag(binding),
        Some(SourceBinding::Field(binding)) => hint(&binding.declaration),
        Some(SourceBinding::Global {
            declaration: Some(info),
            ..
        }) => declaration_tag(info),
        _ => Ok(None),
    }
}
fn resolved_callable(binding: Option<&SourceBinding>) -> Option<&SourceCallable> {
    match binding {
        Some(SourceBinding::Local(binding)) => callable(binding.initializer.as_ref()?),
        _ => None,
    }
}
fn return_tag(methods: &SourceCallable) -> Result<Option<Form>, Diagnostic> {
    let mut result = None;
    for (index, method) in methods.methods.iter().enumerate() {
        let tag = inferred(&method.body, 0)?;
        if index == 0 {
            result = tag;
        } else if !same(&result, &tag) {
            return Ok(None);
        }
    }
    Ok(result)
}
fn union(then: Option<Form>, otherwise: Option<Form>) -> Option<Form> {
    if same(&then, &otherwise)
        || otherwise
            .as_ref()
            .is_some_and(|tag| is_named(tag, "ignore"))
    {
        return then;
    }
    if then.as_ref().is_some_and(|tag| is_named(tag, "ignore")) {
        return otherwise;
    }
    if [&then, &otherwise].iter().all(|tag| {
        tag.as_ref()
            .is_some_and(|tag| is_named(tag, "clj") || is_named(tag, "not-native"))
    }) {
        return Some(named("clj"));
    }
    if [&then, &otherwise].iter().all(|tag| {
        tag.as_ref()
            .is_some_and(|tag| is_named(tag, "boolean") || is_named(tag, "seq"))
    }) {
        return Some(named("seq"));
    }
    let mut items: Vec<Form> = Vec::new();
    for branch in [then, otherwise] {
        let branch = branch.unwrap_or(Form {
            span: 0..0,
            metadata: vec![],
            kind: Kind::Nil,
        });
        let values = match branch {
            Form {
                kind: Kind::Set(values),
                ..
            } => values,
            branch => vec![branch],
        };
        for value in values {
            if !items
                .iter()
                .any(|previous| same(&Some(previous.clone()), &Some(value.clone())))
            {
                items.push(value);
            }
        }
    }
    if items.iter().any(|tag| is_named(tag, "any")) {
        return Some(named("any"));
    }
    if items.iter().any(|tag| is_named(tag, "seq")) {
        items.retain(|tag| !is_named(tag, "clj-nil"));
    }
    if items.len() == 1 {
        items.pop()
    } else {
        Some(Form {
            span: 0..0,
            metadata: vec![],
            kind: Kind::Set(items),
        })
    }
}
fn inferred(hir: &Hir, depth: usize) -> Result<Option<Form>, Diagnostic> {
    if let Some(source) = &hir.source {
        return Ok(source.tags.inferred.clone());
    }
    if depth >= 128 {
        return Err(fail(
            hir.span.clone(),
            "Source inference traversal exceeds bounds",
        ));
    }
    match &hir.kind {
        Expression::Literal(value) => Ok(Some(named(match value {
            Literal::Nil | Literal::Undefined => "clj-nil",
            Literal::Bool(_) => "boolean",
            Literal::Number(_) => "number",
            Literal::String(_) => "string",
        }))),
        Expression::Do(items) => items
            .last()
            .map_or(Ok(Some(named("clj-nil"))), |item| inferred(item, depth + 1)),
        Expression::Let { body, .. }
        | Expression::Loop { body, .. }
        | Expression::DynamicScope { body, .. } => inferred(body, depth + 1),
        Expression::Function { body, .. } => inferred(body, depth + 1),
        _ => Ok(None),
    }
}
pub(super) fn source_tags(
    form: &Form,
    resolved: Option<&SourceBinding>,
    methods: Option<&SourceCallable>,
    hir: &Hir,
) -> Result<SourceTags, Diagnostic> {
    let quoted_const_tag = if let Kind::List(items) = &form.kind
        && items.len() == 2
        && matches!(&items[0].kind, Kind::Symbol(head)
            if head.namespace.is_none() && head.name == "quote")
    {
        quoted_constant_tag(&items[1])?
    } else {
        None
    };
    let inferred_return = methods.map(return_tag).transpose()?;
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
            Kind::Symbol(_) => resolved_tag(resolved)?,
            Kind::List(items) if items.is_empty() => constant(form),
            Kind::List(items)
                if matches!(items.first().map(|head| &head.kind), Some(Kind::Symbol(head)) if head.namespace.is_none() && head.name == "quote")
                    && items.len() == 2 =>
            {
                quoted_const_tag.as_ref().map(|tag| tag.clone().unwrap_or_else(|| named("any")))
            }
            _ => match &hir.kind {
                Expression::Arithmetic { .. } | Expression::Bitwise { .. } => Some(named("number")),
                Expression::Comparison { .. } | Expression::NilTest(_) => Some(named("boolean")),
                Expression::Throw(_) | Expression::Recur { .. } => Some(named("ignore")),
                Expression::Do(_)
                | Expression::Let { .. }
                | Expression::Loop { .. }
                | Expression::DynamicScope { .. } => inferred(hir, 0)?,
                Expression::Try { regions } => inferred(&regions[0], 0)?,
                Expression::If {
                    condition,
                    consequent,
                    alternative,
                } => {
                    let truthy_constant = condition
                        .source
                        .as_ref()
                        .is_some_and(|source| truthy_constant(&source.form));
                    let then = inferred(consequent, 0)?;
                    if truthy_constant {
                        then
                    } else {
                        union(then, inferred(alternative, 0)?)
                    }
                }
                Expression::Definition { initializer, .. } => initializer
                    .as_ref()
                    .map_or(Ok(None), |init| inferred(init, 0))?,
                Expression::Assign { .. } => None,
                Expression::Call { callee, arguments } => {
                    let methods = callable_before_dispatch(callee).or_else(|| resolved_callable(resolved));
                    let method = methods.and_then(|methods| {
                        methods.methods.iter().find(|method| {
                            method.variadic || method.parameters.len() == arguments.len()
                        })
                    });
                    let method_tag = method
                        .map(|method| inferred(&method.body, 0))
                        .transpose()?
                        .flatten();
                    // Global vars retain an aggregate return tag, not their
                    // initializer's individual methods (analyzer def parse).
                    // Local/direct functions retain per-method information.
                    let global_tag = match resolved {
                        Some(SourceBinding::Global { declaration: Some(info), .. }) => {
                            declaration_return_tag(info)?
                        }
                        _ => None,
                    };
                    method_tag.or(global_tag).or(Some(named("any")))
                }
                Expression::Nominal { operation: Nominal::Construct, arguments } => {
                    // parse-new derives the result from source resolution,
                    // not the physical runtime constructor/descriptor value.
                    // Resolution on an invocation belongs to its callee, not
                    // the value of the constructor expression. Only reference
                    // ASTs carry the :info read by pinned parse-new.
                    let class = arguments.first().and_then(|class| class.source.as_ref())
                        .filter(|class| matches!(class.form.kind, Kind::Symbol(_)));
                    let class_tag = class.map(|class| resolved_tag(class.resolved.as_ref()))
                        .transpose()?.flatten();
                    let name = match class.and_then(|class| class.resolved.as_ref()) {
                        Some(SourceBinding::Global { global, .. }) => Some(named(&format!("{}/{}", global.namespace(), global.name()))),
                        Some(SourceBinding::Local(binding)) => Some(binding.declaration.clone()),
                        Some(SourceBinding::Field(binding)) => Some(binding.declaration.clone()),
                        _ => None,
                    };
                    let primitive = |tag: &Form| {
                        if let Kind::Symbol(symbol) = &tag.kind
                            && symbol.namespace.as_deref() == Some("js")
                        {
                            match symbol.name.as_str() {
                                "Object" => Some("object"), "String" => Some("string"),
                                "Array" => Some("array"), "Number" => Some("number"),
                                "Function" => Some("function"), "Boolean" => Some("boolean"),
                                _ => None,
                            }
                        } else { None }
                    };
                    let js_tag = class_tag.as_ref().is_some_and(|tag| matches!(&tag.kind,
                        Kind::Symbol(symbol) if symbol.namespace.as_deref() == Some("js")
                            || (symbol.namespace.is_none() && symbol.name == "js")));
                    if js_tag && class_tag.as_ref().and_then(primitive).is_none() {
                        Some(named("js"))
                    } else {
                        Some(name.and_then(|name| primitive(&name).map(named).or(Some(name)))
                            .unwrap_or(Form { kind: Kind::Nil, span: 0..0, metadata: vec![] }))
                    }
                }
                _ => None,
            },
        }
    };
    // get-tag/infer-tag use if-some. A present nil AST tag remains present
    // in the source view, but does not become an inferred return type.
    let inferred = hint(form)?.or_else(|| tag.clone().filter(|tag| !matches!(tag.kind, Kind::Nil)));
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

    #[test]
    fn scalar_quote_preserves_outer_source_instead_of_private_literal_source() {
        for phase in [Phase::Runtime, Phase::Macro] {
            for source in ["(quote nil)", "(quote false)", "(quote 42)", "(quote \"hello\")"] {
                let hir = crate::portable::analyze_in(source, &Environment::default(), phase).unwrap();
                let Expression::Do(forms) = &hir.kind else { panic!("fragment source forms") };
                assert_eq!(forms.len(), 1);
                assert!(matches!(&forms[0].kind, Expression::Literal(_)));
                let retained = forms[0].source.as_ref().expect("actual quoted source record");
                let expected = suss_reader::forms::read_forms(source).unwrap().remove(0);
                assert_eq!(retained.form, expected, "enclosing source quote in {phase:?}");
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
            let actual = quoted_constant_tag(&form)
                .unwrap()
                .expect("supported source constant");
            assert!(
                same(&actual, &expected),
                "quoted datum {source}: {actual:?}"
            );
        }
    }
}

// analyze-wrap-meta has no own :tag; get-tag may still read a source hint.
pub(super) fn metadata_wrapper_tags(form: &Form) -> Result<SourceTags, Diagnostic> {
    Ok(SourceTags { inferred: hint(form)?, ..Default::default() })
}
