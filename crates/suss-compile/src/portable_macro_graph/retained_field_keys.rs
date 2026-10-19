//! Private declaration-key facts, separate from the existing value update list.
//! No guest references or memo observations can create one of these entries.
use super::*;

#[derive(Clone, Copy)]
enum Key {
    Literal(AnalyzerLiteralKeyword),
    Source(Value),
}
#[derive(Clone, Copy)]
struct Entry {
    label: &'static str,
    key: Key,
}
/// Invocation/graph-local recipe IDs only. Never persistent cache authority.
pub(super) struct RetainedFieldKeys {
    entries: Vec<Entry>,
    ordinary_named: bool,
}
macro_rules! retained_decl_push {
    ($graph:ident, $retained:ident, $fields:ident; $label:expr, $token:expr, $value:expr) => {{
        // Preserve the original value's native validation/evaluation position.
        let value = $value;
        $graph.charge(2)?;
        if let Some(keys) = $retained.as_mut() {
            keys.remember_named($graph, $token)?;
        }
        $fields.push(($label, value));
    }};
}
impl RetainedFieldKeys {
    pub(super) fn new(graph: &mut AnalysisGraph<'_>) -> Result<Self> {
        Self::with_named_policy(graph, true)
    }
    pub(super) fn type_history(graph: &mut AnalysisGraph<'_>) -> Result<Self> {
        Self::with_named_policy(graph, false)
    }
    fn with_named_policy(graph: &mut AnalysisGraph<'_>, ordinary_named: bool) -> Result<Self> {
        // Actual enlarged header, policy init and retirement before construction.
        graph.charge(std::mem::size_of::<Self>().saturating_mul(2).saturating_add(5))?;
        Ok(Self { entries: Vec::new(), ordinary_named })
    }
    fn find(&self, graph: &mut AnalysisGraph<'_>, label: &str) -> Result<Option<Key>> {
        graph.charge(4)?; // header read, iterator and result controls
        for entry in &self.entries {
            graph.charge(entry.label.len().saturating_add(label.len()).saturating_add(4))?;
            if entry.label == label { return Ok(Some(entry.key)); }
        }
        Ok(None)
    }
    fn push(&mut self, graph: &mut AnalysisGraph<'_>, entry: Entry) -> Result<()> {
        // Actual new representation, not only a delta against another buffer.
        // Every append prepays payload initialization and push controls even
        // when capacity is reused. Entry is Copy: no fictitious element Drop.
        graph.charge(12usize.saturating_add(std::mem::size_of::<Entry>()))?;
        let length = self.entries.len();
        let capacity = self.entries.capacity();
        if length == capacity {
            graph.charge(8)?;
            let required = length.checked_add(1).expect("retained key extent");
            let requested = capacity.checked_mul(2).unwrap_or(required).max(required).max(4);
            let bytes = requested.checked_mul(std::mem::size_of::<Entry>())
                .and_then(|n| length.checked_mul(std::mem::size_of::<Entry>()).and_then(|m| n.checked_add(m)))
                .expect("retained key backing extent");
            // Requested backing plus all live relocation bytes; fixed retired
            // buffer/deallocation controls are prepaid before acquisition too.
            graph.charge(bytes.saturating_add(4))?;
            self.entries.reserve_exact(requested - length);
        }
        self.entries.push(entry);
        Ok(())
    }
    pub(super) fn remember_named(
        &mut self, graph: &mut AnalysisGraph<'_>, token: AnalyzerLiteralKeyword,
    ) -> Result<()> {
        graph.charge(1)?; // new policy read/branch; ordinary token fees remain below
        if !self.ordinary_named { return Ok(()); }
        self.remember_proved_named(graph, token)
    }
    pub(super) fn remember_proved_named(
        &mut self, graph: &mut AnalysisGraph<'_>, token: AnalyzerLiteralKeyword,
    ) -> Result<()> {
        graph.charge(3)?; // token read, branch and entry selection
        let label = token.name();
        if self.find(graph, label)?.is_none() {
            self.push(graph, Entry { label, key: Key::Literal(token) })?;
        }
        Ok(())
    }
    pub(super) fn remember_source(
        &mut self, graph: &mut AnalysisGraph<'_>, label: &'static str, form: &Form,
    ) -> Result<()> {
        graph.charge(label.len().saturating_add(8))?;
        // A selected field name is only a selector. The full KEY comes from
        // these actual immutable native Form facts, validated by graph.form.
        if !matches!(&form.kind, Kind::Keyword(key)
            if key.namespace.is_none() && key.name == label) {
            return Err(SessionError::Host(wasmtime::Error::msg("Invalid retained source key")));
        }
        if self.find(graph, label)?.is_none() {
            let value = graph.form(form, 0)?;
            self.push(graph, Entry { label, key: Key::Source(value) })?;
        }
        Ok(())
    }
    pub(super) fn remember_proved_occurrence(
        &mut self, graph: &mut AnalysisGraph<'_>, label: &'static str, form: &Form, occurrence: Value,
    ) -> Result<()> {
        graph.charge(label.len().saturating_add(8))?;
        if !matches!(&form.kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == label) {
            return Err(SessionError::Host(wasmtime::Error::msg("Invalid retained native key")));
        }
        if self.find(graph, label)?.is_none() {
            self.push(graph, Entry { label, key: Key::Source(occurrence) })?;
        }
        Ok(())
    }
    pub(super) fn remember_reader_occurrence(
        &mut self, graph: &mut AnalysisGraph<'_>, label: &'static str,
        form: &Form, occurrence: Value,
    ) -> Result<()> {
        graph.charge(label.len().saturating_add(8))?;
        if !matches!(&form.kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == label) {
            return Err(SessionError::Host(wasmtime::Error::msg("Invalid retained reader key")));
        }
        if self.find(graph, label)?.is_none() {
            self.push(graph, Entry { label, key: Key::Source(occurrence) })?;
        }
        Ok(())
    }
    pub(super) fn finish(
        self, graph: &mut AnalysisGraph<'_>, fields: Vec<(&'static str, Value)>,
    ) -> Result<Value> {
        // This replaces the ORIGINAL map's key conversion Vec, not a second
        // field/value buffer. Original keyword/map allocation charges remain.
        let entries = fields.into_iter().map(|(label, value)| {
            graph.charge(3)?;
            let key = match self.find(graph, label)? {
                Some(Key::Literal(token)) => graph.keyword(FieldKey::Literal(token))?,
                Some(Key::Source(key)) => key,
                None => graph.keyword(label)?,
            };
            Ok((key, value))
        }).collect::<Result<Vec<_>>>()?;
        graph.map_values(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source_keys() -> Vec<Form> {
        suss_reader::forms::read_forms(":doc :meta :fn-var :variadic? :max-fixed-arity :method-params").unwrap()
    }
    fn build(graph: &mut AnalysisGraph<'_>, source: &[Form], costs: &mut Vec<u64>) -> Result<Value> {
        let before = graph.session.operation_fuel_remaining()?;
        let mut keys = RetainedFieldKeys::new(graph)?;
        keys.remember_named(graph, AnalyzerLiteralKeyword("name"))?;
        costs.push(before - graph.session.operation_fuel_remaining()?);
        let mut values = Vec::new();
        for (index, (label, form)) in ["doc", "meta", "fn-var", "variadic?", "max-fixed-arity", "method-params"].into_iter().zip(source).enumerate() {
            keys.remember_source(graph, label, form)?;
            // A later computed value must not replace that raw source KEY.
            keys.remember_named(graph, match label {
                "doc" => AnalyzerLiteralKeyword("doc"), "meta" => AnalyzerLiteralKeyword("meta"),
                "fn-var" => AnalyzerLiteralKeyword("fn-var"), "variadic?" => AnalyzerLiteralKeyword("variadic?"),
                "max-fixed-arity" => AnalyzerLiteralKeyword("max-fixed-arity"),
                _ => AnalyzerLiteralKeyword("method-params"),
            })?;
            values.push((label, graph.number(index + 17)?));
            costs.push(before - graph.session.operation_fuel_remaining()?);
        }
        keys.finish(graph, values)
    }
    #[test]
    fn retained_declaration_source_keys_survive_value_updates_and_share_native_form_recipe() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let forms = source_keys();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let mut keys = RetainedFieldKeys::new(&mut graph).unwrap();
        keys.remember_named(&mut graph, AnalyzerLiteralKeyword("name")).unwrap();
        let raw_name = suss_reader::forms::read_forms(":name").unwrap().remove(0);
        keys.remember_source(&mut graph, "name", &raw_name).unwrap();
        assert!(matches!(keys.find(&mut graph, "name").unwrap(), Some(Key::Literal(_))));
        keys.remember_source(&mut graph, "doc", &forms[0]).unwrap();
        keys.remember_named(&mut graph, AnalyzerLiteralKeyword("doc")).unwrap();
        let expected = graph.form(&forms[0], 0).unwrap();
        assert!(matches!(keys.find(&mut graph, "doc").unwrap(), Some(Key::Source(id)) if id == expected));
        let original = forms.clone();
        let root = build(&mut graph, &forms, &mut Vec::new()).unwrap();
        let Recipe::Map(entries) = graph.recipes[root].as_ref().unwrap() else { panic!("missing map"); };
        assert_eq!(entries.len(), 6);
        assert_eq!(entries[0].0, expected);
        let actual = graph.materialize(root).unwrap();
        drop(graph);
        session.collect().unwrap();
        let form = bridge.read(&mut session, &actual, 0..0).unwrap();
        assert!(matches!(form.kind, Kind::Map(ref values) if values.len() == 12));
        assert_eq!(forms, original);
    }
    #[test]
    fn retained_declaration_key_failures_keep_source_and_recover_after_growth() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let source = source_keys();
        let original = source.clone();
        let mut costs = Vec::new();
        {
            let request = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            build(&mut graph, &source, &mut costs).unwrap();
            drop(graph); drop(request);
        }
        let mut samples = vec![0, 1];
        for cost in costs { samples.extend([cost.saturating_sub(1), cost, cost + 1]); }
        samples.sort_unstable(); samples.dedup();
        for allowance in samples {
            session.set_operation_fuel(allowance);
            let request = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            if let Err(error) = build(&mut graph, &source, &mut Vec::new()) {
                assert!(matches!(error, SessionError::Trap(ref e) if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{allowance}: {error:?}");
            }
            drop(graph); drop(request);
            assert_eq!(source, original);
            session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
            let request = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            build(&mut graph, &source, &mut Vec::new()).unwrap();
            drop(graph); drop(request);
        }
        let request = session.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.session.interrupt_handle().interrupt();
        assert!(build(&mut graph, &source, &mut Vec::new()).unwrap_err().is_interrupt());
        drop(graph); drop(request);
        let request = session.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        build(&mut graph, &source, &mut Vec::new()).unwrap();
        drop(graph); drop(request);
        assert_eq!(source, original);
    }
    #[test]
    fn retained_declaration_key_wrong_native_kind_is_rejected_without_publication() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let bad = suss_reader::forms::read_forms("\"doc\"").unwrap().remove(0);
        let original = bad.clone();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let mut keys = RetainedFieldKeys::new(&mut graph).unwrap();
        assert!(matches!(keys.remember_source(&mut graph, "doc", &bad), Err(SessionError::Host(_))));
        assert!(keys.entries.is_empty());
        assert!(graph.recipes.is_empty());
        assert_eq!(bad, original);
    }

    #[test]
    fn fn_name_var_literal_proof_is_separate_from_parameter_and_adapted_roles() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let mut binding = LocalBinding {
            identity: Arc::new(()), id: hir::BindingId(0), ty: hir::Type::Value,
            declaration: Arc::new(suss_reader::forms::read_forms("self-name").unwrap().remove(0)),
            origin: None, kind: hir::LocalKind::FunctionName,
            declaration_context: portable::AnalysisContext::Expression,
            source_role: SourceRole::FunctionName { variadic: false, methods: None },
            initializer: None, shadow: None, shadow_field: None,
        };
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        for token in [AnalyzerLiteralKeyword("form"), AnalyzerLiteralKeyword("local"), AnalyzerLiteralKeyword("info")] {
            assert!(matches!(graph.binding_literal_key(&binding, token).unwrap(), FieldKey::Cold(_)));
            assert!(matches!(graph.name_var_literal_key(&binding, token).unwrap(), FieldKey::Literal(_)));
        }
        binding.kind = hir::LocalKind::Let; // same reported kind cannot mint the fn-name proof
        assert!(matches!(graph.name_var_literal_key(&binding, AnalyzerLiteralKeyword("form")).unwrap(), FieldKey::Cold(_)));
        binding.source_role = SourceRole::PrivateCatch { anchor: 0..1 };
        assert!(matches!(graph.name_var_literal_key(&binding, AnalyzerLiteralKeyword("form")).unwrap(), FieldKey::Cold(_)));
    }
}
