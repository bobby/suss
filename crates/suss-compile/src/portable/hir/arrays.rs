//! Bounded adaptations of the pin's array macro expansions.
use super::*;
impl Analyzer<'_> {
    pub(super) fn array_form(
        &mut self,
        form: &Form,
        args: &[Form],
        mut operation: ArrayOperation,
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
            if let Some(kind) = kind.filter(|kind| simple && (operation == ArrayOperation::Get || matches!(kind, 2 | 4 | 5))) {
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
        let mut args = args;
        if operation == ArrayOperation::Make && args.len() == 2 {
            args = &args[1..];
        }
        if operation == ArrayOperation::Make && args.len() >= 3 {
            // The pin first constructs the list of more-sizes, then evaluates
            // the outer size. Its compatibility type operand is not evaluated.
            let mut dimensions = Vec::new();
            for argument in &args[2..] {
                let value = self.form(argument)?;
                dimensions.push(self.array_temporary(argument, value));
            }
            let value = if let Some(size) = literal_size(&args[1].kind) {
                if !size.is_finite() || size > 1_000_000.0 {
                    return Err(fail(
                        args[1].span.clone(),
                        "Literal array size exceeds bootstrap compilation limit",
                    ));
                }
                Hir {
                    source: None,
                    span: args[1].span.clone(),
                    metadata: vec![],
                    ty: Type::Number,
                    kind: std::sync::Arc::new(Expression::Literal(Literal::Long(size.ceil().max(0.0) as i64))),
                }
            } else {
                self.form(&args[1])?
            };
            let size = self.array_temporary(&args[1], value);
            let nil = Hir {
                source: None,
                span: form.span.clone(),
                metadata: vec![],
                ty: Type::Nil,
                kind: std::sync::Arc::new(Expression::Literal(Literal::Nil)),
            };
            let mut operands = vec![nil, self.local(form, size.id)];
            operands.extend(
                dimensions
                    .iter()
                    .map(|binding| self.local(form, binding.id)),
            );
            let body = node(operation, operands);
            dimensions.push(size);
            let mut result = node(operation, vec![]);
            result.kind = std::sync::Arc::new(Expression::Let { bindings: dimensions, body: Box::new(body) });
            return Ok(result);
        }
        let mut arguments = Vec::new();
        if operation == ArrayOperation::Make && args.len() == 1 {
            if let Some(size) = literal_size(&args[0].kind) {
                if !size.is_finite() || size > 1_000_000.0 {
                    return Err(fail(
                        args[0].span.clone(),
                        "Literal array size exceeds bootstrap compilation limit",
                    ));
                }
                operation = ArrayOperation::MakeLiteral;
                arguments.push(Hir {
                    source: None,
                    span: args[0].span.clone(),
                    metadata: vec![],
                    ty: Type::Number,
                    kind: std::sync::Arc::new(Expression::Literal(Literal::Long(size.ceil().max(0.0) as i64))),
                });
            }
        }
        if arguments.is_empty() {
            arguments = args
                .iter()
                .map(|arg| self.form(arg))
                .collect::<Result<Vec<_>, _>>()?;
        }
        Ok(node(operation, arguments))
    }
    fn array_temporary(&mut self, form: &Form, value: Hir) -> Binding {
        let id = BindingId(self.next);
        self.next += 1;
        Binding {
            id,
            name: String::new(),
            span: form.span.clone(),
            metadata: vec![],
            value,
        }
    }
}

/// A literal array size, long or double.
fn literal_size(kind: &Kind) -> Option<f64> {
    match kind {
        Kind::Long(size) => Some(*size as f64),
        Kind::Number(size) => Some(*size),
        _ => None,
    }
}

/// The packed element kind, in `packed_arrays::KINDS` order, that an array
/// expression's source tag names (#389).
fn packed_kind(array: &Hir) -> Option<usize> {
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
        _ => return None,
    })
}
