//! Successful full immutable bindings, retained for this AnalysisGraph only.
//! No recipes, guest values, cache certificates or negative verdicts are stored.
use super::*;
use portable::resolve::{CompletedDeclarationSource, DefinitionInfo, Global};

pub(super) struct Binding {
    global: Global,
    holder: Arc<DefinitionInfo>,
    owner: Arc<CompletedDeclarationSource>,
}
impl Binding {
    fn key(&self) -> (usize, usize) {
        (Arc::as_ptr(&self.holder) as usize, Arc::as_ptr(&self.owner) as usize)
    }
}
impl AnalysisGraph<'_> {
    /// Every call polls the live operation meter before reading the memo.
    /// Both Arc allocations are retained: pointer reuse and make_mut revisions
    /// cannot turn a previously checked triple into a different native input.
    pub(super) fn completed_owner_binding(&mut self, global: &Global,
        owner: &Arc<CompletedDeclarationSource>, holder: &Arc<DefinitionInfo>) -> Result<()> {
        self.charge(8)?;
        let wanted = (Arc::as_ptr(holder) as usize, Arc::as_ptr(owner) as usize);
        let mut low = 0;
        let mut high = self.completed_bindings.as_ref().map_or(0, Vec::len);
        while low < high {
            // Bounds, midpoint, two pointer reads, tuple comparison and branch.
            self.charge(16)?;
            let middle = low + (high - low) / 2;
            let key = self.completed_bindings.as_ref().unwrap()[middle].key();
            match key.cmp(&wanted) {
                std::cmp::Ordering::Less => low = middle + 1,
                std::cmp::Ordering::Greater => high = middle,
                std::cmp::Ordering::Equal => {
                    self.charge(global.namespace().len().saturating_add(global.name().len())
                        .saturating_mul(2).saturating_add(8))?;
                    if self.completed_bindings.as_ref().unwrap()[middle].global == *global {
                        return Ok(());
                    }
                    // Same allocations supplied with a foreign Global: the full
                    // original validator must reject it; never replace the entry.
                    return self.completed_owner_binding_full(global, owner, holder);
                }
            }
        }
        self.completed_owner_binding_full(global, owner, holder)?;
        // Validation success is not published until ALL following prepayments
        // and allocations succeed. A failure may retain capacity, never a verdict.
        self.charge(12)?;
        let len = self.completed_bindings.as_ref().map_or(0, Vec::len);
        let capacity = self.completed_bindings.as_ref().map_or(0, Vec::capacity);
        let needed = len.checked_add(1).ok_or_else(binding_overflow)?;
        if needed > capacity {
            let next = capacity.checked_mul(2).unwrap_or(needed).max(needed).max(4);
            let bytes = next.checked_mul(2).and_then(|n| n.checked_add(len))
                .and_then(|n| n.checked_add(capacity))
                .and_then(|n| n.checked_mul(std::mem::size_of::<Binding>()))
                .and_then(|n| n.checked_add(2 * std::mem::size_of::<Option<Vec<Binding>>>() + 8))
                .ok_or_else(binding_overflow)?;
            // New backing, initialization, old relocation/retirement and header.
            self.charge(bytes)?;
            self.completed_bindings.get_or_insert_with(Vec::new)
                .try_reserve_exact(next - len).map_err(|_| binding_overflow())?;
        }
        let shifted = len.checked_sub(low).and_then(|n| n.checked_mul(std::mem::size_of::<Binding>()))
            .and_then(|n| n.checked_mul(2)).ok_or_else(binding_overflow)?;
        let text = global.namespace().len().checked_add(global.name().len())
            .and_then(|n| n.checked_mul(2)).ok_or_else(binding_overflow)?;
        let work = shifted.checked_add(text)
            .and_then(|n| n.checked_add(2 * std::mem::size_of::<Binding>() + 20))
            .ok_or_else(binding_overflow)?;
        // Insertion shift, owned Global bytes/header, Arc clones and eventual
        // entry/Global/Arc retirement are prepaid before any acquisition.
        self.charge(work)?;
        let binding = Binding { global: global.clone(), holder: holder.clone(), owner: owner.clone() };
        self.completed_bindings.as_mut().unwrap().insert(low, binding);
        Ok(())
    }
}
fn binding_overflow() -> SessionError {
    SessionError::Host(wasmtime::Error::msg("Completed binding memo storage overflow"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use portable::resolve::{Environment, Phase};
    // Native preparation still resolves executable collection constructors.
    // Use the genuine shipped catalog, not a fabricated nominal declaration.
    // Loading these immutable compiler facts does not create a guest Session.
    fn native_fixture_environment(phase: Phase) -> Environment {
        let namespace = Environment::default().current_namespace(phase).to_owned();
        let shipped = crate::portable::bootstrap::shipped(phase)
            .unwrap_or_else(|error| panic!("native fixture bootstrap {phase:?}: {error:?}"));
        let mut environment = shipped.environment.clone();
        environment.enter_namespace(phase, &namespace)
            .unwrap_or_else(|error| panic!("native fixture namespace {phase:?}/{namespace}: {error:?}"));
        let symbol = suss_reader::Symbol {
            namespace: Some("suss.core".into()), name: "PersistentVector".into(),
        };
        let class = environment.resolve(phase, &symbol, 0..0)
            .unwrap_or_else(|error| panic!("actual shipped vector class {phase:?}: {error:?}"))
            .global().clone();
        assert_eq!(class.phase(), phase);
        assert_eq!(class.namespace(), "suss.core");
        assert_eq!(class.name(), "PersistentVector");
        assert!(shipped.cells.contains(&class), "vector constructor must be an actual shipped cell");
        let info = environment.definition_info(&class).expect("actual shipped nominal source declaration");
        assert!(info.analysis_completed);
        assert!(info.type_fields.is_some(), "completed nominal type, no guessed field count");
        environment
    }
    fn fixture(source: &str) -> (Global, Arc<DefinitionInfo>) {
        fixture_in_namespace(source, None)
    }
    fn fixture_in_namespace(source: &str, namespace: Option<&str>) -> (Global, Arc<DefinitionInfo>) {
        let forms = suss_reader::forms::resolve_conditionals(
            suss_reader::forms::read_forms(source).unwrap()).unwrap();
        let origin = portable::SourceOrigin::new(source, None);
        let mut baseline = native_fixture_environment(Phase::Macro);
        if let Some(namespace) = namespace {
            // NoExpansion intentionally cannot expand the public ns macro.
            // Establish a real native namespace before analyzing the same def.
            baseline.enter_namespace(Phase::Macro, namespace).unwrap();
        }
        let env = hir::prepare_with_origin(&forms, 0..source.len(), &baseline,
            Phase::Macro, &mut portable::NoExpansion, Some(&origin)).unwrap().1;
        let binding = env.resolve(Phase::Macro, &suss_reader::Symbol::new("subject"), 0..0).unwrap();
        let global = binding.global().clone();
        let info = env.shared_definition_info(&global).unwrap().clone();
        (global, info)
    }
    fn fuel(error: &SessionError) -> bool {
        matches!(error, SessionError::Trap(e)
            if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel))
    }
    #[test]
    fn completed_binding_memo_matches_full_validator_and_reports_actual_work() {
        let source = format!("(def ^{{:doc \"kept\"}} subject [{}])",
            (0..96).map(|n| n.to_string()).collect::<Vec<_>>().join(" "));
        let (global, info) = fixture(&source);
        let owner = info.completed_values.as_ref().unwrap().clone();
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let (full, memo) = session.with_operation_budget(|session| {
            let mut graph = AnalysisGraph::new(&bridge, session);
            let before = graph.session.operation_fuel_remaining()?;
            for _ in 0..8 { graph.completed_owner_binding_full(&global, &owner, &info)?; }
            let full = before - graph.session.operation_fuel_remaining()?;
            let before = graph.session.operation_fuel_remaining()?;
            for _ in 0..8 { graph.completed_owner_binding(&global, &owner, &info)?; }
            let memo = before - graph.session.operation_fuel_remaining()?;
            assert_eq!(graph.completed_bindings.as_ref().unwrap().len(), 1);
            assert!(Arc::ptr_eq(&graph.completed_bindings.as_ref().unwrap()[0].owner, &owner));
            Ok((full, memo))
        }).unwrap();
        eprintln!("completed-binding-work full={full} memo={memo} calls=8 source-bytes={}", source.len());
        assert!(memo < full, "actual full validations removed, including all memo storage/search fees");
    }
    #[test]
    fn completed_binding_memo_reports_cold_many_revision_overhead() {
        let fixtures: Vec<_> = (0..24).map(|n| fixture(&format!("(def subject {n})"))).collect();
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.with_operation_budget(|session| {
            let mut graph = AnalysisGraph::new(&bridge, session);
            let before = graph.session.operation_fuel_remaining()?;
            for (global, info) in &fixtures {
                graph.completed_owner_binding_full(global, info.completed_values.as_ref().unwrap(), info)?;
            }
            let full = before - graph.session.operation_fuel_remaining()?;
            let before = graph.session.operation_fuel_remaining()?;
            for (global, info) in &fixtures {
                graph.completed_owner_binding(global, info.completed_values.as_ref().unwrap(), info)?;
            }
            let cold = before - graph.session.operation_fuel_remaining()?;
            let before = graph.session.operation_fuel_remaining()?;
            for (global, info) in &fixtures {
                graph.completed_owner_binding(global, info.completed_values.as_ref().unwrap(), info)?;
            }
            let warm = before - graph.session.operation_fuel_remaining()?;
            let entries = graph.completed_bindings.as_ref().unwrap();
            assert_eq!(entries.len(), fixtures.len());
            eprintln!("completed-binding-many full={full} cold={cold} warm={warm} entries={} backing-bytes={}",
                entries.len(), entries.capacity() * std::mem::size_of::<Binding>());
            // Cold insertion/shift overhead is exposed, not claimed as a win.
            Ok(())
        }).unwrap();
    }
    #[test]
    fn completed_binding_memo_exact_revisions_sources_and_foreign_owners() {
        let source = "(def ^number subject 17)";
        let (global, info) = fixture(source);
        let owner = info.completed_values.as_ref().unwrap().clone();
        let (foreign_global, foreign_info) = fixture_in_namespace("(def subject 17)", Some("foreign"));
        assert_eq!(foreign_global.namespace(), "foreign");
        assert_eq!(foreign_global.phase(), Phase::Macro);
        assert_ne!(foreign_global, global);
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.with_operation_budget(|session| {
            let mut graph = AnalysisGraph::new(&bridge, session);
            let holder_refs = Arc::strong_count(&info);
            let owner_refs = Arc::strong_count(&owner);
            graph.completed_owner_binding(&global, &owner, &info)?;
            assert_eq!(Arc::strong_count(&info), holder_refs + 1);
            assert_eq!(Arc::strong_count(&owner), owner_refs + 1);
            // Equal cloned native inputs are valid under the original policy,
            // but must run the full validator as a distinct allocation.
            let copy = Arc::new(info.as_ref().clone());
            graph.completed_owner_binding_full(&global, &owner, &copy)?;
            graph.completed_owner_binding(&global, &owner, &copy)?;
            assert_eq!(graph.completed_bindings.as_ref().unwrap().len(), 2);
            let mut revisions = Vec::new();
            let mut changed = info.as_ref().clone(); changed.declaration.kind = Kind::Nil;
            revisions.push(changed);
            let mut changed = info.as_ref().clone(); Arc::make_mut(&mut changed.definition_form).metadata.push(
                suss_reader::forms::read_forms(":poison").unwrap().remove(0));
            revisions.push(changed);
            let mut changed = info.as_ref().clone(); changed.origin = Some(portable::SourceOrigin::new(source, None));
            revisions.push(changed);
            let mut changed = info.as_ref().clone(); changed.initializer = None;
            revisions.push(changed);
            for changed in revisions {
                let changed = Arc::new(changed);
                assert!(graph.completed_owner_binding_full(&global, &owner, &changed).is_err());
                assert!(graph.completed_owner_binding(&global, &owner, &changed).is_err());
                assert_eq!(graph.completed_bindings.as_ref().unwrap().len(), 2);
            }
            assert!(graph.completed_owner_binding(&foreign_global, &owner, &info).is_err());
            let foreign_owner = foreign_info.completed_values.as_ref().unwrap();
            assert!(graph.completed_owner_binding(&global, foreign_owner, &info).is_err());
            assert_eq!(graph.completed_bindings.as_ref().unwrap().len(), 2);
            // Retaining the native revision prevents Arc::get_mut, and make_mut
            // creates an allocation that cannot hit the previously checked entry.
            let mut writable = info.clone();
            assert!(Arc::get_mut(&mut writable).is_none());
            Arc::make_mut(&mut writable).docstring = Some(vec![120]);
            assert!(!Arc::ptr_eq(&writable, &info));
            assert!(graph.completed_owner_binding(&global, &owner, &writable).is_err());
            graph.completed_owner_binding(&global, &owner, &info)?;
            Ok(())
        }).unwrap();
    }
    #[test]
    fn completed_binding_memo_retained_type_plan_is_checked_before_first_hit() {
        let (global, info) = fixture("(def ^number subject 17) (deftype* subject [] nil nil)");
        let owner = info.type_key_history.as_ref().unwrap().values(&global).unwrap().source.as_ref().unwrap().clone();
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.with_operation_budget(|session| {
            let mut graph = AnalysisGraph::new(&bridge, session);
            graph.completed_owner_binding_full(&global, &owner, &info)?;
            graph.completed_owner_binding(&global, &owner, &info)?;
            graph.completed_owner_binding(&global, &owner, &info)?;
            let mut changed = info.as_ref().clone();
            changed.type_fields = Some(1);
            let changed = Arc::new(changed);
            assert!(graph.completed_owner_binding_full(&global, &owner, &changed).is_err());
            assert!(graph.completed_owner_binding(&global, &owner, &changed).is_err());
            assert_eq!(graph.completed_bindings.as_ref().unwrap().len(), 1);
            Ok(())
        }).unwrap();
    }
    #[test]
    fn completed_binding_memo_zero_boundary_irq_failures_and_retry() {
        let (global, info) = fixture("(def subject 17)");
        let owner = info.completed_values.as_ref().unwrap().clone();
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let (cold, hit) = session.with_operation_budget(|session| {
            let mut graph = AnalysisGraph::new(&bridge, session);
            let before = graph.session.operation_fuel_remaining()?;
            graph.completed_owner_binding(&global, &owner, &info)?;
            let cold = before - graph.session.operation_fuel_remaining()?;
            let before = graph.session.operation_fuel_remaining()?;
            graph.completed_owner_binding(&global, &owner, &info)?;
            Ok((cold, before - graph.session.operation_fuel_remaining()?))
        }).unwrap();
        assert!(cold > 1 && hit > 1);
        // Sampled actual whole-operation cuts, not an exhaustive compiler sweep.
        for allowance in [0, 1, cold - 1] {
            session.set_operation_fuel(allowance);
            let error = session.with_operation_budget(|session| {
                let mut graph = AnalysisGraph::new(&bridge, session);
                let error = graph.completed_owner_binding(&global, &owner, &info).unwrap_err();
                assert!(graph.completed_bindings.as_ref().is_none_or(Vec::is_empty));
                Err::<(), _>(error)
            }).unwrap_err();
            assert!(fuel(&error), "{error:?}");
            session.set_operation_fuel(10_000_000);
            session.with_operation_budget(|session| {
                let mut graph = AnalysisGraph::new(&bridge, session);
                graph.completed_owner_binding(&global, &owner, &info)
            }).unwrap();
        }
        session.set_operation_fuel(10_000_000);
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        {
            let _request = graph.session.operation_budget_scope().unwrap();
            graph.completed_owner_binding(&global, &owner, &info).unwrap();
            let irq = graph.session.host_work_interrupt();
            irq.store(true, std::sync::atomic::Ordering::SeqCst);
            let error = graph.completed_owner_binding(&global, &owner, &info).unwrap_err();
            assert!(error.is_interrupt());
            assert_eq!(graph.completed_bindings.as_ref().unwrap().len(), 1);
            irq.store(false, std::sync::atomic::Ordering::SeqCst);
            // Cancellation exhausts this operation: clearing its flag cannot
            // authorize a refill or make even a memo hit succeed here.
            let error = graph.completed_owner_binding(&global, &owner, &info).unwrap_err();
            assert!(fuel(&error), "{error:?}");
            assert_eq!(graph.completed_bindings.as_ref().unwrap().len(), 1);
        }
        {
            let _request = graph.session.operation_budget_scope().unwrap();
            graph.completed_owner_binding(&global, &owner, &info).unwrap();
            assert_eq!(graph.completed_bindings.as_ref().unwrap().len(), 1);
        }
        // New ordinary operation scope installs the configured allowance;
        // no refill inside an active request or inspection hook is needed.
        graph.session.set_operation_fuel(hit - 1);
        {
            let _request = graph.session.operation_budget_scope().unwrap();
            let error = graph.completed_owner_binding(&global, &owner, &info).unwrap_err();
            assert!(fuel(&error));
            assert_eq!(graph.completed_bindings.as_ref().unwrap().len(), 1);
        }
        graph.session.set_operation_fuel(10_000_000);
        {
            let _request = graph.session.operation_budget_scope().unwrap();
            graph.completed_owner_binding(&global, &owner, &info).unwrap();
        }
    }
}
