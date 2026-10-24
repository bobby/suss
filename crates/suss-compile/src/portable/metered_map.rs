//! Original ordered storage with fallible, caller-metered key comparisons.
//!
//! Costs: one charge per searched node; Context::compare must bill the actual
//! compared key data and poll cancellation. Insert first plans height propagation
//! with 32 primitive operations per inspected ancestor, then prepays at most 64
//! primitive tree operations per affected ancestor (height checks plus up to two rotations),
//! plus 12 fixed insertion operations for the leaf, parent link, free list and len.
//! Backing allocations prepay requested bytes and the maximum live relocation
//! bytes. Retain prepays its actual index buffers and final structural writes.
//! No native Ord/Hash/Eq is used on K. Meter errors never mutate the map.
//! Allocator failure follows Vec's ordinary behavior; it is not a Context error.
//! Retain predicates may have external effects, which cannot be rolled back.
use std::{cmp::Ordering, mem};

pub trait Context<K> {
    type Error;
    fn charge(&mut self, units: u64) -> Result<(), Self::Error>;
    /// Must define one consistent total ordering, charging its own key traversal.
    fn compare(&mut self, lhs: &K, rhs: &K) -> Result<Ordering, Self::Error>;
}
struct Node<K, V> {
    key: K,
    value: V,
    left: Option<usize>,
    right: Option<usize>,
    parent: Option<usize>,
    height: usize,
}
enum Slot<K, V> {
    Live(Node<K, V>),
    Free(Option<usize>),
}
pub struct TryOrderedMap<K, V> {
    slots: Vec<Slot<K, V>>,
    root: Option<usize>,
    free: Option<usize>,
    len: usize,
}
impl<K, V> Default for TryOrderedMap<K, V> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            root: None,
            free: None,
            len: 0,
        }
    }
}
fn bytes<T>(count: usize) -> u64 {
    (count as u64).saturating_mul(mem::size_of::<T>() as u64)
}
impl<K, V> TryOrderedMap<K, V> {
    pub fn len(&self) -> usize {
        self.len
    }
    fn node(&self, i: usize) -> &Node<K, V> {
        match &self.slots[i] {
            Slot::Live(n) => n,
            Slot::Free(_) => unreachable!("AVL link to free slot"),
        }
    }
    fn node_mut(&mut self, i: usize) -> &mut Node<K, V> {
        match &mut self.slots[i] {
            Slot::Live(n) => n,
            Slot::Free(_) => unreachable!("AVL link to free slot"),
        }
    }
    fn height(&self, i: Option<usize>) -> usize {
        i.map_or(0, |i| self.node(i).height)
    }
    fn update(&mut self, i: usize) {
        let n = self.node(i);
        let height = 1 + self.height(n.left).max(self.height(n.right));
        self.node_mut(i).height = height;
    }
    fn balance(&self, i: usize) -> isize {
        self.height(self.node(i).left) as isize - self.height(self.node(i).right) as isize
    }
    fn replace_link(&mut self, parent: Option<usize>, old: usize, new: usize) {
        if let Some(parent) = parent {
            let p = self.node_mut(parent);
            if p.left == Some(old) {
                p.left = Some(new);
            } else {
                debug_assert_eq!(p.right, Some(old));
                p.right = Some(new);
            }
        } else {
            self.root = Some(new);
        }
    }
    fn rotate_left(&mut self, x: usize) -> usize {
        let y = self.node(x).right.expect("right child for left rotation");
        let middle = self.node(y).left;
        let parent = self.node(x).parent;
        self.node_mut(x).right = middle;
        if let Some(middle) = middle {
            self.node_mut(middle).parent = Some(x);
        }
        self.node_mut(y).left = Some(x);
        self.node_mut(x).parent = Some(y);
        self.node_mut(y).parent = parent;
        self.replace_link(parent, x, y);
        self.update(x);
        self.update(y);
        y
    }
    fn rotate_right(&mut self, x: usize) -> usize {
        let y = self.node(x).left.expect("left child for right rotation");
        let middle = self.node(y).right;
        let parent = self.node(x).parent;
        self.node_mut(x).left = middle;
        if let Some(middle) = middle {
            self.node_mut(middle).parent = Some(x);
        }
        self.node_mut(y).right = Some(x);
        self.node_mut(x).parent = Some(y);
        self.node_mut(y).parent = parent;
        self.replace_link(parent, x, y);
        self.update(x);
        self.update(y);
        y
    }
    fn rebalance(&mut self, mut cursor: Option<usize>) {
        while let Some(i) = cursor {
            let old_height = self.node(i).height;
            self.update(i);
            let top = if self.balance(i) > 1 {
                let left = self.node(i).left.unwrap();
                if self.balance(left) < 0 {
                    self.rotate_left(left);
                }
                self.rotate_right(i)
            } else if self.balance(i) < -1 {
                let right = self.node(i).right.unwrap();
                if self.balance(right) > 0 {
                    self.rotate_right(right);
                }
                self.rotate_left(i)
            } else {
                i
            };
            // Insertion cannot affect ancestors after this subtree's height
            // stabilizes, including after its single/double rotation.
            if self.node(top).height == old_height { break; }
            cursor = self.node(top).parent;
        }
    }
    fn insertion_rebalance_visits<C: Context<K>>(
        &self, mut cursor: Option<usize>, direction: Ordering, context: &mut C,
    ) -> Result<u64, C::Error> {
        let mut child = None;
        let mut child_height = 1;
        let mut visits = 0u64;
        while let Some(i) = cursor {
            // Read-only planning is billed before inspection. No key callbacks
            // or mutations occur here; include this work in the same budget.
            context.charge(32)?;
            let node = self.node(i);
            let left = self.height(node.left);
            let right = self.height(node.right);
            let grows_left = match child {
                None => direction == Ordering::Less,
                Some(_) => {
                    debug_assert!(node.left == child || node.right == child);
                    node.left == child
                }
            };
            let (left, right) = if grows_left {(child_height,right)} else {(left,child_height)};
            visits += 1;
            // An insertion rotation restores this subtree's previous height.
            if left.abs_diff(right) > 1 { break; }
            let height = 1 + left.max(right);
            if height == node.height { break; }
            child = Some(i);
            child_height = height;
            cursor = node.parent;
        }
        Ok(visits)
    }
    pub fn get<'a, C: Context<K>>(
        &'a self,
        key: &K,
        context: &mut C,
    ) -> Result<Option<&'a V>, C::Error> {
        let mut cursor = self.root;
        // Poll even an empty hit/miss path; zero does not grant free comparison.
        context.charge(0)?;
        while let Some(i) = cursor {
            context.charge(1)?;
            let n = self.node(i);
            cursor = match context.compare(key, &n.key)? {
                Ordering::Less => n.left,
                Ordering::Greater => n.right,
                Ordering::Equal => return Ok(Some(&n.value)),
            };
        }
        Ok(None)
    }
    pub fn insert<C: Context<K>>(
        &mut self,
        key: K,
        value: V,
        context: &mut C,
    ) -> Result<Option<V>, C::Error> {
        let (_, old, _) = self.insert_entry(key, value, true, context)?;
        Ok(old)
    }
    /// One search. An occupied entry retains its original key AND value.
    /// Candidate values have no deferred callbacks; all fallible work completes
    /// before publication. Returns the canonical value and whether inserted.
    pub fn get_or_insert<C: Context<K>>(
        &mut self,
        key: K,
        value: V,
        context: &mut C,
    ) -> Result<(&V, bool), C::Error> {
        let (index, _, inserted) = self.insert_entry(key, value, false, context)?;
        Ok((&self.node(index).value, inserted))
    }
    fn insert_entry<C: Context<K>>(
        &mut self,
        key: K,
        value: V,
        replace: bool,
        context: &mut C,
    ) -> Result<(usize, Option<V>, bool), C::Error> {
        context.charge(0)?;
        let mut cursor = self.root;
        let mut parent = None;
        let mut direction = Ordering::Equal;
        while let Some(i) = cursor {
            context.charge(1)?;
            let n = self.node(i);
            direction = context.compare(&key, &n.key)?;
            match direction {
                Ordering::Equal => {
                    context.charge(1)?;
                    // insert keeps its old replacement semantics; entry never
                    // replaces either object on an occupied path.
                    let old = if replace {
                        Some(mem::replace(&mut self.node_mut(i).value, value))
                    } else {
                        None
                    };
                    return Ok((i, old, false));
                }
                Ordering::Less => cursor = n.left,
                Ordering::Greater => cursor = n.right,
            }
            parent = Some(i);
        }
        // Search has finished. No fallible callbacks run after publication.
        let visits = self.insertion_rebalance_visits(parent, direction, context)?;
        context.charge(visits.saturating_mul(64).saturating_add(12))?;
        let grow_to = if self.free.is_none() && self.slots.len() == self.slots.capacity() {
            Some(self.slots.capacity().saturating_mul(2).max(4))
        } else {
            None
        };
        if let Some(capacity) = grow_to {
            context.charge(
                bytes::<Slot<K, V>>(capacity).saturating_add(bytes::<Slot<K, V>>(self.slots.len())),
            )?;
        }
        // All charges, including allocation, succeeded before reserve/mutation.
        if let Some(capacity) = grow_to {
            self.slots.reserve_exact(capacity - self.slots.len());
        }
        let n = Node {
            key,
            value,
            left: None,
            right: None,
            parent,
            height: 1,
        };
        let i = if let Some(i) = self.free {
            let next = match self.slots[i] {
                Slot::Free(next) => next,
                Slot::Live(_) => unreachable!("live free-list slot"),
            };
            self.slots[i] = Slot::Live(n);
            self.free = next;
            i
        } else {
            let i = self.slots.len();
            self.slots.push(Slot::Live(n));
            i
        };
        if let Some(parent) = parent {
            if direction == Ordering::Less {
                self.node_mut(parent).left = Some(i);
            } else {
                self.node_mut(parent).right = Some(i);
            }
        } else {
            self.root = Some(i);
        }
        self.len += 1;
        self.rebalance(parent);
        Ok((i, None, true))
    }
    fn first<C: Context<K>>(
        &self,
        mut cursor: Option<usize>,
        context: &mut C,
    ) -> Result<Option<usize>, C::Error> {
        while let Some(i) = cursor {
            context.charge(1)?;
            match self.node(i).left {
                Some(left) => cursor = Some(left),
                None => return Ok(Some(i)),
            }
        }
        Ok(None)
    }
    fn successor<C: Context<K>>(
        &self,
        i: usize,
        context: &mut C,
    ) -> Result<Option<usize>, C::Error> {
        context.charge(1)?;
        if self.node(i).right.is_some() {
            return self.first(self.node(i).right, context);
        }
        let mut child = i;
        let mut parent = self.node(child).parent;
        while let Some(p) = parent {
            context.charge(1)?;
            if self.node(p).left == Some(child) {
                return Ok(Some(p));
            }
            child = p;
            parent = self.node(p).parent;
        }
        Ok(None)
    }
    fn rebuild(&mut self, sorted: &[usize], parent: Option<usize>) -> Option<usize> {
        if sorted.is_empty() {
            return None;
        }
        let middle = sorted.len() / 2;
        let i = sorted[middle];
        let left = self.rebuild(&sorted[..middle], Some(i));
        let right = self.rebuild(&sorted[middle + 1..], Some(i));
        let height = 1 + self.height(left).max(self.height(right));
        let n = self.node_mut(i);
        n.left = left;
        n.right = right;
        n.parent = parent;
        n.height = height;
        Some(i)
    }
    pub fn retain<C: Context<K>>(
        &mut self,
        context: &mut C,
        mut keep: impl FnMut(&K, &V) -> bool,
    ) -> Result<(), C::Error> {
        context.charge(0)?;
        // Reserve known-size decision buffers only after paying for them.
        context.charge(bytes::<usize>(self.len).saturating_mul(2))?;
        let mut kept = Vec::with_capacity(self.len);
        let mut removed = Vec::with_capacity(self.len);
        let mut cursor = self.first(self.root, context)?;
        while let Some(i) = cursor {
            context.charge(1)?;
            let n = self.node(i);
            if keep(&n.key, &n.value) {
                kept.push(i);
            } else {
                removed.push(i);
            }
            cursor = self.successor(i, context)?;
        }
        // Charge retirement storage and every final tree/free-list field write.
        context.charge(bytes::<Slot<K, V>>(removed.len()))?;
        context.charge(
            (removed.len() as u64)
                .saturating_mul(2)
                .saturating_add((kept.len() as u64).saturating_mul(4))
                .saturating_add(2),
        )?;
        let mut retired = Vec::with_capacity(removed.len());
        // Nothing below calls comparison, the predicate, or a fallible meter.
        for i in removed {
            let old = mem::replace(&mut self.slots[i], Slot::Free(self.free));
            self.free = Some(i);
            retired.push(old);
        }
        self.root = self.rebuild(&kept, None);
        self.len = kept.len();
        // Key/value destructors run only after a fully valid tree is committed.
        drop(retired);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    #[derive(Debug, PartialEq, Eq)]
    enum Error {
        Fuel,
        Cancel,
    }
    struct Meter {
        remaining: u64,
        calls: usize,
        cancel_at: Option<usize>,
    }
    impl Meter {
        fn full() -> Self {
            Self {
                remaining: u64::MAX,
                calls: 0,
                cancel_at: None,
            }
        }
    }
    impl Context<String> for Meter {
        type Error = Error;
        fn charge(&mut self, units: u64) -> Result<(), Error> {
            self.calls += 1;
            if self.cancel_at == Some(self.calls) {
                return Err(Error::Cancel);
            }
            self.remaining = self.remaining.checked_sub(units).ok_or(Error::Fuel)?;
            Ok(())
        }
        fn compare(&mut self, a: &String, b: &String) -> Result<Ordering, Error> {
            // No a.cmp(b): visit exactly the compared byte pairs, including EOF.
            let mut a = a.bytes();
            let mut b = b.bytes();
            loop {
                self.charge(1)?;
                match (a.next(), b.next()) {
                    (Some(a), Some(b)) => match a.cmp(&b) {
                        Ordering::Equal => {}
                        order => return Ok(order),
                    },
                    (None, None) => return Ok(Ordering::Equal),
                    (None, Some(_)) => return Ok(Ordering::Less),
                    (Some(_), None) => return Ok(Ordering::Greater),
                }
            }
        }
    }
    fn verify(map: &TryOrderedMap<String, usize>, reference: &BTreeMap<String, usize>) {
        fn tree(
            map: &TryOrderedMap<String, usize>,
            i: Option<usize>,
            parent: Option<usize>,
            out: &mut Vec<(String, usize)>,
            seen: &mut Vec<bool>,
        ) -> usize {
            let Some(i) = i else { return 0 };
            assert!(!seen[i], "cycle or repeated child");
            seen[i] = true;
            let n = map.node(i);
            assert_eq!(n.parent, parent);
            let left = tree(map, n.left, Some(i), out, seen);
            out.push((n.key.clone(), n.value));
            let right = tree(map, n.right, Some(i), out, seen);
            assert!(left.abs_diff(right) <= 1);
            assert_eq!(n.height, 1 + left.max(right));
            n.height
        }
        let mut observed = Vec::new();
        let mut seen = vec![false; map.slots.len()];
        tree(map, map.root, None, &mut observed, &mut seen);
        assert_eq!(
            observed,
            reference
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect::<Vec<_>>()
        );
        assert_eq!(map.len(), reference.len());
        let mut free = map.free;
        while let Some(i) = free {
            assert!(!seen[i], "free/live overlap or free cycle");
            seen[i] = true;
            free = match map.slots[i] {
                Slot::Free(next) => next,
                Slot::Live(_) => panic!("live free slot"),
            };
        }
        assert!(seen.into_iter().all(|x| x), "orphan slot");
        let mut context = Meter::full();
        for (key, value) in reference {
            assert_eq!(map.get(key, &mut context).unwrap(), Some(value));
        }
        assert_eq!(
            map.get(&"absent key".to_owned(), &mut context).unwrap(),
            None
        );
    }
    fn fill(
        keys: impl IntoIterator<Item = String>,
    ) -> (TryOrderedMap<String, usize>, BTreeMap<String, usize>) {
        let mut map = TryOrderedMap::default();
        let mut reference = BTreeMap::new();
        let mut meter = Meter::full();
        for (i, key) in keys.into_iter().enumerate() {
            assert_eq!(
                map.insert(key.clone(), i, &mut meter).unwrap(),
                reference.insert(key, i)
            );
            verify(&map, &reference);
        }
        (map, reference)
    }
    struct SearchMeter {
        meter: Meter,
        comparisons: usize,
    }
    impl SearchMeter {
        fn full() -> Self {
            Self {
                meter: Meter::full(),
                comparisons: 0,
            }
        }
    }
    impl Context<String> for SearchMeter {
        type Error = Error;
        fn charge(&mut self, units: u64) -> Result<(), Error> {
            self.meter.charge(units)
        }
        fn compare(&mut self, left: &String, right: &String) -> Result<Ordering, Error> {
            self.comparisons += 1;
            self.meter.compare(left, right)
        }
    }
    #[test]
    fn insertion_with_stable_subtree_height_fits_finite_fuel_and_reuses_storage() {
        let (mut map, mut reference) = fill((0..129).map(|n| format!("{n:03}")));
        map.retain(&mut Meter::full(), |key, _| key != "128").unwrap();
        reference.remove("128");
        verify(&map, &reference);
        assert!(map.free.is_some());
        let mut meter = Meter {remaining:256,calls:0,cancel_at:None};
        assert_eq!(map.insert("-001".into(),999,&mut meter).unwrap(),None);
        reference.insert("-001".into(),999);
        verify(&map,&reference);
    }

    #[test]
    fn entry_searches_once_and_keeps_original_occupied_key_and_value() {
        let (mut map, mut reference) = fill((0..64).map(|n| format!("key{n:04}")));
        for (key, candidate, expected, vacant) in [
            ("key9999".to_owned(), 9000, 9000, true),
            ("key0031".to_owned(), 9001, 31, false),
        ] {
            let mut lookup = SearchMeter::full();
            let _ = map.get(&key, &mut lookup).unwrap();
            let mut entry = SearchMeter::full();
            let (actual, inserted) = map
                .get_or_insert(key.clone(), candidate, &mut entry)
                .unwrap();
            assert_eq!((*actual, inserted), (expected, vacant));
            assert_eq!(
                entry.comparisons, lookup.comparisons,
                "entry must use one search path"
            );
            if vacant {
                reference.insert(key, candidate);
            }
            verify(&map, &reference);
        }
        // The occupied entry retains both allocations without Clone bounds.
        let mut map = TryOrderedMap::default();
        let key = "same".to_owned();
        let key_pointer = key.as_ptr();
        let value = "original".to_owned();
        let value_pointer = value.as_ptr();
        map.get_or_insert(key, value, &mut Meter::full()).unwrap();
        let (value, inserted) = map
            .get_or_insert("same".into(), "replacement".into(), &mut Meter::full())
            .unwrap();
        assert!(!inserted);
        assert_eq!(value.as_ptr(), value_pointer);
        let root = map.root.unwrap();
        assert_eq!(map.node(root).key.as_ptr(), key_pointer);
    }
    #[test]
    fn every_entry_cancellation_point_preserves_old_values_and_recovers() {
        for occupied in [false, true] {
            let candidate = if occupied { "key0015" } else { "key9999" };
            let (mut baseline, _) = fill((0..32).map(|n| format!("key{n:04}")));
            let mut meter = Meter::full();
            baseline
                .get_or_insert(candidate.into(), 9999, &mut meter)
                .unwrap();
            for stop in 1..=meter.calls {
                let (mut map, reference) = fill((0..32).map(|n| format!("key{n:04}")));
                let mut failing = Meter::full();
                failing.cancel_at = Some(stop);
                assert_eq!(
                    map.get_or_insert(candidate.into(), 9999, &mut failing)
                        .map(|(value, inserted)| (*value, inserted)),
                    Err(Error::Cancel)
                );
                verify(&map, &reference);
                let (value, inserted) = map
                    .get_or_insert(candidate.into(), 9999, &mut Meter::full())
                    .unwrap();
                assert_eq!(
                    (*value, inserted),
                    if occupied { (15, false) } else { (9999, true) }
                );
            }
        }
    }

    #[test]
    fn differential_reverse_mixed_and_long_prefix_keys() {
        for keys in [
            (0..160).map(|n| format!("key{n:04}")).collect::<Vec<_>>(),
            (0..160).rev().map(|n| format!("key{n:04}")).collect(),
            (0..320)
                .map(|n| format!("{}{:04}", "prefix/".repeat(30), (n * 97) % 163))
                .collect(),
        ] {
            let (mut map, mut reference) = fill(keys);
            for modulus in [3, 5, 2] {
                map.retain(&mut Meter::full(), |_, value| value % modulus != 0)
                    .unwrap();
                reference.retain(|_, value| *value % modulus != 0);
                verify(&map, &reference);
                for n in 0..20 {
                    let key = format!("inserted/{modulus}/{n}");
                    assert_eq!(
                        map.insert(key.clone(), n, &mut Meter::full()).unwrap(),
                        reference.insert(key, n)
                    );
                    verify(&map, &reference);
                }
            }
            map.retain(&mut Meter::full(), |_, _| false).unwrap();
            reference.clear();
            verify(&map, &reference);
            map.insert("reuse".into(), 42, &mut Meter::full()).unwrap();
            reference.insert("reuse".into(), 42);
            verify(&map, &reference);
        }
    }
    #[test]
    fn every_insert_cancellation_point_leaves_old_map_unchanged_and_recovers() {
        let keys = ["m", "f", "t", "b", "h", "p", "x", "a"];
        for new_key in ["c", "h"] {
            let (mut probe, _) = fill(keys.map(str::to_owned));
            let mut measured = Meter::full();
            probe.insert(new_key.into(), 999, &mut measured).unwrap();
            for cancel_at in 1..=measured.calls {
                let (mut map, mut reference) = fill(keys.map(str::to_owned));
                let mut meter = Meter {
                    remaining: u64::MAX,
                    calls: 0,
                    cancel_at: Some(cancel_at),
                };
                assert_eq!(
                    map.insert(new_key.into(), 999, &mut meter),
                    Err(Error::Cancel)
                );
                verify(&map, &reference);
                assert_eq!(
                    map.insert(new_key.into(), 999, &mut Meter::full()).unwrap(),
                    reference.insert(new_key.into(), 999)
                );
                verify(&map, &reference);
            }
        }
    }
    #[test]
    fn insertion_fuel_failure_including_growth_does_not_publish() {
        // First insertion exercises allocation precharge; fifth exercises growth.
        for count in [0, 4, 9] {
            let keys = (0..count).map(|n| format!("k{n}")).collect::<Vec<_>>();
            let (mut probe, _) = fill(keys.clone());
            let mut measured = Meter::full();
            probe.insert("new".into(), 17, &mut measured).unwrap();
            let cost = u64::MAX - measured.remaining;
            let (mut map, reference) = fill(keys);
            let mut meter = Meter {
                remaining: cost - 1,
                calls: 0,
                cancel_at: None,
            };
            assert_eq!(map.insert("new".into(), 17, &mut meter), Err(Error::Fuel));
            verify(&map, &reference);
        }
    }
    #[test]
    fn every_retain_cancellation_point_preserves_old_values_and_free_list() {
        let keys = (0..25).map(|n| format!("key{n:02}")).collect::<Vec<_>>();
        let (mut probe, _) = fill(keys.clone());
        let mut measured = Meter::full();
        probe
            .retain(&mut measured, |_, value| value % 2 == 0)
            .unwrap();
        for cancel_at in 1..=measured.calls {
            let (mut map, mut reference) = fill(keys.clone());
            let mut meter = Meter {
                remaining: u64::MAX,
                calls: 0,
                cancel_at: Some(cancel_at),
            };
            assert_eq!(
                map.retain(&mut meter, |_, value| value % 2 == 0),
                Err(Error::Cancel)
            );
            verify(&map, &reference);
            map.retain(&mut Meter::full(), |_, value| value % 2 == 0)
                .unwrap();
            reference.retain(|_, value| *value % 2 == 0);
            verify(&map, &reference);
        }
    }
    #[test]
    fn retain_fuel_failure_before_commit_preserves_values_and_recovers() {
        let keys = (0..30).map(|n| format!("key{n:02}")).collect::<Vec<_>>();
        for mode in 0..3 {
            let predicate = |_: &String, value: &usize| match mode {
                0 => true,
                1 => false,
                _ => value % 2 == 0,
            };
            let (mut probe, _) = fill(keys.clone());
            let mut measured = Meter::full();
            probe.retain(&mut measured, predicate).unwrap();
            let cost = u64::MAX - measured.remaining;
            let (mut map, mut reference) = fill(keys.clone());
            let mut short = Meter {
                remaining: cost - 1,
                calls: 0,
                cancel_at: None,
            };
            assert_eq!(map.retain(&mut short, predicate), Err(Error::Fuel));
            verify(&map, &reference);
            map.retain(&mut Meter::full(), predicate).unwrap();
            reference.retain(|key, value| predicate(key, value));
            verify(&map, &reference);
        }
    }
    #[test]
    fn warm_lookup_bills_actual_prefix_comparisons_and_checks_cancellation() {
        let key = "x".repeat(1000);
        let (map, reference) = fill([key.clone()]);
        let mut small = Meter {
            remaining: 100,
            calls: 0,
            cancel_at: None,
        };
        assert_eq!(map.get(&key, &mut small), Err(Error::Fuel));
        let mut cancelled = Meter {
            remaining: u64::MAX,
            calls: 0,
            cancel_at: Some(30),
        };
        assert_eq!(map.get(&key, &mut cancelled), Err(Error::Cancel));
        verify(&map, &reference);
        let mut full = Meter::full();
        assert_eq!(map.get(&key, &mut full).unwrap(), Some(&0));
        assert_eq!(u64::MAX - full.remaining, 1002); // node + 1000 bytes + paired EOF
    }
    #[test]
    fn differential_interleaved_updates_retains_and_reused_slots() {
        let mut map = TryOrderedMap::default();
        let mut reference = BTreeMap::new();
        let mut random = 0xcafe_babe_u64;
        for step in 0..2000 {
            random = random
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let number = ((random >> 32) % 101) as usize;
            let key = format!("shared-prefix/{number:03}");
            match random % 13 {
                0 | 1 => {
                    let modulus = 2 + number % 5;
                    map.retain(&mut Meter::full(), |_, value| (value + step) % modulus != 0)
                        .unwrap();
                    reference.retain(|_, value| (*value + step) % modulus != 0);
                }
                2 => assert_eq!(
                    map.get(&key, &mut Meter::full()).unwrap(),
                    reference.get(&key)
                ),
                3 => {
                    let mut cancelled = Meter {
                        remaining: u64::MAX,
                        calls: 0,
                        cancel_at: Some(1),
                    };
                    assert_eq!(map.insert(key, step, &mut cancelled), Err(Error::Cancel));
                }
                _ => assert_eq!(
                    map.insert(key.clone(), step, &mut Meter::full()).unwrap(),
                    reference.insert(key, step)
                ),
            }
            verify(&map, &reference);
        }
    }
    #[test]
    fn replacement_keeps_original_key_without_clone_and_retain_needs_no_clone() {
        struct Key {
            order: u8,
            identity: u8,
        }
        struct Value(u8);
        struct Plain;
        impl Context<Key> for Plain {
            type Error = ();
            fn charge(&mut self, _: u64) -> Result<(), ()> {
                Ok(())
            }
            fn compare(&mut self, a: &Key, b: &Key) -> Result<Ordering, ()> {
                Ok(a.order.cmp(&b.order))
            }
        }
        let mut map = TryOrderedMap::default();
        assert!(
            map.insert(
                Key {
                    order: 1,
                    identity: 7
                },
                Value(1),
                &mut Plain
            )
            .unwrap()
            .is_none()
        );
        assert_eq!(
            map.insert(
                Key {
                    order: 1,
                    identity: 9
                },
                Value(2),
                &mut Plain
            )
            .unwrap()
            .unwrap()
            .0,
            1
        );
        map.retain(&mut Plain, |key, value| {
            assert_eq!(key.identity, 7);
            value.0 == 2
        })
        .unwrap();
        assert_eq!(map.len(), 1);
    }
}

/// Unordered storage for small fixed-width identity keys (no key data beyond
/// their words), with fixed-key hashing. A lookup pays one keyed hash, group
/// probe and key compare; an insertion also prepays growth (new backing,
/// rehash of every live entry and retirement) before allocating. Meter errors
/// never mutate the map.
pub struct TryHashMap<K, V>(std::collections::HashMap<K, V,
    std::hash::BuildHasherDefault<std::collections::hash_map::DefaultHasher>>);
impl<K, V> Default for TryHashMap<K, V> {
    fn default() -> Self { Self(Default::default()) }
}
const HASH_PROBE: u64 = 4 * mem::size_of::<usize>() as u64;
impl<K: std::hash::Hash + Eq, V> TryHashMap<K, V> {
    pub fn len(&self) -> usize { self.0.len() }
    pub fn get<C: Context<K>>(&self, key: &K, context: &mut C) -> Result<Option<&V>, C::Error> {
        context.charge(HASH_PROBE + mem::size_of::<K>() as u64)?;
        Ok(self.0.get(key))
    }
    pub fn insert<C: Context<K>>(&mut self, key: K, value: V, context: &mut C) -> Result<Option<V>, C::Error> {
        context.charge(HASH_PROBE + mem::size_of::<K>() as u64)?;
        if self.0.len() == self.0.capacity() {
            let next = self.0.capacity().saturating_mul(2).max(4);
            let entry = (mem::size_of::<(K, V)>() + 1) as u64;
            let bytes = (2 * next + self.0.capacity()) as u64 * entry + self.0.len() as u64 * HASH_PROBE;
            context.charge(bytes)?;
            self.0.reserve(next - self.0.len());
        }
        context.charge(4)?; // slot, control byte and length writes
        Ok(self.0.insert(key, value))
    }
}
