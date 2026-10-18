//! Test-only preserved pre-optimization algorithm; no production use.
use super::*;
fn dependencies(session: &mut Session, recipe: &Recipe) -> Result<Vec<Value>> {
    let count = match recipe {
        Recipe::Alias(_) => 1,
        Recipe::List(v)
        | Recipe::Vector(v)
        | Recipe::ReaderVector(v)
        | Recipe::Set(v)
        | Recipe::ReaderSet(v) => v.len(),
        Recipe::Map(v) | Recipe::ReaderMap(v) => v
            .len()
            .checked_mul(2)
            .ok_or_else(|| invalid("Metadata expectation edge count overflow"))?,
        Recipe::Metadata { set_items, .. } => set_items
            .len()
            .checked_add(2)
            .ok_or_else(|| invalid("Metadata expectation edge count overflow"))?,
        _ => 0,
    };
    let mut result = Vec::new();
    reserve(session, &mut result, count)?;
    match recipe {
        Recipe::Alias(value) => push(session, &mut result, *value)?,
        Recipe::List(v)
        | Recipe::Vector(v)
        | Recipe::ReaderVector(v)
        | Recipe::Set(v)
        | Recipe::ReaderSet(v) => {
            for value in v {
                push(session, &mut result, *value)?;
            }
        }
        Recipe::Map(v) | Recipe::ReaderMap(v) => {
            for (key, value) in v {
                push(session, &mut result, *key)?;
                push(session, &mut result, *value)?;
            }
        }
        Recipe::Metadata {
            value,
            metadata,
            set_items,
            ..
        } => {
            push(session, &mut result, *value)?;
            for item in set_items {
                push(session, &mut result, *item)?;
            }
            push(session, &mut result, *metadata)?;
        }
        _ => {}
    }
    Ok(result)
}
struct Frame {
    id: Value,
    dependencies: Vec<Value>,
    next: usize,
    inputs: Vec<Option<Form>>,
}
fn frame(
    session: &mut Session,
    recipes: &[Option<Recipe>],
    id: Value,
    active: &[Frame],
) -> Result<Frame> {
    session.charge_host_work(1)?;
    for ancestor in active {
        session.charge_host_work(1)?;
        if ancestor.id == id {
            return Err(invalid("Cyclic metadata expectation recipe"));
        }
    }
    let recipe = recipes
        .get(id)
        .and_then(Option::as_ref)
        .ok_or_else(|| invalid("Missing or unfinished metadata expectation recipe"))?;
    Ok(Frame {
        id,
        dependencies: dependencies(session, recipe)?,
        next: 0,
        inputs: Vec::new(),
    })
}
fn discard(session: &mut Session, value: Form) -> Result<()> {
    crate::portable_macro_graph::source_facts::discard_form(session, value)
}
fn agrees(shape: MetadataShape, kind: &Kind) -> bool {
    match (shape, kind) {
        (MetadataShape::Symbol, Kind::Symbol(_))
        | (MetadataShape::Vector, Kind::Vector(_))
        | (MetadataShape::Map { .. }, Kind::Map(_)) => true,
        (MetadataShape::List { empty }, Kind::List(values)) => empty == values.is_empty(),
        (MetadataShape::Set, _) => true, // Its logical body comes from set_items.
        _ => false,
    }
}
fn finish(
    session: &mut Session,
    recipe: &Recipe,
    inputs: Vec<Option<Form>>,
) -> Result<Option<Form>> {
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
            .is_some_and(|base| !agrees(*shape, &base.kind))
        {
            return Err(invalid("Metadata recipe disagrees with its source shape"));
        }
        if inputs
            .last()
            .and_then(Option::as_ref)
            .is_some_and(|meta| !matches!(meta.kind, Kind::Map(_) | Kind::Nil))
        {
            return Err(invalid("Expected source metadata must be a map or nil"));
        }
    }
    if inputs.iter().any(Option::is_none) {
        for value in inputs.into_iter().flatten() {
            discard(session, value)?;
        }
        return Ok(None);
    }
    let mut values = Vec::new();
    reserve(session, &mut values, inputs.len())?;
    for input in inputs {
        values.push(input.unwrap());
    }
    let kind = match recipe {
        Recipe::Scalar(Literal::Undefined) => return Ok(None),
        Recipe::Scalar(Literal::Nil) => Kind::Nil,
        Recipe::Scalar(Literal::Bool(v)) => Kind::Bool(*v),
        Recipe::Scalar(Literal::Number(v)) => Kind::Number(*v),
        Recipe::Scalar(Literal::String(v)) => Kind::String(units(session, v)?),
        Recipe::Identifier {
            namespace,
            name,
            keyword,
            // Origin changes private initial storage, not logical Form syntax.
            literal: _,
        } => {
            let namespace = namespace
                .as_ref()
                .map(|value| text(session, value))
                .transpose()?;
            let name = text(session, name)?;
            if *keyword {
                Kind::Keyword(suss_reader::Keyword { namespace, name })
            } else {
                Kind::Symbol(suss_reader::Symbol { namespace, name })
            }
        }
        Recipe::Form(form) => {
            return Ok(Some(crate::portable_macro_graph::source_facts::normalized_copy(session, form)?));
        }
        Recipe::Alias(_) => return Ok(values.pop()),
        Recipe::List(_) => Kind::List(values),
        Recipe::Vector(_) | Recipe::ReaderVector(_) => Kind::Vector(values),
        Recipe::Map(_) | Recipe::ReaderMap(_) => Kind::Map(values),
        Recipe::Set(_) | Recipe::ReaderSet(_) => Kind::Set(values),
        Recipe::Metadata {
            shape, set_items, ..
        } => {
            let metadata = values
                .pop()
                .ok_or_else(|| invalid("Missing expected metadata value"))?;
            let mut inputs = values.into_iter();
            let mut base = inputs
                .next()
                .ok_or_else(|| invalid("Missing expected metadata receiver"))?;
            match shape {
                MetadataShape::Set => {
                    let mut items = Vec::new();
                    reserve(session, &mut items, set_items.len())?;
                    for item in inputs {
                        push(session, &mut items, item)?;
                    }
                    discard(session, base)?;
                    base = plain(Kind::Set(items));
                }
                _ => {}
            }
            for old in std::mem::take(&mut base.metadata) {
                discard(session, old)?;
            }
            match metadata.kind {
                Kind::Nil => {}
                Kind::Map(_) => push(session, &mut base.metadata, metadata)?,
                _ => return Err(invalid("Expected source metadata must be a map or nil")),
            }
            return Ok(Some(base));
        }
    };
    Ok(Some(plain(kind)))
}
pub(super) fn project(session: &mut Session, recipes: &[Option<Recipe>], root: Value) -> Result<Option<Form>> {
    let mut frames = Vec::new();
    let first = frame(session, recipes, root, &frames)?;
    push(session, &mut frames, first)?;
    while let Some(current) = frames.last_mut() {
        session.charge_host_work(1)?;
        if current.next < current.dependencies.len() {
            let id = current.dependencies[current.next];
            current.next += 1;
            let next = frame(session, recipes, id, &frames)?;
            push(session, &mut frames, next)?;
            continue;
        }
        let done = frames.pop().unwrap();
        let value = finish(session, recipes[done.id].as_ref().unwrap(), done.inputs)?;
        if let Some(parent) = frames.last_mut() {
            push(session, &mut parent.inputs, value)?;
        } else {
            return Ok(value);
        }
    }
    unreachable!("a source expectation has a root")
}
