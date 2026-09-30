//! Source-aware HIR for the replacement pipeline. No EDN conversion occurs.
use super::Diagnostic;
use std::{collections::HashMap, ops::Range};
use suss_reader::forms::{Form, Kind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Nil,
    Bool,
    Number,
    String,
    Value,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
struct Analyzer {
    locals: HashMap<String, (BindingId, Type)>,
    next: usize,
}
impl Analyzer {
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
            Kind::Symbol(symbol) if symbol.namespace.is_none() => {
                let (id, ty) = self.locals.get(&symbol.name).ok_or_else(|| {
                    fail(
                        form.span.clone(),
                        format!("Unresolved name {}", symbol.name),
                    )
                })?;
                return Ok(Hir {
                    span: form.span.clone(),
                    metadata: form.metadata.clone(),
                    ty: *ty,
                    kind: Expression::Local(*id),
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
    fn list(&mut self, form: &Form, items: &[Form]) -> Result<Hir, Diagnostic> {
        let Kind::Symbol(symbol) = &items[0].kind else {
            return Err(fail(
                items[0].span.clone(),
                "Computed callees require closure lowering",
            ));
        };
        let args = &items[1..];
        let bare = symbol.namespace.is_none();
        // if/do are special forms; let is a bootstrap macro hidden by locals.
        if bare && symbol.name == "let" && self.locals.contains_key(&symbol.name) {
            return Err(fail(
                items[0].span.clone(),
                "Calling a local binding requires closure lowering",
            ));
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
            (true, "let") => {
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
                if bare && self.locals.contains_key(&symbol.name) {
                    return Err(fail(
                        items[0].span.clone(),
                        "Calling a local binding requires closure lowering",
                    ));
                }
                let core =
                    bare || matches!(symbol.namespace.as_deref(), Some("suss.core" | "cljs.core"));
                let operator = match (core, symbol.name.as_str()) {
                    (true, "+") => Arithmetic::Add,
                    (true, "-") => Arithmetic::Subtract,
                    (true, "*") => Arithmetic::Multiply,
                    (true, "/") => Arithmetic::Divide,
                    _ => {
                        return Err(fail(
                            items[0].span.clone(),
                            format!("Unresolved callable {}", symbol),
                        ));
                    }
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
/// Analyze selected forms. Namespace/macro expansion is still a later integration.
pub fn analyze(forms: &[Form], span: Range<usize>) -> Result<Hir, Diagnostic> {
    Analyzer {
        locals: HashMap::new(),
        next: 0,
    }
    .body(forms, span)
}
