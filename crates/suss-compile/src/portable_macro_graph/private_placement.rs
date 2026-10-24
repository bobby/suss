//! Static callback-argument exclusion for dedicated metadata aliases.
//! This proves placement only, not constructor safety, freshness or source facts.
use super::{AnalysisGraph, Recipe, Result, Value};
use crate::{
    portable_macro_data::MetadataShape,
    portable_session::{Session, SessionError},
};

fn invalid(message: &'static str) -> SessionError {
    wasmtime::Error::msg(message).into()
}
fn reserve<T>(session: &mut Session, values: &mut Vec<T>, count: usize) -> Result<()> {
    session.charge_host_work(1)?;
    if count > values.capacity() - values.len() {
        let needed = values
            .len()
            .checked_add(count)
            .ok_or_else(|| invalid("Private placement overflow"))?;
        let capacity = values
            .capacity()
            .checked_mul(2)
            .unwrap_or(needed)
            .max(needed);
        let bytes = capacity
            .checked_add(values.len())
            .and_then(|n| n.checked_mul(std::mem::size_of::<T>()))
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| invalid("Private placement allocation overflow"))?;
        session.charge_host_work(bytes)?;
        values
            .try_reserve_exact(capacity - values.len())
            .map_err(|_| invalid("Private placement allocation failed"))?;
    }
    Ok(())
}
fn edges(session: &mut Session, recipe: &Recipe) -> Result<Vec<Value>> {
    let mut result = Vec::new();
    let count = match recipe {
        Recipe::Alias(_) => 1,
        Recipe::List(v)
        | Recipe::Vector(v)
        | Recipe::ReaderVector(v)
        | Recipe::Set(v)
        | Recipe::ReaderSet(v) => v.len(),
        Recipe::Map(v) | Recipe::ReaderMap(v) => v
            .len()
            .checked_mul(2)
            .ok_or_else(|| invalid("Private placement edge overflow"))?,
        Recipe::Metadata { set_items, .. } => set_items
            .len()
            .checked_add(2)
            .ok_or_else(|| invalid("Private placement edge overflow"))?,
        Recipe::Scalar(_) | Recipe::Identifier { .. } | Recipe::Form(_) => 0,
    };
    reserve(session, &mut result, count)?;
    session.charge_host_work(count as u64)?;
    match recipe {
        Recipe::Alias(id) => result.push(*id),
        Recipe::List(v)
        | Recipe::Vector(v)
        | Recipe::ReaderVector(v)
        | Recipe::Set(v)
        | Recipe::ReaderSet(v) => result.extend(v),
        Recipe::Map(v) | Recipe::ReaderMap(v) => {
            for (key, value) in v {
                result.extend([*key, *value]);
            }
        }
        Recipe::Metadata {
            value,
            metadata,
            set_items,
            ..
        } => {
            result.push(*value);
            result.extend(set_items);
            result.push(*metadata);
        }
        Recipe::Scalar(_) | Recipe::Identifier { .. } | Recipe::Form(_) => {}
    }
    Ok(result)
}
impl AnalysisGraph<'_> {
    /// The controller must additionally use callback-free parent constructors.
    /// A false result declines this temporary-value strategy; it is not a claim
    /// that an otherwise legitimate source form is invalid.
    pub(super) fn private_metadata_placement(&mut self, root: Value) -> Result<bool> {
        let _request = self.session.operation_budget_scope()?;
        self.finish_recipes()?;
        let mut aliases = Vec::new();
        reserve(self.session, &mut aliases, self.metadata_sources.len())?;
        for source in &self.metadata_sources {
            self.session.charge_host_work(1)?;
            aliases.push(source.recipe);
        }
        self.private_metadata_placement_for(root, &aliases)
    }
    fn private_metadata_placement_for(&mut self, root: Value, aliases: &[Value]) -> Result<bool> {
        self.private_placement_for(root, aliases, false)
    }
    pub(super) fn private_source_placement_for(
        &mut self,
        root: Value,
        aliases: &[Value],
    ) -> Result<bool> {
        self.private_placement_for(root, aliases, true)
    }
    fn private_placement_for(
        &mut self,
        root: Value,
        aliases: &[Value],
        source_inputs: bool,
    ) -> Result<bool> {
        let _request = self.session.operation_budget_scope()?;
        let count = self.recipes.len();
        let mut states = Vec::new();
        let mut tainted = Vec::new();
        reserve(self.session, &mut states, count)?;
        reserve(self.session, &mut tainted, count)?;
        self.session
            .charge_host_work((count as u64).saturating_mul(2))?;
        states.resize(count, 0u8);
        tainted.resize(count, false);
        for &alias in aliases {
            self.session.charge_host_work(1)?;
            if !matches!(self.recipes.get(alias), Some(Some(Recipe::Alias(_)))) {
                return Err(invalid("Private metadata root needs a dedicated alias"));
            }
            tainted[alias] = true;
        }
        let mut pending = Vec::new();
        reserve(self.session, &mut pending, 1)?;
        pending.push((root, false));
        let mut valid = true;
        while let Some((id, exit)) = pending.pop() {
            self.session.charge_host_work(1)?;
            let recipe = self
                .recipes
                .get(id)
                .and_then(Option::as_ref)
                .ok_or_else(|| invalid("Missing private placement recipe"))?;
            if !exit && states[id] == 2 {
                continue;
            }
            let children = edges(self.session, recipe)?;
            if exit {
                for &child in &children {
                    self.session.charge_host_work(1)?;
                    let child_taint = tainted[child];
                    tainted[id] |= child_taint;
                }
                // with_form_metadata inspects its base's class/fields. A raw
                // source-root placeholder cannot stand in for that base. This
                // restriction is separate from guest hash/equality exposure.
                if source_inputs && let Recipe::Metadata { value, .. } = recipe {
                    self.session.charge_host_work(1)?;
                    valid &= !tainted[*value];
                }
                // FormBridge passes keys to hash/key-test, and set elements to
                // hash/equality. Exclude their entire transitive argument graph.
                match recipe {
                    Recipe::Map(entries) | Recipe::ReaderMap(entries) => {
                        for (key, _) in entries {
                            self.session.charge_host_work(1)?;
                            valid &= !tainted[*key];
                        }
                    }
                    Recipe::Set(items) | Recipe::ReaderSet(items) => {
                        for item in items {
                            self.session.charge_host_work(1)?;
                            valid &= !tainted[*item];
                        }
                    }
                    Recipe::Metadata {
                        shape: MetadataShape::Set,
                        set_items,
                        ..
                    } => {
                        for item in set_items {
                            self.session.charge_host_work(1)?;
                            valid &= !tainted[*item];
                        }
                    }
                    Recipe::Scalar(_)
                    | Recipe::Identifier { .. }
                    | Recipe::Form(_)
                    | Recipe::Alias(_)
                    | Recipe::List(_)
                    | Recipe::Vector(_)
                    | Recipe::ReaderVector(_)
                    | Recipe::Metadata {
                        shape:
                            MetadataShape::Symbol
                            | MetadataShape::List { .. }
                            | MetadataShape::Vector
                            | MetadataShape::Map { .. },
                        ..
                    } => {}
                }
                states[id] = 2;
            } else {
                if states[id] == 1 {
                    return Err(invalid("Cyclic private placement recipes"));
                }
                states[id] = 1;
                reserve(
                    self.session,
                    &mut pending,
                    children
                        .len()
                        .checked_add(1)
                        .ok_or_else(|| invalid("Private placement stack overflow"))?,
                )?;
                pending.push((id, true));
                for child in children.into_iter().rev() {
                    self.session.charge_host_work(1)?;
                    if child >= count {
                        return Err(invalid("Private placement dependency out of bounds"));
                    }
                    pending.push((child, false));
                }
            }
        }
        Ok(valid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{portable::hir::Literal, portable_macro_data::FormBridge};
    #[test]
    fn private_metadata_placement_rejects_transitive_hash_arguments() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let nil = graph.recipe(Recipe::Scalar(Literal::Nil)).unwrap();
        let alias = graph.recipe(Recipe::Alias(nil)).unwrap();
        let nested = graph.recipe(Recipe::Vector(vec![alias])).unwrap();
        let safe = graph.recipe(Recipe::Map(vec![(nil, nested)])).unwrap();
        assert!(
            graph
                .private_metadata_placement_for(safe, &[alias])
                .unwrap()
        );
        let shared = graph.recipe(Recipe::Vector(vec![nested, nested])).unwrap();
        let shared_key = graph.recipe(Recipe::Map(vec![(shared, nil)])).unwrap();
        assert!(
            !graph
                .private_metadata_placement_for(shared_key, &[alias])
                .unwrap()
        );
        for recipe in [
            Recipe::Map(vec![(nested, nil)]),
            Recipe::ReaderMap(vec![(nested, nil)]),
            Recipe::Set(vec![nested]),
            Recipe::ReaderSet(vec![nested]),
            Recipe::Metadata {
                shape: MetadataShape::Set,
                value: nil,
                metadata: nil,
                set_items: vec![nested],
            },
        ] {
            let root = graph.recipe(recipe).unwrap();
            assert!(
                !graph
                    .private_metadata_placement_for(root, &[alias])
                    .unwrap()
            );
        }
    }
    #[test]
    fn private_metadata_placement_accepts_actual_captured_declaration_edges() {
        use crate::portable::{hir::SourceNamespace, resolve::Phase};
        let mut session = Session::new_macro().unwrap();
        session
            .eval("(def ^{:audit {:x 17}} placement-target 17)")
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let snapshot = session.compilation_snapshot();
        let namespace = SourceNamespace::capture(&snapshot.environment, Phase::Macro);
        let (global, revision) = namespace
            .declarations
            .iter()
            .find(|(g, _)| g.name() == "placement-target")
            .unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let root = graph.declaration(global, Some(revision), 0).unwrap();
        assert!(graph.private_metadata_placement(root).unwrap());
        assert!(
            !graph.metadata_sources.is_empty(),
            "must check actual captured metadata aliases"
        );
    }
    #[test]
    fn private_metadata_placement_keeps_cycles_and_resource_errors_explicit() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let nil = graph.recipe(Recipe::Scalar(Literal::Nil)).unwrap();
        let alias = graph.recipe(Recipe::Alias(nil)).unwrap();
        let root = graph.recipe(Recipe::Map(vec![(nil, alias)])).unwrap();
        // Hold the outer request so the checker cannot replenish its allowance.
        let request = graph.session.operation_budget_scope().unwrap();
        graph.session.charge_host_work(u64::MAX).unwrap_err();
        assert!(
            graph
                .private_metadata_placement_for(root, &[alias])
                .is_err()
        );
        drop(request);
        assert!(
            graph
                .private_metadata_placement_for(root, &[alias])
                .unwrap()
        );
        let request = graph.session.operation_budget_scope().unwrap();
        graph.session.interrupt_handle().interrupt();
        assert!(
            graph
                .private_metadata_placement_for(root, &[alias])
                .unwrap_err()
                .is_interrupt()
        );
        drop(request);
        assert!(
            graph
                .private_metadata_placement_for(root, &[alias])
                .unwrap()
        );
        graph.recipes[alias] = Some(Recipe::Alias(alias));
        assert!(
            graph
                .private_metadata_placement_for(root, &[alias])
                .unwrap_err()
                .to_string()
                .contains("Cyclic")
        );
    }
}
