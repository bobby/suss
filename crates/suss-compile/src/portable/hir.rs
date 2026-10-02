//! Source-aware HIR for the replacement pipeline. No EDN conversion occurs.
mod arrays;
mod bitwise;
mod cases;
mod collections;
mod comparisons;
mod controls;
mod dynamic;
mod exceptions;
mod nominal;
mod quotes;
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
    /// Internal bootstrap result; there is no undefined source literal.
    Undefined,
    Bool(bool),
    Number(f64),
    String(Vec<u16>),
}
impl Literal {
    pub fn ty(&self) -> Type {
        match self {
            Self::Nil => Type::Nil,
            Self::Undefined => Type::Value,
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
/// Primitive receiver kinds used only by the native protocol bootstrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeKind {
    Nil,
    Boolean,
    Number,
    String,
    Function,
    Object,
    Array,
    Default,
}
impl NativeKind {
    pub(crate) fn index(self) -> i32 {
        self as i32
    }
    pub(crate) fn from_form(form: &Form) -> Option<Self> {
        match &form.kind {
            Kind::Nil => Some(Self::Nil),
            Kind::Symbol(symbol) if symbol.namespace.is_none() => match symbol.name.as_str() {
                "boolean" => Some(Self::Boolean),
                "number" => Some(Self::Number),
                "string" => Some(Self::String),
                "function" => Some(Self::Function),
                "object" => Some(Self::Object),
                "array" => Some(Self::Array),
                "default" => Some(Self::Default),
                _ => None,
            },
            _ => None,
        }
    }
}
/// Bounded array macro operations; operands are evaluated once before execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayOperation {
    Literal,
    Make,
    MakeLiteral,
    Length,
    Get,
    Set,
}
impl ArrayOperation {
    pub(crate) fn export(self) -> &'static str {
        match self {
            Self::Literal => "source-array-new",
            Self::Make => "source-array-make",
            Self::MakeLiteral => "source-array-make-literal",
            Self::Length => "source-array-length-args",
            Self::Get => "source-array-get-indices",
            Self::Set => "source-array-set-indices",
        }
    }
    pub(crate) fn valid(self, count: usize) -> bool {
        count <= i32::MAX as usize
            && match self {
                Self::Literal => true,
                Self::Make => count >= 1,
                Self::MakeLiteral | Self::Length => count == 1,
                Self::Get => count >= 2,
                Self::Set => count >= 3,
            }
    }
}
/// Original checked binary comparison primitive used after bounded macro expansion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    StrictEqual,
}
impl Comparison {
    pub(crate) fn export(self) -> &'static str {
        match self {
            Self::Less => "comparison-less",
            Self::LessEqual => "comparison-less-equal",
            Self::Greater => "comparison-greater",
            Self::GreaterEqual => "comparison-greater-equal",
            Self::StrictEqual => "comparison-strict-equal",
        }
    }
}
/// Checked scalar bitwise primitive; direct macro folds retain nested evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bitwise {
    Int,
    And,
    Or,
    Xor,
    AndNot,
    Not,
    Clear,
    Flip,
    Set,
    Test,
    Left,
    Right,
    Unsigned,
    Imul,
    /// Private scalar binary64 storage adapters; never public bitwise macros.
    F64Coerce,
    F64Word0,
    F64Word4,
    F64Floor,
    F64Finite,
    F64SafeInteger,
    F64TimeClip,
    SafeIntegerRemainder,
    F64Remainder,
    IdentityUid,
}
impl Bitwise {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "int" => Self::Int,
            "bit-and" => Self::And,
            "bit-or" => Self::Or,
            "bit-xor" => Self::Xor,
            "bit-and-not" => Self::AndNot,
            "bit-not" => Self::Not,
            "bit-clear" => Self::Clear,
            "bit-flip" => Self::Flip,
            "bit-set" => Self::Set,
            "bit-test" => Self::Test,
            "bit-shift-left" => Self::Left,
            "bit-shift-right" => Self::Right,
            "unsigned-bit-shift-right" => Self::Unsigned,
            "bit-shift-right-zero-fill" => Self::Unsigned,
            _ => return None,
        })
    }
    pub(crate) fn export(self) -> &'static str {
        match self {
            Self::Int => "primitive-int",
            Self::And => "primitive-bit-and",
            Self::Or => "primitive-bit-or",
            Self::Xor => "primitive-bit-xor",
            Self::AndNot => "primitive-bit-and-not",
            Self::Not => "primitive-bit-not",
            Self::Clear => "primitive-bit-clear",
            Self::Flip => "primitive-bit-flip",
            Self::Set => "primitive-bit-set",
            Self::Test => "primitive-bit-test",
            Self::Left => "primitive-bit-shift-left",
            Self::Right => "primitive-bit-shift-right",
            Self::Unsigned => "primitive-unsigned-bit-shift-right",
            Self::Imul => "primitive-imul",
            Self::F64Coerce => "primitive-f64-coerce",
            Self::F64Word0 => "primitive-f64-word0",
            Self::F64Word4 => "primitive-f64-word4",
            Self::F64Floor => "primitive-f64-floor",
            Self::F64Finite => "primitive-f64-finite",
            Self::F64SafeInteger => "primitive-f64-safe-integer",
            Self::F64TimeClip => "primitive-f64-time-clip",
            Self::SafeIntegerRemainder => "primitive-safe-integer-remainder",
            Self::F64Remainder => "primitive-f64-remainder",
            Self::IdentityUid => "identity-uid",
        }
    }
    pub(crate) fn arity(self) -> usize {
        match self {
            Self::Int
            | Self::Not
            | Self::F64Coerce
            | Self::F64Word0
            | Self::F64Word4
            | Self::F64Floor
            | Self::F64Finite
            | Self::F64SafeInteger
            | Self::F64TimeClip
            | Self::IdentityUid => 1,
            _ => 2,
        }
    }
    pub(crate) fn variadic(self) -> bool {
        matches!(self, Self::And | Self::Or | Self::Xor | Self::AndNot)
    }
    pub(crate) fn result(self) -> Type {
        if matches!(self, Self::Test | Self::F64Finite | Self::F64SafeInteger) {
            Type::Bool
        } else {
            Type::Number
        }
    }
}
/// Original private nominal lowering operations. Arrays here are internal
/// construction storage, never source-language persistent collections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nominal {
    LiveDispatcher,
    IFnLiveDispatcher,
    NativeMarker(NativeKind),
    NativeSet(NativeKind),
    Array,
    Descriptor,
    Class,
    Protocol,
    Construct,
    Instance,
    Field(usize),
    FieldSet(usize),
    NamedGet,
    NamedSet,
    NativeObjectFactory,
    NativeObjectDefaultPrototype,
    NativeObjectGet,
    NativeObjectSet,
    NativeObjectStrictSet,
    LanguageError,
    ObjectSet,
    ObjectInvoke,
    Key(usize),
    Dispatcher,
    Set,
    Marker,
    Satisfies,
    NativeSatisfies,
}
impl Nominal {
    pub fn result(self) -> Type {
        match self {
            Self::Instance | Self::Satisfies | Self::NativeSatisfies | Self::NativeMarker(_) => {
                Type::Bool
            }
            _ => Type::Value,
        }
    }
    pub(crate) fn valid(self, arguments: &[Type]) -> bool {
        let count = arguments.len();
        count <= i32::MAX as usize
            && match self {
                Self::Array => true,
                Self::NativeObjectFactory | Self::NativeObjectDefaultPrototype => count == 0,
                Self::LanguageError => count == 1,
                Self::NativeObjectGet => count == 2,
                Self::NativeObjectSet | Self::NativeObjectStrictSet => count == 3,
                Self::NamedGet => count == 2 && arguments[1] == Type::String,
                Self::NamedSet | Self::ObjectSet => count == 3 && arguments[1] == Type::String,
                Self::ObjectInvoke => count >= 2,
                Self::LiveDispatcher | Self::NativeSet(_) => count == 2,
                Self::IFnLiveDispatcher => count == 3,
                Self::NativeMarker(_) => count == 1,
                Self::Descriptor => arguments.iter().all(|ty| *ty == Type::String),
                Self::Construct => count >= 1,
                Self::Instance | Self::Satisfies | Self::NativeSatisfies | Self::Marker => {
                    count == 2
                }
                Self::Set => count == 3,
                Self::Field(index) | Self::Key(index) => count == 1 && index <= i32::MAX as usize,
                Self::FieldSet(index) => count == 2 && index <= i32::MAX as usize,
                Self::Class | Self::Protocol | Self::Dispatcher => count == 1,
            }
    }
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
pub struct Method {
    pub variadic: bool,
    pub parameters: Vec<Parameter>,
    pub body: Box<Hir>,
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
    Bitwise {
        operation: Bitwise,
        arguments: Vec<Hir>,
    },
    Comparison {
        operation: Comparison,
        arguments: Vec<Hir>,
    },
    /// Compiler bootstrap primitive, independent of mutable core vars.
    NilTest(Box<Hir>),
    Array {
        operation: ArrayOperation,
        arguments: Vec<Hir>,
    },
    DynamicScope {
        bindings: Vec<(Global, Hir)>,
        body: Box<Hir>,
    },
    Assign {
        global: Global,
        value: Box<Hir>,
    },
    /// A terminal language throw of an evaluated portable value.
    Throw(Box<Hir>),
    /// Private compiled regions: body/cleanup arity0, handler arity1 or nil.
    Try {
        regions: [Box<Hir>; 3],
    },
    Nominal {
        operation: Nominal,
        arguments: Vec<Hir>,
    },
    Literal(Literal),
    Local(BindingId),
    /// Read a live cell by resolved language identity, never by a Wasm index.
    Global(Global),
    /// Private cell identity for a captured live protocol fallback.
    GlobalCell(Global),
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
    GeneralFunction {
        methods: Vec<Method>,
        captures: Vec<BindingId>,
        self_binding: Option<Parameter>,
        rest_class: Option<Global>,
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
    expander: &'a mut dyn super::ExpansionHost,
    environment: Environment,
    phase: Phase,
    locals: HashMap<String, (BindingId, Type)>,
    fields: HashMap<String, (Hir, bool)>,
    next: usize,
    next_loop: usize,
    analysis_depth: usize,
    target: Option<(LoopId, usize)>,
}
impl Analyzer<'_> {
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
        // Bounded bootstrap expansion can create deeper syntax than the reader
        // saw (notably nested threading). Check total analysis depth, including
        // nested macros, before recursive analyzer frames exhaust the stack.
        if self.analysis_depth >= 24 {
            return Err(fail(
                form.span.clone(),
                "Bootstrap analysis expansion limit exceeded",
            ));
        }
        self.analysis_depth += 1;
        let result = self.expand_or_analyze(form, statement, tail);
        self.analysis_depth -= 1;
        result
    }
    fn expand_or_analyze(
        &mut self,
        form: &Form,
        statement: bool,
        tail: bool,
    ) -> Result<Hir, Diagnostic> {
        if let Kind::List(items) = &form.kind {
            if let Some(Form {
                kind: Kind::Symbol(symbol),
                ..
            }) = items.first()
            {
                let shadowed = symbol.namespace.is_none()
                    && (self.locals.contains_key(&symbol.name)
                        || self.fields.contains_key(&symbol.name));
                if !shadowed {
                    let context = super::ExpansionContext {
                        environment: &self.environment,
                        phase: self.phase,
                        locals: &self.locals,
                    };
                    if let Some(expanded) = self.expander.expand(form, context)? {
                        return self.form_in(&expanded, statement, tail);
                    }
                }
            }
        }
        self.form_inner(form, statement, tail)
    }
    fn form_inner(&mut self, form: &Form, statement: bool, tail: bool) -> Result<Hir, Diagnostic> {
        let kind = match &form.kind {
            Kind::Nil => Expression::Literal(Literal::Nil),
            Kind::Bool(value) => Expression::Literal(Literal::Bool(*value)),
            Kind::Number(value) => Expression::Literal(Literal::Number(*value)),
            Kind::String(value) => Expression::Literal(Literal::String(value.clone())),
            Kind::Keyword(value) => {
                return self.identifier_literal(
                    form,
                    value.namespace.as_deref(),
                    &value.name,
                    true,
                );
            }
            Kind::Vector(items) => {
                let value = self.vector_literal(form, items)?;
                return self.attach_literal_metadata(form, value);
            }
            Kind::Map(items) => {
                let value = self.map_literal(form, items)?;
                return self.attach_literal_metadata(form, value);
            }
            Kind::Set(items) => {
                let value = self.set_literal(form, items)?;
                return self.attach_literal_metadata(form, value);
            }
            Kind::List(items) if items.is_empty() => {
                // The pinned emitter reads List.EMPTY for each empty-list literal.
                // Resolve the canonical core binding, never a lexical/user List.
                let symbol = suss_reader::Symbol {
                    namespace: Some("suss.core".into()),
                    name: "List".into(),
                };
                let (kind, ty) = self.global_value(&symbol, form.span.clone())?;
                let owner = Hir {
                    span: form.span.clone(),
                    metadata: Vec::new(),
                    ty,
                    kind,
                };
                let key =
                    self.literal_form(form, Literal::String("EMPTY".encode_utf16().collect()));
                let value = self.nominal(form, Nominal::NamedGet, vec![owner, key]);
                return self.attach_constant_metadata(form, value);
            }
            Kind::Symbol(symbol) => {
                let (kind, ty) = if symbol.namespace.is_none() {
                    if let Some((id, ty)) = self.locals.get(&symbol.name) {
                        (Expression::Local(*id), *ty)
                    } else if let Some((field, _)) = self.fields.get(&symbol.name) {
                        (field.kind.clone(), field.ty)
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
            ResolvedBinding::Arithmetic { global, .. } | ResolvedBinding::Core { global, .. } => {
                self.environment.materialize_bootstrap(global.clone());
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
                if key.namespace.is_none() && matches!(key.name.as_str(), "const" | "private" | "macro" | "export"))
            };
            if unsupported(metadata)
                || matches!(&metadata.kind, Kind::Map(entries)
                if entries.chunks_exact(2).any(|entry| unsupported(&entry[0])))
            {
                return Err(fail(
                    args[0].span.clone(),
                    "Definition const/private/macro/export attributes are not implemented yet",
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
        if args
            .first()
            .is_some_and(|arg| matches!(&arg.kind, Kind::Vector(names)
                if !names.iter().any(|name| matches!(&name.kind, Kind::Symbol(symbol) if symbol.namespace.is_none() && symbol.name == "&"))))
        {
            return self.fixed_function(form, args, bootstrap_macro);
        }
        let outer = self.locals.clone();
        let (self_binding, signatures) = if let Some(Form {
            kind: Kind::Symbol(name),
            ..
        }) = args.first()
        {
            if name.namespace.is_some() || name.name == "&" {
                return Err(fail(
                    args[0].span.clone(),
                    "Function name must be unqualified",
                ));
            }
            let id = BindingId(self.next);
            self.next += 1;
            self.locals.insert(name.name.clone(), (id, Type::Value));
            (
                Some(Parameter {
                    id,
                    name: name.name.clone(),
                    metadata: args[0].metadata.clone(),
                    span: args[0].span.clone(),
                }),
                &args[1..],
            )
        } else {
            (None, args)
        };
        let mut methods = Vec::new();
        let mut captures = BTreeSet::new();
        if signatures
            .first()
            .is_some_and(|arg| matches!(arg.kind, Kind::Vector(_)))
        {
            let (method, free) = self.general_method(form, signatures, bootstrap_macro)?;
            captures.extend(free);
            methods.push(method);
        } else {
            for signature in signatures {
                let Kind::List(items) = &signature.kind else {
                    return Err(fail(
                        signature.span.clone(),
                        "Function signature requires a parameter vector and body",
                    ));
                };
                let (method, free) = self.general_method(signature, items, bootstrap_macro)?;
                captures.extend(free);
                methods.push(method);
            }
        }
        // The pinned compiler warns on duplicate fixed arities and executes the last body.
        methods.reverse();
        let mut arities = BTreeSet::new();
        methods.retain(|method| arities.insert((method.parameters.len(), method.variadic)));
        methods.reverse();
        let variadic: Vec<_> = methods.iter().filter(|method| method.variadic).collect();
        if variadic.len() > 1
            || variadic.first().is_some_and(|method| {
                methods.iter().any(|fixed| {
                    !fixed.variadic && fixed.parameters.len() > method.parameters.len() - 1
                })
            })
        {
            return Err(fail(
                form.span.clone(),
                "Function requires one variadic signature with no larger fixed arity",
            ));
        }
        let rest_class = if variadic.is_empty() {
            None
        } else {
            Some(
                self.environment
                    .resolve(
                        self.phase,
                        &suss_reader::Symbol {
                            namespace: Some("suss.core".into()),
                            name: "IndexedSeq".into(),
                        },
                        form.span.clone(),
                    )?
                    .global()
                    .clone(),
            )
        };
        captures.clear();
        for method in &methods {
            let mut bound: BTreeSet<_> = method
                .parameters
                .iter()
                .map(|parameter| parameter.id)
                .collect();
            if let Some(parameter) = &self_binding {
                bound.insert(parameter.id);
            }
            free_bindings(&method.body, &bound, &mut captures);
        }
        self.locals = outer;
        if methods.is_empty() {
            return Err(fail(
                form.span.clone(),
                "Function requires at least one signature",
            ));
        }
        if let Some(parameter) = &self_binding {
            captures.remove(&parameter.id);
        }
        Ok(Hir {
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Value,
            kind: Expression::GeneralFunction {
                methods,
                captures: captures.into_iter().collect(),
                self_binding,
                rest_class,
            },
        })
    }
    fn general_method(
        &mut self,
        form: &Form,
        args: &[Form],
        bootstrap_macro: bool,
    ) -> Result<(Method, Vec<BindingId>), Diagnostic> {
        let mut args = args.to_vec();
        let Some(Form {
            kind: Kind::Vector(names),
            ..
        }) = args.first_mut()
        else {
            return Err(fail(
                form.span.clone(),
                "Function requires a parameter vector",
            ));
        };
        let markers: Vec<_> = names.iter().enumerate().filter_map(|(index, name)|
            matches!(&name.kind, Kind::Symbol(symbol) if symbol.namespace.is_none() && symbol.name == "&").then_some(index)).collect();
        let variadic = !markers.is_empty();
        if let Some(&index) = markers.first() {
            if markers.len() != 1 || index + 2 != names.len() {
                return Err(fail(
                    form.span.clone(),
                    "Variadic signature requires exactly one trailing rest parameter",
                ));
            }
            names.remove(index);
        }
        let method = self.fixed_function(form, &args, bootstrap_macro)?;
        let Expression::Function {
            parameters,
            body,
            captures,
        } = method.kind
        else {
            unreachable!()
        };
        Ok((
            Method {
                parameters,
                body,
                variadic,
            },
            captures,
        ))
    }
    fn fixed_function(
        &mut self,
        form: &Form,
        args: &[Form],
        bootstrap_macro: bool,
    ) -> Result<Hir, Diagnostic> {
        self.fixed_function_fields(form, args, bootstrap_macro, &[], false, false)
    }
    fn fixed_function_fields(
        &mut self,
        form: &Form,
        args: &[Form],
        bootstrap_macro: bool,
        fields: &[Form],
        method_receiver: bool,
        object_method: bool,
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
        if object_method && names.is_empty() {
            return Err(fail(
                params.span.clone(),
                "Object method requires a receiver",
            ));
        }
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
        let outer_fields = self.fields.clone();
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
        let outer_target = self
            .target
            .replace((target, parameters.len() - usize::from(object_method)));
        let mut bindings = Vec::new();
        for parameter in parameters.iter().skip(usize::from(object_method)) {
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
        if method_receiver {
            let parameter = parameters
                .first()
                .ok_or_else(|| fail(form.span.clone(), "Protocol method requires a receiver"))?;
            // The pin anchors a method's physical receiver across recur. The
            // first recur operand still evaluates, but does not replace `this`.
            self.locals
                .insert(parameter.name.clone(), (parameter.id, Type::Value));
        }
        if !fields.is_empty() {
            let receiver = parameters
                .first()
                .ok_or_else(|| fail(form.span.clone(), "Protocol method requires a receiver"))?
                .id;
            for (index, field) in fields.iter().enumerate() {
                let Kind::Symbol(symbol) = &field.kind else {
                    unreachable!()
                };
                if parameters
                    .iter()
                    .any(|parameter| parameter.name == symbol.name)
                {
                    continue;
                }
                self.locals.remove(&symbol.name);
                let value = if object_method {
                    let key = self
                        .literal_form(field, Literal::String(symbol.name.encode_utf16().collect()));
                    self.nominal(
                        field,
                        Nominal::NamedGet,
                        vec![self.local(field, receiver), key],
                    )
                } else {
                    self.nominal(
                        field,
                        Nominal::Field(index),
                        vec![self.local(field, receiver)],
                    )
                };
                self.fields
                    .insert(symbol.name.clone(), (value, Self::field_mutable(field)?));
            }
        }
        // Field reads occur at the original use, including in nested closures;
        // unreferenced fields must not introduce checks or effects before a body.
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
        self.fields = outer_fields;
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
    fn property_name(form: &Form, operator: &str) -> Result<String, Diagnostic> {
        let name = &operator[2..];
        if !name
            .bytes()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_' || c == b'$')
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'$')
        {
            return Err(fail(
                form.span.clone(),
                "Computed or munged property names are not implemented yet",
            ));
        }
        Ok(name.into())
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
        if symbol.namespace.as_deref() == Some("suss.bootstrap") {
            let operation = match symbol.name.as_str() {
                "object-factory" => Some(Nominal::NativeObjectFactory),
                "object-default-prototype" => Some(Nominal::NativeObjectDefaultPrototype),
                "error" => Some(Nominal::LanguageError),
                "object-get" => Some(Nominal::NativeObjectGet),
                "object-set" => Some(Nominal::NativeObjectSet),
                "object-set-strict" => Some(Nominal::NativeObjectStrictSet),
                _ => None,
            };
            if let Some(operation) = operation {
                if !operation.valid(&vec![Type::Value; args.len()]) {
                    return Err(fail(
                        form.span.clone(),
                        "Invalid private native object adapter arity",
                    ));
                }
                let operands = args
                    .iter()
                    .map(|argument| self.form(argument))
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(self.nominal(form, operation, operands));
            }
        }
        if symbol.namespace.as_deref() == Some("suss.bootstrap") {
            let operation = match symbol.name.as_str() {
                "f64-coerce" => Some(Bitwise::F64Coerce),
                "f64-word0" => Some(Bitwise::F64Word0),
                "f64-word4" => Some(Bitwise::F64Word4),
                "f64-floor" => Some(Bitwise::F64Floor),
                "f64-finite" => Some(Bitwise::F64Finite),
                "f64-safe-integer" => Some(Bitwise::F64SafeInteger),
                "f64-time-clip" => Some(Bitwise::F64TimeClip),
                "safe-integer-remainder" => Some(Bitwise::SafeIntegerRemainder),
                "f64-remainder" => Some(Bitwise::F64Remainder),
                "identity-uid" => Some(Bitwise::IdentityUid),
                _ => None,
            };
            if let Some(operation) = operation {
                return self.bitwise_form(form, args, operation);
            }
        }
        if symbol.namespace.as_deref() == Some("suss.bootstrap") && symbol.name == "nil?" {
            if args.len() != 1 {
                return Err(fail(
                    form.span.clone(),
                    "Bootstrap nil? requires one operand",
                ));
            }
            return Ok(Hir {
                span: form.span.clone(),
                metadata: form.metadata.clone(),
                ty: Type::Bool,
                kind: Expression::NilTest(Box::new(self.form(&args[0])?)),
            });
        }
        if bare && symbol.name.starts_with(".-") {
            if args.len() != 1 || matches!(args[0].kind, Kind::Nil) {
                return Err(fail(
                    form.span.clone(),
                    "Named property read requires one non-literal-nil owner",
                ));
            }
            let key = Self::property_name(form, &symbol.name)?;
            let owner = self.form(&args[0])?;
            let key = self.literal_form(form, Literal::String(key.encode_utf16().collect()));
            return Ok(self.nominal(form, Nominal::NamedGet, vec![owner, key]));
        }
        if bare && symbol.name.starts_with('.') && !symbol.name.starts_with(".-") {
            let Some(owner) = args.first() else {
                return Err(fail(form.span.clone(), "Method call requires a receiver"));
            };
            let key = Self::property_name(form, &format!(".-{}", &symbol.name[1..]))?;
            let owner_value = self.form(owner)?;
            let owner_binding = self.fresh_binding(owner, owner_value);
            let owner_read = self.local(owner, owner_binding.id);
            let key = self.literal_form(form, Literal::String(key.encode_utf16().collect()));
            let lookup = self.nominal(form, Nominal::NamedGet, vec![owner_read.clone(), key]);
            let method_binding = self.fresh_binding(form, lookup);
            let mut arguments = vec![self.local(form, method_binding.id), owner_read];
            for arg in &args[1..] {
                arguments.push(self.form(arg)?);
            }
            let body = self.nominal(form, Nominal::ObjectInvoke, arguments);
            return Ok(Hir {
                span: form.span.clone(),
                metadata: form.metadata.clone(),
                ty: Type::Value,
                kind: Expression::Let {
                    bindings: vec![owner_binding, method_binding],
                    body: Box::new(body),
                },
            });
        }
        if symbol.name.ends_with('.') && symbol.name.len() > 1 {
            let mut constructor = items[0].clone();
            let Kind::Symbol(name) = &mut constructor.kind else {
                unreachable!()
            };
            name.name.pop();
            let mut arguments = vec![self.form(&constructor)?];
            arguments.extend(
                args.iter()
                    .map(|argument| self.form(argument))
                    .collect::<Result<Vec<_>, _>>()?,
            );
            return Ok(self.nominal(form, Nominal::Construct, arguments));
        }
        if bare && symbol.name == "new" {
            if args.is_empty() {
                return Err(fail(form.span.clone(), "new requires a constructor"));
            }
            let arguments = args
                .iter()
                .map(|argument| self.form(argument))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(self.nominal(form, Nominal::Construct, arguments));
        }
        if symbol.namespace.as_deref() == Some("suss.bootstrap") && symbol.name == "quote-form" {
            if args.len() != 1 {
                return Err(fail(
                    form.span.clone(),
                    "quote-form requires exactly one operand",
                ));
            }
            return self.quote_form_data(&args[0]);
        }
        if bare && symbol.name == "quote" {
            if args.len() != 1 {
                return Err(fail(
                    form.span.clone(),
                    "quote requires exactly one operand",
                ));
            }
            return self.quote_data(&args[0], 0);
        }
        // Only true special forms bypass lexical and namespace resolution.
        let resolved = if bare
            && matches!(
                symbol.name.as_str(),
                "if" | "do" | "fn*" | "def" | "loop*" | "recur" | "throw" | "try" | "set!"
            ) {
            None
        } else {
            if bare
                && (self.locals.contains_key(&symbol.name)
                    || self.fields.contains_key(&symbol.name))
            {
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
        if let Some(ResolvedBinding::BootstrapControl { operation, .. }) = &resolved {
            return self.control_form(form, args, *operation, statement, tail);
        }
        if let Some(ResolvedBinding::BootstrapBitwise { operation, .. }) = &resolved {
            return self.bitwise_form(form, args, *operation);
        }
        if let Some(ResolvedBinding::BootstrapComparison { operation, .. }) = &resolved {
            return self.comparison_form(form, args, *operation);
        }
        if let Some(ResolvedBinding::BootstrapArray { operation, .. }) = &resolved {
            return self.array_form(form, args, *operation);
        }
        if let Some(ResolvedBinding::Nominal {
            form: nominal_form, ..
        }) = &resolved
        {
            return self.nominal_form(form, args, *nominal_form);
        }
        if bare && symbol.name == "set!" {
            if args.len() != 2 {
                return Err(fail(form.span.clone(), "set! requires a var and value"));
            }
            if let Kind::List(target) = &args[0].kind {
                if let Some(Form {
                    kind: Kind::Symbol(name),
                    ..
                }) = target.first()
                {
                    if name.namespace.is_none() && name.name.starts_with(".-") {
                        if target.len() != 2 || matches!(target[1].kind, Kind::Nil) {
                            return Err(fail(
                                args[0].span.clone(),
                                "Named property assignment requires one non-literal-nil owner",
                            ));
                        }
                        let key = Self::property_name(&args[0], &name.name)?;
                        let owner = self.form(&target[1])?;
                        let key = self
                            .literal_form(&args[0], Literal::String(key.encode_utf16().collect()));
                        let value = self.form(&args[1])?;
                        return Ok(self.nominal(form, Nominal::NamedSet, vec![owner, key, value]));
                    }
                }
            }
            if let Kind::Symbol(name) = &args[0].kind {
                if name.namespace.is_none() && !self.locals.contains_key(&name.name) {
                    if let Some((field, mutable)) = self.fields.get(&name.name).cloned() {
                        if !mutable {
                            return Err(fail(
                                args[0].span.clone(),
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
                        arguments.push(self.form(&args[1])?);
                        let setter = match operation {
                            Nominal::Field(index) => Nominal::FieldSet(index),
                            Nominal::NamedGet => Nominal::NamedSet,
                            _ => unreachable!(),
                        };
                        return Ok(self.nominal(form, setter, arguments));
                    }
                }
            }
            let global = self.assignment_target(&args[0])?;
            return Ok(Hir {
                span: form.span.clone(),
                metadata: form.metadata.clone(),
                ty: Type::Value,
                kind: Expression::Assign {
                    global,
                    value: Box::new(self.form(&args[1])?),
                },
            });
        }
        if matches!(resolved, Some(ResolvedBinding::BootstrapBinding(_))) {
            return self.dynamic_scope(form, args, statement);
        }
        if bare && symbol.name == "throw" {
            if args.len() != 1 {
                return Err(fail(
                    form.span.clone(),
                    "throw requires exactly one operand",
                ));
            }
            return Ok(Hir {
                span: form.span.clone(),
                metadata: form.metadata.clone(),
                ty: Type::Value,
                kind: Expression::Throw(Box::new(self.form(&args[0])?)),
            });
        }
        if bare && symbol.name == "try" {
            return self.try_form(form, args, statement);
        }
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
        Expression::Assign { value, .. } => free_bindings(value, bound, free),
        Expression::DynamicScope { bindings, body } => {
            for (_, value) in bindings {
                free_bindings(value, bound, free);
            }
            free_bindings(body, bound, free);
        }
        Expression::Throw(value) | Expression::NilTest(value) => free_bindings(value, bound, free),
        Expression::Try { regions } => {
            for region in regions {
                free_bindings(region, bound, free);
            }
        }
        Expression::Definition {
            initializer: Some(value),
            ..
        } => free_bindings(value, bound, free),
        Expression::Local(id) => {
            if !bound.contains(id) {
                free.insert(*id);
            }
        }
        Expression::Function { captures, .. } | Expression::GeneralFunction { captures, .. } => {
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
        }
        | Expression::Bitwise {
            arguments: items, ..
        }
        | Expression::Comparison {
            arguments: items, ..
        }
        | Expression::Array {
            arguments: items, ..
        }
        | Expression::Nominal {
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
    prepare_with_expander(forms, span, environment, phase, &mut super::NoExpansion)
}
pub(crate) fn prepare_with_expander(
    forms: &[Form],
    span: Range<usize>,
    environment: &Environment,
    phase: Phase,
    expander: &mut dyn super::ExpansionHost,
) -> Result<(Hir, Environment), Diagnostic> {
    let mut analyzer = Analyzer {
        expander,
        environment: environment.clone(),
        phase,
        locals: HashMap::new(),
        fields: HashMap::new(),
        next: 0,
        next_loop: 0,
        analysis_depth: 0,
        target: None,
    };
    let hir = analyzer.body(forms, span, true, false)?;
    Ok((hir, analyzer.environment))
}
