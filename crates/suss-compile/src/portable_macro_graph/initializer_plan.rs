//! Invocation-local correspondence, never an identity inferred from spans alone.
use super::Result;
use crate::portable_session::{Session, SessionError};
use suss_reader::forms::{Form, Kind};

/// Grow a work stack with its allocation charged before it happens.
fn reserve<T>(session: &mut Session, values: &mut Vec<T>, extra: usize) -> Result<()> {
    session.charge_host_work(1)?;
    if extra > values.capacity() - values.len() {
        let needed = values.len().checked_add(extra).ok_or_else(|| invalid("Initializer plan allocation overflow"))?;
        let cap = values.capacity().checked_mul(2).unwrap_or(needed).max(needed);
        let bytes = cap
            .checked_add(values.len())
            .and_then(|n| n.checked_mul(std::mem::size_of::<T>()))
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| invalid("Initializer plan allocation overflow"))?;
        session.charge_host_work(bytes)?;
        values.try_reserve_exact(cap - values.len()).map_err(|_| invalid("Initializer plan allocation failed"))?;
    }
    Ok(())
}
fn invalid(message: &'static str) -> SessionError {
    wasmtime::Error::msg(message).into()
}
fn text<T: PartialEq>(session: &mut Session, a: &[T], b: &[T]) -> Result<bool> {
    session.charge_host_work(1)?;
    if a.len() != b.len() { return Ok(false); }
    for (a, b) in a.chunks(256).zip(b.chunks(256)) {
        session.charge_host_work((a.len() + b.len()) as u64)?;
        if a != b { return Ok(false); }
    }
    Ok(true)
}
fn namespace(session: &mut Session, a: &Option<String>, b: &Option<String>) -> Result<bool> {
    session.charge_host_work(1)?;
    match (a, b) {
        (None, None) => Ok(true),
        (Some(a), Some(b)) => text(session, a.as_bytes(), b.as_bytes()),
        _ => Ok(false),
    }
}
/// Strictly stronger than raw-key equality: includes every span and raw metadata
/// prefix, distinguishes kinds and exact numeric bits, and never calls guests.
pub(super) fn same(session: &mut Session, a: &Form, b: &Form) -> Result<bool> {
    fn push<'a, 'b>(session: &mut Session, pending: &mut Vec<(&'a Form, &'b Form)>,
        a: &'a Form, b: &'b Form) -> Result<()> {
        reserve(session, pending, 1)?;
        session.charge_host_work(3)?;
        pending.push((a, b));
        Ok(())
    }
    let mut pending = Vec::new();
    push(session, &mut pending, a, b)?;
    while let Some((a, b)) = pending.pop() {
        session.charge_host_work(8)?;
        if a.span != b.span || a.metadata.len() != b.metadata.len() { return Ok(false); }
        match (&a.kind, &b.kind) {
            (Kind::Nil, Kind::Nil) => {}
            (Kind::Bool(a), Kind::Bool(b)) if a == b => {}
            (Kind::Number(a), Kind::Number(b)) if a.to_bits() == b.to_bits() => {}
            (Kind::String(a), Kind::String(b)) => {
                if !text(session, a, b)? { return Ok(false); }
            }
            (Kind::Symbol(a), Kind::Symbol(b)) => {
                if !namespace(session, &a.namespace, &b.namespace)?
                    || !text(session, a.name.as_bytes(), b.name.as_bytes())? { return Ok(false); }
            }
            (Kind::Keyword(a), Kind::Keyword(b)) => {
                if !namespace(session, &a.namespace, &b.namespace)?
                    || !text(session, a.name.as_bytes(), b.name.as_bytes())? { return Ok(false); }
            }
            (Kind::List(a), Kind::List(b)) | (Kind::Vector(a), Kind::Vector(b))
            | (Kind::Map(a), Kind::Map(b)) | (Kind::Set(a), Kind::Set(b))
            | (Kind::Conditional(a), Kind::Conditional(b))
            | (Kind::SplicingConditional(a), Kind::SplicingConditional(b)) => {
                if a.len() != b.len() { return Ok(false); }
                for (a, b) in a.iter().zip(b).rev() { push(session, &mut pending, a, b)?; }
            }
            (Kind::Discard(a), Kind::Discard(b)) => push(session, &mut pending, a, b)?,
            (Kind::Prefix { operator: ao, target: at }, Kind::Prefix { operator: bo, target: bt }) => {
                push(session, &mut pending, at, bt)?;
                push(session, &mut pending, ao, bo)?;
            }
            _ => return Ok(false),
        }
        for (a, b) in a.metadata.iter().zip(&b.metadata).rev() {
            push(session, &mut pending, a, b)?;
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::AnalysisGraph;
    use crate::{portable_macro_data::FormBridge, portable_session::SessionError};
    fn form(kind: Kind) -> Form { Form { span: 10..20, metadata: vec![], kind } }
    fn read(text: &str) -> Form {
        suss_reader::forms::read_forms(text).unwrap().pop().unwrap()
    }
    #[test]
    fn initializer_correspondence_checks_every_metadata_span_kind_and_number_bit() {
        let mut session = Session::new_macro().unwrap();
        let a = read("^{:outer [1 2]} [foo/bar {:a #{17 18}}]");
        assert!(same(&mut session, &a, &a.clone()).unwrap());
        let mut b = a.clone(); b.span.end += 1;
        assert!(!same(&mut session, &a, &b).unwrap());
        let mut b = a.clone(); b.metadata[0].span.start += 1;
        assert!(!same(&mut session, &a, &b).unwrap());
        let mut b = a.clone(); b.metadata[0] = read("{:outer [1 3]}");
        assert!(!same(&mut session, &a, &b).unwrap());
        let mut b = a.clone();
        let Kind::Vector(v) = &mut b.kind else { unreachable!() };
        v[0].kind = Kind::Symbol(suss_reader::Symbol::namespaced("other", "bar"));
        assert!(!same(&mut session, &a, &b).unwrap());
        for (a, b, expected) in [
            (0, 1u64 << 63, false),
            (0x7ff8_0000_0000_0001, 0x7ff8_0000_0000_0001, true),
            (0x7ff8_0000_0000_0001, 0x7ff8_0000_0000_0002, false),
        ] {
            assert_eq!(same(&mut session, &form(Kind::Number(f64::from_bits(a))),
                &form(Kind::Number(f64::from_bits(b)))).unwrap(), expected);
        }
        let a = form(Kind::String(vec![0xd800, 0, 0xdc00]));
        assert!(same(&mut session, &a, &a.clone()).unwrap());
        assert!(!same(&mut session, &a, &form(Kind::String(vec![0xd801, 0, 0xdc00]))).unwrap());
        assert!(!same(&mut session, &read("[1 2]"), &read("(1 2)")).unwrap());
    }
    #[test]
    fn initializer_recipe_reuses_exact_child_with_root_metadata_and_declines_other_inputs() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let definition = read("^{:root 7} (def target ^{:child 8} [17 {:a #{1 2}}])");
        let Kind::List(items) = &definition.kind else { unreachable!() };
        let initializer = items.last().unwrap().clone();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let (_, selected) = graph.definition_with_initializer(&definition, Some(&initializer), 0).unwrap();
        assert_eq!(selected, Some(graph.form(&initializer, 0).unwrap()));
        let mut other = initializer.clone(); other.span.end += 1;
        assert!(graph.definition_with_initializer(&definition, Some(&other), 0).unwrap().1.is_none());
        // Span mismatch conservatively falls back although raw keys ignore spans.
        assert_eq!(graph.form(&other, 0).unwrap(), selected.unwrap());
        let mut other = initializer.clone(); other.metadata[0] = read("{:child 9}");
        assert!(graph.definition_with_initializer(&definition, Some(&other), 0).unwrap().1.is_none());
        assert_ne!(graph.form(&other, 0).unwrap(), selected.unwrap());
        let bad = form(Kind::Map(vec![form(Kind::Nil)]));
        let malformed = form(Kind::List(vec![read("def"), read("bad"), bad.clone()]));
        assert!(graph.definition_with_initializer(&malformed, Some(&bad), 0).is_err());
        assert!(graph.definition_with_initializer(&definition, Some(&bad), 0).unwrap().1.is_none());
        assert!(graph.form(&bad, 0).is_err());
        assert!(graph.definition_with_initializer(&read("nil"), Some(&initializer), 0).unwrap().1.is_none());
        // Force a genuine root-key hit without its descendant recipe. The
        // selected key is present in the Plan, but must NOT authorize a value.
        let child_key = graph.reader_key(&initializer).unwrap();
        graph.forms[child_key.0] = None;
        assert!(graph.definition_with_initializer(&definition, Some(&initializer), 0).unwrap().1.is_none());
        // Exactly the production fallback: reimport the initializer normally.
        graph.form(&initializer, 0).unwrap();
    }
    #[test]
    fn initializer_plan_reuse_saves_actual_work_and_recovers_with_same_allowance() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let initializer = form(Kind::Vector(vec![form(Kind::String(vec![0xd800; 65536]))]));
        let definition = form(Kind::List(vec![read("def"), read("large"), initializer.clone()]));
        let measure = |session: &mut Session, reuse: bool| {
            session.with_operation_budget(|session| {
                let before = session.operation_fuel_remaining()?;
                let mut graph = AnalysisGraph::new(&bridge, session);
                if reuse {
                    let (_, child) = graph.definition_with_initializer(&definition, Some(&initializer), 0)?;
                    assert!(child.is_some());
                } else {
                    graph.form(&definition, 0)?;
                    graph.form(&initializer, 0)?;
                }
                Ok(before - graph.session.operation_fuel_remaining()?)
            })
        };
        let optimized = measure(&mut session, true).unwrap();
        let legacy = measure(&mut session, false).unwrap();
        assert!(optimized < legacy, "actual charged work: optimized={optimized} legacy={legacy}");
        session.set_operation_fuel(optimized);
        assert_eq!(measure(&mut session, true).unwrap(), optimized);
        let error = measure(&mut session, false).unwrap_err();
        assert!(matches!(error, SessionError::Trap(ref e)
            if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{error:?}");
        assert_eq!(measure(&mut session, true).unwrap(), optimized);
        let irq = session.host_work_interrupt();
        let error = session.with_operation_budget(|session| {
            irq.store(true, std::sync::atomic::Ordering::SeqCst);
            let mut graph = AnalysisGraph::new(&bridge, session);
            graph.definition_with_initializer(&definition, Some(&initializer), 0)
        }).unwrap_err();
        assert!(error.is_interrupt());
        assert_eq!(measure(&mut session, true).unwrap(), optimized);
        eprintln!("initializer-plan-cost optimized={optimized} legacy={legacy} same-allowance={optimized}");
    }
}
