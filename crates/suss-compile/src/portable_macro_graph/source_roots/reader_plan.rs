//! Callback-free native admission for the single retained-source controller.
//! No admission result grants guest authority. In particular admission never grants
//! a token permission to enter hash/equality: actual KEY roots are separate
//! checked-direct bindings, and every other selected child remains private.
use super::reader_keys::ReaderSelector;
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::portable_macro_graph) enum ChildMode {
    PrivateValue,
    CheckedDirectKey,
}
pub(in crate::portable_macro_graph) struct NativeReaderPlan {
    pub(super) layout: Vec<Vec<usize>>,
    pub(super) order: Vec<usize>,
    pub(super) expected: Vec<Option<Rc<Expectation>>>,
    pub(super) exports: Vec<usize>,
    pub(super) modes: Vec<ChildMode>,
}
pub(super) fn populate<T>(
    session: &mut Session,
    output: &mut Vec<T>,
    entries: impl ExactSizeIterator<Item = T>,
) -> Result<()> {
    let count = entries.len();
    let work = u64::try_from(count)
        .ok()
        .and_then(|n| n.checked_mul(3))
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| invalid("Reader plan population overflow"))?;
    reserve(session, output, count)?;
    session.charge_host_work(work)?;
    output.extend(entries);
    Ok(())
}
impl AnalysisGraph<'_> {
    pub(in crate::portable_macro_graph) fn admit_reader_controller(
        &mut self,
        root: Value,
    ) -> Result<Option<NativeReaderPlan>> {
        self.admit_reader_controller_with_placement(root, true)
    }
    pub(in crate::portable_macro_graph) fn admit_first_use_data_controller(
        &mut self,
        root: Value,
    ) -> Result<Option<NativeReaderPlan>> {
        self.admit_reader_controller_with_placement(root, false)
    }
    fn admit_reader_controller_with_placement(
        &mut self,
        root: Value,
        private_slots: bool,
    ) -> Result<Option<NativeReaderPlan>> {
        self.session.charge_host_work(3)?;
        if !self.capture_roots || !self.direct_reader_roots {
            return Ok(None);
        }
        // Full graph cycle/error proof, even behind a prospective retained cut.
        // This performs no guest construction or retained-cache publication.
        let layout = self.source_root_children()?;
        let mut exports = Vec::new();
        let mut seen = Vec::new();
        reserve(self.session, &mut seen, self.recipes.len())?;
        self.session.charge_host_work(self.recipes.len() as u64)?;
        seen.resize(self.recipes.len(), false);
        let mut frontier = Vec::new();
        populate(self.session, &mut frontier, std::iter::once(root))?;
        while let Some(id) = frontier.pop() {
            self.session.charge_host_work(3)?;
            if let Some(boundary) = self.source_roots.index(self.session, id)? {
                self.session.charge_host_work(exports.len() as u64)?;
                if !exports.contains(&boundary) {
                    reserve(self.session, &mut exports, 1)?;
                    self.session.charge_host_work(1)?;
                    exports.push(boundary);
                }
                continue;
            }
            let slot = seen
                .get_mut(id)
                .ok_or_else(|| invalid("Reader frontier outside recipe arena"))?;
            if *slot {
                continue;
            }
            *slot = true;
            let recipe = self
                .recipes
                .get(id)
                .and_then(Option::as_ref)
                .ok_or_else(|| invalid("Missing reader frontier recipe"))?;
            let edges = dependencies(self.session, recipe)?;
            populate(self.session, &mut frontier, edges.into_iter().rev())?;
        }
        if exports.is_empty() {
            return Ok(None);
        }
        let mut states = Vec::new();
        reserve(self.session, &mut states, layout.len())?;
        self.session.charge_host_work(layout.len() as u64)?;
        states.resize(layout.len(), 0u8);
        let mut stack = Vec::new();
        let mut order = Vec::new();
        populate(
            self.session,
            &mut stack,
            exports.iter().rev().map(|id| (*id, false)),
        )?;
        while let Some((id, exit)) = stack.pop() {
            self.session.charge_host_work(3)?;
            if exit {
                states[id] = 2;
                reserve(self.session, &mut order, 1)?;
                self.session.charge_host_work(1)?;
                order.push(id);
            } else if states[id] != 2 {
                if states[id] == 1 {
                    return Err(invalid("Cyclic reader boundary graph"));
                }
                states[id] = 1;
                populate(self.session, &mut stack, std::iter::once((id, true)))?;
                populate(
                    self.session,
                    &mut stack,
                    layout[id].iter().rev().map(|id| (*id, false)),
                )?;
            }
        }
        let mut modes = Vec::new();
        reserve(self.session, &mut modes, layout.len())?;
        for capture in &self.source_roots.roots {
            self.session.charge_host_work(4)?;
            modes.push(
                if matches!(
                    capture.owner,
                    Owner::ReaderSource(_, _, ReaderSelector::Key(_))
                ) {
                    ChildMode::CheckedDirectKey
                } else {
                    ChildMode::PrivateValue
                },
            );
        }
        let mut expected: Vec<Option<Rc<Expectation>>> = Vec::new();
        reserve(self.session, &mut expected, layout.len())?;
        self.session.charge_host_work(layout.len() as u64)?;
        expected.resize_with(layout.len(), || None);
        for &id in &order {
            self.session.charge_host_work(2)?;
            if private_slots && matches!(&self.source_roots.roots[id].owner,
                Owner::ReaderSource(_, _, ReaderSelector::Value(_))) {
                return Ok(None); // only the identity-only first-use route admits DATA values
            }
            let Some(witness) = self.source_root_expectation_with_layout(id, &layout)? else {
                return Ok(None);
            };
            // Reject inconsistent duplicate exports rather than publication by
            // incidental traversal order. Coalescing requires a separate proof.
            for &prior in &order {
                self.session.charge_host_work(1)?;
                if prior == id {
                    break;
                }
                let previous = expected[prior].as_ref().unwrap();
                self.session
                    .charge_host_work(key_comparison_work(&previous.key, &witness.key))?;
                if previous.key == witness.key {
                    return Ok(None);
                }
            }
            self.session.charge_host_work(1)?;
            if private_slots {
                let mut private_aliases = Vec::new();
                reserve(self.session, &mut private_aliases, layout[id].len())?;
                for &child in &layout[id] {
                    self.session.charge_host_work(3)?;
                    if modes[child] == ChildMode::PrivateValue {
                        private_aliases.push(self.source_roots.roots[child].alias);
                    }
                }
                // Direct KEY values may reach callbacks ONLY after strict current
                // selection. That does not permit any token-valued descendant there.
                let native = self.source_roots.roots[id].native;
                if !self.private_source_placement_for(native, &private_aliases)? {
                    return Ok(None);
                }
            }
            self.session.charge_host_work(1)?;
            expected[id] = Some(witness);
        }
        // Prove the outer constructor's private placements too, independently
        // of each captured parent's proof. No cache fallback after callbacks is
        // authorized by a successful admission result.
        self.session.charge_host_work(1)?;
        if private_slots {
            let mut private_exports = Vec::new();
            reserve(self.session, &mut private_exports, exports.len())?;
            for &id in &exports {
                self.session.charge_host_work(3)?;
                if modes[id] == ChildMode::PrivateValue {
                    private_exports.push(self.source_roots.roots[id].alias);
                }
            }
            if self.source_roots.index(self.session, root)?.is_none()
                && !self.private_source_placement_for(root, &private_exports)?
            {
                return Ok(None);
            }
        }
        Ok(Some(NativeReaderPlan {
            layout,
            order,
            expected,
            exports,
            modes,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::{
        hir,
        resolve::{Environment, Phase},
    };
    use crate::portable_macro_data::FormBridge;
    #[test]
    fn actual_completed_declaration_admission_distinguishes_direct_keys_from_private_children() {
        let input = "(def ^number watched 17)";
        let forms = suss_reader::forms::read_forms(input).unwrap();
        let environment = hir::prepare(
            &forms,
            0..input.len(),
            &Environment::default(),
            Phase::Runtime,
        )
        .unwrap()
        .1;
        let binding = environment
            .resolve(Phase::Runtime, &suss_reader::Symbol::new("watched"), 0..0)
            .unwrap();
        let global = binding.global();
        let holder = environment.shared_definition_info(global).unwrap();
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.capture_roots = true;
        graph.direct_reader_roots = true;
        let root = graph.declaration(global, Some(holder), 0).unwrap();
        let plan = graph.admit_reader_controller(root).unwrap().unwrap();
        assert!(!plan.exports.is_empty());
        let mut direct = 0;
        for &id in &plan.order {
            assert!(plan.expected[id].is_some());
            for &child in &plan.layout[id] {
                let child_position = plan.order.iter().position(|old| *old == child).unwrap();
                let parent_position = plan.order.iter().position(|old| *old == id).unwrap();
                assert!(child_position < parent_position);
            }
            if plan.modes[id] == ChildMode::CheckedDirectKey {
                direct += 1;
            }
        }
        assert!(
            direct > 0,
            "actual retained reader key must be a separate native boundary"
        );
        assert!(plan.modes.contains(&ChildMode::PrivateValue));
    }
    #[test]
    fn reader_admission_is_disabled_on_legacy_builds_and_rejects_native_cycles() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let root = graph
            .recipe(Recipe::Scalar(crate::portable::hir::Literal::Nil))
            .unwrap();
        assert!(graph.admit_reader_controller(root).unwrap().is_none());
        graph.capture_roots = true;
        graph.direct_reader_roots = true;
        graph.recipes[root] = Some(Recipe::Alias(root));
        // No captured root means source_root_children does not cover the outer
        // cycle. Frontier's seen set alone cannot prove cycle freedom; it must
        // report a strategy decline, never publish/execute the malformed graph.
        assert!(graph.admit_reader_controller(root).unwrap().is_none());
    }
}
