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
/// Primitive result information. Dynamic operands are checked/coerced at runtime;
/// object conversions remain an explicit unsupported boundary, not Number casts.
pub(crate) fn arithmetic_type(operator: Arithmetic, arguments: &[Type]) -> Option<Type> {
    if arguments.len() == 1 && matches!(operator, Arithmetic::Add | Arithmetic::Multiply) {
        return Some(arguments[0]);
    }
    if arguments.iter().any(|ty| matches!(ty, Type::Closure(_))) {
        return None;
    }
    if operator != Arithmetic::Add || arguments.is_empty() {
        return Some(Type::Number);
    }
    Some(arguments[1..].iter().fold(arguments[0], |left, &right| {
        if left == Type::String || right == Type::String {
            Type::String
        } else if [left, right]
            .iter()
            .all(|ty| matches!(ty, Type::Nil | Type::Bool | Type::Number))
        {
            Type::Number
        } else {
            Type::Value
        }
    }))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BindingId(pub usize);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LoopId(pub usize);
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
    Definition {
        global: Global,
        name_metadata: Vec<Form>,
        name_span: Range<usize>,
        docstring: Option<Vec<u16>>,
        initializer: Option<Box<Hir>>,
        once: bool,
    },
    Let {
        bindings: Vec<Binding>,
        body: Box<Hir>,
    },
    Loop {
        target: LoopId,
        bindings: Vec<Binding>,
        body: Box<Hir>,
    },
    Recur {
        target: LoopId,
        arguments: Vec<Hir>,
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
struct Analyzer {
    environment: Environment,
    phase: Phase,
    locals: HashMap<String, (BindingId, Type)>,
    next: usize,
    next_loop: usize,
    target: Option<(LoopId, usize)>,
}
impl Analyzer {
    fn body(
        &mut self,
        forms: &[Form],
        span: Range<usize>,
        statement: bool,
        tail: bool,
    ) -> Result<Hir, Diagnostic> {
        // Intermediate forms discard their result; only the final form inherits
        // its caller context. A fragment itself is a sequence of statements.
        let items = forms
            .iter()
            .enumerate()
            .map(|(index, form)| {
                self.form_in(
                    form,
                    statement || index + 1 < forms.len(),
                    tail && index + 1 == forms.len(),
                )
            })
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
        self.form_in(form, false, false)
    }
    fn form_in(&mut self, form: &Form, statement: bool, tail: bool) -> Result<Hir, Diagnostic> {
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
            Kind::List(items) if !items.is_empty() => {
                return self.list(form, items, statement, tail);
            }
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
        &mut self,
        symbol: &suss_reader::Symbol,
        span: Range<usize>,
    ) -> Result<(Expression, Type), Diagnostic> {
        match self.environment.resolve(self.phase, symbol, span.clone())? {
            ResolvedBinding::Cell(global) => Ok((Expression::Global(global), Type::Value)),
            ResolvedBinding::Arithmetic { global, .. } => {
                self.environment.materialize_arithmetic(global.clone());
                Ok((Expression::Global(global), Type::Value))
            }
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
    fn definition(&mut self, form: &Form, args: &[Form], once: bool) -> Result<Hir, Diagnostic> {
        if (once && args.len() != 2) || (!once && !(1..=3).contains(&args.len())) {
            return Err(fail(
                form.span.clone(),
                "Definition requires a name and optional initializer; defonce requires both",
            ));
        }
        let Kind::Symbol(name) = &args[0].kind else {
            return Err(fail(
                args[0].span.clone(),
                "Definition name must be a symbol",
            ));
        };
        for metadata in &args[0].metadata {
            let unsupported = |form: &Form| {
                matches!(&form.kind, Kind::Keyword(key)
                if key.namespace.is_none() && matches!(key.name.as_str(), "const" | "dynamic" | "private" | "macro" | "export"))
            };
            if unsupported(metadata)
                || matches!(&metadata.kind, Kind::Map(entries)
                if entries.chunks_exact(2).any(|entry| unsupported(&entry[0])))
            {
                return Err(fail(
                    args[0].span.clone(),
                    "Definition const/dynamic/private/macro/export attributes are not implemented yet",
                ));
            }
        }
        let namespace = self.environment.current_namespace(self.phase).to_owned();
        if name
            .namespace
            .as_deref()
            .is_some_and(|ns| ns != namespace && !(namespace == "suss.core" && ns == "cljs.core"))
        {
            return Err(fail(
                args[0].span.clone(),
                "Cannot define a name in another namespace",
            ));
        }
        let global = self
            .environment
            .declare_cell(self.phase, &namespace, &name.name)
            .map_err(|mut error| {
                error.span = args[0].span.clone();
                error
            })?;
        let init = if args.len() == 3 {
            if !matches!(args[1].kind, Kind::String(_)) {
                return Err(fail(
                    args[1].span.clone(),
                    "Definition docstring must be a string",
                ));
            }
            Some(&args[2])
        } else {
            args.get(1)
        };
        let initializer = init.map(|init| self.form(init).map(Box::new)).transpose()?;
        Ok(Hir {
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Value,
            kind: Expression::Definition {
                global,
                name_metadata: args[0].metadata.clone(),
                name_span: args[0].span.clone(),
                docstring: if args.len() == 3 {
                    match &args[1].kind {
                        Kind::String(units) => Some(units.clone()),
                        _ => unreachable!(),
                    }
                } else {
                    None
                },
                initializer,
                once,
            },
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
        let target = LoopId(self.next_loop);
        self.next_loop += 1;
        let outer_target = self.target.replace((target, parameters.len()));
        let mut bindings = Vec::new();
        for parameter in &parameters {
            let id = BindingId(self.next);
            self.next += 1;
            bindings.push(Binding {
                id,
                name: parameter.name.clone(),
                metadata: parameter.metadata.clone(),
                span: parameter.span.clone(),
                value: Hir {
                    span: parameter.span.clone(),
                    metadata: Vec::new(),
                    ty: Type::Value,
                    kind: Expression::Local(parameter.id),
                },
            });
            self.locals
                .insert(parameter.name.clone(), (id, Type::Value));
        }
        let inner_body = self.body(&args[1..], form.span.clone(), false, true)?;
        self.target = outer_target;
        let body = Box::new(Hir {
            span: form.span.clone(),
            metadata: Vec::new(),
            ty: inner_body.ty,
            kind: Expression::Loop {
                target,
                bindings,
                body: Box::new(inner_body),
            },
        });
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
    fn list(
        &mut self,
        form: &Form,
        items: &[Form],
        statement: bool,
        tail: bool,
    ) -> Result<Hir, Diagnostic> {
        let Kind::Symbol(symbol) = &items[0].kind else {
            return self.call(form, items);
        };
        let args = &items[1..];
        let bare = symbol.namespace.is_none();
        // Only true special forms bypass lexical and namespace resolution.
        let resolved = if bare
            && matches!(
                symbol.name.as_str(),
                "if" | "do" | "fn*" | "def" | "loop*" | "recur"
            ) {
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
        if bare && symbol.name == "recur" {
            let Some((target, arity)) = self.target else {
                return Err(fail(
                    form.span.clone(),
                    "recur requires an enclosing loop or function",
                ));
            };
            if !tail {
                return Err(fail(form.span.clone(), "recur must be in tail position"));
            }
            if args.len() != arity {
                return Err(fail(
                    form.span.clone(),
                    format!(
                        "recur arity mismatch: expected {arity}, received {}",
                        args.len()
                    ),
                ));
            }
            let arguments = args
                .iter()
                .map(|arg| self.form(arg))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(Hir {
                span: form.span.clone(),
                metadata: form.metadata.clone(),
                ty: Type::Value,
                kind: Expression::Recur { target, arguments },
            });
        }
        if bare && symbol.name == "def" {
            if args.len() == 1 && !statement {
                return Err(fail(
                    form.span.clone(),
                    "Initializerless def expression results are not certified yet; use a declaration statement",
                ));
            }
            return self.definition(form, args, false);
        }
        if matches!(resolved, Some(ResolvedBinding::BootstrapDefonce(_))) {
            return self.definition(form, args, true);
        }
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
                let body = self.body(args, form.span.clone(), statement, tail)?;
                (body.kind, body.ty)
            }
            (true, "if") => {
                if !(2..=3).contains(&args.len()) {
                    return Err(fail(form.span.clone(), "if requires two or three operands"));
                }
                let condition = Box::new(self.form(&args[0])?);
                let consequent = Box::new(self.form_in(&args[1], statement, tail)?);
                let alternative = Box::new(if args.len() == 3 {
                    self.form_in(&args[2], statement, tail)?
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
            _ if (bare && symbol.name == "loop*")
                || matches!(
                    resolved,
                    Some(ResolvedBinding::BootstrapLet(_) | ResolvedBinding::BootstrapLoop(_))
                ) =>
            {
                let is_loop = (bare && symbol.name == "loop*")
                    || matches!(resolved, Some(ResolvedBinding::BootstrapLoop(_)));
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
                    self.locals.insert(
                        name.name.clone(),
                        (id, if is_loop { Type::Value } else { value.ty }),
                    );
                    bindings.push(Binding {
                        id,
                        name: name.name.clone(),
                        metadata: pair[0].metadata.clone(),
                        span: pair[0].span.clone(),
                        value,
                    });
                }
                let target = LoopId(self.next_loop);
                let outer_target = self.target;
                if is_loop {
                    self.next_loop += 1;
                    self.target = Some((target, bindings.len()));
                }
                let body = Box::new(self.body(
                    &args[1..],
                    form.span.clone(),
                    statement,
                    is_loop || tail,
                )?);
                self.target = outer_target;
                self.locals = outer;
                let ty = body.ty;
                (
                    if is_loop {
                        Expression::Loop {
                            target,
                            bindings,
                            body,
                        }
                    } else {
                        Expression::Let { bindings, body }
                    },
                    ty,
                )
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
                let types = arguments.iter().map(|arg| arg.ty).collect::<Vec<_>>();
                let ty = arithmetic_type(operator, &types).ok_or_else(|| {
                    let operand = arguments
                        .iter()
                        .find(|arg| matches!(arg.ty, Type::Closure(_)))
                        .unwrap();
                    fail(
                        operand.span.clone(),
                        "Arithmetic object coercions are not lowered yet",
                    )
                })?;
                (
                    Expression::Arithmetic {
                        operator,
                        arguments,
                    },
                    ty,
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
        Expression::Definition {
            initializer: Some(value),
            ..
        } => free_bindings(value, bound, free),
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
        Expression::Let { bindings, body } | Expression::Loop { bindings, body, .. } => {
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
        Expression::Recur {
            arguments: items, ..
        }
        | Expression::Do(items)
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
    Ok(prepare(forms, span, environment, phase)?.0)
}
/// Analyze with a private environment snapshot; failure cannot mutate caller state.
pub(crate) fn prepare(
    forms: &[Form],
    span: Range<usize>,
    environment: &Environment,
    phase: Phase,
) -> Result<(Hir, Environment), Diagnostic> {
    let mut analyzer = Analyzer {
        environment: environment.clone(),
        phase,
        locals: HashMap::new(),
        next: 0,
        next_loop: 0,
        target: None,
    };
    let hir = analyzer.body(forms, span, true, false)?;
    Ok((hir, analyzer.environment))
}
