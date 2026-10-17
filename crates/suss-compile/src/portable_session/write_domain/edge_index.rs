//! One native-only validation pass. Hashes never survive an adapter, callback,
//! collection, or the enclosing RootScope. Membership is ALWAYS exact ref_eq.
use super::{Result, Session, SessionError, SessionValue, State};
use std::{hash::Hasher, mem::size_of, sync::atomic::AtomicBool};
use wasmtime::{AnyRef, AsContextMut, Rooted, StoreContextMut};

#[cfg(test)]
use std::sync::atomic::Ordering;

const INLINE: usize = 256;
const LINEAR: usize = 8;
const EMPTY: usize = 0;

pub(super) struct Meter<'a> {
    store: StoreContextMut<'a, ()>,
    interrupt: &'a AtomicBool,
    #[cfg(test)]
    cancel_after: Option<usize>,
    #[cfg(test)]
    force_collision: bool,
    #[cfg(test)]
    ticks: usize,
}
impl<'a> Meter<'a> {
    pub(super) fn new(store: StoreContextMut<'a, ()>, interrupt: &'a AtomicBool) -> Self {
        Self {
            store,
            interrupt,
            #[cfg(test)]
            cancel_after: None,
            #[cfg(test)]
            force_collision: false,
            #[cfg(test)]
            ticks: 0,
        }
    }
    fn work(&mut self, units: u64) -> Result<()> {
        #[cfg(test)]
        {
            self.ticks += 1;
        }
        #[cfg(test)]
        if let Some(left) = &mut self.cancel_after {
            if *left == 0 {
                self.interrupt.store(true, Ordering::SeqCst);
                self.cancel_after = None;
            } else {
                *left -= 1;
            }
        }
        Session::consume_host_work(self.store.as_context_mut(), units, self.interrupt)?;
        Ok(())
    }
    fn hash(&mut self, value: &Rooted<AnyRef>) -> Result<u64> {
        // Pinned Wasmtime49.0.1: try_gc_ref then derived Hash of transparent
        // NonZeroU32 VMGcRef. Prepay reference access, adapter write and finish.
        self.work(3)?;
        let mut hash = ReferenceWord::default();
        value.ref_hash(&self.store, &mut hash)?;
        let word = hash.finish();
        #[cfg(test)]
        let word = if self.force_collision { 0 } else { word };
        Ok(word)
    }
}
// No root-index hashing or raw pointer conversion. ref_hash supplies the actual
// GC reference's scalar word. Generic byte writes deliberately collide: exact
// comparisons remain correct if a future Hash implementation chooses that API.
#[derive(Default)]
struct ReferenceWord(u64);
impl Hasher for ReferenceWord {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        self.0 = bytes.len() as u64;
    }
    fn write_u32(&mut self, word: u32) {
        self.0 = u64::from(word);
    }
    fn write_u64(&mut self, word: u64) {
        self.0 = word;
    }
    fn write_usize(&mut self, word: usize) {
        self.0 = word as u64;
    }
}

enum Slots {
    Linear,
    Inline([usize; INLINE]),
    Heap(Vec<usize>),
}
impl Slots {
    fn len(&self) -> usize {
        match self {
            Self::Linear => 0,
            Self::Inline(_) => INLINE,
            Self::Heap(v) => v.len(),
        }
    }
    fn get(&self, index: usize) -> usize {
        match self {
            Self::Inline(v) => v[index],
            Self::Heap(v) => v[index],
            Self::Linear => unreachable!(),
        }
    }
    fn set(&mut self, index: usize, value: usize) {
        match self {
            Self::Inline(v) => v[index] = value,
            Self::Heap(v) => v[index] = value,
            Self::Linear => unreachable!(),
        }
    }
}

pub(super) struct Index<'a> {
    state: &'a State,
    owner: u64,
    slots: Slots,
    total: usize,
    shift: u32,
    ready: bool,
}
impl<'a> Index<'a> {
    // Construct an uninitialized logical index in its final caller stack slot;
    // prepare mutates by reference rather than returning a populated2KB array.
    pub(super) fn unprepared(state: &'a State, owner: u64) -> Self {
        Self {
            state,
            owner,
            slots: Slots::Linear,
            total: 0,
            shift: 0,
            ready: false,
        }
    }
    pub(super) fn prepare(&mut self, meter: &mut Meter<'_>) -> Result<()> {
        meter.work(4)?;
        if self.ready {
            return Err(invalid("Edge index already prepared"));
        }
        self.total = self
            .state
            .nodes
            .len()
            .checked_add(self.state.children.len())
            .ok_or_else(|| invalid("Edge root count overflow"))?;
        // No partially prepared index is usable, even if its caller catches an
        // error. State, roots, snapshots and receipts are never modified here.
        self.slots = if self.total <= LINEAR {
            meter.work(1)?;
            Slots::Linear
        } else if self.total <= INLINE / 2 {
            // Bounded stack initialization AND its handoff into the final field;
            // no fictitious heap allocation fee. Prepay cleanup metadata too.
            meter.work((2 * INLINE + 2) as u64)?;
            Slots::Inline([EMPTY; INLINE])
        } else {
            let capacity = self
                .total
                .checked_mul(2)
                .and_then(usize::checked_next_power_of_two)
                .ok_or_else(|| invalid("Edge index capacity overflow"))?;
            let bytes = capacity
                .checked_mul(size_of::<usize>())
                .filter(|bytes| *bytes <= isize::MAX as usize)
                .and_then(|bytes| u64::try_from(bytes).ok())
                .ok_or_else(|| invalid("Edge index allocation overflow"))?;
            meter.work(
                bytes
                    .checked_add(2)
                    .ok_or_else(|| invalid("Edge index debit overflow"))?,
            )?;
            let mut slots = Vec::new();
            slots
                .try_reserve_exact(capacity)
                .map_err(|_| invalid("Edge index allocation failed"))?;
            for _ in 0..capacity {
                meter.work(1)?;
                slots.push(EMPTY);
            }
            Slots::Heap(slots)
        };
        if self.slots.len() != 0 {
            meter.work(2)?;
            self.shift = 64_u32
                .checked_sub(self.slots.len().trailing_zeros())
                .ok_or_else(|| invalid("Edge index shift overflow"))?;
        }
        for id in 0..self.total {
            let value = self.root(id, meter)?;
            if self.slots.len() == 0 {
                continue;
            }
            meter.work(1)?;
            let reference = value.value.to_rooted(&mut meter.store);
            let (slot, found) = self.position(&reference, meter)?;
            if !found {
                meter.work(1)?;
                self.slots.set(
                    slot,
                    id.checked_add(1)
                        .ok_or_else(|| invalid("Edge ordinal overflow"))?,
                );
            }
        }
        meter.work(1)?;
        self.ready = true;
        Ok(())
    }
    fn root(&self, id: usize, meter: &mut Meter<'_>) -> Result<&'a SessionValue> {
        meter.work(1)?;
        let value = if id < self.state.nodes.len() {
            &self.state.nodes[id].value
        } else {
            &self
                .state
                .children
                .get(id - self.state.nodes.len())
                .ok_or_else(|| invalid("Edge root ordinal invalid"))?
                .value
        };
        if value.identity != self.owner {
            return Err(SessionError::ForeignValue);
        }
        Ok(value)
    }
    fn position(&self, reference: &Rooted<AnyRef>, meter: &mut Meter<'_>) -> Result<(usize, bool)> {
        let word = meter.hash(reference)?;
        // Multiplicative HIGH-bit placement distributes aligned32bit GC words;
        // no low-bit address/alignment assumption or cryptographic authority.
        meter.work(3)?;
        let mut slot = (word.wrapping_mul(0x9e3779b97f4a7c15) >> self.shift) as usize;
        for _ in 0..self.slots.len() {
            meter.work(2)?; // slot read and empty/occupied dispatch
            let ordinal = self.slots.get(slot);
            if ordinal == EMPTY {
                return Ok((slot, false));
            }
            let value = self.root(ordinal - 1, meter)?;
            meter.work(1)?; // same existing rooting/exact-reference comparison
            let candidate = value.value.to_rooted(&mut meter.store);
            if Rooted::<AnyRef>::ref_eq(&meter.store, reference, &candidate)? {
                return Ok((slot, true));
            }
            meter.work(1)?; // bounded increment and wrap
            slot = (slot + 1) & (self.slots.len() - 1);
        }
        Err(invalid("Edge index probe bound exceeded"))
    }
    pub(super) fn contains(&self, value: &SessionValue, meter: &mut Meter<'_>) -> Result<bool> {
        meter.work(2)?; // readiness and incoming ownership BEFORE lookup/fast hit
        if !self.ready {
            return Err(invalid("Incomplete edge index"));
        }
        if value.identity != self.owner {
            return Err(SessionError::ForeignValue);
        }
        meter.work(1)?;
        let reference = value.value.to_rooted(&mut meter.store);
        if reference.as_i31(&meter.store)?.is_some() {
            return Ok(true);
        }
        if self.slots.len() == 0 {
            for id in 0..self.total {
                let candidate = self.root(id, meter)?;
                meter.work(1)?;
                let candidate = candidate.value.to_rooted(&mut meter.store);
                if Rooted::<AnyRef>::ref_eq(&meter.store, &reference, &candidate)? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        Ok(self.position(&reference, meter)?.1)
    }
}
fn invalid(message: &'static str) -> SessionError {
    wasmtime::Error::msg(message).into()
}

#[cfg(test)]
mod tests {
    use super::super::{ChildDomain, Node, WriteDomain};
    use super::*;
    use crate::portable::hir::Literal;
    use wasmtime::RootScope;

    // These fixtures test the native membership primitive, NOT source authority:
    // complete factory/seed/child-domain authorization remains in the parent.
    fn roots(session: &mut Session, count: usize) -> State {
        let mut state = State {
            valid: true,
            nodes: Vec::new(),
            children: Vec::new(),
            sealed: false,
        };
        for i in 0..count {
            let value = session.data_scalar(&Literal::Number(i as f64)).unwrap();
            state.nodes.push(Node {
                value,
                snapshot: None,
                seed: None,
                edges: Vec::new(),
                watch: None,
            });
        }
        state
    }
    fn linear(
        state: &State,
        owner: u64,
        value: &SessionValue,
        meter: &mut Meter<'_>,
    ) -> Result<bool> {
        meter.work(1)?;
        if value.identity != owner {
            return Err(SessionError::ForeignValue);
        }
        let reference = value.value.to_rooted(&mut meter.store);
        if reference.as_i31(&meter.store)?.is_some() {
            return Ok(true);
        }
        for root in state
            .nodes
            .iter()
            .map(|n| &n.value)
            .chain(state.children.iter().map(|c| &c.value))
        {
            meter.work(1)?;
            let root = root.value.to_rooted(&mut meter.store);
            if Rooted::<AnyRef>::ref_eq(&meter.store, &reference, &root)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    #[test]
    fn exact_edge_index_inline_heap_aliases_children_collisions_match_linear_after_gc() {
        let mut session = Session::new_macro().unwrap();
        for count in [0, 1, 6, 7, 8, 9, 55, 120, 126, 127, 128, 129, 257] {
            let mut state = roots(&mut session, count);
            if let Some(node) = state.nodes.first() {
                state.children.push(ChildDomain {
                    value: node.value.clone(),
                    domain: WriteDomain::new(),
                });
            }
            let child = session.data_scalar(&Literal::Number(-1.0)).unwrap();
            state.children.push(ChildDomain {
                value: child.clone(),
                domain: WriteDomain::new(),
            });
            let missing = session.data_scalar(&Literal::Number(-1.0)).unwrap();
            session.collect().unwrap();
            for collision in [false, true] {
                let _request = session.operation_budget_scope().unwrap();
                let irq = session.host_work_interrupt();
                let owner = session.identity;
                let mut scope = RootScope::new(&mut session.store);
                let mut meter = Meter::new(scope.as_context_mut(), &irq);
                meter.force_collision = collision;
                let mut index = Index::unprepared(&state, owner);
                index.prepare(&mut meter).unwrap();
                assert_eq!(matches!(index.slots, Slots::Linear), index.total <= LINEAR);
                assert_eq!(
                    matches!(index.slots, Slots::Heap(_)),
                    index.total > INLINE / 2
                );
                for value in state
                    .nodes
                    .iter()
                    .map(|n| &n.value)
                    .chain([&child, &missing])
                {
                    let expected = linear(&state, owner, value, &mut meter).unwrap();
                    assert_eq!(index.contains(value, &mut meter).unwrap(), expected);
                }
                assert!(
                    !index.contains(&missing, &mut meter).unwrap(),
                    "equal contents are not exact membership"
                );
            }
        }
    }
    #[test]
    fn exact_edge_index_hashes_underlying_ref_and_rejects_foreign_before_hits() {
        let mut session = Session::new_macro().unwrap();
        for count in [12, 129] {
            let state = roots(&mut session, count);
            let mut foreign = Session::new_macro().unwrap();
            let foreign_number = foreign.data_scalar(&Literal::Number(0.0)).unwrap();
            let foreign_nil = foreign.data_scalar(&Literal::Nil).unwrap();
            let _request = session.operation_budget_scope().unwrap();
            let irq = session.host_work_interrupt();
            let mut scope = RootScope::new(&mut session.store);
            let mut meter = Meter::new(scope.as_context_mut(), &irq);
            let first = state.nodes[0].value.value.to_rooted(&mut meter.store);
            let again = state.nodes[0].value.value.to_rooted(&mut meter.store);
            assert!(Rooted::<AnyRef>::ref_eq(&meter.store, &first, &again).unwrap());
            assert_eq!(meter.hash(&first).unwrap(), meter.hash(&again).unwrap());
            meter.force_collision = true;
            let mut index = Index::unprepared(&state, state.nodes[0].value.identity);
            index.prepare(&mut meter).unwrap();
            for value in [&foreign_number, &foreign_nil] {
                assert!(matches!(
                    index.contains(value, &mut meter),
                    Err(SessionError::ForeignValue)
                ));
            }
            assert!(index.contains(&state.nodes[0].value, &mut meter).unwrap());
        }
    }
    fn attempt(
        session: &mut Session,
        state: &State,
        allowance: Option<u64>,
        cancel: Option<usize>,
    ) -> Result<(u64, usize)> {
        session.with_operation_budget(|session| {
            let start = session.operation_fuel_remaining()?;
            if let Some(allowance) = allowance {
                session.charge_host_work(start - allowance)?;
            }
            let before = session.operation_fuel_remaining()?;
            let irq = session.host_work_interrupt();
            let owner = session.identity;
            let mut scope = RootScope::new(&mut session.store);
            let mut meter = Meter::new(scope.as_context_mut(), &irq);
            // All collision semantics are exercised separately at every size;
            // sweep every cut on colliding inline and normal heap construction.
            meter.force_collision = state.nodes.len() <= INLINE / 2;
            meter.cancel_after = cancel;
            let mut index = Index::unprepared(state, owner);
            let prepared = index.prepare(&mut meter);
            if prepared.is_err() {
                assert!(!index.ready);
            }
            prepared?;
            assert!(index.contains(&state.nodes[0].value, &mut meter)?);
            let remaining = meter.store.get_fuel()?;
            Ok((before - remaining, meter.ticks))
        })
    }
    #[test]
    fn exact_edge_index_all_native_fuel_and_irq_cuts_drop_private_partial_state_and_recover() {
        let mut session = Session::new_macro().unwrap();
        // Inline and heap paths; each fault attempt is a separate native request,
        // not a refill inside a compound certificate or the unchanged cohort.
        for count in [9, 129] {
            let state = roots(&mut session, count);
            let handles = session.stats().external_value_handles;
            let (cost, ticks) = attempt(&mut session, &state, None, None).unwrap();
            for fuel in 0..cost {
                let error = attempt(&mut session, &state, Some(fuel), None).unwrap_err();
                assert!(matches!(error, SessionError::Trap(ref trap)
                    if trap.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
                assert_eq!(session.stats().external_value_handles, handles);
            }
            for cut in 0..ticks {
                let error = attempt(&mut session, &state, None, Some(cut)).unwrap_err();
                assert!(error.is_interrupt());
                assert_eq!(session.stats().external_value_handles, handles);
            }
            session.collect().unwrap();
            assert!(attempt(&mut session, &state, None, None).is_ok());
            assert_eq!(session.stats().external_value_handles, handles);
        }
    }
}
