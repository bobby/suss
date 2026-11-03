//! A persistent ordered map: a clone shares every node, and a change copies
//! only the path to the changed key (#319). Namespace snapshots hold one per
//! namespace, so taking a snapshot costs a reference count instead of a copy.
use std::{borrow::Borrow, cmp::Ordering, fmt, sync::Arc};

type Link<K, V> = Option<Arc<Node<K, V>>>;

struct Node<K, V> {
    key: K,
    value: V,
    height: u8,
    left: Link<K, V>,
    right: Link<K, V>,
}

fn height<K, V>(link: &Link<K, V>) -> u8 {
    link.as_ref().map_or(0, |node| node.height)
}

/// A node over `left` and `right`, rotated once if their heights differ by two.
fn balance<K: Clone, V: Clone>(key: K, value: V, left: Link<K, V>, right: Link<K, V>) -> Arc<Node<K, V>> {
    let node = |key, value, left: Link<K, V>, right: Link<K, V>| {
        let height = height(&left).max(height(&right)) + 1;
        Arc::new(Node { key, value, height, left, right })
    };
    let (lh, rh) = (height(&left), height(&right));
    if lh > rh + 1 {
        let l = left.expect("taller side");
        if height(&l.left) >= height(&l.right) {
            node(l.key.clone(), l.value.clone(), l.left.clone(), Some(node(key, value, l.right.clone(), right)))
        } else {
            let lr = l.right.as_ref().expect("taller inner side");
            node(
                lr.key.clone(),
                lr.value.clone(),
                Some(node(l.key.clone(), l.value.clone(), l.left.clone(), lr.left.clone())),
                Some(node(key, value, lr.right.clone(), right)),
            )
        }
    } else if rh > lh + 1 {
        let r = right.expect("taller side");
        if height(&r.right) >= height(&r.left) {
            node(r.key.clone(), r.value.clone(), Some(node(key, value, left, r.left.clone())), r.right.clone())
        } else {
            let rl = r.left.as_ref().expect("taller inner side");
            node(
                rl.key.clone(),
                rl.value.clone(),
                Some(node(key, value, left, rl.left.clone())),
                Some(node(r.key.clone(), r.value.clone(), rl.right.clone(), r.right.clone())),
            )
        }
    } else {
        node(key, value, left, right)
    }
}

fn insert<K: Ord + Clone, V: Clone>(link: &Link<K, V>, key: K, value: V, replaced: &mut bool) -> Arc<Node<K, V>> {
    let Some(node) = link else {
        return Arc::new(Node { key, value, height: 1, left: None, right: None });
    };
    match key.cmp(&node.key) {
        Ordering::Less => {
            let left = insert(&node.left, key, value, replaced);
            balance(node.key.clone(), node.value.clone(), Some(left), node.right.clone())
        }
        Ordering::Greater => {
            let right = insert(&node.right, key, value, replaced);
            balance(node.key.clone(), node.value.clone(), node.left.clone(), Some(right))
        }
        Ordering::Equal => {
            *replaced = true;
            Arc::new(Node { key, value, height: node.height, left: node.left.clone(), right: node.right.clone() })
        }
    }
}

/// The subtree without its least entry, and that entry.
fn remove_min<K: Clone, V: Clone>(node: &Node<K, V>) -> (Link<K, V>, K, V) {
    match &node.left {
        None => (node.right.clone(), node.key.clone(), node.value.clone()),
        Some(left) => {
            let (left, key, value) = remove_min(left);
            (Some(balance(node.key.clone(), node.value.clone(), left, node.right.clone())), key, value)
        }
    }
}

/// The subtree without `key`, or `None` when `key` is absent.
fn remove<K, V, Q>(link: &Link<K, V>, key: &Q) -> Option<Link<K, V>>
where
    K: Ord + Clone + Borrow<Q>,
    V: Clone,
    Q: Ord + ?Sized,
{
    let node = link.as_ref()?;
    Some(match key.cmp(node.key.borrow()) {
        Ordering::Less => {
            let left = remove(&node.left, key)?;
            Some(balance(node.key.clone(), node.value.clone(), left, node.right.clone()))
        }
        Ordering::Greater => {
            let right = remove(&node.right, key)?;
            Some(balance(node.key.clone(), node.value.clone(), node.left.clone(), right))
        }
        Ordering::Equal => match (&node.left, &node.right) {
            (None, right) => right.clone(),
            (left, None) => left.clone(),
            (left, Some(right)) => {
                let (right, key, value) = remove_min(right);
                Some(balance(key, value, left.clone(), right))
            }
        },
    })
}

pub struct PersistentMap<K, V> {
    root: Link<K, V>,
    len: usize,
}

impl<K, V> Clone for PersistentMap<K, V> {
    fn clone(&self) -> Self {
        Self { root: self.root.clone(), len: self.len }
    }
}
impl<K, V> Default for PersistentMap<K, V> {
    fn default() -> Self {
        Self { root: None, len: 0 }
    }
}

impl<K: Ord + Clone, V: Clone> PersistentMap<K, V> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        let mut link = &self.root;
        while let Some(node) = link {
            match key.cmp(node.key.borrow()) {
                Ordering::Less => link = &node.left,
                Ordering::Greater => link = &node.right,
                Ordering::Equal => return Some(&node.value),
            }
        }
        None
    }
    pub fn contains_key<Q: Ord + ?Sized>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.get(key).is_some()
    }
    pub fn insert(&mut self, key: K, value: V) {
        let mut replaced = false;
        self.root = Some(insert(&self.root, key, value, &mut replaced));
        if !replaced {
            self.len += 1;
        }
    }
    /// Whether `key` was present.
    pub fn remove<Q: Ord + ?Sized>(&mut self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        match remove(&self.root, key) {
            Some(root) => {
                self.root = root;
                self.len -= 1;
                true
            }
            None => false,
        }
    }
    pub fn iter(&self) -> Iter<'_, K, V> {
        let mut iter = Iter { stack: Vec::new(), remaining: self.len };
        iter.descend(&self.root);
        iter
    }
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(key, _)| key)
    }
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, value)| value)
    }
}

/// In-order iteration over a stack of nodes whose left subtrees are done.
pub struct Iter<'a, K, V> {
    stack: Vec<&'a Node<K, V>>,
    remaining: usize,
}
impl<'a, K, V> Iter<'a, K, V> {
    fn descend(&mut self, mut link: &'a Link<K, V>) {
        while let Some(node) = link {
            self.stack.push(node);
            link = &node.left;
        }
    }
}
impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        let node = self.stack.pop()?;
        self.descend(&node.right);
        self.remaining -= 1;
        Some((&node.key, &node.value))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl<K, V> ExactSizeIterator for Iter<'_, K, V> {}

impl<'a, K: Ord + Clone, V: Clone> IntoIterator for &'a PersistentMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<K: Ord + Clone, V: Clone> FromIterator<(K, V)> for PersistentMap<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(entries: I) -> Self {
        let mut map = Self::new();
        for (key, value) in entries {
            map.insert(key, value);
        }
        map
    }
}
impl<K: Ord + Clone + Borrow<Q>, V: Clone, Q: Ord + ?Sized> std::ops::Index<&Q> for PersistentMap<K, V> {
    type Output = V;
    fn index(&self, key: &Q) -> &V {
        self.get(key).expect("key in persistent map")
    }
}
impl<K: Ord + Clone + fmt::Debug, V: Clone + fmt::Debug> fmt::Debug for PersistentMap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}
impl<K: Ord + Clone, V: Clone + PartialEq> PartialEq for PersistentMap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.iter().eq(other.iter())
    }
}
impl<K: Ord + Clone, V: Clone + Eq> Eq for PersistentMap<K, V> {}

/// A persistent ordered set, as a map to `()`.
pub struct PersistentSet<K>(PersistentMap<K, ()>);

impl<K> Clone for PersistentSet<K> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<K> Default for PersistentSet<K> {
    fn default() -> Self {
        Self(PersistentMap::default())
    }
}
impl<K: Ord + Clone> PersistentSet<K> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn contains<Q: Ord + ?Sized>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.0.contains_key(key)
    }
    /// Whether `key` was absent. A present key leaves the set unchanged.
    pub fn insert(&mut self, key: K) -> bool {
        let absent = !self.contains(&key);
        if absent {
            self.0.insert(key, ());
        }
        absent
    }
    pub fn remove<Q: Ord + ?Sized>(&mut self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.0.remove(key)
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &K> {
        self.0.iter().map(|(key, _)| key)
    }
}
impl<'a, K: Ord + Clone> IntoIterator for &'a PersistentSet<K> {
    type Item = &'a K;
    type IntoIter = std::iter::Map<Iter<'a, K, ()>, fn((&'a K, &'a ())) -> &'a K>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(|(key, _)| key)
    }
}
impl<K: Ord + Clone> FromIterator<K> for PersistentSet<K> {
    fn from_iter<I: IntoIterator<Item = K>>(keys: I) -> Self {
        Self(keys.into_iter().map(|key| (key, ())).collect())
    }
}
impl<K: Ord + Clone + fmt::Debug> fmt::Debug for PersistentSet<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}
impl<K: Ord + Clone> PartialEq for PersistentSet<K> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<K: Ord + Clone> Eq for PersistentSet<K> {}
impl<K: Ord + Clone> PartialOrd for PersistentSet<K> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<K: Ord + Clone> Ord for PersistentSet<K> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.iter().cmp(other.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Checks heights, balance and order, and returns the node count.
    fn check<K: Ord, V>(link: &Link<K, V>, low: Option<&K>, high: Option<&K>) -> usize {
        let Some(node) = link else { return 0 };
        assert!(low.is_none_or(|low| *low < node.key) && high.is_none_or(|high| node.key < *high));
        let (lh, rh) = (height(&node.left), height(&node.right));
        assert!(lh.abs_diff(rh) <= 1, "unbalanced");
        assert_eq!(node.height, lh.max(rh) + 1);
        check(&node.left, low, Some(&node.key)) + 1 + check(&node.right, Some(&node.key), high)
    }

    #[test]
    fn matches_btree_map_and_keeps_old_versions() {
        let mut map = PersistentMap::new();
        let mut model = BTreeMap::new();
        let mut versions = Vec::new();
        // A fixed pseudo-random sequence of inserts, replacements and removals.
        let mut seed = 0x2545_f491_u32;
        for step in 0..4000 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let key = seed % 600;
            if seed % 3 == 0 {
                assert_eq!(map.remove(&key), model.remove(&key).is_some());
            } else {
                map.insert(key, step);
                model.insert(key, step);
            }
            assert_eq!(check(&map.root, None, None), model.len());
            assert_eq!(map.len(), model.len());
            if step % 500 == 0 {
                versions.push((map.clone(), model.clone()));
            }
        }
        assert!(map.iter().map(|(k, v)| (*k, *v)).eq(model.iter().map(|(k, v)| (*k, *v))));
        for key in 0..600 {
            assert_eq!(map.get(&key), model.get(&key));
        }
        for (map, model) in versions {
            assert!(map.iter().map(|(k, v)| (*k, *v)).eq(model.into_iter()));
        }
    }
}
