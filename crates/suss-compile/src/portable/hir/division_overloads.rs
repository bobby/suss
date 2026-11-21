//! Select numeric overloads after lexical and loop source kinds settle.
//! Physical raw/boxed storage inference is a later, independent operation.
use super::*;
use primitive_returns::Primitive;
use std::sync::Arc;

pub(super) fn annotate(
    hir: &mut Hir,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    // Keep original expression addresses alive while the memoized rewrite
    // makes shallow copies. Analyzer BindingIds are unique within this HIR,
    // so every shared expression has one settled lexical source context.
    let original = hir.clone();
    let result = Pass { needed: HashMap::new(), rewritten: HashMap::new(), meter }
        .walk(hir, &mut HashMap::new());
    drop(original);
    result
}

struct Pass<'a> {
    needed: HashMap<usize, bool>,
    rewritten: HashMap<usize, Arc<Expression>>,
    meter: &'a mut dyn FnMut(u64) -> Result<(), Diagnostic>,
}

impl Pass<'_> {
    fn any<'a>(&mut self, children: impl IntoIterator<Item = &'a Hir>) -> Result<bool, Diagnostic> {
        for child in children {
            if self.has_division(child)? { return Ok(true); }
        }
        Ok(false)
    }

    fn has_division(&mut self, hir: &Hir) -> Result<bool, Diagnostic> {
        (self.meter)(1)?;
        let key = Arc::as_ptr(&hir.kind) as usize;
        if let Some(needed) = self.needed.get(&key) { return Ok(*needed); }
        let needed = stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
            use Expression::*;
            match &*hir.kind {
                Arithmetic { operator: super::Arithmetic::Divide, coercing: false, .. } => Ok(true),
                Arithmetic { arguments, .. } | Bitwise { arguments, .. }
                | Comparison { arguments, .. } | Array { arguments, .. }
                | Nominal { arguments, .. } | Recur { arguments, .. } | Do(arguments) => self.any(arguments),
                Let { bindings, body } | Loop { bindings, body, .. } =>
                    self.any(bindings.iter().map(|binding| &binding.value).chain(std::iter::once(body.as_ref()))),
                Function { body, .. } | Future { body, .. } => self.has_division(body),
                GeneralFunction { methods, .. } => self.any(methods.iter().map(|method| method.body.as_ref())),
                DynamicScope { bindings, body } | AsyncDynamicScope { bindings, body } =>
                    self.any(bindings.iter().map(|(_, value)| value).chain(std::iter::once(body.as_ref()))),
                AsyncTry { body, handler, cleanup, .. } => self.any(std::iter::once(body.as_ref())
                    .chain(handler.iter().map(Box::as_ref)).chain(cleanup.iter().map(Box::as_ref))),
                Try { regions } => self.any(regions.iter().map(Box::as_ref)),
                If { condition, consequent, alternative } => self.any([condition.as_ref(), consequent, alternative]),
                Call { callee, arguments } => self.any(std::iter::once(callee.as_ref()).chain(arguments)),
                Definition { initializer, .. } => self.any(initializer.iter().map(Box::as_ref)),
                Assign { value, .. } | Throw(value) | NilTest(value) | Await { value } => self.has_division(value),
                Literal(_) | Local(_) | Global(_) | GlobalOrFallback { .. } | GlobalCell(_) => Ok(false),
            }
        })?;
        self.needed.insert(key, needed);
        Ok(needed)
    }

    fn walk(&mut self, hir: &mut Hir, facts: &mut HashMap<BindingId, Primitive>) -> Result<(), Diagnostic> {
        (self.meter)(1)?;
        if !self.has_division(hir)? { return Ok(()); }
        let key = Arc::as_ptr(&hir.kind) as usize;
        if let Some(rewritten) = self.rewritten.get(&key) {
            hir.kind = rewritten.clone();
            return Ok(());
        }
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
            use Expression::*;
            match hir.kind_mut() {
                Let { bindings, body } => {
                    let mut previous = Vec::with_capacity(bindings.len());
                    for binding in bindings {
                        self.walk(&mut binding.value, facts)?;
                        let kind = primitive_returns::kind(&binding.value, facts, self.meter)?;
                        previous.push((binding.id, facts.insert(binding.id, kind)));
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
                    let kinds = primitive_returns::loop_kinds(*target, bindings, body, facts, self.meter)?;
                    let mut previous = Vec::with_capacity(bindings.len());
                    for (binding, kind) in bindings.iter_mut().zip(kinds) {
                        self.walk(&mut binding.value, facts)?;
                        previous.push((binding.id, facts.insert(binding.id, kind)));
                    }
                    let result = self.walk(body, facts);
                    restore(facts, previous);
                    result?;
                }
                Function { parameters, body, .. } => {
                    let previous = parameters.iter().map(|parameter|
                        (parameter.id, facts.insert(parameter.id, Primitive::Boxed))).collect();
                    let result = self.walk(body, facts);
                    restore(facts, previous);
                    result?;
                }
                GeneralFunction { methods, .. } => {
                    for method in methods {
                        let previous = method.parameters.iter().map(|parameter|
                            (parameter.id, facts.insert(parameter.id, Primitive::Boxed))).collect();
                        let result = self.walk(&mut method.body, facts);
                        restore(facts, previous);
                        result?;
                    }
                }
                Arithmetic { operator, arguments, coercing, division_primitives } => {
                    for argument in arguments.iter_mut() { self.walk(argument, facts)?; }
                    division_primitives.clear();
                    if *operator == super::Arithmetic::Divide && !*coercing && arguments.len() > 1 {
                        use Primitive::{Boxed, Double, Float};
                        let mut previous = primitive_returns::kind(&arguments[0], facts, self.meter)?;
                        for argument in &arguments[1..] {
                            let kind = primitive_returns::kind(argument, facts, self.meter)?;
                            let primitive = matches!(previous, Double | Float) || matches!(kind, Double | Float);
                            division_primitives.push(primitive);
                            previous = if primitive { Double } else { Boxed };
                        }
                    }
                }
                Bitwise { arguments, .. } | Comparison { arguments, .. } | Array { arguments, .. }
                | Nominal { arguments, .. } | Recur { arguments, .. } | Do(arguments) => {
                    for argument in arguments { self.walk(argument, facts)?; }
                }
                DynamicScope { bindings, body } | AsyncDynamicScope { bindings, body } => {
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
                    for argument in arguments { self.walk(argument, facts)?; }
                }
                Definition { initializer, .. } => {
                    if let Some(value) = initializer { self.walk(value, facts)?; }
                }
                Assign { value, .. } | Throw(value) | NilTest(value) | Await { value }
                | Future { body: value, .. } => self.walk(value, facts)?,
                Literal(_) | Local(_) | Global(_) | GlobalOrFallback { .. } | GlobalCell(_) => {}
            }
            Ok(())
        })?;
        self.rewritten.insert(key, hir.kind.clone());
        Ok(())
    }
}

fn restore(facts: &mut HashMap<BindingId, Primitive>, previous: Vec<(BindingId, Option<Primitive>)>) {
    for (id, old) in previous.into_iter().rev() {
        match old { Some(kind) => { facts.insert(id, kind); }, None => { facts.remove(&id); } }
    }
}
