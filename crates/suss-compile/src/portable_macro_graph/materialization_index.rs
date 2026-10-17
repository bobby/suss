//! Invocation-private scratch addressing. Keys are native recipe IDs, never
//! guest references. Membership is built before callbacks and then read only.
use crate::portable::metered_map::{Context, TryOrderedMap};
use std::{cmp::Ordering, mem};

pub(super) const SPARSE_THRESHOLD: usize = 4096;

pub(super) trait Index {
    const DENSE: bool;
    fn states<E>(&self, nodes: usize, charge: &mut impl FnMut(u64) -> Result<(), E>)
        -> Result<(Vec<u8>, Vec<bool>), E>;
    fn postorder<E>(&self, nodes: usize, charge: &mut impl FnMut(u64) -> Result<(), E>)
        -> Result<Vec<usize>, E>;
    fn enter<E>(&mut self, id: usize, states: &mut Vec<u8>, repeat: &mut Vec<bool>,
        charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<usize, E>;
    fn slot<E>(&self, id: usize, charge: &mut impl FnMut(u64) -> Result<(), E>)
        -> Result<usize, E>;
    fn push_postorder<E>(&self, values: &mut Vec<usize>, id: usize,
        charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<(), E>;
    fn any_repeat<E>(&self, ids: &[usize], repeat: &[bool],
        charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<bool, E> {
        for &id in ids {
            if repeat[self.slot(id, charge)?] { return Ok(true); }
        }
        Ok(false)
    }
}

pub(super) struct Dense;
impl Index for Dense {
    const DENSE: bool = true;
    fn states<E>(&self, nodes: usize, charge: &mut impl FnMut(u64) -> Result<(), E>)
        -> Result<(Vec<u8>, Vec<bool>), E> {
        charge(nodes.saturating_mul(3) as u64)?;
        Ok((vec![0; nodes], vec![false; nodes]))
    }
    fn postorder<E>(&self, nodes: usize, charge: &mut impl FnMut(u64) -> Result<(), E>)
        -> Result<Vec<usize>, E> {
        charge(nodes as u64)?;
        Ok(Vec::with_capacity(nodes))
    }
    fn enter<E>(&mut self, id: usize, _: &mut Vec<u8>, _: &mut Vec<bool>,
        _: &mut impl FnMut(u64) -> Result<(), E>) -> Result<usize, E> { Ok(id) }
    fn slot<E>(&self, id: usize, _: &mut impl FnMut(u64) -> Result<(), E>)
        -> Result<usize, E> { Ok(id) }
    fn push_postorder<E>(&self, values: &mut Vec<usize>, id: usize,
        _: &mut impl FnMut(u64) -> Result<(), E>) -> Result<(), E> {
        values.push(id);
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct Sparse {
    slots: TryOrderedMap<usize, usize>,
}
struct Meter<'a, C>(&'a mut C);
impl<E, C: FnMut(u64) -> Result<(), E>> Context<usize> for Meter<'_, C> {
    type Error = E;
    fn charge(&mut self, units: u64) -> Result<(), E> { (self.0)(units) }
    fn compare(&mut self, a: &usize, b: &usize) -> Result<Ordering, E> {
        (self.0)(1)?;
        Ok(a.cmp(b))
    }
}
fn reserve<T, E>(v: &mut Vec<T>, extra: usize,
    charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<(), E> {
    charge(0)?;
    if extra > v.capacity() - v.len() {
        let needed = v.len().checked_add(extra).expect("materialization scratch capacity overflow");
        let capacity = needed.max(v.capacity().saturating_mul(2)).max(4);
        charge((capacity as u64).saturating_add(v.len() as u64)
            .saturating_mul(mem::size_of::<T>() as u64))?;
        v.reserve_exact(capacity - v.len());
    }
    Ok(())
}
impl Index for Sparse {
    const DENSE: bool = false;
    fn states<E>(&self, _: usize, _: &mut impl FnMut(u64) -> Result<(), E>)
        -> Result<(Vec<u8>, Vec<bool>), E> {
        Ok((Vec::new(), Vec::new()))
    }
    fn postorder<E>(&self, _: usize, _: &mut impl FnMut(u64) -> Result<(), E>)
        -> Result<Vec<usize>, E> { Ok(Vec::new()) }
    fn enter<E>(&mut self, id: usize, states: &mut Vec<u8>, repeat: &mut Vec<bool>,
        charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<usize, E> {
        if let Some(slot) = self.slots.get(&id, &mut Meter(&mut *charge))? { return Ok(*slot); }
        debug_assert_eq!(states.len(), repeat.len());
        debug_assert_eq!(states.len(), self.slots.len());
        let slot = states.len();
        // Reserve and prepay both writes before publishing membership. An
        // interruption may grow private capacity, but cannot consume a slot.
        reserve(states, 1, charge)?;
        reserve(repeat, 1, charge)?;
        charge(2)?;
        self.slots.insert(id, slot, &mut Meter(&mut *charge))?;
        states.push(0);
        repeat.push(false);
        Ok(slot)
    }
    fn slot<E>(&self, id: usize, charge: &mut impl FnMut(u64) -> Result<(), E>)
        -> Result<usize, E> {
        Ok(*self.slots.get(&id, &mut Meter(&mut *charge))?
            .expect("recipe was not reached during materialization planning"))
    }
    fn push_postorder<E>(&self, values: &mut Vec<usize>, id: usize,
        charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<(), E> {
        reserve(values, 1, charge)?;
        charge(1)?;
        values.push(id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn unlimited(_: u64) -> Result<(), &'static str> { Ok(()) }

    #[test]
    fn sparse_slots_keep_exact_native_ids_without_gap_allocation() {
        let mut index = Sparse::default();
        let (mut states, mut repeat) = index.states(usize::MAX, &mut unlimited).unwrap();
        let ids = [usize::MAX, 0, 4096, 1, usize::MAX - 1];
        for (slot, id) in ids.into_iter().enumerate() {
            assert_eq!(index.enter(id, &mut states, &mut repeat, &mut unlimited), Ok(slot));
            states[slot] = 2;
            repeat[slot] = slot % 2 == 0;
        }
        for (slot, id) in ids.into_iter().enumerate().rev() {
            assert_eq!(index.enter(id, &mut states, &mut repeat, &mut unlimited), Ok(slot));
            assert_eq!(index.slot(id, &mut unlimited), Ok(slot));
            assert_eq!(states[slot], 2);
            assert_eq!(repeat[slot], slot % 2 == 0);
        }
        assert_eq!(states.len(), ids.len());
        assert_eq!(repeat.len(), ids.len());
        assert_eq!(index.slots.len(), ids.len());
        assert!(states.capacity() <= 8);
        let mut postorder = index.postorder(usize::MAX, &mut unlimited).unwrap();
        for id in ids { index.push_postorder(&mut postorder, id, &mut unlimited).unwrap(); }
        assert_eq!(postorder, ids, "postorder must retain global IDs");
    }

    #[test]
    fn every_sparse_insertion_fuel_and_irq_cut_preserves_membership_and_recovers() {
        fn setup() -> (Sparse, Vec<u8>, Vec<bool>) {
            let mut index = Sparse::default();
            let (mut states, mut repeat) = index.states(100_000, &mut unlimited).unwrap();
            for id in [8, 2, 32, 16] {
                index.enter(id, &mut states, &mut repeat, &mut unlimited).unwrap();
            }
            (index, states, repeat)
        }
        let (mut index, mut states, mut repeat) = setup();
        let (mut units, mut calls) = (0, 0);
        assert_eq!(index.enter(usize::MAX, &mut states, &mut repeat, &mut |n| {
            units += n; calls += 1; Ok::<_, &'static str>(())
        }), Ok(4));
        for allowance in 0..units {
            let (mut index, mut states, mut repeat) = setup();
            let mut remaining = allowance;
            assert_eq!(index.enter(usize::MAX, &mut states, &mut repeat, &mut |n| {
                remaining = remaining.checked_sub(n).ok_or("fuel")?;
                Ok(())
            }), Err("fuel"), "fuel {allowance}");
            assert_eq!(states.len(), 4);
            assert_eq!(repeat.len(), 4);
            assert_eq!(index.slots.len(), 4);
            assert_eq!(index.enter(usize::MAX, &mut states, &mut repeat, &mut unlimited), Ok(4));
            for (slot, id) in [8, 2, 32, 16, usize::MAX].into_iter().enumerate() {
                assert_eq!(index.slot(id, &mut unlimited), Ok(slot));
            }
        }
        for stop in 1..=calls {
            let (mut index, mut states, mut repeat) = setup();
            let mut call = 0;
            assert_eq!(index.enter(usize::MAX, &mut states, &mut repeat, &mut |_| {
                call += 1;
                if call == stop { Err("irq") } else { Ok(()) }
            }), Err("irq"));
            assert_eq!(states.len(), 4);
            assert_eq!(repeat.len(), 4);
            assert_eq!(index.slots.len(), 4);
            assert_eq!(index.enter(usize::MAX, &mut states, &mut repeat, &mut unlimited), Ok(4));
        }
    }
}
