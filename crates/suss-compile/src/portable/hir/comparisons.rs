//! Original expansion of the inline comparison macros.
use super::*;
impl Analyzer<'_> {
    pub(super) fn comparison_form(
        &mut self,
        form: &Form,
        args: &[Form],
        operation: Comparison,
    ) -> Result<Hir, Diagnostic> {
        self.expander.charge_expansion_work(args.len() as u64, form.span.clone())?;
        if args.is_empty() {
            return Err(fail(
                form.span.clone(),
                "Wrong bootstrap comparison macro arity",
            ));
        }
        if args.len() == 1 {
            // As in Clojure, a unary comparison evaluates its operand for
            // effects and is true without checking it (#294).
            let operand = self.form(&args[0])?;
            return Ok(Hir {
                source: None,
                span: form.span.clone(),
                metadata: form.metadata.clone(),
                ty: Type::Bool,
                kind: Expression::Do(vec![operand, self.literal_form(form, Literal::Bool(true))]),
            });
        }
        // Source comparisons are Clojure's numeric ones; core keeps the coercing
        // primitives its pinned string/array comparators rely on.
        let operation = if self.coercing_operands() { operation } else { operation.numeric() };
        let comparison = |left: Hir, right: Hir| Hir {
            source: None,
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Bool,
            kind: Expression::Comparison {
                operation,
                arguments: vec![left, right],
            },
        };
        if args.len() == 2 {
            return Ok(comparison(self.form(&args[0])?, self.form(&args[1])?));
        }
        // Clojure's comparison is a function: every operand is evaluated once, in
        // order, before the pairs are compared (#293). Pairs after the first
        // false one are not compared, so their operands are not checked.
        let mut bindings = Vec::with_capacity(args.len());
        for arg in args {
            let value = self.form(arg)?;
            bindings.push(self.fresh_binding(arg, value));
        }
        let operand = |binding: &Binding| {
            let mut local = self.local(form, binding.id);
            local.ty = binding.value.ty;
            local
        };
        let mut chain = comparison(
            operand(&bindings[args.len() - 2]),
            operand(&bindings[args.len() - 1]),
        );
        for pair in bindings.windows(2).rev().skip(1) {
            chain = Hir {
                source: None,
                span: form.span.clone(),
                metadata: form.metadata.clone(),
                ty: Type::Bool,
                kind: Expression::If {
                    condition: Box::new(comparison(operand(&pair[0]), operand(&pair[1]))),
                    consequent: Box::new(chain),
                    alternative: Box::new(self.literal_form(form, Literal::Bool(false))),
                },
            };
        }
        Ok(Hir {
            source: None,
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Bool,
            kind: Expression::Let {
                bindings,
                body: Box::new(chain),
            },
        })
    }
}
