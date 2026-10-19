//! Prepaid ADDED work for literal-aware native entry buffers. No guest effects.
use super::*;

// Count syntax without evaluating, cloning or moving any source expression.
macro_rules! field_entries {
    ($graph:ident; [$($entry:expr),* $(,)?]) => {{
        let count = <[()]>::len(&[$(field_entries!(@unit $entry)),*]);
        let mut entries = $graph.new_field_entries(count)?;
        $(entries.push($entry);)*
        entries
    }};
    (@unit $entry:expr) => { () };
}
macro_rules! field_map {
    ($graph:ident; [$($entry:expr),* $(,)?]) => {{
        let entries = field_entries!($graph; [$($entry),*]);
        $graph.map(entries)
    }};
}
macro_rules! field_map_in_task_slot {
    ($graph:ident, $slot:expr; [$($entry:expr),* $(,)?]) => {{
        let slot = $slot;
        let entries = field_entries!($graph; [$($entry),*]);
        $graph.map_in_task_slot(slot, entries)
    }};
}
macro_rules! field_push {
    ($graph:ident, $entries:ident; $entry:expr) => {{
        $graph.reserve_field_entries(&mut $entries, 1)?;
        $entries.push($entry);
    }};
}
macro_rules! field_extend {
    ($graph:ident, $entries:ident; [$($entry:expr),* $(,)?]) => {{
        let count = <[()]>::len(&[$(field_entries!(@unit $entry)),*]);
        $graph.reserve_field_entries(&mut $entries, count)?;
        $($entries.push($entry);)*
    }};
}

impl AnalysisGraph<'_> {
    // Retain every old map/recipe charge. Only the ADDED bytes of the key
    // representation are paid here, plus actual new builder controls. T can be
    // Value or the existing borrowed-HIR grouping payload; no payload copies.
    fn field_entry_extra<T>() -> usize {
        std::mem::size_of::<(FieldKey<'_>, T)>()
            .checked_sub(std::mem::size_of::<(&str, T)>())
            .expect("Literal key entry unexpectedly smaller than baseline")
    }
    pub(super) fn new_field_entries<'k, T>(
        &mut self, count: usize,
    ) -> Result<Vec<(FieldKey<'k>, T)>> {
        // Type extents, checked byte arithmetic, init/reserve metadata/control.
        // This payment occurs BEFORE constructing the Vec or evaluating entries.
        self.charge(10)?;
        let bytes = Self::field_entry_extra::<T>().checked_mul(count)
            .expect("Literal entry extent overflow");
        // Per entry: added tag initialization and the explicit capacity
        // check, length read/update and write-address controls of builder push.
        // FieldKey is Copy: no invented per-element destructor charge.
        self.charge(bytes.saturating_add(count.saturating_mul(5)))?;
        Ok(Vec::with_capacity(count))
    }
    pub(super) fn reserve_field_entries<'k, T>(
        &mut self, entries: &mut Vec<(FieldKey<'k>, T)>, additional: usize,
    ) -> Result<()> {
        // Prepay length/capacity reads, checked length, branch and size controls.
        self.charge(12)?;
        let length = entries.len();
        let capacity = entries.capacity();
        let required = length.checked_add(additional).expect("Literal entry length overflow");
        // Entry initialization/push controls regardless of capacity reuse.
        self.charge(additional.saturating_mul(5))?;
        if required > capacity {
            self.charge(8)?; // checked growth, maxima and reserve difference
            let requested = capacity.checked_mul(2).unwrap_or(required).max(required).max(4);
            let added = requested.checked_add(length)
                .and_then(|n| n.checked_mul(Self::field_entry_extra::<T>()))
                .expect("Literal entry allocation extent overflow");
            // Added requested backing AND live relocation are prepaid before
            // acquisition. Same requested-capacity policy as existing reserves.
            self.charge(added)?;
            entries.reserve_exact(requested - length);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn build(graph: &mut AnalysisGraph<'_>, input: &Form) -> Result<Value> {
        build_traced(graph, input, &mut Vec::new())
    }
    fn build_traced(graph: &mut AnalysisGraph<'_>, input: &Form, costs: &mut Vec<u64>) -> Result<Value> {
        let start = graph.session.operation_fuel_remaining()?;
        let mut fields = field_entries!(graph; [
            (FieldKey::Literal(AnalyzerLiteralKeyword("op")), graph.form(input, 0)?),
        ]);
        costs.push(start - graph.session.operation_fuel_remaining()?);
        // Force growth from capacity1, preserving the original expression order.
        field_push!(graph, fields;
            (FieldKey::Cold("suss/early-retry"), graph.flag(true)?));
        costs.push(start - graph.session.operation_fuel_remaining()?);
        field_extend!(graph, fields; [
            (FieldKey::Cold("suss/second"), graph.flag(false)?),
            (FieldKey::Cold("suss/third"), graph.number(19)?),
        ]);
        costs.push(start - graph.session.operation_fuel_remaining()?);
        let value = graph.map(fields)?;
        costs.push(start - graph.session.operation_fuel_remaining()?);
        Ok(value)
    }
    #[test]
    fn literal_entry_builders_prepaid_failures_preserve_source_and_fresh_retry() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let source = suss_reader::forms::read_forms("^{:kept [3 4]} [17 18]").unwrap().remove(0);
        let original = source.clone();
        let mut output = None;
        let mut costs = Vec::new();
        {
            let request = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            build_traced(&mut graph, &source, &mut costs).unwrap();
            drop(graph); drop(request);
        }
        // Derive samples from actual fresh-graph work, so later growth is
        // reached rather than assuming a small fixed allowance reaches it.
        // This is NOT exhaustive whole-compiler coverage.
        let mut allowances = vec![0, 1, 9, 10, 20];
        for cost in costs { allowances.extend([cost.saturating_sub(1), cost, cost + 1]); }
        allowances.sort_unstable(); allowances.dedup();
        for allowance in allowances {
            session.set_operation_fuel(allowance);
            let request = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            let result = build(&mut graph, &source);
            match result {
                Ok(value) => { assert!(graph.recipes[value].is_some()); }
                Err(error) => {
                    assert!(matches!(error, SessionError::Trap(ref e)
                        if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
                        "allowance={allowance}: {error:?}");
                    if allowance < 10 {
                        // No recipe/key cache publication before acquisition fee.
                        assert!(graph.recipes.is_empty());
                        assert!(graph.keys.iter().all(BTreeMap::is_empty));
                        assert!(graph.maps.is_empty());
                    }
                }
            }
            drop(graph); drop(request);
            assert_eq!(source, original);
            session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
            let request = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            let value = build(&mut graph, &source).unwrap();
            let actual = graph.materialize(value).unwrap();
            drop(graph);
            let decoded = bridge.read(&mut session, &actual, 0..0).unwrap();
            if let Some(expected) = &output { assert_eq!(expected, &decoded); }
            else { output = Some(decoded); }
            drop(request);
        }
        session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        let request = session.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.session.interrupt_handle().interrupt();
        assert!(build(&mut graph, &source).unwrap_err().is_interrupt());
        assert!(graph.recipes.is_empty());
        assert!(graph.keys.iter().all(BTreeMap::is_empty));
        drop(graph); drop(request);
        assert_eq!(source, original);
        let request = session.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let value = build(&mut graph, &source).unwrap();
        assert!(graph.recipes[value].is_some());
        drop(graph); drop(request);
    }
    #[test]
    fn literal_entry_late_invalid_source_keeps_original_guard_and_recovers() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let bad = Form { span: 7..9, metadata: vec![], kind: Kind::Map(vec![Form {
            span: 7..8, metadata: vec![], kind: Kind::Nil,
        }]) };
        let old = bad.clone();
        let request = session.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let error = build(&mut graph, &bad).unwrap_err();
        assert!(!error.is_interrupt());
        assert!(matches!(error, SessionError::Host(_)), "{error:?}");
        assert!(graph.recipes.is_empty());
        assert!(graph.keys.iter().all(BTreeMap::is_empty));
        drop(graph); drop(request);
        assert_eq!(bad, old);
        let good = suss_reader::forms::read_forms("[17]").unwrap().remove(0);
        let request = session.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        assert!(build(&mut graph, &good).is_ok());
        drop(graph); drop(request);
    }
}
