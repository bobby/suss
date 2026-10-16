//! Metered native hash-to-chain heads for exact raw reader keys.
//! Native keys only. Generic charging errors are propagated unchanged.
//! Allocation follows parent reserve's infallible Vec policy: capacity overflow
//! panics explicitly; allocator OOM is NOT newly recoverable. To recover these
//! errors requires an explicit error-construction contract at raw_keys callers.
use super::reserve;

#[derive(Default)]
pub(super) struct RawHeads {
    slots: Vec<Option<(u64, usize)>>,
    len: usize, // DISTINCT full hashes, not raw key entries
}
impl RawHeads {
    fn checked_capacity(current: usize) -> usize {
        let capacity = if current == 0 { 4 } else {
            current.checked_mul(2).expect("Raw heads capacity overflow")
        };
        assert!(capacity.is_power_of_two(), "Raw heads invalid capacity");
        let bytes = capacity.checked_mul(std::mem::size_of::<Option<(u64, usize)>>())
            .expect("Raw heads allocation overflow");
        assert!(bytes <= isize::MAX as usize, "Raw heads allocation exceeds isize");
        capacity
    }
    fn allocate<E>(capacity: usize, charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<Self, E> {
        let mut slots = Vec::new();
        // Parent reserve charges requested capacity + previous live relocation.
        // Here previous live length is zero; the old table remains untouched.
        reserve(&mut slots, capacity, charge)?;
        for _ in 0..capacity {
            charge(1)?;
            slots.push(None);
        }
        Ok(Self { slots, len: 0 })
    }
    // Returns matching or empty slot. Tables are always <=half occupied.
    fn position<E>(&self, hash: u64, charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<Option<usize>, E> {
        if self.slots.is_empty() { charge(1)?; return Ok(None); }
        charge(3)?;
        let mask = self.slots.len() - 1;
        let mut index = hash as usize & mask;
        for _ in 0..self.slots.len() {
            charge(4)?; // slot read, full hash compare, probe/wrap work
            match self.slots[index] {
                None => return Ok(Some(index)),
                Some((key, _)) if key == hash => return Ok(Some(index)),
                _ => index = (index + 1) & mask,
            }
        }
        unreachable!("Raw heads half-load invariant violated")
    }
    pub(super) fn get<E>(&self, hash: u64, charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<Option<usize>, E> {
        let Some(index) = self.position(hash, charge)? else { return Ok(None); };
        charge(2)?;
        Ok(self.slots[index].map(|(_, head)| head))
    }
    pub(super) fn get_or_insert<E>(&mut self, hash: u64, head: usize,
        charge: &mut impl FnMut(u64) -> Result<(), E>,
    ) -> Result<usize, E> {
        let position = self.position(hash, charge)?;
        if let Some(index) = position {
            charge(2)?;
            if let Some((_, prior)) = self.slots[index] { return Ok(prior); }
        }
        // Only a new full hash can trigger growth. Head values never replace
        // existing heads; raw_keys maintains exact-byte Entry.next tail chains.
        charge(2)?; // occupancy/next-length decision
        let next_len = self.len.checked_add(1).expect("Raw heads length overflow");
        if self.slots.is_empty() || next_len > self.slots.len() / 2 {
            let capacity = Self::checked_capacity(self.slots.len());
            let mut staged = Self::allocate(capacity, charge)?;
            for slot in &self.slots {
                charge(1)?; // inspect EVERY old slot, including empties
                if let Some((key, value)) = *slot {
                    let index = staged.position(key, charge)?.expect("allocated table");
                    charge(3)?; // write tuple and update staged occupancy
                    staged.slots[index] = Some((key, value));
                    staged.len += 1;
                }
            }
            let index = staged.position(hash, charge)?.expect("allocated table");
            charge(3)?;
            staged.slots[index] = Some((hash, head));
            staged.len = next_len;
            charge(2)?; // final table/len publication, BEFORE swap
            std::mem::swap(self, &mut staged);
        } else {
            let index = position.expect("nonempty table");
            charge(3)?;
            self.slots[index] = Some((hash, head));
            self.len = next_len;
        }
        Ok(head)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Debug, PartialEq)]
    enum Stop { Fuel, Irq }
    fn unlimited(_: u64) -> Result<(), Stop> { Ok(()) }
    fn primed() -> RawHeads {
        let mut table = RawHeads::default();
        // Capacity4 at half occupancy. Hash3 and7 collide, wrapping at index3.
        assert_eq!(table.get_or_insert(3, 10, &mut unlimited), Ok(10));
        assert_eq!(table.get_or_insert(7, 20, &mut unlimited), Ok(20));
        assert_eq!(table.slots.len(), 4);
        table
    }
    #[test]
    fn placement_collisions_growth_existing_heads_and_absence() {
        let mut table = primed();
        let capacity = table.slots.len();
        assert_eq!(table.get_or_insert(3, 999, &mut unlimited), Ok(10));
        assert_eq!(table.slots.len(), capacity);
        for (key, value) in [(11, 30), (15, 40), (19, 50), (23, 60), (27, 70)] {
            assert_eq!(table.get_or_insert(key, value, &mut unlimited), Ok(value));
            assert!(table.len <= table.slots.len() / 2);
            for (key, value) in [(3, 10), (7, 20)] {
                assert_eq!(table.get(key, &mut unlimited), Ok(Some(value)));
            }
        }
        for (key, value) in [(11, 30), (15, 40), (19, 50), (23, 60), (27, 70)] {
            assert_eq!(table.get(key, &mut unlimited), Ok(Some(value)));
        }
        assert_eq!(table.get(31, &mut unlimited), Ok(None));
        assert_eq!(table.get(u64::MAX, &mut unlimited), Ok(None));
        assert_eq!(table.get_or_insert(u64::MAX, 80, &mut unlimited), Ok(80));
        assert_eq!(table.get_or_insert(0, 90, &mut unlimited), Ok(90));
        assert_eq!(table.get(0, &mut unlimited), Ok(Some(90)));
    }
    #[test]
    fn every_insufficient_growth_allowance_preserves_old_heads_then_recovers() {
        let mut measured = primed();
        let mut cost = 0u64;
        measured.get_or_insert(11, 30, &mut |n| { cost += n; Ok::<_, Stop>(()) }).unwrap();
        for allowance in 0..cost {
            let mut table = primed();
            let original = table.slots.clone();
            let old_len = table.len;
            let mut remaining = allowance;
            let result = table.get_or_insert(11, 30, &mut |n| {
                if remaining < n { return Err(Stop::Fuel); }
                remaining -= n;
                Ok(())
            });
            assert_eq!(result, Err(Stop::Fuel), "allowance {allowance}");
            assert_eq!(table.slots, original);
            assert_eq!(table.len, old_len);
            assert_eq!(table.get(11, &mut unlimited), Ok(None));
            assert_eq!(table.get_or_insert(11, 30, &mut unlimited), Ok(30));
            assert_eq!(table.get(3, &mut unlimited), Ok(Some(10)));
            assert_eq!(table.get(7, &mut unlimited), Ok(Some(20)));
            assert_eq!(table.get(11, &mut unlimited), Ok(Some(30)));
        }
    }
    #[test]
    fn every_growth_charge_irq_cut_preserves_old_heads_then_recovers() {
        let mut measured = primed();
        let mut calls = 0usize;
        measured.get_or_insert(11, 30, &mut |_| { calls += 1; Ok::<_, Stop>(()) }).unwrap();
        for cut in 0..calls {
            let mut table = primed();
            let original = table.slots.clone();
            let mut call = 0;
            let result = table.get_or_insert(11, 30, &mut |_| {
                let fail = call == cut; call += 1;
                if fail { Err(Stop::Irq) } else { Ok(()) }
            });
            assert_eq!(result, Err(Stop::Irq));
            assert_eq!(table.slots, original);
            assert_eq!(table.len, 2);
            assert_eq!(table.get_or_insert(11, 30, &mut unlimited), Ok(30));
        }
    }
    #[test]
    fn first_allocation_no_growth_and_hit_preserve_heads_at_every_stop() {
        fn setup(mode: usize) -> RawHeads {
            let mut table = RawHeads::default();
            if mode > 0 { table.get_or_insert(3, 10, &mut unlimited).unwrap(); }
            table
        }
        for (mode, key, expected) in [(0, 3, 99), (1, 7, 99), (2, 3, 10)] {
            let mut table = setup(mode);
            let mut cost = 0u64;
            let mut calls = 0usize;
            assert_eq!(table.get_or_insert(key, 99, &mut |n| {
                cost += n; calls += 1; Ok::<_, Stop>(())
            }), Ok(expected));
            for allowance in 0..cost {
                let mut table = setup(mode);
                let original = table.slots.clone();
                let old_len = table.len;
                let mut remaining = allowance;
                assert_eq!(table.get_or_insert(key, 99, &mut |n| {
                    if remaining < n { return Err(Stop::Fuel); }
                    remaining -= n; Ok(())
                }), Err(Stop::Fuel), "mode {mode}, allowance {allowance}");
                assert_eq!(table.slots, original);
                assert_eq!(table.len, old_len);
                assert_eq!(table.get_or_insert(key, 99, &mut unlimited), Ok(expected));
                if mode > 0 { assert_eq!(table.get(3, &mut unlimited), Ok(Some(10))); }
            }
            for cut in 0..calls {
                let mut table = setup(mode);
                let original = table.slots.clone();
                let old_len = table.len;
                let mut call = 0;
                assert_eq!(table.get_or_insert(key, 99, &mut |_| {
                    let stop = call == cut; call += 1;
                    if stop { Err(Stop::Irq) } else { Ok(()) }
                }), Err(Stop::Irq));
                assert_eq!(table.slots, original);
                assert_eq!(table.len, old_len);
                assert_eq!(table.get_or_insert(key, 99, &mut unlimited), Ok(expected));
            }
        }
    }
    #[test]
    fn checked_capacity_rejects_native_layout_overflow() {
        assert_eq!(RawHeads::checked_capacity(0), 4);
        assert!(std::panic::catch_unwind(|| RawHeads::checked_capacity(usize::MAX)).is_err());
        let largest_power = 1usize << (usize::BITS - 2);
        assert!(std::panic::catch_unwind(|| RawHeads::checked_capacity(largest_power)).is_err());
    }
}
