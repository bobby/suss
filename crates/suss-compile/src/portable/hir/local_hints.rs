//! Original lowering of Clojure primitive local hints (#337).
//! JVM Clojure 1.12.6's Compiler.LocalBinding/LocalBindingExpr distinguish
//! getJavaClass from canEmitPrimitive. Runtime observations and provenance
//! are retained in results/m12-local-hints; no JVM implementation is shipped.
use super::*;
use primitive_returns::{Primitive, SourceFacts};

/// Source calls which need numeric inlining even without a local hint.
/// Aliases and compiler-authored core keep their existing call boundaries.
pub(super) fn numeric_call(hir: &Hir) -> bool {
    let Some(source) = &hir.source else { return false };
    if source.origin.as_ref().is_some_and(|origin| origin.is_privileged()) { return false; }
    let Some(SourceBinding::Global { global, .. }) = &source.resolved else { return false };
    let Some(node) = &source.node else { return false };
    let SourceNode::Invoke { arguments, .. } = node.as_ref() else { return false };
    global.namespace() == "suss.core" && matches!((global.name(), arguments.len()),
        ("quot" | "rem" | "unchecked-add" | "unchecked-subtract" | "unchecked-multiply", 2)
        | ("abs" | "unchecked-negate" | "unchecked-inc" | "unchecked-dec", 1))
}

/// These Clojure core functions have :inline primitive implementations. An
/// ordinary alias or user function call still takes the original Object.
pub(super) fn inline(
    hir: &mut Hir, facts: &mut SourceFacts,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    let Some(source) = &hir.source else { return Ok(()) };
    let Some(SourceBinding::Global { global, .. }) = &source.resolved else { return Ok(()) };
    if global.namespace() != "suss.core" { return Ok(()); }
    let Some(node) = &source.node else { return Ok(()) };
    let SourceNode::Invoke { callee, arguments } = node.as_ref() else { return Ok(()) };
    if global.name() == "=" && arguments.len() == 2 {
        let mut classes = Vec::with_capacity(2);
        let mut hinted_object = false;
        for argument in arguments.iter() {
            let class = primitive_returns::class(argument, facts, meter)?;
            hinted_object |= matches!(class, Primitive::Long | Primitive::Double)
                && primitive_returns::kind(argument, facts, meter)? == Primitive::Boxed;
            classes.push(class);
        }
        // Util.equiv has (long,long), (double,double), and either primitive
        // paired with Object, but no mixed (long,double) overload. Such a
        // source call is reflective and must keep both original Objects.
        let numeric = |class| match class {
            Primitive::Byte | Primitive::Short | Primitive::Int | Primitive::Long => Some(Primitive::Long),
            Primitive::Float | Primitive::Double => Some(Primitive::Double),
            _ => None,
        };
        let selected = match (numeric(classes[0]), numeric(classes[1])) {
            (Some(a), Some(b)) => a == b,
            (Some(_), None) => classes[1] == Primitive::Boxed,
            (None, Some(_)) => classes[0] == Primitive::Boxed,
            _ => false,
        };
        if hinted_object && selected {
            let mut arguments = arguments.to_vec();
            for argument in &mut arguments { operand(argument, facts, meter)?; }
            hir.kind = std::sync::Arc::new(if classes.iter().all(|class| numeric(*class).is_some()) {
                // Both selected primitives have the same numeric category,
                // so value equality is also category equality. Keep the
                // comparison raw instead of creating numeric boxes.
                Expression::Comparison { operation: Comparison::StrictEqual, arguments }
            } else {
                // Keep ordinary equality dispatch for the unknown Object;
                // numeric equiv alone cannot stand in for arbitrary IEquiv.
                Expression::Call { callee: Box::new(callee.as_ref().clone()), arguments }
            });
            hir.ty = if classes.iter().all(|class| numeric(*class).is_some()) { Type::Bool } else { Type::Value };
        }
        return Ok(());
    }
    if matches!(global.name(), "unchecked-inc" | "unchecked-dec") && arguments.len() == 1 {
        let operation = if global.name() == "unchecked-inc" { Bitwise::UncheckedAdd } else { Bitwise::UncheckedSubtract };
        let one = Hir { source: None, span: hir.span.clone(), metadata: Vec::new(), ty: Type::Number,
            kind: std::sync::Arc::new(Expression::Literal(Literal::Long(1))) };
        hir.kind = std::sync::Arc::new(Expression::Bitwise { operation, arguments: vec![arguments[0].clone(), one] });
        hir.ty = operation.result();
        return Ok(());
    }
    let operation = match (global.name(), arguments.len()) {
        ("quot", 2) => Some(Bitwise::Quot), ("rem", 2) => Some(Bitwise::Rem),
        ("abs", 1) => Some(Bitwise::Abs),
        ("unchecked-add", 2) => Some(Bitwise::UncheckedAdd),
        ("unchecked-subtract", 2) => Some(Bitwise::UncheckedSubtract),
        ("unchecked-multiply", 2) => Some(Bitwise::UncheckedMultiply),
        ("unchecked-negate", 1) => Some(Bitwise::UncheckedNegate),
        _ => None,
    };
    if let Some(operation) = operation {
        hir.kind = std::sync::Arc::new(Expression::Bitwise { operation, arguments: arguments.to_vec() });
        hir.ty = operation.result();
        return Ok(());
    }
    if arguments.len() != 1 { return Ok(()); }
    let index = match global.name() {
        "byte" => 3, "short" => 2, "int" => 1, "float" => 8,
        "unchecked-byte" => 7, "unchecked-short" => 6, "unchecked-int" => 5, "unchecked-float" => 9,
        _ => return Ok(()),
    };
    let index = Hir { source: None, span: hir.span.clone(), metadata: Vec::new(), ty: Type::Number,
        kind: std::sync::Arc::new(Expression::Literal(Literal::Long(index))) };
    hir.kind = std::sync::Arc::new(Expression::Bitwise { operation: Bitwise::Cast, arguments: vec![arguments[0].clone(), index] });
    hir.ty = Bitwise::Cast.result();
    Ok(())
}

/// Numbers' bitwise overloads take only long or Object. A floating source
/// class cannot match either formal primitive/reference type, so Clojure
/// invokes reflectively and none of the Object local hints are consumed.
pub(super) fn long_operands(
    arguments: &[Hir], facts: &mut SourceFacts,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<bool, Diagnostic> {
    parameters(arguments, false, facts, meter)
}

/// Numeric overloads have long/double/Object parameters, but a primitive
/// boolean cannot match any of them. Reflection then uses original Objects.
pub(super) fn numeric_operands(
    arguments: &[Hir], facts: &mut SourceFacts,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<bool, Diagnostic> {
    parameters(arguments, true, facts, meter)
}

fn parameters(
    arguments: &[Hir], floating: bool, facts: &mut SourceFacts,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<bool, Diagnostic> {
    for argument in arguments {
        let accepts = match primitive_returns::class(argument, facts, meter)? {
            Primitive::Boxed | Primitive::Byte | Primitive::Short | Primitive::Int | Primitive::Long => true,
            Primitive::Float | Primitive::Double => floating,
            _ => false,
        };
        if !accepts { return Ok(false); }
    }
    Ok(true)
}

pub(super) fn operand(
    hir: &mut Hir, facts: &mut SourceFacts,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    if primitive_returns::kind(hir, facts, meter)? != Primitive::Boxed { return Ok(()); }
    let operation = match primitive_returns::class(hir, facts, meter)? {
        Primitive::Long => Nominal::LongCast, Primitive::Double => Nominal::DoubleCast,
        _ => return Ok(()),
    };
    let value = hir.clone();
    *hir = Hir { source: None, span: value.span.clone(), metadata: Vec::new(), ty: operation.result(),
        kind: std::sync::Arc::new(Expression::Nominal { operation, arguments: vec![value] }) };
    Ok(())
}
