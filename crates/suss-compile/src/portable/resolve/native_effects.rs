//! Merge declarations actually published by reentrant native compilation.
//! The enclosing analyzer retains its provisional declarations and input scope.
use super::*;

fn map_changes<K: Ord + Clone, V: Clone + PartialEq>(
    target: &mut BTreeMap<K, V>, before: &BTreeMap<K, V>, after: &BTreeMap<K, V>,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    for (key, value) in after {
        meter(1)?;
        if before.get(key) != Some(value) { target.insert(key.clone(), value.clone()); }
    }
    for key in before.keys() {
        meter(1)?;
        if !after.contains_key(key) { target.remove(key); }
    }
    Ok(())
}
fn set_changes<K: Ord + Clone>(
    target: &mut BTreeSet<K>, before: &BTreeSet<K>, after: &BTreeSet<K>,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    for key in after { meter(1)?; if !before.contains(key) { target.insert(key.clone()); } }
    for key in before { meter(1)?; if !after.contains(key) { target.remove(key); } }
    Ok(())
}
impl Environment {
    /// Apply only changes executed during a transformer, rather than replacing
    /// the outer compilation's staged environment with a live-state snapshot.
    pub(crate) fn apply_native_effects(
        &mut self, before: &Self, after: &Self,
        meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
    ) -> Result<(), Diagnostic> {
        if before.source_generation == after.source_generation { return Ok(()); }
        // These effects already executed in the caller, rather than belonging
        // to the source declarations awaiting a later Runtime input.
        let publication = self.publication.take();
        let outcome = (|| {
        set_changes(&mut self.namespaces, &before.namespaces, &after.namespaces, meter)?;
        for (key, scope) in &after.scopes {
            meter(1)?;
            let empty = Scope::new(&scope.namespace);
            let old = before.scopes.get(key).unwrap_or(&empty);
            let target = self.scopes.entry(key.clone()).or_insert_with(|| old.clone());
            map_changes(&mut target.aliases, &old.aliases, &scope.aliases, meter)?;
            map_changes(&mut target.refers, &old.refers, &scope.refers, meter)?;
            map_changes(&mut target.macro_aliases, &old.macro_aliases, &scope.macro_aliases, meter)?;
            map_changes(&mut target.macro_refers, &old.macro_refers, &scope.macro_refers, meter)?;
            set_changes(&mut target.used_refers, &old.used_refers, &scope.used_refers, meter)?;
            set_changes(&mut target.renamed_refers, &old.renamed_refers, &scope.renamed_refers, meter)?;
            set_changes(&mut target.excluded_core, &old.excluded_core, &scope.excluded_core, meter)?;
            set_changes(&mut target.used_macro_refers, &old.used_macro_refers, &scope.used_macro_refers, meter)?;
            set_changes(&mut target.renamed_macro_refers, &old.renamed_macro_refers, &scope.renamed_macro_refers, meter)?;
        }
        for (global, binding) in &after.bindings {
            meter(1)?;
            if before.bindings.get(global) != Some(binding) { self.bind(global.clone(), binding.clone()); }
        }
        for global in before.bindings.keys() {
            meter(1)?;
            if !after.bindings.contains_key(global) { self.unbind(global); }
        }
        for (namespace, definitions) in &after.definitions {
            for (global, revision) in definitions.iter() {
                meter(1)?;
                if !before.shared_definition_info(global).is_some_and(|old| std::sync::Arc::ptr_eq(old, revision)) {
                    self.definitions.entry(namespace.clone()).or_default().insert(global.clone(), revision.clone());
                }
            }
        }
        for (namespace, definitions) in &before.definitions {
            for global in definitions.keys() {
                meter(1)?;
                if after.shared_definition_info(global).is_none() {
                    if let Some(target) = self.definitions.get_mut(namespace) { target.remove(global); }
                }
            }
        }
        for (global, revision) in after.macro_declarations.iter() {
            meter(1)?;
            let same = match (before.macro_declarations.get(global), revision) {
                (Some(None), None) => true,
                (Some(Some(old)), Some(new)) => std::sync::Arc::ptr_eq(old, new),
                _ => false,
            };
            if !same { self.macro_declarations.insert(global.clone(), revision.clone()); }
        }
        for global in before.macro_declarations.keys() {
            meter(1)?;
            if !after.macro_declarations.contains_key(global) { self.macro_declarations.remove(global); }
        }
        set_changes(&mut self.materialized_bootstrap, &before.materialized_bootstrap, &after.materialized_bootstrap, meter)?;
        set_changes(&mut self.macro_exports, &before.macro_exports, &after.macro_exports, meter)?;
        map_changes(&mut self.macro_owners, &before.macro_owners, &after.macro_owners, meter)?;
        set_changes(&mut self.macro_namespaces, &before.macro_namespaces, &after.macro_namespaces, meter)?;
        map_changes(&mut self.protocols, &before.protocols, &after.protocols, meter)?;
        set_changes(&mut self.core_protocols, &before.core_protocols, &after.core_protocols, meter)?;
        map_changes(&mut self.export_param_hints, &before.export_param_hints, &after.export_param_hints, meter)?;
        for cell in after.keyword_caches.keys() {
            meter(1)?;
            if !before.keyword_caches.contains_key(cell) { self.keyword_cache(cell); }
        }
        // Reader gensyms, the selected input namespace, analysis-only macro
        // observations and AOT linking policy belong to the enclosing input.
        // Native execution publishes live cells/declarations, never AOT slots.
        self.source_changed();
        Ok(())
        })();
        self.publication = publication;
        outcome
    }
}
