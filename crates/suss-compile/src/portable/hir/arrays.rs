//! Bounded adaptations of the pin's array macro expansions.
use super::*;
impl Analyzer {
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
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Value,
            kind: Expression::Array {
                operation,
                arguments,
            },
        };
        if operation == ArrayOperation::Get || operation == ArrayOperation::Set {
            let mut current = self.form(&args[0])?;
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
            let value = if let Kind::Number(size) = args[1].kind {
                if !size.is_finite() || size > 1_000_000.0 {
                    return Err(fail(
                        args[1].span.clone(),
                        "Literal array size exceeds bootstrap compilation limit",
                    ));
                }
                Hir {
                    span: args[1].span.clone(),
                    metadata: vec![],
                    ty: Type::Number,
                    kind: Expression::Literal(Literal::Number(size.ceil().max(0.0))),
                }
            } else {
                self.form(&args[1])?
            };
            let size = self.array_temporary(&args[1], value);
            let nil = Hir {
                span: form.span.clone(),
                metadata: vec![],
                ty: Type::Nil,
                kind: Expression::Literal(Literal::Nil),
            };
            let mut operands = vec![nil, self.local(form, size.id)];
            operands.extend(
                dimensions
                    .iter()
                    .map(|binding| self.local(form, binding.id)),
            );
            let body = node(operation, operands);
            dimensions.push(size);
            return Ok(Hir {
                kind: Expression::Let {
                    bindings: dimensions,
                    body: Box::new(body),
                },
                ..node(operation, vec![])
            });
        }
        let mut arguments = Vec::new();
        if operation == ArrayOperation::Make && args.len() == 1 {
            if let Kind::Number(size) = args[0].kind {
                if !size.is_finite() || size > 1_000_000.0 {
                    return Err(fail(
                        args[0].span.clone(),
                        "Literal array size exceeds bootstrap compilation limit",
                    ));
                }
                operation = ArrayOperation::MakeLiteral;
                arguments.push(Hir {
                    span: args[0].span.clone(),
                    metadata: vec![],
                    ty: Type::Number,
                    kind: Expression::Literal(Literal::Number(size.ceil().max(0.0))),
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
