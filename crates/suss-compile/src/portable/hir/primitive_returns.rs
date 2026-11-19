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

pub(super) fn validate(
    body: &Hir,
    hint: Nominal,
    bindings: &mut HashMap<BindingId, Primitive>,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    let actual = infer(body, bindings, meter, &mut Vec::new())?;
    let expected = if hint == Nominal::LongCast { Primitive::Long } else { Primitive::Double };
    if actual != Primitive::Boxed && actual != Primitive::Never && actual != expected
        && !(expected == Primitive::Long && actual == Primitive::Int)
        && !(expected == Primitive::Double && actual == Primitive::Float)
    {
        return Err(fail(body.span.clone(), format!(
            "Mismatched primitive return, expected: {}, had: {}", expected.name(), actual.name(),
        )));
    }
    Ok(())
}

struct LoopInputs { target: LoopId, kinds: Vec<Primitive> }

fn infer(
    hir: &Hir,
    bindings: &mut HashMap<BindingId, Primitive>,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
    loops: &mut Vec<LoopInputs>,
) -> Result<Primitive, Diagnostic> {
    meter(1)?;
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        use Primitive::*;
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
                    let kind = infer(&arguments[0], bindings, meter, loops)?;
                    if kind.floating() { Double } else if kind.integer() { Long } else { Boxed }
                }
                ("unchecked-add" | "unchecked-subtract" | "unchecked-multiply", 2) => {
                    let a = infer(&arguments[0], bindings, meter, loops)?;
                    let b = infer(&arguments[1], bindings, meter, loops)?;
                    numeric("+", &[a, b])
                }
                ("nil?" | "zero?" | "pos?" | "neg?", 1)
                | ("identical?" | "=" | "==" | "<" | ">" | "<=" | ">=", 2) => Boolean,
                ("+" | "*" | "/", 1) | ("-" | "/", 0) => Boxed,
                ("+" | "-" | "*" | "/" | "quot" | "rem", _) => {
                    let mut kinds = Vec::with_capacity(arguments.len());
                    for argument in arguments.iter() { kinds.push(infer(argument, bindings, meter, loops)?); }
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
            Expression::Arithmetic { operator, arguments, coercing } => {
                if *coercing { Boxed } else {
                    let mut kinds = Vec::with_capacity(arguments.len());
                    for argument in arguments { kinds.push(infer(argument, bindings, meter, loops)?); }
                    numeric(match operator {
                        Arithmetic::Add => "+", Arithmetic::Subtract | Arithmetic::Negate => "-",
                        Arithmetic::Multiply => "*", Arithmetic::Divide => "/",
                    }, &kinds)
                }
            }
            Expression::Local(id) => bindings.get(id).copied().unwrap_or(Boxed),
            Expression::Do(items) => match items.last() {
                Some(last) => infer(last, bindings, meter, loops)?, None => Boxed,
            },
            Expression::Let { bindings: locals, body } => {
                let mut previous = Vec::with_capacity(locals.len());
                for binding in locals {
                    let kind = infer(&binding.value, bindings, meter, loops)?;
                    previous.push((binding.id, bindings.insert(binding.id, kind)));
                }
                let result = infer(body, bindings, meter, loops);
                for (id, old) in previous.into_iter().rev() {
                    match old { Some(kind) => { bindings.insert(id, kind); }, None => { bindings.remove(&id); } }
                }
                result?
            }
            Expression::Loop { target, bindings: locals, body } => {
                let mut previous = Vec::with_capacity(locals.len());
                let mut kinds = Vec::with_capacity(locals.len());
                for binding in locals {
                    // Clojure widens int/float loop initializers, while byte
                    // and short retain their distinct primitive source kind.
                    let kind = match infer(&binding.value, bindings, meter, loops)? {
                        Int => Long, Float => Double, kind => kind,
                    };
                    kinds.push(kind);
                    previous.push((binding.id, bindings.insert(binding.id, kind)));
                }
                let result = loop {
                    for (binding, kind) in locals.iter().zip(&kinds) { bindings.insert(binding.id, *kind); }
                    loops.push(LoopInputs { target: *target, kinds: kinds.clone() });
                    let result = infer(body, bindings, meter, loops);
                    let next = loops.pop().expect("source loop inference").kinds;
                    if result.is_err() || next == kinds { break result; }
                    // Each binding can only lose its primitive kind once.
                    // Revisit uses after a mixed recur makes a local boxed.
                    kinds = next;
                };
                for (id, old) in previous.into_iter().rev() {
                    match old { Some(kind) => { bindings.insert(id, kind); }, None => { bindings.remove(&id); } }
                }
                result?
            }
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
            Expression::Bitwise { operation, .. } => match operation {
                Bitwise::Int => Int,
                Bitwise::And | Bitwise::Or | Bitwise::Xor | Bitwise::AndNot | Bitwise::Not
                | Bitwise::Left | Bitwise::Right | Bitwise::Unsigned => Long,
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
            Expression::Let { body, .. } => array_kind_cached(body, meter, memo)?,
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
    let all_numeric = kinds.iter().all(|kind| kind.integer() || kind.floating());
    if matches!(name, "quot" | "rem") && !all_numeric { return Boxed; }
    if kinds.iter().any(|kind| kind.floating()) { return Double; }
    if all_numeric && name != "/" { Long } else { Boxed }
}
