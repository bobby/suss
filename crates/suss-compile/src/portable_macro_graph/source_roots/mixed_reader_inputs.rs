//! Strict parent construction with a mixed private-value/direct-reader-key
//! inventory. The ordinary traversal is shared. Every selected edge taints its
//! ancestors, so direct-key parents ALSO use deferred factories even though no
//! placeholder occurs in their keys. Domain enrollment follows final child proof.
use super::reader_keys::ReaderSelector;
use super::*;

impl AnalysisGraph<'_> {
    fn mixed_source_root_inputs(
        &mut self,
        id: usize,
        children: Vec<CheckedChild>,
    ) -> Result<Option<SourceInputs>> {
        let _request = self.session.operation_budget_scope()?;
        let layout = self.source_root_children()?;
        let Some(expected) = self.source_root_expectation_with_layout(id, &layout)? else {
            return Ok(None);
        };
        self.session.charge_host_work(2)?;
        if children.len() != expected.children.len() {
            return Ok(None);
        }
        let mut current = Vec::new();
        reserve(self.session, &mut current, children.len())?;
        let mut private_aliases = Vec::new();
        reserve(self.session, &mut private_aliases, children.len())?;
        // No callbacks occur while building this independently checked inventory.
        for (ordinal, supplied) in children.into_iter().enumerate() {
            self.session.charge_host_work(3)?;
            let child_id = layout[id][ordinal];
            let Some(child_expected) =
                self.source_root_expectation_with_layout(child_id, &layout)?
            else {
                return Ok(None);
            };
            let Some(child) = Certificate::checked(
                self.session,
                self.bridge,
                &supplied.certificate,
                &child_expected,
            )?
            else {
                return Ok(None);
            };
            let direct = matches!(
                self.source_roots.roots[child_id].owner,
                Owner::ReaderSource(_, _, ReaderSelector::Key(_))
            );
            let alias = self.source_roots.roots[child_id].alias;
            self.session.charge_host_work(4)?;
            if !direct {
                private_aliases.push(alias);
            }
            current.push((alias, child, child_expected, direct));
        }
        let native = self.source_roots.roots[id].native;
        // Alias-only exports need the controller's dedicated reference-following
        // lineage path; do not pretend a zero-fact parent is a new constructor.
        let mut cursor = native;
        for _ in 0..self.recipes.len() {
            self.session.charge_host_work(current.len() as u64 + 3)?;
            if current.iter().any(|(alias, _, _, _)| *alias == cursor) {
                return Ok(None);
            }
            match self.recipes.get(cursor).and_then(Option::as_ref) {
                Some(Recipe::Alias(next)) => cursor = *next,
                Some(_) => break,
                None => return Err(invalid("Missing mixed parent native recipe")),
            }
        }
        if !self.private_source_placement_for(native, &private_aliases)? {
            return Ok(None);
        }
        let log =
            crate::portable_session::private_fixups::slot_log::PrivateSlotLog::new(self.session)?;
        let mut inputs = Vec::new();
        reserve(self.session, &mut inputs, current.len())?;
        let mut by_alias = Vec::new();
        reserve(self.session, &mut by_alias, self.recipes.len())?;
        self.session.charge_host_work(self.recipes.len() as u64)?;
        by_alias.resize(self.recipes.len(), None);
        for (ordinal, (alias, child, child_expected, direct)) in current.into_iter().enumerate() {
            self.session.charge_host_work(2)?;
            let token = if direct {
                None
            } else {
                drop(log.borrow_mut().token(self.session, ordinal)?);
                Some(log.borrow().token_handle(self.session, ordinal)?)
            };
            self.session.charge_host_work(5)?;
            by_alias[alias] = Some(ordinal);
            inputs.push(SourceInput {
                alias,
                ordinal,
                token,
                exposed: std::cell::Cell::new(false),
                child,
                expected: child_expected,
            });
        }
        Ok(Some(SourceInputs {
            expected,
            log,
            inputs,
            by_alias,
            transient: None,
        }))
    }

    /// Staged, STRICT path only. No fallback/replay is allowed after construction
    /// starts. Callers must recheck every completed parent again after their LAST
    /// callback, then prepare a single atomic cache replacement. This method
    /// itself never publishes to any retained cache.
    pub(in crate::portable_macro_graph) fn materialize_mixed_source_parent(
        &mut self,
        id: usize,
        children: Vec<CheckedChild>,
    ) -> Result<Option<BoundParent>> {
        let _request = self.session.operation_budget_scope()?;
        let Some(inputs) = self.mixed_source_root_inputs(id, children)? else {
            return Ok(None);
        };
        let normal = self.bridge.recording(self.session)?;
        let private = normal.deferred_private_bridge(self.session, &inputs.log)?;
        self.session.charge_host_work(2)?;
        let native = self.source_roots.roots[id].native;
        let value =
            self.materialize_with_inputs(native, &private, None, Some((&inputs, &normal)))?;
        drop(normal);
        let (mut records, deferred) = private.into_deferred_records()?;
        // All callbacks belonging to this parent have now finished. Never turn
        // this failure into old->fresh key replay or patch an exposed map key.
        let mut checked = Vec::new();
        reserve(self.session, &mut checked, inputs.inputs.len())?;
        let results = Certificate::checked_batch(
            self.session,
            self.bridge,
            inputs
                .inputs
                .iter()
                .map(|input| (&input.child.certificate, &input.expected)),
        )?;
        let mut valid = true;
        for (input, child) in inputs.inputs.iter().zip(results) {
            self.session.charge_host_work(3)?;
            valid &= child.is_some();
            // Direct aliases are reachable native dependencies: absence of actual
            // exposure is inconsistent with the admitted materialization.
            if input.token.is_none() {
                valid &= input.exposed.get();
            }
            checked.push(child);
        }
        valid &= records.matches_class_schemas(self.session)?;
        if !valid {
            return Err(invalid(
                "Mixed source child changed during construction callbacks",
            ));
        }
        let mut pairs = Vec::new();
        reserve(self.session, &mut pairs, inputs.inputs.len())?;
        for input in &inputs.inputs {
            self.session.charge_host_work(2)?;
            if let Some(token) = &input.token {
                let slots = inputs.log.borrow();
                self.session.charge_host_work(3)?;
                if !token.belongs_to(&slots) || token.id() != input.ordinal {
                    return Err(invalid("Foreign mixed parent private binding"));
                }
                drop(slots.token_handle(self.session, input.ordinal)?);
                pairs.push((token.value(), input.child.value()));
            }
        }
        // Direct keys never enter this substitution list, even if their guest
        // contents or backing happen to resemble a private token.
        inputs.log.borrow().apply(self.session, &value, &pairs)?;
        let mut children = Vec::new();
        reserve(self.session, &mut children, checked.len())?;
        for child in checked {
            let child = child.unwrap(); // all checked above, no callback since
            records.attach_source_child(self.session, &child)?;
            self.session.charge_host_work(1)?;
            children.push(child);
        }
        // Parent write-domain enrollment cannot precede child attachment. Every
        // selected direct ancestor was tainted, forcing this deferred route.
        for factory in deferred {
            self.session.charge_host_work(1)?;
            let (value, fact) = factory.finalize(self.session, &pairs)?;
            records.push(self.session, value, fact)?;
        }
        drop(pairs);
        Ok(Some(BoundParent {
            source: inputs.expected,
            value,
            records,
            children,
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
    fn source(phase: Phase) -> (Environment, Global, Arc<DefinitionInfo>) {
        let text = "(def ^number watched 17)";
        let forms = suss_reader::forms::read_forms(text).unwrap();
        let environment = hir::prepare(&forms, 0..text.len(), &Environment::default(), phase)
            .unwrap()
            .1;
        let binding = environment
            .resolve(phase, &suss_reader::Symbol::new("watched"), 0..0)
            .unwrap();
        let global = binding.global().clone();
        let info = environment.shared_definition_info(&global).unwrap().clone();
        (environment, global, info)
    }
    fn boundary(graph: &mut AnalysisGraph<'_>, mut root: Value) -> usize {
        for _ in 0..graph.recipes.len() {
            if let Some(id) = graph.source_roots.index(graph.session, root).unwrap() {
                return id;
            }
            match graph.recipes[root].as_ref().unwrap() {
                Recipe::Alias(next) => root = *next,
                _ => panic!("actual completed metadata must reach its captured boundary"),
            }
        }
        panic!("cyclic fixture metadata aliases")
    }
    fn original_alias_target(graph: &AnalysisGraph<'_>, mut value: Value) -> Value {
        for _ in 0..graph.recipes.len() {
            match graph.recipes[value].as_ref().unwrap() {
                Recipe::Alias(child) => value = *child,
                // Stop BEFORE Metadata or another real constructor. Matching
                // the actual node retains its complete metadata-producing DAG.
                _ => return value,
            }
        }
        panic!("cyclic original fixture alias lineage")
    }
    fn strict_completed_metadata(
        graph: &mut AnalysisGraph<'_>, global: &Global, info: &Arc<DefinitionInfo>,
    ) -> Value {
        use crate::portable::hir::MetadataKeyOrigin;
        use crate::portable_macro_graph::{metadata_expectations, source_facts};
        let owner = info.completed_values.as_ref().unwrap();
        let original_declaration = info.declaration.clone();
        // Do not build a DATA view and delete captures. Actual production final
        // fields are captured from the outset; automatic DATA decoration is off.
        assert!(graph.capture_roots);
        assert!(!graph.direct_reader_roots);
        graph.completed_owner_binding_full(global, owner, info).unwrap();
        let metadata_before = graph.metadata_sources.len();
        let declaration = graph.declaration(global, Some(info), 0).unwrap();
        graph.finish_recipes().unwrap();
        let fields = graph.completed_pairs(declaration).unwrap();
        let tag_position = graph.declaration_field_position(&fields, "tag").unwrap().unwrap();
        let tag_alias = fields[tag_position].1;
        let tag_id = graph.source_roots.index(graph.session, tag_alias).unwrap().unwrap();
        match &graph.source_roots.roots[tag_id].owner {
            Owner::DeclarationField(actual, holder, ordinal) => {
                assert_eq!(actual, global);
                assert!(Arc::ptr_eq(holder, info));
                assert_eq!(*ordinal, tag_position, "actual FINAL declaration ordinal");
            }
            _ => panic!("private child must be the actual production tag field"),
        }
        let original_export = {
            let captured = &graph.metadata_sources[metadata_before];
            assert_eq!(&captured.global, global);
            assert!(Arc::ptr_eq(&captured.revision, info));
            captured.recipe
        };
        let original_pairs = graph.completed_pairs(original_export).unwrap();
        let metadata = graph.completed_reader(owner).unwrap();
        assert_eq!(metadata.pairs.len(), 2, "complete original ^number metadata, no selection/filter");
        assert_eq!(metadata.origins.len(), 1);
        assert_eq!(metadata.key_positions.len(), 1);
        assert!(matches!(metadata.origins[0], MetadataKeyOrigin::ReaderShorthandTag));
        assert_eq!(original_pairs.len(), 1, "complete final metadata export");
        let tag_in_original_meta = graph.declaration_field_position(&original_pairs, "tag").unwrap().unwrap();
        // This actual source ordinal, not symbol spelling/Form equality, proves
        // the final tag and metadata value used ONE producer occurrence.
        assert_eq!(tag_in_original_meta, 0);
        assert_eq!(original_alias_target(graph, original_pairs[0].1),
            original_alias_target(graph, graph.source_roots.roots[tag_id].native),
            "a different inferred/top-fn/tag writer is not this strict fixture");
        let reader_source = graph.completed_reader_source(owner).unwrap();
        let position = metadata.key_positions[0];
        assert_eq!(position, (0, 0), "the original genuine shorthand prefix/pair");
        assert!(matches!(reader_source.metadata[position.0].kind, suss_reader::forms::Kind::Symbol(_)));
        assert_eq!(metadata.pairs[1], reader_source.metadata[position.0],
            "retain the original complete Symbol, source span and metadata");
        let original_projected = metadata_expectations::project(graph.session, &graph.recipes, original_export)
            .unwrap().unwrap();
        // The private reader-key constructor, selected by this genuine
        // normalized origin, emits Identifier rather than a located source
        // Form. Project that constructor independently of original_projected;
        // do NOT erase source/value spans or compare only keyword spelling.
        let expected_key_recipe = graph.reader_key_recipe(&metadata.pairs[0], metadata.origins[0]).unwrap();
        let expected_key = metadata_expectations::project(graph.session, &graph.recipes, expected_key_recipe)
            .unwrap().unwrap();
        assert_eq!(expected_key.kind, metadata.pairs[0].kind,
            "the complete actual keyword value must survive the origin-selected constructor");
        assert_eq!(expected_key.metadata, metadata.pairs[0].metadata,
            "no source-key metadata may be discarded by this fixture");
        let mut expected_pairs = metadata.pairs.clone();
        expected_pairs[0] = expected_key;
        assert_eq!(expected_pairs[1], reader_source.metadata[position.0],
            "the entire raw Symbol span/metadata remains exact after key projection");
        let expected_source = suss_reader::forms::Form {
            span: 0..0, metadata: vec![],
            kind: suss_reader::forms::Kind::Map(expected_pairs),
        };
        let expected = source_facts::normalized_copy(graph.session, &expected_source).unwrap();
        assert_eq!(original_projected, expected,
            "the original full metadata export must itself match the independent owner source");
        assert!(graph.source_roots.roots.iter().all(|root| !matches!(&root.owner,
            Owner::ReaderSource(_, _, ReaderSelector::Value(_) | ReaderSelector::NestedKey(_)))),
            "strict builder must not create/delete/relabel a DATA capture");
        assert!(source_facts::LogicalSourceWitness::from_normalized_source(graph.session, expected)
            .unwrap().is_some(), "independent complete owner view must be admissible");
        let key_alias = graph.capture_completed_reader_key(global, owner, info, position).unwrap();
        let key_id = graph.source_roots.index(graph.session, key_alias).unwrap().unwrap();
        match &graph.source_roots.roots[key_id].owner {
            Owner::ReaderSource(actual, super::super::reader_keys::ReaderSourceOwner::Completed(actual_owner), ReaderSelector::Key(_)) => {
                assert_eq!(actual, global);
                assert!(Arc::ptr_eq(actual_owner, owner));
                // ReaderKeyPath fields intentionally remain private. The
                // existing protected constructor receives the actual position
                // above and derives origin from the fully checked metadata.
                assert!(matches!(metadata.origins[0], MetadataKeyOrigin::ReaderShorthandTag));
            }
            _ => panic!("direct child must retain the original protected reader key origin"),
        }
        let mut keys = [Some(key_alias)];
        let values = [Some(tag_alias)];
        let native = graph.declaration_reader_metadata_map_with_values(
            &metadata, &mut keys, Some(&values), global.namespace() == "suss.core", 0,
        ).unwrap();
        graph.finish_recipes().unwrap();
        let assembled = metadata_expectations::project(graph.session, &graph.recipes, native)
            .unwrap().unwrap();
        assert_eq!(assembled, original_projected, "full original view, not a scalar-only tag check");
        assert_eq!(info.declaration, original_declaration, "no source mutation");
        let root = graph.capture_completed_metadata(global, owner, info, native).unwrap();
        let parent = graph.source_roots.index(graph.session, root).unwrap().unwrap();
        let layout = graph.source_root_children().unwrap();
        assert_eq!(layout[parent], [key_id, tag_id],
            "exactly TWO genuine original inputs in key/private-value order; none filtered");
        assert!(layout[key_id].is_empty());
        assert!(layout[tag_id].is_empty());
        assert!(!matches!(&graph.source_roots.roots[tag_id].owner,
            Owner::ReaderSource(_, _, ReaderSelector::Value(_) | ReaderSelector::NestedKey(_))));
        root
    }
    fn children(graph: &mut AnalysisGraph<'_>, id: usize) -> Vec<CheckedChild> {
        let layout = graph.source_root_children().unwrap();
        let mut result = Vec::new();
        for child in &layout[id] {
            assert!(
                layout[*child].is_empty(),
                "fixture keys have no captured child dependencies"
            );
            let expected = graph
                .source_root_expectation_with_layout(*child, &layout)
                .unwrap()
                .unwrap();
            let recording = graph.bridge.recording(graph.session).unwrap();
            let value = graph
                .materialize_using(graph.source_roots.roots[*child].alias, &recording)
                .unwrap();
            if std::env::var_os("SUSS_MIXED_CERT_TRACE").is_some() {
                eprintln!("MIXED_CHILD ordinal={} total={} boundary={} alias={} native={} key={} expected_children={}",
                    result.len(), layout[id].len(), child,
                    graph.source_roots.roots[*child].alias,
                    graph.source_roots.roots[*child].native,
                    super::super::TraceSourceKey(&expected.key), expected.children.len());
            }
            let proof = Certificate::finish(
                graph.session,
                graph.bridge,
                expected.clone(),
                value,
                recording.into_records().unwrap(),
                vec![],
            )
            .unwrap()
            .unwrap();
            result.push(
                Certificate::checked(graph.session, graph.bridge, &proof, &expected)
                    .unwrap()
                    .unwrap(),
            );
        }
        assert!(!result.is_empty());
        result
    }
    #[test]
    fn actual_metadata_parent_keeps_direct_key_reference_and_enrolls_children_before_snapshot() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let (_environment, global, info) = source(phase);
            let mut session = Session::new_macro().unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            graph.capture_roots = true;
            graph.direct_reader_roots = false;
            let root = strict_completed_metadata(&mut graph, &global, &info);
            let id = boundary(&mut graph, root);
            let children = children(&mut graph, id);
            let first = children[0].value().clone();
            let bound = graph
                .materialize_mixed_source_parent(id, children)
                .unwrap()
                .unwrap();
            let inspect = graph.session.eval("(fn [m key] (reduce (fn [seen candidate] (or seen (identical? candidate key))) false (keys m)))").unwrap();
            graph.session.collect().unwrap();
            let observed = graph
                .session
                .invoke(&inspect, &[&bound.value, &first])
                .unwrap();
            assert_eq!(
                crate::portable_repl::display(graph.session, &observed).unwrap(),
                "true"
            );
            let certificate = Certificate::finish_bound(graph.session, graph.bridge, bound)
                .unwrap()
                .unwrap();
            assert!(Certificate::validate(graph.session, graph.bridge, &certificate).unwrap());
        }
    }
    #[test]
    fn later_guest_key_poison_prevents_parent_publication_and_preserves_effect() {
        let (_environment, global, info) = source(Phase::Runtime);
        let mut session = Session::new_macro().unwrap();
        session.eval("(def key-poison-count 0)").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.capture_roots = true;
        graph.direct_reader_roots = false;
        let root = strict_completed_metadata(&mut graph, &global, &info);
        let id = boundary(&mut graph, root);
        let children = children(&mut graph, id);
        let key = children[0].value().clone();
        let bound = graph
            .materialize_mixed_source_parent(id, children)
            .unwrap()
            .unwrap();
        let poison = graph.session.eval("(fn [key] (do (set! key-poison-count (+ key-poison-count 1)) (set! (.-_hash key) 991)))").unwrap();
        graph.session.invoke(&poison, &[&key]).unwrap();
        graph.session.collect().unwrap();
        assert!(
            Certificate::finish_bound(graph.session, graph.bridge, bound)
                .unwrap()
                .is_none()
        );
        let effect = graph.session.eval("key-poison-count").unwrap();
        assert_eq!(
            crate::portable_repl::display(graph.session, &effect).unwrap(),
            "1"
        );
        let getter = graph.session.eval("(fn [key] (.-_hash key))").unwrap();
        let actual = graph.session.invoke(&getter, &[&key]).unwrap();
        assert_eq!(
            crate::portable_repl::display(graph.session, &actual).unwrap(),
            "991"
        );
    }
    #[test]
    fn mixed_parent_fuel_and_active_irq_leave_no_retained_publication_and_retry() {
        let (_environment, global, info) = source(Phase::Macro);
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.capture_roots = true;
        graph.direct_reader_roots = false;
        let root = strict_completed_metadata(&mut graph, &global, &info);
        let id = boundary(&mut graph, root);
        let supplied = children(&mut graph, id);
        let alias_count = graph.source_roots.roots.len();
        let recipe_count = graph.recipes.len();
        let retained = RootCache::default();
        // Only early bounded cuts are claimed; not every guest-return debit.
        for allowance in [0, 1] {
            graph.session.set_operation_fuel(allowance);
            let op = graph.session.operation_budget_scope().unwrap();
            let error = graph
                .materialize_mixed_source_parent(id, Vec::new())
                .err()
                .unwrap();
            assert!(matches!(error, SessionError::Trap(_)), "{error}");
            assert_eq!(retained.entries.len(), 0);
            assert_eq!(graph.source_roots.roots.len(), alias_count);
            assert_eq!(graph.recipes.len(), recipe_count);
            drop(op);
        }
        graph
            .session
            .set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        let op = graph.session.operation_budget_scope().unwrap();
        graph.session.interrupt_handle().interrupt();
        assert!(
            graph
                .materialize_mixed_source_parent(id, Vec::new())
                .err()
                .unwrap()
                .is_interrupt()
        );
        drop(op);
        let bound = graph
            .materialize_mixed_source_parent(id, supplied)
            .unwrap()
            .unwrap();
        assert!(
            Certificate::finish_bound(graph.session, graph.bridge, bound)
                .unwrap()
                .is_some()
        );
        assert_eq!(
            retained.entries.len(),
            0,
            "this staging API never publishes cache entries"
        );
    }
    #[test]
    fn original_completed_data_value_still_rejects_standalone_strict_certificate() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let (_environment, global, info) = source(phase);
            let owner = info.completed_values.as_ref().unwrap();
            let mut session = Session::new_macro().unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            graph.capture_roots = true;
            graph.direct_reader_roots = true;
            // ORIGINAL DATA producer and inventory stay intact in this separate
            // negative, not relabeled or filtered to construct the strict view.
            let root = graph.completed_metadata_map(&global, owner, &info, 0).unwrap();
            graph.finish_recipes().unwrap();
            let id = boundary(&mut graph, root);
            let layout = graph.source_root_children().unwrap();
            assert_eq!(layout[id].len(), 2);
            assert!(matches!(&graph.source_roots.roots[layout[id][0]].owner,
                Owner::ReaderSource(_, _, ReaderSelector::Key(_))));
            let value_id = layout[id][1];
            assert!(matches!(&graph.source_roots.roots[value_id].owner,
                Owner::ReaderSource(_, _, ReaderSelector::Value(_))));
            assert!(layout[value_id].is_empty());
            let expected = graph.source_root_expectation_with_layout(value_id, &layout).unwrap().unwrap();
            let recording = graph.bridge.recording(graph.session).unwrap();
            let value = graph.materialize_using(graph.source_roots.roots[value_id].alias, &recording).unwrap();
            let records = recording.into_records().unwrap();
            assert!(expected.witness.matches_logical_source(graph.session, graph.bridge, &value).unwrap());
            assert!(records.matches_direct_facts(graph.session).unwrap());
            assert!(records.matches_recorded_inputs(graph.session, &value).unwrap());
            assert!(records.matches_class_schemas(graph.session).unwrap());
            assert!(records.matches_prototype_origins(graph.session).unwrap());
            assert!(Certificate::finish(graph.session, graph.bridge, expected, value, records, vec![])
                .unwrap().is_none(), "healthy DATA Value must NOT become a strict source capability");
        }
    }

}
