//! Original bounded bootstrap expansions needed by retained core source.
use super::*;
use crate::portable::resolve::ControlForm;

impl Analyzer {
    fn control_if(&self, form: &Form, condition: Hir, consequent: Hir, alternative: Hir) -> Hir {
        let ty = if consequent.ty == alternative.ty {
            consequent.ty
        } else {
            Type::Value
        };
        Hir {
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty,
            kind: Expression::If {
                condition: Box::new(condition),
                consequent: Box::new(consequent),
                alternative: Box::new(alternative),
            },
        }
    }
    fn control_test(&mut self, form: &Form) -> Result<Hir, Diagnostic> {
        // A literal keyword is always truthy. This does not materialize keywords
        // as source values; these macros use their test only for branching.
        if matches!(form.kind, Kind::Keyword(_)) {
            Ok(self.literal_form(form, Literal::Bool(true)))
        } else {
            self.form(form)
        }
    }
    pub(super) fn control_form(
        &mut self,
        form: &Form,
        args: &[Form],
        operation: ControlForm,
        statement: bool,
        tail: bool,
    ) -> Result<Hir, Diagnostic> {
        if args.len() > 256 {
            return Err(fail(
                form.span.clone(),
                "Control macro expansion limit exceeded",
            ));
        }
        let nil = self.literal_form(form, Literal::Nil);
        match operation {
            ControlForm::CachingHash => {
                if args.len() != 3 || !matches!(args[2].kind, Kind::Symbol(_)) {
                    return Err(fail(
                        form.span.clone(),
                        "caching-hash requires collection, hash function and a symbol cache key",
                    ));
                }
                // Pinned core.cljc1284 caches the first key read and tests nil,
                // including internal undefined. The hit branch evaluates neither
                // hash function nor collection syntax; false and zero are hits.
                let cached = self.form(&args[2])?;
                let cached_binding = self.fresh_binding(&args[2], cached);
                let cached = self.local(&args[2], cached_binding.id);
                let test = Hir {
                    span: args[2].span.clone(),
                    metadata: Vec::new(),
                    ty: Type::Bool,
                    kind: Expression::NilTest(Box::new(cached.clone())),
                };
                let callee = self.form(&args[1])?;
                let collection = self.form(&args[0])?;
                let hash = Hir {
                    span: form.span.clone(),
                    metadata: Vec::new(),
                    ty: Type::Value,
                    kind: Expression::Call {
                        callee: Box::new(callee),
                        arguments: vec![collection],
                    },
                };
                let hash_binding = self.fresh_binding(form, hash);
                let hash = self.local(form, hash_binding.id);
                let Kind::Symbol(key) = &args[2].kind else {
                    unreachable!()
                };
                let assignment = if key.namespace.is_none()
                    && !self.locals.contains_key(&key.name)
                    && self.fields.contains_key(&key.name)
                {
                    let (field, mutable) = self.fields[&key.name].clone();
                    if !mutable {
                        return Err(fail(
                            args[2].span.clone(),
                            "Cannot assign a local or immutable field",
                        ));
                    }
                    let Expression::Nominal {
                        operation,
                        mut arguments,
                    } = field.kind
                    else {
                        unreachable!()
                    };
                    arguments.push(hash.clone());
                    let setter = match operation {
                        Nominal::Field(index) => Nominal::FieldSet(index),
                        Nominal::NamedGet => Nominal::NamedSet,
                        _ => unreachable!(),
                    };
                    self.nominal(form, setter, arguments)
                } else {
                    let global = self.assignment_target(&args[2])?;
                    Hir {
                        span: form.span.clone(),
                        metadata: Vec::new(),
                        ty: Type::Value,
                        kind: Expression::Assign {
                            global,
                            value: Box::new(hash.clone()),
                        },
                    }
                };
                let miss_body = Hir {
                    span: form.span.clone(),
                    metadata: Vec::new(),
                    ty: Type::Value,
                    kind: Expression::Do(vec![assignment, hash]),
                };
                let miss = Hir {
                    span: form.span.clone(),
                    metadata: Vec::new(),
                    ty: Type::Value,
                    kind: Expression::Let {
                        bindings: vec![hash_binding],
                        body: Box::new(miss_body),
                    },
                };
                let body = self.control_if(form, test, miss, cached);
                Ok(Hir {
                    span: form.span.clone(),
                    metadata: form.metadata.clone(),
                    ty: body.ty,
                    kind: Expression::Let {
                        bindings: vec![cached_binding],
                        body: Box::new(body),
                    },
                })
            }
            ControlForm::Declare => {
                if !statement {
                    return Err(fail(
                        form.span.clone(),
                        "Declaration expression results are not certified yet; use a declaration statement",
                    ));
                }
                let mut definitions = Vec::new();
                for name in args {
                    let Kind::Symbol(symbol) = &name.kind else {
                        return Err(fail(name.span.clone(), "declare requires symbol names"));
                    };
                    if symbol.name == "&" {
                        return Err(fail(
                            name.span.clone(),
                            "declare requires ordinary symbol names",
                        ));
                    }
                    let mut declared = name.clone();
                    let syntax = |kind| Form {
                        span: name.span.clone(),
                        metadata: Vec::new(),
                        kind,
                    };
                    declared.metadata.push(syntax(Kind::Map(vec![
                        syntax(Kind::Keyword(suss_reader::Keyword::new("declared"))),
                        syntax(Kind::Bool(true)),
                    ])));
                    definitions.push(self.definition(form, &[declared], false)?);
                }
                Ok(Hir {
                    span: form.span.clone(),
                    metadata: form.metadata.clone(),
                    ty: Type::Nil,
                    kind: Expression::Do(definitions),
                })
            }
            ControlForm::When | ControlForm::WhenNot => {
                if args.is_empty() {
                    return Err(fail(form.span.clone(), "when/when-not require a test"));
                }
                let test = self.control_test(&args[0])?;
                let body = self.body(&args[1..], form.span.clone(), statement, tail)?;
                Ok(if operation == ControlForm::When {
                    self.control_if(form, test, body, nil)
                } else {
                    self.control_if(form, test, nil, body)
                })
            }
            ControlForm::IfNot => {
                if !(2..=3).contains(&args.len()) {
                    return Err(fail(
                        form.span.clone(),
                        "if-not requires two or three operands",
                    ));
                }
                let test = self.control_test(&args[0])?;
                let consequent = self.form_in(&args[1], statement, tail)?;
                let alternative = if args.len() == 3 {
                    self.form_in(&args[2], statement, tail)?
                } else {
                    nil
                };
                Ok(self.control_if(form, test, alternative, consequent))
            }
            ControlForm::Cond => {
                if args.len() % 2 != 0 {
                    return Err(fail(
                        form.span.clone(),
                        "cond requires an even number of forms",
                    ));
                }
                if args.is_empty() {
                    return Ok(nil);
                }
                let test = self.control_test(&args[0])?;
                let consequent = self.form_in(&args[1], statement, tail)?;
                let alternative =
                    self.control_form(form, &args[2..], operation, statement, tail)?;
                Ok(self.control_if(form, test, consequent, alternative))
            }
            ControlForm::And | ControlForm::Or => {
                if args.is_empty() {
                    return Ok(if operation == ControlForm::And {
                        self.literal_form(form, Literal::Bool(true))
                    } else {
                        nil
                    });
                }
                if args.len() == 1 {
                    return self.form_in(&args[0], statement, tail);
                }
                let value = self.form(&args[0])?;
                let id = BindingId(self.next);
                self.next += 1;
                let local = Hir {
                    span: args[0].span.clone(),
                    metadata: Vec::new(),
                    ty: value.ty,
                    kind: Expression::Local(id),
                };
                let rest = self.control_form(form, &args[1..], operation, statement, tail)?;
                let body = if operation == ControlForm::And {
                    self.control_if(form, local.clone(), rest, local)
                } else {
                    self.control_if(form, local.clone(), local, rest)
                };
                Ok(Hir {
                    span: form.span.clone(),
                    metadata: form.metadata.clone(),
                    ty: body.ty,
                    kind: Expression::Let {
                        bindings: vec![Binding {
                            id,
                            name: "bootstrap control temporary".into(),
                            span: args[0].span.clone(),
                            metadata: Vec::new(),
                            value,
                        }],
                        body: Box::new(body),
                    },
                })
            }
        }
    }
}
