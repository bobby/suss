//! Original lowering of Clojure primitive local hints (#337).
//! JVM Clojure 1.12.6's Compiler.LocalBinding/LocalBindingExpr distinguish
//! getJavaClass from canEmitPrimitive. Runtime observations and provenance
//! are retained in results/m12-local-hints; no JVM implementation is shipped.
use super::*;
use primitive_returns::{Primitive, SourceFacts};

/// These Clojure core functions have :inline primitive implementations. An
/// ordinary alias or user function call still takes the original Object.
pub(super) fn inline(hir: &mut Hir) {
    let Some(source) = &hir.source else { return };
    let Some(SourceBinding::Global { global, .. }) = &source.resolved else { return };
    if global.namespace() != "suss.core" { return; }
    let Some(node) = &source.node else { return };
    let SourceNode::Invoke { arguments, .. } = node.as_ref() else { return };
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
        return;
    }
    if arguments.len() != 1 { return; }
    let index = match global.name() {
        "byte" => 3, "short" => 2, "int" => 1, "float" => 8,
        "unchecked-byte" => 7, "unchecked-short" => 6, "unchecked-int" => 5, "unchecked-float" => 9,
        _ => return,
    };
    let index = Hir { source: None, span: hir.span.clone(), metadata: Vec::new(), ty: Type::Number,
        kind: std::sync::Arc::new(Expression::Literal(Literal::Long(index))) };
    hir.kind = std::sync::Arc::new(Expression::Bitwise { operation: Bitwise::Cast, arguments: vec![arguments[0].clone(), index] });
    hir.ty = Bitwise::Cast.result();
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
