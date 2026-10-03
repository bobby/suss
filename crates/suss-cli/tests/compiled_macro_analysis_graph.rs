use suss_cli::{
    portable_macro_data::FormBridge,
    portable_macro_graph::AnalysisGraph,
    portable_session::{Session, SessionValue},
};
use suss_compile::portable::{self, Diagnostic, ExpansionContext, ExpansionHost};
use suss_reader::forms::{Form, Kind};

struct Inspect {
    macros: Session,
    bridge: FormBridge,
    query: SessionValue,
    calls: Vec<Form>,
    declaration_metadata: Option<Form>,
    imports: Option<suss_cli::portable_macros::CompiledMacros>,
    import_paths: Vec<std::path::PathBuf>,
}
impl Inspect {
    fn new(query: &str) -> Self {
        let mut macros = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut macros).unwrap();
        let query = macros.eval(query).unwrap();
        Self {
            macros,
            bridge,
            query,
            calls: vec![],
            declaration_metadata: None,
            imports: None,
            import_paths: vec![],
        }
    }
}
impl ExpansionHost for Inspect {
    fn supports_macro_imports(&self) -> bool { self.imports.is_some() }
    fn source_paths(&mut self, paths: &[std::path::PathBuf]) {
        if let Some(imports) = &mut self.imports {
            imports.source_paths(if self.import_paths.is_empty() { paths } else { &self.import_paths });
        }
    }
    fn load_macro_namespace(&mut self, namespace: &str, span: std::ops::Range<usize>) -> Result<Vec<String>, Diagnostic> {
        self.imports.as_mut().expect("configured compiled macro loader").load_macro_namespace(namespace, span)
    }
    fn expand(
        &mut self,
        form: &Form,
        context: ExpansionContext<'_>,
    ) -> Result<Option<Form>, Diagnostic> {
        let Kind::List(items) = &form.kind else {
            return Ok(None);
        };
        if !matches!(items.first().map(|item| &item.kind), Some(Kind::Symbol(name)) if name.namespace.is_none() && name.name == "inspect-graph")
        {
            return Ok(None);
        }
        let mut locals = context.locals.clone();
        if let Some(metadata) = &self.declaration_metadata {
            locals.get_mut("x").unwrap().declaration.metadata = vec![metadata.clone()];
        }
        let context = ExpansionContext {
            locals: &locals,
            ..context
        };
        let before = self.macros.stats();
        let env = AnalysisGraph::new(&self.bridge, &mut self.macros)
            .expansion(context)
            .map_err(|error| Diagnostic {
                span: form.span.clone(),
                message: error.to_string(),
            })?;
        self.macros.collect().unwrap();
        let result = self.macros.invoke(&self.query, &[&env]).unwrap();
        self.macros.collect().unwrap();
        self.calls.push(
            self.bridge
                .read(&mut self.macros, &result, form.span.clone())
                .unwrap(),
        );
        drop(result);
        drop(env);
        self.macros.collect().unwrap();
        assert_eq!(
            self.macros.stats().resident_fragments,
            before.resident_fragments
        );
        assert_eq!(
            self.macros.stats().resident_artifact_bytes,
            before.resident_artifact_bytes
        );
        assert_eq!(
            self.macros.stats().external_value_handles,
            before.external_value_handles
        );
        Ok(Some(Form {
            span: form.span.clone(),
            metadata: vec![],
            kind: Kind::Number(42.0),
        }))
    }
}

#[test]
fn native_analysis_graph_retains_source_methods_before_wrappers_and_duplicate_arity_elimination() {
    let query = r#"(fn [env]
      (let [defs (get (get env :ns) :defs)
            catalog (get (get env :suss/catalog) :defs)
            fixed (get (get defs 'fixed) :suss/source-function)
            multiple (get (get defs 'multiple) :suss/source-function)
            duplicate (get (get defs 'duplicate) :suss/source-function)
            nested (get (get defs 'nested) :suss/source-function)
            nested-method (nth (get nested :suss/methods) 0)
            inner-ast (nth (get (get nested-method :suss/body) :suss/children) 0)
            inner-method (nth (get (get inner-ast :suss/source-function) :suss/methods) 0)
            fixed-methods (get fixed :suss/methods)
            multiple-methods (get multiple :suss/methods)
            duplicate-methods (get duplicate :suss/methods)]
        [(get fixed :suss/variadic)
         (get fixed :suss/max-fixed-arity)
         (get (nth fixed-methods 0) :suss/parameters)
         (get (meta (nth (get (nth fixed-methods 0) :suss/parameters) 0)) :tag)
         (get multiple :suss/variadic)
         (get multiple :suss/max-fixed-arity)
         (get (nth multiple-methods 0) :suss/parameters)
         (get (nth multiple-methods 1) :suss/parameters)
         (get duplicate :suss/max-fixed-arity)
         (get (nth duplicate-methods 0) :suss/parameters)
         (get (nth duplicate-methods 1) :suss/parameters)
         (get (get defs 'alias) :suss/source-function)
         (identical? multiple (get (get catalog 'multiple) :suss/source-function))
         (get (get (nth fixed-methods 0) :suss/body) :suss/operation)
         (get nested-method :suss/parameters)
         (get inner-method :suss/parameters)]))"#;
    let source = r#"
      (def effects 0)
      (def fixed (fn [^number x] (set! effects (+ effects 1)) x))
      (def multiple (fn self ([x] x) ([x y & more] y)))
      (def duplicate (fn ([x] 1) ([y] 2)))
      (def alias fixed)
      (def nested (fn [outer] (fn [inner] (+ outer inner))))
      (inspect-graph)"#;
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut host = Inspect::new(query);
        session.eval_with_macros(source, &mut host).unwrap();
        assert_eq!(host.calls.len(), 1);
        let Kind::Vector(values) = &host.calls[0].kind else { panic!("executed source methods query") };
        assert_eq!(values.len(), 16);
        assert!(matches!(values[0].kind, Kind::Bool(false)));
        assert!(matches!(values[1].kind, Kind::Number(1.0)));
        assert!(matches!(&values[3].kind, Kind::Symbol(name) if name.name == "number"));
        assert!(matches!(values[4].kind, Kind::Bool(true)));
        assert!(matches!(values[5].kind, Kind::Number(2.0)));
        assert!(matches!(values[8].kind, Kind::Number(1.0)));
        for (index, expected) in [(2, vec!["x"]), (6, vec!["x"]), (7, vec!["x", "y", "more"]), (9, vec!["x"]), (10, vec!["y"]), (14, vec!["outer"]), (15, vec!["inner"])] {
            let Kind::Vector(parameters) = &values[index].kind else { panic!("actual source parameters") };
            assert_eq!(parameters.len(), expected.len());
            for (actual, expected) in parameters.iter().zip(expected) {
                assert!(matches!(&actual.kind, Kind::Symbol(name) if name.name == expected));
            }
        }
        assert!(matches!(values[11].kind, Kind::Nil));
        assert!(matches!(values[12].kind, Kind::Bool(true)));
        assert!(matches!(&values[13].kind, Kind::Keyword(name) if name.name == "do"));
        let result = session.eval("(+ (fixed 11) (multiple 12) (multiple 12 13 14) (duplicate 17) ((nested 20) 22) effects)").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        assert!(matches!(bridge.read(&mut session, &result, 0..1).unwrap().kind, Kind::Number(81.0)));
    }
}

#[test]
fn native_analysis_graph_separates_top_level_namespace_snapshot_from_live_catalog() {
    let query = r#"(fn [env]
      (let [snapshot (get (get env :ns) :defs)
            catalog (get (get env :suss/catalog) :defs)]
        [(contains? snapshot 'scalar)
         (get (get snapshot 'scalar) :doc)
         (contains? snapshot 'nested-first)
         (contains? snapshot 'nested-second)
         (contains? snapshot 'nested-wrapper)
         (contains? catalog 'scalar)
         (contains? catalog 'nested-first)
         (contains? catalog 'nested-second)
         (contains? catalog 'nested-wrapper)]))"#;
    let source = r#"
      (def scalar "old documentation" (inspect-graph))
      (inspect-graph)
      (def scalar (inspect-graph))
      (inspect-graph)
      (def nested-wrapper
        (do (def nested-first 1)
            (inspect-graph)
            (def nested-second (inspect-graph))
            (inspect-graph)))
      (inspect-graph)"#;
    // Fresh primary observations in declaration-environment-observations.json
    // establish this timing. Full portable metadata schema remains separate.
    let expected = [
        (false, None, [false, false, false], [true, false, false, false]),
        (true, Some("old documentation"), [false, false, false], [true, false, false, false]),
        (true, Some("old documentation"), [false, false, false], [true, false, false, false]),
        (true, None, [false, false, false], [true, false, false, false]),
        (true, None, [false, false, false], [true, true, false, true]),
        (true, None, [false, false, false], [true, true, true, true]),
        (true, None, [false, false, false], [true, true, true, true]),
        (true, None, [true, true, true], [true, true, true, true]),
    ];
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut host = Inspect::new(query);
        session.eval_with_macros(source, &mut host).unwrap();
        assert_eq!(host.calls.len(), expected.len());
        for (index, (actual, (scalar, doc, nested, catalog))) in host.calls.iter().zip(expected).enumerate() {
            let Kind::Vector(values) = &actual.kind else { panic!("executed snapshot query") };
            assert_eq!(values.len(), 9);
            assert!(matches!(values[0].kind, Kind::Bool(value) if value == scalar), "snapshot {index}");
            match doc {
                Some(doc) => assert!(matches!(&values[1].kind, Kind::String(units) if String::from_utf16(units).unwrap() == doc), "old declaration {index}"),
                None => assert!(matches!(values[1].kind, Kind::Nil), "absent document {index}"),
            }
            for (value, expected) in values[2..5].iter().zip(nested).chain(values[5..9].iter().zip(catalog)) {
                assert!(matches!(value.kind, Kind::Bool(value) if value == expected), "catalog/snapshot visibility {index}");
            }
        }
        let value = session.eval("(+ scalar nested-first nested-second nested-wrapper)").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        assert!(matches!(bridge.read(&mut session, &value, 0..1).unwrap().kind, Kind::Number(127.0)));
    }
}

#[test]
fn native_analysis_graph_preserves_deep_shared_initializer_scopes_without_source_execution() {
    let query = r#"(fn [env]
      (let [binding (get (get env :locals) 'x)
            old (get binding :shadow)
            init (get binding :init)
            old-in-env (get (get (get init :env) :locals) 'x)]
        [(identical? old old-in-env)
         (loop [binding binding n 0]
           (if (get binding :shadow)
             (recur (get binding :shadow) (+ n 1)) n))
         (get init :form)
         (get (get init :suss/lowering) :suss/native-operator)
         (identical? (get env :ns) (get (get init :env) :ns))]))"#;
    let mut bindings = "x (do (set! effects (+ effects 1)) 0)".to_owned();
    for _ in 0..96 {
        bindings.push_str(" x (+ x 1)");
    }
    let source = format!("(def effects 0) (def result (let [{bindings}] (inspect-graph)))");
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut host = Inspect::new(query);
        // The 96-step compiled inspection performs retained HAMT lookups.
        // Keep a bounded explicit budget for this traversal stress fixture.
        host.macros.set_operation_fuel(100_000_000);
        session.eval_with_macros(&source, &mut host).unwrap();
        assert_eq!(host.calls.len(), 1);
        let Kind::Vector(values) = &host.calls[0].kind else {
            panic!("executed graph query")
        };
        assert!(matches!(values[0].kind, Kind::Bool(true)));
        assert!(matches!(values[1].kind, Kind::Number(96.0)));
        assert!(
            matches!(&values[2].kind, Kind::List(parts) if parts.len() == 3 && matches!(&parts[0].kind, Kind::Symbol(name) if name.name == "+"))
        );
        assert!(
            matches!(&values[3].kind, Kind::String(text) if String::from_utf16(text).unwrap() == "Add")
        );
        assert!(matches!(values[4].kind, Kind::Bool(true)));
        let effects = session.eval("effects").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        assert!(matches!(
            bridge.read(&mut session, &effects, 0..1).unwrap().kind,
            Kind::Number(1.0)
        ));
    }
}

#[test]
fn native_analysis_graph_keeps_function_declaration_environment_outside_new_self_binding() {
    let query = r#"(fn [env]
      (let [scope (nth (get env :fn-scope) 0)
            declaration-env (get scope :env)]
        [(get scope :name)
         (get (get scope :info) :suss/explicit-self)
         (get (get scope :info) :suss/phase)
         (get (get declaration-env :locals) 'self)
         (get (get env :locals) 'self)]))"#;
    let source = "(def result (let [self 7] ((fn self [] (inspect-graph)))))";
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut host = Inspect::new(query);
        session.eval_with_macros(source, &mut host).unwrap();
        let Kind::Vector(values) = &host.calls[0].kind else {
            panic!("scope query")
        };
        assert!(matches!(&values[0].kind, Kind::Symbol(name) if name.name == "self"));
        assert!(matches!(values[1].kind, Kind::Bool(true)));
        let phase = if session.phase() == portable::resolve::Phase::Runtime {
            "runtime"
        } else {
            "macro"
        };
        assert!(matches!(&values[2].kind, Kind::Keyword(name) if name.name == phase));
        for (value, expected) in [(&values[3], "let"), (&values[4], "fn")] {
            let Kind::Map(entries) = &value.kind else {
                panic!("actual binding record")
            };
            let pair = entries
                .chunks_exact(2)
                .find(|pair| matches!(&pair[0].kind, Kind::Keyword(name) if name.name == "local"))
                .unwrap();
            assert!(matches!(&pair[1].kind, Kind::Keyword(name) if name.name == expected));
        }
    }
}

#[test]
fn oversized_analysis_metadata_fails_before_runtime_effects_or_macro_store_allocation() {
    let mut session = Session::new_repl().unwrap();
    session.eval("(def effects 0)").unwrap();
    let mut host = Inspect::new("(fn [env] (count (get env :locals)))");
    let before = session.stats();
    let macro_before = host.macros.stats();
    let doc = "a".repeat(1_048_577);
    let source = format!(
        "(def bad (let [^{{:doc \"{doc}\"}} x (do (set! effects 99) 42)] (inspect-graph)))"
    );
    let error = session.eval_with_macros(&source, &mut host).unwrap_err();
    assert!(matches!(
        error,
        suss_cli::portable_session::SessionError::Compile(_)
    ));
    assert!(error.to_string().contains("UTF-16 storage bound"));
    assert!(host.calls.is_empty());
    assert_eq!(
        session.stats().resident_fragments,
        before.resident_fragments
    );
    assert_eq!(session.stats().binding_cells, before.binding_cells);
    assert_eq!(
        host.macros.stats().resident_fragments,
        macro_before.resident_fragments
    );
    assert_eq!(
        host.macros.stats().external_value_handles,
        macro_before.external_value_handles
    );
    assert!(session.eval("bad").is_err());
    let value = session.eval("(== effects 0)").unwrap();
    assert_eq!(
        session
            .inspect(&value, |store, value| Ok(value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32()))
            .unwrap(),
        4
    );
}

#[test]
fn native_analysis_graph_preserves_unqualified_slash_keys_in_hash_maps() {
    let mut session = Session::new_repl().unwrap();
    let mut host = Inspect::new("(fn [env] (get (get (get env :locals) '/) :local))");
    session
        .eval_with_macros(
            "(let [/ 0 a 1 b 2 c 3 d 4 e 5 f 6 g 7 h 8] (inspect-graph))",
            &mut host,
        )
        .unwrap();
    assert!(matches!(&host.calls[0].kind, Kind::Keyword(name) if name.name == "let"));
}

#[test]
fn native_analysis_graph_reader_depth_is_independent_of_record_depth() {
    // Host-owned syntax at the bridge's legal limit; this is not a claim that
    // the surrounding source reader accepts equally deep enclosing syntax.
    let mut nested = Form {
        span: 0..1,
        metadata: vec![],
        kind: Kind::Number(42.0),
    };
    for _ in 0..61 {
        nested = Form {
            span: 0..1,
            metadata: vec![],
            kind: Kind::Vector(vec![nested]),
        };
    }
    let key = suss_reader::forms::read_forms(":doc").unwrap().remove(0);
    let mut host = Inspect::new(
        "(fn [env] (loop [v (get (meta (get (get (get env :locals) 'x) :name)) :doc) n 61] (if (== n 0) v (recur (nth v 0) (- n 1)))))",
    );
    host.declaration_metadata = Some(Form {
        span: 0..1,
        metadata: vec![],
        kind: Kind::Map(vec![key, nested]),
    });
    let mut session = Session::new_repl().unwrap();
    session
        .eval_with_macros("(let [x 1] (inspect-graph))", &mut host)
        .unwrap();
    assert!(matches!(host.calls[0].kind, Kind::Number(42.0)));
}

#[test]
fn native_analysis_graph_retains_staged_definition_and_function_syntax() {
    let query = r#"(fn [env]
      (let [definition (get (get (get env :suss/catalog) :defs) 'staged)
            scope (nth (get env :fn-scope) 0)]
        [(get definition :suss/initializer-recorded)
         (get definition :suss/initializer-form)
         (get (get scope :info) :suss/function-form)
         (contains? (get (get (get scope :env) :ns) :defs) 'staged)
         (contains? (get (get (get scope :env) :suss/catalog) :defs) 'staged)
         (identical? (get env :ns) (get (get scope :env) :ns))]))"#;
    let function = "(fn staged ([x] (inspect-graph)) ([x & xs] (inspect-graph)))";
    let expected = suss_reader::forms::read_forms(function).unwrap().remove(0);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut host = Inspect::new(query);
        session.eval_with_macros(&format!("(def staged {function})"), &mut host).unwrap();
        assert_eq!(host.calls.len(), 2);
        for call in &host.calls {
            let Kind::Vector(values) = &call.kind else { panic!("staged query") };
            assert!(matches!(values[0].kind, Kind::Bool(false)));
            // Read-back spans deliberately refer to the macro call. Compare
            // actual syntax after discarding only those host-owned spans.
            fn erase_span(form: &mut Form) {
                form.span = 0..0;
                for metadata in &mut form.metadata { erase_span(metadata); }
                if let Kind::List(items) | Kind::Vector(items) = &mut form.kind {
                    for item in items { erase_span(item); }
                }
            }
            let mut expected = expected.clone(); erase_span(&mut expected);
            for actual in &values[1..3] {
                let mut actual = actual.clone(); erase_span(&mut actual);
                assert_eq!(actual, expected);
            }
            assert!(matches!(values[3].kind, Kind::Bool(false)));
            assert!(matches!(values[4].kind, Kind::Bool(true)));
            assert!(matches!(values[5].kind, Kind::Bool(true)));
        }
        let result = session.eval("(== (+ (staged 1) (staged 1 2 3)) 84)").unwrap();
        assert_eq!(session.inspect(&result, |store, value| Ok(value.unwrap_anyref().unwrap().as_i31(&store)?.unwrap().get_u32())).unwrap(), 4);
    }
}

#[test]
fn native_analysis_graph_namespace_exclusions_are_canonical_sets() {
    let mut host = Inspect::new("(fn [env] (let [excludes (get (get env :ns) :excludes)] [(set? excludes) (count excludes) (contains? excludes 'identity)]))");
    let mut session = Session::new_repl().unwrap();
    session.eval_with_macros("(ns exclusions (:refer-clojure :exclude [identity])) (inspect-graph)", &mut host).unwrap();
    let Kind::Vector(values) = &host.calls[0].kind else { panic!("namespace exclusions") };
    assert!(matches!(values[0].kind, Kind::Bool(true)));
    assert!(matches!(values[1].kind, Kind::Number(1.0)));
    assert!(matches!(values[2].kind, Kind::Bool(true)));
}


#[test]
fn native_analysis_graph_namespace_maps_preserve_actual_imports_and_renames_in_both_phases() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("graph")).unwrap();
    std::fs::write(root.path().join("graph/empty.sus"), "(ns graph.empty)").unwrap();
    std::fs::write(root.path().join("graph/lib.sus"), "(ns graph.lib) (def one 1) (def two 2)").unwrap();
    std::fs::write(root.path().join("graph/macempty.sus"), "(ns graph.macempty)").unwrap();
    std::fs::write(root.path().join("graph/tools.sus"), "(ns graph.tools) (defmacro one [] 41) (defmacro two [] 42)").unwrap();
    std::fs::write(root.path().join("graph/blank.sus"), "(ns graph.blank (:require-macros [graph.tools :as m])) (def observed 42)").unwrap();
    let query = r#"(fn [env] (let [ns (get env :ns)]
      [(get ns :name) (get ns :requires) (get ns :uses) (get ns :renames)
       (get ns :require-macros) (get ns :use-macros) (get ns :rename-macros)
       [(contains? ns :requires) (contains? ns :uses) (contains? ns :renames)
        (contains? ns :require-macros) (contains? ns :use-macros) (contains? ns :rename-macros)]]))"#;
    let source = "(ns graph.app (:require graph.empty [graph.lib :as g :refer [one two] :rename {one renamed}] [graph.lib :refer [one] :rename {one one}] [graph.lib :refer [one]]) (:require-macros graph.macempty [graph.tools :as m :refer [one two] :rename {one renamed-macro}] [graph.tools :refer [one] :rename {one one}] [graph.tools :refer [one]])) (inspect-graph)";
    let sources = [source,
        "(ns graph.blank (:require-macros [graph.tools :as m])) (inspect-graph)",
        "(ns graph.nomac (:require graph.blank)) (inspect-graph)"];
    let expected: serde_json::Value = serde_json::from_str(include_str!("../../../tests/oracle/namespace-environment-observations.json")).unwrap();
    for phase in [portable::resolve::Phase::Runtime, portable::resolve::Phase::Macro] {
        let mut session = Session::with_options_in(suss_cli::portable_session::SessionOptions {
            source_paths: vec![root.path().to_owned()], ..Default::default()
        }, phase).unwrap();
        let mut host = Inspect::new(query);
        host.imports = Some(suss_cli::portable_macros::CompiledMacros::new().unwrap());
        host.import_paths = vec![root.path().to_owned()];
        for (index, source) in sources.into_iter().enumerate() {
            session.eval_with_macros(source, &mut host).unwrap();
            assert_eq!(host.calls.len(), index + 1);
            let Kind::Vector(maps) = &host.calls[index].kind else { panic!("namespace maps") };
            assert_eq!(maps.len(), 8);
            let Kind::Vector(presence) = &maps[7].kind else { panic!("map presence") };
            assert_eq!(presence.len(), 6);
            fn symbol(form: &Form) -> String {
                let Kind::Symbol(value) = &form.kind else { panic!("namespace symbol: {form:?}") };
                value.namespace.as_ref().map_or_else(|| value.name.clone(), |ns| format!("{ns}/{}", value.name))
            }
            let mut actual_maps = Vec::new();
            for ((name, map), present) in ["requires", "uses", "renames", "require-macros", "use-macros", "rename-macros"].into_iter().zip(&maps[1..7]).zip(presence) {
                let Kind::Bool(present) = present.kind else { panic!("map presence") };
                let entries = match &map.kind {
                    Kind::Nil => serde_json::Value::Null,
                    Kind::Map(entries) => {
                        assert_eq!(entries.len() % 2, 0);
                        let mut pairs = entries.chunks_exact(2).map(|pair| [symbol(&pair[0]), symbol(&pair[1])]).collect::<Vec<_>>();
                        pairs.sort();
                        serde_json::json!(pairs)
                    }
                    _ => panic!("{name}: {map:?}"),
                };
                actual_maps.push(serde_json::json!([name, present, entries]));
            }
            let actual = serde_json::json!({"schema":1, "upstream":"c4295f303100bbf5afac449242d30bca1126f1a1", "namespace":symbol(&maps[0]), "maps":actual_maps});
            assert_eq!(actual, expected["cases"][index], "{phase:?}: {source}");
        }
    }
}
