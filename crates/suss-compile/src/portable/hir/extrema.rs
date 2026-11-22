//! Original max/min lowering from JVM Clojure 1.12.6 observations (#337).
//! Core.clj's nary-inline nests binary Numbers calls; Numbers.java returns
//! primitives only for long/long and double/double. References and EPL provenance
//! are retained in results/m12-extrema. No Java implementation is shipped.
use super::*;
use primitive_returns::{Primitive, SourceFacts};

pub(super) fn is_call(hir: &Hir) -> bool {
    let Some(source) = &hir.source else { return false };
    if source.origin.as_ref().is_some_and(|origin| origin.is_privileged()) { return false; }
    matches!(&source.resolved, Some(SourceBinding::Global { global, .. })
        if global.namespace() == "suss.core" && matches!(global.name(), "max" | "min"))
        && matches!(&source.node, Some(node) if matches!(node.as_ref(), SourceNode::Invoke { arguments, .. } if arguments.len() > 1))
}

/// The selected primitive return, or Object for mixed/unknown/reflected calls.
pub(super) fn result(left: Primitive, right: Primitive) -> Primitive {
    use Primitive::*;
    let widen = |kind| match kind { Byte | Short | Int => Long, Float => Double, kind => kind };
    match (widen(left), widen(right)) {
        (Long, Long) => Long, (Double, Double) => Double, _ => Boxed,
    }
}

pub(super) fn inline(
    hir: &mut Hir, facts: &mut SourceFacts,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    if !is_call(hir) { return Ok(()); }
    let source = hir.source.as_ref().unwrap();
    let Some(SourceBinding::Global { global, .. }) = &source.resolved else { unreachable!() };
    let SourceNode::Invoke { arguments, .. } = source.node.as_ref().unwrap().as_ref() else { unreachable!() };
    let maximum = global.name() == "max";
    let mut left = arguments[0].clone();
    for right in &arguments[1..] {
        meter(1)?;
        let kind = result(primitive_returns::class(&left, facts, meter)?,
            primitive_returns::class(right, facts, meter)?);
        let operation = match (maximum, kind) {
            (true, Primitive::Long) => Bitwise::LongMax, (false, Primitive::Long) => Bitwise::LongMin,
            (true, Primitive::Double) => Bitwise::DoubleMax, (false, Primitive::Double) => Bitwise::DoubleMin,
            (true, _) => Bitwise::Max, (false, _) => Bitwise::Min,
        };
        let mut pair = vec![left, right.clone()];
        if local_hints::numeric_operands(&pair, facts, meter)? {
            for operand in &mut pair { local_hints::operand(operand, facts, meter)?; }
        }
        // Synthetic folds have no source syntax. Their operation carries the
        // selected return kind so the next pair sees the correct source class.
        left = Hir { source: None, span: hir.span.clone(), metadata: Vec::new(), ty: operation.result(),
            kind: std::sync::Arc::new(Expression::Bitwise { operation, arguments: pair }) };
    }
    hir.kind = left.kind.clone();
    hir.ty = left.ty;
    Ok(())
}
