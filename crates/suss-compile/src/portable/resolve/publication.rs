//! Publication of static source declarations, independently of executed effects.
//! No Runtime values, initializer execution or source reconstruction live here.
use super::*;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub(super) struct Log(Arc<Mutex<Option<Vec<Change>>>>);
pub(crate) struct Capture(Log);
impl Capture {
    pub(crate) fn finish(self) -> Publication {
        let changes = self.0.0.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
        Publication { changes }
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        self.0.0.lock().unwrap_or_else(|e| e.into_inner()).take();
    }
}

#[derive(Debug)]
pub(crate) struct Publication { changes: Vec<Change> }
#[derive(Clone, Debug)]
pub(super) enum ScopeChange {
    Alias(String, String, bool),
    Aliases(BTreeMap<String, String>),
    Refer(String, Global, bool),
    MacroRefer(String, (String, String), bool),
    Exclude(String),
}
#[derive(Clone, Debug)]
pub(super) enum Change {
    Bind(Global, Option<Binding>),
    Definition(Global, Arc<DefinitionInfo>),
    Materialize(Global),
    Namespace(Phase, String),
    Enter(Phase, String),
    Reset(Phase, String),
    Scope(Phase, String, ScopeChange),
    MacroDeclaration(Global, Option<Arc<DefinitionInfo>>),
    MacroOwner(Global, Option<MacroOwner>),
    MacroExports(Phase, String, Vec<String>),
    Protocol(Global, Vec<ProtocolMethod>, bool),
    DirectLink(Global, Option<(usize, usize)>),
    ExportHints(BTreeMap<(String, String), Vec<Option<bool>>>),
    Keyword(Global),
}
impl Change {
    fn macro_namespace(&self) -> Option<(Phase, String)> {
        match self {
            Self::MacroDeclaration(g, _) | Self::MacroOwner(g, _) => Some((g.phase, g.namespace.to_string())),
            Self::MacroExports(phase, name, _) => Some((*phase, name.clone())),
            _ => None,
        }
    }
}
impl Publication {
    pub(crate) fn macro_namespaces(&self, meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>) -> Result<BTreeSet<(Phase, String)>, Diagnostic> {
        let mut namespaces = BTreeSet::new();
        for change in &self.changes {
            meter(1)?;
            if let Some(namespace) = change.macro_namespace() {
                meter(namespace.1.len() as u64 + 1)?;
                namespaces.insert(namespace);
            }
        }
        Ok(namespaces)
    }
    pub(crate) fn charge(&self, meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>) -> Result<(), Diagnostic> {
        for change in &self.changes {
            match change {
                Change::Scope(_, namespace, ScopeChange::Aliases(aliases)) => {
                    meter(namespace.len() as u64 + 1)?;
                    for (a, b) in aliases { meter((a.len() + b.len()) as u64 + 1)?; }
                }
                Change::Scope(_, namespace, ScopeChange::Alias(a, b, _)) => meter((namespace.len() + a.len() + b.len()) as u64 + 1)?,
                Change::MacroExports(_, namespace, names) => {
                    meter(namespace.len() as u64 + 1)?;
                    for name in names { meter(name.len() as u64 + 1)?; }
                }
                Change::ExportHints(hints) => {
                    meter(1)?;
                    for ((ns, name), args) in hints { meter((ns.len() + name.len() + args.len()) as u64 + 1)?; }
                }
                Change::Protocol(_, methods, _) => {
                    meter(1)?;
                    for method in methods { meter((method.name.len() + method.signatures.len()) as u64 + 1)?; }
                }
                _ => meter(2)?,
            }
        }
        Ok(())
    }
    pub(crate) fn apply_runtime(&self, environment: &mut Environment, meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>) -> Result<(), Diagnostic> {
        self.apply(environment, false, meter)
    }
    pub(crate) fn apply_catalog(&self, environment: &mut Environment, meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>) -> Result<(), Diagnostic> {
        self.apply(environment, true, meter)
    }
    fn apply(&self, environment: &mut Environment, catalog: bool, meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>) -> Result<(), Diagnostic> {
        let log = environment.publication.take();
        let outcome = (|| {
            for change in &self.changes {
                if change.macro_namespace().is_some() != catalog { continue; }
                meter(1)?;
                match change {
                    Change::Bind(global, Some(binding)) => environment.bind(global.clone(), binding.clone()),
                    Change::Bind(global, None) => environment.unbind(global),
                    Change::Definition(global, info) => { environment.definitions.entry((global.phase, global.namespace.clone())).or_default().insert(global.clone(), info.clone()); }
                    Change::Materialize(global) => environment.materialize_bootstrap(global.clone()),
                    Change::Namespace(phase, name) => { environment.namespaces.insert((*phase, name.clone())); }
                    Change::Enter(phase, name) => {
                        environment.scopes.entry((*phase, name.clone())).or_insert_with(|| Scope::new(name));
                        environment.current.insert(*phase, name.clone());
                    }
                    Change::Reset(phase, name) => { environment.scopes.insert((*phase, name.clone()), Scope::new(name)); }
                    Change::Scope(phase, name, change) => {
                        let scope = environment.scopes.entry((*phase, name.clone())).or_insert_with(|| Scope::new(name));
                        match change {
                            ScopeChange::Alias(alias, target, macros) => { if *macros { &mut scope.macro_aliases } else { &mut scope.aliases }.insert(alias.clone(), target.clone()); }
                            ScopeChange::Aliases(aliases) => scope.aliases = aliases.clone(),
                            ScopeChange::Refer(local, global, renamed) => {
                                scope.refers.insert(local.clone(), global.clone());
                                if *renamed { &mut scope.renamed_refers } else { &mut scope.used_refers }.insert(local.clone());
                            }
                            ScopeChange::MacroRefer(local, target, renamed) => {
                                scope.macro_refers.insert(local.clone(), target.clone());
                                if *renamed { &mut scope.renamed_macro_refers } else { &mut scope.used_macro_refers }.insert(local.clone());
                            }
                            ScopeChange::Exclude(name) => { scope.excluded_core.insert(name.clone()); }
                        }
                    }
                    Change::MacroDeclaration(global, info) => {
                        environment.native_macro_status.remove(global);
                        environment.macro_declarations.insert(global.clone(), info.clone());
                    }
                    Change::MacroOwner(global, owner) => match owner {
                        Some(owner) => { environment.macro_owners.insert(global.clone(), owner.clone()); }
                        None => { environment.macro_owners.remove(global); }
                    },
                    Change::MacroExports(phase, name, names) => {
                        environment.macro_namespaces.insert((*phase, name.clone()));
                        environment.macro_exports.retain(|(p, ns, _)| p != phase || ns != name);
                        environment.macro_exports.extend(names.iter().map(|n| (*phase, name.clone(), n.clone())));
                    }
                    Change::Protocol(global, methods, core) => {
                        environment.protocols.insert(global.clone(), methods.clone());
                        if *core { environment.core_protocols.insert(global.clone()); } else { environment.core_protocols.remove(global); }
                    }
                    Change::DirectLink(global, Some((slot, definition))) => {
                        environment.direct_slots.insert(global.clone(), *slot);
                        environment.direct_links.insert(global.clone(), (*slot, *definition));
                    }
                    Change::DirectLink(global, None) => { environment.direct_links.remove(global); }
                    Change::ExportHints(hints) => environment.export_param_hints = hints.clone(),
                    Change::Keyword(global) => { environment.keyword_cache(global); }
                }
            }
            if !self.changes.is_empty() { environment.source_changed(); }
            Ok(())
        })();
        environment.publication = log;
        outcome
    }
}
impl Environment {
    pub(crate) fn begin_publication(&mut self) -> Capture {
        assert!(self.publication.is_none(), "one source publication per snapshot");
        let log = Log(Arc::new(Mutex::new(Some(Vec::new()))));
        self.publication = Some(log.clone());
        Capture(log)
    }
    pub(crate) fn clear_publication(&mut self) { self.publication = None; }
    pub(super) fn record_publication(&self, change: impl FnOnce() -> Change) {
        if let Some(log) = &self.publication {
            if let Some(changes) = log.0.lock().unwrap_or_else(|e| e.into_inner()).as_mut() { changes.push(change()); }
        }
    }
    pub(super) fn record_scope_publication(&self, phase: Phase, change: impl FnOnce() -> ScopeChange) {
        self.record_publication(|| Change::Scope(phase, self.current[&phase].clone(), change()));
    }
    pub(crate) fn macro_catalog_publication(&self, namespaces: &BTreeSet<(Phase, String)>, meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>) -> Result<Publication, Diagnostic> {
        let mut changes = Vec::new();
        for (phase, namespace) in namespaces {
            meter(namespace.len() as u64 + 1)?;
            let mut names = Vec::new();
            for (p, ns, name) in &self.macro_exports {
                meter(1)?;
                if p == phase && ns == namespace { meter(name.len() as u64 + 1)?; names.push(name.clone()); }
            }
            changes.push(Change::MacroExports(*phase, namespace.clone(), names));
            for (global, info) in self.macro_declarations.iter() {
                meter(1)?;
                if global.phase != *phase || global.namespace.as_ref() != namespace { continue; }
                changes.push(Change::MacroDeclaration(global.clone(), info.clone()));
                changes.push(Change::MacroOwner(global.clone(), self.macro_owners.get(global).cloned()));
            }
        }
        Ok(Publication { changes })
    }
    pub(crate) fn reserve_prepared_identities(&mut self, prepared: &Self) {
        self.source_generation = self.source_generation.max(prepared.source_generation);
        self.direct_link_count = self.direct_link_count.max(prepared.direct_link_count);
        self.reader_state.reserve(&prepared.reader_state);
    }
    pub(crate) fn set_protocol(&mut self, global: Global, methods: Vec<ProtocolMethod>, core: bool) {
        self.record_publication(|| Change::Protocol(global.clone(), methods.clone(), core));
        self.protocols.insert(global.clone(), methods);
        if core { self.core_protocols.insert(global); } else { self.core_protocols.remove(&global); }
    }
}
