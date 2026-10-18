//! Graph-local memoization of immutable native syntax, never of guest state.
use super::*;
use crate::portable::metered_map::TryOrderedMap;
use crate::portable_macro_graph::source_facts::shared::{
    self as syntax, Body, Meter, Node, Ref, Shape,
};

enum State {
    Active,
    Ready(Option<Ref>),
}
pub(in crate::portable_macro_graph) struct Builder<'a> {
    recipes: &'a [Option<Recipe>],
    states: TryOrderedMap<Value, State>,
}
struct Frame {
    id: Value,
    count: usize,
    next: usize,
    inputs: Vec<Option<Ref>>,
}
impl<'a> Builder<'a> {
    pub(in crate::portable_macro_graph) fn new(recipes: &'a [Option<Recipe>]) -> Self {
        Self {
            recipes,
            states: TryOrderedMap::default(),
        }
    }
    pub(in crate::portable_macro_graph) fn project(
        &mut self,
        session: &mut Session,
        root: Value,
    ) -> Result<Option<Ref>> {
        let result = self.run(session, root);
        // No rejected budget, interruption or malformed-source result is cached.
        // Existing immutable results already returned to callers remain intact.
        if result.is_err() {
            self.states = TryOrderedMap::default();
        }
        result
    }
    fn enter(&mut self, session: &mut Session, id: Value) -> Result<Option<Frame>> {
        session.charge_host_work(1)?;
        match self.states.get(&id, &mut Meter(session))? {
            Some(State::Active) => return Err(invalid("Cyclic metadata expectation recipe")),
            Some(State::Ready(_)) => return Ok(None),
            None => {}
        }
        let recipe = self
            .recipes
            .get(id)
            .and_then(Option::as_ref)
            .ok_or_else(|| invalid("Missing or unfinished metadata expectation recipe"))?;
        let count = dependency_count(recipe)?;
        self.states.insert(id, State::Active, &mut Meter(session))?;
        Ok(Some(Frame {
            id,
            count,
            next: 0,
            inputs: Vec::new(),
        }))
    }
    fn ready(&self, session: &mut Session, id: Value) -> Result<Option<Ref>> {
        match self.states.get(&id, &mut Meter(session))? {
            Some(State::Ready(value)) => value
                .as_ref()
                .map(|n| syntax::retain(session, n))
                .transpose(),
            _ => Err(invalid("Incomplete shared native projection")),
        }
    }
    fn run(&mut self, session: &mut Session, root: Value) -> Result<Option<Ref>> {
        let Some(first) = self.enter(session, root)? else {
            return self.ready(session, root);
        };
        let mut pending = Vec::new();
        push(session, &mut pending, first)?;
        while let Some(frame) = pending.last_mut() {
            session.charge_host_work(1)?;
            if frame.next < frame.count {
                let child = dependency(
                    session,
                    self.recipes[frame.id].as_ref().unwrap(),
                    frame.next,
                )?;
                if let Some(next) = self.enter(session, child)? {
                    push(session, &mut pending, next)?;
                } else {
                    let value = self.ready(session, child)?;
                    let frame = pending.last_mut().unwrap();
                    push(session, &mut frame.inputs, value)?;
                    frame.next += 1;
                }
                continue;
            }
            let done = pending.pop().unwrap();
            let value = finish_shared(
                session,
                self.recipes[done.id].as_ref().unwrap(),
                done.inputs,
            )?;
            let stored = value
                .as_ref()
                .map(|n| syntax::retain(session, n))
                .transpose()?;
            self.states
                .insert(done.id, State::Ready(stored), &mut Meter(session))?;
            if let Some(parent) = pending.last_mut() {
                push(session, &mut parent.inputs, value)?;
                parent.next += 1;
            } else {
                return Ok(value);
            }
        }
        unreachable!("native projection has a root")
    }
}
fn matches_shape(shape: MetadataShape, node: &Node) -> bool {
    match shape {
        MetadataShape::Symbol => matches!(&**node.body(), Body::Atom(Kind::Symbol(_))),
        MetadataShape::Vector => node.body().is(Shape::Vector),
        MetadataShape::Map { .. } => node.body().is(Shape::Map),
        MetadataShape::List { empty } => {
            matches!(&**node.body(), Body::Sequence(Shape::List, v) if empty == v.is_empty())
        }
        MetadataShape::Set => true,
    }
}
fn sequence(session: &mut Session, shape: Shape, values: Vec<Ref>) -> Result<Ref> {
    let body = syntax::body(session, Body::Sequence(shape, values))?;
    Node::new(session, body, None)
}
fn finish_shared(
    session: &mut Session,
    recipe: &Recipe,
    inputs: Vec<Option<Ref>>,
) -> Result<Option<Ref>> {
    session.charge_host_work(inputs.len() as u64 + 1)?;
    if let Recipe::Metadata {
        shape, set_items, ..
    } = recipe
    {
        session.charge_host_work(4)?;
        if !matches!(shape, MetadataShape::Set) && !set_items.is_empty() {
            return Err(invalid("Non-set metadata recipe has set operands"));
        }
        if inputs
            .first()
            .and_then(Option::as_ref)
            .is_some_and(|n| !matches_shape(*shape, n))
        {
            return Err(invalid("Metadata recipe disagrees with its source shape"));
        }
        if inputs.last().and_then(Option::as_ref).is_some_and(|n| {
            !n.body().is(Shape::Map) && !matches!(&**n.body(), Body::Atom(Kind::Nil))
        }) {
            return Err(invalid("Expected source metadata must be a map or nil"));
        }
    }
    if inputs.iter().any(Option::is_none) {
        return Ok(None);
    }
    let mut values = Vec::new();
    reserve(session, &mut values, inputs.len())?;
    for value in inputs {
        push(session, &mut values, value.unwrap())?;
    }
    let result = match recipe {
        Recipe::Scalar(Literal::Undefined) => return Ok(None),
        Recipe::Scalar(literal) => {
            let kind = match literal {
                Literal::Nil => Kind::Nil,
                Literal::Bool(v) => Kind::Bool(*v),
                Literal::Number(v) => Kind::Number(*v),
                Literal::String(v) => Kind::String(units(session, v)?),
                Literal::Undefined => unreachable!(),
            };
            let body = syntax::body(session, Body::Atom(kind))?;
            Node::new(session, body, None)?
        }
        Recipe::Identifier {
            namespace,
            name,
            keyword,
            ..
        } => {
            let namespace = namespace.as_ref().map(|v| text(session, v)).transpose()?;
            let name = text(session, name)?;
            let kind = if *keyword {
                Kind::Keyword(suss_reader::Keyword { namespace, name })
            } else {
                Kind::Symbol(suss_reader::Symbol { namespace, name })
            };
            let body = syntax::body(session, Body::Atom(kind))?;
            Node::new(session, body, None)?
        }
        Recipe::Form(form) => {
            let normalized = super::super::source_facts::normalized_copy(session, form)?;
            import_form(session, normalized)?
        }
        Recipe::Alias(_) => return Ok(values.pop()),
        Recipe::List(_) => sequence(session, Shape::List, values)?,
        Recipe::Vector(_) | Recipe::ReaderVector(_) => sequence(session, Shape::Vector, values)?,
        Recipe::Map(_) | Recipe::ReaderMap(_) => sequence(session, Shape::Map, values)?,
        Recipe::Set(_) | Recipe::ReaderSet(_) => sequence(session, Shape::Set, values)?,
        Recipe::Metadata { shape, .. } => {
            let metadata = values
                .pop()
                .ok_or_else(|| invalid("Missing expected metadata value"))?;
            let mut rest = values.into_iter();
            let base = rest
                .next()
                .ok_or_else(|| invalid("Missing expected metadata receiver"))?;
            let metadata = if matches!(&**metadata.body(), Body::Atom(Kind::Nil)) {
                None
            } else {
                Some(metadata)
            };
            if matches!(shape, MetadataShape::Set) {
                let mut items = Vec::new();
                reserve(session, &mut items, rest.len())?;
                for item in rest {
                    push(session, &mut items, item)?;
                }
                let body = syntax::body(session, Body::Sequence(Shape::Set, items))?;
                Node::new(session, body, metadata)?
            } else {
                Node::overlay(session, &base, metadata)?
            }
        }
    };
    Ok(Some(result))
}
fn import_form(session: &mut Session, form: Form) -> Result<Ref> {
    stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
        session.charge_host_work(3)?;
        let (span, metadata, kind) = form.into_parts();
        let metadata = if metadata.len() > 1 {
            return Err(invalid(
                "Logical source metadata must be one map on a symbol or collection",
            ));
        } else {
            metadata
                .into_iter()
                .next()
                .map(|f| import_form(session, f))
                .transpose()?
        };
        let body = match kind {
            Kind::List(v) => import_children(session, Shape::List, v)?,
            Kind::Vector(v) => import_children(session, Shape::Vector, v)?,
            Kind::Map(v) => import_children(session, Shape::Map, v)?,
            Kind::Set(v) => import_children(session, Shape::Set, v)?,
            kind => Body::Atom(kind),
        };
        let body = syntax::body(session, body)?;
        Node::at(session, body, metadata, span)
    })
}
fn import_children(session: &mut Session, shape: Shape, forms: Vec<Form>) -> Result<Body> {
    let mut nodes = Vec::new();
    reserve(session, &mut nodes, forms.len())?;
    for form in forms {
        let value = import_form(session, form)?;
        push(session, &mut nodes, value)?;
    }
    Ok(Body::Sequence(shape, nodes))
}

#[cfg(test)]
mod tests;
