//! Original scalar bootstrap for pinned core.cljc case (2405–2474).
//! Full compiled macros, composite constants and no-default printing remain pending.
use super::*;
use std::collections::BTreeSet;

fn constant(form: &Form) -> Result<(Literal, (u8, Vec<u8>)), Diagnostic> {
    Ok(match &form.kind {
        Kind::Nil => (Literal::Nil, (0, vec![])),
        Kind::Bool(value) => (Literal::Bool(*value), (1, vec![u8::from(*value)])),
        Kind::Number(value) => {
            let bits = if *value == 0.0 {
                0
            } else if value.is_nan() {
                f64::NAN.to_bits()
            } else {
                value.to_bits()
            };
            (Literal::Number(*value), (2, bits.to_be_bytes().to_vec()))
        }
        Kind::String(units) => (
            Literal::String(units.clone()),
            (
                3,
                units.iter().flat_map(|unit| unit.to_be_bytes()).collect(),
            ),
        ),
        _ => {
            return Err(fail(
                form.span.clone(),
                "case bootstrap requires scalar literal constants",
            ))
        }
    })
}

impl Analyzer<'_> {
    pub(super) fn case_form(
        &mut self,
        form: &Form,
        args: &[Form],
        context: super::super::AnalysisContext,
        tail: bool,
    ) -> Result<Hir, Diagnostic> {
        if args.len() < 2 || args.len() % 2 != 0 {
            return Err(fail(
                form.span.clone(),
                "case bootstrap requires an expression and explicit default",
            ));
        }
        let mut seen = BTreeSet::new();
        let mut groups = vec![];
        for pair in args[1..args.len() - 1].chunks_exact(2) {
            let constants = match &pair[0].kind {
                Kind::List(constants) => constants.as_slice(),
                _ => std::slice::from_ref(&pair[0]),
            };
            if constants.is_empty() {
                return Err(fail(
                    pair[0].span.clone(),
                    "case bootstrap requires nonempty groups",
                ));
            }
            let mut group = vec![];
            for syntax in constants {
                let (value, key) = constant(syntax)?;
                if !seen.insert(key) {
                    return Err(fail(syntax.span.clone(), "Duplicate case test constant"));
                }
                if seen.len() > 256 {
                    return Err(fail(
                        form.span.clone(),
                        "Case constant expansion limit exceeded",
                    ));
                }
                group.push((syntax, value));
            }
            groups.push((group, &pair[1]));
        }
        let primitive = groups.iter().all(|(group, _)| {
            group
                .iter()
                .all(|(_, value)| matches!(value, Literal::Number(_) | Literal::String(_)))
        });
        if !primitive && seen.len() > 8 {
            return Err(fail(
                form.span.clone(),
                "Case equality bootstrap supports at most eight constants",
            ));
        }
        // Analyze user expressions in source order, then build branching from
        // the already analyzed arms. Selector effects happen exactly once even
        // with no clauses.
        let value = self.form(&args[0])?;
        let binding = self.fresh_binding(&args[0], value);
        let selected = self.local(&args[0], binding.id);
        // The pinned case macro binds its selector in a let before branching.
        let context = context.returning();
        let mut arms = vec![];
        for (group, body) in groups {
            arms.push((group, self.form_in(body, context, tail)?));
        }
        let mut body = self.form_in(args.last().unwrap(), context, tail)?;
        for (group, result) in arms.into_iter().rev() {
            let mut condition = self.literal_form(form, Literal::Bool(false));
            for (syntax, value) in group.into_iter().rev() {
                let comparison = if primitive {
                    Hir {
                        source: None,
                        span: syntax.span.clone(),
                        metadata: syntax.metadata.clone(),
                        ty: Type::Bool,
                        kind: Expression::Comparison {
                            operation: Comparison::StrictEqual,
                            arguments: vec![selected.clone(), self.literal_form(syntax, value)],
                        },
                    }
                } else {
                    // Pin's generic branch uses live qualified core =, with the
                    // test constant first. Keep that lookup inside each test.
                    let symbol = Form {
                        span: syntax.span.clone(),
                        metadata: vec![],
                        kind: Kind::Symbol(suss_reader::Symbol {
                            namespace: Some("suss.core".into()),
                            name: "=".into(),
                        }),
                    };
                    let callee = self.form(&symbol)?;
                    Hir {
                        source: None,
                        span: syntax.span.clone(),
                        metadata: syntax.metadata.clone(),
                        ty: Type::Value,
                        kind: Expression::Call {
                            callee: Box::new(callee),
                            arguments: vec![self.literal_form(syntax, value), selected.clone()],
                        },
                    }
                };
                condition = self.control_if(
                    form,
                    comparison,
                    self.literal_form(form, Literal::Bool(true)),
                    condition,
                );
            }
            body = self.control_if(form, condition, result, body);
        }
        Ok(Hir {
            source: None,
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: body.ty,
            kind: Expression::Let {
                bindings: vec![binding],
                body: Box::new(body),
            },
        })
    }
}
