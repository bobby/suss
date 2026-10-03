use suss_cli::{
    portable_macro_data::FormBridge,
    portable_macros::CompiledMacros,
    portable_session::Session,
};
use suss_reader::forms::{Form, Kind};

fn read(session: &mut Session, value: &suss_cli::portable_session::SessionValue) -> Form {
    let bridge = FormBridge::new(session).unwrap();
    session.collect().unwrap();
    bridge.read(session, value, 0..1).unwrap()
}

// Match the primary helper's deliberately bounded projection. Symbol metadata
// and spans are excluded by that projection; they are not certified by this corpus.
fn assert_shared(label: &str, actual: &Form) {
    fn data(value: &serde_json::Value) -> Form {
        let kind = match value {
            serde_json::Value::Null => Kind::Nil,
            serde_json::Value::Bool(value) => Kind::Bool(*value),
            serde_json::Value::Number(value) => Kind::Number(value.as_f64().unwrap()),
            serde_json::Value::String(value) => Kind::String(value.encode_utf16().collect()),
            serde_json::Value::Array(values) if values.len() == 2 => {
                match values[0].as_str().unwrap() {
                    "vector" => Kind::Vector(values[1].as_array().unwrap().iter().map(data).collect()),
                    "symbol" | "keyword" => {
                        let keyword = values[0] == "keyword";
                        let spelling = values[1].as_str().unwrap();
                        let spelling = if keyword { spelling.strip_prefix(':').unwrap() } else { spelling };
                        let (namespace, name) = spelling.split_once('/').map_or((None, spelling), |(namespace, name)| (Some(namespace.to_owned()), name));
                        if keyword { Kind::Keyword(suss_reader::Keyword { namespace, name: name.to_owned() }) }
                        else { Kind::Symbol(suss_reader::Symbol { namespace, name: name.to_owned() }) }
                    }
                    _ => panic!("unrecognized primary source environment projection"),
                }
            }
            _ => panic!("malformed primary source environment projection"),
        };
        Form { span: 0..0, metadata: vec![], kind }
    }
    fn project(form: &mut Form) {
        form.span = 0..0;
        form.metadata.clear();
        if let Kind::Vector(items) = &mut form.kind { for item in items { project(item); } }
    }
    let corpus: serde_json::Value = serde_json::from_str(include_str!("../../../tests/oracle/source-environment-observations.json")).unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(corpus["upstream"], "c4295f303100bbf5afac449242d30bca1126f1a1");
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.iter().map(|case| case[0].as_str().unwrap()).collect::<Vec<_>>(), ["lexical", "function", "variadic"]);
    let expected = data(&cases.iter().find(|case| case[0] == label).unwrap()[1]);
    let mut actual = actual.clone();
    project(&mut actual);
    assert_eq!(actual, expected, "executed native projection matches actual pinned macro &env");
}

#[test]
fn compiled_source_macro_environment_retains_lexical_initializer_and_shadow_without_reexecution() {
    let definition = r#"(defmacro lexical-facts [name]
      (let [binding (get (get &env :locals) name)]
        (list 'quote [(get binding :op)
                      (get binding :local)
                      (get binding :form)
                      (get (get binding :init) :form)
                      (get (get binding :shadow) :local)
                      (get &env :context)
                      (nth &form 1)])))"#;
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        session.enter_namespace("suss-oracle.source-environment-runner").unwrap();
        macros.enter_namespace("suss-oracle.source-environment-runner").unwrap();
        macros.define(definition).unwrap();
        let value = session.eval_with_macros(r#"
          (def effects 0)
          (def result
            (let [x (do (set! effects (+ effects 1)) 7)]
              (let [x 9] (lexical-facts x))))
          result"#, &mut macros).unwrap();
        let result = read(&mut session, &value);
        assert_shared("lexical", &result);
        let Kind::Vector(items) = result.kind else { panic!("executed source environment facts") };
        assert_eq!(items.len(), 7);
        for (index, name) in [(0, "binding"), (1, "let"), (4, "let"), (5, "return")] {
            assert!(matches!(&items[index].kind, Kind::Keyword(keyword) if keyword.name == name));
        }
        for index in [2, 6] {
            assert!(matches!(&items[index].kind, Kind::Symbol(symbol) if symbol.name == "x"));
        }
        assert!(matches!(items[3].kind, Kind::Number(9.0)));
        let effects = session.eval("effects").unwrap();
        assert!(matches!(read(&mut session, &effects).kind, Kind::Number(1.0)));
    }
}

#[test]
fn compiled_source_macro_environment_tracks_function_scopes_and_implicit_arguments_in_all_signatures() {
    let definition = r#"(defmacro scope-facts
      ([] (list 'quote [(get &env :context)
                       (count (get &env :fn-scope))
                       (get (get (get &env :locals) 'x) :local)]))
      ([name & more]
       (list 'quote [(get (get &env :ns) :name)
                    (get (get (get &env :locals) name) :local)
                    (count more)
                    (count &form)])))"#;
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        session.enter_namespace("suss-oracle.source-environment-runner").unwrap();
        macros.enter_namespace("suss-oracle.source-environment-runner").unwrap();
        macros.define(definition).unwrap();
        let value = session.eval_with_macros("(def function (fn [x] (scope-facts))) (function 17)", &mut macros).unwrap();
        let result = read(&mut session, &value);
        assert_shared("function", &result);
        let Kind::Vector(items) = result.kind else { panic!("function facts") };
        assert_eq!(items.len(), 3);
        assert!(matches!(&items[0].kind, Kind::Keyword(keyword) if keyword.name == "return"));
        assert!(matches!(items[1].kind, Kind::Number(1.0)));
        assert!(matches!(&items[2].kind, Kind::Keyword(keyword) if keyword.name == "arg"));
        let value = session.eval_with_macros("(let [x 7] (scope-facts x a b))", &mut macros).unwrap();
        let result = read(&mut session, &value);
        assert_shared("variadic", &result);
        let Kind::Vector(items) = result.kind else { panic!("variadic facts") };
        assert_eq!(items.len(), 4);
        assert!(matches!(&items[0].kind, Kind::Symbol(symbol) if symbol.name == "suss-oracle.source-environment-runner"));
        assert!(matches!(&items[1].kind, Kind::Keyword(keyword) if keyword.name == "let"));
        assert!(matches!(items[2].kind, Kind::Number(2.0)));
        assert!(matches!(items[3].kind, Kind::Number(4.0)));
    }
}

#[test]
fn compiled_source_macro_environment_keeps_snapshot_documents_separate_from_staged_catalog() {
    let definition = r#"(defmacro snapshot-facts []
      (let [snapshot (get (get &env :ns) :defs)
            catalog (get (get &env :suss/catalog) :defs)]
        (list 'quote [(contains? snapshot 'tracked)
                      (get (get snapshot 'tracked) :doc)
                      (get (get catalog 'tracked) :doc)
                      (get (get catalog 'tracked) :suss/analysis-completed)])))"#;
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        session.enter_namespace("suss-oracle.source-environment-runner").unwrap();
        macros.enter_namespace("suss-oracle.source-environment-runner").unwrap();
        macros.define(definition).unwrap();
        let first = session.eval_with_macros("(def tracked \"first\" (snapshot-facts))", &mut macros).unwrap();
        let second = session.eval_with_macros("(def tracked \"second\" (snapshot-facts))", &mut macros).unwrap();
        let after = session.eval_with_macros("(snapshot-facts)", &mut macros).unwrap();
        for (index, value) in [&first, &second, &after].into_iter().enumerate() {
            let Kind::Vector(items) = read(&mut session, value).kind else { panic!("snapshot facts") };
            assert_eq!(items.len(), 4);
            assert!(matches!(items[0].kind, Kind::Bool(present) if present == (index != 0)));
            if index == 0 {
                assert!(matches!(items[1].kind, Kind::Nil));
            } else {
                let expected: Vec<u16> = if index == 1 { "first" } else { "second" }.encode_utf16().collect();
                assert!(matches!(&items[1].kind, Kind::String(units) if units == &expected));
            }
            // Pinned parse-def installs raw symbol metadata provisionally;
            // the explicit docstring is associated only after init analysis.
            // Independently executed declaration-metadata-review-probe.clj
            // observes nil during both initializers, then the completed doc.
            if index < 2 {
                assert!(matches!(items[2].kind, Kind::Nil));
            } else {
                let expected: Vec<u16> = "second".encode_utf16().collect();
                assert!(matches!(&items[2].kind, Kind::String(units) if units == &expected));
            }
            assert!(matches!(items[3].kind, Kind::Bool(completed) if completed == (index == 2)));
        }
    }
}
