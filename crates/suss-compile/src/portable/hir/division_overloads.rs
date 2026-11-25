//! Select numeric overloads after lexical and loop source kinds settle.
//! Physical raw/boxed storage inference is a later, independent operation.
use super::*;
use primitive_returns::{Primitive, SourceFacts, SourceType};
use std::sync::Arc;

pub(super) fn annotate(
    hir: &mut Hir,
    methods: &HashMap<LoopId, primitive_returns::Method>,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    // Keep original expression addresses alive while the memoized rewrite
    // makes shallow copies. Analyzer BindingIds are unique within this HIR,
    // so every shared expression has one settled lexical source context.
    let original = hir.clone();
    let mut validate_methods = false;
    let mut source_casts = HashMap::new();
    for method in methods.values() {
        meter(1 + method.parameters.len() as u64 + method.casts.len() as u64)?;
        validate_methods |= method.needs_validation();
        source_casts.extend(method.casts.iter().copied());
    }
    let mut pass = Pass { hints: false, validate_methods, methods, source_casts: &source_casts,
        validate_only: false, needed: HashMap::new(), rewritten: HashMap::new(), meter };
    let hints = pass.features(hir)?.hints;
    pass.hints = hints;
    if hints || validate_methods {
        // Clojure diagnoses hints while first parsing the loop, before a
        // mismatched recur triggers boxing/reanalysis. Include captured fn
        // bodies in that initial validation context.
        Pass { hints, validate_methods, methods, source_casts: &source_casts, validate_only: true,
            needed: HashMap::new(), rewritten: HashMap::new(), meter: &mut *pass.meter }
            .walk(&mut hir.clone(), &mut SourceFacts::default())?;
    }
    let result = pass.walk(hir, &mut SourceFacts::default());
    drop(original);
    result
}

#[derive(Clone, Copy, Default)]
struct Features { overloads: bool, hints: bool, extrema: bool, numeric_core: bool, objects: bool }
impl Features {
    fn include(&mut self, other: Self) { self.overloads |= other.overloads; self.hints |= other.hints; self.extrema |= other.extrema; self.numeric_core |= other.numeric_core; self.objects |= other.objects; }
}

struct Pass<'a> {
    hints: bool,
    validate_methods: bool,
    methods: &'a HashMap<LoopId, primitive_returns::Method>,
    source_casts: &'a HashMap<BindingId, Primitive>,
    validate_only: bool,
    needed: HashMap<usize, Features>,
    rewritten: HashMap<usize, (Arc<Expression>, Type)>,
    meter: &'a mut dyn FnMut(u64) -> Result<(), Diagnostic>,
}

impl Pass<'_> {
    fn any<'a>(&mut self, children: impl IntoIterator<Item = &'a Hir>) -> Result<Features, Diagnostic> {
        let mut features = Features::default();
        for child in children { features.include(self.features(child)?); }
        Ok(features)
    }

    fn features(&mut self, hir: &Hir) -> Result<Features, Diagnostic> {
        (self.meter)(1)?;
        let key = Arc::as_ptr(&hir.kind) as usize;
        if let Some(needed) = self.needed.get(&key) { return Ok(*needed); }
        let mut needed = stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
            use Expression::*;
            match &*hir.kind {
                Arithmetic { operator, coercing, arguments, .. } => {
                    let mut features = self.any(arguments)?;
                    features.overloads |= matches!(operator, super::Arithmetic::Divide | super::Arithmetic::Subtract) && !*coercing;
                    Ok(features)
                }
                Bitwise { operation, arguments } => {
                    let mut features = self.any(arguments)?;
                    features.numeric_core |= matches!(operation, super::Bitwise::Int | super::Bitwise::Cast
                        | super::Bitwise::UncheckedSubtract | super::Bitwise::PromoteSubtract);
                    Ok(features)
                }
                Comparison { arguments, .. } | Array { arguments, .. }
                | Nominal { arguments, .. } | Recur { arguments, .. } | Do(arguments) => self.any(arguments),
                Let { bindings, body } | LetFn { bindings, body } | Loop { bindings, body, .. } => {
                    let mut features = self.any(bindings.iter().map(|binding| &binding.value).chain(std::iter::once(body.as_ref())))?;
                    for binding in bindings {
                        features.hints |= primitive_returns::numeric_hint(&binding.metadata, &binding.span, self.meter)?.is_some();
                    }
                    Ok(features)
                }
                Function { body, .. } | Future { body, .. } => self.features(body),
                GeneralFunction { methods, .. } => self.any(methods.iter().map(|method| method.body.as_ref())),
                DynamicScope { bindings, body, .. } | AsyncDynamicScope { bindings, body, .. } =>
                    self.any(bindings.iter().map(|(_, value)| value).chain(std::iter::once(body.as_ref()))),
                AsyncTry { body, handler, cleanup, .. } => self.any(std::iter::once(body.as_ref())
                    .chain(handler.iter().map(Box::as_ref)).chain(cleanup.iter().map(Box::as_ref))),
                Try { regions } => self.any(regions.iter().map(Box::as_ref)),
                If { condition, consequent, alternative } => self.any([condition.as_ref(), consequent, alternative]),
                Call { callee, arguments } => self.any(std::iter::once(callee.as_ref()).chain(arguments)),
                Definition { initializer, .. } => self.any(initializer.iter().map(Box::as_ref)),
                Assign { value, .. } | Throw(value) | NilTest(value) | Await { value } => self.features(value),
                Literal(_) | Local(_) | Global(_) | GlobalOrFallback { .. } | GlobalCell(_) => Ok(Features::default()),
            }
        })?;
        needed.hints |= primitive_returns::numeric_hint(&hir.metadata, &hir.span, self.meter)?.is_some();
        needed.extrema |= extrema::is_call(hir);
        needed.numeric_core |= local_hints::numeric_call(hir);
        needed.objects |= matches!(&*hir.kind, Expression::Literal(Literal::Number(_))
            | Expression::Nominal { operation: super::Nominal::Identical | super::Nominal::DoubleCast, .. }
            // A local alone has no context-free feature, but its settled source
            // type still selects fresh Object boxing when stored in an array.
            | Expression::Array { operation: ArrayOperation::Objects | ArrayOperation::Literal, .. });
        needed.objects |= matches!(&*hir.kind, Expression::Call { .. })
            && hir.source.as_ref().is_some_and(|source|
                !source.origin.as_ref().is_some_and(|origin| origin.is_privileged()));
        self.needed.insert(key, needed);
        Ok(needed)
    }

    fn walk(&mut self, hir: &mut Hir, facts: &mut SourceFacts) -> Result<(), Diagnostic> {
        (self.meter)(1)?;
        if !self.hints && !self.validate_methods {
            let needed = self.features(hir)?;
            if !needed.overloads && !needed.extrema && !needed.numeric_core && !needed.objects { return Ok(()); }
        }
        let key = Arc::as_ptr(&hir.kind) as usize;
        if let Some((rewritten, ty)) = self.rewritten.get(&key) {
            hir.kind = rewritten.clone();
            hir.ty = *ty;
            return Ok(());
        }
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
            if !self.validate_only && (self.hints || local_hints::numeric_call(hir)) {
                local_hints::inline(hir, facts, self.source_casts, self.meter)?;
            }
            if !self.validate_only { extrema::inline(hir, facts, self.meter)?; }
            // Generated nary comparison pairs are calls on Object temporaries,
            // not Clojure's binary :inline overloads.
            let source_comparison = hir.source.is_some();
            let primitive_parameters = if let Expression::Call { callee, arguments } = &*hir.kind {
                super::super::primitive::call_shape(hir, callee, self.meter)?
                    .and_then(|shape| shape.fixed(arguments.len()).map(|signature| signature.parameters.clone()))
            } else { None };
            use Expression::*;
            match hir.kind_mut() {
                LetFn { bindings, body } => {
                    let previous = bindings.iter().map(|binding|
                        (binding.id, facts.insert(binding.id, Primitive::Boxed))).collect();
                    let result = (|| {
                        for binding in bindings { self.walk(&mut binding.value, facts)?; }
                        self.walk(body, facts)
                    })();
                    restore(facts, previous);
                    result?;
                }
                Let { bindings, body } => {
                    let mut previous = Vec::with_capacity(bindings.len());
                    for binding in bindings {
                        self.walk(&mut binding.value, facts)?;
                        let old = if let Some(kind) = self.source_casts.get(&binding.id) {
                            facts.insert(binding.id, *kind)
                        } else {
                            let ty = primitive_returns::binding_type(binding, facts, self.meter)?;
                            facts.insert_type(binding.id, ty)
                        };
                        previous.push((binding.id, old));
                    }
                    let result = self.walk(body, facts);
                    restore(facts, previous);
                    result?;
                }
                Loop { target, bindings, body } => {
                    // Settle recur inputs before visiting even the later
                    // initializers: Clojure reanalyzes those when boxing an
                    // earlier loop local. Aliases and closures then see the
                    // same final source kind as direct reads of the local.
                    let kinds = if self.validate_only { None } else {
                        Some(primitive_returns::loop_types(*target, bindings, body, facts, self.meter)?)
                    };
                    let mut previous = Vec::with_capacity(bindings.len());
                    for (index, binding) in bindings.iter_mut().enumerate() {
                        self.walk(&mut binding.value, facts)?;
                        let ty = if let Some(kinds) = &kinds { kinds[index] } else {
                            let mut ty = primitive_returns::binding_type(binding, facts, self.meter)?;
                            ty.primitive = match ty.primitive { Primitive::Int => Primitive::Long,
                                Primitive::Float => Primitive::Double, kind => kind };
                            if ty.primitive != Primitive::Boxed { ty.class = ty.primitive; }
                            ty
                        };
                        previous.push((binding.id, facts.insert_type(binding.id, ty)));
                    }
                    let result = self.walk(body, facts);
                    restore(facts, previous);
                    result?;
                    if let Some(method) = self.methods.get(target) {
                        method.validate_return(facts, self.meter)?;
                    }
                }
                Function { parameters, body, .. } => {
                    let previous = parameters.iter().map(|parameter|
                        (parameter.id, facts.insert(parameter.id, Primitive::Boxed))).collect();
                    let result = self.walk(body, facts).and_then(|()| {
                        if !self.validate_only && !matches!(&*body.kind, Expression::Nominal {
                            operation: super::Nominal::LongCast | super::Nominal::DoubleCast, .. }) {
                            object_context::object(body, true, facts, self.source_casts, self.meter)?;
                        }
                        Ok(())
                    });
                    restore(facts, previous);
                    result?;
                }
                GeneralFunction { methods, .. } => {
                    for method in methods {
                        let previous = method.parameters.iter().map(|parameter|
                            (parameter.id, facts.insert(parameter.id, Primitive::Boxed))).collect();
                        let result = self.walk(&mut method.body, facts).and_then(|()| {
                            if !self.validate_only && !matches!(&*method.body.kind, Expression::Nominal {
                                operation: super::Nominal::LongCast | super::Nominal::DoubleCast, .. }) {
                                object_context::object(&mut method.body, true, facts, self.source_casts, self.meter)?;
                            }
                            Ok(())
                        });
                        restore(facts, previous);
                        result?;
                    }
                }
                Arithmetic { operator, arguments, coercing, primitive_overloads } => {
                    let inline = arguments_len_inline(*operator, arguments.len())
                        && (!self.hints || self.validate_only
                            || local_hints::numeric_operands(arguments, facts, self.meter)?);
                    for argument in arguments.iter_mut() {
                        self.walk(argument, facts)?;
                        if self.hints && !self.validate_only && !*coercing && inline {
                            local_hints::operand(argument, facts, self.meter)?;
                        }
                    }
                    primitive_overloads.clear();
                    if matches!(operator, super::Arithmetic::Divide | super::Arithmetic::Subtract)
                        && !*coercing && arguments.len() > 1
                    {
                        use Primitive::{Boxed, Double, Float, Long};
                        let mut previous = primitive_returns::kind(&arguments[0], facts, self.meter)?;
                        for argument in &arguments[1..] {
                            let kind = primitive_returns::kind(argument, facts, self.meter)?;
                            let floating = matches!(previous, Double | Float) || matches!(kind, Double | Float);
                            let primitive = floating || (*operator == super::Arithmetic::Subtract
                                && integer(previous) && integer(kind));
                            primitive_overloads.push(primitive);
                            previous = if floating { Double } else if primitive { Long } else { Boxed };
                        }
                    }
                }
                Bitwise { operation, arguments } => {
                    if !self.validate_only { local_hints::cast_overload(operation, arguments, facts, self.meter)?; }
                    let long_only = matches!(operation, super::Bitwise::And | super::Bitwise::Or | super::Bitwise::Xor
                        | super::Bitwise::AndNot | super::Bitwise::Not | super::Bitwise::Left | super::Bitwise::Right
                        | super::Bitwise::Unsigned);
                    let numeric = matches!(operation, super::Bitwise::Int | super::Bitwise::IntPrimitive | super::Bitwise::IntFloat | super::Bitwise::Cast | super::Bitwise::Quot
                        | super::Bitwise::Rem | super::Bitwise::Abs | super::Bitwise::UncheckedAdd
                        | super::Bitwise::UncheckedSubtract | super::Bitwise::UncheckedMultiply | super::Bitwise::UncheckedNegate);
                    let inline = (numeric && self.hints && !self.validate_only
                            && local_hints::numeric_operands(arguments, facts, self.meter)?)
                        || (long_only && self.hints && !self.validate_only
                            && local_hints::long_operands(arguments, facts, self.meter)?);
                    for argument in arguments.iter_mut() { self.walk(argument, facts)?;
                        if self.hints && !self.validate_only && inline { local_hints::operand(argument, facts, self.meter)?; }
                    }
                    if !self.validate_only && arguments.len() == 2
                        && matches!(operation, super::Bitwise::UncheckedSubtract | super::Bitwise::PromoteSubtract)
                    {
                        let a = primitive_returns::kind(&arguments[0], facts, self.meter)?;
                        let b = primitive_returns::kind(&arguments[1], facts, self.meter)?;
                        if matches!(a, Primitive::Double | Primitive::Float)
                            || matches!(b, Primitive::Double | Primitive::Float)
                            || (integer(a) && integer(b))
                        {
                            *operation = if *operation == super::Bitwise::UncheckedSubtract {
                                super::Bitwise::UncheckedSubtractPrimitive
                            } else { super::Bitwise::PromoteSubtractPrimitive };
                        }
                    }
                }
                Comparison { arguments, .. } => {
                    let inline = self.hints && !self.validate_only && source_comparison
                        && local_hints::numeric_operands(arguments, facts, self.meter)?;
                    for argument in arguments { self.walk(argument, facts)?;
                        if inline { local_hints::operand(argument, facts, self.meter)?; }
                    }
                }
                Nominal { operation, arguments } => {
                    let inline = source_comparison && matches!(operation, super::Nominal::LongCast
                        | super::Nominal::DoubleCast | super::Nominal::UncheckedLongCast);
                    for argument in arguments { self.walk(argument, facts)?;
                        if self.hints && !self.validate_only && inline { local_hints::operand(argument, facts, self.meter)?; }
                        if !self.validate_only && *operation == super::Nominal::Identical {
                            object_context::object(argument, true, facts, self.source_casts, self.meter)?;
                        }
                    }
                }
                Recur { target, arguments } => {
                    for argument in arguments.iter_mut() { self.walk(argument, facts)?; }
                    if let Some(method) = self.methods.get(target) {
                        method.validate_recur(arguments, facts, self.meter)?;
                    }
                }
                Array { operation, arguments } => {
                    for argument in arguments {
                        self.walk(argument, facts)?;
                        if !self.validate_only && matches!(operation, ArrayOperation::Objects | ArrayOperation::Literal) {
                            object_context::object(argument, false, facts, self.source_casts, self.meter)?;
                        }
                    }
                }
                Do(arguments) => { for argument in arguments { self.walk(argument, facts)?; } }
                DynamicScope { bindings, body, .. } | AsyncDynamicScope { bindings, body, .. } => {
                    for (_, value) in bindings { self.walk(value, facts)?; }
                    self.walk(body, facts)?;
                }
                AsyncTry { body, handler, cleanup, .. } => {
                    self.walk(body, facts)?;
                    for region in handler.iter_mut().chain(cleanup) { self.walk(region, facts)?; }
                }
                Try { regions } => { for region in regions { self.walk(region, facts)?; } }
                If { condition, consequent, alternative } => {
                    self.walk(condition, facts)?;
                    self.walk(consequent, facts)?;
                    self.walk(alternative, facts)?;
                }
                Call { callee, arguments } => {
                    self.walk(callee, facts)?;
                    for (position, argument) in arguments.iter_mut().enumerate() {
                        self.walk(argument, facts)?;
                        let boxed = primitive_parameters.as_ref().and_then(|parameters| parameters.get(position))
                            .is_none_or(|kind| *kind == super::super::primitive::Kind::Boxed);
                        if !self.validate_only && boxed { object_context::object(argument, true, facts, self.source_casts, self.meter)?; }
                    }
                }
                Definition { initializer, .. } => {
                    if let Some(value) = initializer { self.walk(value, facts)?; }
                }
                Assign { value, .. } | Throw(value) | NilTest(value) | Await { value }
                | Future { body: value, .. } => self.walk(value, facts)?,
                Local(_) => { if self.hints { primitive_returns::class(hir, facts, self.meter)?; } }
                Literal(_) | Global(_) | GlobalOrFallback { .. } | GlobalCell(_) => {}
            }
            Ok(())
        })?;
        self.rewritten.insert(key, (hir.kind.clone(), hir.ty));
        Ok(())
    }
}

fn restore(facts: &mut SourceFacts, previous: Vec<(BindingId, Option<SourceType>)>) {
    for (id, old) in previous.into_iter().rev() {
        match old { Some(kind) => { facts.insert_type(id, kind); }, None => { facts.remove(&id); } }
    }
}

fn arguments_len_inline(operator: Arithmetic, count: usize) -> bool {
    count > 1 || matches!(operator, Arithmetic::Subtract | Arithmetic::Negate)
}

/// JVM widens these source primitives to the long numeric overload.
fn integer(kind: Primitive) -> bool {
    matches!(kind, Primitive::Byte | Primitive::Short | Primitive::Int | Primitive::Long)
}
