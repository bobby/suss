//! Clojure source primitive types at the fn return boundary. These are not
//! optimizer representations: WIT specialization and Suss-only fast paths
//! must not turn a source boxed expression into a primitive-return error.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Primitive { Boxed, Long, Double, Int, Boolean, Never }
impl Primitive {
    fn name(self) -> &'static str {
        match self {
            Self::Long => "long", Self::Double => "double", Self::Int => "int",
            Self::Boolean => "boolean", Self::Boxed => "Object", Self::Never => "never",
        }
    }
    fn integer(self) -> bool { matches!(self, Self::Long | Self::Int) }
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
                    // Clojure stores an int loop initializer as a long.
                    let kind = match infer(&binding.value, bindings, meter, loops)? {
                        Int => Long, kind => kind,
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
                        if *old != new && !(*old == Long && new == Int) { *old = Boxed; }
                    }
                }
                Never
            }
            // Clojure's try expressions are boxed even when all arms have
            // the same primitive kind; finally does not change their type.
            Expression::Try { .. } | Expression::AsyncTry { .. } => Boxed,
            Expression::Nominal { operation, .. } => match operation {
                Nominal::LongCast | Nominal::UncheckedLongCast | Nominal::PackedGet(4) | Nominal::PackedSet(4) => Long,
                Nominal::DoubleCast | Nominal::PackedGet(5) | Nominal::PackedSet(5) => Double,
                _ => Boxed,
            },
            Expression::NilTest(_) | Expression::Comparison { .. } => Boolean,
            Expression::Bitwise { operation, .. } => if *operation == Bitwise::Int { Int } else { Long },
            _ => Boxed,
        })
    })
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
    let all_numeric = kinds.iter().all(|kind| kind.integer() || *kind == Double);
    if matches!(name, "quot" | "rem") && !all_numeric { return Boxed; }
    if kinds.contains(&Double) { return Double; }
    if all_numeric && name != "/" { Long } else { Boxed }
}
