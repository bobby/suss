//! Per-occurrence reader key recipes, never a global spelling/memo cache.
use super::*;
use crate::portable::hir::{MetadataKeyOrigin, ReaderMetadata};

impl AnalysisGraph<'_> {
    pub(super) fn reader_key_slots(&mut self, count: usize) -> Result<Vec<Option<Value>>> {
        self.charge(std::mem::size_of::<Vec<Option<Value>>>().saturating_mul(2).saturating_add(4))?;
        let mut slots = Vec::new();
        reuse::reserve(self.session, &mut slots, count)?;
        self.charge(count.saturating_mul(2))?;
        slots.resize(count, None);
        Ok(slots)
    }
    pub(super) fn reader_key_recipe(&mut self, form: &Form, origin: MetadataKeyOrigin) -> Result<Value> {
        self.charge(4)?;
        match origin {
            MetadataKeyOrigin::SourceSyntax => self.form(form, 0),
            MetadataKeyOrigin::ReaderShorthandTag => {
                // Origin is minted only by the reader's symbol/string branch.
                // Check the native constructor payload as well, not as authority.
                self.charge(8)?;
                if !matches!(&form.kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == "tag")
                    || !form.metadata.is_empty() {
                    return Err(SessionError::Host(wasmtime::Error::msg("Invalid reader shorthand key")));
                }
                // A fresh recipe for THIS normalized occurrence. In particular,
                // do not use keyword(), its global per-label literal cache.
                self.charge(3usize.saturating_add(std::mem::size_of::<Recipe>()).saturating_add(6))?;
                self.recipe(Recipe::Identifier {
                    namespace: None, name: "tag".into(), keyword: true,
                    literal: Some(AnalyzerLiteralKeyword::reader_shorthand_tag()),
                })
            }
        }
    }
    fn reader_metadata_map(
        &mut self, map: &Form, origins: &[MetadataKeyOrigin], supplied: &[Option<Value>], depth: usize,
    ) -> Result<Value> {
        self.charge(8)?;
        let Kind::Map(pairs) = &map.kind else {
            return Err(SessionError::Host(wasmtime::Error::msg("Expected reader metadata map")));
        };
        if pairs.len() % 2 != 0 || pairs.len() / 2 != origins.len() || supplied.len() != origins.len() {
            return Err(SessionError::Host(wasmtime::Error::msg("Reader metadata occurrence extent mismatch")));
        }
        // Same whole synthetic-map validator before selecting any key route;
        // values and losing-prefix validation were not replaced by origin facts.
        validate_form_data(map, &mut |units| self.session.charge_host_work(units))?;
        self.charge(std::mem::size_of::<Vec<(Value, Value)>>().saturating_mul(2).saturating_add(4))?;
        let mut entries = Vec::new();
        reuse::reserve(self.session, &mut entries, origins.len())?;
        self.charge(origins.len().saturating_mul(4))?;
        for (index, pair) in pairs.chunks_exact(2).enumerate() {
            self.charge(5)?;
            let key = match supplied[index] {
                Some(key) => key,
                None => self.reader_key_recipe(&pair[0], origins[index])?,
            };
            let value = self.form(&pair[1], depth + 1)?;
            entries.push((key, value));
        }
        self.recipe(Recipe::ReaderMap(entries))
    }
    pub(super) fn reader_metadata_recipe(&mut self, metadata: ReaderMetadata, span: std::ops::Range<usize>) -> Result<Value> {
        self.charge(metadata.origins.len().saturating_add(8))?;
        if metadata.pairs.len() % 2 != 0 || metadata.pairs.len() / 2 != metadata.origins.len()
            || metadata.key_positions.len() != metadata.origins.len() {
            return Err(SessionError::Host(wasmtime::Error::msg("Reader metadata carrier extent mismatch")));
        }
        let map = Form { span, metadata: vec![], kind: Kind::Map(metadata.pairs) };
        if !metadata.origins.iter().any(|origin| *origin == MetadataKeyOrigin::ReaderShorthandTag) {
            return self.form(&map, 0);
        }
        let keys = self.reader_key_slots(metadata.origins.len())?;
        self.reader_metadata_map(&map, &metadata.origins, &keys, 0)
    }
    pub(super) fn declaration_reader_metadata_map(
        &mut self, metadata: &ReaderMetadata, keys: &mut [Option<Value>], core_file: bool, depth: usize,
    ) -> Result<Value> {
        self.charge(12)?;
        if metadata.pairs.len() % 2 != 0 || metadata.pairs.len() / 2 != metadata.origins.len()
            || metadata.key_positions.len() != metadata.origins.len() || keys.len() != metadata.origins.len() {
            return Err(SessionError::Host(wasmtime::Error::msg("Declaration metadata occurrence extent mismatch")));
        }
        let count = metadata.origins.len().checked_add(usize::from(core_file))
            .ok_or_else(|| SessionError::Host(wasmtime::Error::msg("Declaration metadata extent overflow")))?;
        self.charge(std::mem::size_of::<ReaderMetadata>().saturating_mul(2).saturating_add(4))?;
        let mut pairs = Vec::new();
        let mut origins = Vec::new();
        reuse::reserve(self.session, &mut pairs, count.checked_mul(2)
            .ok_or_else(|| SessionError::Host(wasmtime::Error::msg("Declaration metadata extent overflow")))?)?;
        reuse::reserve(self.session, &mut origins, count)?;
        let mut supplied = self.reader_key_slots(count)?;
        let mut selected = 0;
        for (index, pair) in metadata.pairs.chunks_exact(2).enumerate() {
            self.charge(12)?;
            if matches!(&pair[0].kind, Kind::Keyword(key) if key.namespace.is_none()
                && (key.name == "test" || (core_file && key.name == "file"))) { continue; }
            // Preserve the old selected pair order and complete copied payload.
            pairs.push(self.clone_form(&pair[0])?);
            pairs.push(self.clone_form(&pair[1])?);
            origins.push(metadata.origins[index]);
            supplied[selected] = keys[index];
            selected += 1;
        }
        if core_file {
            self.charge(16usize.saturating_add("file".len()).saturating_add("cljs/core.cljs".len().saturating_mul(2)))?;
            pairs.push(Form { span: 0..0, metadata: vec![], kind: Kind::Keyword(suss_reader::Keyword::new("file")) });
            pairs.push(Form { span: 0..0, metadata: vec![], kind: Kind::String("cljs/core.cljs".encode_utf16().collect()) });
            origins.push(MetadataKeyOrigin::SourceSyntax);
            selected += 1;
        }
        self.charge(count.saturating_add(2))?;
        supplied.truncate(selected);
        let map = Form { span: 0..0, metadata: vec![], kind: Kind::Map(pairs) };
        self.reader_metadata_map(&map, &origins, &supplied, depth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reader_metadata_key_recipes_are_per_occurrence_and_shared_only_with_retained_owner() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let form = suss_reader::forms::read_forms("^number owner").unwrap().remove(0);
        let metadata = crate::portable_macro_data::normalized_reader_metadata_view(&mut session, &form).unwrap();
        assert_eq!(metadata.origins, vec![MetadataKeyOrigin::ReaderShorthandTag]);
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let a = graph.reader_key_recipe(&metadata.pairs[0], metadata.origins[0]).unwrap();
        let b = graph.reader_key_recipe(&metadata.pairs[0], metadata.origins[0]).unwrap();
        assert_ne!(a, b, "independent occurrence recipes cannot be globally interned by tag spelling");
        let mut slots = graph.reader_key_slots(1).unwrap(); slots[0] = Some(a);
        let map = graph.declaration_reader_metadata_map(&metadata, &mut slots, false, 0).unwrap();
        let Some(Recipe::ReaderMap(entries)) = graph.recipes[map].as_ref() else { panic!("reader map") };
        assert_eq!(entries[0].0, a);
        let mut retained = RetainedFieldKeys::new(&mut graph).unwrap();
        retained.remember_reader_occurrence(&mut graph, "tag", &metadata.pairs[0], a).unwrap();
        let tag = graph.symbol("number").unwrap();
        let outer = retained.finish(&mut graph, vec![("tag", tag)]).unwrap();
        let Some(Recipe::Map(entries)) = graph.recipes[outer].as_ref() else { panic!("declaration map") };
        assert_eq!(entries[0].0, a);
    }
    #[test]
    fn reader_metadata_origin_bridge_preserves_memos_and_source_controls_after_gc() {
        for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
            let bridge = FormBridge::new(&mut session).unwrap();
            let project = session.eval("(fn [x] (let [m (meta x) k (first (keys m))] [(str k) (.-_hash k) (str (get m k))]))").unwrap();
            for (source, numeric, value) in [
                ("^number owner", true, "number"),
                ("^\"number\" owner", true, "number"),
                ("^{:tag source-tag} owner", false, "source-tag"),
                ("^:private owner", false, "true"),
                ("^number ^{:tag source-tag} owner", false, "number"),
                ("^{:tag source-tag} ^number owner", true, "source-tag"),
                ("^\"number\" ^{:tag source-tag} owner", false, "number"),
                ("^{:tag source-tag} ^\"number\" owner", true, "source-tag"),
            ] {
                let form = suss_reader::forms::read_forms(source).unwrap().remove(0);
                let datum = bridge.quote_ref(&mut session, &form).unwrap();
                session.collect().unwrap();
                let result = session.invoke(&project, &[&datum]).unwrap();
                let decoded = bridge.read(&mut session, &result, 0..0).unwrap();
                let Kind::Vector(values) = &decoded.kind else { panic!("projection") };
                assert_eq!(values.len(), 3);
                assert_eq!(values[1].kind, if numeric { Kind::Number(-1290361223.0) } else { Kind::Nil }, "{source}");
                assert!(matches!(&values[2].kind,Kind::String(units) if units.iter().copied().eq(value.encode_utf16())), "{source}");
            }
        }
    }
    #[test]
    fn reader_metadata_transport_preserves_typed_fuel_irq_and_full_retry() {
        let mut session = Session::new_macro().unwrap();
        let form = suss_reader::forms::read_forms("^number ^{:tag source-tag :other [1 2]} owner").unwrap().remove(0);
        let original = form.clone();
        let total = {
            let request = session.operation_budget_scope().unwrap();
            let start = session.operation_fuel_remaining().unwrap();
            let metadata = crate::portable_macro_data::normalized_reader_metadata_view(&mut session, &form).unwrap();
            drop(metadata);
            let used = start - session.operation_fuel_remaining().unwrap();
            drop(request); used
        };
        for allowance in [0, 1, total / 2, total.saturating_sub(1), total] {
            session.set_operation_fuel(allowance);
            let request = session.operation_budget_scope().unwrap();
            if let Err(error) = crate::portable_macro_data::normalized_reader_metadata_view(&mut session, &form) {
                assert!(matches!(error, SessionError::Trap(ref e)
                    if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{error:?}");
            }
            drop(request);
            assert_eq!(form, original);
            session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
            let request = session.operation_budget_scope().unwrap();
            let retry = crate::portable_macro_data::normalized_reader_metadata_view(&mut session, &form).unwrap();
            assert_eq!(retry.origins.len() * 2, retry.pairs.len());
            drop(retry); drop(request);
        }
        let request = session.operation_budget_scope().unwrap();
        session.interrupt_handle().interrupt();
        let error = crate::portable_macro_data::normalized_reader_metadata_view(&mut session, &form)
            .err().expect("pending interrupt must propagate");
        assert!(error.is_interrupt());
        drop(request);
        let request = session.operation_budget_scope().unwrap();
        crate::portable_macro_data::normalized_reader_metadata_view(&mut session, &form).unwrap();
        drop(request);
        assert_eq!(form, original);
    }

    #[test]
    fn reader_metadata_source_clone_preserves_original_key_occurrence_selector() {
        for source in ["(def ^number watched 17)", "(def ^number ^number watched 17)",
            "(def ^number ^{:tag source-tag} watched 17)", "(def ^{:tag source-tag} ^number watched 17)"] {
            let forms = suss_reader::forms::read_forms(source).unwrap();
            let Kind::List(items) = &forms[0].kind else { unreachable!() };
            let owner = &items[1];
            let origin = portable::SourceOrigin::new(source, None);
            let (enriched, original_prefixes) = origin.macro_form_data_with_metadata_origins_metered(owner, &mut |_| Ok(())).unwrap();
            assert_eq!(original_prefixes, Some(owner.metadata.len()), "{source}");
            assert_eq!(enriched, origin.macro_form_data_metered(owner, &mut |_| Ok(())).unwrap());
            let metadata = hir::reader_metadata_owned_view_metered(&enriched, &mut |_| Ok(())).unwrap();
            let index = metadata.pairs.chunks_exact(2).position(|pair|
                matches!(&pair[0].kind,Kind::Keyword(key) if key.name == "tag")).unwrap();
            assert_eq!(metadata.key_positions[index], (owner.metadata.len()-1, 0),
                "first key selector survives later equal-key/value updates: {source}");
        }
    }
    #[test]
    fn reader_metadata_supplied_slot_extent_is_checked_before_publication() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let form = suss_reader::forms::read_forms("^number owner").unwrap().remove(0);
        let metadata = crate::portable_macro_data::normalized_reader_metadata_view(&mut session, &form).unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let before = graph.recipes.len();
        assert!(matches!(graph.declaration_reader_metadata_map(&metadata, &mut [], false, 0), Err(SessionError::Host(_))));
        assert_eq!(graph.recipes.len(), before);
    }

}
