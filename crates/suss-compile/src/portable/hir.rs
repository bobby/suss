//! Source-aware HIR for the replacement pipeline. No EDN conversion occurs.
use super::{
    Diagnostic,
    resolve::{Binding as ResolvedBinding, Environment, Global, Phase},
};
use std::{
    collections::{BTreeSet, HashMap},
    ops::Range,
};
use suss_reader::forms::{Form, Kind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Nil,
    Bool,
    Number,
    String,
    Value,
    Closure(usize),
}
impl Type {
    pub(crate) fn accepts(self, other: Self) -> bool {
        self == Self::Value || self == other
    }
}
#[derive(Debug, Clone)]
pub enum Literal {
    Nil,
    Bool(bool),
    Number(f64),
    String(Vec<u16>),
}
impl Literal {
    pub fn ty(&self) -> Type {
        match self {
            Self::Nil => Type::Nil,
            Self::Bool(_) => Type::Bool,
            Self::Number(_) => Type::Number,
            Self::String(_) => Type::String,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arithmetic {
    Add,
    Subtract,
    Multiply,
    Divide,
    Negate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BindingId(pub usize);
#[derive(Debug, Clone)]
pub struct Binding {
    pub id: BindingId,
    pub name: String,
    pub metadata: Vec<Form>,
    pub span: Range<usize>,
    pub value: Hir,
}
#[derive(Debug, Clone)]
pub struct Parameter {
    pub id: BindingId,
    pub name: String,
    pub metadata: Vec<Form>,
    pub span: Range<usize>,
}
#[derive(Debug, Clone)]
pub struct Hir {
    pub span: Range<usize>,
    pub metadata: Vec<Form>,
    pub ty: Type,
    pub kind: Expression,
}
#[derive(Debug, Clone)]
pub enum Expression {
    Literal(Literal),
    Local(BindingId),
    /// Read a live cell by resolved language identity, never by a Wasm index.
    Global(Global),
    Let {
        bindings: Vec<Binding>,
        body: Box<Hir>,
    },
    If {
        condition: Box<Hir>,
        consequent: Box<Hir>,
        alternative: Box<Hir>,
    },
    Do(Vec<Hir>),
    Function {
        parameters: Vec<Parameter>,
        captures: Vec<BindingId>,
        body: Box<Hir>,
    },
    Call {
        callee: Box<Hir>,
        arguments: Vec<Hir>,
    },
    /// Resolved bootstrap intrinsic identity, never an unresolved name.
    Arithmetic {
        operator: Arithmetic,
        arguments: Vec<Hir>,
    },
}
fn fail(span: Range<usize>, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        span,
        message: message.into(),
    }
}
struct Analyzer<'a> {
    environment: &'a Environment,
    phase: Phase,
    locals: HashMap<String, (BindingId, Type)>,
    next: usize,
}
impl Analyzer<'_> {
    fn body(&mut self, forms: &[Form], span: Range<usize>) -> Result<Hir, Diagnostic> {
        let items = forms
            .iter()
            .map(|form| self.form(form))
            .collect::<Result<Vec<_>, _>>()?;
        let ty = items.last().map_or(Type::Nil, |item| item.ty);
        Ok(Hir {
            span,
            metadata: Vec::new(),
            ty,
            kind: Expression::Do(items),
        })
    }
    fn form(&mut self, form: &Form) -> Result<Hir, Diagnostic> {
        let kind = match &form.kind {
            Kind::Nil => Expression::Literal(Literal::Nil),
            Kind::Bool(value) => Expression::Literal(Literal::Bool(*value)),
            Kind::Number(value) => Expression::Literal(Literal::Number(*value)),
            Kind::String(value) => Expression::Literal(Literal::String(value.clone())),
            Kind::Symbol(symbol) => {
                let (kind, ty) = if symbol.namespace.is_none() {
                    if let Some((id, ty)) = self.locals.get(&symbol.name) {
                        (Expression::Local(*id), *ty)
                    } else {
                        self.global_value(symbol, form.span.clone())?
                    }
                } else {
                    self.global_value(symbol, form.span.clone())?
                };
                return Ok(Hir {
                    span: form.span.clone(),
                    metadata: form.metadata.clone(),
                    ty,
                    kind,
                });
            }
            Kind::List(items) if !items.is_empty() => return self.list(form, items),
            _ => {
                return Err(fail(
                    form.span.clone(),
                    "Form requires namespace, macro, collection or closure lowering not yet implemented",
                ));
            }
        };
        let ty = match &kind {
            Expression::Literal(value) => value.ty(),
            _ => unreachable!(),
        };
        Ok(Hir {
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty,
            kind,
        })
    }
    fn global_value(
        &self,
        symbol: &suss_reader::Symbol,
        span: Range<usize>,
    ) -> Result<(Expression, Type), Diagnostic> {
        match self.environment.resolve(self.phase, symbol, span.clone())? {
            ResolvedBinding::Cell(global) => Ok((Expression::Global(global), Type::Value)),
            _ => Err(fail(
                span,
                "Bootstrap core function/macro values are not materialized yet",
            )),
        }
    }
    fn call(&mut self, form: &Form, items: &[Form]) -> Result<Hir, Diagnostic> {
        let callee = Box::new(self.form(&items[0])?);
        let arguments = items[1..]
            .iter()
            .map(|arg| self.form(arg))
            .collect::<Result<Vec<_>, _>>()?;
        if let Type::Closure(arity) = callee.ty {
            if arity != arguments.len() {
                return Err(fail(
                    form.span.clone(),
                    format!(
                        "Wrong arity: expected {arity}, received {}",
                        arguments.len()
                    ),
                ));
            }
        }
        Ok(Hir {
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Value,
            kind: Expression::Call { callee, arguments },
        })
    }
    fn function(
        &mut self,
        form: &Form,
        args: &[Form],
        bootstrap_macro: bool,
    ) -> Result<Hir, Diagnostic> {
        let Some(params) = args.first() else {
            return Err(fail(form.span.clone(), "fn requires a parameter vector"));
        };
        let Kind::Vector(names) = &params.kind else {
            return Err(fail(
                params.span.clone(),
                "Named/multiple-arity functions are not lowered yet; expected parameter vector",
            ));
        };
        // Pinned cljs.core/fn reads conditions from signature metadata as well
        // as a leading body map. fn* receives already-expanded syntax instead.
        if bootstrap_macro {
            for metadata in &params.metadata {
                let condition_key = |form: &Form| {
                    matches!(&form.kind, Kind::Keyword(key)
                        if key.namespace.is_none() && matches!(key.name.as_str(), "pre" | "post"))
                };
                if condition_key(metadata)
                    || matches!(&metadata.kind, Kind::Map(entries)
                        if entries.chunks_exact(2).any(|entry| condition_key(&entry[0])))
                {
                    return Err(fail(
                        params.span.clone(),
                        "Function pre/post conditions are not lowered yet",
                    ));
                }
            }
        }
        let outer = self.locals.clone();
        let mut parameters = Vec::new();
        for name in names {
            let Kind::Symbol(symbol) = &name.kind else {
                return Err(fail(
                    name.span.clone(),
                    "Parameter destructuring is not lowered yet",
                ));
            };
            if symbol.namespace.is_some() || symbol.name == "&" {
                return Err(fail(
                    name.span.clone(),
                    "Parameters must be unqualified; variadic rest sequences are not lowered yet",
                ));
            }
            let id = BindingId(self.next);
            self.next += 1;
            self.locals.insert(symbol.name.clone(), (id, Type::Value));
            parameters.push(Parameter {
                id,
                name: symbol.name.clone(),
                metadata: name.metadata.clone(),
                span: name.span.clone(),
            });
        }
        let body = Box::new(self.body(&args[1..], form.span.clone())?);
        self.locals = outer;
        let bound = parameters.iter().map(|parameter| parameter.id).collect();
        let mut captures = BTreeSet::new();
        free_bindings(&body, &bound, &mut captures);
        Ok(Hir {
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Closure(parameters.len()),
            kind: Expression::Function {
                parameters,
                captures: captures.into_iter().collect(),
                body,
            },
        })
    }
    fn list(&mut self, form: &Form, items: &[Form]) -> Result<Hir, Diagnostic> {
        let Kind::Symbol(symbol) = &items[0].kind else {
            return self.call(form, items);
        };
        let args = &items[1..];
        let bare = symbol.namespace.is_none();
        // Only true special forms bypass lexical and namespace resolution.
        let resolved = if bare && matches!(symbol.name.as_str(), "if" | "do" | "fn*") {
            None
        } else {
            if bare && self.locals.contains_key(&symbol.name) {
                return self.call(form, items);
            }
            Some(
                if let Some(binding) = self.environment.resolve_bootstrap_macro(self.phase, symbol)
                {
                    binding
                } else {
                    self.environment
                        .resolve(self.phase, symbol, items[0].span.clone())?
                },
            )
        };
        if (bare && symbol.name == "fn*")
            || matches!(resolved, Some(ResolvedBinding::BootstrapFn(_)))
        {
            return self.function(
                form,
                args,
                matches!(resolved, Some(ResolvedBinding::BootstrapFn(_))),
            );
        }
        let (kind, ty) = match (bare, symbol.name.as_str()) {
            (true, "do") => {
                let body = self.body(args, form.span.clone())?;
                (body.kind, body.ty)
            }
            (true, "if") => {
                if !(2..=3).contains(&args.len()) {
                    return Err(fail(form.span.clone(), "if requires two or three operands"));
                }
                let condition = Box::new(self.form(&args[0])?);
                let consequent = Box::new(self.form(&args[1])?);
                let alternative = Box::new(if args.len() == 3 {
                    self.form(&args[2])?
                } else {
                    Hir {
                        span: form.span.clone(),
                        metadata: Vec::new(),
                        ty: Type::Nil,
                        kind: Expression::Literal(Literal::Nil),
                    }
                });
                let ty = if consequent.ty == alternative.ty {
                    consequent.ty
                } else {
                    Type::Value
                };
                (
                    Expression::If {
                        condition,
                        consequent,
                        alternative,
                    },
                    ty,
                )
            }
            _ if matches!(resolved, Some(ResolvedBinding::BootstrapLet(_))) => {
                if args.is_empty() {
                    return Err(fail(form.span.clone(), "let requires a binding vector"));
                }
                let Kind::Vector(entries) = &args[0].kind else {
                    return Err(fail(args[0].span.clone(), "let bindings must be a vector"));
                };
                if entries.len() % 2 != 0 {
                    return Err(fail(args[0].span.clone(), "let binding has no initializer"));
                }
                let outer = self.locals.clone();
                let mut bindings = Vec::new();
                for pair in entries.chunks_exact(2) {
                    let Kind::Symbol(name) = &pair[0].kind else {
                        return Err(fail(
                            pair[0].span.clone(),
                            "Binding destructuring is not lowered yet",
                        ));
                    };
                    if name.namespace.is_some() || name.name == "&" {
                        return Err(fail(
                            pair[0].span.clone(),
                            "Binding name must be unqualified",
                        ));
                    }
                    let value = self.form(&pair[1])?;
                    let id = BindingId(self.next);
                    self.next += 1;
                    self.locals.insert(name.name.clone(), (id, value.ty));
                    bindings.push(Binding {
                        id,
                        name: name.name.clone(),
                        metadata: pair[0].metadata.clone(),
                        span: pair[0].span.clone(),
                        value,
                    });
                }
                let body = Box::new(self.body(&args[1..], form.span.clone())?);
                self.locals = outer;
                let ty = body.ty;
                (Expression::Let { bindings, body }, ty)
            }
            _ => {
                let Some(ResolvedBinding::Arithmetic { operator, .. }) = resolved else {
                    return self.call(form, items);
                };
                if args.is_empty() && matches!(operator, Arithmetic::Subtract | Arithmetic::Divide)
                {
                    return Err(fail(
                        form.span.clone(),
                        "Arithmetic call requires at least one argument",
                    ));
                }
                let arguments = args
                    .iter()
                    .map(|arg| self.form(arg))
                    .collect::<Result<Vec<_>, _>>()?;
                for arg in &arguments {
                    if arg.ty != Type::Number {
                        return Err(fail(
                            arg.span.clone(),
                            "Arithmetic requires verified Number operands; dynamic checking is not lowered yet",
                        ));
                    }
                }
                (
                    Expression::Arithmetic {
                        operator,
                        arguments,
                    },
                    Type::Number,
                )
            }
        };
        Ok(Hir {
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty,
            kind,
        })
    }
}
fn free_bindings(hir: &Hir, bound: &BTreeSet<BindingId>, free: &mut BTreeSet<BindingId>) {
    match &hir.kind {
        Expression::Local(id) => {
            if !bound.contains(id) {
                free.insert(*id);
            }
        }
        Expression::Function { captures, .. } => {
            for id in captures {
                if !bound.contains(id) {
                    free.insert(*id);
                }
            }
        }
        Expression::Let { bindings, body } => {
            let mut bound = bound.clone();
            for binding in bindings {
                free_bindings(&binding.value, &bound, free);
                bound.insert(binding.id);
            }
            free_bindings(body, &bound, free);
        }
        Expression::If {
            condition,
            consequent,
            alternative,
        } => {
            for item in [condition, consequent, alternative] {
                free_bindings(item, bound, free);
            }
        }
        Expression::Do(items)
        | Expression::Arithmetic {
            arguments: items, ..
        } => {
            for item in items {
                free_bindings(item, bound, free);
            }
        }
        Expression::Call { callee, arguments } => {
            free_bindings(callee, bound, free);
            for arg in arguments {
                free_bindings(arg, bound, free);
            }
        }
        _ => {}
    }
}
/// Analyze selected forms using an explicit phase-specific namespace environment.
pub fn analyze(forms: &[Form], span: Range<usize>) -> Result<Hir, Diagnostic> {
    analyze_in(forms, span, &Environment::default(), Phase::Runtime)
}
pub fn analyze_in(
    forms: &[Form],
    span: Range<usize>,
    environment: &Environment,
    phase: Phase,
) -> Result<Hir, Diagnostic> {
    Analyzer {
        environment,
        phase,
        locals: HashMap::new(),
        next: 0,
    }
    .body(forms, span)
}
