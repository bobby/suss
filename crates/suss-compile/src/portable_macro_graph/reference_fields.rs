//! Semantic selection within native declaration recipes, never guest authority.
use super::*;

impl AnalysisGraph<'_> {
    fn native_field_key_is(&mut self, mut key: Value, label: &str) -> Result<bool> {
        self.charge(2)?;
        // An acyclic alias path cannot exceed the already-owned recipe arena.
        // This is a cycle bound, not a limit on finite source/transport size.
        let bound = self.recipes.len();
        for _ in 0..bound {
            self.charge(label.len().saturating_add(6))?;
            match self.recipes.get(key).and_then(Option::as_ref) {
                Some(Recipe::Identifier { namespace, name, keyword, .. }) => {
                    return Ok(*keyword && namespace.is_none() && name == label);
                }
                Some(Recipe::Form(form)) => {
                    return Ok(matches!(&form.kind, Kind::Keyword(name)
                        if name.namespace.is_none() && name.name == label));
                }
                Some(Recipe::Alias(next)) => key = *next,
                Some(_) => return Ok(false),
                None => return Err(SessionError::Host(wasmtime::Error::msg("Missing native field key recipe"))),
            }
        }
        Err(SessionError::Host(wasmtime::Error::msg("Cyclic native field key recipe")))
    }
    pub(super) fn declaration_field_position(&mut self, fields: &[(Value, Value)], label: &str) -> Result<Option<usize>> {
        self.charge(3)?;
        for (index, (key, _)) in fields.iter().enumerate() {
            self.charge(2)?;
            if self.native_field_key_is(*key, label)? { return Ok(Some(index)); }
        }
        Ok(None)
    }
    pub(super) fn declaration_field_value(&mut self, fields: &[(Value, Value)], label: &str) -> Result<Option<Value>> {
        Ok(self.declaration_field_position(fields, label)?.map(|index| fields[index].1))
    }
    pub(super) fn declaration_field_assoc(&mut self, fields: &mut Vec<(Value, Value)>, token: AnalyzerLiteralKeyword, value: Value) -> Result<()> {
        // Pinned resolve-var merges into the declaration: replacing a value
        // retains its earlier KEY and position. A new key has this independent
        // explicit literal constructor, regardless of other same-text recipes.
        if let Some(index) = self.declaration_field_position(fields, token.name())? {
            self.charge(2)?;
            fields[index].1 = value;
        } else {
            let key = self.keyword(FieldKey::Literal(token))?;
            reuse::reserve(self.session, fields, 1)?;
            self.charge(2)?;
            fields.push((key, value));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_reference_fields_keep_literal_and_source_keys_and_tag_values_after_gc() {
        for mut session in [Session::new_macro().unwrap(), Session::new_repl().unwrap()] {
            let bridge = FormBridge::new(&mut session).unwrap();
            let actual = {
                let mut graph = AnalysisGraph::new(&bridge, &mut session);
                let name_key = graph.keyword(FieldKey::Literal(AnalyzerLiteralKeyword("name"))).unwrap();
                let tag_key = graph.keyword(FieldKey::Literal(AnalyzerLiteralKeyword("tag"))).unwrap();
                let cold_tag = graph.keyword("tag").unwrap();
                assert_ne!(tag_key, cold_tag);
                let source_key = suss_reader::forms::read_forms(":ns").unwrap().remove(0);
                let ns_key = graph.form(&source_key, 0).unwrap();
                let old_name = graph.symbol("raw-name").unwrap();
                let old_ns = graph.symbol("raw-ns").unwrap();
                let tag = graph.symbol("number").unwrap();
                let mut fields = vec![(name_key, old_name), (tag_key, tag), (ns_key, old_ns)];
                assert!(!fields.iter().any(|(key,_)| *key == cold_tag));
                assert_eq!(graph.declaration_field_value(&fields, "tag").unwrap(), Some(tag));
                let original = fields.clone();
                let name = graph.symbol("ref.lookup/reference-num").unwrap();
                let ns = graph.symbol("ref.lookup").unwrap();
                let op = graph.keyword("var").unwrap();
                graph.declaration_field_assoc(&mut fields, AnalyzerLiteralKeyword("name"), name).unwrap();
                graph.declaration_field_assoc(&mut fields, AnalyzerLiteralKeyword("op"), op).unwrap();
                graph.declaration_field_assoc(&mut fields, AnalyzerLiteralKeyword("ns"), ns).unwrap();
                assert_eq!(fields.len(), 4);
                assert_eq!(fields[0], (name_key, name));
                assert_eq!(fields[2], (ns_key, ns));
                assert_eq!(original[0], (name_key, old_name));
                assert_eq!(original[2], (ns_key, old_ns));
                assert!(matches!(graph.recipes[fields[3].0].as_ref(), Some(Recipe::Identifier { literal: Some(_), keyword: true, .. })));
                let root = graph.map_values(fields).unwrap();
                graph.materialize(root).unwrap()
            };
            session.collect().unwrap();
            let project = session.eval("(fn [info] [(:name info) (:ns info) (:tag info) (:op info) (count info)])").unwrap();
            let value = session.invoke(&project, &[&actual]).unwrap();
            let Kind::Vector(values) = bridge.read(&mut session, &value, 0..0).unwrap().into_parts().2 else { panic!("projection vector") };
            assert_eq!(values.len(), 5);
            assert!(matches!(&values[0].kind, Kind::Symbol(s) if s.namespace.as_deref()==Some("ref.lookup") && s.name=="reference-num"));
            assert!(matches!(&values[1].kind, Kind::Symbol(s) if s.namespace.is_none() && s.name=="ref.lookup"));
            assert!(matches!(&values[2].kind, Kind::Symbol(s) if s.name=="number"));
            assert!(matches!(&values[3].kind, Kind::Keyword(k) if k.namespace.is_none() && k.name=="var"));
            assert_eq!(values[4].kind, Kind::Number(4.0));
        }
    }
    #[test]
    fn native_reference_field_selection_accepts_aliases_preserves_false_and_rejects_cycles() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let tag = graph.keyword(FieldKey::Literal(AnalyzerLiteralKeyword("tag"))).unwrap();
        let alias = graph.recipe(Recipe::Alias(tag)).unwrap();
        let value = graph.flag(false).unwrap();
        let fields = vec![(alias, value)];
        assert_eq!(graph.declaration_field_value(&fields, "tag").unwrap(), Some(value));
        let symbol = graph.symbol("tag").unwrap();
        let qualified = graph.keyword("other/tag").unwrap();
        assert_eq!(graph.declaration_field_position(&[(symbol,value),(qualified,value)], "tag").unwrap(), None);
        let cycle = graph.recipe(Recipe::Alias(alias)).unwrap();
        graph.recipes[cycle] = Some(Recipe::Alias(cycle));
        let mut invalid = vec![(cycle, value)];
        let before = invalid.clone();
        assert!(matches!(graph.declaration_field_assoc(&mut invalid, AnalyzerLiteralKeyword("tag"), value), Err(SessionError::Host(_))));
        assert_eq!(invalid, before);
    }
}
