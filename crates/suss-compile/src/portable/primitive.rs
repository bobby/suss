//! Primitive function signatures retained in IR and artifact identities (#448).
//! Source return validation is separate: these signatures describe an actual
//! entry's Wasm parameters/result, not a promise about a future var value.
use super::{Diagnostic, hir::{self, Hir, Nominal, Parameter}};
use suss_reader::forms::Kind as FormKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind { Boxed, Long, Double }

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Signature {
    pub parameters: Vec<Kind>,
    pub result: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Method {
    pub variadic: bool,
    pub signature: Signature,
}

/// The additional fields of a code subtype, sorted by formal arity. Including
/// every fixed primitive method makes the shape canonical across fragments.
/// A variadic body's final formal is a rest seq, not one fixed argument;
/// only its ordinary adapters may call it until typed adapters exist.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Shape(pub Vec<Method>);

impl Shape {
    pub fn fixed(&self, arity: usize) -> Option<&Signature> {
        self.0.iter().find(|method| !method.variadic && method.signature.parameters.len() == arity)
            .map(|method| &method.signature)
    }
}

/// An actual initializer's entry shape. Follow only value-preserving lexical
/// wrappers; source call declarations never promise a future var's value.
fn shape<'a>(hir: &'a Hir, scope: &[&'a hir::Binding], meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>) -> Result<Shape, Diagnostic> {
    meter(1)?;
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        Ok(match &*hir.kind {
            hir::Expression::Function { parameters, body, .. } => Shape(signature(parameters, body, meter)?
                .into_iter().map(|signature| Method { variadic: false, signature }).collect()),
            hir::Expression::GeneralFunction { methods, .. } => {
                let mut result = Vec::new();
                for method in methods {
                    if method.variadic { continue; }
                    if let Some(signature) = signature(&method.parameters, &method.body, meter)? {
                        result.push(Method { variadic: method.variadic, signature });
                    }
                }
                result.sort_by_key(|method| (method.signature.parameters.len(), method.variadic));
                Shape(result)
            }
            hir::Expression::Local(id) => match scope.iter().rev().find(|binding| binding.id == *id) {
                Some(binding) => shape(&binding.value, &[], meter)?, None => Shape(Vec::new()),
            },
            hir::Expression::Do(items) => match items.last() {
                Some(last) => shape(last, scope, meter)?, None => Shape(Vec::new()),
            },
            hir::Expression::Let { bindings, body } | hir::Expression::LetFn { bindings, body } => {
                meter(scope.len().saturating_add(bindings.len()) as u64)?;
                let mut inner = scope.to_vec();
                inner.extend(bindings);
                shape(body, &inner, meter)?
            }
            _ => Shape(Vec::new()),
        })
    })
}

pub(crate) fn call_shape(hir: &Hir, callee: &Hir, meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>) -> Result<Option<Shape>, Diagnostic> {
    let source = hir.source.as_ref().or(callee.source.as_ref());
    let Some(hir::SourceBinding::Global { declaration: Some(info), .. }) = source.and_then(|source| source.resolved.as_ref()) else {
        return Ok(None);
    };
    // Live calls require arglist hints, as Clojure's IFn primitive invocation
    // does. Direct links can use the actual initializer's signature directly.
    if !matches!(&*callee.kind, hir::Expression::Nominal { operation: Nominal::DirectLinkRead(_), .. }) {
        let declaration = super::clone_form_metered(&info.declaration, meter)?;
        let pairs = hir::reader_metadata_pairs_metered(&declaration, meter)?;
        if !pairs.chunks_exact(2).any(|pair| matches!(&pair[0].kind,
            FormKind::Keyword(key) if key.namespace.is_none() && key.name == "arglists")) {
            return Ok(None);
        }
    }
    let Some(initializer) = &info.initializer else { return Ok(None) };
    let result = shape(initializer, &[], meter)?;
    Ok((!result.0.is_empty()).then_some(result))
}

pub(crate) fn signature(
    parameters: &[Parameter], body: &Hir,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<Option<Signature>, Diagnostic> {
    let mut kinds = Vec::with_capacity(parameters.len());
    for parameter in parameters {
        meter(1)?;
        let mut metadata = Vec::with_capacity(parameter.metadata.len());
        for prefix in &parameter.metadata { metadata.push(super::clone_form_metered(prefix, meter)?); }
        let carrier = suss_reader::forms::Form { span: parameter.span.clone(), metadata, kind: FormKind::Nil };
        let pairs = hir::reader_metadata_pairs_metered(&carrier, meter)?;
        let kind = pairs.chunks_exact(2).find_map(|pair| {
            if !matches!(&pair[0].kind, FormKind::Keyword(key) if key.namespace.is_none() && key.name == "tag") { return None; }
            match &pair[1].kind {
                FormKind::Symbol(symbol) if symbol.namespace.is_none() => match symbol.name.as_str() {
                    "long" => Some(Kind::Long), "double" => Some(Kind::Double), _ => None,
                },
                _ => None,
            }
        }).unwrap_or(Kind::Boxed);
        kinds.push(kind);
    }
    // The return conversion sits outside the method's recurrence loop. An
    // explicit core cast *inside* that loop is not a declaration return tag.
    let result = match &*body.kind {
        hir::Expression::Nominal { operation: Nominal::LongCast | Nominal::UncheckedLongCast, .. } => Kind::Long,
        hir::Expression::Nominal { operation: Nominal::DoubleCast, .. } => Kind::Double,
        _ => Kind::Boxed,
    };
    // Only an actually converted parameter can move to a raw entry. In
    // particular, a nominal method's receiver keeps its receiver type even
    // when source metadata contains a conflicting numeric tag.
    if kinds.iter().any(|kind| *kind != Kind::Boxed) {
        let loop_body = match &*body.kind {
            hir::Expression::Nominal { arguments, .. } if arguments.len() == 1 => &arguments[0],
            _ => body,
        };
        let mut inputs = std::collections::HashMap::new();
        let mut casts = std::collections::HashMap::new();
        if let hir::Expression::Loop { bindings, body, .. } = &*loop_body.kind {
            for binding in bindings {
                meter(1)?;
                if let hir::Expression::Local(id) = &*binding.value.kind { inputs.insert(*id, binding.id); }
            }
            if let hir::Expression::Let { bindings, .. } = &*body.kind {
                for binding in bindings {
                    meter(1)?;
                    if let hir::Expression::Nominal { operation, arguments } = &*binding.value.kind
                        && arguments.len() == 1
                        && let hir::Expression::Local(id) = &*arguments[0].kind
                    {
                        let kind = match operation {
                            Nominal::LongCast => Kind::Long, Nominal::DoubleCast => Kind::Double, _ => continue,
                        };
                        casts.insert(*id, kind);
                    }
                }
            }
        }
        for (parameter, kind) in parameters.iter().zip(&mut kinds) {
            meter(1)?;
            if *kind != Kind::Boxed && inputs.get(&parameter.id).and_then(|id| casts.get(id)) != Some(kind) {
                *kind = Kind::Boxed;
            }
        }
    }
    Ok((result != Kind::Boxed || kinds.iter().any(|kind| *kind != Kind::Boxed))
        .then_some(Signature { parameters: kinds, result }))
}
