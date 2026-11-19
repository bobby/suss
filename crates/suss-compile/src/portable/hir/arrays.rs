//! Bounded adaptations of the pin's array macro expansions.
use super::*;
impl Analyzer<'_> {
    pub(super) fn array_form(
        &mut self,
        form: &Form,
        args: &[Form],
        operation: ArrayOperation,
    ) -> Result<Hir, Diagnostic> {
        if !operation.valid(args.len()) {
            return Err(fail(form.span.clone(), "Wrong bootstrap array macro arity"));
        }
        let node = |operation, arguments| Hir {
            source: None,
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Value,
            kind: std::sync::Arc::new(Expression::Array {
                operation,
                arguments,
            }),
        };
        if operation == ArrayOperation::Get || operation == ArrayOperation::Set {
            let mut current = self.form(&args[0])?;
            // One index into an array whose source tag is a packed element
            // type (#389): `^longs`, `^doubles` and the rest, as Clojure's
            // hinted aget and aset, read and write the element inline.
            let kind = packed_kind(&current);
            let simple = if operation == ArrayOperation::Get { args.len() == 2 } else { args.len() == 3 };
            if let Some(kind) = kind.filter(|kind| simple && (operation == ArrayOperation::Get || matches!(kind, 2 | 4 | 5 | 6))) {
                let mut operands = vec![current];
                for argument in &args[1..] {
                    operands.push(self.form(argument)?);
                }
                let nominal = if operation == ArrayOperation::Get { Nominal::PackedGet(kind) } else { Nominal::PackedSet(kind) };
                return Ok(self.nominal(form, nominal, operands));
            }
            let last = if operation == ArrayOperation::Set {
                args.len() - 2
            } else {
                args.len() - 1
            };
            for (position, index) in args[1..=last].iter().enumerate() {
                let argument = self.form(index)?;
                if operation == ArrayOperation::Set && position + 1 == last {
                    return Ok(node(
                        ArrayOperation::Set,
                        vec![current, argument, self.form(&args[last + 1])?],
                    ));
                }
                current = node(ArrayOperation::Get, vec![current, argument]);
            }
            return Ok(current);
        }
        // make-array's operands are evaluated left to right, the type too,
        // as Clojure evaluates a call's (#425); the runtime ignores the type.
        let arguments = args
            .iter()
            .map(|arg| self.form(arg))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(node(operation, arguments))
    }
}

/// The packed element kind, in `packed_arrays::KINDS` order, that an array
/// expression's source tag names (#389).
pub(super) fn packed_kind(array: &Hir) -> Option<usize> {
    let source = array.source.as_ref()?;
    // The inferred tag already gives a hint precedence over the operand's own.
    let tag = source.tags.inferred.as_ref().or(source.tags.tag.as_ref())?;
    let Kind::Symbol(symbol) = &tag.kind else { return None };
    if symbol.namespace.is_some() {
        return None;
    }
    Some(match symbol.name.as_str() {
        "bytes" => 0,
        "shorts" => 1,
        "ints" => 2,
        "floats" => 3,
        "longs" => 4,
        "doubles" => 5,
        // Clojure's Object[] (#378).
        "objects" => 6,
        _ => return None,
    })
}
