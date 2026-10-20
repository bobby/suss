//! One request's source-aware first-use traversal. This module never creates a
//! Certificate, domain, receipt, native owner, or persistent identity entry.
//!
//! Unlike the rejected eager root loop, it visits ALL native dependencies in
//! their original order, including uncaptured siblings. A source observer may
//! select a current retained value only at that node's first-use position. The
//! same cache stores values AND their construction evidence across boundaries.
use super::*;
use crate::portable_macro_data::MetadataShape;

pub(super) struct Observed<E> {
    pub(super) value: crate::portable_session::SessionValue,
    pub(super) evidence: E,
}
pub(super) enum Entry<E, S> {
    Visit(S),
    Selected {
        value: Observed<E>,
        bindings: Vec<(Value, Observed<E>)>,
    },
}
/// The source controller must implement each explicit evidence operation. A
/// cached SessionValue alone is insufficient: the caller must retain its actual
/// original native factory observations, not resnapshot the value into proof.
pub(super) trait Observer {
    type Evidence;
    type Scope;
    /// Read-only eligibility/current checks only; no guest constructor replay.
    /// cached is the actual already-constructed value/evidence for this exact
    /// native ID. A retained selection may not replace that physical identity.
    fn enter(
        &mut self,
        graph: &mut AnalysisGraph<'_>,
        id: Value,
        cached: Option<&Observed<Self::Evidence>>,
        context: &Context<Self::Evidence>,
    ) -> Result<Entry<Self::Evidence, Self::Scope>>;
    /// Must prepay the real retain/release and evidence-buffer work.
    fn retain(
        &mut self,
        session: &mut Session,
        source: &Observed<Self::Evidence>,
    ) -> Result<Observed<Self::Evidence>>;
    /// ONE original recipe construction at its ordinary completion point. The
    /// implementation must call produce_native_recipe once using the correctly
    /// scoped original bridge. It must preserve all input construction evidence.
    fn construct(
        &mut self,
        graph: &mut AnalysisGraph<'_>,
        scope: Self::Scope,
        id: Value,
        inputs: Vec<Observed<Self::Evidence>>,
    ) -> Result<Observed<Self::Evidence>>;
}
struct Frame<E, S> {
    id: Value,
    scope: S,
    dependencies: Vec<Value>,
    next: usize,
    inputs: Vec<Observed<E>>,
}
/// Strictly build-local scratch. No guest value/evidence survives the request
/// through this type. Persistent identity policy belongs to a separate checked
/// source controller and cannot be inferred from native cache membership.
pub(super) struct Context<E> {
    arena: usize,
    count: usize,
    repeat: Vec<bool>,
    cache: Vec<Option<Observed<E>>>,
}
fn edges(graph: &mut AnalysisGraph<'_>, id: Value) -> Result<Vec<Value>> {
    let recipe = graph
        .recipes
        .get(id)
        .and_then(Option::as_ref)
        .ok_or_else(|| invalid("Missing first-use native recipe"))?;
    dependencies(graph.session, recipe)
}
impl<E> Context<E> {
    pub(super) fn new(graph: &mut AnalysisGraph<'_>, root: Value) -> Result<Self> {
        graph.finish_recipes()?;
        let count = graph.recipes.len();
        if root >= count {
            return Err(invalid("First-use root outside native arena"));
        }
        // Validate the entire original dependency graph before any callback,
        // including nodes a later retained selection might cut off. Repeat
        // semantics are EXACTLY the original native walk's reader policy.
        let mut state = Vec::new();
        let mut repeat = Vec::new();
        let mut cache = Vec::new();
        reserve(graph.session, &mut state, count)?;
        reserve(graph.session, &mut repeat, count)?;
        reserve(graph.session, &mut cache, count)?;
        graph.session.charge_host_work(
            (count as u64)
                .checked_mul(3)
                .ok_or_else(|| invalid("First-use scratch initialization overflow"))?,
        )?;
        state.resize(count, 0u8);
        repeat.resize(count, false);
        cache.resize_with(count, || None);
        let mut pending = Vec::new();
        populate(graph.session, &mut pending, std::iter::once((root, false)))?;
        while let Some((id, exit)) = pending.pop() {
            graph.session.charge_host_work(3)?;
            let slot = state
                .get_mut(id)
                .ok_or_else(|| invalid("First-use dependency outside arena"))?;
            if !exit {
                if *slot == 2 {
                    continue;
                }
                if *slot == 1 {
                    return Err(invalid("Cyclic first-use native graph"));
                }
                *slot = 1;
                let children = edges(graph, id)?;
                populate(graph.session, &mut pending, std::iter::once((id, true)))?;
                populate(
                    graph.session,
                    &mut pending,
                    children.into_iter().rev().map(|child| (child, false)),
                )?;
            } else {
                let children = edges(graph, id)?;
                graph.session.charge_host_work(children.len() as u64 + 2)?;
                repeat[id] = match graph.recipes[id].as_ref().unwrap() {
                    Recipe::ReaderMap(_) | Recipe::ReaderSet(_) => true,
                    Recipe::List(_) | Recipe::ReaderVector(_) | Recipe::Metadata { .. } => {
                        matches!(
                            graph.recipes[id].as_ref().unwrap(),
                            Recipe::Metadata {
                                shape: MetadataShape::Set,
                                ..
                            }
                        ) || children.iter().any(|child| repeat[*child])
                    }
                    _ => false,
                };
                state[id] = 2;
            }
        }
        Ok(Self {
            arena: graph.recipes.as_ptr() as usize,
            count,
            repeat,
            cache,
        })
    }
    // Trace-only read of the existing policy, not a new cache decision.
    // None means out of range; do not fabricate a repeat result for diagnostics.
    pub(super) fn diagnostic_repeat_policy(&self, id: Value) -> Option<bool> {
        self.repeat.get(id).copied()
    }
    pub(super) fn cached(&self, id: Value) -> Option<&Observed<E>> {
        if self.repeat.get(id).copied().unwrap_or(true) {
            None
        } else {
            self.cache.get(id).and_then(Option::as_ref)
        }
    }
    fn start<O: Observer<Evidence = E>>(
        &mut self,
        graph: &mut AnalysisGraph<'_>,
        observer: &mut O,
        frames: &mut Vec<Frame<E, O::Scope>>,
        id: Value,
    ) -> Result<Option<Observed<E>>> {
        graph.session.charge_host_work(3)?;
        if id >= self.count {
            return Err(invalid("First-use native input outside arena"));
        }
        let cached = if self.repeat[id] {
            None
        } else {
            self.cache[id].as_ref()
        };
        match observer.enter(graph, id, cached, self)? {
            Entry::Selected { value, bindings } => {
                // Every requested binding is preflighted before any native
                // cache write. No guest heap or persistent cache is modified.
                for (slot, proposed) in &bindings {
                    graph.session.charge_host_work(2)?;
                    if *slot >= self.count {
                        return Err(invalid("First-use binding outside arena"));
                    }
                    if let Some(old) = self.cache[*slot].as_ref().filter(|_| !self.repeat[*slot]) {
                        if !graph
                            .session
                            .values_identical(&old.value, &proposed.value)?
                        {
                            return Err(invalid(
                                "First-use retained binding conflicts with prior construction",
                            ));
                        }
                    }
                }
                // An already constructed native object cannot be silently
                // replaced by a same-content retained source object. Physical
                // comparison is a consistency check, never an owner lookup.
                if let Some(previous) = cached {
                    if !graph
                        .session
                        .values_identical(&previous.value, &value.value)?
                    {
                        return Err(invalid(
                            "First-use selection replaced constructed native identity",
                        ));
                    }
                }
                graph.session.charge_host_work(bindings.len() as u64)?;
                for (slot, proposed) in bindings {
                    if !self.repeat[slot] && self.cache[slot].is_none() {
                        self.cache[slot] = Some(proposed);
                    }
                }
                if !self.repeat[id] && self.cache[id].is_none() {
                    self.cache[id] = Some(observer.retain(graph.session, &value)?);
                }
                Ok(Some(value))
            }
            Entry::Visit(scope) => {
                if cached.is_some() {
                    return Err(invalid(
                        "First-use observer attempted native construction replay",
                    ));
                }
                let dependencies = edges(graph, id)?;
                reserve(graph.session, frames, 1)?;
                graph.session.charge_host_work(2)?;
                frames.push(Frame {
                    id,
                    scope,
                    dependencies,
                    next: 0,
                    inputs: Vec::new(),
                });
                Ok(None)
            }
        }
    }
    /// Resumptions share this context's exact cache/evidence. No separate
    /// materialize_using call is permitted for a captured root. This primitive
    /// does not publish, seal, or otherwise certify any observation.
    pub(super) fn walk<O: Observer<Evidence = E>>(
        &mut self,
        graph: &mut AnalysisGraph<'_>,
        root: Value,
        observer: &mut O,
    ) -> Result<Observed<E>> {
        let _request = graph.session.operation_budget_scope()?;
        graph.session.charge_host_work(3)?;
        if graph.recipes.len() != self.count || graph.recipes.as_ptr() as usize != self.arena {
            return Err(invalid(
                "First-use context belongs to a different native arena",
            ));
        }
        let mut frames = Vec::new();
        if let Some(value) = self.start(graph, observer, &mut frames, root)? {
            return Ok(value);
        }
        loop {
            let next = {
                let frame = frames
                    .last_mut()
                    .ok_or_else(|| invalid("Missing first-use frame"))?;
                graph.session.charge_host_work(2)?;
                if frame.next < frame.dependencies.len() {
                    let id = frame.dependencies[frame.next];
                    frame.next += 1;
                    Some(id)
                } else {
                    None
                }
            };
            let completed = if let Some(id) = next {
                match self.start(graph, observer, &mut frames, id)? {
                    Some(value) => Some(value),
                    None => continue,
                }
            } else {
                let frame = frames
                    .pop()
                    .ok_or_else(|| invalid("Missing completed first-use frame"))?;
                // Same one completion/dispatch debit as the original walker;
                // additional native context bookkeeping has its own debits.
                graph.session.charge_host_work(1)?;
                let value = observer.construct(graph, frame.scope, frame.id, frame.inputs)?;
                if !self.repeat[frame.id] {
                    self.cache[frame.id] = Some(observer.retain(graph.session, &value)?);
                }
                Some(value)
            };
            if let Some(value) = completed {
                if let Some(parent) = frames.last_mut() {
                    reserve(graph.session, &mut parent.inputs, 1)?;
                    graph.session.charge_host_work(1)?;
                    parent.inputs.push(value);
                } else {
                    return Ok(value);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::hir::Literal;
    struct Ordinary<'a>(&'a crate::portable_macro_data::FormBridge);
    impl Observer for Ordinary<'_> {
        type Evidence = ();
        type Scope = ();
        fn enter(
            &mut self,
            graph: &mut AnalysisGraph<'_>,
            _id: Value,
            cached: Option<&Observed<()>>,
            _context: &Context<()>,
        ) -> Result<Entry<(), ()>> {
            Ok(match cached {
                Some(value) => Entry::Selected {
                    value: self.retain(graph.session, value)?,
                    bindings: Vec::new(),
                },
                None => Entry::Visit(()),
            })
        }
        fn retain(&mut self, session: &mut Session, source: &Observed<()>) -> Result<Observed<()>> {
            session.charge_host_work(3)?;
            Ok(Observed {
                value: source.value.clone(),
                evidence: (),
            })
        }
        fn construct(
            &mut self,
            graph: &mut AnalysisGraph<'_>,
            _: (),
            id: Value,
            inputs: Vec<Observed<()>>,
        ) -> Result<Observed<()>> {
            let mut values = Vec::new();
            reserve(graph.session, &mut values, inputs.len())?;
            graph.session.charge_host_work(inputs.len() as u64)?;
            values.extend(inputs.into_iter().map(|input| input.value));
            let value = graph.produce_native_recipe(id, self.0, &values)?;
            Ok(Observed {
                value,
                evidence: (),
            })
        }
    }
    #[test]
    fn one_first_use_context_preserves_native_sharing_around_an_inline_sibling() {
        let mut session = Session::new_macro().unwrap();
        let bridge = crate::portable_macro_data::FormBridge::new(&mut session).unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let nil = graph.scalar(Literal::Nil).unwrap();
        let number = graph.number(17).unwrap();
        let key = graph.keyword("actual-key").unwrap();
        let shared = graph.map_values(vec![(key, number)]).unwrap();
        let sibling = graph.vector(&[nil, number]).unwrap();
        let root = graph.vector(&[shared, sibling, shared]).unwrap();
        let mut context = Context::new(&mut graph, root).unwrap();
        let observed = context
            .walk(&mut graph, root, &mut Ordinary(&bridge))
            .unwrap();
        let project = graph
            .session
            .eval("(fn [v] [(identical? (nth v 0) (nth v 2)) (count (nth v 0)) (count (nth v 1))])")
            .unwrap();
        graph.session.collect().unwrap();
        let result = graph.session.invoke(&project, &[&observed.value]).unwrap();
        assert_eq!(
            crate::portable_repl::display(graph.session, &result).unwrap(),
            "[true 1 2]"
        );
    }
    #[test]
    fn first_use_cycle_is_rejected_before_observer_construction() {
        let mut session = Session::new_macro().unwrap();
        let bridge = crate::portable_macro_data::FormBridge::new(&mut session).unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let root = graph.recipe(Recipe::Alias(0)).unwrap();
        assert!(Context::<()>::new(&mut graph, root).is_err());
    }
}
