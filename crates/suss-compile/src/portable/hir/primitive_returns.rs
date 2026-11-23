//! Clojure source primitive types at the fn return boundary. These are not
//! optimizer representations: WIT specialization and Suss-only fast paths
//! must not turn a source boxed expression into a primitive-return error.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Primitive { Boxed, Byte, Short, Int, Long, Float, Double, Boolean, Never }
impl Primitive {
    fn name(self) -> &'static str {
        match self {
            Self::Byte => "byte", Self::Short => "short", Self::Int => "int",
            Self::Long => "long", Self::Float => "float", Self::Double => "double",
            Self::Boolean => "boolean", Self::Boxed => "Object", Self::Never => "never",
        }
    }
    fn integer(self) -> bool { matches!(self, Self::Byte | Self::Short | Self::Int | Self::Long) }
    fn floating(self) -> bool { matches!(self, Self::Float | Self::Double) }
}

/// Clojure keeps an Object local's value even when its Java class is a
/// primitive hint. Only an inline primitive use consumes that class.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct SourceType { pub primitive: Primitive, pub class: Primitive }
impl SourceType {
    fn primitive(kind: Primitive) -> Self { Self { primitive: kind, class: kind } }
}
#[derive(Default)]
pub(super) struct SourceFacts(HashMap<BindingId, SourceType>);
impl SourceFacts {
    pub fn insert(&mut self, id: BindingId, kind: Primitive) -> Option<SourceType> {
        self.insert_type(id, SourceType::primitive(kind))
    }
    pub fn insert_type(&mut self, id: BindingId, ty: SourceType) -> Option<SourceType> { self.0.insert(id, ty) }
    pub fn remove(&mut self, id: &BindingId) { self.0.remove(id); }
    fn get(&self, id: &BindingId) -> SourceType {
        self.0.get(id).copied().unwrap_or(SourceType::primitive(Primitive::Boxed))
    }
}

pub(super) fn numeric_hint(
    metadata: &[Form], span: &Range<usize>,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<Option<Primitive>, Diagnostic> {
    if metadata.is_empty() { return Ok(None); }
    let form = Form { kind: Kind::Nil, span: span.clone(), metadata: metadata.to_vec() };
    for pair in reader_metadata_pairs_metered(&form, meter)?.chunks_exact(2) {
        if matches!(&pair[0].kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == "tag") {
            let name = match &pair[1].kind {
                Kind::Symbol(name) if name.namespace.is_none() => name.name.clone(),
                Kind::String(units) => String::from_utf16_lossy(units),
                _ => return Ok(None),
            };
            return Ok(match name.as_str() { "long" => Some(Primitive::Long), "double" => Some(Primitive::Double), _ => None });
        }
    }
    Ok(None)
}

pub(super) fn binding_type(
    binding: &Binding, facts: &mut SourceFacts,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<SourceType, Diagnostic> {
    binding_type_in(binding, facts, meter, &mut Vec::new())
}
fn binding_type_in(
    binding: &Binding, facts: &mut SourceFacts,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>, loops: &mut Vec<LoopInputs>,
) -> Result<SourceType, Diagnostic> {
    let primitive = infer(&binding.value, facts, meter, loops)?;
    let hinted = numeric_hint(&binding.metadata, &binding.span, meter)?;
    if hinted.is_some() && !matches!(primitive, Primitive::Boxed | Primitive::Never) {
        return Err(fail(binding.span.clone(), "Can't type hint a local with a primitive initializer"));
    }
    let class = match hinted { Some(class) => class, None => class_in(&binding.value, facts, meter, loops)? };
    Ok(SourceType { primitive, class })
}

/// A source numeric Java class, independently of canEmitPrimitive/storage.
pub(super) fn class(
    hir: &Hir, facts: &mut SourceFacts,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<Primitive, Diagnostic> { class_in(hir, facts, meter, &mut Vec::new()) }
fn class_in(
    hir: &Hir, facts: &mut SourceFacts,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>, loops: &mut Vec<LoopInputs>,
) -> Result<Primitive, Diagnostic> {
    meter(1)?;
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        use Primitive::*;
        if let Expression::Local(id) = &*hir.kind {
            let ty = facts.get(id);
            if let Some(hint) = numeric_hint(&hir.metadata, &hir.span, meter)? {
                if !matches!(ty.primitive, Boxed | Never) && ty.primitive != hint {
                    return Err(fail(hir.span.clone(), "Can't type hint a primitive local with a different type"));
                }
                return Ok(hint);
            }
            return Ok(ty.class);
        }
        match &*hir.kind {
            Expression::Do(items) => items.last().map_or(Ok(Boxed), |last| class_in(last, facts, meter, loops)),
            Expression::If { consequent, alternative, .. } => {
                let a = class_in(consequent, facts, meter, loops)?;
                let b = class_in(alternative, facts, meter, loops)?;
                Ok(if a == Never { b } else if b == Never || a == b { a } else { Boxed })
            }
            Expression::LetFn { bindings, body } => {
                let previous = bindings.iter().map(|binding|
                    (binding.id, facts.insert(binding.id, Primitive::Boxed))).collect();
                let result = class_in(body, facts, meter, loops);
                restore(facts, previous);
                result
            }
            Expression::Let { bindings, body } => {
                let mut previous = Vec::new();
                for binding in bindings {
                    let ty = binding_type_in(binding, facts, meter, loops)?;
                    previous.push((binding.id, facts.insert_type(binding.id, ty)));
                }
                let result = class_in(body, facts, meter, loops);
                restore(facts, previous);
                result
            }
            _ => infer(hir, facts, meter, loops),
        }
    })
}
fn restore(facts: &mut SourceFacts, previous: Vec<(BindingId, Option<SourceType>)>) {
    for (id, old) in previous.into_iter().rev() {
        match old { Some(ty) => { facts.insert_type(id, ty); }, None => facts.remove(&id) }
    }
}

/// Analyzer facts retained before the physical parameter/return wrappers.
/// WIT-only casts have Object source type and do not impose recur diagnostics.
pub(super) struct Method {
    pub body: Hir,
    pub return_hint: Option<Nominal>,
    pub casts: Vec<(BindingId, Primitive)>,
    pub parameters: Vec<(String, Option<Primitive>)>,
}
impl Method {
    pub fn needs_validation(&self) -> bool {
        self.return_hint.is_some() || self.parameters.iter().any(|(_, kind)| kind.is_some())
    }
    pub fn validate_return(
        &self, facts: &mut SourceFacts,
        meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
    ) -> Result<(), Diagnostic> {
        let Some(hint) = self.return_hint else { return Ok(()) };
        meter(self.casts.len() as u64)?;
        let previous = self.casts.iter().map(|(id, kind)| (*id, facts.insert(*id, *kind))).collect();
        let result = validate(&self.body, hint, facts, meter);
        restore(facts, previous);
        result
    }
    pub fn validate_recur(
        &self, arguments: &[Hir], facts: &mut SourceFacts,
        meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
    ) -> Result<(), Diagnostic> {
        for (argument, (name, expected)) in arguments.iter().zip(&self.parameters) {
            meter(1)?;
            let Some(expected) = expected else { continue };
            let actual = kind(argument, facts, meter)?;
            if !compatible(*expected, actual) {
                // A hinted Object still cannot emit a primitive recur operand,
                // even though the diagnostic reports its hinted Java class.
                let class = class(argument, facts, meter)?;
                return Err(fail(argument.span.clone(), format!(
                    "recur arg for primitive local: {name} is not matching primitive, had: {}, needed: {}",
                    class.name(), expected.name(),
                )));
            }
        }
        Ok(())
    }
}
fn compatible(expected: Primitive, actual: Primitive) -> bool {
    expected == actual || (expected == Primitive::Long && actual == Primitive::Int)
        || (expected == Primitive::Double && actual == Primitive::Float)
}

pub(super) fn validate(
    body: &Hir,
    hint: Nominal,
    bindings: &mut SourceFacts,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    let actual = infer(body, bindings, meter, &mut Vec::new())?;
    let expected = if hint == Nominal::LongCast { Primitive::Long } else { Primitive::Double };
    if actual != Primitive::Boxed && actual != Primitive::Never && !compatible(expected, actual)
    {
        return Err(fail(body.span.clone(), format!(
            "Mismatched primitive return, expected: {}, had: {}", expected.name(), actual.name(),
        )));
    }
    Ok(())
}

struct LoopInputs { target: LoopId, kinds: Vec<Primitive> }

/// A genuine Clojure source kind; physical boxing must not select overloads.
pub(super) fn kind(
    hir: &Hir,
    bindings: &mut SourceFacts,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<Primitive, Diagnostic> {
    infer(hir, bindings, meter, &mut Vec::new())
}

fn infer(
    hir: &Hir,
    bindings: &mut SourceFacts,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
    loops: &mut Vec<LoopInputs>,
) -> Result<Primitive, Diagnostic> {
    meter(1)?;
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        use Primitive::*;
        // A frozen constant has its initializer's source primitive class even
        // while the runtime retains its boxed value in a private binding cell.
        // The source record holds the revision preceding the top-level form.
        if let Expression::Global(global) = &*hir.kind
            && global.namespace() == "suss.internal.constants"
            && let Some(source) = &hir.source
            && let Some(SourceBinding::Global { declaration: Some(info), .. }) = &source.resolved
            && info.constant.is_some()
            && let Some(initializer) = info.initializer.as_deref()
        {
            return infer(initializer, bindings, meter, loops);
        }
        // Inspect genuine source calls before compiler-generated lowering.
        // For example, Suss makes hash raw, but Clojure's hash call is boxed.
        if let Some(source) = &hir.source
            && let Some(node) = &source.node
            && let SourceNode::Invoke { arguments, .. } = node.as_ref()
        {
            let Some(SourceBinding::Global { global, declaration }) = &source.resolved else { return Ok(Boxed) };
            if global.namespace() != "suss.core" {
                return match declaration {
                    Some(info) => declared_return(&info.declaration, arguments.len(), meter),
                    None => Ok(Boxed),
                };
            }
            let name = global.name();
            return Ok(match (name, arguments.len()) {
                ("long" | "unchecked-long", 1) => Long,
                ("double" | "unchecked-double", 1) => Double,
                ("int" | "unchecked-int" | "count", 1) => Int,
                ("byte" | "unchecked-byte", 1) => Byte,
                ("short" | "unchecked-short", 1) => Short,
                ("float" | "unchecked-float", 1) => Float,
                ("alength", 1) => if array_kind(&arguments[0], meter)?.is_some() { Int } else { Boxed },
                ("aget", 2) | ("aset", 3) => array_kind(&arguments[0], meter)?.map(packed).unwrap_or(Boxed),
                ("inc" | "dec" | "abs" | "unchecked-inc" | "unchecked-dec" | "unchecked-negate", 1) => {
                    let kind = class_in(&arguments[0], bindings, meter, loops)?;
                    if kind.floating() { Double } else if kind.integer() { Long } else { Boxed }
                }
                ("unchecked-add" | "unchecked-subtract" | "unchecked-multiply", 2) => {
                    let a = class_in(&arguments[0], bindings, meter, loops)?;
                    let b = class_in(&arguments[1], bindings, meter, loops)?;
                    numeric("+", &[a, b])
                }
                ("nil?" | "zero?" | "pos?" | "neg?", 1)
                | ("identical?" | "=" | "==" | "<" | ">" | "<=" | ">=", 2) => Boolean,
                ("bit-and" | "bit-or" | "bit-xor" | "bit-and-not" | "bit-not"
                    | "bit-shift-left" | "bit-shift-right" | "unsigned-bit-shift-right", _) =>
                    bitwise_result(arguments, bindings, meter, loops)?,
                ("+" | "*" | "/", 1) | ("-" | "/", 0) => Boxed,
                ("max" | "min", count) if count > 1 && extrema::is_call(hir) => {
                    let mut kind = class_in(&arguments[0], bindings, meter, loops)?;
                    for argument in &arguments[1..] {
                        kind = extrema::result(kind, class_in(argument, bindings, meter, loops)?);
                    }
                    kind
                }
                ("+" | "-" | "*" | "/" | "quot" | "rem", _) => {
                    let mut kinds = Vec::with_capacity(arguments.len());
                    for argument in arguments.iter() { kinds.push(class_in(argument, bindings, meter, loops)?); }
                    numeric(name, &kinds)
                }
                _ => Boxed,
            });
        }
        Ok(match &*hir.kind {
            Expression::Literal(Literal::Long(_)) => Long,
            Expression::Literal(Literal::Number(_)) => Double,
            // Bootstrap inc/dec expand to arithmetic without an Invoke node.
            // Infer from source operands, never the backend's numeric facts.
            Expression::Arithmetic { operator, arguments, coercing, .. } => {
                if *coercing { Boxed } else {
                    let mut kinds = Vec::with_capacity(arguments.len());
                    for argument in arguments { kinds.push(class_in(argument, bindings, meter, loops)?); }
                    numeric(match operator {
                        Arithmetic::Add => "+", Arithmetic::Subtract | Arithmetic::Negate => "-",
                        Arithmetic::Multiply => "*", Arithmetic::Divide => "/",
                    }, &kinds)
                }
            }
            Expression::Local(id) => {
                class_in(hir, bindings, meter, loops)?; // validate a use-site tag even in Object context
                bindings.get(id).primitive
            },
            Expression::Do(items) => match items.last() {
                Some(last) => infer(last, bindings, meter, loops)?, None => Boxed,
            },
            Expression::LetFn { bindings: locals, body } => {
                let previous = locals.iter().map(|binding|
                    (binding.id, bindings.insert(binding.id, Primitive::Boxed))).collect();
                let result = infer(body, bindings, meter, loops);
                restore(bindings, previous);
                result?
            }
            Expression::Let { bindings: locals, body } => {
                let mut previous = Vec::with_capacity(locals.len());
                for binding in locals {
                    let ty = binding_type_in(binding, bindings, meter, loops)?;
                    previous.push((binding.id, bindings.insert_type(binding.id, ty)));
                }
                let result = infer(body, bindings, meter, loops);
                for (id, old) in previous.into_iter().rev() {
                    match old { Some(ty) => { bindings.insert_type(id, ty); }, None => { bindings.remove(&id); } }
                }
                result?
            }
            Expression::Loop { target, bindings: locals, body } => {
                let result = loop_analysis(*target, locals, body, bindings, meter, loops)?.0;
                // Clojure wraps a loop in expression context in an ordinary
                // zero-argument fn call. A return-context loop stays primitive.
                if hir.source.as_ref().is_some_and(|source| source.context == super::super::AnalysisContext::Expression) {
                    Boxed
                } else { result }
            },
            Expression::If { consequent, alternative, .. } => {
                let a = infer(consequent, bindings, meter, loops)?;
                let b = infer(alternative, bindings, meter, loops)?;
                if a == Never { b } else if b == Never || a == b { a } else { Boxed }
            }
            // Throw has Object type in Clojure's if joins; recur has no
            // value and does not change the other branch's primitive type.
            Expression::Throw(_) => Boxed,
            Expression::Recur { target, arguments } => {
                let mut incoming = Vec::with_capacity(arguments.len());
                for argument in arguments { incoming.push(infer(argument, bindings, meter, loops)?); }
                if let Some(scope) = loops.iter_mut().rev().find(|scope| scope.target == *target) {
                    for (old, new) in scope.kinds.iter_mut().zip(incoming) {
                        if *old != new && !(*old == Long && new == Int)
                            && !(*old == Double && new == Float) { *old = Boxed; }
                    }
                }
                Never
            }
            // Clojure's try expressions are boxed even when all arms have
            // the same primitive kind; finally does not change their type.
            Expression::Try { .. } | Expression::AsyncTry { .. } => Boxed,
            Expression::Nominal { operation, .. } => match operation {
                Nominal::LongCast | Nominal::UncheckedLongCast => Long,
                Nominal::DoubleCast => Double,
                Nominal::PackedGet(kind) | Nominal::PackedSet(kind) => packed(*kind),
                _ => Boxed,
            },
            // Byte/short/float stores use the ordinary array operation. Its
            // source element kind is still given by the actual array hint.
            Expression::Array { operation, arguments } if
                (*operation == ArrayOperation::Get && arguments.len() == 2)
                    || (*operation == ArrayOperation::Set && arguments.len() == 3) =>
                array_kind(&arguments[0], meter)?.map(packed).unwrap_or(Boxed),
            Expression::Array { operation: ArrayOperation::Length, arguments } if arguments.len() == 1 =>
                if array_kind(&arguments[0], meter)?.is_some() { Int } else { Boxed },
            Expression::NilTest(_) | Expression::Comparison { .. } => Boolean,
            Expression::Bitwise { operation, arguments } => match operation {
                Bitwise::PromoteSubtractPrimitive => {
                    let a = infer(&arguments[0], bindings, meter, loops)?;
                    let b = infer(&arguments[1], bindings, meter, loops)?;
                    if a.floating() || b.floating() { Double } else { Boxed }
                }
                Bitwise::LongMax | Bitwise::LongMin => Long,
                Bitwise::DoubleMax | Bitwise::DoubleMin => Double,
                Bitwise::Int | Bitwise::IntPrimitive | Bitwise::IntFloat => Int,
                Bitwise::And | Bitwise::Or | Bitwise::Xor | Bitwise::AndNot | Bitwise::Not
                | Bitwise::Left | Bitwise::Right | Bitwise::Unsigned =>
                    bitwise_result(arguments, bindings, meter, loops)?,
                // Public bit-test/clear/flip/set are boxed calls in Clojure. Private numeric
                // intrinsics are implementation facts, not source types.
                _ => Boxed,
            },
            _ => Boxed,
        })
    })
}

fn packed(kind: usize) -> Primitive {
    use Primitive::*;
    match kind { 0 => Byte, 1 => Short, 2 => Int, 3 => Float, 4 => Long, 5 => Double, _ => Boxed }
}

/// A source Java-array class, not a backend storage fact. In particular an
/// unhinted argument or an arbitrary call stays unknown, even if its runtime
/// value happens to be an array. Clojure then uses a boxed reflective alength.
fn array_kind(hir: &Hir, meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>) -> Result<Option<usize>, Diagnostic> {
    array_kind_cached(hir, meter, &mut HashMap::new())
}
fn array_kind_cached(
    hir: &Hir, meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
    memo: &mut HashMap<usize, Option<usize>>,
) -> Result<Option<usize>, Diagnostic> {
    meter(1)?;
    let key = hir as *const Hir as usize;
    if let Some(kind) = memo.get(&key) { return Ok(*kind); }
    let result = stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        if let Some(kind) = arrays::packed_kind(hir) { return Ok(Some(kind)); }
        if let Some(source) = &hir.source {
            if let Some(SourceBinding::Global { global, .. }) = &source.resolved
                && global.namespace() == "suss.core"
                && let Kind::List(items) = &source.form.kind
            {
                let arity = items.len().saturating_sub(1);
                let kind = match (global.name(), arity) {
                    ("byte-array", 1 | 2) => Some(0), ("short-array", 1 | 2) => Some(1),
                    ("int-array", 1 | 2) => Some(2), ("float-array", 1 | 2) => Some(3),
                    ("long-array", 1 | 2) => Some(4), ("double-array", 1 | 2) => Some(5),
                    ("object-array", 1) => Some(6), _ => None,
                };
                if kind.is_some() { return Ok(kind); }
            }
            if let Some(SourceBinding::Local(binding)) = &source.resolved
                && matches!(binding.kind, LocalKind::Let | LocalKind::Loop)
                && let Some(initializer) = &binding.initializer
            {
                // Object loop locals retain their initializer's source class,
                // including when recur later supplies a different value.
                return array_kind_cached(initializer, meter, memo);
            }
            // Do not look through a Suss-only fast path for an ordinary
            // source call, for example identity returning an array at runtime.
            if source.node.as_ref().is_some_and(|node| matches!(node.as_ref(), SourceNode::Invoke { .. })) {
                return Ok(None);
            }
        }
        Ok(match &*hir.kind {
            Expression::Do(items) => match items.last() { Some(last) => array_kind_cached(last, meter, memo)?, None => None },
            Expression::Let { body, .. } | Expression::LetFn { body, .. } => array_kind_cached(body, meter, memo)?,
            Expression::If { consequent, alternative, .. } => {
                let a = array_kind_cached(consequent, meter, memo)?;
                let b = array_kind_cached(alternative, meter, memo)?;
                if a == b { a }
                else if matches!(&*consequent.kind, Expression::Literal(Literal::Nil)) { b }
                else if matches!(&*alternative.kind, Expression::Literal(Literal::Nil)) { a }
                else { None }
            }
            _ => None,
        })
    });
    if let Ok(kind) = &result {
        meter(1)?;
        memo.insert(key, *kind);
    }
    result
}

/// Only declarations with arglists describe primitive global invocation.
/// A tag on a plain def, or hints on a local fn's body, do not do that.
fn declared_return(
    declaration: &Form, arity: usize,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<Primitive, Diagnostic> {
    let pairs = reader_metadata_pairs_metered(declaration, meter)?;
    let tag = |pairs: &[Form]| pairs.chunks_exact(2).find_map(|pair| {
        if matches!(&pair[0].kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == "tag") {
            match &pair[1].kind {
                Kind::Symbol(symbol) if symbol.namespace.is_none() => match symbol.name.as_str() {
                    "long" => Some(Primitive::Long), "double" => Some(Primitive::Double), _ => None,
                },
                _ => None,
            }
        } else { None }
    });
    let Some(quoted) = pairs.chunks_exact(2).find_map(|pair| {
        matches!(&pair[0].kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == "arglists")
            .then_some(&pair[1])
    }) else { return Ok(Primitive::Boxed) };
    let Kind::List(quote) = &quoted.kind else { return Ok(Primitive::Boxed) };
    if quote.len() != 2 || !matches!(&quote[0].kind, Kind::Symbol(symbol) if symbol.name == "quote") {
        return Ok(Primitive::Boxed);
    }
    let (Kind::List(arglists) | Kind::Vector(arglists)) = &quote[1].kind else { return Ok(Primitive::Boxed) };
    for arglist in arglists {
        meter(1)?;
        let Kind::Vector(parameters) = &arglist.kind else { continue };
        if parameters.len() == arity && !parameters.iter().any(|parameter|
            matches!(&parameter.kind, Kind::Symbol(symbol) if symbol.name == "&" && symbol.namespace.is_none()))
        {
            // The name's :rettag converts the body, but only an arglist's
            // own result tag declares a primitive var invocation to Clojure.
            return Ok(tag(&reader_metadata_pairs_metered(arglist, meter)?).unwrap_or(Primitive::Boxed));
        }
    }
    Ok(Primitive::Boxed)
}

fn numeric(name: &str, kinds: &[Primitive]) -> Primitive {
    use Primitive::*;
    if kinds.is_empty() { return if matches!(name, "+" | "*") { Long } else { Boxed }; }
    if matches!(name, "quot" | "rem") && kinds.len() != 2 { return Boxed; }
    if kinds.iter().any(|kind| !matches!(kind, Boxed) && !kind.integer() && !kind.floating()) { return Boxed; }
    let all_numeric = kinds.iter().all(|kind| kind.integer() || kind.floating());
    if matches!(name, "quot" | "rem") && !all_numeric { return Boxed; }
    if kinds.iter().any(|kind| kind.floating()) { return Double; }
    if all_numeric && name != "/" { Long } else { Boxed }
}

/// A selected bitwise method returns primitive long even for Object inputs.
/// With a floating/boolean primitive class no static method matches, so the
/// reflective call has Object source type, independently of its runtime value.
fn bitwise_result(
    arguments: &[Hir], bindings: &mut SourceFacts,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>, loops: &mut Vec<LoopInputs>,
) -> Result<Primitive, Diagnostic> {
    use Primitive::*;
    let Some(first) = arguments.first() else { return Ok(Boxed); };
    let matches_long = |kind: Primitive| kind == Boxed || kind.integer();
    let mut left = class_in(first, bindings, meter, loops)?;
    if arguments.len() == 1 { return Ok(if matches_long(left) { Long } else { Boxed }); }
    for right in &arguments[1..] {
        let right = class_in(right, bindings, meter, loops)?;
        left = if matches_long(left) && matches_long(right) { Long } else { Boxed };
    }
    Ok(left)
}

/// Loop source types settle before any use, including later initializers.
/// Clojure reanalyzes a loop when a mismatched recur boxes an earlier local.
/// Each local can lose its primitive source type once, so this terminates.
pub(super) fn loop_types(
    target: LoopId, locals: &[Binding], body: &Hir,
    bindings: &mut SourceFacts,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<Vec<SourceType>, Diagnostic> {
    Ok(loop_analysis(target, locals, body, bindings, meter, &mut Vec::new())?.1)
}

fn loop_analysis(
    target: LoopId, locals: &[Binding], body: &Hir,
    bindings: &mut SourceFacts,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
    loops: &mut Vec<LoopInputs>,
) -> Result<(Primitive, Vec<SourceType>), Diagnostic> {
    use Primitive::*;
    let widen = |kind| match kind { Int => Long, Float => Double, kind => kind };
    let mut previous = Vec::with_capacity(locals.len());
    let mut kinds = Vec::with_capacity(locals.len());
    for binding in locals {
        let mut ty = binding_type_in(binding, bindings, meter, loops)?;
        ty.primitive = widen(ty.primitive);
        if ty.primitive != Boxed { ty.class = ty.primitive; }
        kinds.push(ty.primitive);
        previous.push((binding.id, bindings.insert_type(binding.id, ty)));
    }
    let result = loop {
        let before = kinds.clone();
        // An earlier local's lost primitive type also affects the source
        // type of any later initializer which reads it.
        for (binding, kind) in locals.iter().zip(&mut kinds) {
            let mut ty = binding_type_in(binding, bindings, meter, loops)?;
            let initializer = widen(ty.primitive);
            if *kind != initializer { *kind = Boxed; ty.class = Boxed; }
            ty.primitive = *kind;
            if *kind != Boxed { ty.class = *kind; }
            bindings.insert_type(binding.id, ty);
        }
        loops.push(LoopInputs { target, kinds: kinds.clone() });
        let result = infer(body, bindings, meter, loops);
        kinds = loops.pop().expect("source loop inference").kinds;
        if result.is_err() || kinds == before { break result; }
    };
    let types = locals.iter().map(|binding| bindings.get(&binding.id)).collect();
    restore(bindings, previous);
    Ok((result?, types))
}
