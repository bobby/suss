//! Source-aware HIR for the replacement pipeline. No EDN conversion occurs.
mod arrays;
mod local_bindings;
pub use local_bindings::{FieldBinding, FunctionScope, LocalBinding, LocalKind, SourceRole};
mod function_parameters;
mod destructuring;
pub use function_parameters::{SourceFunctionParameters, SourceParameterMethod};
mod bitwise;
mod callable_signatures;
mod cases;
mod collections;
mod comparisons;
mod controls;
mod dynamic;
mod exceptions;
mod nominal;
mod quotes;
mod source_tags;
pub use source_tags::{SourceTags, local_tag, declaration_tag};
use super::{
    Diagnostic,
    resolve::{Binding as ResolvedBinding, Environment, Global, Phase},
};
pub use quotes::{identifier_hash, reader_metadata_pairs};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
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
    F64Ceil,
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
            Self::F64Ceil => "primitive-f64-ceil",
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
            | Self::F64Ceil
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
    /// Immutable private tests; public predicate cells remain live.
    IsNumber,
    IsString,
    IsClosure,
    BindCallable,
    /// Private selected binding-cell presence, not value truthiness.
    BindingDefined,
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
    CallableGet,
    NamedSet,
    NativeObjectFactory,
    IsNativeObject,
    IsTypeConstructor,
    ValueConstructor,
    SourceFunctionName,
    NativeObjectDefaultPrototype,
    NativeObjectGet,
    NativeObjectSet,
    NativeObjectStrictSet,
    CoerceString,
    ConcatString,
    StringIndexOf,
    StringSlice,
    LanguageError,
    IsLanguageError,
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
            Self::CoerceString | Self::ConcatString | Self::StringSlice => Type::String,
            Self::StringIndexOf => Type::Number,
            Self::BindingDefined
            | Self::IsNumber
            | Self::IsString
            | Self::IsClosure
            | Self::IsNativeObject
            | Self::IsTypeConstructor
            | Self::IsLanguageError
            | Self::Instance
            | Self::Satisfies
            | Self::NativeSatisfies
            | Self::NativeMarker(_) => Type::Bool,
            _ => Type::Value,
        }
    }
    pub(crate) fn valid(self, arguments: &[Type]) -> bool {
        let count = arguments.len();
        count <= i32::MAX as usize
            && match self {
                Self::Array => true,
                Self::NativeObjectFactory | Self::NativeObjectDefaultPrototype => count == 0,
                Self::IsNumber | Self::IsString | Self::LanguageError | Self::IsLanguageError | Self::IsClosure | Self::IsNativeObject | Self::IsTypeConstructor | Self::ValueConstructor | Self::SourceFunctionName => count == 1,
                Self::BindingDefined | Self::CoerceString => count == 1,
                Self::ConcatString | Self::StringIndexOf => count == 2,
                Self::StringSlice => count == 3,
                Self::BindCallable => count == 2,
                Self::NativeObjectGet => count == 2,
                Self::NativeObjectSet | Self::NativeObjectStrictSet => count == 3,
                Self::NamedGet => count == 2 && arguments[1] == Type::String,
                Self::CallableGet => count == 3 && arguments[1] == Type::String,
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
/// Reader/expansion syntax actually analyzed for this expression. Compiler-only
/// lowering nodes have no source record; their syntax must never be fabricated.
/// Immutable namespace facts at the actual source analysis boundary.
#[derive(Debug, Clone)]
pub struct SourceNamespace {
    pub namespace: String,
    pub aliases: BTreeMap<String, String>,
    pub refers: BTreeMap<String, Global>,
    pub used_refers: BTreeSet<String>,
    pub renamed_refers: BTreeSet<String>,
    pub excluded_core: BTreeSet<String>,
    pub macro_aliases: BTreeMap<String, String>,
    pub macro_refers: BTreeMap<String, (String, String)>,
    pub used_macro_refers: BTreeSet<String>,
    pub renamed_macro_refers: BTreeSet<String>,
    pub declarations: BTreeMap<Global, std::sync::Arc<super::resolve::DefinitionInfo>>,
    pub identities: Vec<Global>,
}
impl SourceNamespace {
    pub fn capture(environment: &Environment, phase: Phase) -> Self {
        Self::capture_namespace(environment, phase, environment.current_namespace(phase))
            .expect("declared current namespace")
    }
    /// Capture an actual namespace revision without installing it as current.
    pub fn capture_namespace(environment: &Environment, phase: Phase, namespace: &str) -> Option<Self> {
        let scope = environment.namespace_scope_for(phase, namespace)?;
        Some(Self {
            namespace: scope.namespace.into(),
            aliases: scope.aliases.clone(),
            refers: scope.refers.clone(),
            used_refers: scope.used_refers.clone(),
            renamed_refers: scope.renamed_refers.clone(),
            excluded_core: scope.excluded_core.clone(),
            macro_aliases: scope.macro_aliases.clone(),
            macro_refers: scope.macro_refers.clone(),
            used_macro_refers: scope.used_macro_refers.clone(),
            renamed_macro_refers: scope.renamed_macro_refers.clone(),
            declarations: scope
                .declarations
                .into_iter()
                .map(|(global, _)| (global.clone(), environment.shared_definition_info(global)
                    .expect("captured declaration revision").clone()))
                .collect(),
            identities: environment
                .cells()
                .into_iter()
                .filter(|global| global.phase() == phase && global.namespace() == scope.namespace
                    && !environment.is_hidden_cell(phase, global.namespace(), global.name()))
                .collect(),
        })
    }
}
#[derive(Debug, Clone)]
pub enum SourceBinding {
    Local(std::sync::Arc<LocalBinding>),
    Field(std::sync::Arc<FieldBinding>),
    Global {
        global: Global,
        declaration: Option<std::sync::Arc<super::resolve::DefinitionInfo>>,
    },
}
/// Actual analyzed source methods, before runtime signature wrappers or duplicate
/// arity elimination. These are compiler facts, not portable inferred tags.
#[derive(Debug, Clone)]
pub struct SourceCallable {
    /// Genuine source name scope, including declaration hints without lexical IDs.
    pub name: Option<std::sync::Arc<FunctionScope>>,
    pub methods: Vec<SourceMethod>,
}
/// Actual method-entry environment, captured before parameter allocation.
#[derive(Debug, Clone)]
pub struct SourceMethodEnvironment {
    pub scope: std::sync::Arc<SourceNamespace>,
    pub namespace_snapshot: std::sync::Arc<SourceNamespace>,
    pub locals: std::sync::Arc<HashMap<String, LocalBinding>>,
    pub fields: std::sync::Arc<HashMap<String, FieldBinding>>,
    pub function_scopes: std::sync::Arc<[std::sync::Arc<FunctionScope>]>,
    pub context: super::AnalysisContext,
}
#[derive(Debug, Clone)]
pub struct SourceMethod {
    pub form: Form,
    pub environment: SourceMethodEnvironment,
    pub declarations: Vec<LocalBinding>,
    pub parameters: Vec<Parameter>,
    pub variadic: bool,
    /// Whether analysis accepted a recur targeting this method (nil otherwise).
    pub recurs: Option<bool>,
    pub body: std::sync::Arc<Hir>,
}
impl SourceCallable {
    pub fn variadic(&self) -> bool {
        self.methods.iter().any(|method| method.variadic)
    }
    pub fn max_fixed_arity(&self) -> Option<usize> {
        if self.methods.is_empty() { return None; }
        self.methods.iter().try_fold(0, |maximum, method| {
            method.parameters.len().checked_sub(usize::from(method.variadic)).map(|arity| maximum.max(arity))
        })
    }
}
/// Original analyzed operands and declarations retained before lowering.
/// Quoted data constructors do not create expression child records.
#[derive(Debug, Clone)]
pub enum SourceNode {
    Future(std::sync::Arc<Hir>),
    Await(std::sync::Arc<Hir>),
    Vector(std::sync::Arc<[Hir]>),
    Map(std::sync::Arc<[Hir]>),
    Set(std::sync::Arc<[Hir]>),
    Do { statements: std::sync::Arc<[Hir]>, result: std::sync::Arc<Hir> },
    If {
        condition: std::sync::Arc<Hir>,
        consequent: std::sync::Arc<Hir>,
        alternative: std::sync::Arc<Hir>,
    },
    Bindings {
        is_loop: bool,
        bindings: std::sync::Arc<[LocalBinding]>,
        body: std::sync::Arc<Hir>,
    },
    Recur(std::sync::Arc<[Hir]>),
    Throw(std::sync::Arc<Hir>),
    Try {
        body: std::sync::Arc<Hir>,
        handler: std::sync::Arc<Hir>,
        cleanup: Option<std::sync::Arc<Hir>>,
        payload: Option<std::sync::Arc<LocalBinding>>,
    },
    Assign { target: std::sync::Arc<Hir>, value: std::sync::Arc<Hir> },
    Invoke { callee: std::sync::Arc<Hir>, arguments: std::sync::Arc<[Hir]> },
    Construct { class: std::sync::Arc<Hir>, arguments: std::sync::Arc<[Hir]> },
    WithMeta {
        expression: std::sync::Arc<Hir>,
        metadata: std::sync::Arc<Hir>,
        inner: Option<std::sync::Arc<SourceNode>>,
    },
}
#[derive(Debug, Clone)]
pub struct SourceAnalysis {
    /// Set only for analyzer-generated body regions, before runtime lowering.
    pub is_body: bool,
    pub tags: SourceTags,
    pub form: Form,
    pub resolved: Option<SourceBinding>,
    pub name_hint: Option<Form>,
    /// Live resolution catalog at the time this source node was entered.
    pub scope: std::sync::Arc<SourceNamespace>,
    /// Namespace supplied to macro &env at enclosing top-level form entry.
    pub namespace_snapshot: std::sync::Arc<SourceNamespace>,
    pub callable: Option<std::sync::Arc<SourceCallable>>,
    pub node: Option<std::sync::Arc<SourceNode>>,
    pub locals: std::sync::Arc<HashMap<String, LocalBinding>>,
    pub fields: std::sync::Arc<HashMap<String, FieldBinding>>,
    pub function_scopes: std::sync::Arc<[std::sync::Arc<FunctionScope>]>,
    pub context: super::AnalysisContext,
    pub phase: Phase,
    pub namespace: String,
    pub origin: Option<super::SourceOrigin>,
}
#[derive(Debug, Clone)]
pub struct Hir {
    pub source: Option<std::sync::Arc<SourceAnalysis>>,
    pub span: Range<usize>,
    pub metadata: Vec<Form>,
    pub ty: Type,
    pub kind: Expression,
}
#[derive(Debug, Clone)]
pub enum Expression {
    /// Deferred executable body and its lexical captures; never a body value.
    Future { captures: Vec<BindingId>, body: Box<Hir> },
    Await { value: Box<Hir> },
    /// Suspension-aware executable regions, before closure packaging.
    AsyncTry {
        body: Box<Hir>,
        handler: Option<Box<Hir>>,
        cleanup: Option<Box<Hir>>,
        payload: Option<Parameter>,
    },
    AsyncDynamicScope { bindings: Vec<(Global, Hir)>, body: Box<Hir> },
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
    /// Reader calls use a live public value only after successful initialization.
    GlobalOrFallback { global: Global, fallback: Global },
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
    suspendable: bool,
    expander: &'a mut dyn super::ExpansionHost,
    origin: Option<super::SourceOrigin>,
    environment: Environment,
    phase: Phase,
    locals: HashMap<String, LocalBinding>,
    fields: HashMap<String, FieldBinding>,
    function_scopes: Vec<std::sync::Arc<FunctionScope>>,
    next: usize,
    next_loop: usize,
    analysis_depth: usize,
    analysis_contexts: Vec<super::AnalysisContext>,
    source_namespace: Option<(u64, Phase, String, std::sync::Arc<SourceNamespace>)>,
    namespace_snapshot: Option<std::sync::Arc<SourceNamespace>>,
    source_callables: Vec<Option<std::sync::Arc<SourceCallable>>>,
    source_nodes: Vec<Option<std::sync::Arc<SourceNode>>>,
    callable_keys: BTreeMap<Global, Hir>,
    catch_aliases: Vec<(Form, std::sync::Arc<LocalBinding>)>,
    target: Option<(LoopId, usize)>,
    method_recurrence: Vec<(LoopId, bool)>,
}
impl Analyzer<'_> {
    fn capture_source_namespace(&mut self) -> std::sync::Arc<SourceNamespace> {
        let namespace = self.environment.current_namespace(self.phase).to_owned();
        let generation = self.environment.source_generation();
        match &self.source_namespace {
            Some((version, phase, name, scope))
                if *version == generation && *phase == self.phase && *name == namespace =>
            {
                scope.clone()
            }
            _ => {
                let scope =
                    std::sync::Arc::new(SourceNamespace::capture(&self.environment, self.phase));
                self.source_namespace = Some((generation, self.phase, namespace, scope.clone()));
                scope
            }
        }
    }
    fn body(
        &mut self,
        forms: &[Form],
        span: Range<usize>,
        context: super::AnalysisContext,
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
                    if index + 1 < forms.len() {
                        super::AnalysisContext::Statement
                    } else if forms.len() > 1 {
                        context.returning()
                    } else {
                        context
                    },
                    tail && index + 1 == forms.len(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let ty = items.last().map_or(Type::Nil, |item| item.ty);
        Ok(Hir {
            source: None,
            span,
            metadata: Vec::new(),
            ty,
            kind: Expression::Do(items),
        })
    }
    fn form(&mut self, form: &Form) -> Result<Hir, Diagnostic> {
        self.form_in(form, super::AnalysisContext::Expression, false)
    }
    fn analyzed_body(
        &mut self,
        forms: &[Form],
        span: Range<usize>,
        context: super::AnalysisContext,
        tail: bool,
    ) -> Result<Hir, Diagnostic> {
        // Analyzer-generated do syntax has genuine source facts and context,
        // while its children retain their ordinary bounded analysis frames.
        // Do not charge an additional source nesting level for this wrapper.
        let mut body_forms = vec![Form {
            kind: Kind::Symbol(suss_reader::Symbol { namespace: None, name: "do".into() }),
            span: span.clone(), metadata: vec![],
        }];
        body_forms.extend_from_slice(forms);
        let body_form = Form { kind: Kind::List(body_forms), span, metadata: vec![] };
        self.analysis_contexts.push(context);
        let result = stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
            self.expand_or_analyze(&body_form, context, tail, None)
        });
        self.analysis_contexts.pop();
        result.map(|mut body| {
            let source = body.source.as_mut().expect("analyzed source body");
            std::sync::Arc::make_mut(source).is_body = true;
            body
        })
    }
    fn form_in(
        &mut self,
        form: &Form,
        context: super::AnalysisContext,
        tail: bool,
    ) -> Result<Hir, Diagnostic> {
        self.form_in_named(form, context, tail, None)
    }
    fn form_in_named(
        &mut self,
        form: &Form,
        context: super::AnalysisContext,
        tail: bool,
        name_hint: Option<&Form>,
    ) -> Result<Hir, Diagnostic> {
        // Bounded bootstrap expansion can create deeper syntax than the reader
        // saw (notably nested threading). Check total analysis depth, including
        // nested macros, before recursive analyzer frames exhaust the stack.
        if self.analysis_depth >= 64 {
            return Err(fail(
                form.span.clone(),
                "Bootstrap analysis expansion limit exceeded",
            ));
        }
        let previous_origin = self.origin.clone();
        let reader_form = if self.analysis_depth == 0 && super::syntax_quote::has_template(form)? {
            let mut reader = self.environment.reader_state.clone();
            let raw = reader.expand_with_origin(form, &self.environment, self.phase, self.origin.as_ref())?;
            // Replay the same initial counter against verified indexing data;
            // this does not allocate another set of observable generated IDs.
            let mut replay = self.environment.reader_state.clone();
            let located = if let Some(origin) = &self.origin { origin.reader_tree(form)? } else { form.clone() };
            let located = replay.expand(&located, &self.environment, self.phase)?;
            if reader != replay {
                return Err(fail(form.span.clone(), "Reader provenance generated IDs diverged"));
            }
            let lowered = super::syntax_quote::lower_generated(&raw)?;
            let origin = self.origin.clone().unwrap_or_else(|| super::SourceOrigin::new("", None));
            self.origin = Some(origin.with_reader_expansion(lowered.clone(), located));
            self.environment.reader_state = reader;
            Some(lowered)
        } else { None };
        let form = reader_form.as_ref().unwrap_or(form);
        if self.analysis_depth == 0 {
            // The pinned analyzer refreshes &env's namespace between top-level
            // forms, not after each nested def. Keep resolution's live catalog
            // separate so recursive definitions still resolve provisionally.
            self.namespace_snapshot = Some(self.capture_source_namespace());
        }
        self.analysis_depth += 1;
        self.analysis_contexts.push(context);
        let result = stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
            self.expand_or_analyze(form, context, tail, name_hint)
        });
        self.analysis_depth -= 1;
        self.analysis_contexts.pop();
        self.origin = previous_origin;
        result
    }
    fn expand_or_analyze(
        &mut self,
        form: &Form,
        context: super::AnalysisContext,
        tail: bool,
        name_hint: Option<&Form>,
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
                    let expansion_context = super::ExpansionContext {
                        environment: &self.environment,
                        namespace_snapshot: self.namespace_snapshot.as_ref().expect("top-level source snapshot"),
                        origin: self.origin.as_ref(),
                        phase: self.phase,
                        context,
                        locals: &self.locals,
                        fields: &self.fields,
                        function_scopes: &self.function_scopes,
                    };
                    if let Some(expanded) = self.expander.expand(form, expansion_context)? {
                        return self.form_in_named(&expanded, context, tail, name_hint);
                    }
                    // Original bounded bootstrap for simple binding vectors.
                    // Pinned EPL-1.0 core.cljc772–813 expands let/loop to their
                    // primitive forms. Analyze that actual expansion before
                    // recording source facts; lexical callees stay untouched.
                    let primitive = match self.environment.resolve_bootstrap_macro(self.phase, symbol) {
                        Some(ResolvedBinding::BootstrapLet(_)) => Some("let*"),
                        Some(ResolvedBinding::BootstrapLoop(_)) => Some("loop*"),
                        _ => None,
                    };
                    if let Some(primitive) = primitive {
                        let mut expanded = form.clone();
                        let Kind::List(items) = &mut expanded.kind else { unreachable!() };
                        items[0].kind = Kind::Symbol(suss_reader::Symbol {
                            namespace: None, name: primitive.into(),
                        });
                        // The fixed primitive rewrite is analyzed in this
                        // frame; it cannot form a recursive bootstrap chain.
                        return stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
                            self.expand_or_analyze(&expanded, context, tail, name_hint)
                        });
                    }
                }
            }
        }
        if let Some(coerced) = super::syntax_quote::reader_sequence_initializer(form, &self.environment, self.phase, self.origin.as_ref())? {
            let (global, fallback, fresh) = self.environment.reader_sequence_cells(self.phase);
            if fresh {
                let initializer = self.form(&coerced)?;
                self.callable_keys.insert(fallback.clone(), Hir {
                    source: None, span: form.span.clone(), metadata: Vec::new(), ty: Type::Value,
                    kind: Expression::Definition {
                        global: fallback.clone(), name_metadata: Vec::new(), name_span: form.span.clone(),
                        docstring: None, initializer: Some(Box::new(initializer)), once: true,
                    },
                });
            }
            let Kind::List(items) = &form.kind else { unreachable!() };
            let argument = self.form(&items[1])?;
            return Ok(Hir {
                source: None, span: form.span.clone(), metadata: form.metadata.clone(), ty: Type::Value,
                kind: Expression::Call {
                    callee: Box::new(Hir { source: None, span: items[0].span.clone(),
                        metadata: Vec::new(), ty: Type::Value, kind: Expression::GlobalOrFallback { global, fallback } }),
                    arguments: vec![argument],
                },
            });
        }
        let namespace = self.environment.current_namespace(self.phase).to_owned();
        let scope = self.capture_source_namespace();
        let locals = std::sync::Arc::new(self.locals.clone());
        let fields = std::sync::Arc::new(self.fields.clone());
        let function_scopes = self.function_scopes.clone().into();
        let symbol = match &form.kind {
            Kind::Symbol(symbol) => Some(symbol),
            Kind::List(items) => items.first().and_then(|head| {
                if let Kind::Symbol(symbol) = &head.kind {
                    Some(symbol)
                } else {
                    None
                }
            }),
            _ => None,
        };
        let resolved = symbol.and_then(|symbol| {
            if symbol.namespace.is_none() {
                if let Some(binding) = self.locals.get(&symbol.name) {
                    return Some(SourceBinding::Local(std::sync::Arc::new(binding.clone())));
                }
                if let Some(field) = self.fields.get(&symbol.name) {
                    return Some(SourceBinding::Field(std::sync::Arc::new(field.clone())));
                }
            }
            self.environment
                .resolve(self.phase, symbol, form.span.clone())
                .ok()
                .map(|binding| {
                    let global = binding.global().clone();
                    let declaration = self
                        .environment
                        .shared_definition_info(&global)
                        .cloned();
                    SourceBinding::Global {
                        global,
                        declaration,
                    }
                })
        });
        // A nested source function gets its own fact slot. Compiler-only
        // wrappers never become the evidence for a source function's methods.
        self.source_callables.push(None);
        self.source_nodes.push(None);
        let result = self.form_inner(form, context, tail, name_hint);
        let callable = self.source_callables.pop().expect("source callable fact slot");
        let node = self.source_nodes.pop().expect("source node fact slot");
        result.and_then(|mut expression| {
                // A bootstrap/source expansion may already have returned its actual
                // analyzed node. Preserve that record instead of attributing the
                // original macro call to the expanded expression.
                if expression.source.is_none() {
                    let tags = source_tags::source_tags(form, resolved.as_ref(), callable.as_deref(), &expression)?;
                    let mut source = SourceAnalysis {
                        is_body: false,
                        tags,
                        form: form.clone(),
                        resolved,
                        name_hint: name_hint.cloned(),
                        scope,
                        namespace_snapshot: self.namespace_snapshot.as_ref().expect("top-level source snapshot").clone(),
                        callable,
                        node,
                        locals,
                        fields,
                        function_scopes,
                        context,
                        phase: self.phase,
                        namespace,
                        origin: self.origin.clone(),
                    };
                    if let Some(node) = &source.node
                        && let SourceNode::WithMeta { expression: value, metadata, inner } = node.as_ref()
                    {
                        // The child uses the original entry snapshot and form,
                        // but the expression context of analyze-wrap-meta.
                        let mut value = value.as_ref().clone();
                        let mut child = source.clone();
                        child.context = super::AnalysisContext::Expression;
                        child.node = inner.clone();
                        value.source = Some(std::sync::Arc::new(child));
                        source.tags = source_tags::metadata_wrapper_tags(form)?;
                        source.node = Some(std::sync::Arc::new(SourceNode::WithMeta {
                            expression: std::sync::Arc::new(value),
                            metadata: metadata.clone(),
                            inner: inner.clone(),
                        }));
                    }
                    expression.source = Some(std::sync::Arc::new(source));
                }
                Ok(expression)
            })
    }
    fn form_inner(
        &mut self,
        form: &Form,
        context: super::AnalysisContext,
        tail: bool,
        name_hint: Option<&Form>,
    ) -> Result<Hir, Diagnostic> {
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
                    source: None,
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
                    if let Some(binding) = self.locals.get(&symbol.name) {
                        (Expression::Local(binding.id), binding.ty)
                    } else if let Some(field) = self.fields.get(&symbol.name) {
                        (field.access.kind.clone(), field.access.ty)
                    } else {
                        self.global_value(symbol, form.span.clone())?
                    }
                } else {
                    self.global_value(symbol, form.span.clone())?
                };
                return Ok(Hir {
                    source: None,
                    span: form.span.clone(),
                    metadata: form.metadata.clone(),
                    ty,
                    kind,
                });
            }
            Kind::List(items) if !items.is_empty() => {
                return self.list(form, items, context, tail, name_hint);
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
            source: None,
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
        *self.source_nodes.last_mut().expect("source node fact slot") = Some(std::sync::Arc::new(SourceNode::Invoke {
            callee: std::sync::Arc::new(callee.as_ref().clone()),
            arguments: arguments.clone().into(),
        }));
        Ok(Hir {
            source: None,
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Value,
            kind: Expression::Call {
                callee: Box::new(self.prepare_source_callee(form, *callee, arguments.len())?),
                arguments,
            },
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
        // ClojureScript :private is retained declaration metadata. Qualified
        // cross-namespace access remains callable (the pinned analyzer emits a
        // warning, not an access error). Warning emission/namespace filtering
        // remain separate unfinished namespace acceptance requirements.
        for metadata in &args[0].metadata {
            let unsupported = |form: &Form| {
                matches!(&form.kind, Kind::Keyword(key)
                if key.namespace.is_none() && matches!(key.name.as_str(), "const" | "macro"))
            };
            if unsupported(metadata)
                || matches!(&metadata.kind, Kind::Map(entries)
                if entries.chunks_exact(2).any(|entry| unsupported(&entry[0])))
            {
                return Err(fail(
                    args[0].span.clone(),
                    "Definition const/macro attributes are not implemented yet",
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
        // Pinned ClojureScript analyzer.cljc2092–2172 (c4295f3) preserves
        // existing declaration data for a truthy :declared def without an
        // initializer. Initializer-bearing defs still publish their new
        // provisional record before analysis. This original implementation
        // retains the existing immutable revision, including source functions.
        let preserve_declaration = init.is_none()
            && self.environment.definition_info(&global).is_some()
            && reader_metadata_pairs(&args[0])?.chunks_exact(2).any(|pair| {
                matches!(&pair[0].kind, Kind::Keyword(key)
                    if key.namespace.is_none() && key.name == "declared")
                    && !matches!(pair[1].kind, Kind::Nil | Kind::Bool(false))
            });
        let mut definition = super::resolve::DefinitionInfo {
            definition_form: form.clone(),
            analysis_completed: false,
            declaration: args[0].clone(),
            docstring: if args.len() == 3 {
                match &args[1].kind {
                    Kind::String(units) => Some(units.clone()),
                    _ => unreachable!(),
                }
            } else {
                None
            },
            origin: self.origin.clone(),
            initializer_form: init.cloned(),
            initializer: None,
            type_fields: None,
            once,
        };
        if !preserve_declaration {
            self.environment
                .record_definition(global.clone(), definition.clone());
        }
        let initializer = init
            .map(|init| {
                self.form_in_named(
                    init,
                    super::AnalysisContext::Expression,
                    false,
                    Some(&args[0]),
                )
                .map(Box::new)
            })
            .transpose()?;
        // A nested initializer can declare this same global. Publish the outer
        // declaration and its initializer together when its analysis completes.
        definition.initializer = initializer.as_deref().cloned().map(std::sync::Arc::new);
        definition.analysis_completed = true;
        if !preserve_declaration {
            self.environment
                .record_definition(global.clone(), definition);
        }
        Ok(Hir {
            source: None,
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
        name_hint: Option<&Form>,
    ) -> Result<Hir, Diagnostic> {
        let expanded;
        let args = if bootstrap_macro {
            expanded = self.destructured_function_arguments(form, args)?;
            expanded.as_slice()
        } else {
            args
        };
        let outer_scope_count = self.function_scopes.len();
        let suspendable = std::mem::replace(&mut self.suspendable, false);
        let result = self.function_inner(form, args, bootstrap_macro, name_hint);
        self.suspendable = suspendable;
        if result.is_ok() {
            let name = self.function_scopes.get(outer_scope_count).cloned();
            if let Some(callable) = self.source_callables.last_mut().and_then(Option::as_mut) {
                std::sync::Arc::make_mut(callable).name = name;
            }
        }
        self.function_scopes.truncate(outer_scope_count);
        result
    }
    fn function_inner(
        &mut self,
        form: &Form,
        args: &[Form],
        bootstrap_macro: bool,
        name_hint: Option<&Form>,
    ) -> Result<Hir, Diagnostic> {
        if args
            .first()
            .is_some_and(|arg| matches!(&arg.kind, Kind::Vector(names)
                if !names.iter().any(|name| matches!(&name.kind, Kind::Symbol(symbol) if symbol.namespace.is_none() && symbol.name == "&"))))
        {
            self.enter_function_scope(form, name_hint, None);
            let (function, source_method) = self.fixed_function(form, args, bootstrap_macro)?;
            *self.source_callables.last_mut().expect("source function fact slot") = Some(std::sync::Arc::new(SourceCallable {
                name: None,
                methods: vec![source_method],
            }));
            return Ok(function);
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
            self.insert_local(&args[0], id, Type::Value, LocalKind::FunctionName, None);
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
        if let Some(parameter) = &self_binding {
            // These signatures are actual source declarations, before body analysis.
            let variadic = if let Some(Form {
                kind: Kind::Vector(names),
                ..
            }) = signatures.first()
            {
                names.iter().any(|name| matches!(&name.kind, Kind::Symbol(name) if name.namespace.is_none() && name.name == "&"))
            } else {
                signatures.iter().any(|signature| matches!(&signature.kind, Kind::List(parts) if matches!(parts.first().map(|part| &part.kind), Some(Kind::Vector(names)) if names.iter().any(|name| matches!(&name.kind, Kind::Symbol(name) if name.namespace.is_none() && name.name == "&")))))
            };
            self.locals.get_mut(&parameter.name).unwrap().source_role =
                SourceRole::FunctionName { variadic, methods: None };
        }
        let declaration = if self_binding.is_some() {
            args.first()
        } else {
            name_hint
        };
        let local = self_binding
            .as_ref()
            .map(|binding| self.locals[&binding.name].clone());
        self.enter_function_scope(form, declaration, local);
        if let Some(parameter) = &self_binding {
            let staged = std::sync::Arc::new(self.stage_function_parameters(form, signatures)?);
            let variadic = staged.methods.iter().any(|method| method.variadic);
            self.locals.get_mut(&parameter.name).expect("self binding").source_role =
                SourceRole::FunctionName { variadic, methods: Some(staged) };
        }
        let mut methods = Vec::new();
        let mut source_methods = Vec::new();
        let mut captures = BTreeSet::new();
        if signatures
            .first()
            .is_some_and(|arg| matches!(arg.kind, Kind::Vector(_)))
        {
            let (method, free, source_method) = self.general_method(form, signatures, bootstrap_macro)?;
            source_methods.push(source_method);
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
                let multi = signatures.len() > 1;
                if multi { self.analysis_contexts.push(super::AnalysisContext::Expression); }
                let result = self.general_method(signature, items, bootstrap_macro);
                if multi { self.analysis_contexts.pop(); }
                let (method, free, source_method) = result?;
                source_methods.push(source_method);
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
        let function = Hir {
            source: None,
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Value,
            kind: Expression::GeneralFunction {
                methods,
                captures: captures.into_iter().collect(),
                self_binding,
                rest_class,
            },
        };
        let function = self.attach_callable_signatures(form, function)?;
        *self.source_callables.last_mut().expect("source function fact slot") = Some(std::sync::Arc::new(SourceCallable { name: None, methods: source_methods }));
        Ok(function)
    }
    fn general_method(
        &mut self,
        form: &Form,
        args: &[Form],
        bootstrap_macro: bool,
    ) -> Result<(Method, Vec<BindingId>, SourceMethod), Diagnostic> {
        let method_form = if matches!(&form.kind, Kind::List(parts)
            if matches!(parts.first().map(|part| &part.kind), Some(Kind::Vector(_)))) {
            form.clone()
        } else { Form { kind: Kind::List(args.to_vec()), span: form.span.clone(), metadata: vec![] } };
        let (args, variadic, rest_parameter) = Self::normalize_function_method(form, args)?;
        let (method, mut source_method) = self.fixed_function_fields(
            form,
            &args,
            bootstrap_macro,
            &[],
            false,
            false,
            rest_parameter,
            None,
        )?;
        source_method.form = method_form;
        source_method.variadic = variadic;
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
            source_method,
        ))
    }
    fn fixed_function(
        &mut self,
        form: &Form,
        args: &[Form],
        bootstrap_macro: bool,
    ) -> Result<(Hir, SourceMethod), Diagnostic> {
        self.fixed_function_fields(form, args, bootstrap_macro, &[], false, false, None, None)
    }
    fn fixed_function_fields(
        &mut self,
        form: &Form,
        args: &[Form],
        bootstrap_macro: bool,
        fields: &[Form],
        method_receiver: bool,
        object_method: bool,
        rest_parameter: Option<usize>,
        receiver_type: Option<&Form>,
    ) -> Result<(Hir, SourceMethod), Diagnostic> {
        // Object and protocol methods also enter through this boundary rather
        // than the ordinary function wrapper. Their bodies are synchronous;
        // only a nested future may introduce a new suspendable context.
        let expanded;
        let args = if bootstrap_macro {
            expanded = self.destructured_function_arguments(form, args)?;
            expanded.as_slice()
        } else {
            args
        };
        let suspendable = std::mem::replace(&mut self.suspendable, false);
        let result = self.fixed_function_fields_inner(
            form, args, bootstrap_macro, fields, method_receiver, object_method,
            rest_parameter, receiver_type,
        );
        self.suspendable = suspendable;
        result
    }
    fn fixed_function_fields_inner(
        &mut self,
        form: &Form,
        args: &[Form],
        bootstrap_macro: bool,
        fields: &[Form],
        method_receiver: bool,
        object_method: bool,
        rest_parameter: Option<usize>,
        receiver_type: Option<&Form>,
    ) -> Result<(Hir, SourceMethod), Diagnostic> {
        let Some(params) = args.first() else {
            return Err(fail(form.span.clone(), "fn requires a parameter vector"));
        };
        let Kind::Vector(names) = &params.kind else {
            return Err(fail(
                params.span.clone(),
                "Function signature requires a parameter vector",
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
        let method_environment = SourceMethodEnvironment {
            scope: self.capture_source_namespace(),
            namespace_snapshot: self.namespace_snapshot.as_ref().expect("top-level source snapshot").clone(),
            locals: std::sync::Arc::new(self.locals.clone()),
            fields: std::sync::Arc::new(self.fields.clone()),
            function_scopes: self.function_scopes.clone().into(),
            context: *self.analysis_contexts.last().expect("method analysis context"),
        };
        let outer = self.locals.clone();
        let outer_fields = self.fields.clone();
        let (parameters, declarations) = self.allocate_function_parameters(names, rest_parameter, receiver_type.is_some())?;
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
                    source: None,
                    span: parameter.span.clone(),
                    metadata: Vec::new(),
                    ty: Type::Value,
                    kind: Expression::Local(parameter.id),
                },
            });
            self.remap_local(&parameter.name, id, Type::Value);
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
                let parameter = parameters
                    .iter()
                    .any(|parameter| parameter.name == symbol.name);
                if !parameter {
                    self.locals.remove(&symbol.name);
                }
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
                let record = FieldBinding {
                    identity: std::sync::Arc::new(()),
                    declaration: field.clone(),
                    origin: self.origin.clone(),
                    index,
                    mutable: Self::field_mutable(field)?,
                    access: value,
                };
                if parameter {
                    let binding = self.locals.get_mut(&symbol.name).expect("method parameter");
                    binding.shadow = None;
                    binding.shadow_field = Some(std::sync::Arc::new(record.clone()));
                }
                self.fields.insert(symbol.name.clone(), record);
            }
        }
        // Retain the visible argument before this-as anchors the receiver.
        // Repeated names are legal: the last formal can shadow the receiver.
        let receiver_argument = if receiver_type.is_some() {
            parameters
                .first()
                .and_then(|parameter| self.locals.get(&parameter.name))
                .cloned()
        } else {
            None
        };
        if method_receiver {
            let parameter = parameters
                .first()
                .ok_or_else(|| fail(form.span.clone(), "Protocol method requires a receiver"))?;
            // The pin anchors a method's physical receiver across recur. The
            // first recur operand still evaluates, but does not replace `this`.
            self.remap_local(&parameter.name, parameter.id, Type::Value);
        }
        if let Some(receiver_type) = receiver_type {
            self.record_method_roles(
                &parameters,
                names,
                receiver_type,
                object_method,
                receiver_argument,
            );
        }
        // Field reads occur at the original use, including in nested closures;
        // unreferenced fields must not introduce checks or effects before a body.
        self.method_recurrence.push((target, false));
        let inner_body = self.analyzed_body(
            &args[1..],
            form.span.clone(),
            super::AnalysisContext::Return,
            true,
        )?;
        let (method_target, recurred) = self.method_recurrence.pop().expect("method recurrence frame");
        debug_assert_eq!(method_target, target);
        let source_method = SourceMethod {
            environment: method_environment,
            form: Form { kind: Kind::List(args.to_vec()), span: form.span.clone(), metadata: vec![] },
            declarations, parameters: parameters.clone(), variadic: rest_parameter.is_some(),
            recurs: recurred.then_some(true),
            body: std::sync::Arc::new(inner_body.clone()),
        };
        self.target = outer_target;
        let body = Box::new(Hir {
            source: None,
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
        Ok((Hir {
            source: None,
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Closure(parameters.len()),
            kind: Expression::Function {
                parameters,
                captures: captures.into_iter().collect(),
                body,
            },
        }, source_method))
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
        context: super::AnalysisContext,
        tail: bool,
        name_hint: Option<&Form>,
    ) -> Result<Hir, Diagnostic> {
        let Kind::Symbol(symbol) = &items[0].kind else {
            return self.call(form, items);
        };
        let args = &items[1..];
        let bare = symbol.namespace.is_none();
        if symbol.namespace.as_deref() == Some("suss.async") {
            if symbol.name == "future*" {
                let suspendable = std::mem::replace(&mut self.suspendable, true);
                let target = self.target.take();
                let locals = self.locals.clone();
                let body = self.analyzed_body(args, form.span.clone(), super::AnalysisContext::Return, false);
                self.suspendable = suspendable;
                self.target = target;
                self.locals = locals;
                let body = body?;
                let mut captures = BTreeSet::new();
                free_bindings(&body, &BTreeSet::new(), &mut captures);
                *self.source_nodes.last_mut().expect("source node fact slot") =
                    Some(std::sync::Arc::new(SourceNode::Future(std::sync::Arc::new(body.clone()))));
                return Ok(Hir { source: None, span: form.span.clone(), metadata: form.metadata.clone(),
                    ty: Type::Value, kind: Expression::Future { captures: captures.into_iter().collect(), body: Box::new(body) } });
            }
            if symbol.name == "await*" {
                if !self.suspendable {
                    return Err(fail(form.span.clone(), "await is legal only inside a future body"));
                }
                if args.len() != 1 {
                    return Err(fail(form.span.clone(), "await requires exactly one operand"));
                }
                let value = self.form(&args[0])?;
                *self.source_nodes.last_mut().expect("source node fact slot") =
                    Some(std::sync::Arc::new(SourceNode::Await(std::sync::Arc::new(value.clone()))));
                return Ok(Hir { source: None, span: form.span.clone(), metadata: form.metadata.clone(),
                    ty: Type::Value, kind: Expression::Await { value: Box::new(value) } });
            }
        }
        if symbol.namespace.as_deref() == Some("suss.compiler") && symbol.name == "cell-defined?" {
            if args.len() != 1 {
                return Err(fail(
                    form.span.clone(),
                    "Compiler cell-defined? requires one qualified symbol",
                ));
            }
            let Kind::Symbol(target) = &args[0].kind else {
                return Err(fail(
                    args[0].span.clone(),
                    "Compiler cell-defined? requires a qualified symbol, not an evaluated value",
                ));
            };
            let Some(namespace) = target.namespace.as_deref() else {
                return Err(fail(
                    args[0].span.clone(),
                    "Compiler cell-defined? requires an explicit namespace identity",
                ));
            };
            // resolve filters reader/internal reservations. Reject before the
            // fallback can promote a ReaderCell through declare_cell; only an
            // actual source declaration may make that identity public.
            if self
                .environment
                .is_hidden_cell(self.phase, namespace, &target.name)
            {
                return Err(fail(
                    args[0].span.clone(),
                    "Compiler-owned cells are not source publication targets",
                ));
            }
            // Existing aliases/refers resolve through the ordinary phase catalog.
            // A fresh generated type can query a real unbound cell only in the
            // current, already declared namespace; no arbitrary namespace or
            // source declaration facts are fabricated for an existence probe.
            let global = match self
                .environment
                .resolve(self.phase, target, args[0].span.clone())
            {
                Ok(ResolvedBinding::Cell(global)) => global,
                Ok(_) => {
                    return Err(fail(
                        args[0].span.clone(),
                        "Compiler cell-defined? requires a source cell, not a compiler bootstrap binding",
                    ));
                }
                Err(_) if namespace == self.environment.current_namespace(self.phase) => self
                    .environment
                    .declare_cell(self.phase, namespace, &target.name)?,
                Err(error) => return Err(error),
            };
            if self
                .environment
                .is_hidden_cell(self.phase, global.namespace(), global.name())
            {
                return Err(fail(
                    args[0].span.clone(),
                    "Compiler-owned cells are not source publication targets",
                ));
            }
            let cell = Hir {
                source: None,
                span: args[0].span.clone(),
                metadata: Vec::new(),
                ty: Type::Value,
                kind: Expression::GlobalCell(global),
            };
            return Ok(self.nominal(form, Nominal::BindingDefined, vec![cell]));
        }
        if symbol.namespace.as_deref() == Some("suss.bootstrap") {
            let operation = match symbol.name.as_str() {
                "number?" => Some(Nominal::IsNumber),
                "string?" => Some(Nominal::IsString),
                "object-factory" => Some(Nominal::NativeObjectFactory),
                "native-object?" => Some(Nominal::IsNativeObject),
                "type-constructor?" => Some(Nominal::IsTypeConstructor),
                "value-constructor" => Some(Nominal::ValueConstructor),
                "function-name" => Some(Nominal::SourceFunctionName),
                "object-default-prototype" => Some(Nominal::NativeObjectDefaultPrototype),
                "coerce-string" => Some(Nominal::CoerceString),
                "concat-string" => Some(Nominal::ConcatString),
                "string-index-of" => Some(Nominal::StringIndexOf),
                "string-slice" => Some(Nominal::StringSlice),
                "error" => Some(Nominal::LanguageError),
                "error?" => Some(Nominal::IsLanguageError),
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
                "f64-ceil" => Some(Bitwise::F64Ceil),
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
                source: None,
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
            return self.callable_named_get(form, owner, key, &symbol.name[2..]);
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
            let lookup =
                self.callable_named_get(form, owner_read.clone(), key, &symbol.name[1..])?;
            let method_binding = self.fresh_binding(form, lookup);
            let mut arguments = vec![self.local(form, method_binding.id), owner_read];
            for arg in &args[1..] {
                arguments.push(self.form(arg)?);
            }
            let body = self.nominal(form, Nominal::ObjectInvoke, arguments);
            return Ok(Hir {
                source: None,
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
            // Pinned macroexpand-1 rewrites Foo. to genuine (new Foo ...)
            // syntax before analysis. Keep that source record, not the lowered
            // descriptor call or the original shorthand invocation.
            constructor.metadata.clear();
            let mut parts = vec![Form {
                kind: Kind::Symbol(suss_reader::Symbol::new("new")),
                span: items[0].span.clone(), metadata: vec![],
            }, constructor];
            parts.extend_from_slice(args);
            let expanded = Form { kind: Kind::List(parts), ..form.clone() };
            // This fixed rewrite cannot recurse through another trailing dot;
            // preserve the existing source-depth bound like let/loop rewrites.
            return stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
                self.expand_or_analyze(&expanded, context, tail, name_hint)
            });
        }
        if bare && symbol.name == "new" {
            if args.is_empty() {
                return Err(fail(form.span.clone(), "new requires a constructor"));
            }
            let arguments = args
                .iter()
                .map(|argument| self.form(argument))
                .collect::<Result<Vec<_>, _>>()?;
            *self.source_nodes.last_mut().expect("source node fact slot") = Some(std::sync::Arc::new(SourceNode::Construct {
                class: std::sync::Arc::new(arguments[0].clone()),
                arguments: arguments[1..].to_vec().into(),
            }));
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
                "if" | "do" | "fn*" | "def" | "let*" | "loop*" | "recur" | "throw" | "try" | "set!"
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
            return self.control_form(form, args, *operation, context, tail);
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
                    if let Some(field) = self.fields.get(&name.name).cloned() {
                        if !field.mutable {
                            return Err(fail(
                                args[0].span.clone(),
                                "Cannot assign a local or immutable field",
                            ));
                        }
                        let Expression::Nominal {
                            operation,
                            mut arguments,
                        } = field.access.kind
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
            let target = self.form(&args[0])?;
            let value = self.form(&args[1])?;
            *self.source_nodes.last_mut().expect("source node fact slot") = Some(std::sync::Arc::new(SourceNode::Assign {
                target: std::sync::Arc::new(target),
                value: std::sync::Arc::new(value.clone()),
            }));
            return Ok(Hir {
                source: None,
                span: form.span.clone(),
                metadata: form.metadata.clone(),
                ty: Type::Value,
                kind: Expression::Assign {
                    global,
                    value: Box::new(value),
                },
            });
        }
        if matches!(resolved, Some(ResolvedBinding::BootstrapBinding(_))) {
            return self.dynamic_scope(form, args, context);
        }
        if bare && symbol.name == "throw" {
            if args.len() != 1 {
                return Err(fail(
                    form.span.clone(),
                    "throw requires exactly one operand",
                ));
            }
            let exception = self.form(&args[0])?;
            *self.source_nodes.last_mut().expect("source node fact slot") = Some(std::sync::Arc::new(SourceNode::Throw(std::sync::Arc::new(exception.clone()))));
            return Ok(Hir {
                source: None,
                span: form.span.clone(),
                metadata: form.metadata.clone(),
                ty: Type::Value,
                kind: Expression::Throw(Box::new(exception)),
            });
        }
        if bare && symbol.name == "try" {
            return self.try_form(form, args, context);
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
            if let Some((_, recurred)) = self.method_recurrence.iter_mut().rev().find(|(id, _)| *id == target) {
                *recurred = true;
            }
            *self.source_nodes.last_mut().expect("source node fact slot") = Some(std::sync::Arc::new(SourceNode::Recur(arguments.clone().into())));
            return Ok(Hir {
                source: None,
                span: form.span.clone(),
                metadata: form.metadata.clone(),
                ty: Type::Value,
                kind: Expression::Recur { target, arguments },
            });
        }
        if bare && symbol.name == "def" {
            if args.len() == 1 && context != super::AnalysisContext::Statement {
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
            let value = self.function(
                form,
                args,
                matches!(resolved, Some(ResolvedBinding::BootstrapFn(_))),
                name_hint,
            )?;
            return self.attach_literal_metadata(form, value);
        }
        let (kind, ty) = match (bare, symbol.name.as_str()) {
            (true, "do") => {
                let body = self.body(args, form.span.clone(), context, tail)?;
                let Expression::Do(children) = &body.kind else { unreachable!() };
                let (statements, result) = if let Some((result, statements)) = children.split_last() {
                    (statements.to_vec(), result.clone())
                } else {
                    let nil = Form { kind: Kind::Nil, span: form.span.clone(), metadata: vec![] };
                    (vec![], self.form_in(&nil, context, tail)?)
                };
                *self.source_nodes.last_mut().expect("source node fact slot") = Some(std::sync::Arc::new(SourceNode::Do {
                    statements: statements.into(), result: std::sync::Arc::new(result),
                }));
                (body.kind, body.ty)
            }
            (true, "if") => {
                if !(2..=3).contains(&args.len()) {
                    return Err(fail(form.span.clone(), "if requires two or three operands"));
                }
                let condition = Box::new(self.form(&args[0])?);
                let consequent = Box::new(self.form_in(&args[1], context, tail)?);
                let alternative = Box::new(if args.len() == 3 {
                    self.form_in(&args[2], context, tail)?
                } else {
                    let nil = Form { kind: Kind::Nil, span: form.span.clone(), metadata: vec![] };
                    self.form_in(&nil, context, tail)?
                });
                let ty = if consequent.ty == alternative.ty {
                    consequent.ty
                } else {
                    Type::Value
                };
                *self.source_nodes.last_mut().expect("source node fact slot") = Some(std::sync::Arc::new(SourceNode::If {
                    condition: std::sync::Arc::new(condition.as_ref().clone()),
                    consequent: std::sync::Arc::new(consequent.as_ref().clone()),
                    alternative: std::sync::Arc::new(alternative.as_ref().clone()),
                }));
                (
                    Expression::If {
                        condition,
                        consequent,
                        alternative,
                    },
                    ty,
                )
            }
            _ if bare && matches!(symbol.name.as_str(), "let*" | "loop*") =>
            {
                let is_loop = symbol.name == "loop*";
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
                let mut source_bindings = Vec::new();
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
                    // Only the pending compiler-generated catch alias may share
                    // its physical payload ID. Consume the original declaration
                    // before entering the body; ordinary nested lets stay distinct.
                    let alias = if !is_loop {
                        self.catch_aliases.iter().position(|(declaration, hidden)| {
                            declaration == &pair[0] && pair[1].kind == hidden.declaration.kind
                        }).map(|index| self.catch_aliases.remove(index).1)
                    } else { None };
                    let value = self.form(&pair[1])?;
                    let id = if let Some(hidden) = &alias { hidden.id } else {
                        let id = BindingId(self.next); self.next += 1; id
                    };
                    self.insert_local(
                        &pair[0],
                        id,
                        if is_loop { Type::Value } else { value.ty },
                        if alias.is_some() {
                            LocalKind::Catch
                        } else if is_loop {
                            LocalKind::Loop
                        } else {
                            LocalKind::Let
                        },
                        Some(value.clone()),
                    );
                    if let Some(hidden) = &alias {
                        self.locals.get_mut(&name.name).expect("catch alias").source_role = SourceRole::CatchBinding {
                            hidden: hidden.clone(), access: self.local(&pair[1], hidden.id),
                        };
                    }
                    source_bindings.push(self.locals[&name.name].clone());
                    if alias.is_none() { bindings.push(Binding {
                        id,
                        name: name.name.clone(),
                        metadata: pair[0].metadata.clone(),
                        span: pair[0].span.clone(),
                        value,
                    }); }
                }
                let target = LoopId(self.next_loop);
                let outer_target = self.target;
                if is_loop {
                    self.next_loop += 1;
                    self.target = Some((target, bindings.len()));
                }
                // Pinned analyze-let-body explicitly analyzes a synthetic do
                // in this lexical scope. This is its genuine source analysis,
                // not a body reconstructed from the physical Do expression.
                let body = Box::new(self.analyzed_body(&args[1..], form.span.clone(), context.returning(), is_loop || tail)?);
                *self.source_nodes.last_mut().expect("source node fact slot") = Some(std::sync::Arc::new(SourceNode::Bindings {
                    is_loop, bindings: source_bindings.into(), body: std::sync::Arc::new(body.as_ref().clone()),
                }));
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
                let callee = self.form(&items[0])?;
                let arguments = args
                    .iter()
                    .map(|arg| self.form(arg))
                    .collect::<Result<Vec<_>, _>>()?;
                *self.source_nodes.last_mut().expect("source node fact slot") = Some(std::sync::Arc::new(SourceNode::Invoke {
                    callee: std::sync::Arc::new(callee), arguments: arguments.clone().into(),
                }));
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
            source: None,
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
        Expression::DynamicScope { bindings, body } | Expression::AsyncDynamicScope { bindings, body } => {
            for (_, value) in bindings {
                free_bindings(value, bound, free);
            }
            free_bindings(body, bound, free);
        }
        Expression::Throw(value) | Expression::NilTest(value) | Expression::Await { value } => free_bindings(value, bound, free),
        Expression::AsyncTry { body, handler, cleanup, payload } => {
            free_bindings(body, bound, free);
            if let Some(handler) = handler {
                let mut bound = bound.clone();
                if let Some(payload) = payload { bound.insert(payload.id); }
                free_bindings(handler, &bound, free);
            }
            if let Some(cleanup) = cleanup { free_bindings(cleanup, bound, free); }
        }
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
        Expression::Function { captures, .. } | Expression::GeneralFunction { captures, .. }
        | Expression::Future { captures, .. } => {
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
    prepare_with_origin(forms, span, environment, phase, expander, None)
}
pub(crate) fn prepare_with_origin(
    forms: &[Form],
    span: Range<usize>,
    environment: &Environment,
    phase: Phase,
    expander: &mut dyn super::ExpansionHost,
    origin: Option<&super::SourceOrigin>,
) -> Result<(Hir, Environment), Diagnostic> {
    let mut analyzer = Analyzer {
        suspendable: false,
        expander,
        origin: origin.cloned(),
        environment: environment.clone(),
        phase,
        locals: HashMap::new(),
        fields: HashMap::new(),
        function_scopes: Vec::new(),
        next: 0,
        next_loop: 0,
        analysis_depth: 0,
        analysis_contexts: Vec::new(),
        source_namespace: None,
        namespace_snapshot: None,
        source_callables: Vec::new(),
        source_nodes: Vec::new(),
        callable_keys: BTreeMap::new(),
        catch_aliases: Vec::new(),
        target: None,
        method_recurrence: Vec::new(),
    };
    let mut hir = analyzer.body(forms, span, super::AnalysisContext::Statement, false)?;
    if !analyzer.callable_keys.is_empty() {
        let mut initializers = analyzer.callable_keys.into_values().collect::<Vec<_>>();
        let ty = hir.ty;
        let span = hir.span.clone();
        let metadata = hir.metadata.clone();
        initializers.push(hir);
        hir = Hir {
            source: None,
            span,
            metadata,
            ty,
            kind: Expression::Do(initializers),
        };
    }
    Ok((hir, analyzer.environment))
}
