//! Object consumption follows the pinned primary source boxing controls.
//! Source Object consumption differs from backend raw/boxed representation.
use super::*;
use primitive_returns::{Primitive, SourceFacts, SourceType};

/// Select the Object result of an expression. Generic calls and unhinted
/// function returns load numeric literal objects; primitive array elements
/// box their values. A primitive local boxes afresh in either context.
pub(super) fn object(
    hir: &mut Hir, literal_constant: bool, facts: &mut SourceFacts,
    source_casts: &std::collections::HashMap<BindingId, Primitive>,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    meter(1)?;
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        use Expression::*;
        match hir.kind_mut() {
            Do(items) => {
                if let Some(last) = items.last_mut() { object(last, literal_constant, facts, source_casts, meter)?; }
                return Ok(());
            }
            Let { bindings, body } => {
                let mut previous = Vec::new();
                for binding in bindings {
                    let old = if let Some(kind) = source_casts.get(&binding.id) {
                        facts.insert(binding.id, *kind)
                    } else {
                        let ty = primitive_returns::binding_type(binding, facts, meter)?;
                        facts.insert_type(binding.id, ty)
                    };
                    previous.push((binding.id, old));
                }
                let result = object(body, literal_constant, facts, source_casts, meter);
                restore(facts, previous);
                return result;
            }
            LetFn { bindings, body } => {
                let previous = bindings.iter().map(|binding|
                    (binding.id, facts.insert(binding.id, Primitive::Boxed))).collect();
                let result = object(body, literal_constant, facts, source_casts, meter);
                restore(facts, previous);
                return result;
            }
            Loop { target, bindings, body } => {
                let kinds = primitive_returns::loop_types(*target, bindings, body, facts, meter)?;
                let previous = bindings.iter().zip(kinds).map(|(binding, ty)|
                    (binding.id, facts.insert_type(binding.id, ty))).collect();
                let result = object(body, literal_constant, facts, source_casts, meter);
                restore(facts, previous);
                return result;
            }
            If { consequent, alternative, .. } => {
                object(consequent, literal_constant, facts, source_casts, meter)?;
                object(alternative, literal_constant, facts, source_casts, meter)?;
                return Ok(());
            }
            _ => {}
        }
        if !matches!(primitive_returns::kind(hir, facts, meter)?, Primitive::Double | Primitive::Float) {
            return Ok(());
        }
        if literal_constant && let Expression::Literal(super::Literal::Number(value)) = &*hir.kind {
            hir.kind = std::sync::Arc::new(Expression::Literal(super::Literal::ObjectNumber(*value)));
        } else {
            let value = hir.clone();
            *hir = Hir { source: None, span: value.span.clone(), metadata: Vec::new(),
                ty: super::Nominal::BoxDoubleObject.result(), kind: std::sync::Arc::new(Expression::Nominal {
                    operation: super::Nominal::BoxDoubleObject, arguments: vec![value] }) };
        }
        Ok(())
    })
}
fn restore(facts: &mut SourceFacts, previous: Vec<(BindingId, Option<SourceType>)>) {
    for (id, old) in previous.into_iter().rev() {
        match old { Some(ty) => { facts.insert_type(id, ty); }, None => facts.remove(&id) }
    }
}
