//! Original bounded expansion following the pinned nested bitwise macros.
use super::*;
impl Analyzer {
    pub(super) fn bitwise_form(
        &mut self,
        form: &Form,
        args: &[Form],
        operation: Bitwise,
    ) -> Result<Hir, Diagnostic> {
        if args.len() > 256
            || (operation.variadic() && args.len() < 2)
            || (!operation.variadic() && args.len() != operation.arity())
        {
            return Err(fail(
                form.span.clone(),
                "Wrong bootstrap bitwise macro arity or expansion limit",
            ));
        }
        let mut arguments = Vec::new();
        for arg in &args[..operation.arity()] {
            arguments.push(self.form(arg)?);
        }
        let mut result = Hir {
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: operation.result(),
            kind: Expression::Bitwise {
                operation,
                arguments,
            },
        };
        for arg in &args[operation.arity()..] {
            let right = self.form(arg)?;
            result = Hir {
                span: form.span.clone(),
                metadata: form.metadata.clone(),
                ty: operation.result(),
                kind: Expression::Bitwise {
                    operation,
                    arguments: vec![result, right],
                },
            };
        }
        Ok(result)
    }
}
