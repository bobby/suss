//! Structurally shared ordered collections for immutable compiler snapshots.
//! Cloning retains one root; an update path-copies O(log n) nodes and leaves
//! every earlier snapshot unchanged. Iteration order matches BTreeMap/BTreeSet.
use std::{borrow::Borrow, cmp::Ordering, fmt, sync::Arc};

struct Node<K, V> {
    key: K,
    value: V,
    left: Link<K, V>,
    right: Link<K, V>,
    height: u8,
}
type Link<K, V> = Option<Arc<Node<K, V>>>;

fn height<K, V>(link: &Link<K, V>) -> u8 {
    link.as_ref().map_or(0, |node| node.height)
}

fn node<K, V>(key: K, value: V, left: Link<K, V>, right: Link<K, V>) -> Arc<Node<K, V>> {
    let height = height(&left).max(height(&right)) + 1;
    Arc::new(Node { key, value, left, right, height })
}

impl<K: Clone, V: Clone> Node<K, V> {
    fn with(&self, left: Link<K, V>, right: Link<K, V>) -> Arc<Node<K, V>> {
        node(self.key.clone(), self.value.clone(), left, right)
    }
}

fn balance<K: Clone, V: Clone>(key: K, value: V, left: Link<K, V>, right: Link<K, V>) -> Arc<Node<K, V>> {
    let (hl, hr) = (height(&left), height(&right));
    if hl > hr + 1 {
        let l = left.expect("left-heavy subtree");
        if height(&l.left) >= height(&l.right) {
            l.with(l.left.clone(), Some(node(key, value, l.right.clone(), right)))
        } else {
            let lr = l.right.as_ref().expect("left-right subtree");
            lr.with(
                Some(l.with(l.left.clone(), lr.left.clone())),
                Some(node(key, value, lr.right.clone(), right)),
            )
        }
    } else if hr > hl + 1 {
        let r = right.expect("right-heavy subtree");
        if height(&r.right) >= height(&r.left) {
            r.with(Some(node(key, value, left, r.left.clone())), r.right.clone())
        } else {
            let rl = r.left.as_ref().expect("right-left subtree");
            rl.with(
                Some(node(key, value, left, rl.left.clone())),
                Some(r.with(rl.right.clone(), r.right.clone())),
            )
        }
    } else {
        node(key, value, left, right)
    }
}

fn insert<K: Ord + Clone, V: Clone>(link: &Link<K, V>, key: K, value: V, old: &mut Option<V>) -> Arc<Node<K, V>> {
    let Some(current) = link else { return node(key, value, None, None) };
    match key.cmp(&current.key) {
        Ordering::Less => {
            let left = insert(&current.left, key, value, old);
            balance(current.key.clone(), current.value.clone(), Some(left), current.right.clone())
        }
        Ordering::Greater => {
            let right = insert(&current.right, key, value, old);
            balance(current.key.clone(), current.value.clone(), current.left.clone(), Some(right))
        }
        Ordering::Equal => {
            *old = Some(current.value.clone());
            // BTreeMap::insert keeps the existing key and replaces the value.
            node(current.key.clone(), value, current.left.clone(), current.right.clone())
        }
    }
}

fn take_min<K: Ord + Clone, V: Clone>(current: &Arc<Node<K, V>>) -> (K, V, Link<K, V>) {
    match &current.left {
        None => (current.key.clone(), current.value.clone(), current.right.clone()),
        Some(left) => {
            let (key, value, rest) = take_min(left);
            (key, value, Some(balance(current.key.clone(), current.value.clone(), rest, current.right.clone())))
        }
    }
}

fn remove<K, V, Q>(link: &Link<K, V>, key: &Q, old: &mut Option<V>) -> Link<K, V>
where K: Ord + Clone + Borrow<Q>, V: Clone, Q: Ord + ?Sized {
    let current = link.as_ref()?;
    match key.cmp(current.key.borrow()) {
        Ordering::Less => {
            let left = remove(&current.left, key, old);
            if old.is_none() { return link.clone(); }
            Some(balance(current.key.clone(), current.value.clone(), left, current.right.clone()))
        }
        Ordering::Greater => {
            let right = remove(&current.right, key, old);
            if old.is_none() { return link.clone(); }
            Some(balance(current.key.clone(), current.value.clone(), current.left.clone(), right))
        }
        Ordering::Equal => {
            *old = Some(current.value.clone());
            match (&current.left, &current.right) {
                (None, right) => right.clone(),
                (left, None) => left.clone(),
                (left, Some(right)) => {
                    let (key, value, rest) = take_min(right);
                    Some(balance(key, value, left.clone(), rest))
                }
            }
        }
    }
}

pub struct PersistentMap<K, V> {
    root: Link<K, V>,
    len: usize,
}

impl<K, V> Clone for PersistentMap<K, V> {
    fn clone(&self) -> Self { Self { root: self.root.clone(), len: self.len } }
}
impl<K, V> Default for PersistentMap<K, V> {
    fn default() -> Self { Self { root: None, len: 0 } }
}

impl<K, V> PersistentMap<K, V> {
    pub fn new() -> Self { Self::default() }
    pub fn len(&self) -> usize { self.len }
    pub fn is_empty(&self) -> bool { self.len == 0 }
    pub fn iter(&self) -> Iter<'_, K, V> {
        let mut iter = Iter { stack: Vec::new(), back: Vec::new(), remaining: self.len };
        iter.push_left(self.root.as_deref());
        iter.push_right(self.root.as_deref());
        iter
    }
    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &K> + '_ { self.iter().map(|(key, _)| key) }
    pub fn values(&self) -> impl DoubleEndedIterator<Item = &V> + '_ { self.iter().map(|(_, value)| value) }
    /// Nodes one insert can allocate here: the root-to-leaf path, the new leaf
    /// and the two extra nodes of its single (possibly double) rotation.
    pub fn insert_nodes(&self) -> usize { height(&self.root) as usize + 3 }
    /// A removal may rebalance at every level and also path-copies the
    /// successor spine: at most three nodes per level plus the root fixups.
    pub fn remove_nodes(&self) -> usize { 3 * height(&self.root) as usize + 3 }
    pub fn first_key_value(&self) -> Option<(&K, &V)> { self.iter().next() }
    pub fn last_key_value(&self) -> Option<(&K, &V)> { self.iter().next_back() }
    /// Same root: both maps are the identical immutable revision.
    pub fn ptr_eq(&self, other: &Self) -> bool {
        match (&self.root, &other.root) {
            (Some(left), Some(right)) => Arc::ptr_eq(left, right),
            (None, None) => true,
            _ => false,
        }
    }
}

impl<K: Ord, V> PersistentMap<K, V> {
    pub fn get_key_value<Q: Ord + ?Sized>(&self, key: &Q) -> Option<(&K, &V)> where K: Borrow<Q> {
        let mut link = self.root.as_deref();
        while let Some(node) = link {
            match key.cmp(node.key.borrow()) {
                Ordering::Less => link = node.left.as_deref(),
                Ordering::Greater => link = node.right.as_deref(),
                Ordering::Equal => return Some((&node.key, &node.value)),
            }
        }
        None
    }
    pub fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&V> where K: Borrow<Q> {
        self.get_key_value(key).map(|(_, value)| value)
    }
    pub fn contains_key<Q: Ord + ?Sized>(&self, key: &Q) -> bool where K: Borrow<Q> {
        self.get_key_value(key).is_some()
    }
}

impl<K: Ord + Clone, V: Clone> PersistentMap<K, V> {
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let mut old = None;
        self.root = Some(insert(&self.root, key, value, &mut old));
        if old.is_none() { self.len += 1; }
        old
    }
    pub fn remove<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<V> where K: Borrow<Q> {
        let mut old = None;
        let root = remove(&self.root, key, &mut old);
        if old.is_some() {
            self.root = root;
            self.len -= 1;
        }
        old
    }
}

pub struct Iter<'a, K, V> {
    stack: Vec<&'a Node<K, V>>,
    back: Vec<&'a Node<K, V>>,
    remaining: usize,
}
impl<'a, K, V> Iter<'a, K, V> {
    fn push_left(&mut self, mut link: Option<&'a Node<K, V>>) {
        while let Some(node) = link { self.stack.push(node); link = node.left.as_deref(); }
    }
    fn push_right(&mut self, mut link: Option<&'a Node<K, V>>) {
        while let Some(node) = link { self.back.push(node); link = node.right.as_deref(); }
    }
}
impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 { return None; }
        let node = self.stack.pop()?;
        self.push_left(node.right.as_deref());
        self.remaining -= 1;
        Some((&node.key, &node.value))
    }
    fn size_hint(&self) -> (usize, Option<usize>) { (self.remaining, Some(self.remaining)) }
}
impl<K, V> DoubleEndedIterator for Iter<'_, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 { return None; }
        let node = self.back.pop()?;
        self.push_right(node.left.as_deref());
        self.remaining -= 1;
        Some((&node.key, &node.value))
    }
}
impl<K, V> ExactSizeIterator for Iter<'_, K, V> {}

impl<'a, K, V> IntoIterator for &'a PersistentMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter { self.iter() }
}
impl<K: Ord + Clone, V: Clone> FromIterator<(K, V)> for PersistentMap<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(items: I) -> Self {
        let mut map = Self::new();
        for (key, value) in items { map.insert(key, value); }
        map
    }
}
impl<K: Ord + Borrow<Q>, V, Q: Ord + ?Sized> std::ops::Index<&Q> for PersistentMap<K, V> {
    type Output = V;
    fn index(&self, key: &Q) -> &V { self.get(key).expect("key present in PersistentMap") }
}
impl<K: fmt::Debug, V: fmt::Debug> fmt::Debug for PersistentMap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.debug_map().entries(self.iter()).finish() }
}
impl<K: PartialEq, V: PartialEq> PartialEq for PersistentMap<K, V> {
    fn eq(&self, other: &Self) -> bool { self.len == other.len && self.iter().eq(other.iter()) }
}
impl<K: Eq, V: Eq> Eq for PersistentMap<K, V> {}
impl<K: PartialOrd, V: PartialOrd> PartialOrd for PersistentMap<K, V> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { self.iter().partial_cmp(other.iter()) }
}
impl<K: Ord, V: Ord> Ord for PersistentMap<K, V> {
    fn cmp(&self, other: &Self) -> Ordering { self.iter().cmp(other.iter()) }
}

/// Ordered set of unique keys; `push` inserts in sorted position.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PersistentSet<K>(PersistentMap<K, ()>);
impl<K> Default for PersistentSet<K> {
    fn default() -> Self { Self(PersistentMap::default()) }
}
impl<K> PersistentSet<K> {
    pub fn new() -> Self { Self::default() }
    pub fn len(&self) -> usize { self.0.len() }
    pub fn is_empty(&self) -> bool { self.0.is_empty() }
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &K> + ExactSizeIterator + '_ {
        self.0.iter().map(|(key, _)| key)
    }
    pub fn ptr_eq(&self, other: &Self) -> bool { self.0.ptr_eq(&other.0) }
    pub fn insert_nodes(&self) -> usize { self.0.insert_nodes() }
    pub fn remove_nodes(&self) -> usize { self.0.remove_nodes() }
}
impl<K: Ord> PersistentSet<K> {
    pub fn contains<Q: Ord + ?Sized>(&self, key: &Q) -> bool where K: Borrow<Q> { self.0.contains_key(key) }
    pub fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&K> where K: Borrow<Q> {
        self.0.get_key_value(key).map(|(key, _)| key)
    }
}
impl<K: Ord + Clone> PersistentSet<K> {
    pub fn insert(&mut self, key: K) -> bool { self.0.insert(key, ()).is_none() }
    pub fn push(&mut self, key: K) { self.insert(key); }
    pub fn remove<Q: Ord + ?Sized>(&mut self, key: &Q) -> bool where K: Borrow<Q> { self.0.remove(key).is_some() }
}
impl<'a, K> IntoIterator for &'a PersistentSet<K> {
    type Item = &'a K;
    type IntoIter = std::iter::Map<Iter<'a, K, ()>, fn((&'a K, &'a ())) -> &'a K>;
    fn into_iter(self) -> Self::IntoIter { self.0.iter().map(|(key, _)| key) }
}
impl<K: Ord + Clone> FromIterator<K> for PersistentSet<K> {
    fn from_iter<I: IntoIterator<Item = K>>(items: I) -> Self {
        Self(items.into_iter().map(|key| (key, ())).collect())
    }
}
impl<K: fmt::Debug> fmt::Debug for PersistentSet<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.debug_list().entries(self.iter()).finish() }
}
impl<K: PartialEq> PartialEq<Vec<K>> for PersistentSet<K> {
    fn eq(&self, other: &Vec<K>) -> bool { self.len() == other.len() && self.iter().eq(other.iter()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    // Deterministic xorshift sequence; no external randomness.
    fn sequence(seed: u64, count: usize) -> Vec<u64> {
        let mut state = seed;
        (0..count).map(|_| { state ^= state << 13; state ^= state >> 7; state ^= state << 17; state }).collect()
    }

    fn check_balanced<K, V>(link: &Link<K, V>) -> u8 {
        let Some(node) = link else { return 0 };
        let (left, right) = (check_balanced(&node.left), check_balanced(&node.right));
        assert!(left.abs_diff(right) <= 1, "AVL balance");
        assert_eq!(node.height, left.max(right) + 1);
        node.height
    }

    #[test]
    fn matches_btreemap_under_inserts_replacements_and_removals() {
        let mut expected = BTreeMap::new();
        let mut actual = PersistentMap::new();
        let mut revisions = Vec::new();
        for (step, value) in sequence(0x9e37_79b9_7f4a_7c15, 4000).into_iter().enumerate() {
            let key = value % 512;
            if value % 3 == 0 {
                assert_eq!(actual.remove(&key), expected.remove(&key));
            } else {
                assert_eq!(actual.insert(key, step), expected.insert(key, step));
            }
            assert_eq!(actual.len(), expected.len());
            if step % 97 == 0 { revisions.push((actual.clone(), expected.clone())); }
        }
        check_balanced(&actual.root);
        assert!(actual.iter().map(|(k, v)| (*k, *v)).eq(expected.iter().map(|(k, v)| (*k, *v))));
        assert!(actual.iter().rev().map(|(k, v)| (*k, *v)).eq(expected.iter().rev().map(|(k, v)| (*k, *v))));
        let mut both = actual.iter();
        let mut reference = expected.iter();
        while let (Some(front), Some(back)) = (both.next(), both.next_back()) {
            assert_eq!(front, reference.next().unwrap());
            assert_eq!(back, reference.next_back().unwrap());
        }
        for key in 0..512u64 { assert_eq!(actual.get(&key), expected.get(&key)); }
        // Every earlier revision is unchanged by later structural updates.
        for (revision, reference) in revisions {
            check_balanced(&revision.root);
            assert!(revision.iter().map(|(k, v)| (*k, *v)).eq(reference.into_iter()));
        }
    }

    #[test]
    fn sets_keep_sorted_unique_order_and_shared_revisions() {
        let mut set: PersistentSet<u64> = sequence(7, 300).into_iter().map(|v| v % 64).collect();
        let snapshot = set.clone();
        assert!(set.ptr_eq(&snapshot));
        let sorted: Vec<u64> = set.iter().copied().collect();
        assert!(sorted.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(set, sorted);
        set.push(1_000);
        assert!(!set.ptr_eq(&snapshot) && !snapshot.contains(&1_000) && set.contains(&1_000));
        assert!(snapshot < set || snapshot > set);
        assert!(set.insert_nodes() > check_balanced(&set.0.root) as usize);
        assert!(set.remove_nodes() >= 3 * check_balanced(&set.0.root) as usize);
    }
}
