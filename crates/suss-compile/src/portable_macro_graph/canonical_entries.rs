//! Append-only canonical entries. Stable IDs index fixed pages, never addresses.
//! Allocation keeps the parent's infallible Vec policy; overflow is explicit,
//! while allocator OOM is not newly recoverable through generic charge errors.
use super::{Entry, EntryStats};
const PAGE: usize = 128;
#[derive(Default)]
pub(super) struct Entries {
    pages: Vec<Vec<Entry>>,
    len: usize,
    capacity: usize,
}
#[derive(Clone, Copy)]
pub(super) struct Location {
    page: usize,
    offset: usize,
}
pub(super) struct Prepared {
    location: Location,
    index: usize,
    next_len: usize,
    next_capacity: usize,
    new_page: Option<Vec<Entry>>,
}
fn extent<T>(capacity: usize) -> usize {
    let bytes = capacity
        .checked_mul(std::mem::size_of::<T>())
        .expect("Canonical entry allocation overflow");
    assert!(
        bytes <= isize::MAX as usize,
        "Canonical entry allocation exceeds isize"
    );
    bytes
}
fn pay<E>(
    charge: &mut impl FnMut(u64) -> Result<(), E>,
    bucket: &mut u64,
    work: u64,
) -> Result<(), E> {
    charge(work)?;
    *bucket = bucket.saturating_add(work);
    Ok(())
}
fn grow<T, E>(
    v: &mut Vec<T>,
    capacity: usize,
    charge: &mut impl FnMut(u64) -> Result<(), E>,
    allocation: &mut u64,
    relocation: &mut u64,
) -> Result<(), E> {
    let allocate = extent::<T>(capacity) as u64;
    let relocate = extent::<T>(v.len()) as u64;
    charge(
        allocate
            .checked_add(relocate)
            .expect("Canonical entry charge overflow"),
    )?;
    *allocation = allocation.saturating_add(allocate);
    *relocation = relocation.saturating_add(relocate);
    v.reserve_exact(capacity - v.len());
    Ok(())
}
impl Entries {
    pub(super) fn len(&self) -> usize {
        self.len
    }
    pub(super) fn capacity(&self) -> usize {
        self.capacity
    }
    pub(super) fn get<E>(
        &self,
        index: usize,
        charge: &mut impl FnMut(u64) -> Result<(), E>,
        stats: &mut EntryStats,
    ) -> Result<&Entry, E> {
        pay(charge, &mut stats.access, 4)?; // page/offset, directory and entry read
        Ok(&self.pages[index / PAGE][index % PAGE])
    }
    pub(super) fn prepare_link<E>(
        &self,
        index: usize,
        charge: &mut impl FnMut(u64) -> Result<(), E>,
        stats: &mut EntryStats,
    ) -> Result<Location, E> {
        pay(charge, &mut stats.access, 6)?; // two offsets, directory/slot check, write prepayment
        let location = Location {
            page: index / PAGE,
            offset: index % PAGE,
        };
        assert!(index < self.len && location.offset < self.pages[location.page].len());
        Ok(location)
    }
    pub(super) fn prepare<E>(
        &mut self,
        charge: &mut impl FnMut(u64) -> Result<(), E>,
        stats: &mut EntryStats,
    ) -> Result<Prepared, E> {
        pay(charge, &mut stats.staging, 8)?; // length, checked advance, offsets, page selection
        let next_len = self
            .len
            .checked_add(1)
            .expect("Canonical entry length overflow");
        extent::<Entry>(next_len);
        let location = Location {
            page: self.len / PAGE,
            offset: self.len % PAGE,
        };
        let mut next_capacity = self.capacity;
        let new_page = if location.page == self.pages.len() {
            if self.pages.len() == self.pages.capacity() {
                pay(charge, &mut stats.staging, 4)?;
                let capacity = self
                    .pages
                    .capacity()
                    .checked_mul(2)
                    .expect("Canonical entry directory capacity overflow")
                    .max(4);
                grow(
                    &mut self.pages,
                    capacity,
                    charge,
                    &mut stats.directory_allocation,
                    &mut stats.directory_relocation,
                )?;
            }
            // Preserve small initial payload capacities. Subsequent fixed pages
            // never relocate entries from earlier pages.
            let capacity = if location.page == 0 { 4 } else { PAGE };
            next_capacity = next_capacity
                .checked_add(capacity)
                .expect("Canonical entry capacity overflow");
            extent::<Entry>(next_capacity);
            pay(
                charge,
                &mut stats.allocation,
                extent::<Entry>(capacity) as u64,
            )?;
            Some(Vec::with_capacity(capacity))
        } else {
            let page = &mut self.pages[location.page];
            if page.len() == page.capacity() {
                // Only page zero grows; every later page starts at PAGE.
                pay(charge, &mut stats.staging, 4)?;
                let capacity = page
                    .capacity()
                    .checked_mul(2)
                    .expect("Canonical entry page capacity overflow");
                assert!(capacity <= PAGE, "Canonical entry page overflow");
                next_capacity = next_capacity
                    .checked_add(capacity - page.capacity())
                    .expect("Canonical entry capacity overflow");
                extent::<Entry>(next_capacity);
                grow(
                    page,
                    capacity,
                    charge,
                    &mut stats.allocation,
                    &mut stats.relocation,
                )?;
                // This already-performed reserve may survive later failure,
                // just as Vec capacity did. It is not a logical entry publish.
                self.capacity = next_capacity;
            }
            None
        };
        // No fallible call is permitted after the head insertion succeeds.
        pay(charge, &mut stats.publication, 8)?; // directory/entry push and len/capacity publication
        Ok(Prepared {
            location,
            index: self.len,
            next_len,
            next_capacity,
            new_page,
        })
    }
    pub(super) fn publish(&mut self, prepared: Prepared, entry: Entry) {
        debug_assert_eq!(self.len, prepared.index);
        if let Some(page) = prepared.new_page {
            self.pages.push(page);
        }
        let page = &mut self.pages[prepared.location.page];
        debug_assert_eq!(page.len(), prepared.location.offset);
        debug_assert!(page.len() < page.capacity());
        page.push(entry);
        self.len = prepared.next_len;
        self.capacity = prepared.next_capacity;
    }
    pub(super) fn link(&mut self, location: Location, index: usize) {
        self.pages[location.page][location.offset].next = Some(index);
    }
    #[cfg(test)]
    pub(super) fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.pages.iter().flat_map(|page| page.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Debug, PartialEq, Eq)]
    enum Stop {
        Fuel,
        Irq,
    }
    fn entry(i: usize) -> Entry {
        Entry {
            key: (i as u64).to_le_bytes().to_vec(),
            next: i.checked_sub(1),
        }
    }
    fn primed(count: usize) -> Entries {
        let mut entries = Entries::default();
        let mut stats = EntryStats::default();
        for i in 0..count {
            let slot = entries
                .prepare(&mut |_| Ok::<_, Stop>(()), &mut stats)
                .unwrap();
            entries.publish(slot, entry(i));
        }
        entries
    }
    fn snapshot(entries: &Entries) -> Vec<(Vec<u8>, Option<usize>)> {
        entries.iter().map(|e| (e.key.clone(), e.next)).collect()
    }
    #[test]
    fn pages_preserve_all_ids_links_payloads_and_old_page_addresses() {
        let mut entries = Entries::default();
        let mut stats = EntryStats::default();
        let mut old_address = None;
        for i in 0..4097 {
            let slot = entries
                .prepare(&mut |_| Ok::<_, Stop>(()), &mut stats)
                .unwrap();
            entries.publish(slot, entry(i));
            if i == 127 {
                old_address = Some(
                    entries
                        .get(0, &mut |_| Ok::<_, Stop>(()), &mut stats)
                        .unwrap() as *const Entry as usize,
                );
            }
            if i >= 128 {
                assert_eq!(
                    entries
                        .get(0, &mut |_| Ok::<_, Stop>(()), &mut stats)
                        .unwrap() as *const Entry as usize,
                    old_address.unwrap()
                );
            }
        }
        for i in 0..4097 {
            let value = entries
                .get(i, &mut |_| Ok::<_, Stop>(()), &mut stats)
                .unwrap();
            assert_eq!(value.key, (i as u64).to_le_bytes());
            assert_eq!(value.next, i.checked_sub(1));
        }
        assert_eq!(entries.pages.len(), 33);
        assert_eq!(entries.capacity(), 33 * PAGE);
        assert_eq!(stats.allocation, extent::<Entry>(252 + 32 * PAGE) as u64);
        assert_eq!(stats.relocation, extent::<Entry>(124) as u64);
        assert_eq!(stats.directory_allocation, extent::<Vec<Entry>>(124) as u64);
        assert_eq!(stats.directory_relocation, extent::<Vec<Entry>>(60) as u64);
        assert_eq!(stats.publication, 4097 * 8);
    }
    #[test]
    fn every_prepare_fuel_irq_cut_preserves_logical_entries_and_retry() {
        for count in [0, 4, 128, 512] {
            let mut measured = primed(count);
            let mut work = 0;
            let mut calls = 0;
            let slot = measured
                .prepare(
                    &mut |n| {
                        work += n;
                        calls += 1;
                        Ok::<_, Stop>(())
                    },
                    &mut EntryStats::default(),
                )
                .unwrap();
            measured.publish(slot, entry(count));
            for fuel in 0..work {
                let mut entries = primed(count);
                let before = snapshot(&entries);
                let mut remaining = fuel;
                let result = entries.prepare(
                    &mut |n| {
                        remaining = remaining.checked_sub(n).ok_or(Stop::Fuel)?;
                        Ok(())
                    },
                    &mut EntryStats::default(),
                );
                assert!(
                    matches!(result, Err(Stop::Fuel)),
                    "seed={count} fuel={fuel}/{work}"
                );
                assert_eq!(entries.len(), count);
                assert_eq!(snapshot(&entries), before);
                let slot = entries
                    .prepare(&mut |_| Ok::<_, Stop>(()), &mut EntryStats::default())
                    .unwrap();
                entries.publish(slot, entry(count));
                assert_eq!(entries.len(), count + 1);
                assert_eq!(snapshot(&entries), snapshot(&measured));
            }
            for stop in 1..=calls {
                let mut entries = primed(count);
                let before = snapshot(&entries);
                let mut call = 0;
                let result = entries.prepare(
                    &mut |_| {
                        call += 1;
                        if call == stop { Err(Stop::Irq) } else { Ok(()) }
                    },
                    &mut EntryStats::default(),
                );
                assert!(matches!(result, Err(Stop::Irq)));
                assert_eq!(snapshot(&entries), before);
                let slot = entries
                    .prepare(&mut |_| Ok::<_, Stop>(()), &mut EntryStats::default())
                    .unwrap();
                entries.publish(slot, entry(count));
                assert_eq!(snapshot(&entries), snapshot(&measured));
            }
        }
    }
    #[test]
    fn native_overflow_is_explicit_before_any_entry_publication() {
        assert!(std::panic::catch_unwind(|| extent::<Entry>(usize::MAX)).is_err());
        assert!(std::panic::catch_unwind(|| extent::<Vec<Entry>>(usize::MAX)).is_err());
        let mut entries = Entries {
            pages: vec![],
            len: usize::MAX,
            capacity: 0,
        };
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || entries.prepare(&mut |_| Ok::<_, Stop>(()), &mut EntryStats::default())
            ))
            .is_err()
        );
        assert_eq!(entries.len(), usize::MAX);
        assert!(entries.pages.is_empty());
    }
}
