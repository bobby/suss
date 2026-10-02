//! Original bounded expansion of the pinned comparison macros.
use super::*;
impl Analyzer<'_> {
    pub(super) fn comparison_form(
        &mut self,
        form: &Form,
        args: &[Form],
        operation: Comparison,
    ) -> Result<Hir, Diagnostic> {
        if args.len() > 256 {
            return Err(fail(
                form.span.clone(),
                "Comparison macro expansion limit exceeded",
            ));
        }
        if args.is_empty() {
            return Err(fail(
                form.span.clone(),
                "Wrong bootstrap comparison macro arity",
            ));
        }
        if args.len() == 1 {
            // The pin erases unary operand syntax entirely, including throws.
            return Ok(self.literal_form(form, Literal::Bool(true)));
        }
        let comparison = Hir {
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Bool,
            kind: Expression::Comparison {
                operation,
                arguments: vec![self.form(&args[0])?, self.form(&args[1])?],
            },
        };
        if args.len() == 2 {
            return Ok(comparison);
        }
        // Expand the remaining syntax afresh: the pin repeats middle expressions,
        // while the If avoids effects in comparisons after the first false pair.
        let rest = self.comparison_form(form, &args[1..], operation)?;
        Ok(Hir {
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Bool,
            kind: Expression::If {
                condition: Box::new(comparison),
                consequent: Box::new(rest),
                alternative: Box::new(self.literal_form(form, Literal::Bool(false))),
            },
        })
    }
}
