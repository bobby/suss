//! Core's analysis as EDN, for the build cache (#299 step 3).
//!
//! `bootstrap::catalog` analyzes the shipped core once per phase, and its
//! result depends only on the compiler and the embedded sources. This module
//! writes that result as EDN and reads it back. The layout is in
//! `docs/runtime/build-cache.md`.
//!
//! The analysis is a graph of reference-counted values, and the macro
//! environment caches records by identity, so which values share an
//! allocation is part of its meaning. Each shared value is written once, as a
//! row of its table, and referred to by its index. Plain values (forms, HIR,
//! bindings) are written once per distinct content. Rows are written children
//! first, so reading them in order needs no fixups.
use super::{
    AnalysisContext, SourceOrigin,
    hir::{
        ArrayOperation, Arithmetic, Binding, Bitwise, BindingId, Comparison, Expression, FieldBinding,
        FunctionScope, Hir, Literal, LocalBinding, LocalKind, LoopId, Method, NativeKind, Nominal,
        Parameter, SourceAnalysis, SourceBinding, SourceCallable, SourceFunctionParameters,
        SourceMethod, SourceMethodEnvironment, SourceNamespace, SourceNode, SourceParameterMethod,
        SourceRole, SourceTags, Type,
    },
    resolve::{DefinitionInfo, Global, Phase},
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    ops::Range,
    sync::Arc,
};
use suss_reader::{
    Keyword, Symbol,
    edn::Value,
    forms::{Form, Kind},
};

/// Bump when the layout changes; older entries become misses.
pub(crate) const VERSION: u32 = 1;

pub(crate) type R<T> = Result<T, String>;
thread_local! { pub(crate) static PROBE: std::cell::RefCell<BTreeMap<Table, (std::time::Duration, usize)>> = Default::default(); }

macro_rules! tables {
    ($($table:ident),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub(crate) enum Table { $($table),* }
        const TABLES: &[Table] = &[$(Table::$table),*];
    };
}
impl Table {
    /// Whether the compiler tells this table's values apart by allocation,
    /// so that equal values must stay distinct. Values of other tables are
    /// written once per distinct content and shared when read.
    fn has_identity(self) -> bool {
        matches!(
            self,
            Table::Analysis | Table::Namespace | Table::Definition | Table::Callable | Table::Scope
                | Table::Identity | Table::Parameters
        )
    }
    /// Whether a row's referrers are counted, so the last one can take the
    /// value instead of copying it.
    fn counted(self) -> bool {
        matches!(self, Table::Form | Table::Hir)
    }
}
tables! {
    Str, Global, Form, ReaderForms, Origin, Hir, ArcHir, HirSlice, Analysis, Namespace,
    Definition, Callable, Node, Scope, Scopes, Identity, Local, ArcLocal, LocalSlice, Locals,
    Field, ArcField, Fields, Parameters,
}

/// A fieldless enum as its variant's position.
macro_rules! unit_enum {
    ($vis:vis $encode:ident, $decode:ident, $ty:ident { $($variant:ident),* $(,)? }) => {
        $vis fn $encode(value: $ty) -> Value {
            match value { $($ty::$variant)|* => {} }
            let all = [$($ty::$variant),*];
            number(all.iter().position(|v| *v == value).unwrap_or_default())
        }
        $vis fn $decode(value: &Value) -> R<$ty> {
            let all = [$($ty::$variant),*];
            all.get(index(value)?).copied().ok_or_else(|| format!("Unknown {}", stringify!($ty)))
        }
    };
}
pub(crate) use unit_enum;
unit_enum!(pub(crate) encode_phase, decode_phase, Phase { Runtime, Macro });
unit_enum!(encode_context, decode_context, AnalysisContext { Statement, Expression, Return });
unit_enum!(pub(crate) encode_arithmetic, decode_arithmetic, Arithmetic { Add, Subtract, Multiply, Divide, Negate });
unit_enum!(encode_native, decode_native, NativeKind { Nil, Boolean, Number, String, Function, Object, Array, Default });
unit_enum!(pub(crate) encode_array, decode_array, ArrayOperation { Literal, Make, MakeLiteral, Length, Get, Set });
unit_enum!(pub(crate) encode_comparison, decode_comparison, Comparison {
    Less, LessEqual, Greater, GreaterEqual, StrictEqual, NumberLess, NumberLessEqual, NumberGreater,
    NumberGreaterEqual, NumberEqual, NumberZero, NumberPositive, NumberNegative,
});
unit_enum!(pub(crate) encode_bitwise, decode_bitwise, Bitwise {
    Int, And, Or, Xor, AndNot, Not, Clear, Flip, Set, Test, Left, Right, Unsigned, Imul, F64Coerce,
    F64Word0, F64Word4, F64Floor, F64Ceil, F64Finite, F64SafeInteger, F64TimeClip,
    SafeIntegerRemainder, F64Remainder, IdentityUid,
});
/// The variants of `Nominal` without data, then those with data.
macro_rules! nominal {
    ($($variant:ident),* $(,)?) => {
        fn encode_nominal(value: Nominal) -> Value {
            let units = [$(Nominal::$variant),*];
            let data = |tag: usize, payload: Value| Value::Vector(vec![number(tag), payload]);
            match value {
                $(Nominal::$variant)|* => number(units.iter().position(|v| *v == value).unwrap_or_default()),
                Nominal::NativeMarker(kind) => data(0, encode_native(kind)),
                Nominal::NativeSet(kind) => data(1, encode_native(kind)),
                Nominal::Descriptor(n) => data(2, number(n)),
                Nominal::Field(n) => data(3, number(n)),
                Nominal::FieldSet(n) => data(4, number(n)),
                Nominal::Key(n) => data(5, number(n)),
            }
        }
        fn decode_nominal(value: &Value) -> R<Nominal> {
            let units = [$(Nominal::$variant),*];
            if let Value::Number(_) = value {
                return units.get(index(value)?).copied().ok_or_else(|| "Unknown Nominal".to_owned());
            }
            let [tag, payload] = fixed(value)?;
            Ok(match index(tag)? {
                0 => Nominal::NativeMarker(decode_native(payload)?),
                1 => Nominal::NativeSet(decode_native(payload)?),
                2 => Nominal::Descriptor(index(payload)?),
                3 => Nominal::Field(index(payload)?),
                4 => Nominal::FieldSet(index(payload)?),
                5 => Nominal::Key(index(payload)?),
                _ => return Err("Unknown Nominal".into()),
            })
        }
    };
}
nominal! {
    IsNumber, IsString, IsBoolean, IsClosure, BindCallable, BindingDefined, ObjectDescriptor,
    MethodGet, LiveDispatcher, IFnLiveDispatcher, Array, Class, Protocol, Construct, Instance,
    SameNominalType, NamedGet, CallableGet, NamedSet, CoreDescriptor, CoreStaticsDescriptor,
    CoreClosedDescriptor, CoreConstructibleDescriptor, CoreNamedGet, CoreNamedSet,
    BuiltinClassGuard, BuiltinConstructGuard, NativeObjectFactory, IsNativeObject,
    IsTypeConstructor, ValueConstructor, SourceFunctionName, NativeObjectDefaultPrototype,
    CurrentThis, NativeObjectGet, NativeObjectSet, NativeObjectStrictSet, CoerceString,
    ObjectCoerceString, ConcatString, ValueAdd, StringIndexOf, StringSlice, StringFromUnit,
    LanguageError, IsLanguageError, ObjectSet, ObjectInvoke, Dispatcher, Set, Marker, Satisfies,
    NativeSatisfies, TypeName,
}

pub(crate) fn number(n: usize) -> Value {
    Value::Number(n as f64)
}
fn float(x: f64) -> Value {
    // EDN prints every NaN alike; keep the bits.
    if x.is_nan() { Value::String(format!("{:016x}", x.to_bits())) } else { Value::Number(x) }
}
fn decode_float(value: &Value) -> R<f64> {
    match value {
        Value::Number(x) => Ok(*x),
        Value::String(bits) => u64::from_str_radix(bits, 16).map(f64::from_bits).map_err(|e| e.to_string()),
        _ => Err("Expected a number".into()),
    }
}
fn boolean(b: bool) -> Value {
    Value::Bool(b)
}
pub(crate) fn decode_bool(value: &Value) -> R<bool> {
    match value {
        Value::Bool(b) => Ok(*b),
        _ => Err("Expected a boolean".into()),
    }
}
pub(crate) fn index(value: &Value) -> R<usize> {
    match value {
        Value::Number(x) if *x >= 0.0 && x.fract() == 0.0 && *x < 9.0e15 => Ok(*x as usize),
        _ => Err(format!("Expected an index, not {value:?}")),
    }
}
pub(crate) fn items(value: &Value) -> R<&[Value]> {
    match value {
        Value::Vector(items) => Ok(items),
        _ => Err("Expected a vector".into()),
    }
}
pub(crate) fn fixed<const N: usize>(value: &Value) -> R<&[Value; N]> {
    items(value)?.try_into().map_err(|_| format!("Expected {N} items"))
}
fn optional<T>(value: &Value, f: impl FnOnce(&Value) -> R<T>) -> R<Option<T>> {
    if let Value::Nil = value { Ok(None) } else { f(value).map(Some) }
}
fn units(units: &[u16]) -> Value {
    match String::from_utf16(units) {
        Ok(text) => Value::String(text),
        Err(_) => Value::Vector(units.iter().map(|&u| number(usize::from(u))).collect()),
    }
}
fn decode_units(value: &Value) -> R<Vec<u16>> {
    match value {
        Value::String(text) => Ok(text.encode_utf16().collect()),
        Value::Vector(items) => items
            .iter()
            .map(|item| index(item).and_then(|u| u16::try_from(u).map_err(|e| e.to_string())))
            .collect(),
        _ => Err("Expected UTF-16 text".into()),
    }
}
/// A span as its start and length.
fn span(range: &Range<usize>) -> [Value; 2] {
    [number(range.start), number(range.end.saturating_sub(range.start))]
}
fn decode_span(start: &Value, length: &Value) -> R<Range<usize>> {
    let start = index(start)?;
    Ok(start..start + index(length)?)
}
fn encode_type(ty: Type) -> Value {
    match ty {
        Type::Nil => number(0),
        Type::Bool => number(1),
        Type::Number => number(2),
        Type::String => number(3),
        Type::Value => number(4),
        Type::Closure(arity) => Value::Vector(vec![number(arity)]),
    }
}
fn decode_type(value: &Value) -> R<Type> {
    if let Value::Vector(_) = value {
        let [arity] = fixed(value)?;
        return Ok(Type::Closure(index(arity)?));
    }
    Ok(match index(value)? {
        0 => Type::Nil,
        1 => Type::Bool,
        2 => Type::Number,
        3 => Type::String,
        4 => Type::Value,
        _ => return Err("Unknown type".into()),
    })
}
fn identifiers(ids: &[BindingId]) -> Value {
    Value::Vector(ids.iter().map(|id| number(id.0)).collect())
}
fn decode_identifiers(value: &Value) -> R<Vec<BindingId>> {
    items(value)?.iter().map(|id| index(id).map(BindingId)).collect()
}

/// Writes rows; see the module documentation.
pub(crate) struct Encoder<'a> {
    rows: Vec<Value>,
    counts: Vec<usize>,
    content: Vec<BTreeMap<Vec<Value>, usize>>,
    pointers: HashMap<(Table, usize), usize>,
    /// How many references to each row of a counted table were written.
    uses: Vec<Vec<u32>>,
    /// The core source text, which every origin must share.
    text: &'a str,
    /// The allocation of that text that the origins share.
    shared_text: Option<*const u8>,
    /// The latest snapshot written for each namespace, as a delta base.
    namespaces: HashMap<String, (Arc<SourceNamespace>, usize)>,
}
impl<'a> Encoder<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        Self {
            rows: Vec::new(),
            counts: vec![0; TABLES.len()],
            content: vec![BTreeMap::new(); TABLES.len()],
            pointers: HashMap::new(),
            uses: vec![Vec::new(); TABLES.len()],
            text,
            shared_text: None,
            namespaces: HashMap::new(),
        }
    }
    fn push(&mut self, table: Table, fields: Vec<Value>) -> Value {
        let i = self.counts[table as usize];
        self.counts[table as usize] += 1;
        if table.counted() {
            self.uses[table as usize].push(0);
        }
        let mut row = Vec::with_capacity(fields.len() + 1);
        row.push(number(table as usize));
        row.extend(fields);
        self.rows.push(Value::Vector(row));
        number(i)
    }
    /// A row deduplicated by content.
    fn content(&mut self, table: Table, fields: Vec<Value>) -> Value {
        let i = match self.content[table as usize].get(&fields) {
            Some(&i) => i,
            None => {
                let i = index(&self.push(table, fields.clone())).unwrap_or_default();
                self.content[table as usize].insert(fields, i);
                i
            }
        };
        // A referrer that turns out to duplicate a row also counts here, so a
        // count may exceed the references read; reading then copies instead.
        if table.counted() {
            self.uses[table as usize][i] += 1;
        }
        number(i)
    }
    fn seen<T: ?Sized>(&self, table: Table, pointer: *const T) -> Option<Value> {
        self.pointers.get(&(table, pointer as *const () as usize)).map(|&i| number(i))
    }
    /// A row for one shared allocation.
    fn shared<T: ?Sized>(&mut self, table: Table, pointer: *const T, fields: Vec<Value>) -> Value {
        let i = if table.has_identity() { self.push(table, fields) } else { self.content(table, fields) };
        self.pointers.insert((table, pointer as *const () as usize), index(&i).unwrap_or_default());
        i
    }

    pub(crate) fn string(&mut self, text: &str) -> Value {
        self.content(Table::Str, vec![Value::String(text.to_owned())])
    }
    pub(crate) fn global(&mut self, global: &Global) -> Value {
        let fields = vec![encode_phase(global.phase()), self.string(global.namespace()), self.string(global.name())];
        self.content(Table::Global, fields)
    }
    fn optional_global(&mut self, global: Option<&Global>) -> Value {
        global.map_or(Value::Nil, |global| self.global(global))
    }
    pub(crate) fn form(&mut self, form: &Form) -> R<Value> {
        stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || self.form_inner(form))
    }
    fn form_inner(&mut self, form: &Form) -> R<Value> {
        let mut fields = span(&form.span).to_vec();
        match &form.kind {
            Kind::Nil => fields.push(number(0)),
            Kind::Bool(b) => fields.extend([number(1), boolean(*b)]),
            Kind::Number(x) => fields.extend([number(2), float(*x)]),
            Kind::String(text) => fields.extend([number(3), units(text)]),
            Kind::Symbol(symbol) => {
                let namespace = symbol.namespace.as_deref().map_or(Value::Nil, |ns| self.string(ns));
                fields.extend([number(4), namespace, self.string(&symbol.name)]);
            }
            Kind::Keyword(keyword) => {
                let namespace = keyword.namespace.as_deref().map_or(Value::Nil, |ns| self.string(ns));
                fields.extend([number(5), namespace, self.string(&keyword.name)]);
            }
            Kind::List(items) => fields.extend([number(6), self.forms(items)?]),
            Kind::Vector(items) => fields.extend([number(7), self.forms(items)?]),
            Kind::Set(items) => fields.extend([number(8), self.forms(items)?]),
            Kind::Map(items) => fields.extend([number(9), self.forms(items)?]),
            _ => return Err("Unresolved reader syntax in core's analysis".into()),
        }
        if !form.metadata.is_empty() {
            fields.push(self.forms(&form.metadata)?);
        }
        Ok(self.content(Table::Form, fields))
    }
    pub(crate) fn forms(&mut self, forms: &[Form]) -> R<Value> {
        forms.iter().map(|form| self.form(form)).collect::<R<_>>().map(Value::Vector)
    }
    pub(crate) fn optional_form(&mut self, form: Option<&Form>) -> R<Value> {
        form.map_or(Ok(Value::Nil), |form| self.form(form))
    }
    fn origin(&mut self, origin: Option<&SourceOrigin>) -> R<Value> {
        let Some(origin) = origin else { return Ok(Value::Nil) };
        let pointer = origin.text().as_ptr();
        let shared = *self.shared_text.get_or_insert(pointer);
        if pointer != shared || origin.text() != self.text || origin.path().is_some() || !origin.is_privileged()
            || origin.metadata_initialized()
        {
            return Err("Core's analysis has an origin other than core".into());
        }
        let reader = match origin.reader_forms() {
            None => Value::Nil,
            Some(pair) => match self.seen(Table::ReaderForms, Arc::as_ptr(pair)) {
                Some(i) => i,
                None => {
                    let fields = vec![self.form(&pair.0)?, self.form(&pair.1)?];
                    self.shared(Table::ReaderForms, Arc::as_ptr(pair), fields)
                }
            },
        };
        Ok(self.content(Table::Origin, vec![reader]))
    }
    fn hirs(&mut self, hirs: &[Hir]) -> R<Value> {
        hirs.iter().map(|hir| self.hir(hir)).collect::<R<_>>().map(Value::Vector)
    }
    fn optional_hir(&mut self, hir: Option<&Hir>) -> R<Value> {
        hir.map_or(Ok(Value::Nil), |hir| self.hir(hir))
    }
    fn parameter(&mut self, parameter: &Parameter) -> R<Value> {
        let [start, end] = span(&parameter.span);
        Ok(Value::Vector(vec![
            number(parameter.id.0),
            self.string(&parameter.name),
            self.forms(&parameter.metadata)?,
            start,
            end,
        ]))
    }
    fn parameters(&mut self, parameters: &[Parameter]) -> R<Value> {
        parameters.iter().map(|p| self.parameter(p)).collect::<R<_>>().map(Value::Vector)
    }
    fn bindings(&mut self, bindings: &[Binding]) -> R<Value> {
        bindings
            .iter()
            .map(|binding| {
                let [start, end] = span(&binding.span);
                Ok(Value::Vector(vec![
                    number(binding.id.0),
                    self.string(&binding.name),
                    self.forms(&binding.metadata)?,
                    start,
                    end,
                    self.hir(&binding.value)?,
                ]))
            })
            .collect::<R<_>>()
            .map(Value::Vector)
    }
    fn scoped(&mut self, bindings: &[(Global, Hir)]) -> R<Value> {
        bindings
            .iter()
            .map(|(global, hir)| Ok(Value::Vector(vec![self.global(global), self.hir(hir)?])))
            .collect::<R<_>>()
            .map(Value::Vector)
    }
    pub(crate) fn hir(&mut self, hir: &Hir) -> R<Value> {
        stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || self.hir_inner(hir))
    }
    fn hir_inner(&mut self, hir: &Hir) -> R<Value> {
        let source = match &hir.source {
            Some(source) => self.analysis(source)?,
            None => Value::Nil,
        };
        // Most nodes span their source form.
        let extent = if hir.source.as_ref().is_some_and(|s| s.form.span == hir.span) {
            Value::Nil
        } else {
            Value::Vector(span(&hir.span).to_vec())
        };
        let mut fields = vec![source, extent, self.forms(&hir.metadata)?, encode_type(hir.ty)];
        use Expression as E;
        let mut kind = |tag: usize, payload: Vec<Value>| {
            fields.push(number(tag));
            fields.extend(payload);
        };
        match &hir.kind {
            E::Future { captures, body } => kind(0, vec![identifiers(captures), self.hir(body)?]),
            E::Await { value } => kind(1, vec![self.hir(value)?]),
            E::AsyncTry { body, handler, cleanup, payload } => {
                let payload = payload.as_ref().map_or(Ok(Value::Nil), |p| self.parameter(p))?;
                kind(2, vec![self.hir(body)?, self.optional_hir(handler.as_deref())?, self.optional_hir(cleanup.as_deref())?, payload])
            }
            E::AsyncDynamicScope { bindings, body } => kind(3, vec![self.scoped(bindings)?, self.hir(body)?]),
            E::Bitwise { operation, arguments } => kind(4, vec![encode_bitwise(*operation), self.hirs(arguments)?]),
            E::Comparison { operation, arguments } => kind(5, vec![encode_comparison(*operation), self.hirs(arguments)?]),
            E::NilTest(value) => kind(6, vec![self.hir(value)?]),
            E::Array { operation, arguments } => kind(7, vec![encode_array(*operation), self.hirs(arguments)?]),
            E::DynamicScope { bindings, body } => kind(8, vec![self.scoped(bindings)?, self.hir(body)?]),
            E::Assign { global, value, linked } => {
                kind(9, vec![self.global(global), self.hir(value)?, self.optional_global(linked.as_ref())])
            }
            E::Throw(value) => kind(10, vec![self.hir(value)?]),
            E::Try { regions } => kind(11, vec![self.hir(&regions[0])?, self.hir(&regions[1])?, self.hir(&regions[2])?]),
            E::Nominal { operation, arguments } => kind(12, vec![encode_nominal(*operation), self.hirs(arguments)?]),
            E::Literal(literal) => kind(13, match literal {
                Literal::Nil => vec![number(0)],
                Literal::Undefined => vec![number(1)],
                Literal::Bool(b) => vec![number(2), boolean(*b)],
                Literal::Number(x) => vec![number(3), float(*x)],
                Literal::String(text) => vec![number(4), units(text)],
            }),
            E::Local(id) => kind(14, vec![number(id.0)]),
            E::Global(global) => kind(15, vec![self.global(global)]),
            E::GlobalOrFallback { global, fallback } => kind(16, vec![self.global(global), self.global(fallback)]),
            E::GlobalCell(global) => kind(17, vec![self.global(global)]),
            E::Definition { global, name_metadata, name_span, docstring, initializer, once, linked } => {
                let [start, end] = span(name_span);
                kind(18, vec![
                    self.global(global),
                    self.forms(name_metadata)?,
                    start,
                    end,
                    docstring.as_deref().map_or(Value::Nil, units),
                    self.optional_hir(initializer.as_deref())?,
                    boolean(*once),
                    self.optional_global(linked.as_ref()),
                ])
            }
            E::Let { bindings, body } => kind(19, vec![self.bindings(bindings)?, self.hir(body)?]),
            E::Loop { target, bindings, body } => {
                kind(20, vec![number(target.0), self.bindings(bindings)?, self.hir(body)?])
            }
            E::Recur { target, arguments } => kind(21, vec![number(target.0), self.hirs(arguments)?]),
            E::If { condition, consequent, alternative } => {
                kind(22, vec![self.hir(condition)?, self.hir(consequent)?, self.hir(alternative)?])
            }
            E::Do(items) => kind(23, vec![self.hirs(items)?]),
            E::Function { parameters, captures, body } => {
                kind(24, vec![self.parameters(parameters)?, identifiers(captures), self.hir(body)?])
            }
            E::GeneralFunction { methods, captures, self_binding, rest_class } => {
                let methods = methods
                    .iter()
                    .map(|m| Ok(Value::Vector(vec![boolean(m.variadic), self.parameters(&m.parameters)?, self.hir(&m.body)?])))
                    .collect::<R<_>>()?;
                let self_binding = self_binding.as_ref().map_or(Ok(Value::Nil), |p| self.parameter(p))?;
                kind(25, vec![Value::Vector(methods), identifiers(captures), self_binding, self.optional_global(rest_class.as_ref())])
            }
            E::Call { callee, arguments } => kind(26, vec![self.hir(callee)?, self.hirs(arguments)?]),
            E::Arithmetic { operator, arguments, coercing } => {
                kind(27, vec![encode_arithmetic(*operator), self.hirs(arguments)?, boolean(*coercing)])
            }
        }
        Ok(self.content(Table::Hir, fields))
    }
    fn arc_hir(&mut self, hir: &Arc<Hir>) -> R<Value> {
        if let Some(i) = self.seen(Table::ArcHir, Arc::as_ptr(hir)) {
            return Ok(i);
        }
        let fields = vec![self.hir(hir)?];
        Ok(self.shared(Table::ArcHir, Arc::as_ptr(hir), fields))
    }
    fn optional_arc_hir(&mut self, hir: Option<&Arc<Hir>>) -> R<Value> {
        hir.map_or(Ok(Value::Nil), |hir| self.arc_hir(hir))
    }
    fn hir_slice(&mut self, hirs: &Arc<[Hir]>) -> R<Value> {
        if let Some(i) = self.seen(Table::HirSlice, Arc::as_ptr(hirs)) {
            return Ok(i);
        }
        let fields = vec![self.hirs(hirs)?];
        Ok(self.shared(Table::HirSlice, Arc::as_ptr(hirs), fields))
    }
    fn tags(&mut self, tags: &SourceTags) -> R<Value> {
        let mut twice = |tag: &Option<Option<Form>>| -> R<Value> {
            Ok(match tag {
                None => Value::Nil,
                Some(inner) => Value::Vector(vec![self.optional_form(inner.as_ref())?]),
            })
        };
        let quoted = twice(&tags.quoted_const_tag)?;
        let inferred_return = twice(&tags.inferred_return)?;
        Ok(Value::Vector(vec![
            self.optional_form(tags.tag.as_ref())?,
            quoted,
            inferred_return,
            self.optional_form(tags.inferred.as_ref())?,
        ]))
    }
    fn analysis(&mut self, analysis: &Arc<SourceAnalysis>) -> R<Value> {
        if let Some(i) = self.seen(Table::Analysis, Arc::as_ptr(analysis)) {
            return Ok(i);
        }
        let SourceAnalysis {
            is_body, tags, form, resolved, name_hint, scope, namespace_snapshot, callable, node, locals,
            fields, function_scopes, context, phase, namespace, origin,
        } = &**analysis;
        let resolved = match resolved {
            None => Value::Nil,
            Some(SourceBinding::JavaScript { global }) => Value::Vector(vec![number(0), self.global(global)]),
            Some(SourceBinding::Local(local)) => Value::Vector(vec![number(1), self.arc_local(local)?]),
            Some(SourceBinding::Field(field)) => Value::Vector(vec![number(2), self.arc_field(field)?]),
            Some(SourceBinding::Global { global, declaration }) => {
                let declaration = declaration.as_ref().map_or(Ok(Value::Nil), |d| self.definition(d))?;
                Value::Vector(vec![number(3), self.global(global), declaration])
            }
        };
        let row = vec![
            boolean(*is_body),
            self.tags(tags)?,
            self.form(form)?,
            resolved,
            self.optional_form(name_hint.as_ref())?,
            self.namespace(scope)?,
            self.namespace(namespace_snapshot)?,
            callable.as_ref().map_or(Ok(Value::Nil), |c| self.callable(c))?,
            node.as_ref().map_or(Ok(Value::Nil), |n| self.node(n))?,
            self.locals(locals)?,
            self.fields(fields)?,
            self.scopes(function_scopes)?,
            encode_context(*context),
            encode_phase(*phase),
            self.string(namespace),
            self.origin(origin.as_ref())?,
        ];
        Ok(self.shared(Table::Analysis, Arc::as_ptr(analysis), row))
    }
    fn string_set(&mut self, set: &BTreeSet<String>) -> Value {
        Value::Vector(set.iter().map(|s| self.string(s)).collect())
    }
    fn string_map(&mut self, map: &BTreeMap<String, String>) -> Value {
        Value::Vector(map.iter().flat_map(|(k, v)| [self.string(k), self.string(v)]).collect())
    }
    pub(crate) fn namespace(&mut self, namespace: &Arc<SourceNamespace>) -> R<Value> {
        if let Some(i) = self.seen(Table::Namespace, Arc::as_ptr(namespace)) {
            return Ok(i);
        }
        let SourceNamespace {
            namespace: name, aliases, refers, used_refers, renamed_refers, excluded_core, macro_aliases,
            macro_refers, used_macro_refers, renamed_macro_refers, declarations, identities,
        } = &**namespace;
        // Declarations and identities as changes from the namespace's
        // previous snapshot, since each top-level form adds a few.
        let base = self.namespaces.get(name).cloned();
        let (empty, no_identities) = Default::default();
        let (base_declarations, base_identities, base_row) = match &base {
            Some((base, row)) => (&base.declarations, &base.identities, number(*row)),
            None => (&empty, &no_identities, Value::Nil),
        };
        let removed = base_declarations
            .keys()
            .filter(|global| !declarations.contains_key(*global))
            .cloned()
            .collect::<Vec<_>>();
        let mut changed = Vec::new();
        for (global, info) in declarations {
            if !base_declarations.get(global).is_some_and(|old| Arc::ptr_eq(old, info)) {
                changed.push(self.global(global));
                changed.push(self.definition(info)?);
            }
        }
        let removed_identities = base_identities
            .iter()
            .filter(|global| !identities.contains(*global))
            .map(|global| self.global(global))
            .collect();
        let added_identities = identities
            .iter()
            .filter(|global| !base_identities.contains(*global))
            .map(|global| self.global(global))
            .collect();
        let row = vec![
            base_row,
            self.string(name),
            self.string_map(aliases),
            Value::Vector(refers.iter().flat_map(|(k, g)| [self.string(k), self.global(g)]).collect()),
            self.string_set(used_refers),
            self.string_set(renamed_refers),
            self.string_set(excluded_core),
            self.string_map(macro_aliases),
            Value::Vector(
                macro_refers.iter().flat_map(|(k, (a, b))| [self.string(k), self.string(a), self.string(b)]).collect(),
            ),
            self.string_set(used_macro_refers),
            self.string_set(renamed_macro_refers),
            Value::Vector(removed.iter().map(|g| self.global(g)).collect()),
            Value::Vector(changed),
            Value::Vector(removed_identities),
            Value::Vector(added_identities),
        ];
        let i = self.shared(Table::Namespace, Arc::as_ptr(namespace), row);
        self.namespaces.insert(name.clone(), (namespace.clone(), index(&i)?));
        Ok(i)
    }
    pub(crate) fn definition(&mut self, info: &Arc<DefinitionInfo>) -> R<Value> {
        if let Some(i) = self.seen(Table::Definition, Arc::as_ptr(info)) {
            return Ok(i);
        }
        let DefinitionInfo {
            definition_form, analysis_completed, declaration, docstring, origin, initializer_form,
            initializer, type_fields, type_is_record, once,
        } = &**info;
        let row = vec![
            self.form(definition_form)?,
            boolean(*analysis_completed),
            self.form(declaration)?,
            docstring.as_deref().map_or(Value::Nil, units),
            self.origin(origin.as_ref())?,
            self.optional_form(initializer_form.as_ref())?,
            self.optional_arc_hir(initializer.as_ref())?,
            type_fields.map_or(Value::Nil, number),
            boolean(*type_is_record),
            boolean(*once),
        ];
        Ok(self.shared(Table::Definition, Arc::as_ptr(info), row))
    }
    fn callable(&mut self, callable: &Arc<SourceCallable>) -> R<Value> {
        if let Some(i) = self.seen(Table::Callable, Arc::as_ptr(callable)) {
            return Ok(i);
        }
        let name = callable.name.as_ref().map_or(Ok(Value::Nil), |s| self.scope(s))?;
        let mut methods = Vec::new();
        for method in &callable.methods {
            let SourceMethod { form, environment, declarations, parameters, variadic, recurs, body } = method;
            let declarations = declarations.iter().map(|d| self.local(d)).collect::<R<_>>()?;
            methods.push(Value::Vector(vec![
                self.form(form)?,
                self.method_environment(environment)?,
                Value::Vector(declarations),
                self.parameters(parameters)?,
                boolean(*variadic),
                recurs.map_or(Value::Nil, boolean),
                self.arc_hir(body)?,
            ]));
        }
        Ok(self.shared(Table::Callable, Arc::as_ptr(callable), vec![name, Value::Vector(methods)]))
    }
    fn method_environment(&mut self, environment: &SourceMethodEnvironment) -> R<Value> {
        let SourceMethodEnvironment { scope, namespace_snapshot, locals, fields, function_scopes, context } = environment;
        Ok(Value::Vector(vec![
            self.namespace(scope)?,
            self.namespace(namespace_snapshot)?,
            self.locals(locals)?,
            self.fields(fields)?,
            self.scopes(function_scopes)?,
            encode_context(*context),
        ]))
    }
    fn node(&mut self, node: &Arc<SourceNode>) -> R<Value> {
        if let Some(i) = self.seen(Table::Node, Arc::as_ptr(node)) {
            return Ok(i);
        }
        use SourceNode as N;
        let tagged = |tag: usize, mut payload: Vec<Value>| {
            payload.insert(0, number(tag));
            payload
        };
        let row = match &**node {
            N::Future(x) => tagged(0, vec![self.arc_hir(x)?]),
            N::Await(x) => tagged(1, vec![self.arc_hir(x)?]),
            N::Vector(xs) => tagged(2, vec![self.hir_slice(xs)?]),
            N::Map(xs) => tagged(3, vec![self.hir_slice(xs)?]),
            N::Set(xs) => tagged(4, vec![self.hir_slice(xs)?]),
            N::Do { statements, result } => tagged(5, vec![self.hir_slice(statements)?, self.arc_hir(result)?]),
            N::If { condition, consequent, alternative } => {
                tagged(6, vec![self.arc_hir(condition)?, self.arc_hir(consequent)?, self.arc_hir(alternative)?])
            }
            N::Bindings { is_loop, bindings, body } => {
                let bindings = match self.seen(Table::LocalSlice, Arc::as_ptr(bindings)) {
                    Some(i) => i,
                    None => {
                        let locals = bindings.iter().map(|b| self.local(b)).collect::<R<_>>()?;
                        self.shared(Table::LocalSlice, Arc::as_ptr(bindings), vec![Value::Vector(locals)])
                    }
                };
                tagged(7, vec![boolean(*is_loop), bindings, self.arc_hir(body)?])
            }
            N::Recur(xs) => tagged(8, vec![self.hir_slice(xs)?]),
            N::Throw(x) => tagged(9, vec![self.arc_hir(x)?]),
            N::Try { body, handler, cleanup, payload } => tagged(10, vec![
                self.arc_hir(body)?,
                self.arc_hir(handler)?,
                self.optional_arc_hir(cleanup.as_ref())?,
                payload.as_ref().map_or(Ok(Value::Nil), |p| self.arc_local(p))?,
            ]),
            N::Assign { target, value } => tagged(11, vec![self.arc_hir(target)?, self.arc_hir(value)?]),
            N::Invoke { callee, arguments } => tagged(12, vec![self.arc_hir(callee)?, self.hir_slice(arguments)?]),
            N::Construct { class, arguments } => tagged(13, vec![self.arc_hir(class)?, self.hir_slice(arguments)?]),
            N::WithMeta { expression, metadata, inner } => tagged(14, vec![
                self.arc_hir(expression)?,
                self.arc_hir(metadata)?,
                inner.as_ref().map_or(Ok(Value::Nil), |n| self.node(n))?,
            ]),
        };
        Ok(self.shared(Table::Node, Arc::as_ptr(node), row))
    }
    fn scope(&mut self, scope: &Arc<FunctionScope>) -> R<Value> {
        if let Some(i) = self.seen(Table::Scope, Arc::as_ptr(scope)) {
            return Ok(i);
        }
        let FunctionScope {
            function_form, declaration, origin, namespace, declaration_context, scope: namespace_scope,
            namespace_snapshot, locals, fields, phase, parents, self_binding, shadow, shadow_field,
        } = &**scope;
        let row = vec![
            self.form(function_form)?,
            self.form(declaration)?,
            self.origin(origin.as_ref())?,
            self.string(namespace),
            encode_context(*declaration_context),
            self.namespace(namespace_scope)?,
            self.namespace(namespace_snapshot)?,
            self.locals(locals)?,
            self.fields(fields)?,
            encode_phase(*phase),
            self.scopes(parents)?,
            self_binding.as_ref().map_or(Ok(Value::Nil), |b| self.local(b))?,
            shadow.as_ref().map_or(Ok(Value::Nil), |b| self.arc_local(b))?,
            shadow_field.as_ref().map_or(Ok(Value::Nil), |f| self.arc_field(f))?,
        ];
        Ok(self.shared(Table::Scope, Arc::as_ptr(scope), row))
    }
    fn scopes(&mut self, scopes: &Arc<[Arc<FunctionScope>]>) -> R<Value> {
        if let Some(i) = self.seen(Table::Scopes, Arc::as_ptr(scopes)) {
            return Ok(i);
        }
        let row = scopes.iter().map(|s| self.scope(s)).collect::<R<_>>()?;
        Ok(self.shared(Table::Scopes, Arc::as_ptr(scopes), vec![Value::Vector(row)]))
    }
    fn identity(&mut self, identity: &Arc<()>) -> Value {
        match self.seen(Table::Identity, Arc::as_ptr(identity)) {
            Some(i) => i,
            None => self.shared(Table::Identity, Arc::as_ptr(identity), vec![]),
        }
    }
    fn local(&mut self, local: &LocalBinding) -> R<Value> {
        stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || self.local_inner(local))
    }
    fn local_inner(&mut self, local: &LocalBinding) -> R<Value> {
        let LocalBinding {
            identity, id, ty, declaration, origin, kind, declaration_context, source_role, initializer,
            shadow, shadow_field,
        } = local;
        let kind = match kind {
            LocalKind::Let => number(0),
            LocalKind::Loop => number(1),
            LocalKind::Argument { index, rest } => Value::Vector(vec![number(*index), boolean(*rest)]),
            LocalKind::FunctionName => number(3),
            LocalKind::Catch => number(4),
        };
        let role = match source_role {
            SourceRole::Plain => vec![number(0)],
            SourceRole::FunctionName { variadic, methods } => vec![
                number(1),
                boolean(*variadic),
                methods.as_ref().map_or(Ok(Value::Nil), |m| self.function_parameters(m))?,
            ],
            SourceRole::PrivateCatch { anchor } => {
                let [start, end] = span(anchor);
                vec![number(2), start, end]
            }
            SourceRole::CatchBinding { hidden, access } => vec![number(3), self.arc_local(hidden)?, self.hir(access)?],
            SourceRole::MethodArgument { index, rest } => vec![number(4), number(*index), boolean(*rest)],
            SourceRole::MethodThis {
                type_declaration, namespace, protocol_receiver, receiver_declaration, argument, access,
            } => vec![
                number(5),
                self.form(type_declaration)?,
                self.string(namespace),
                boolean(*protocol_receiver),
                self.form(receiver_declaration)?,
                argument.as_ref().map_or(Ok(Value::Nil), |a| self.arc_local(a))?,
                self.hir(access)?,
            ],
        };
        let row = vec![
            self.identity(identity),
            number(id.0),
            encode_type(*ty),
            self.form(declaration)?,
            self.origin(origin.as_ref())?,
            kind,
            encode_context(*declaration_context),
            Value::Vector(role),
            self.optional_arc_hir(initializer.as_ref())?,
            shadow.as_ref().map_or(Ok(Value::Nil), |s| self.arc_local(s))?,
            shadow_field.as_ref().map_or(Ok(Value::Nil), |f| self.arc_field(f))?,
        ];
        Ok(self.content(Table::Local, row))
    }
    fn arc_local(&mut self, local: &Arc<LocalBinding>) -> R<Value> {
        if let Some(i) = self.seen(Table::ArcLocal, Arc::as_ptr(local)) {
            return Ok(i);
        }
        let row = vec![self.local(local)?];
        Ok(self.shared(Table::ArcLocal, Arc::as_ptr(local), row))
    }
    fn locals(&mut self, locals: &Arc<HashMap<String, LocalBinding>>) -> R<Value> {
        if let Some(i) = self.seen(Table::Locals, Arc::as_ptr(locals)) {
            return Ok(i);
        }
        let mut entries = locals.iter().collect::<Vec<_>>();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        let mut row = Vec::with_capacity(entries.len() * 2);
        for (name, local) in entries {
            row.push(self.string(name));
            row.push(self.local(local)?);
        }
        Ok(self.shared(Table::Locals, Arc::as_ptr(locals), vec![Value::Vector(row)]))
    }
    fn field(&mut self, field: &FieldBinding) -> R<Value> {
        let FieldBinding { identity, declaration, origin, index, mutable, access } = field;
        let row = vec![
            self.identity(identity),
            self.form(declaration)?,
            self.origin(origin.as_ref())?,
            number(*index),
            boolean(*mutable),
            self.hir(access)?,
        ];
        Ok(self.content(Table::Field, row))
    }
    fn arc_field(&mut self, field: &Arc<FieldBinding>) -> R<Value> {
        if let Some(i) = self.seen(Table::ArcField, Arc::as_ptr(field)) {
            return Ok(i);
        }
        let row = vec![self.field(field)?];
        Ok(self.shared(Table::ArcField, Arc::as_ptr(field), row))
    }
    fn fields(&mut self, fields: &Arc<HashMap<String, FieldBinding>>) -> R<Value> {
        if let Some(i) = self.seen(Table::Fields, Arc::as_ptr(fields)) {
            return Ok(i);
        }
        let mut entries = fields.iter().collect::<Vec<_>>();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        let mut row = Vec::with_capacity(entries.len() * 2);
        for (name, field) in entries {
            row.push(self.string(name));
            row.push(self.field(field)?);
        }
        Ok(self.shared(Table::Fields, Arc::as_ptr(fields), vec![Value::Vector(row)]))
    }
    fn function_parameters(&mut self, parameters: &Arc<SourceFunctionParameters>) -> R<Value> {
        if let Some(i) = self.seen(Table::Parameters, Arc::as_ptr(parameters)) {
            return Ok(i);
        }
        let mut methods = Vec::new();
        for SourceParameterMethod { parameters, variadic } in &parameters.methods {
            let locals = parameters.iter().map(|p| self.local(p)).collect::<R<_>>()?;
            methods.push(Value::Vector(vec![Value::Vector(locals), boolean(*variadic)]));
        }
        Ok(self.shared(Table::Parameters, Arc::as_ptr(parameters), vec![Value::Vector(methods)]))
    }
}

/// Rows of a counted table: the last reference takes the value, and earlier
/// ones copy it.
struct Counted<T> {
    values: std::cell::RefCell<Vec<Option<T>>>,
    uses: std::cell::RefCell<Vec<u32>>,
}
impl<T: Clone> Counted<T> {
    fn new() -> Self {
        Self { values: Default::default(), uses: Default::default() }
    }
    fn push(&self, uses: u32, value: T) {
        self.values.borrow_mut().push(Some(value));
        self.uses.borrow_mut().push(uses);
    }
    fn get(&self, value: &Value) -> R<T> {
        let i = index(value)?;
        let mut uses = self.uses.borrow_mut();
        let remaining = uses.get_mut(i).ok_or("Reference to a later row")?;
        *remaining = remaining.checked_sub(1).ok_or("Row used more often than counted")?;
        let mut values = self.values.borrow_mut();
        let slot = &mut values[i];
        if *remaining == 0 { slot.take() } else { slot.clone() }.ok_or_else(|| "Row used more often than counted".into())
    }
}

/// Reads rows written by [`Encoder`], in order.
pub(crate) struct Decoder {
    base: SourceOrigin,
    strings: Vec<String>,
    globals: Vec<Global>,
    forms: Counted<Form>,
    reader_forms: Vec<Arc<(Form, Form)>>,
    origins: Vec<SourceOrigin>,
    hirs: Counted<Hir>,
    arc_hirs: Vec<Arc<Hir>>,
    hir_slices: Vec<Arc<[Hir]>>,
    analyses: Vec<Arc<SourceAnalysis>>,
    namespaces: Vec<Arc<SourceNamespace>>,
    definitions: Vec<Arc<DefinitionInfo>>,
    callables: Vec<Arc<SourceCallable>>,
    nodes: Vec<Arc<SourceNode>>,
    scopes: Vec<Arc<FunctionScope>>,
    scope_slices: Vec<Arc<[Arc<FunctionScope>]>>,
    identities: Vec<Arc<()>>,
    locals: Vec<LocalBinding>,
    arc_locals: Vec<Arc<LocalBinding>>,
    local_slices: Vec<Arc<[LocalBinding]>>,
    local_maps: Vec<Arc<HashMap<String, LocalBinding>>>,
    fields: Vec<FieldBinding>,
    arc_fields: Vec<Arc<FieldBinding>>,
    field_maps: Vec<Arc<HashMap<String, FieldBinding>>>,
    parameters: Vec<Arc<SourceFunctionParameters>>,
}
fn get<'a, T>(table: &'a [T], value: &Value) -> R<&'a T> {
    table.get(index(value)?).ok_or_else(|| "Reference to a later row".to_owned())
}
fn optional_get<T: Clone>(table: &[T], value: &Value) -> R<Option<T>> {
    optional(value, |v| get(table, v).cloned())
}
impl Decoder {
    pub(crate) fn new(base: SourceOrigin) -> Self {
        Self {
            base,
            strings: Vec::new(),
            globals: Vec::new(),
            forms: Counted::new(),
            reader_forms: Vec::new(),
            origins: Vec::new(),
            hirs: Counted::new(),
            arc_hirs: Vec::new(),
            hir_slices: Vec::new(),
            analyses: Vec::new(),
            namespaces: Vec::new(),
            definitions: Vec::new(),
            callables: Vec::new(),
            nodes: Vec::new(),
            scopes: Vec::new(),
            scope_slices: Vec::new(),
            identities: Vec::new(),
            locals: Vec::new(),
            arc_locals: Vec::new(),
            local_slices: Vec::new(),
            local_maps: Vec::new(),
            fields: Vec::new(),
            arc_fields: Vec::new(),
            field_maps: Vec::new(),
            parameters: Vec::new(),
        }
    }
    pub(crate) fn string(&self, value: &Value) -> R<String> {
        get(&self.strings, value).cloned()
    }
    pub(crate) fn global(&self, value: &Value) -> R<Global> {
        get(&self.globals, value).cloned()
    }
    fn optional_global(&self, value: &Value) -> R<Option<Global>> {
        optional_get(&self.globals, value)
    }
    pub(crate) fn form(&self, value: &Value) -> R<Form> {
        self.forms.get(value)
    }
    pub(crate) fn optional_form(&self, value: &Value) -> R<Option<Form>> {
        optional(value, |v| self.form(v))
    }
    pub(crate) fn forms(&self, value: &Value) -> R<Vec<Form>> {
        items(value)?.iter().map(|v| self.form(v)).collect()
    }
    pub(crate) fn definition(&self, value: &Value) -> R<Arc<DefinitionInfo>> {
        get(&self.definitions, value).cloned()
    }
    fn hir(&self, value: &Value) -> R<Hir> {
        self.hirs.get(value)
    }
    fn boxed(&self, value: &Value) -> R<Box<Hir>> {
        self.hir(value).map(Box::new)
    }
    fn optional_boxed(&self, value: &Value) -> R<Option<Box<Hir>>> {
        optional(value, |v| self.boxed(v))
    }
    fn hirs(&self, value: &Value) -> R<Vec<Hir>> {
        items(value)?.iter().map(|v| self.hir(v)).collect()
    }
    fn origin(&self, value: &Value) -> R<Option<SourceOrigin>> {
        optional_get(&self.origins, value)
    }
    fn local(&self, value: &Value) -> R<LocalBinding> {
        get(&self.locals, value).cloned()
    }
    fn arc_local(&self, value: &Value) -> R<Arc<LocalBinding>> {
        get(&self.arc_locals, value).cloned()
    }
    fn arc_field(&self, value: &Value) -> R<Arc<FieldBinding>> {
        get(&self.arc_fields, value).cloned()
    }
    fn arc_hir(&self, value: &Value) -> R<Arc<Hir>> {
        get(&self.arc_hirs, value).cloned()
    }
    fn hir_slice(&self, value: &Value) -> R<Arc<[Hir]>> {
        get(&self.hir_slices, value).cloned()
    }
    fn parameter(&self, value: &Value) -> R<Parameter> {
        let [id, name, metadata, start, end] = fixed(value)?;
        Ok(Parameter {
            id: BindingId(index(id)?),
            name: self.string(name)?,
            metadata: self.forms(metadata)?,
            span: decode_span(start, end)?,
        })
    }
    fn parameters(&self, value: &Value) -> R<Vec<Parameter>> {
        items(value)?.iter().map(|v| self.parameter(v)).collect()
    }
    fn bindings(&self, value: &Value) -> R<Vec<Binding>> {
        items(value)?
            .iter()
            .map(|v| {
                let [id, name, metadata, start, end, hir] = fixed(v)?;
                Ok(Binding {
                    id: BindingId(index(id)?),
                    name: self.string(name)?,
                    metadata: self.forms(metadata)?,
                    span: decode_span(start, end)?,
                    value: self.hir(hir)?,
                })
            })
            .collect()
    }
    fn scoped(&self, value: &Value) -> R<Vec<(Global, Hir)>> {
        items(value)?
            .iter()
            .map(|v| {
                let [global, hir] = fixed(v)?;
                Ok((self.global(global)?, self.hir(hir)?))
            })
            .collect()
    }
    fn string_set(&self, value: &Value) -> R<BTreeSet<String>> {
        items(value)?.iter().map(|v| self.string(v)).collect()
    }
    fn string_map(&self, value: &Value) -> R<BTreeMap<String, String>> {
        items(value)?
            .chunks(2)
            .map(|pair| match pair {
                [k, v] => Ok((self.string(k)?, self.string(v)?)),
                _ => Err("Odd map".into()),
            })
            .collect()
    }
    fn namespace(&self, value: &Value) -> R<Arc<SourceNamespace>> {
        get(&self.namespaces, value).cloned()
    }
    fn scopes(&self, value: &Value) -> R<Arc<[Arc<FunctionScope>]>> {
        get(&self.scope_slices, value).cloned()
    }
    fn local_map(&self, value: &Value) -> R<Arc<HashMap<String, LocalBinding>>> {
        get(&self.local_maps, value).cloned()
    }
    fn field_map(&self, value: &Value) -> R<Arc<HashMap<String, FieldBinding>>> {
        get(&self.field_maps, value).cloned()
    }

    /// Read one row.
    pub(crate) fn row(&mut self, row: &Value) -> R<()> {
        stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || self.row_inner(row))
    }
    fn row_inner(&mut self, row: &Value) -> R<()> {
        let row = items(row)?;
        let (table, fields) = row.split_first().ok_or("Empty row")?;
        let table = *TABLES.get(index(table)?).ok_or("Unknown table")?;
        let probe_start = std::time::Instant::now();
        let result = self.row_table(table, fields);
        PROBE.with(|p| { let mut p = p.borrow_mut(); let e = p.entry(table).or_default(); e.0 += probe_start.elapsed(); e.1 += 1; });
        result
    }
    fn row_table(&mut self, table: Table, fields: &[Value]) -> R<()> {
        match table {
            Table::Str => {
                let [Value::String(text)] = fields else { return Err("Bad string row".into()) };
                self.strings.push(text.clone());
            }
            Table::Global => {
                let [phase, namespace, name] = fields else { return Err("Bad global row".into()) };
                self.globals.push(Global::from_parts(
                    decode_phase(phase)?,
                    &self.string(namespace)?,
                    &self.string(name)?,
                ));
            }
            Table::Form => {
                let [uses, fields @ ..] = fields else { return Err("Bad form row".into()) };
                let form = self.decode_form(fields)?;
                self.forms.push(index(uses)? as u32, form);
            }
            Table::ReaderForms => {
                let [lowered, reader] = fields else { return Err("Bad reader forms row".into()) };
                self.reader_forms.push(Arc::new((self.form(lowered)?, self.form(reader)?)));
            }
            Table::Origin => {
                let [reader] = fields else { return Err("Bad origin row".into()) };
                let origin = match optional_get(&self.reader_forms, reader)? {
                    None => self.base.clone(),
                    Some(pair) => self.base.with_shared_reader_forms(pair),
                };
                self.origins.push(origin);
            }
            Table::Hir => {
                let [uses, fields @ ..] = fields else { return Err("Bad HIR row".into()) };
                let hir = self.decode_hir(fields)?;
                self.hirs.push(index(uses)? as u32, hir);
            }
            Table::ArcHir => {
                let [hir] = fields else { return Err("Bad shared HIR row".into()) };
                self.arc_hirs.push(Arc::new(self.hir(hir)?));
            }
            Table::HirSlice => {
                let [hirs] = fields else { return Err("Bad HIR slice row".into()) };
                self.hir_slices.push(self.hirs(hirs)?.into());
            }
            Table::Analysis => {
                let analysis = self.decode_analysis(fields)?;
                self.analyses.push(Arc::new(analysis));
            }
            Table::Namespace => {
                let namespace = self.decode_namespace(fields)?;
                self.namespaces.push(Arc::new(namespace));
            }
            Table::Definition => {
                let [definition_form, analysis_completed, declaration, docstring, origin, initializer_form,
                    initializer, type_fields, type_is_record, once] = fields
                else {
                    return Err("Bad definition row".into());
                };
                self.definitions.push(Arc::new(DefinitionInfo {
                    definition_form: self.form(definition_form)?,
                    analysis_completed: decode_bool(analysis_completed)?,
                    declaration: self.form(declaration)?,
                    docstring: optional(docstring, decode_units)?,
                    origin: self.origin(origin)?,
                    initializer_form: self.optional_form(initializer_form)?,
                    initializer: optional_get(&self.arc_hirs, initializer)?,
                    type_fields: optional(type_fields, index)?,
                    type_is_record: decode_bool(type_is_record)?,
                    once: decode_bool(once)?,
                }));
            }
            Table::Callable => {
                let [name, methods] = fields else { return Err("Bad callable row".into()) };
                let methods = items(methods)?
                    .iter()
                    .map(|method| {
                        let [form, environment, declarations, parameters, variadic, recurs, body] = fixed(method)?;
                        Ok(SourceMethod {
                            form: self.form(form)?,
                            environment: self.decode_method_environment(environment)?,
                            declarations: items(declarations)?.iter().map(|d| self.local(d)).collect::<R<_>>()?,
                            parameters: self.parameters(parameters)?,
                            variadic: decode_bool(variadic)?,
                            recurs: optional(recurs, decode_bool)?,
                            body: self.arc_hir(body)?,
                        })
                    })
                    .collect::<R<_>>()?;
                self.callables.push(Arc::new(SourceCallable { name: optional_get(&self.scopes, name)?, methods }));
            }
            Table::Node => {
                let node = self.decode_node(fields)?;
                self.nodes.push(Arc::new(node));
            }
            Table::Scope => {
                let scope = self.decode_scope(fields)?;
                self.scopes.push(Arc::new(scope));
            }
            Table::Scopes => {
                let [scopes] = fields else { return Err("Bad scopes row".into()) };
                let scopes = items(scopes)?.iter().map(|s| get(&self.scopes, s).cloned()).collect::<R<Vec<_>>>()?;
                self.scope_slices.push(scopes.into());
            }
            Table::Identity => self.identities.push(Arc::new(())),
            Table::Local => {
                let local = self.decode_local(fields)?;
                self.locals.push(local);
            }
            Table::ArcLocal => {
                let [local] = fields else { return Err("Bad shared local row".into()) };
                self.arc_locals.push(Arc::new(self.local(local)?));
            }
            Table::LocalSlice => {
                let [locals] = fields else { return Err("Bad local slice row".into()) };
                let locals = items(locals)?.iter().map(|l| self.local(l)).collect::<R<Vec<_>>>()?;
                self.local_slices.push(locals.into());
            }
            Table::Locals => {
                let [entries] = fields else { return Err("Bad locals row".into()) };
                let map = items(entries)?
                    .chunks(2)
                    .map(|pair| match pair {
                        [name, local] => Ok((self.string(name)?, self.local(local)?)),
                        _ => Err("Odd locals".to_owned()),
                    })
                    .collect::<R<HashMap<_, _>>>()?;
                self.local_maps.push(Arc::new(map));
            }
            Table::Field => {
                let [identity, declaration, origin, index_value, mutable, access] = fields else {
                    return Err("Bad field row".into());
                };
                self.fields.push(FieldBinding {
                    identity: get(&self.identities, identity)?.clone(),
                    declaration: self.form(declaration)?,
                    origin: self.origin(origin)?,
                    index: index(index_value)?,
                    mutable: decode_bool(mutable)?,
                    access: self.hir(access)?,
                });
            }
            Table::ArcField => {
                let [field] = fields else { return Err("Bad shared field row".into()) };
                self.arc_fields.push(Arc::new(get(&self.fields, field)?.clone()));
            }
            Table::Fields => {
                let [entries] = fields else { return Err("Bad fields row".into()) };
                let map = items(entries)?
                    .chunks(2)
                    .map(|pair| match pair {
                        [name, field] => Ok((self.string(name)?, get(&self.fields, field)?.clone())),
                        _ => Err("Odd fields".to_owned()),
                    })
                    .collect::<R<HashMap<_, _>>>()?;
                self.field_maps.push(Arc::new(map));
            }
            Table::Parameters => {
                let [methods] = fields else { return Err("Bad parameters row".into()) };
                let methods = items(methods)?
                    .iter()
                    .map(|m| {
                        let [parameters, variadic] = fixed(m)?;
                        Ok(SourceParameterMethod {
                            parameters: items(parameters)?.iter().map(|p| self.local(p)).collect::<R<_>>()?,
                            variadic: decode_bool(variadic)?,
                        })
                    })
                    .collect::<R<_>>()?;
                self.parameters.push(Arc::new(SourceFunctionParameters { methods }));
            }
        }
        Ok(())
    }
    fn decode_form(&self, fields: &[Value]) -> R<Form> {
        let [start, length, tag, rest @ ..] = fields else { return Err("Bad form row".into()) };
        let tag = index(tag)?;
        let arity = match tag {
            0 => 0,
            4 | 5 => 2,
            _ => 1,
        };
        let (payload, metadata) = rest.split_at(arity.min(rest.len()));
        let metadata = match metadata {
            [] => Vec::new(),
            [metadata] => self.forms(metadata)?,
            _ => return Err("Bad form row".into()),
        };
        let kind = match (tag, payload) {
            (0, []) => Kind::Nil,
            (1, [b]) => Kind::Bool(decode_bool(b)?),
            (2, [x]) => Kind::Number(decode_float(x)?),
            (3, [text]) => Kind::String(decode_units(text)?),
            (4, [namespace, name]) => Kind::Symbol(Symbol {
                namespace: optional(namespace, |v| self.string(v))?,
                name: self.string(name)?,
            }),
            (5, [namespace, name]) => Kind::Keyword(Keyword {
                namespace: optional(namespace, |v| self.string(v))?,
                name: self.string(name)?,
            }),
            (6, [children]) => Kind::List(self.forms(children)?),
            (7, [children]) => Kind::Vector(self.forms(children)?),
            (8, [children]) => Kind::Set(self.forms(children)?),
            (9, [children]) => Kind::Map(self.forms(children)?),
            _ => return Err("Bad form kind".into()),
        };
        Ok(Form { span: decode_span(start, length)?, metadata, kind })
    }
    fn decode_hir(&self, fields: &[Value]) -> R<Hir> {
        let [source, span, metadata, ty, tag, payload @ ..] = fields else {
            return Err("Bad HIR row".into());
        };
        let source = optional_get(&self.analyses, source)?;
        let span = match (span, &source) {
            (Value::Nil, Some(source)) => source.form.span.clone(),
            _ => {
                let [start, length] = fixed(span)?;
                decode_span(start, length)?
            }
        };
        use Expression as E;
        let kind = match (index(tag)?, payload) {
            (0, [captures, body]) => E::Future { captures: decode_identifiers(captures)?, body: self.boxed(body)? },
            (1, [value]) => E::Await { value: self.boxed(value)? },
            (2, [body, handler, cleanup, payload]) => E::AsyncTry {
                body: self.boxed(body)?,
                handler: self.optional_boxed(handler)?,
                cleanup: self.optional_boxed(cleanup)?,
                payload: optional(payload, |p| self.parameter(p))?,
            },
            (3, [bindings, body]) => E::AsyncDynamicScope { bindings: self.scoped(bindings)?, body: self.boxed(body)? },
            (4, [operation, arguments]) => E::Bitwise { operation: decode_bitwise(operation)?, arguments: self.hirs(arguments)? },
            (5, [operation, arguments]) => {
                E::Comparison { operation: decode_comparison(operation)?, arguments: self.hirs(arguments)? }
            }
            (6, [value]) => E::NilTest(self.boxed(value)?),
            (7, [operation, arguments]) => E::Array { operation: decode_array(operation)?, arguments: self.hirs(arguments)? },
            (8, [bindings, body]) => E::DynamicScope { bindings: self.scoped(bindings)?, body: self.boxed(body)? },
            (9, [global, value, linked]) => E::Assign {
                global: self.global(global)?,
                value: self.boxed(value)?,
                linked: self.optional_global(linked)?,
            },
            (10, [value]) => E::Throw(self.boxed(value)?),
            (11, [a, b, c]) => E::Try { regions: [self.boxed(a)?, self.boxed(b)?, self.boxed(c)?] },
            (12, [operation, arguments]) => E::Nominal { operation: decode_nominal(operation)?, arguments: self.hirs(arguments)? },
            (13, [literal, value @ ..]) => E::Literal(match (index(literal)?, value) {
                (0, []) => Literal::Nil,
                (1, []) => Literal::Undefined,
                (2, [b]) => Literal::Bool(decode_bool(b)?),
                (3, [x]) => Literal::Number(decode_float(x)?),
                (4, [text]) => Literal::String(decode_units(text)?),
                _ => return Err("Bad literal".into()),
            }),
            (14, [id]) => E::Local(BindingId(index(id)?)),
            (15, [global]) => E::Global(self.global(global)?),
            (16, [global, fallback]) => E::GlobalOrFallback { global: self.global(global)?, fallback: self.global(fallback)? },
            (17, [global]) => E::GlobalCell(self.global(global)?),
            (18, [global, name_metadata, name_start, name_end, docstring, initializer, once, linked]) => E::Definition {
                global: self.global(global)?,
                name_metadata: self.forms(name_metadata)?,
                name_span: decode_span(name_start, name_end)?,
                docstring: optional(docstring, decode_units)?,
                initializer: self.optional_boxed(initializer)?,
                once: decode_bool(once)?,
                linked: self.optional_global(linked)?,
            },
            (19, [bindings, body]) => E::Let { bindings: self.bindings(bindings)?, body: self.boxed(body)? },
            (20, [target, bindings, body]) => E::Loop {
                target: LoopId(index(target)?),
                bindings: self.bindings(bindings)?,
                body: self.boxed(body)?,
            },
            (21, [target, arguments]) => E::Recur { target: LoopId(index(target)?), arguments: self.hirs(arguments)? },
            (22, [condition, consequent, alternative]) => E::If {
                condition: self.boxed(condition)?,
                consequent: self.boxed(consequent)?,
                alternative: self.boxed(alternative)?,
            },
            (23, [items_value]) => E::Do(self.hirs(items_value)?),
            (24, [parameters, captures, body]) => E::Function {
                parameters: self.parameters(parameters)?,
                captures: decode_identifiers(captures)?,
                body: self.boxed(body)?,
            },
            (25, [methods, captures, self_binding, rest_class]) => E::GeneralFunction {
                methods: items(methods)?
                    .iter()
                    .map(|m| {
                        let [variadic, parameters, body] = fixed(m)?;
                        Ok(Method {
                            variadic: decode_bool(variadic)?,
                            parameters: self.parameters(parameters)?,
                            body: self.boxed(body)?,
                        })
                    })
                    .collect::<R<_>>()?,
                captures: decode_identifiers(captures)?,
                self_binding: optional(self_binding, |p| self.parameter(p))?,
                rest_class: self.optional_global(rest_class)?,
            },
            (26, [callee, arguments]) => E::Call { callee: self.boxed(callee)?, arguments: self.hirs(arguments)? },
            (27, [operator, arguments, coercing]) => E::Arithmetic {
                operator: decode_arithmetic(operator)?,
                arguments: self.hirs(arguments)?,
                coercing: decode_bool(coercing)?,
            },
            _ => return Err("Bad HIR kind".into()),
        };
        Ok(Hir {
            source,
            span,
            metadata: self.forms(metadata)?,
            ty: decode_type(ty)?,
            kind,
        })
    }
    fn decode_tags(&self, value: &Value) -> R<SourceTags> {
        let [tag, quoted, inferred_return, inferred] = fixed(value)?;
        let twice = |value: &Value| -> R<Option<Option<Form>>> {
            optional(value, |v| {
                let [inner] = fixed(v)?;
                self.optional_form(inner)
            })
        };
        Ok(SourceTags {
            tag: self.optional_form(tag)?,
            quoted_const_tag: twice(quoted)?,
            inferred_return: twice(inferred_return)?,
            inferred: self.optional_form(inferred)?,
        })
    }
    fn decode_analysis(&self, fields: &[Value]) -> R<SourceAnalysis> {
        let [is_body, tags, form, resolved, name_hint, scope, namespace_snapshot, callable, node, locals,
            fields_value, function_scopes, context, phase, namespace, origin] = fields
        else {
            return Err("Bad analysis row".into());
        };
        let resolved = optional(resolved, |v| {
            let parts = items(v)?;
            Ok(match (parts.first().map(index).transpose()?, &parts[1..]) {
                (Some(0), [global]) => SourceBinding::JavaScript { global: self.global(global)? },
                (Some(1), [local]) => SourceBinding::Local(self.arc_local(local)?),
                (Some(2), [field]) => SourceBinding::Field(self.arc_field(field)?),
                (Some(3), [global, declaration]) => SourceBinding::Global {
                    global: self.global(global)?,
                    declaration: optional_get(&self.definitions, declaration)?,
                },
                _ => return Err("Bad resolved binding".into()),
            })
        })?;
        Ok(SourceAnalysis {
            is_body: decode_bool(is_body)?,
            tags: self.decode_tags(tags)?,
            form: self.form(form)?,
            resolved,
            name_hint: self.optional_form(name_hint)?,
            scope: self.namespace(scope)?,
            namespace_snapshot: self.namespace(namespace_snapshot)?,
            callable: optional_get(&self.callables, callable)?,
            node: optional_get(&self.nodes, node)?,
            locals: self.local_map(locals)?,
            fields: self.field_map(fields_value)?,
            function_scopes: self.scopes(function_scopes)?,
            context: decode_context(context)?,
            phase: decode_phase(phase)?,
            namespace: self.string(namespace)?,
            origin: self.origin(origin)?,
        })
    }
    fn decode_namespace(&self, fields: &[Value]) -> R<SourceNamespace> {
        let [base, name, aliases, refers, used_refers, renamed_refers, excluded_core, macro_aliases,
            macro_refers, used_macro_refers, renamed_macro_refers, removed, changed, removed_identities, added] = fields
        else {
            return Err("Bad namespace row".into());
        };
        let base = optional_get(&self.namespaces, base)?;
        let mut declarations = base.as_ref().map(|b| b.declarations.clone()).unwrap_or_default();
        for global in items(removed)? {
            declarations.remove(&self.global(global)?);
        }
        for pair in items(changed)?.chunks(2) {
            let [global, info] = pair else { return Err("Odd declarations".into()) };
            declarations.insert(self.global(global)?, self.definition(info)?);
        }
        let mut identities = base.as_ref().map(|b| b.identities.clone()).unwrap_or_default();
        for global in items(removed_identities)? {
            identities.remove(&self.global(global)?);
        }
        for global in items(added)? {
            identities.insert(self.global(global)?);
        }
        Ok(SourceNamespace {
            namespace: self.string(name)?,
            aliases: self.string_map(aliases)?,
            refers: items(refers)?
                .chunks(2)
                .map(|pair| match pair {
                    [k, g] => Ok((self.string(k)?, self.global(g)?)),
                    _ => Err("Odd refers".to_owned()),
                })
                .collect::<R<_>>()?,
            used_refers: self.string_set(used_refers)?,
            renamed_refers: self.string_set(renamed_refers)?,
            excluded_core: self.string_set(excluded_core)?,
            macro_aliases: self.string_map(macro_aliases)?,
            macro_refers: items(macro_refers)?
                .chunks(3)
                .map(|triple| match triple {
                    [k, a, b] => Ok((self.string(k)?, (self.string(a)?, self.string(b)?))),
                    _ => Err("Odd macro refers".to_owned()),
                })
                .collect::<R<_>>()?,
            used_macro_refers: self.string_set(used_macro_refers)?,
            renamed_macro_refers: self.string_set(renamed_macro_refers)?,
            declarations,
            identities,
        })
    }
    fn decode_method_environment(&self, value: &Value) -> R<SourceMethodEnvironment> {
        let [scope, namespace_snapshot, locals, fields, function_scopes, context] = fixed(value)?;
        Ok(SourceMethodEnvironment {
            scope: self.namespace(scope)?,
            namespace_snapshot: self.namespace(namespace_snapshot)?,
            locals: self.local_map(locals)?,
            fields: self.field_map(fields)?,
            function_scopes: self.scopes(function_scopes)?,
            context: decode_context(context)?,
        })
    }
    fn decode_node(&self, fields: &[Value]) -> R<SourceNode> {
        let [tag, payload @ ..] = fields else { return Err("Bad node row".into()) };
        use SourceNode as N;
        Ok(match (index(tag)?, payload) {
            (0, [x]) => N::Future(self.arc_hir(x)?),
            (1, [x]) => N::Await(self.arc_hir(x)?),
            (2, [xs]) => N::Vector(self.hir_slice(xs)?),
            (3, [xs]) => N::Map(self.hir_slice(xs)?),
            (4, [xs]) => N::Set(self.hir_slice(xs)?),
            (5, [statements, result]) => N::Do { statements: self.hir_slice(statements)?, result: self.arc_hir(result)? },
            (6, [c, t, e]) => N::If { condition: self.arc_hir(c)?, consequent: self.arc_hir(t)?, alternative: self.arc_hir(e)? },
            (7, [is_loop, bindings, body]) => N::Bindings {
                is_loop: decode_bool(is_loop)?,
                bindings: get(&self.local_slices, bindings)?.clone(),
                body: self.arc_hir(body)?,
            },
            (8, [xs]) => N::Recur(self.hir_slice(xs)?),
            (9, [x]) => N::Throw(self.arc_hir(x)?),
            (10, [body, handler, cleanup, payload]) => N::Try {
                body: self.arc_hir(body)?,
                handler: self.arc_hir(handler)?,
                cleanup: optional_get(&self.arc_hirs, cleanup)?,
                payload: optional_get(&self.arc_locals, payload)?,
            },
            (11, [target, value]) => N::Assign { target: self.arc_hir(target)?, value: self.arc_hir(value)? },
            (12, [callee, arguments]) => N::Invoke { callee: self.arc_hir(callee)?, arguments: self.hir_slice(arguments)? },
            (13, [class, arguments]) => N::Construct { class: self.arc_hir(class)?, arguments: self.hir_slice(arguments)? },
            (14, [expression, metadata, inner]) => N::WithMeta {
                expression: self.arc_hir(expression)?,
                metadata: self.arc_hir(metadata)?,
                inner: optional_get(&self.nodes, inner)?,
            },
            _ => return Err("Bad node kind".into()),
        })
    }
    fn decode_scope(&self, fields: &[Value]) -> R<FunctionScope> {
        let [function_form, declaration, origin, namespace, declaration_context, scope, namespace_snapshot,
            locals, fields_value, phase, parents, self_binding, shadow, shadow_field] = fields
        else {
            return Err("Bad scope row".into());
        };
        Ok(FunctionScope {
            function_form: self.form(function_form)?,
            declaration: self.form(declaration)?,
            origin: self.origin(origin)?,
            namespace: self.string(namespace)?,
            declaration_context: decode_context(declaration_context)?,
            scope: self.namespace(scope)?,
            namespace_snapshot: self.namespace(namespace_snapshot)?,
            locals: self.local_map(locals)?,
            fields: self.field_map(fields_value)?,
            phase: decode_phase(phase)?,
            parents: self.scopes(parents)?,
            self_binding: optional(self_binding, |v| self.local(v))?,
            shadow: optional_get(&self.arc_locals, shadow)?,
            shadow_field: optional_get(&self.arc_fields, shadow_field)?,
        })
    }
    fn decode_local(&self, fields: &[Value]) -> R<LocalBinding> {
        let [identity, id, ty, declaration, origin, kind, declaration_context, role, initializer, shadow,
            shadow_field] = fields
        else {
            return Err("Bad local row".into());
        };
        let kind = match kind {
            Value::Vector(_) => {
                let [i, rest] = fixed(kind)?;
                LocalKind::Argument { index: index(i)?, rest: decode_bool(rest)? }
            }
            other => match index(other)? {
                0 => LocalKind::Let,
                1 => LocalKind::Loop,
                3 => LocalKind::FunctionName,
                4 => LocalKind::Catch,
                _ => return Err("Bad local kind".into()),
            },
        };
        let role = items(role)?;
        let source_role = match (role.first().map(index).transpose()?, &role[1.min(role.len())..]) {
            (Some(0), []) => SourceRole::Plain,
            (Some(1), [variadic, methods]) => SourceRole::FunctionName {
                variadic: decode_bool(variadic)?,
                methods: optional_get(&self.parameters, methods)?,
            },
            (Some(2), [start, end]) => SourceRole::PrivateCatch { anchor: decode_span(start, end)? },
            (Some(3), [hidden, access]) => SourceRole::CatchBinding { hidden: self.arc_local(hidden)?, access: self.hir(access)? },
            (Some(4), [i, rest]) => SourceRole::MethodArgument { index: index(i)?, rest: decode_bool(rest)? },
            (Some(5), [type_declaration, namespace, protocol_receiver, receiver_declaration, argument, access]) => {
                SourceRole::MethodThis {
                    type_declaration: self.form(type_declaration)?,
                    namespace: self.string(namespace)?,
                    protocol_receiver: decode_bool(protocol_receiver)?,
                    receiver_declaration: self.form(receiver_declaration)?,
                    argument: optional_get(&self.arc_locals, argument)?,
                    access: self.hir(access)?,
                }
            }
            _ => return Err("Bad source role".into()),
        };
        Ok(LocalBinding {
            identity: get(&self.identities, identity)?.clone(),
            id: BindingId(index(id)?),
            ty: decode_type(ty)?,
            declaration: self.form(declaration)?,
            origin: self.origin(origin)?,
            kind,
            declaration_context: decode_context(declaration_context)?,
            source_role,
            initializer: optional_get(&self.arc_hirs, initializer)?,
            shadow: optional_get(&self.arc_locals, shadow)?,
            shadow_field: optional_get(&self.arc_fields, shadow_field)?,
        })
    }
}

/// What `bootstrap::restore` keeps of core's analysis in one phase.
pub(crate) struct CoreAnalysis {
    pub environment: super::resolve::Environment,
    pub cells: Vec<Global>,
    pub namespace_directive: Option<Form>,
    /// The `#()` parameter numbers that reading core allocated.
    pub anonymous_ids: u64,
}

fn header_key(name: &str) -> Value {
    Value::keyword(&format!("suss.core-analysis/{name}"))
}

/// Core's analysis in `phase` as EDN text: a header map, one value per row,
/// then a map of the environment and the fragment's other results.
pub(crate) fn encode(
    analyzed: &super::AnalyzedFragment,
    text: &str,
    phase: Phase,
    anonymous_ids: u64,
) -> R<String> {
    let mut encoder = Encoder::new(text);
    // Write namespace snapshots in the order the forms took them, so each is
    // a small change from the one before.
    if let Expression::Do(forms) = &analyzed.hir.kind {
        for form in forms {
            if let Some(source) = &form.source {
                encoder.namespace(&source.scope)?;
                encoder.namespace(&source.namespace_snapshot)?;
            }
        }
    }
    let environment = analyzed.environment.encode(&mut encoder)?;
    let cells = Value::Vector(analyzed.cells.iter().map(|g| encoder.global(g)).collect());
    let directive = encoder.optional_form(analyzed.namespace_directive.as_ref())?;
    let header = Value::Map(BTreeMap::from([
        (header_key("version"), number(VERSION as usize)),
        (header_key("phase"), encode_phase(phase)),
        (header_key("rows"), number(encoder.rows.len())),
        (header_key("anonymous-ids"), number(usize::try_from(anonymous_ids).map_err(|e| e.to_string())?)),
    ]));
    let tail = Value::Map(BTreeMap::from([
        (header_key("environment"), environment),
        (header_key("cells"), cells),
        (header_key("namespace-directive"), directive),
    ]));
    let mut out = String::new();
    let mut print = |value: &Value| -> R<()> {
        out.push_str(&suss_reader::edn::print(value).map_err(|e| e.message)?);
        out.push('\n');
        Ok(())
    };
    print(&header)?;
    let mut positions = vec![0; TABLES.len()];
    for row in &mut encoder.rows {
        let Value::Vector(fields) = row else { continue };
        let table = TABLES[index(&fields[0])?];
        if table.counted() {
            let uses = encoder.uses[table as usize][positions[table as usize]];
            fields.insert(1, number(uses as usize));
        }
        positions[table as usize] += 1;
        print(row)?;
    }
    print(&tail)?;
    Ok(out)
}

/// Read what [`encode`] wrote for `phase`, with `base` as every origin.
pub(crate) fn decode(text: &str, phase: Phase, base: SourceOrigin) -> R<CoreAnalysis> {
    let options = suss_reader::edn::Options {
        work_allowance: 4 * text.len() as u64 + 1024,
        max_depth: 64,
        ..suss_reader::edn::Options::default()
    };
    let values = suss_reader::edn::read_all(text, &options).map_err(|e| e.message)?;
    let mut values = values.into_iter();
    let Some(Value::Map(header)) = values.next() else { return Err("Missing header".into()) };
    let field = |name: &str| header.get(&header_key(name)).ok_or_else(|| format!("Missing {name}"));
    if index(field("version")?)? != VERSION as usize || decode_phase(field("phase")?)? != phase {
        return Err("Incompatible core analysis".into());
    }
    let rows = index(field("rows")?)?;
    let anonymous_ids = index(field("anonymous-ids")?)? as u64;
    let mut decoder = Decoder::new(base);
    for _ in 0..rows {
        decoder.row(&values.next().ok_or("Missing rows")?)?;
    }
    let Some(Value::Map(tail)) = values.next() else { return Err("Missing environment".into()) };
    if values.next().is_some() {
        return Err("Trailing values".into());
    }
    let field = |name: &str| tail.get(&header_key(name)).ok_or_else(|| format!("Missing {name}"));
    Ok(CoreAnalysis {
        environment: super::resolve::Environment::decode(&decoder, field("environment")?)?,
        cells: items(field("cells")?)?.iter().map(|g| decoder.global(g)).collect::<R<_>>()?,
        namespace_directive: decoder.optional_form(field("namespace-directive")?)?,
        anonymous_ids,
    })
}
