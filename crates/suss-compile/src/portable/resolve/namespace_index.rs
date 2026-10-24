//! Live per-namespace declaration/identity revisions, maintained as bindings,
//! definitions and bootstrap materializations change. A source snapshot shares
//! these structurally instead of rescanning every Environment binding.
//!
//! Membership is exactly the full scan's: a declaration is a definitions entry
//! of that phase and namespace; an identity is the ACTUAL Global carried by a
//! Cell/InternalCell/ReaderCell binding value, or materialized for bootstrap,
//! that is not hidden by its own canonical binding.
use super::{Binding, DefinitionInfo, Environment, Global, GlobalKey, Phase, Query, canonical};
use crate::portable::persistent::{PersistentMap, PersistentSet};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug, Clone, Default)]
pub(crate) struct NamespaceIndex {
    pub(crate) declarations: PersistentMap<Global, Arc<DefinitionInfo>>,
    pub(crate) identities: PersistentSet<Global>,
}

/// Indexed revisions by phase and exact Global namespace text.
#[derive(Debug, Clone, Default)]
pub(crate) struct NamespaceIndexes(BTreeMap<Phase, BTreeMap<String, NamespaceIndex>>);

impl NamespaceIndexes {
    pub(crate) fn get(&self, phase: Phase, namespace: &str) -> Option<&NamespaceIndex> {
        self.0.get(&phase)?.get(namespace)
    }
    fn entry(&mut self, phase: Phase, namespace: &str) -> &mut NamespaceIndex {
        let namespaces = self.0.entry(phase).or_default();
        if !namespaces.contains_key(namespace) {
            namespaces.insert(namespace.to_owned(), NamespaceIndex::default());
        }
        namespaces.get_mut(namespace).expect("inserted namespace index")
    }
}

/// Native work units for one structural update path-copying `nodes` nodes:
/// allocate and later retire each copy, and compare the key's text at each.
fn update_work<V>(nodes: usize, namespace: &str, name: &str) -> u64 {
    let node = std::mem::size_of::<(Global, V, usize, usize, u8)>() + 2 * std::mem::size_of::<usize>();
    let compare = 2 * (namespace.len() + name.len()) + 8;
    (nodes * (2 * node + compare)) as u64
}

pub(super) fn cell_global(binding: &Binding) -> Option<&Global> {
    match binding {
        Binding::Cell(global) | Binding::InternalCell(global) | Binding::ReaderCell(global) => Some(global),
        _ => None,
    }
}

impl Environment {
    /// Structural update of one identity revision: `insert` and/or `remove`.
    fn identity_work(&self, phase: Phase, namespace: &str, name: &str, insert: bool, remove: bool) -> u64 {
        let identities = self.namespace_index.get(phase, namespace).map(|index| &index.identities);
        let nodes = usize::from(insert) * identities.map_or(3, |set| set.insert_nodes())
            + usize::from(remove) * identities.map_or(3, |set| set.remove_nodes());
        // A first entry also allocates that namespace's index key text.
        update_work::<()>(nodes, namespace, name) + namespace.len() as u64
    }
    /// Work to decide one identity's membership after a binding change: three
    /// keyed hash queries (cell values, bootstrap, hiding binding) and the
    /// index probe, plus the structural update only when membership can change.
    fn planned_identity_work(&self, phase: Phase, namespace: &str, name: &str, key: &Global,
        value: Option<&Binding>, removed: Option<&Global>, added: Option<&Global>) -> u64 {
        let check = 4 * (namespace.len() + name.len() + 16) as u64;
        let query = Query { phase, namespace, name, compare: None };
        let query = &query as &dyn GlobalKey;
        let same = |global: Option<&Global>| global.is_some_and(|global| (global as &dyn GlobalKey) == query);
        let count = self.cell_refs.get(query).copied().unwrap_or(0) + usize::from(same(added)) - usize::from(same(removed));
        let hidden = if key.phase() == phase && key.namespace() == canonical(namespace) && key.name() == name {
            matches!(value, Some(Binding::ReaderCell(_) | Binding::InternalCell(_)))
        } else {
            matches!(
                self.bindings.get(&Query { phase, namespace: canonical(namespace), name, compare: None } as &dyn GlobalKey),
                Some(Binding::ReaderCell(_) | Binding::InternalCell(_))
            )
        };
        let member = (count > 0 || self.materialized_bootstrap.contains(query)) && !hidden;
        let present = self.namespace_index.get(phase, namespace).is_some_and(|index| index.identities.contains(query));
        // A removed carrier may also change which equal-valued owner is retained.
        let replace = member && present && same(removed);
        if member != present || replace {
            check + self.identity_work(phase, namespace, name, member, present)
        } else { check }
    }
    /// Exact-owner identical replacement: nothing observable changes.
    fn unchanged_binding(&self, key: &Global, value: Option<&Binding>) -> bool {
        match (self.bindings.get(key), value) {
            (Some(old), Some(new)) => old == new && match (cell_global(old), cell_global(new)) {
                (Some(old), Some(new)) => Arc::ptr_eq(&old.0, &new.0),
                (None, None) => true,
                _ => false,
            },
            (None, None) => true,
            _ => false,
        }
    }
    /// Work for set_binding/remove_binding of `key`, computed BEFORE any
    /// publication so metered callers can prepay it and fail without effects.
    pub(super) fn binding_update_work(&self, key: &Global, value: Option<&Binding>) -> u64 {
        let lookup = (key.namespace().len() + key.name().len() + 16) as u64;
        if self.unchanged_binding(key, value) { return lookup; }
        let removed = self.bindings.get(key).and_then(cell_global);
        let added = value.and_then(cell_global);
        let mut work = lookup;
        for global in removed.into_iter().chain(added) {
            work += self.planned_identity_work(global.phase(), global.namespace(), global.name(), key, value, removed, added);
        }
        work += self.planned_identity_work(key.phase(), key.namespace(), key.name(), key, value, removed, added);
        if key.namespace() == "suss.core" {
            work += self.planned_identity_work(key.phase(), "cljs.core", key.name(), key, value, removed, added);
        }
        work
    }
    pub(super) fn bootstrap_update_work(&self, global: &Global) -> u64 {
        // Materialization can only add or re-own a visible identity.
        self.identity_work(global.phase(), global.namespace(), global.name(), true, true)
    }
    pub(super) fn definition_update_work(&self, global: &Global) -> u64 {
        let nodes = self.namespace_index.get(global.phase(), global.namespace()).map_or(3, |index| index.declarations.insert_nodes());
        update_work::<Arc<DefinitionInfo>>(nodes, global.namespace(), global.name()) + global.namespace().len() as u64
    }
    /// Replace one binding and refresh every identity whose membership it can
    /// change. Metered callers prepay binding_update_work first.
    pub(super) fn set_binding(&mut self, key: Global, value: Binding) {
        if self.unchanged_binding(&key, Some(&value)) { return; }
        let added = cell_global(&value).cloned();
        let previous = self.bindings.insert(key.clone(), value);
        self.binding_changed(&key, previous.as_ref().and_then(cell_global).cloned(), added);
    }
    pub(super) fn remove_binding(&mut self, key: &Global) {
        if let Some(previous) = self.bindings.remove(key) {
            self.binding_changed(key, cell_global(&previous).cloned(), None);
        }
    }
    fn binding_changed(&mut self, key: &Global, removed: Option<Global>, added: Option<Global>) {
        if let Some(global) = &removed {
            let count = self.cell_refs.get_mut(global).expect("counted cell binding value");
            *count -= 1;
            if *count == 0 { self.cell_refs.remove(global); }
        }
        if let Some(global) = &added { *self.cell_refs.entry(global.clone()).or_default() += 1; }
        for global in removed.iter().chain(&added) {
            self.refresh_identity(global.phase(), global.namespace(), global.name());
        }
        // The key's own variant decides hiding for every same-named identity
        // whose namespace canonicalizes to the key's namespace.
        self.refresh_identity(key.phase(), key.namespace(), key.name());
        if key.namespace() == "suss.core" {
            self.refresh_identity(key.phase(), "cljs.core", key.name());
        }
    }
    pub(super) fn index_bootstrap(&mut self, global: &Global) {
        self.refresh_identity(global.phase(), global.namespace(), global.name());
    }
    /// Recompute one identity's membership from the actual live sources.
    fn refresh_identity(&mut self, phase: Phase, namespace: &str, name: &str) {
        let query = Query { phase, namespace, name, compare: None };
        let query = &query as &dyn GlobalKey;
        let owner = self.cell_refs.get_key_value(query).map(|(global, _)| global)
            .or_else(|| self.materialized_bootstrap.get(query)).cloned();
        let hidden = matches!(
            self.bindings.get(&Query { phase, namespace: canonical(namespace), name, compare: None } as &dyn GlobalKey),
            Some(Binding::ReaderCell(_) | Binding::InternalCell(_))
        );
        match owner.filter(|_| !hidden) {
            Some(owner) => {
                let index = self.namespace_index.entry(phase, namespace);
                if !index.identities.contains_owner(&owner) {
                    index.identities.remove(&owner);
                    index.identities.insert(owner);
                }
            }
            None => {
                if let Some(index) = self.namespace_index.0.get_mut(&phase).and_then(|indexes| indexes.get_mut(namespace)) {
                    index.identities.remove(query);
                }
            }
        }
    }
    pub(super) fn set_definition(&mut self, global: Global, info: Arc<DefinitionInfo>) {
        // Both keep the established key owner on replacement.
        self.namespace_index.entry(global.phase(), global.namespace()).declarations.insert(global.clone(), info.clone());
        self.definitions.insert(global, info);
    }
    pub(super) fn remove_definition(&mut self, global: &Global) {
        if self.definitions.remove(global).is_some() {
            self.namespace_index.entry(global.phase(), global.namespace()).declarations.remove(global);
        }
    }
    pub(crate) fn namespace_index(&self, phase: Phase, namespace: &str) -> Option<&NamespaceIndex> {
        self.namespace_index.get(phase, namespace)
    }
}

trait OwnerSet {
    fn contains_owner(&self, owner: &Global) -> bool;
}
impl OwnerSet for PersistentSet<Global> {
    /// Present with this exact owner, not merely an equal identity value.
    fn contains_owner(&self, owner: &Global) -> bool {
        self.get(owner).is_some_and(|present| Arc::ptr_eq(&present.0, &owner.0))
    }
}
