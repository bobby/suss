//! Original bounded bootstrap expansions needed by retained core source.
use super::*;
use crate::portable::resolve::ControlForm;

impl Analyzer {
    pub(super) fn control_if(&self, form: &Form, condition: Hir, consequent: Hir, alternative: Hir) -> Hir {
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
            ControlForm::Case => self.case_form(form, args, statement, tail),
            ControlForm::UncheckedGet | ControlForm::UncheckedSet => {
                let operation = if operation == ControlForm::UncheckedGet { Nominal::NativeObjectGet } else { Nominal::NativeObjectSet };
                if !operation.valid(&vec![Type::Value; args.len()]) {
                    return Err(fail(form.span.clone(), "Invalid unchecked property macro arity"));
                }
                let operands = args.iter().map(|argument| self.form(argument)).collect::<Result<Vec<_>, _>>()?;
                Ok(self.nominal(form, operation, operands))
            }
            ControlForm::Increment | ControlForm::Decrement => {
                if args.len() != 1 {
                    return Err(fail(form.span.clone(), if operation == ControlForm::Increment {
                        "inc requires one operand"
                    } else { "dec requires one operand" }));
                }
                let value = self.form(&args[0])?;
                let operator = if operation == ControlForm::Increment {
                    Arithmetic::Add
                } else { Arithmetic::Subtract };
                let arguments = vec![value, self.literal_form(form, Literal::Number(1.0))];
                let ty = arithmetic_type(operator, &arguments.iter().map(|arg| arg.ty).collect::<Vec<_>>())
                    .ok_or_else(|| fail(args[0].span.clone(), "Arithmetic object coercions are not lowered yet"))?;
                Ok(Hir {
                    span: form.span.clone(),
                    metadata: form.metadata.clone(),
                    ty,
                    kind: Expression::Arithmetic { operator, arguments },
                })
            }
            ControlForm::Zero | ControlForm::Positive | ControlForm::Negative => {
                if args.len() != 1 {
                    return Err(fail(
                        form.span.clone(),
                        match operation { ControlForm::Zero => "zero? requires one operand", ControlForm::Negative => "neg? requires one operand", _ => "pos? requires one operand" },
                    ));
                }
                let value = self.form(&args[0])?;
                Ok(Hir {
                    span: form.span.clone(),
                    metadata: form.metadata.clone(),
                    ty: Type::Bool,
                    kind: Expression::Comparison {
                        operation: if operation == ControlForm::Zero {
                            Comparison::StrictEqual
                        } else if operation == ControlForm::Negative {
                            Comparison::Less
                        } else {
                            Comparison::Greater
                        },
                        arguments: vec![value, self.literal_form(form, Literal::Number(0.0))],
                    },
                })
            }
            ControlForm::ThreadFirst => {
                let Some(first) = args.first() else {
                    return Err(fail(form.span.clone(), "-> requires an initial expression"));
                };
                let mut threaded = first.clone();
                for step in &args[1..] {
                    let items = match &step.kind {
                        Kind::List(items) if !items.is_empty() => {
                            let mut next = vec![items[0].clone(), threaded];
                            next.extend_from_slice(&items[1..]);
                            next
                        }
                        Kind::List(_) => {
                            return Err(fail(step.span.clone(), "Threading step has no callee"))
                        }
                        _ => vec![step.clone(), threaded],
                    };
                    threaded = Form {
                        kind: Kind::List(items),
                        span: step.span.clone(),
                        metadata: if matches!(step.kind, Kind::List(_)) {
                            step.metadata.clone()
                        } else {
                            Vec::new()
                        },
                    };
                }
                self.form_in(&threaded, statement, tail)
            }
            ControlForm::AsThread => {
                if args.len() < 2 {
                    return Err(fail(
                        form.span.clone(),
                        "as-> requires expression and binding name",
                    ));
                }
                let Kind::Symbol(name) = &args[1].kind else {
                    return Err(fail(args[1].span.clone(), "as-> binding must be a symbol"));
                };
                if name.namespace.is_some() || name.name == "&" {
                    return Err(fail(
                        args[1].span.clone(),
                        "as-> binding must be unqualified",
                    ));
                }
                let name = name.name.clone();
                let outer = self.locals.clone();
                let result = (|| {
                    let mut bindings = Vec::new();
                    let value = self.form(&args[0])?;
                    let mut binding = self.fresh_binding(&args[1], value);
                    binding.name = name.clone();
                    binding.metadata = args[1].metadata.clone();
                    self.locals
                        .insert(name.clone(), (binding.id, binding.value.ty));
                    bindings.push(binding);
                    for step in args[2..].iter().take(args.len().saturating_sub(3)) {
                        let value = self.form(step)?;
                        let mut binding = self.fresh_binding(&args[1], value);
                        binding.name = name.clone();
                        binding.metadata = args[1].metadata.clone();
                        self.locals
                            .insert(name.clone(), (binding.id, binding.value.ty));
                        bindings.push(binding);
                    }
                    let body = if args.len() == 2 {
                        self.form_in(&args[1], statement, tail)?
                    } else {
                        self.form_in(args.last().unwrap(), statement, tail)?
                    };
                    Ok(Hir {
                        span: form.span.clone(),
                        metadata: form.metadata.clone(),
                        ty: body.ty,
                        kind: Expression::Let {
                            bindings,
                            body: Box::new(body),
                        },
                    })
                })();
                self.locals = outer;
                result
            }
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
            ControlForm::IfLet => {
                if !(2..=3).contains(&args.len()) {
                    return Err(fail(
                        form.span.clone(),
                        "if-let requires two or three operands",
                    ));
                }
                let Kind::Vector(entries) = &args[0].kind else {
                    return Err(fail(
                        args[0].span.clone(),
                        "if-let bindings must be a vector",
                    ));
                };
                if entries.len() != 2 {
                    return Err(fail(
                        args[0].span.clone(),
                        "if-let requires exactly one binding pair",
                    ));
                }
                let Kind::Symbol(name) = &entries[0].kind else {
                    return Err(fail(
                        entries[0].span.clone(),
                        "Binding destructuring is not lowered yet",
                    ));
                };
                if name.namespace.is_some() || name.name == "&" {
                    return Err(fail(
                        entries[0].span.clone(),
                        "Binding name must be unqualified",
                    ));
                }
                // The test runs once outside the source binding's scope.
                let test = self.form(&entries[1])?;
                let temporary = self.fresh_binding(&entries[1], test);
                let condition = self.local(form, temporary.id);
                let mut binding = self.fresh_binding(&entries[0], condition.clone());
                binding.name = name.name.clone();
                binding.metadata = entries[0].metadata.clone();
                let outer = self.locals.clone();
                self.locals
                    .insert(name.name.clone(), (binding.id, binding.value.ty));
                let consequent = self.form_in(&args[1], statement, tail);
                self.locals = outer;
                let consequent = consequent?;
                let consequent = Hir {
                    span: args[1].span.clone(),
                    metadata: args[1].metadata.clone(),
                    ty: consequent.ty,
                    kind: Expression::Let {
                        bindings: vec![binding],
                        body: Box::new(consequent),
                    },
                };
                let alternative = if args.len() == 3 {
                    self.form_in(&args[2], statement, tail)?
                } else {
                    nil
                };
                let body = self.control_if(form, condition, consequent, alternative);
                Ok(Hir {
                    span: form.span.clone(),
                    metadata: form.metadata.clone(),
                    ty: body.ty,
                    kind: Expression::Let {
                        bindings: vec![temporary],
                        body: Box::new(body),
                    },
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
