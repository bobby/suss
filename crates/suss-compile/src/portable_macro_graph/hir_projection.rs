//! Private immutable lowering views. Ordinals select retained native facts;
//! they never certify a source revision or a guest value.
use super::{hir, Expression, Hir, Literal, Result, Session, SessionError};
use std::{ops::Range, sync::Arc};

pub(super) enum Owner {
    Shared(Arc<Hir>),
    Slice { nodes: Arc<[Hir]>, index: usize },
}
impl Owner {
    pub fn root(&self) -> &Hir {
        match self { Self::Shared(node) => node, Self::Slice { nodes, index } => &nodes[*index] }
    }
    pub fn shared(session: &mut Session, node: &Arc<Hir>) -> Result<Self> {
        session.charge_host_work(2)?; // retain and eventual release
        Ok(Self::Shared(node.clone()))
    }
    pub fn slice(session: &mut Session, nodes: &Arc<[Hir]>, index: usize) -> Result<Self> {
        session.charge_host_work(3)?; // bounds, retain and eventual release
        if nodes.get(index).is_none() { return Err(invalid("Invalid source HIR slice index")); }
        Ok(Self::Slice { nodes: nodes.clone(), index })
    }
    pub fn copied(session: &mut Session, node: Hir) -> Result<Self> {
        session.charge_host_work((std::mem::size_of::<Hir>() + 2 * std::mem::size_of::<usize>() + 2) as u64)?;
        Ok(Self::Shared(Arc::new(node)))
    }
}
pub(super) enum Detail {
    None, Literal(Literal), Local(usize), Loop(usize),
    Arithmetic(hir::Arithmetic), Nominal(hir::Nominal), Array(hir::ArrayOperation),
    Comparison(hir::Comparison), Bitwise(hir::Bitwise),
}
pub(super) struct Node {
    pub source: Option<Arc<hir::SourceAnalysis>>,
    pub operation: &'static str,
    pub detail: Detail,
    children: Range<usize>,
}
struct Arena { owner: Owner, nodes: Vec<Node> }
pub(super) struct View { arena: Arc<Arena>, index: usize }
fn invalid(message: &'static str) -> SessionError { SessionError::Host(wasmtime::Error::msg(message)) }
type Meter<'a> = dyn FnMut(u64) -> Result<()> + 'a;
fn reserve<T>(meter: &mut Meter<'_>, values: &mut Vec<T>, extra: usize) -> Result<()> {
    meter(3)?; // reserve control plus eventual backing/metadata release
    if extra > values.capacity() - values.len() {
        let needed = values.len().checked_add(extra).ok_or_else(|| invalid("HIR projection capacity overflow"))?;
        let capacity = values.capacity().checked_mul(2).ok_or_else(|| invalid("HIR projection capacity overflow"))?.max(needed).max(4);
        let bytes = capacity.checked_add(values.len()).and_then(|n| n.checked_mul(std::mem::size_of::<T>()))
            .and_then(|n| u64::try_from(n).ok()).ok_or_else(|| invalid("HIR projection allocation overflow"))?;
        meter(bytes)?;
        values.try_reserve_exact(capacity - values.len()).map_err(|_| invalid("Unable to reserve HIR projection"))?;
    }
    Ok(())
}
fn push<T>(meter: &mut Meter<'_>, values: &mut Vec<T>, value: T) -> Result<()> {
    reserve(meter, values, 1)?;
    meter(2)?; // initialize and later drop this entry, before acquisition
    values.push(value); Ok(())
}
fn copy_literal_metered(value: &Literal, meter: &mut Meter<'_>) -> Result<Literal> {
    Ok(match value {
        Literal::String(units) => {
            let mut copy = Vec::new();
            reserve(meter, &mut copy, units.len())?;
            meter(units.len() as u64 + 1)?; // copy plus eventual literal-buffer release
            copy.extend_from_slice(units);
            Literal::String(copy)
        }
        Literal::Nil => Literal::Nil, Literal::Undefined => Literal::Undefined,
        Literal::Bool(value) => Literal::Bool(*value), Literal::Number(value) => Literal::Number(*value),
    })
}
pub(super) fn copy_literal(session: &mut Session, value: &Literal) -> Result<Literal> {
    copy_literal_metered(value, &mut |work| session.charge_host_work(work))
}
impl View {
    pub fn root(session: &mut Session, owner: Owner) -> Result<Self> {
        Self::build(owner, &mut |work| session.charge_host_work(work))
    }
    fn build(owner: Owner, meter: &mut Meter<'_>) -> Result<Self> {
        meter((std::mem::size_of::<Arena>() + 2 * std::mem::size_of::<usize>()
            + std::mem::size_of::<Vec<&Hir>>() + std::mem::size_of::<Self>() + 4) as u64)?;
        let mut pending = Vec::new();
        push(meter, &mut pending, owner.root())?;
        // Pass 1: immutable BFS discovery. No AST queue/memo publication.
        // A node's children are appended contiguously and never deduplicated.
        let mut cursor = 0;
        while cursor < pending.len() {
            meter(2)?;
            let (_, children) = children(pending[cursor]);
            for child in children {
                meter(1)?;
                push(meter, &mut pending, child)?;
            }
            cursor += 1;
        }
        let bytes = pending.len().checked_mul(std::mem::size_of::<Node>())
            .and_then(|n| u64::try_from(n).ok()).ok_or_else(|| invalid("HIR projection node backing overflow"))?;
        meter(bytes)?;
        meter(3)?; // single reserve and eventual backing release
        let mut nodes = Vec::new();
        nodes.try_reserve_exact(pending.len()).map_err(|_| invalid("Unable to reserve exact HIR projection nodes"))?;
        // Pass 2: final node storage has exact requested capacity, no relocation.
        // Recount child arity using the same bounded iterator. The immutable
        // BFS queue implies each range is exactly the next unassigned node IDs.
        let mut next_unassigned = 1usize;
        for node in &pending {
            meter(2)?;
            let (operation, children) = children(node);
            let start = next_unassigned;
            for _ in children {
                meter(1)?;
                next_unassigned = next_unassigned.checked_add(1)
                    .ok_or_else(|| invalid("HIR projection child range overflow"))?;
            }
            if next_unassigned > pending.len() { return Err(invalid("Invalid HIR projection child range")); }
            let detail = match &node.kind {
                Expression::Literal(value) => Detail::Literal(copy_literal_metered(value, meter)?),
                Expression::Local(id) => Detail::Local(id.0),
                Expression::Arithmetic { operator, .. } => Detail::Arithmetic(*operator),
                Expression::Nominal { operation, .. } => Detail::Nominal(*operation),
                Expression::Array { operation, .. } => Detail::Array(*operation),
                Expression::Comparison { operation, .. } => Detail::Comparison(*operation),
                Expression::Bitwise { operation, .. } => Detail::Bitwise(*operation),
                Expression::Loop { target, .. } | Expression::Recur { target, .. } => Detail::Loop(target.0),
                _ => Detail::None,
            };
            meter(2)?; // exact source owner retain and eventual release
            meter(2)?; // initialize and eventual node release before push
            nodes.push(Node { source: node.source.clone(), operation, detail, children: start..next_unassigned });
        }
        if next_unassigned != pending.len() { return Err(invalid("Incomplete HIR projection child ranges")); }
        // Every backing store/entry/retain was prepaid at acquisition, so
        // failures at any earlier meter call also have covered cleanup.
        drop(pending); // no reference into owner escapes this private build
        Ok(Self { arena: Arc::new(Arena { owner, nodes }), index: 0 })
    }
    pub fn node(&self) -> &Node { &self.arena.nodes[self.index] }
    pub fn retained_root(&self) -> &Hir { self.arena.owner.root() }
    pub fn child_count(&self) -> usize { self.node().children.len() }
    pub fn child(&self, session: &mut Session, ordinal: usize) -> Result<Self> {
        session.charge_host_work(6)?; // bounds/range/node reads, retain and release
        if ordinal >= self.child_count() { return Err(invalid("Invalid lowering child ordinal")); }
        let id = self.node().children.start + ordinal;
        if id >= self.arena.nodes.len() { return Err(invalid("Invalid lowering node ordinal")); }
        Ok(Self { arena: self.arena.clone(), index: id })
    }
}

// No temporary child Vec: each operation's iterator is a borrowed immutable
// slice/chain. This enum avoids boxed iterator allocation at every HIR node.
enum Children<'a> {
    Slice(std::slice::Iter<'a, Hir>), One(Option<&'a Hir>),
    Many { node: &'a Hir, next: usize },
}
impl<'a> Iterator for Children<'a> {
    type Item = &'a Hir;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Slice(values) => values.next(), Self::One(value) => value.take(),
            Self::Many { node, next } => { let result = child_at(node, *next); *next += usize::from(result.is_some()); result }
        }
    }
}
fn child_at(node: &Hir, ordinal: usize) -> Option<&Hir> {
    match &node.kind {
        Expression::AsyncTry { body, handler, cleanup, .. } =>
            [Some(body.as_ref()), handler.as_deref(), cleanup.as_deref()].into_iter().flatten().nth(ordinal),
        Expression::Definition { initializer, .. } => if ordinal == 0 { initializer.as_deref() } else { None },
        Expression::Let { bindings, body } | Expression::Loop { bindings, body, .. } =>
            bindings.get(ordinal).map(|binding| binding.value.as_ref()).or_else(|| (ordinal == bindings.len()).then_some(body.as_ref())),
        Expression::If { condition, consequent, alternative } => [condition.as_ref(), consequent.as_ref(), alternative.as_ref()].get(ordinal).copied(),
        Expression::Call { callee, arguments } => if ordinal == 0 { Some(callee) } else { arguments.get(ordinal - 1) },
        Expression::GeneralFunction { methods, .. } => methods.get(ordinal).map(|method| method.body.as_ref()),
        Expression::AsyncDynamicScope { bindings, body } | Expression::DynamicScope { bindings, body } =>
            bindings.get(ordinal).map(|(_, value)| value).or_else(|| (ordinal == bindings.len()).then_some(body.as_ref())),
        Expression::Try { regions } => regions.get(ordinal).map(Box::as_ref),
        _ => None,
    }
}
fn children(node: &Hir) -> (&'static str, Children<'_>) {
    use Expression::*;
    match &node.kind {
        Future { body, .. } => ("future", Children::One(Some(body))),
        Await { value } => ("await", Children::One(Some(value))),
        AsyncTry { .. } => ("async-try", Children::Many { node, next: 0 }),
        Literal(_) => ("literal", Children::One(None)), Local(_) => ("local", Children::One(None)),
        Global(_) => ("global", Children::One(None)), GlobalCell(_) => ("global-cell", Children::One(None)),
        GlobalOrFallback { .. } => ("reader-global-fallback", Children::One(None)),
        Bitwise { arguments, .. } | Comparison { arguments, .. } | Array { arguments, .. }
        | Nominal { arguments, .. } | Arithmetic { arguments, .. } => ("native-operation", Children::Slice(arguments.iter())),
        NilTest(value) => ("nil-test", Children::One(Some(value))),
        Throw(value) => ("throw", Children::One(Some(value))),
        Assign { value, .. } => ("assign", Children::One(Some(value))),
        Definition { .. } => ("definition", Children::Many { node, next: 0 }),
        Let { .. } => ("let", Children::Many { node, next: 0 }), Loop { .. } => ("loop", Children::Many { node, next: 0 }),
        If { .. } => ("if", Children::Many { node, next: 0 }),
        Do(values) => ("do", Children::Slice(values.iter())), Recur { arguments, .. } => ("recur", Children::Slice(arguments.iter())),
        Call { .. } => ("call", Children::Many { node, next: 0 }),
        Function { body, .. } => ("function", Children::One(Some(body))),
        GeneralFunction { .. } => ("function", Children::Many { node, next: 0 }),
        AsyncDynamicScope { .. } => ("async-dynamic-scope", Children::Many { node, next: 0 }),
        DynamicScope { .. } => ("dynamic-scope", Children::Many { node, next: 0 }),
        Try { .. } => ("try", Children::Many { node, next: 0 }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn literal(value: f64) -> Hir {
        Hir { source: None, span: 3..8, metadata: vec![], ty: hir::Type::Number,
            kind: Expression::Literal(Literal::Number(value)) }
    }
    #[test]
    fn retained_projection_preserves_real_owner_and_all_ordered_lowering_children() {
        let metadata = suss_reader::forms::read_forms(
            &format!("{{:doc \"{}\"}}", "m".repeat(1024))).unwrap();
        let root = Arc::new(Hir { source: None, span: 0..99, metadata, ty: hir::Type::Value,
            kind: Expression::Call { callee: Box::new(literal(17.0)), arguments: vec![literal(-0.0), literal(f64::NAN)] } });
        let initial = Arc::strong_count(&root);
        let metadata_buffer = root.metadata.as_ptr();
        let mut old_copy_work = 0;
        let copied = hir::clone_hir_metered(&root, &mut |work| { old_copy_work += work; Ok(()) }).unwrap();
        assert_eq!(copied.metadata, root.metadata);
        assert_ne!(copied.metadata.as_ptr(), metadata_buffer, "old route owns a separate copied metadata vector");
        drop(copied);
        let mut charges = Vec::new();
        let view = View::build(Owner::Shared(root.clone()), &mut |work| { charges.push(work); Ok(()) }).unwrap();
        assert!(std::ptr::eq(view.retained_root(), root.as_ref()));
        assert_eq!(view.retained_root().metadata.as_ptr(), metadata_buffer, "complete unselected metadata stays in the genuine owner");
        assert_eq!(view.retained_root().metadata, root.metadata);
        assert_eq!(view.node().operation, "call");
        assert_eq!(view.child_count(), 3);
        for (ordinal, expected) in [17.0_f64, -0.0, f64::NAN].into_iter().enumerate() {
            let id = view.node().children.start + ordinal;
            let Detail::Literal(Literal::Number(actual)) = &view.arena.nodes[id].detail else { panic!("literal child") };
            assert_eq!(actual.to_bits(), expected.to_bits(), "callee/argument order and bits");
        }
        assert_eq!(view.arena.nodes.len(), 4);
        assert_eq!(view.arena.nodes.iter().map(|node| node.children.len()).sum::<usize>(), 3);
        let equal_but_distinct = Arc::new(root.as_ref().clone());
        let other = View::build(Owner::Shared(equal_but_distinct.clone()), &mut |_| Ok(())).unwrap();
        assert!(!std::ptr::eq(view.retained_root(), other.retained_root()), "no equal-content owner interning");
        assert!(Arc::ptr_eq(&equal_but_distinct, match &other.arena.owner { Owner::Shared(owner) => owner, _ => unreachable!() }));
        drop(view);
        assert_eq!(Arc::strong_count(&root), initial);
        let projection_work = charges.iter().sum::<u64>();
        assert!(projection_work < old_copy_work, "this metadata-heavy fixture must remove actual owned-copy work: projection={projection_work}, old={old_copy_work}");
        eprintln!("hir-projection nodes=4 logical_child_slots=3 charged={projection_work} old_copy={old_copy_work} metadata={} node_bytes={}",
            std::mem::size_of::<Arena>(), std::mem::size_of::<Node>());
    }
    #[test]
    fn flat_projection_handles_deep_and_wide_executable_trees_without_path_replay() {
        let mut measured = Vec::new();
        for count in [16, 64] {
            let mut deep = literal(17.0);
            for _ in 0..count {
                deep = Hir { source: None, span: 0..1, metadata: vec![], ty: hir::Type::Value,
                    kind: Expression::Throw(Box::new(deep)) };
            }
            let root = Arc::new(deep);
            let mut charged = 0;
            let view = View::build(Owner::Shared(root.clone()), &mut |work| { charged += work; Ok(()) }).unwrap();
            assert_eq!(view.arena.nodes.len(), count + 1);
            assert_eq!(view.arena.nodes.iter().map(|node| node.children.len()).sum::<usize>(), count);
            let mut id = 0;
            for _ in 0..count {
                let node = &view.arena.nodes[id];
                assert_eq!(node.operation, "throw");
                assert_eq!(node.children.len(), 1);
                id = node.children.start;
            }
            assert!(matches!(view.arena.nodes[id].detail, Detail::Literal(Literal::Number(17.0))));
            let wide = Arc::new(Hir { source: None, span: 0..1, metadata: vec![], ty: hir::Type::Value,
                kind: Expression::Do((0..count).map(|i| literal(i as f64)).collect()) });
            let mut wide_charged = 0;
            let wide = View::build(Owner::Shared(wide), &mut |work| { wide_charged += work; Ok(()) }).unwrap();
            assert_eq!(wide.arena.nodes.len(), count + 1);
            for (ordinal, id) in wide.node().children.clone().enumerate() {
                assert!(matches!(wide.arena.nodes[id].detail, Detail::Literal(Literal::Number(value)) if value == ordinal as f64));
            }
            measured.push((charged, wide_charged));
            eprintln!("hir-projection count={count} deep_charge={charged} wide_charge={wide_charged}");
        }
        assert!(measured[1].0 < measured[0].0 * 5,
            "fourfold depth must not introduce quadratic path replay: {measured:?}");
        assert!(measured[1].1 < measured[0].1 * 5,
            "fourfold width must not introduce per-child prefix rescans: {measured:?}");
    }
    #[test]
    fn projection_every_charge_failure_releases_private_storage_and_retry_pays_full_work() {
        let root = Arc::new(Hir { source: None, span: 0..1, metadata: vec![], ty: hir::Type::String,
            kind: Expression::Literal(Literal::String(vec![65, 0xd800, 0xdc00, 0])) });
        let mut charges = Vec::new();
        let original = View::build(Owner::Shared(root.clone()), &mut |work| { charges.push(work); Ok(()) }).unwrap();
        drop(original);
        let strong = Arc::strong_count(&root);
        let Expression::Literal(Literal::String(original_units)) = &root.kind else { unreachable!() };
        let projected = View::build(Owner::Shared(root.clone()), &mut |_| Ok(())).unwrap();
        let Detail::Literal(Literal::String(projected_units)) = &projected.node().detail else { unreachable!() };
        assert_ne!(projected_units.as_ptr(), original_units.as_ptr(), "projection payload has one explicit paid UTF16 copy");
        let scalar_input = copy_literal_metered(match &projected.node().detail { Detail::Literal(value) => value, _ => unreachable!() }, &mut |_| Ok(())).unwrap();
        let Literal::String(scalar_units) = scalar_input else { unreachable!() };
        assert_ne!(scalar_units.as_ptr(), projected_units.as_ptr(), "scalar-input ownership is a separate paid copy");
        assert_eq!(scalar_units, *original_units);
        drop(projected);
        for fail in 0..charges.len() {
            for fuel in [false, true] {
                let mut calls = 0;
                let result = View::build(Owner::Shared(root.clone()), &mut |_| {
                    let failed = calls == fail; calls += 1;
                    if !failed { Ok(()) }
                    else if fuel { Err(SessionError::Trap(wasmtime::Error::new(wasmtime::Trap::OutOfFuel))) }
                    else { Err(SessionError::ForeignValue) }
                });
                match result {
                    Err(SessionError::ForeignValue) if !fuel => {},
                    Err(SessionError::Trap(error)) if fuel => assert_eq!(error.downcast_ref::<wasmtime::Trap>(), Some(&wasmtime::Trap::OutOfFuel)),
                    _ => panic!("typed error must survive projection failure at charge {fail}"),
                }
                assert_eq!(Arc::strong_count(&root), strong, "failed build publishes/retains no owner");
                let mut retry = Vec::new();
                let view = View::build(Owner::Shared(root.clone()), &mut |work| { retry.push(work); Ok(()) }).unwrap();
                assert_eq!(retry, charges);
                assert!(std::ptr::eq(view.retained_root(), root.as_ref()));
                assert!(matches!(&view.node().detail, Detail::Literal(Literal::String(units)) if units == &[65, 0xd800, 0xdc00, 0]));
            }
        }
    }
}
