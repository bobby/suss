//! Private, callback-free placeholder fixups for unexposed compiler-built data.
//!
//! This is NOT source certification or arbitrary object repair. The caller must
//! prove each placeholder is a unique, private source-array factory result, was
//! not reachable before this build, and was never passed to hash/equality or any
//! other guest callback. Every staged write must target fresh, unexposed mutable
//! parent storage. Shared seed storage may be scanned read-only; scanning grants
//! no write authority. Retained metadata roots are opaque replacements (cut).
//! This helper cannot establish those preconditions.
//!
//! Only canonical OBJECT.field1 and canonical ARGS elements are followed. Other
//! types and descriptor/properties/UID edges are leaves. Exact slots are staged
//! read-only first. A write-phase failure may leave PARTIAL fixups: discard the
//! fresh parent graph. No rollback, retained-root publication or guest execution
//! occurs here. Cycle termination is not finite-syntax validation.
//!
//! Placeholder/target indexes use ephemeral ref_hash buckets plus exact ref_eq
//! collision chains. Normal binding preparation costs O(P log P), and each
//! scanned edge uses O(log P) binding lookup, plus actual collision comparisons.
//! Adversarial/all-forced collisions can still be quadratic, always metered.
//! No source quotas or default allowances are changed by this helper.
use super::{Session, SessionError, SessionValue};
use crate::portable::metered_map::{Context, TryOrderedMap};
use std::{cmp::Ordering, collections::hash_map::DefaultHasher, hash::Hasher, mem};
use wasmtime::{
    AnyRef, ArrayRef, ArrayType, AsContextMut, RootScope, Rooted, StoreContextMut, StructType, Val,
};

type Result<T> = std::result::Result<T, SessionError>;
fn invalid(message: &'static str) -> SessionError {
    wasmtime::Error::msg(message).into()
}
struct Meter<'a> {
    store: StoreContextMut<'a, ()>,
    interrupt: &'a std::sync::atomic::AtomicBool,
}
impl Context<u64> for Meter<'_> {
    type Error = SessionError;
    fn charge(&mut self, units: u64) -> Result<()> {
        Session::consume_host_work(self.store.as_context_mut(), units, self.interrupt)
            .map_err(Into::into)
    }
    fn compare(&mut self, left: &u64, right: &u64) -> Result<Ordering> {
        self.charge(1)?;
        Ok(left.cmp(right))
    }
}
impl Meter<'_> {
    // Charge the requested backing plus maximal live relocation before reserve.
    fn reserve<T>(&mut self, values: &mut Vec<T>, additional: usize) -> Result<()> {
        self.charge(1)?;
        let needed = values
            .len()
            .checked_add(additional)
            .ok_or_else(|| invalid("Private fixup buffer length overflow"))?;
        if needed <= values.capacity() {
            return Ok(());
        }
        let capacity = values
            .capacity()
            .checked_mul(2)
            .unwrap_or(needed)
            .max(needed);
        let bytes = capacity
            .checked_add(values.len())
            .and_then(|n| n.checked_mul(mem::size_of::<T>()))
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| invalid("Private fixup allocation overflow"))?;
        self.charge(bytes)?;
        values
            .try_reserve_exact(capacity - values.len())
            .map_err(|_| invalid("Private fixup allocation failed"))?;
        Ok(())
    }
    fn hash(&mut self, value: &Rooted<AnyRef>, force_collision: bool) -> Result<u64> {
        self.charge(1)?;
        let mut hash = DefaultHasher::new();
        value.ref_hash(&self.store, &mut hash)?;
        Ok(if force_collision { 0 } else { hash.finish() })
    }
    fn same(&mut self, left: &Rooted<AnyRef>, right: &Rooted<AnyRef>) -> Result<bool> {
        self.charge(1)?;
        Ok(Rooted::<AnyRef>::ref_eq(&self.store, left, right)?)
    }
}
struct Pair {
    placeholder: Rooted<AnyRef>,
    replacement: Rooted<AnyRef>,
    placeholder_hash: u64,
    replacement_hash: u64,
}
#[derive(Clone, Copy)]
struct Location {
    array: Rooted<ArrayRef>,
    index: u32,
}
struct Pending {
    value: Rooted<AnyRef>,
    location: Option<Location>,
}
struct Seen {
    value: Rooted<AnyRef>,
    collision: Option<usize>,
}
struct Write {
    location: Location,
    pair: usize,
}

struct IdentityEntry {
    value: Rooted<AnyRef>,
    binding: usize,
    collision: Option<usize>,
}
#[derive(Default)]
struct IdentityIndex {
    heads: TryOrderedMap<u64, usize>,
    entries: Vec<IdentityEntry>,
}
impl IdentityIndex {
    fn lookup(
        &self,
        hash: u64,
        value: &Rooted<AnyRef>,
        meter: &mut Meter<'_>,
    ) -> Result<Option<usize>> {
        let mut cursor = self.heads.get(&hash, meter)?.copied();
        while let Some(index) = cursor {
            let entry = &self.entries[index];
            if meter.same(value, &entry.value)? {
                return Ok(Some(entry.binding));
            }
            cursor = entry.collision;
        }
        Ok(None)
    }
    // Single ordered-map search. Duplicate identities retain their first
    // binding; replacement aliases are allowed, placeholder aliases are not.
    fn insert(
        &mut self,
        hash: u64,
        value: Rooted<AnyRef>,
        binding: usize,
        meter: &mut Meter<'_>,
    ) -> Result<bool> {
        // Capacity must precede map publication. Reserve amortizes geometrically;
        // duplicate insertion does not append an entry or replace its binding.
        meter.reserve(&mut self.entries, 1)?;
        let next = self.entries.len();
        let (head, inserted) = self.heads.get_or_insert(hash, next, meter)?;
        let head = *head;
        let mut previous = None;
        if !inserted {
            let mut cursor = head;
            loop {
                let entry = &self.entries[cursor];
                if meter.same(&value, &entry.value)? {
                    return Ok(false);
                }
                match entry.collision {
                    Some(index) => cursor = index,
                    None => {
                        previous = Some(cursor);
                        break;
                    }
                }
            }
        }
        // All fallible work finished. Hash heads and links are invocation-local.
        self.entries.push(IdentityEntry {
            value,
            binding,
            collision: None,
        });
        if let Some(previous) = previous {
            self.entries[previous].collision = Some(next);
        }
        Ok(true)
    }
}

impl Session {
    /// Replace exact private placeholder identities in a fresh, unexposed graph.
    /// `pairs[0].0` supplies canonical OBJECT/ARGS types; callers must supply an
    /// actual private source-array factory result, not a shape-compatible value.
    /// Placeholders must be distinct and disjoint from all replacement roots.
    /// The root itself must not be a placeholder (there is no ARGS slot to fix).
    /// Returns unique physical slots written, not logical occurrence count.
    /// See module preconditions; this API neither validates nor repairs sources.
    pub(crate) fn fixup_private_placeholders(
        &mut self,
        root: &SessionValue,
        pairs: &[(&SessionValue, &SessionValue)],
    ) -> Result<usize> {
        self.fixup_private_placeholders_impl(root, pairs, false)
    }
    fn fixup_private_placeholders_impl(
        &mut self,
        root: &SessionValue,
        pairs: &[(&SessionValue, &SessionValue)],
        force_collision: bool,
    ) -> Result<usize> {
        self.fixup_private_placeholders_cut_impl(root, pairs, &[], force_collision)
    }
    /// Cuts are independently proven placeholder-free by the controlled builder.
    /// This API trusts that provenance; it does not certify arbitrary cut roots.
    pub(crate) fn fixup_private_placeholders_with_cuts(
        &mut self, root: &SessionValue,
        pairs: &[(&SessionValue, &SessionValue)], cuts: &[SessionValue],
    ) -> Result<usize> {
        self.fixup_private_placeholders_cut_impl(root, pairs, cuts, false)
    }
    fn fixup_private_placeholders_cut_impl(
        &mut self, root: &SessionValue,
        pairs: &[(&SessionValue, &SessionValue)], cuts: &[SessionValue],
        force_collision: bool,
    ) -> Result<usize> {
        let _request = self.operation_budget_scope()?;
        self.charge_host_work(1)?;
        self.check(root)?;
        for (placeholder, replacement) in pairs {
            self.charge_host_work(1)?;
            self.check(placeholder)?;
            self.check(replacement)?;
        }
        for cut in cuts {
            self.charge_host_work(1)?;
            self.check(cut)?;
        }
        if pairs.is_empty() {
            return Ok(0);
        }
        let interrupt = self.host_work_interrupt();
        // One root scope; no guest calls, guest allocation or GC below. Native
        // fallible buffers are charged. Identity hashes die with this invocation.
        let mut scope = RootScope::new(&mut self.store);
        let root = root.value.to_rooted(&mut scope);
        let mut meter = Meter {
            store: scope.as_context_mut(),
            interrupt: &interrupt,
        };
        let mut bindings = Vec::new();
        meter.reserve(&mut bindings, pairs.len())?;
        for (placeholder, replacement) in pairs {
            meter.charge(2)?;
            let placeholder = placeholder.value.to_rooted(&mut meter.store);
            let replacement = replacement.value.to_rooted(&mut meter.store);
            let placeholder_hash = meter.hash(&placeholder, force_collision)?;
            let replacement_hash = meter.hash(&replacement, force_collision)?;
            bindings.push(Pair {
                placeholder,
                replacement,
                placeholder_hash,
                replacement_hash,
            });
        }
        meter.charge(4)?;
        let sample = bindings[0]
            .placeholder
            .as_struct(&meter.store)?
            .ok_or_else(|| invalid("Private placeholder needs source-array OBJECT"))?;
        let object_type = sample.ty(&meter.store)?;
        let storage = sample.field(&mut meter.store, 1)?;
        let storage = storage
            .anyref()
            .flatten()
            .ok_or_else(|| invalid("Private placeholder needs ARGS storage"))?
            .as_array(&meter.store)?
            .ok_or_else(|| invalid("Private placeholder needs ARGS storage"))?;
        let args_type = storage.ty(&meter.store)?;
        let mut placeholders = IdentityIndex::default();
        let mut replacements = IdentityIndex::default();
        for (id, binding) in bindings.iter().enumerate() {
            meter.charge(1)?;
            let object = binding
                .placeholder
                .as_struct(&meter.store)?
                .ok_or_else(|| invalid("Private placeholder is not an OBJECT"))?;
            if !StructType::eq(&object.ty(&meter.store)?, &object_type) {
                return Err(invalid("Private placeholders have different OBJECT types"));
            }
            if !placeholders.insert(
                binding.placeholder_hash,
                binding.placeholder,
                id,
                &mut meter,
            )? {
                return Err(invalid("Duplicate private placeholder"));
            }
            replacements.insert(
                binding.replacement_hash,
                binding.replacement,
                id,
                &mut meter,
            )?;
        }
        // Both complete indexes precede cross-set checks, so overlap in either
        // binding order is rejected without pairwise scans. Shared targets dedup.
        for binding in &bindings {
            if replacements
                .lookup(binding.placeholder_hash, &binding.placeholder, &mut meter)?
                .is_some()
            {
                return Err(invalid("Private placeholder aliases a replacement"));
            }
        }
        for cut in cuts {
            let cut = cut.value.to_rooted(&mut meter.store);
            let hash = meter.hash(&cut, force_collision)?;
            if placeholders.lookup(hash, &cut, &mut meter)?.is_some() {
                return Err(invalid("Private placeholder aliases an opaque cut"));
            }
            replacements.insert(hash, cut, 0, &mut meter)?;
        }
        let root_hash = meter.hash(&root, force_collision)?;
        if placeholders.lookup(root_hash, &root, &mut meter)?.is_some() {
            return Err(invalid("Private fixup root cannot be a placeholder"));
        }
        let mut pending = Vec::new();
        meter.reserve(&mut pending, 1)?;
        pending.push(Pending {
            value: root,
            location: None,
        });
        let mut index = TryOrderedMap::<u64, usize>::default();
        let mut seen: Vec<Seen> = Vec::new();
        let mut writes = Vec::new();
        while let Some(item) = pending.pop() {
            meter.charge(1)?;
            // Match placeholders before visitation; their private contents are
            // opaque. Never follow a selected real root, even already in parent.
            let hash = meter.hash(&item.value, force_collision)?;
            let matched = placeholders.lookup(hash, &item.value, &mut meter)?;
            if let Some(pair) = matched {
                let location = item
                    .location
                    .ok_or_else(|| invalid("Placeholder has no mutable ARGS slot"))?;
                meter.reserve(&mut writes, 1)?;
                writes.push(Write { location, pair });
                continue;
            }
            if replacements
                .lookup(hash, &item.value, &mut meter)?
                .is_some()
            {
                continue;
            }
            meter.charge(2)?;
            let object = item.value.as_struct(&meter.store)?;
            let object = match object {
                Some(object) if StructType::eq(&object.ty(&meter.store)?, &object_type) => {
                    Some(object)
                }
                _ => None,
            };
            let array = item.value.as_array(&meter.store)?;
            let array = match array {
                Some(array) if ArrayType::eq(&array.ty(&meter.store)?, &args_type) => Some(array),
                _ => None,
            };
            if object.is_none() && array.is_none() {
                continue;
            }
            let head = index.get(&hash, &mut meter)?.copied();
            let mut cursor = head;
            let mut duplicate = false;
            while let Some(id) = cursor {
                if meter.same(&item.value, &seen[id].value)? {
                    duplicate = true;
                    break;
                }
                cursor = seen[id].collision;
            }
            if duplicate {
                continue;
            }
            meter.reserve(&mut seen, 1)?;
            let id = seen.len();
            seen.push(Seen {
                value: item.value,
                collision: head,
            });
            index.insert(hash, id, &mut meter)?;
            if let Some(object) = object {
                meter.charge(1)?;
                let storage = object.field(&mut meter.store, 1)?;
                let value = storage
                    .anyref()
                    .flatten()
                    .copied()
                    .ok_or_else(|| invalid("Fresh OBJECT has null field storage"))?;
                let array = value
                    .as_array(&meter.store)?
                    .ok_or_else(|| invalid("Fresh OBJECT has non-ARGS field storage"))?;
                if !ArrayType::eq(&array.ty(&meter.store)?, &args_type) {
                    return Err(invalid("Fresh OBJECT has foreign field storage"));
                }
                meter.reserve(&mut pending, 1)?;
                pending.push(Pending {
                    value,
                    location: None,
                });
            }
            if let Some(array) = array {
                meter.charge(1)?;
                let length = array.len(&meter.store)?;
                // Each element is inspected once; shared arrays are visited once.
                // No reserve proportional to a hypothetical unfolded occurrence.
                for index in 0..length {
                    meter.charge(1)?;
                    if let Val::AnyRef(Some(value)) = array.get(&mut meter.store, index)? {
                        meter.reserve(&mut pending, 1)?;
                        pending.push(Pending {
                            value,
                            location: Some(Location { array, index }),
                        });
                    }
                }
            }
        }
        // All type/ownership/read checks and native allocation precede writes.
        // Only fuel/IRQ/native set failure can interrupt this final fresh-graph
        // phase. Selected roots are read-only and were never walked.
        for write in &writes {
            meter.charge(1)?;
            write.location.array.set(
                &mut meter.store,
                write.location.index,
                Val::AnyRef(Some(bindings[write.pair].replacement)),
            )?;
        }
        Ok(writes.len())
    }
}

#[cfg(test)]
mod tests {
    use super::super::own;
    use super::*;

    fn args(session: &mut Session, values: &[&SessionValue]) -> SessionValue {
        let _request = session.operation_budget_scope().unwrap();
        let mut scope = RootScope::new(&mut session.store);
        let function = session.runtime.get_func(&mut scope, "args-new").unwrap();
        let mut result = [Val::null_any_ref()];
        function
            .call(
                &mut scope,
                &[Val::I32(i32::try_from(values.len()).unwrap())],
                &mut result,
            )
            .unwrap();
        let array = result[0]
            .unwrap_anyref()
            .unwrap()
            .as_array(&scope)
            .unwrap()
            .unwrap();
        for (index, value) in values.iter().enumerate() {
            let value = Val::AnyRef(Some(value.value.to_rooted(&mut scope)));
            array.set(&mut scope, index as u32, value).unwrap();
        }
        own(
            &mut scope,
            result[0].clone(),
            session.identity,
            &session.handles,
        )
        .unwrap()
    }
    fn set(session: &mut Session, array: &SessionValue, index: u32, value: &SessionValue) {
        session
            .inspect(array, |mut store, array| {
                let array = array.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                let value = Val::AnyRef(Some(value.value.to_rooted(&mut store)));
                array.set(&mut store, index, value)
            })
            .unwrap();
    }
    fn is(
        session: &mut Session,
        array: &SessionValue,
        index: u32,
        expected: &SessionValue,
    ) -> bool {
        session
            .inspect(array, |mut store, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                let value = array
                    .get(&mut store, index)?
                    .unwrap_anyref()
                    .copied()
                    .unwrap();
                let expected = expected.value.to_rooted(&mut store);
                Rooted::<AnyRef>::ref_eq(&store, &value, &expected)
            })
            .unwrap()
    }
    #[test]
    fn private_fixups_opaque_frontier_preserves_slots_and_avoids_untainted_scan() {
        let mut session = Session::with_options(Default::default()).unwrap();
        let placeholder = session.data_array(&[]).unwrap();
        let replacement = session.eval("17").unwrap();
        let scalar = session.eval("0").unwrap();
        let branch = args(&mut session, &vec![&scalar; 512]);
        let full = args(&mut session, &[&placeholder, &branch]);
        let cut = args(&mut session, &[&placeholder, &branch]);
        let _operation = session.operation_budget_scope().unwrap();
        let before = session.operation_fuel_remaining().unwrap();
        assert_eq!(session.fixup_private_placeholders(&full, &[(&placeholder, &replacement)]).unwrap(), 1);
        let full_cost = before - session.operation_fuel_remaining().unwrap();
        let before = session.operation_fuel_remaining().unwrap();
        assert_eq!(session.fixup_private_placeholders_cut_impl(&cut, &[(&placeholder, &replacement)], &[branch.clone()], true).unwrap(), 1);
        let cut_cost = before - session.operation_fuel_remaining().unwrap();
        assert!(cut_cost < full_cost, "opaque frontier must avoid actual scan work: {cut_cost} >= {full_cost}");
        assert!(is(&mut session, &full, 0, &replacement));
        assert!(is(&mut session, &cut, 0, &replacement));
        assert!(is(&mut session, &cut, 1, &branch));
        assert!(is(&mut session, &branch, 511, &scalar));
        let rejected = args(&mut session, &[&placeholder]);
        assert!(session.fixup_private_placeholders_with_cuts(&rejected, &[(&placeholder, &replacement)], &[placeholder.clone()]).is_err());
        assert!(is(&mut session, &rejected, 0, &placeholder));
    }
    #[test]
    fn private_fixups_shared_cycles_collisions_and_gc() {
        let mut session = Session::with_options(Default::default()).unwrap();
        let placeholder = session.data_array(&[]).unwrap();
        let replacement = session.eval("17").unwrap();
        let child = args(&mut session, &[&placeholder, &placeholder]);
        set(&mut session, &child, 1, &child); // One private cycle, one fixup slot.
        let parent = args(&mut session, &[&child, &child]);
        // Also exercise canonical OBJECT.field1 via a fresh source-array parent.
        let root = session.data_array(&[&parent]).unwrap();
        session.collect().unwrap();
        assert_eq!(
            session
                .fixup_private_placeholders_impl(&root, &[(&placeholder, &replacement)], true)
                .unwrap(),
            1
        );
        session.collect().unwrap();
        assert!(is(&mut session, &child, 0, &replacement));
        assert!(is(&mut session, &child, 1, &child));
        assert!(is(&mut session, &parent, 0, &child));
        assert!(is(&mut session, &parent, 1, &child));
    }
    #[test]
    fn private_fixups_exact_unique_bindings_and_foreign_reset_reject_before_write() {
        let mut session = Session::with_options(Default::default()).unwrap();
        let first = session.data_array(&[]).unwrap();
        let second = session.data_array(&[]).unwrap();
        let target = session.eval("99").unwrap();
        let parent = args(&mut session, &[&first, &second]);
        assert!(
            session
                .fixup_private_placeholders(&parent, &[(&first, &target), (&first, &target)])
                .is_err()
        );
        assert!(is(&mut session, &parent, 0, &first));
        assert!(
            session
                .fixup_private_placeholders(&parent, &[(&first, &second), (&second, &target)])
                .is_err()
        );
        assert!(
            session
                .fixup_private_placeholders(&first, &[(&first, &target)])
                .is_err()
        );
        let mut foreign = Session::with_options(Default::default()).unwrap();
        let other = foreign.eval("1").unwrap();
        assert!(matches!(
            session.fixup_private_placeholders(&parent, &[(&first, &other)]),
            Err(SessionError::ForeignValue)
        ));
        assert!(is(&mut session, &parent, 0, &first));
        assert_eq!(
            session
                .fixup_private_placeholders(&parent, &[(&first, &target), (&second, &target)])
                .unwrap(),
            2
        );
        assert!(is(&mut session, &parent, 0, &target));
        assert!(is(&mut session, &parent, 1, &target));
        session.reset().unwrap();
        assert!(matches!(
            session.fixup_private_placeholders(&parent, &[(&first, &target)]),
            Err(SessionError::ForeignValue)
        ));
    }
    #[test]
    fn private_fixups_binding_indexes_handle_collisions_duplicates_and_both_overlap_orders() {
        let mut session = Session::with_options(Default::default()).unwrap();
        let first = session.data_array(&[]).unwrap();
        let second = session.data_array(&[]).unwrap();
        let third = session.data_array(&[]).unwrap();
        let a = session.eval("17").unwrap();
        let b = session.eval("99").unwrap();
        let parent = args(&mut session, &[&first, &second, &third, &a, &b]);
        // Force ALL indexes (placeholder, target and visited) into one bucket.
        // Check a duplicate beyond the first collision entry; no writes occur.
        let duplicate = session
            .fixup_private_placeholders_impl(
                &parent,
                &[(&first, &a), (&second, &b), (&third, &a), (&second, &a)],
                true,
            )
            .unwrap_err();
        assert!(
            duplicate
                .to_string()
                .contains("Duplicate private placeholder")
        );
        for bindings in [
            vec![(&first, &a), (&second, &first)],
            vec![(&first, &second), (&second, &a)],
        ] {
            let overlap = session
                .fixup_private_placeholders_impl(&parent, &bindings, true)
                .unwrap_err();
            assert!(overlap.to_string().contains("aliases a replacement"));
        }
        let root = session
            .fixup_private_placeholders_impl(
                &third,
                &[(&first, &a), (&second, &b), (&third, &a)],
                true,
            )
            .unwrap_err();
        assert!(root.to_string().contains("root cannot be a placeholder"));
        assert!(is(&mut session, &parent, 0, &first));
        assert!(is(&mut session, &parent, 1, &second));
        assert!(is(&mut session, &parent, 2, &third));
        // Distinct placeholder hits traverse the forced collision chain. Shared
        // targets retain one canonical binding and both distinct targets are cut.
        assert_eq!(
            session
                .fixup_private_placeholders_impl(
                    &parent,
                    &[(&first, &a), (&second, &b), (&third, &a)],
                    true
                )
                .unwrap(),
            3
        );
        assert!(is(&mut session, &parent, 0, &a));
        assert!(is(&mut session, &parent, 1, &b));
        assert!(is(&mut session, &parent, 2, &a));
        assert!(is(&mut session, &parent, 3, &a));
        assert!(is(&mut session, &parent, 4, &b));
    }
    #[test]
    fn private_fixups_replacement_targets_are_opaque_and_cost_no_descendant_walk() {
        let mut session = Session::with_options(Default::default()).unwrap();
        let placeholder = session.data_array(&[]).unwrap();
        let nil = session.eval("nil").unwrap();
        let small = args(&mut session, &[&nil]);
        let many = vec![&nil; 4096];
        let large = args(&mut session, &many);
        let mut costs = Vec::new();
        for target in [&small, &large] {
            let parent = args(&mut session, &[target, &placeholder]);
            let _request = session.operation_budget_scope().unwrap();
            let before = session.store.get_fuel().unwrap();
            assert_eq!(
                session
                    .fixup_private_placeholders(&parent, &[(&placeholder, target)])
                    .unwrap(),
                1
            );
            costs.push(before - session.store.get_fuel().unwrap());
            assert!(is(&mut session, &parent, 1, target));
        }
        assert_eq!(
            costs[0], costs[1],
            "replacement descendants must not be visited"
        );
    }
    #[test]
    fn private_fixups_placeholder_hidden_behind_cut_replacement_remains_untouched() {
        let mut session = Session::with_options(Default::default()).unwrap();
        let placeholder = session.data_array(&[]).unwrap();
        let replacement = args(&mut session, &[&placeholder]);
        let parent = args(&mut session, &[&replacement, &placeholder]);
        // Negative boundary fixture: deliberately hiding the private token in a
        // target does not establish caller isolation or source eligibility.
        // The helper must neither inspect nor repair that selected root, even
        // when it is already reachable from the fresh parent before writes.
        session.collect().unwrap();
        assert_eq!(
            session
                .fixup_private_placeholders_impl(&parent, &[(&placeholder, &replacement)], true)
                .unwrap(),
            1
        );
        session.collect().unwrap();
        assert!(is(&mut session, &parent, 0, &replacement));
        assert!(is(&mut session, &parent, 1, &replacement));
        assert!(is(&mut session, &replacement, 0, &placeholder));
    }
    #[test]
    fn private_fixups_fuel_irq_cleanup_and_recovery() {
        let mut session = Session::with_options(Default::default()).unwrap();
        let placeholder = session.data_array(&[]).unwrap();
        let target = session.eval("17").unwrap();
        let handles = session.stats().external_value_handles;
        // Fresh/discarded parent on every attempt: no atomic-write promise.
        for allowance in [0, 1, 16, 64, 128, 256, 512] {
            let parent = args(&mut session, &[&placeholder, &placeholder]);
            let request = session.operation_budget_scope().unwrap();
            session.store.set_fuel(allowance).unwrap();
            let result = session.fixup_private_placeholders(&parent, &[(&placeholder, &target)]);
            if allowance == 0 {
                assert!(result.is_err(), "zero fuel must reject before any write");
            }
            if let Err(error) = &result {
                assert!(
                    matches!(error, SessionError::Trap(error)
                    if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
                    "{error:?}"
                );
            }
            drop(request);
            if allowance == 0 {
                assert!(is(&mut session, &parent, 0, &placeholder));
                assert!(is(&mut session, &parent, 1, &placeholder));
            }
            drop(parent);
            assert_eq!(session.stats().external_value_handles, handles);
        }
        let parent = args(&mut session, &[&placeholder]);
        {
            let _request = session.operation_budget_scope().unwrap();
            session.interrupt_handle().interrupt();
            assert!(
                session
                    .fixup_private_placeholders(&parent, &[(&placeholder, &target)])
                    .unwrap_err()
                    .is_interrupt()
            );
        }
        session.collect().unwrap();
        assert!(is(&mut session, &parent, 0, &placeholder));
        assert_eq!(
            session
                .fixup_private_placeholders(&parent, &[(&placeholder, &target)])
                .unwrap(),
            1
        );
        assert!(is(&mut session, &parent, 0, &target));
    }
}
