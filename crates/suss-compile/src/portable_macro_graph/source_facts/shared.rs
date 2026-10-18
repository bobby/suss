//! Immutable native expectations. Sharing here never caches a guest verdict.
use super::*;
use crate::portable::metered_map::{Context, TryOrderedMap};
use crate::portable_session::SessionError;
use std::{cmp::Ordering, rc::Rc};

pub(in crate::portable_macro_graph) type Ref = Rc<Node>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::portable_macro_graph) enum Shape {
    List,
    Vector,
    Map,
    Set,
}
pub(in crate::portable_macro_graph) enum Body {
    Atom(Kind),
    Sequence(Shape, Vec<Ref>),
}
impl Body {
    pub(super) fn kind(&self) -> KindView<'_> {
        match self {
            Self::Atom(kind) => KindView::Atom(kind),
            Self::Sequence(shape, values) => KindView::Sequence(*shape, values),
        }
    }
    fn children(&self) -> &[Ref] {
        match self {
            Self::Sequence(_, v) => v,
            _ => &[],
        }
    }
    pub(in crate::portable_macro_graph) fn is(&self, shape: Shape) -> bool {
        matches!(self, Self::Sequence(s, _) if *s == shape)
    }
    fn header_error(&self) -> Option<&'static str> {
        match self {
            Self::Sequence(Shape::Map, v) if !v.len().is_multiple_of(2) => {
                Some("Logical source map requires paired entries")
            }
            Self::Atom(
                Kind::Nil
                | Kind::Bool(_)
                | Kind::Number(_)
                | Kind::String(_)
                | Kind::Symbol(_)
                | Kind::Keyword(_),
            )
            | Self::Sequence(_, _) => None,
            _ => Some("Logical source requires resolved reader data"),
        }
    }
}
pub(super) enum KindView<'a> {
    Atom(&'a Kind),
    Sequence(Shape, &'a [Ref]),
}

pub(in crate::portable_macro_graph) struct Node {
    // Option permits infallible bounded-stack destruction on error paths, without
    // allocating a replacement body or relying on guest/Store cleanup.
    body: Option<Rc<Body>>,
    pub(in crate::portable_macro_graph) span: std::ops::Range<usize>,
    metadata: Option<Ref>,
    error: Option<&'static str>,
    admitted: bool,
    small: bool,
}
impl Drop for Node {
    fn drop(&mut self) {
        stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
            drop(self.body.take());
            drop(self.metadata.take());
        });
    }
}
pub(in crate::portable_macro_graph) fn retain(session: &mut Session, value: &Ref) -> Result<Ref> {
    session.charge_host_work(2)?; // retain plus its eventual release
    Ok(value.clone())
}
pub(in crate::portable_macro_graph) fn body(
    session: &mut Session,
    value: Body,
) -> Result<Rc<Body>> {
    let count = value.children().len();
    // Charge the Rc allocation and eventual bounded destruction, including every
    // owned child-reference release, before allocating or publishing the body.
    session.charge_host_work(
        (std::mem::size_of::<Body>() + 2 * std::mem::size_of::<usize>()) as u64 + count as u64 + 2,
    )?;
    Ok(Rc::new(value))
}
impl Node {
    pub(in crate::portable_macro_graph) fn body(&self) -> &Rc<Body> {
        self.body.as_ref().unwrap()
    }
    pub(in crate::portable_macro_graph) fn error(&self) -> Option<&'static str> {
        self.error
    }
    pub(in crate::portable_macro_graph) fn admitted(&self) -> bool {
        self.admitted
    }
    pub(in crate::portable_macro_graph) fn small(&self) -> bool {
        self.small
    }
    pub(in crate::portable_macro_graph) fn new(
        session: &mut Session,
        body: Rc<Body>,
        metadata: Option<Ref>,
    ) -> Result<Ref> {
        Self::at(session, body, metadata, 0..0)
    }
    pub(in crate::portable_macro_graph) fn at(
        session: &mut Session,
        body: Rc<Body>,
        metadata: Option<Ref>,
        span: std::ops::Range<usize>,
    ) -> Result<Ref> {
        session.charge_host_work(8)?;
        let children = body.children();
        let mut error = body.header_error();
        if error.is_none()
            && metadata.is_some()
            && (!matches!(&*body, Body::Atom(Kind::Symbol(_)) | Body::Sequence(_, _))
                || !metadata.as_ref().unwrap().body().is(Shape::Map))
        {
            error = Some("Logical source metadata must be one map on a symbol or collection");
        }
        if error.is_none() {
            error = metadata.as_ref().and_then(|n| n.error);
        }
        for child in children.iter().rev() {
            session.charge_host_work(1)?;
            if error.is_none() {
                error = child.error;
            }
        }
        let mut admitted = error.is_none() && metadata.as_ref().is_none_or(|n| n.admitted);
        let mut small = metadata.as_ref().is_none_or(|n| n.small);
        for child in children {
            session.charge_host_work(2)?;
            admitted &= child.admitted;
            small &= child.small;
        }
        if error.is_none()
            && let Body::Sequence(shape @ (Shape::Map | Shape::Set), values) = &*body
        {
            let map = *shape == Shape::Map;
            let stride = if map { 2 } else { 1 };
            small &= values.len() / stride <= 16;
            // Structural validity remains separate from ambiguity. A duplicate
            // map stays projectable and can still expose a later shape error.
            for i in (0..values.len()).step_by(stride) {
                session.charge_host_work(1)?;
                admitted &= match &**values[i].body() {
                    Body::Atom(kind) => scalar_key_kind(session, kind, map)?,
                    _ => false,
                };
                for j in (0..i).step_by(stride) {
                    session.charge_host_work(1)?;
                    if locator_nodes(session, &values[j], &values[i])? {
                        admitted = false;
                    }
                }
            }
        }
        session.charge_host_work(
            (std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>()) as u64 + 3,
        )?;
        Ok(Rc::new(Self {
            body: Some(body),
            span,
            metadata,
            error,
            admitted,
            small,
        }))
    }
    pub(in crate::portable_macro_graph) fn overlay(
        session: &mut Session,
        base: &Ref,
        metadata: Option<Ref>,
    ) -> Result<Ref> {
        session.charge_host_work(2)?; // body retain/release; never mutate base
        Self::at(session, base.body().clone(), metadata, base.span.clone())
    }
}
// Reuse the exact scalar-key policy, including Symbol's canonical FQN and signed
// zero key location. Empty metadata is only a borrowed syntax view here.
fn scalar_key_kind(session: &mut Session, kind: &Kind, map: bool) -> Result<bool> {
    match kind {
        Kind::String(_) | Kind::Keyword(_) => Ok(true),
        Kind::Symbol(symbol) => {
            let bytes = symbol
                .name
                .len()
                .checked_add(symbol.namespace.as_ref().map_or(0, String::len))
                .and_then(|n| n.checked_add(3))
                .and_then(|n| u64::try_from(n).ok())
                .ok_or_else(|| {
                    SessionError::Host(wasmtime::Error::msg("Symbol key scan overflow"))
                })?;
            session.charge_host_work(bytes)?;
            Ok(!symbol.name.is_empty()
                && match &symbol.namespace {
                    Some(ns) => !ns.is_empty() && !ns.contains('/'),
                    None => symbol.name == "/" || !symbol.name.contains('/'),
                })
        }
        Kind::Nil | Kind::Bool(_) => Ok(!map),
        Kind::Number(v) => Ok(!map && v.is_finite()),
        _ => Ok(false),
    }
}
fn scalar_equal(session: &mut Session, a: &Kind, b: &Kind, locator: bool) -> Result<bool> {
    session.charge_host_work(1)?;
    Ok(match (a, b) {
        (Kind::Nil, Kind::Nil) => true,
        (Kind::Bool(a), Kind::Bool(b)) => a == b,
        (Kind::Number(a), Kind::Number(b)) => {
            if locator {
                a == b
            } else {
                a.to_bits() == b.to_bits()
            }
        }
        (Kind::String(a), Kind::String(b)) => text_equal(session, a, b)?,
        (Kind::Keyword(a), Kind::Keyword(b)) => keyword_equal(session, a, b)?,
        (Kind::Symbol(a), Kind::Symbol(b)) => {
            let ns = match (&a.namespace, &b.namespace) {
                (None, None) => true,
                (Some(a), Some(b)) => text_equal(session, a.as_bytes(), b.as_bytes())?,
                _ => false,
            };
            ns && text_equal(session, a.name.as_bytes(), b.name.as_bytes())?
        }
        _ => false,
    })
}
fn locator_nodes(session: &mut Session, a: &Node, b: &Node) -> Result<bool> {
    match (&**a.body(), &**b.body()) {
        (Body::Atom(a), Body::Atom(b)) => scalar_equal(session, a, b, true),
        _ => Ok(false),
    }
}
fn push<T>(session: &mut Session, v: &mut Vec<T>, item: T) -> Result<()> {
    super::super::reuse::reserve(session, v, 1)?;
    session.charge_host_work(1)?;
    v.push(item);
    Ok(())
}
fn actual_children(kind: &Kind) -> Option<(Shape, &[Form])> {
    Some(match kind {
        Kind::List(v) => (Shape::List, v),
        Kind::Vector(v) => (Shape::Vector, v),
        Kind::Map(v) => (Shape::Map, v),
        Kind::Set(v) => (Shape::Set, v),
        _ => return None,
    })
}
fn push_pair<'a, 'b>(
    session: &mut Session,
    pending: &mut Vec<(&'a Node, &'b Form)>,
    pair: (&'a Node, &'b Form),
) -> Result<()> {
    super::super::reuse::reserve(session, pending, 1)?;
    session.charge_host_work(3)?; // both references plus publication, as legacy
    pending.push(pair);
    Ok(())
}
fn paired(session: &mut Session, expected: &Node, actual: &Form) -> Result<bool> {
    let mut pending = Vec::new();
    push_pair(session, &mut pending, (expected, actual))?;
    while let Some((a, b)) = pending.pop() {
        session.charge_host_work(8)?;
        if usize::from(a.metadata.is_some()) != b.metadata.len() {
            return Ok(false);
        }
        match a.body().kind() {
            KindView::Atom(kind) => {
                if !scalar_equal(session, kind, &b.kind, false)? {
                    return Ok(false);
                }
            }
            KindView::Sequence(shape, av) => {
                let Some((bs, bv)) = actual_children(&b.kind) else {
                    return Ok(false);
                };
                if shape != bs || av.len() != bv.len() {
                    return Ok(false);
                }
                if matches!(shape, Shape::Map | Shape::Set) {
                    let stride = if shape == Shape::Map { 2 } else { 1 };
                    if !bv.len().is_multiple_of(stride) {
                        return Ok(false);
                    }
                    for i in (0..av.len()).step_by(stride).rev() {
                        let mut found = None;
                        for j in (0..bv.len()).step_by(stride) {
                            session.charge_host_work(2)?;
                            if let Body::Atom(kind) = &**av[i].body()
                                && scalar_equal(session, kind, &bv[j].kind, true)?
                            {
                                found = Some(j);
                                break;
                            }
                        }
                        let Some(j) = found else {
                            return Ok(false);
                        };
                        if stride == 2 {
                            push_pair(session, &mut pending, (&*av[i + 1], &bv[j + 1]))?;
                        }
                        push_pair(session, &mut pending, (&*av[i], &bv[j]))?;
                    }
                } else {
                    for (a, b) in av.iter().zip(bv).rev() {
                        push_pair(session, &mut pending, (&**a, b))?;
                    }
                }
            }
        }
        if let Some(metadata) = &a.metadata {
            push_pair(session, &mut pending, (&**metadata, &b.metadata[0]))?;
        }
    }
    Ok(true)
}
pub(in crate::portable_macro_graph) struct Meter<'a>(
    pub(in crate::portable_macro_graph) &'a mut Session,
);
impl Context<usize> for Meter<'_> {
    type Error = SessionError;
    fn charge(&mut self, n: u64) -> Result<()> {
        self.0.charge_host_work(n)
    }
    fn compare(&mut self, a: &usize, b: &usize) -> Result<Ordering> {
        self.0.charge_host_work(1)?;
        Ok(a.cmp(b))
    }
}
fn canonical(
    session: &mut Session,
    root: &Node,
    keys: &mut Interner,
) -> Result<super::super::raw_keys::KeyId> {
    let mut done = TryOrderedMap::default();
    let mut pending = Vec::new();
    push(session, &mut pending, (root, false))?;
    while let Some((node, finish)) = pending.pop() {
        let id = node as *const Node as usize;
        if done.get(&id, &mut Meter(session))?.is_some() {
            continue;
        }
        session.charge_host_work(3)?;
        if !finish {
            push(session, &mut pending, (node, true))?;
            for child in node.body().children().iter().rev() {
                push(session, &mut pending, (&**child, false))?;
            }
            if let Some(meta) = &node.metadata {
                push(session, &mut pending, (&**meta, false))?;
            }
            continue;
        }
        let mut children = Vec::new();
        super::super::reuse::reserve(session, &mut children, node.body().children().len())?;
        for child in node.body().children() {
            let key = *done
                .get(&(Rc::as_ptr(child) as usize), &mut Meter(session))?
                .unwrap();
            session.charge_host_work(1)?;
            children.push(key);
        }
        let mut metadata = Vec::new();
        if let Some(meta) = &node.metadata {
            let key = *done
                .get(&(Rc::as_ptr(meta) as usize), &mut Meter(session))?
                .unwrap();
            push(session, &mut metadata, key)?;
        }
        let placeholder;
        let kind = match &**node.body() {
            Body::Atom(kind) => kind,
            Body::Sequence(shape, _) => {
                placeholder = match shape {
                    Shape::List => Kind::List(vec![]),
                    Shape::Vector => Kind::Vector(vec![]),
                    Shape::Map => Kind::Map(vec![]),
                    Shape::Set => Kind::Set(vec![]),
                };
                &placeholder
            }
        };
        let key = keys.unordered_node(kind, &metadata, &children, &mut |n| {
            session.charge_host_work(n)
        })?;
        done.insert(id, key, &mut Meter(session))?;
    }
    Ok(*done
        .get(&(root as *const Node as usize), &mut Meter(session))?
        .unwrap())
}
pub(in crate::portable_macro_graph) fn matches(
    session: &mut Session,
    expected: &Node,
    actual: &Form,
) -> Result<bool> {
    if expected.small {
        return paired(session, expected, actual);
    }
    let mut keys = Interner::default();
    let expected = canonical(session, expected, &mut keys)?;
    let mut plan = Plan::default();
    let actual = keys.unordered_plan(actual, &mut plan, &mut |n| session.charge_host_work(n))?;
    session.charge_host_work(1)?;
    Ok(expected == plan.nodes[actual].key)
}

// Independent differential observation only. Production admission/comparison
// never reconstructs the expanded owned tree.
#[cfg(test)]
pub(in crate::portable_macro_graph) fn to_form(node: &Node) -> Form {
    stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
        let kind = match &**node.body() {
            Body::Atom(kind) => kind.clone(),
            Body::Sequence(shape, values) => {
                let values = values.iter().map(|n| to_form(n)).collect();
                match shape {
                    Shape::List => Kind::List(values),
                    Shape::Vector => Kind::Vector(values),
                    Shape::Map => Kind::Map(values),
                    Shape::Set => Kind::Set(values),
                }
            }
        };
        Form {
            span: node.span.clone(),
            metadata: node.metadata.iter().map(|n| to_form(n)).collect(),
            kind,
        }
    })
}
