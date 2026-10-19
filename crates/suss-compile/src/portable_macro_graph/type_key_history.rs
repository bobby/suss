//! Transport only the explicit native parse-type facts, never blanket type keys.
use super::*;
use crate::portable::resolve::{TypeKey, TypeKeyOrigin, TypeKeyLiteralSite, RetainedKey, RetainedSite};

impl AnalysisGraph<'_> {
    pub(super) fn seed_type_key_history(
        &mut self, keys: &mut RetainedFieldKeys, global: &portable::resolve::Global,
        info: &portable::resolve::DefinitionInfo,
    ) -> Result<()> {
        self.charge(6)?;
        let Some(history) = &info.type_key_history else { return Ok(()); };
        for field in TypeKey::ALL {
            // Global equality can only compare a full string after equal lengths.
            // Prepay both strings, key slot selection and origin dispatch each time.
            self.charge(global.namespace().len().saturating_add(global.name().len())
                .saturating_mul(2).saturating_add(12))?;
            let token = match field {
                TypeKey::Name => AnalyzerLiteralKeyword("name"),
                TypeKey::Tag => AnalyzerLiteralKeyword("tag"),
                TypeKey::Type => AnalyzerLiteralKeyword("type"),
                TypeKey::NumFields => AnalyzerLiteralKeyword("num-fields"),
                TypeKey::Record => AnalyzerLiteralKeyword("record"),
            };
            match history.key(global, field) {
                Some(TypeKeyOrigin::Literal(site)) => {
                    self.charge(4)?;
                    let valid = match site {
                        TypeKeyLiteralSite::OrdinaryName => field == TypeKey::Name,
                        TypeKeyLiteralSite::CompletedTag => field == TypeKey::Tag,
                        TypeKeyLiteralSite::ParseType(original) => *original == field,
                        TypeKeyLiteralSite::Retained(_) => false,
                    };
                    if !valid { return Err(SessionError::Host(wasmtime::Error::msg("Invalid native type key site"))); }
                    keys.remember_proved_named(self, token)?;
                }
                Some(TypeKeyOrigin::Source(form)) => keys.remember_source(self, field.name(), form)?,
                Some(TypeKeyOrigin::ReaderShorthandTag(form)) => {
                    self.charge(12)?;
                    if field != TypeKey::Tag
                        || !matches!(&form.kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == "tag")
                        || !form.metadata.is_empty()
                    { return Err(SessionError::Host(wasmtime::Error::msg("Invalid retained shorthand type key"))); }
                    validate_form_data(form, &mut |units| self.session.charge_host_work(units))?;
                    // Fresh recipe for this genuine retained occurrence, outside
                    // keyword()'s same-label literal cache. No guest memo proof.
                    self.charge(std::mem::size_of::<Recipe>().saturating_add(9))?;
                    let key = self.recipe(Recipe::Identifier {
                        namespace: None, name: "tag".into(), keyword: true, literal: Some(token),
                    })?;
                    keys.remember_proved_occurrence(self, "tag", form, key)?;
                }
                Some(TypeKeyOrigin::Unknown) | None => (),
            }
        }
        self.seed_retained_type_keys(keys, global, history)?;
        Ok(())
    }
    fn seed_retained_type_keys(&mut self, keys: &mut RetainedFieldKeys,
        global: &portable::resolve::Global, history: &portable::resolve::TypeKeyHistory,
    ) -> Result<()> {
        for field in RetainedKey::ALL {
            self.charge(global.namespace().len().saturating_add(global.name().len()).saturating_mul(2).saturating_add(14))?;
            let token = match field {
                RetainedKey::Ns => AnalyzerLiteralKeyword("ns"),
                RetainedKey::Private => AnalyzerLiteralKeyword("private"),
                RetainedKey::Dynamic => AnalyzerLiteralKeyword("dynamic"),
                RetainedKey::Doc => AnalyzerLiteralKeyword("doc"),
                RetainedKey::Declared => AnalyzerLiteralKeyword("declared"),
                RetainedKey::RetTag => AnalyzerLiteralKeyword("ret-tag"),
                RetainedKey::FnVar => AnalyzerLiteralKeyword("fn-var"),
                RetainedKey::Variadic => AnalyzerLiteralKeyword("variadic?"),
                RetainedKey::MaxFixedArity => AnalyzerLiteralKeyword("max-fixed-arity"),
                RetainedKey::MethodParams => AnalyzerLiteralKeyword("method-params"),
                RetainedKey::Arglists => AnalyzerLiteralKeyword("arglists"),
                RetainedKey::ArglistsMeta => AnalyzerLiteralKeyword("arglists-meta"),
                RetainedKey::Export => AnalyzerLiteralKeyword("export"),
                RetainedKey::Meta => AnalyzerLiteralKeyword("meta"),
                RetainedKey::Line => AnalyzerLiteralKeyword("line"),
                RetainedKey::Column => AnalyzerLiteralKeyword("column"),
                RetainedKey::File => AnalyzerLiteralKeyword("file"),
            };
            match history.retained_key(global, field) {
                Some(TypeKeyOrigin::Literal(TypeKeyLiteralSite::Retained(site))) => {
                    self.charge(8)?;
                    let valid = match site {
                        RetainedSite::CompletedMeta => field == RetainedKey::Meta,
                        RetainedSite::Docstring => field == RetainedKey::Doc,
                        RetainedSite::SourceInfo(original) => *original == field
                            && matches!(field, RetainedKey::Line | RetainedKey::Column | RetainedKey::File),
                        RetainedSite::CoreFile => field == RetainedKey::File,
                        RetainedSite::FunctionField(original) => *original == field
                            && (RetainedKey::FUNCTION.contains(&field) || field == RetainedKey::RetTag),
                        RetainedSite::ForwardDeclared(original) => *original == field
                            && matches!(field, RetainedKey::Declared | RetainedKey::FnVar | RetainedKey::MethodParams),
                    };
                    if !valid { return Err(SessionError::Host(wasmtime::Error::msg("Invalid retained declaration constructor"))); }
                    keys.remember_proved_named(self, token)?;
                }
                Some(TypeKeyOrigin::Source(form)) => keys.remember_source(self, field.name(), form)?,
                Some(TypeKeyOrigin::Literal(_)) | Some(TypeKeyOrigin::ReaderShorthandTag(_)) => {
                    return Err(SessionError::Host(wasmtime::Error::msg("Invalid retained declaration key origin")));
                }
                Some(TypeKeyOrigin::Unknown) | None => (),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::resolve::Phase;
    #[test]
    fn type_key_history_transport_uses_native_sites_preserves_raw_keys_and_keeps_unproved_fields_cold() {
        for (phase, mut session) in [(Phase::Runtime, Session::new_repl().unwrap()), (Phase::Macro, Session::new_macro().unwrap())] {
            let bridge = FormBridge::new(&mut session).unwrap();
            use crate::portable_session::memo_leaf_test_support::Method;
            session.eval("(def leaf-original-method nil) (def type-key-events [])").unwrap();
            let original = Method::capture(&mut session, "Keyword").unwrap();
            original.bind_delegate(&mut session).unwrap();
            session.eval(r#"(extend-type Keyword IHash (-hash [self]
                (let [memo (.-_hash self)]
                    (do (set! type-key-events (conj type-key-events [self memo]))
                        (if (nil? memo) (leaf-original-method self) memo)))))"#).unwrap();
            let project = session.eval(r#"(fn [info]
                (mapv (fn [label]
                    (let [key (reduce (fn [found k] (if (= (str k) label) k found)) nil (keys info))
                          events (filter (fn [event] (identical? key (first event))) type-key-events)]
                        (if (seq events) (second (first events)) (.-_hash key))))
                    [":name" ":tag" ":type" ":num-fields" ":record" ":meta"]))"#).unwrap();
            for (source, tag_literal, raw_flags, meta_literal) in [
                ("(deftype* subject [] nil nil)", true, false, false),
                ("(defrecord* subject [] nil nil)", true, false, false),
                ("(deftype* ^{:tag override :type false :num-fields 99 :record true} subject [] nil nil)", true, false, false),
                ("(def ^{:tag original :type false :num-fields 99 :record true} subject 17) (deftype* subject [] nil nil)", false, true, true),
                ("(def ^number subject 17) (deftype* subject [] nil nil)", true, false, true),
                ("(def ^{:tag original} subject 17) (defrecord* subject [] nil nil) (deftype* subject [x] nil nil)", false, false, true),
            ] {
                session.eval("(set! type-key-events [])").unwrap();
                let forms = suss_reader::forms::read_forms(source).unwrap();
                let (_, environment) = portable::prepare(&forms, 0..source.len(), &portable::resolve::Environment::default(), phase).unwrap();
                let binding = environment.resolve(phase, &suss_reader::Symbol::new("subject"), 0..0).unwrap();
                let global = binding.global();
                let info = environment.shared_definition_info(global).unwrap();
                let actual = {
                    let mut graph = AnalysisGraph::new(&bridge, &mut session);
                    let root = graph.declaration(global, Some(info), 0).unwrap();
                    let Some(Recipe::Map(entries)) = graph.recipes[root].as_ref() else { panic!("declaration map"); };
                    // Extra native/Suss metadata keys must not acquire blanket
                    // literal provenance merely because this is a type record.
                    for (key, _) in entries {
                        if let Some(Recipe::Identifier { name, literal, .. }) = graph.recipes[*key].as_ref() {
                            if name == "meta" {
                                assert_eq!(literal.is_some(), meta_literal, "only an actual prior meta constructor qualifies");
                            }
                        }
                    }
                    graph.materialize(root).unwrap()
                };
                session.collect().unwrap();
                let result = session.invoke(&project, &[&actual]).unwrap();
                let decoded = bridge.read(&mut session, &result, 0..0).unwrap();
                let Kind::Vector(memos) = &decoded.kind else { panic!("memo vector"); };
                assert_eq!(memos.len(), 6);
                // Exact initial literal memos independently observed in the
                // pinned type-history reference, not merely "some number".
                assert_eq!(memos[5].kind, if meta_literal { Kind::Number(1499536964.0) } else { Kind::Nil }, "{source}");
                assert_eq!(memos[0].kind, Kind::Number(1843675177.0), "{source}");
                assert_eq!(memos[1].kind, if tag_literal { Kind::Number(-1290361223.0) } else { Kind::Nil }, "{source}");
                for (memo, expected) in memos[2..5].iter().zip([1174270348.0, 1529154024.0, -779106859.0]) {
                    assert_eq!(memo.kind, if raw_flags { Kind::Nil } else { Kind::Number(expected) }, "{source}");
                }
            }
            original.restore(&mut session).unwrap();
            session.collect().unwrap();
            let restored = session.eval("(hash :tag)").unwrap();
            assert_eq!(bridge.read(&mut session, &restored, 0..0).unwrap().kind, Kind::Number(-1290361223.0));
        }
    }
    #[test]
    fn type_key_history_retained_projection_matches_independent_ordinary_graph_keys() {
        let cases = [
            "(def ^{:doc raw-doc :meta {:x 1} :fn-var false :variadic? false} subject 17)",
            "(def ^{:doc raw-doc} subject \"computed doc\" 17)",
            "(def subject (fn* [x] x))",
            "(def ^{:top-fn {:variadic? false :max-fixed-arity 7}} subject (fn* [] 17))",
            "(def ^{:declared true :arglists '([x])} subject)",
        ];
        for (phase, mut session) in [(Phase::Runtime, Session::new_repl().unwrap()), (Phase::Macro, Session::new_macro().unwrap())] {
            let bridge = FormBridge::new(&mut session).unwrap();
            for source in cases {
                let forms = suss_reader::forms::read_forms(source).unwrap();
                let (_, environment) = portable::prepare(&forms, 0..source.len(), &portable::resolve::Environment::default(), phase).unwrap();
                let binding = environment.resolve(phase, &suss_reader::Symbol::new("subject"), 0..0).unwrap();
                let global = binding.global();
                let info = environment.shared_definition_info(global).unwrap();
                let history = portable::resolve::TypeKeyHistory::capture(global, Some(info), &mut |_| Ok(())).unwrap();
                let mut graph = AnalysisGraph::new(&bridge, &mut session);
                // This is the unchanged ordinary-def graph route. It does NOT
                // consume the new type snapshot: it independently establishes
                // the previous declaration's real key-construction policy.
                let root = graph.declaration(global, Some(info), 0).unwrap();
                let Some(Recipe::Map(entries)) = graph.recipes[root].as_ref() else { panic!("ordinary declaration"); };
                let entries = entries.clone();
                for field in RetainedKey::ALL {
                    let position = graph.declaration_field_position(&entries, field.name()).unwrap();
                    match history.retained_key(global, field) {
                        Some(TypeKeyOrigin::Literal(_)) => {
                            let slot = entries[position.expect(field.name())].0;
                            assert!(matches!(graph.recipes[slot].as_ref(), Some(Recipe::Identifier { literal: Some(_), .. })), "{source}/{}", field.name());
                        }
                        Some(TypeKeyOrigin::Source(_)) => {
                            let slot = entries[position.expect(field.name())].0;
                            assert!(!matches!(graph.recipes[slot].as_ref(), Some(Recipe::Identifier { literal: Some(_), .. })), "{source}/{}", field.name());
                        }
                        Some(TypeKeyOrigin::Unknown) => (),
                        Some(TypeKeyOrigin::ReaderShorthandTag(_)) => panic!("shorthand only constructs tag"),
                        None => assert!(position.is_none(), "missing previous key fact {source}/{}", field.name()),
                    }
                }
            }
        }
    }

}
