//! Actual bridge construction; no substituted validators or source authority.
use super::*;
fn memo(session: &mut Session, bridge: &FormBridge, value: &SessionValue) -> Kind {
    let fields = session.data_fields(value).unwrap();
    bridge.read(session, &fields[3], 0..0).unwrap().kind.clone()
}
#[test]
fn analyzer_literals_keep_cold_same_text_reader_symbols_and_live_hash_order_after_gc() {
    use crate::portable_session::memo_leaf_test_support::Method;
    for mut session in [Session::new_macro().unwrap(), Session::new_repl().unwrap()] {
        session.eval("(def leaf-original-method nil) (def literal-log [])").unwrap();
        let original = Method::capture(&mut session, "Keyword").unwrap();
        original.bind_delegate(&mut session).unwrap();
        session.eval(r#"(extend-type Keyword IHash
          (-hash [self]
            (do (set! literal-log (conj literal-log (nil? (.-_hash self))))
                (leaf-original-method self))))"#).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let (literal, cold, source, symbol, dynamic, extension, map) = {
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            let literal = graph.keyword(FieldKey::Literal(AnalyzerLiteralKeyword("op"))).unwrap();
            let cold = graph.keyword("op").unwrap();
            let source_form = suss_reader::forms::read_forms(":op").unwrap().remove(0);
            let source = graph.form(&source_form, 0).unwrap();
            let sym_form = suss_reader::forms::read_forms("op").unwrap().remove(0);
            let symbol = graph.form(&sym_form, 0).unwrap();
            let dynamic = graph.symbol("op").unwrap();
            let extension = graph.keyword("suss/catalog").unwrap();
            assert_ne!(literal, cold);
            assert_eq!(literal, graph.keyword(FieldKey::Literal(AnalyzerLiteralKeyword("op"))).unwrap());
            assert_eq!(cold, graph.keyword("op").unwrap());
            let value = graph.number(17).unwrap();
            let literal_value = graph.materialize(literal).unwrap();
            let cold_value = graph.materialize(cold).unwrap();
            let source_value = graph.materialize(source).unwrap();
            let symbol_value = graph.materialize(symbol).unwrap();
            let dynamic_value = graph.materialize(dynamic).unwrap();
            let extension_value = graph.materialize(extension).unwrap();
            // A genuine map construction must still call the live IHash body.
            let mut entries = vec![(literal, value)];
            for text in ["ns", "context", "locals", "fn-scope", "name", "tag", "env"] {
                // These test tokens represent the same eight proved literals;
                // production never derives eligibility from a text lookup.
                let key = graph.keyword(FieldKey::Literal(AnalyzerLiteralKeyword(text))).unwrap();
                entries.push((key, value));
            }
            let cold_key = graph.keyword("unproven").unwrap();
            entries.push((cold_key, value));
            let map = graph.map_values(entries).unwrap();
            let map_value = graph.materialize(map).unwrap();
            (literal_value,cold_value,source_value,symbol_value,dynamic_value,extension_value,map_value)
        };
        session.collect().unwrap();
        assert_eq!(memo(&mut session, &bridge, &literal), Kind::Number(-1882987955.0));
        for value in [&cold,&source,&symbol,&dynamic,&extension] {
            assert_eq!(memo(&mut session, &bridge, value), Kind::Nil);
        }
        let log = session.eval("literal-log").unwrap();
        let observed = crate::portable_repl::display(&mut session, &log).unwrap();
        eprintln!("analyzer-literal live IHash log: {observed}");
        assert_eq!(observed, "[false false false false false false false false true]");
        let decoded = bridge.read(&mut session, &map, 0..0).unwrap();
        assert!(matches!(decoded.kind, Kind::Map(ref entries) if entries.len()==18 && entries[1].kind==Kind::Number(17.0)));
        original.restore(&mut session).unwrap();
        let hash = session.eval("hash").unwrap();
        let result = session.invoke(&hash, &[&literal]).unwrap();
        assert_eq!(bridge.read(&mut session, &result, 0..0).unwrap().kind, Kind::Number(-1882987955.0));
    }
}
#[test]
fn analyzer_literal_recorded_numeric_memo_cannot_gain_nil_transition_authority() {
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap().recording(&mut session).unwrap();
    let value = {
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        let key = graph.keyword(FieldKey::Literal(AnalyzerLiteralKeyword("op"))).unwrap();
        graph.materialize(key).unwrap()
    };
    let records = bridge.into_records().unwrap();
    assert!(records.matches_direct_facts(&mut session).unwrap());
    let clear = session.eval("(fn [x] (set! (.-_hash x) nil))").unwrap();
    session.invoke(&clear, &[&value]).unwrap();
    assert!(!records.matches_direct_facts(&mut session).unwrap());
    let hash = session.eval("hash").unwrap();
    session.invoke(&hash, &[&value]).unwrap();
    session.collect().unwrap();
    assert!(!records.matches_direct_facts(&mut session).unwrap(), "clearing a literal must not authorize a protected rewarming receipt");
}

#[test]
fn analyzer_literal_binding_route_guard_does_not_promote_adapted_or_self_roles() {
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut binding = LocalBinding {
        identity: Arc::new(()), id: hir::BindingId(0), ty: hir::Type::Value,
        declaration: suss_reader::forms::read_forms("p").unwrap().remove(0),
        origin: None, kind: hir::LocalKind::Let,
        declaration_context: portable::AnalysisContext::Expression,
        source_role: SourceRole::Plain, initializer: None, shadow: None, shadow_field: None,
    };
    let mut graph = AnalysisGraph::new(&bridge, &mut session);
    let operation = graph.session.operation_budget_scope().unwrap();
    for kind in [hir::LocalKind::Let,hir::LocalKind::Loop,hir::LocalKind::Argument{index:0,rest:false}] {
        binding.kind=kind;
        assert!(matches!(graph.binding_literal_key(&binding,AnalyzerLiteralKeyword("form")).unwrap(),FieldKey::Literal(_)));
    }
    binding.kind=hir::LocalKind::Let;
    binding.source_role=SourceRole::PrivateCatch{anchor:0..1};
    assert!(matches!(graph.binding_literal_key(&binding,AnalyzerLiteralKeyword("form")).unwrap(),FieldKey::Cold("form")));
    binding.source_role=SourceRole::FunctionName{variadic:false,methods:None};
    binding.kind=hir::LocalKind::FunctionName;
    assert!(matches!(graph.binding_literal_key(&binding,AnalyzerLiteralKeyword("form")).unwrap(),FieldKey::Cold("form")));
    let access=Hir{source:None,span:0..1,metadata:vec![],ty:hir::Type::Value,kind:Expression::Literal(Literal::Nil)};
    binding.source_role=SourceRole::MethodThis{
        type_declaration:binding.declaration.clone(),namespace:"user".into(),protocol_receiver:false,
        receiver_declaration:binding.declaration.clone(),argument:None,access,
    };
    assert_eq!(binding.source_kind(),hir::LocalKind::Let);
    assert!(matches!(graph.binding_literal_key(&binding,AnalyzerLiteralKeyword("form")).unwrap(),FieldKey::Cold("form")));
    drop(operation);
}

#[test]
fn analyzer_literal_parameter_record_keeps_genuine_reader_form_and_metadata_value_cold() {
    let mut session=Session::new_macro().unwrap();
    let bridge=FormBridge::new(&mut session).unwrap();
    let binding=LocalBinding{
        identity:Arc::new(()),id:hir::BindingId(1),ty:hir::Type::Value,
        declaration:suss_reader::forms::read_forms("^{:audit :source-leaf} p").unwrap().remove(0),
        origin:None,kind:hir::LocalKind::Argument{index:0,rest:false},
        declaration_context:portable::AnalysisContext::Expression,
        source_role:SourceRole::Plain,initializer:None,shadow:None,shadow_field:None,
    };
    let value={
        let mut graph=AnalysisGraph::new(&bridge,&mut session);
        let id=graph.local(&binding,0).unwrap();
        graph.finish_recipes().unwrap();
        let entries=match graph.recipes[id].as_ref().unwrap(){Recipe::Map(entries)=>entries,_=>panic!("actual parameter recipe must be a map")};
        for name in ["form","local","binding-form?","arg-id","shadow","info"] {
            assert!(entries.iter().any(|(key,_)|matches!(graph.recipes[*key].as_ref(),Some(Recipe::Identifier{name:n,literal:Some(_),..}) if n==name)),"{name}");
        }
        graph.materialize(id).unwrap()
    };
    session.collect().unwrap();
    let form=session.eval("(fn [info] (:form info))").unwrap();
    let form=session.invoke(&form,&[&value]).unwrap();
    assert_eq!(memo(&mut session,&bridge,&form),Kind::Nil);
    let leaf=session.eval("(fn [info] (:audit (meta (:form info))))").unwrap();
    let leaf=session.invoke(&leaf,&[&value]).unwrap();
    assert_eq!(memo(&mut session,&bridge,&leaf),Kind::Nil);
    assert!(matches!(bridge.read(&mut session,&leaf,0..1).unwrap().kind,Kind::Keyword(ref k) if k.name=="source-leaf"));
}


#[test]
fn analyzer_literal_namespace_routes_keep_source_controls_cold_and_live_dispatch() {
    use crate::portable_session::memo_leaf_test_support::Method;
    for mut session in [Session::new_macro().unwrap(), Session::new_repl().unwrap()] {
        session.eval("(def leaf-original-method nil) (def namespace-literal-log [])").unwrap();
        let original = Method::capture(&mut session, "Keyword").unwrap();
        original.bind_delegate(&mut session).unwrap();
        session.eval(r#"(extend-type Keyword IHash
          (-hash [self]
            (do (if (or (= (.-name self) "name") (= (.-name self) "requires")
                        (= (.-name self) "renames") (= (.-name self) "uses") (= (.-name self) "require-macros")
                        (= (.-name self) "use-macros") (= (.-name self) "rename-macros")
                        (= (.-name self) "excludes") (= (.-name self) "defs"))
                    (set! namespace-literal-log
                      (conj namespace-literal-log [(.-name self) (nil? (.-_hash self))])))
                (leaf-original-method self))))"#).unwrap();
        let bridge=FormBridge::new(&mut session).unwrap();
        let namespace=Arc::new(SourceNamespace {
            namespace:"user".into(),aliases:Default::default(),refers:Default::default(),
            used_refers:Default::default(),renamed_refers:Default::default(),excluded_core:Default::default(),
            macro_aliases:Default::default(),macro_refers:Default::default(),used_macro_refers:Default::default(),
            renamed_macro_refers:Default::default(),declarations:Default::default(),identities:vec![],
        });
        let (value, controls)={
            let mut graph=AnalysisGraph::new(&bridge,&mut session);
            let root=graph.namespace(&namespace,0).unwrap();
            graph.finish_recipes().unwrap();
            let entries=match graph.recipes[root].as_ref().unwrap() {Recipe::Map(entries)=>entries.clone(),_=>panic!("namespace task must finish as actual map")};
            for name in ["name","requires","uses","renames","require-macros","use-macros","rename-macros","excludes","defs"] {
                assert!(entries.iter().any(|(key,_)|matches!(graph.recipes[*key].as_ref(),Some(Recipe::Identifier{name:n,literal:Some(_),..}) if n==name)),"{name}");
            }
            let refers=entries.iter().find_map(|(key,_)|match graph.recipes[*key].as_ref().unwrap() {
                Recipe::Identifier{namespace:Some(ns),name,literal:None,..} if ns=="suss" && name=="refers"=>Some(*key),_=>None
            }).expect("Suss-only field remains cold");
            let mut controls=vec![graph.materialize(refers).unwrap()];
            for text in [":name",":requires",":renames",":require-macros",":rename-macros",":use-macros",":excludes",":uses",":defs"] {
                let form=suss_reader::forms::read_forms(text).unwrap().remove(0);
                let id=graph.form(&form,0).unwrap();
                controls.push(graph.materialize(id).unwrap());
            }
            for control in &controls {assert_eq!(memo(graph.session,&bridge,control),Kind::Nil);}
            let value=graph.materialize(root).unwrap();
            (value,controls)
        };
        session.collect().unwrap();
        // Distinct reader leaves stay cold even after the same-text map keys hash.
        for control in &controls {assert_eq!(memo(&mut session,&bridge,control),Kind::Nil);}
        let log=session.eval("namespace-literal-log").unwrap();
        let log=bridge.read(&mut session,&log,0..1).unwrap();
        eprintln!("namespace-literal-dispatch {:?}",log.kind);
        let Kind::Vector(rows)=&log.kind else {panic!("dispatch vector")};
        let expected_names=["name","requires","uses","renames","require-macros","use-macros","rename-macros","excludes","defs"];
        assert_eq!(rows.len(),expected_names.len(),"every selected key still invokes the actual replacement exactly once");
        for (row, expected) in rows.iter().zip(expected_names) {
            let Kind::Vector(fields)=&row.kind else {panic!("event")};
            assert_eq!(fields.len(),2);
            assert_eq!(fields[0].kind,Kind::String(expected.encode_utf16().collect()),"actual callback order and key identity by text");
            assert_eq!(fields[1].kind,Kind::Bool(false));
        }
        let decoded=bridge.read(&mut session,&value,0..1).unwrap();
        assert!(matches!(decoded.kind,Kind::Map(_)));
        original.restore(&mut session).unwrap();
    }
}
