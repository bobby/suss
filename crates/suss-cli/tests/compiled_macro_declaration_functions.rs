//! Execute the selected function declaration projection from the pinned oracle.
use suss_cli::{
    portable_macro_data::FormBridge, portable_macros::CompiledMacros, portable_session::Session,
};
use suss_reader::forms::{Form, Kind};

const FIELDS: [&str; 6] = [
    "fn-var",
    "variadic?",
    "max-fixed-arity",
    "method-params",
    "arglists",
    "arglists-meta",
];

const DEFINITION: &str = r#"(defmacro function-facts [name]
      (let [info (get (get (get &env :ns) :defs) name)]
        (list 'quote [[(contains? info :fn-var) (get info :fn-var)]
                      [(contains? info :variadic?) (get info :variadic?)]
                      [(contains? info :max-fixed-arity) (get info :max-fixed-arity)]
                      [(contains? info :method-params) (get info :method-params)]
                      [(contains? info :arglists) (get info :arglists)]
                      [(contains? info :arglists-meta) (get info :arglists-meta)]])))"#;

fn data(value: &serde_json::Value) -> Form {
    let kind = match value {
        serde_json::Value::Null => Kind::Nil,
        serde_json::Value::Bool(value) => Kind::Bool(*value),
        serde_json::Value::Number(value) => Kind::Number(value.as_f64().unwrap()),
        serde_json::Value::String(value) => Kind::String(value.encode_utf16().collect()),
        serde_json::Value::Array(values) if values.len() == 2 => {
            match values[0].as_str().unwrap() {
                "seq" => Kind::List(values[1].as_array().unwrap().iter().map(data).collect()),
                "vector" => Kind::Vector(values[1].as_array().unwrap().iter().map(data).collect()),
                "symbol" => Kind::Symbol(suss_reader::Symbol {
                    namespace: None,
                    name: values[1].as_str().unwrap().to_owned(),
                }),
                "keyword" => Kind::Keyword(suss_reader::Keyword {
                    namespace: None,
                    name: values[1]
                        .as_str()
                        .unwrap()
                        .strip_prefix(':')
                        .unwrap()
                        .to_owned(),
                }),
                "map" => Kind::Map(
                    values[1]
                        .as_array()
                        .unwrap()
                        .iter()
                        .flat_map(|pair| [data(&pair[0]), data(&pair[1])])
                        .collect(),
                ),
                other => panic!("Unsupported primary function projection {other}"),
            }
        }
        _ => panic!("Malformed primary function projection"),
    };
    Form {
        span: 0..0,
        metadata: vec![],
        kind,
    }
}

fn normalize(form: &mut Form) {
    form.span = 0..0;
    form.metadata.clear();
    if let Kind::Vector(items) | Kind::List(items) = &mut form.kind {
        for item in items {
            normalize(item);
        }
    }
    if let Kind::Map(items) = &mut form.kind {
        for item in items.iter_mut() {
            normalize(item);
        }
        let mut pairs = items
            .chunks_exact(2)
            .map(|pair| (pair[0].clone(), pair[1].clone()))
            .collect::<Vec<_>>();
        pairs.sort_by_key(|(key, _)| match &key.kind {
            Kind::Keyword(key) => key.name.clone(),
            _ => panic!("Expected primary metadata keyword"),
        });
        *items = pairs
            .into_iter()
            .flat_map(|(key, value)| [key, value])
            .collect();
    }
}

#[test]
fn compiled_argument_lists_preserve_quoted_data_and_actual_file_positions() {
    use suss_cli::portable_session::SessionOptions;
    use suss_compile::portable::resolve::Phase;
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("app.sus");
    // Match the actual observed definition's reader line and columns, while
    // independently asserting this native module's own file provenance.
    let source = format!(
        "(ns app)\n{}{}",
        "\n".repeat(42),
        concat!(
            "(def ^{:arglists '([x] ^{:doc \"rest declaration\"} [x & more])} annotated\n",
            "  (fn ([x] x) ([x & more] x)))\n"
        )
    );
    std::fs::write(&file, source).unwrap();
    let actual_path = file.canonicalize().unwrap().to_str().unwrap().to_owned();
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(
            SessionOptions {
                source_paths: vec![root.path().to_owned()],
                ..SessionOptions::default()
            },
            phase,
        )
        .unwrap();
        session
            .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
            .unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros.define(DEFINITION).unwrap();
        session
            .load_namespace_with_macros("app", &mut macros)
            .unwrap();
        session.enter_namespace("app").unwrap();
        let value = session
            .eval_with_macros("(user/function-facts annotated)", &mut macros)
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let mut actual = bridge.read(&mut session, &value, 0..1).unwrap();
        fn normalize_file(form: &mut Form, path: &str) {
            match &mut form.kind {
                Kind::Map(items) => {
                    for pair in items.chunks_exact_mut(2) {
                        if matches!(&pair[0].kind, Kind::Keyword(key) if key.name == "file") {
                            assert_eq!(pair[1].kind, Kind::String(path.encode_utf16().collect()));
                            pair[1].kind = Kind::String(
                                "tests/oracle/src/suss_oracle/declaration_runner.cljs"
                                    .encode_utf16()
                                    .collect(),
                            );
                        } else {
                            normalize_file(&mut pair[1], path);
                        }
                    }
                }
                Kind::List(items) | Kind::Vector(items) => {
                    for item in items {
                        normalize_file(item, path);
                    }
                }
                _ => {}
            }
        }
        normalize_file(&mut actual, &actual_path);
        normalize(&mut actual);
        let mut primary = expected("after-arglists", "annotated");
        normalize(&mut primary);
        assert_eq!(
            actual, primary,
            "Exact quoted lists and reader metadata projection"
        );
        let result = session.eval("(annotated 22 23)").unwrap();
        assert_eq!(
            bridge.read(&mut session, &result, 0..1).unwrap().kind,
            Kind::Number(22.0)
        );
    }
}

fn expected(label: &str, name: &str) -> Form {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/declaration-environment-observations.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let row = corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[0] == label)
        .unwrap();
    let declaration = row[2]
        .as_array()
        .unwrap()
        .iter()
        .find(|declaration| declaration[0] == name)
        .unwrap();
    assert_eq!(declaration[1], true);
    let mut items = Vec::new();
    for field in FIELDS {
        let value = declaration[2]
            .as_array()
            .unwrap()
            .iter()
            .find(|value| value[0] == field)
            .unwrap();
        items.push(Form {
            span: 0..0,
            metadata: vec![],
            kind: Kind::Vector(vec![data(&value[1]), data(&value[2])]),
        });
    }
    Form {
        span: 0..0,
        metadata: vec![],
        kind: Kind::Vector(items),
    }
}

#[test]
fn compiled_function_declarations_match_primary_presence_and_parameter_shapes() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros.define(DEFINITION).unwrap();
        session.eval_with_macros("(def fixed (fn [x] x)) (def multiple (fn self ([x] x) ([x y & more] y))) (def alias fixed) (def duplicate (fn ([x] 1) ([y] 2))) (declare declared) (def declared (fn [x] x)) (def ^{:arglists {:arity [x]}} mapped-arglists (fn [x] x)) (def ^{:top-fn {}} empty-top-fn (fn [x] x)) (def ^{:top-fn {:variadic? false :max-fixed-arity 1 :method-params [[override]] :arglists [[override]] :arglists-meta [nil]}} overridden (fn [x] x)) (def ^{:declared true :arglists '([given])} declared-meta (fn [x] x)) (def ^{:top-fn nil} nil-top-fn (fn [x] x)) (def ^{:top-fn [:fn-var false]} entry-top-fn (fn [x] x)) (def ^{:top-fn ()} empty-seq-top-fn (fn [x] x))", &mut macros).unwrap();
        for (label, name) in [
            ("after-fixed", "fixed"),
            ("after-multiple", "multiple"),
            ("after-alias", "alias"),
            ("after-duplicate", "duplicate"),
            ("after-declared-definition", "declared"),
            ("after-mapped-arglists", "mapped-arglists"),
            ("after-empty-top-fn", "empty-top-fn"),
            ("after-overridden", "overridden"),
            ("after-declared-meta", "declared-meta"),
            ("after-nil-top-fn", "nil-top-fn"),
            ("after-entry-top-fn", "entry-top-fn"),
            ("after-empty-seq-top-fn", "empty-seq-top-fn"),
        ] {
            let value = session
                .eval_with_macros(&format!("(function-facts {name})"), &mut macros)
                .unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            session.collect().unwrap();
            let mut actual = bridge.read(&mut session, &value, 0..1).unwrap();
            normalize(&mut actual);
            assert_eq!(
                actual,
                expected(label, name),
                "Executed {label} in both caller phases"
            );
        }
        // Duplicate source signatures remain visible above; emission still runs
        // the final body, and aliases remain executable without fn-var facts.
        for (source, number) in [
            ("(duplicate 17)", 2.0),
            ("(alias 15)", 15.0),
            ("(multiple 12 13 14)", 13.0),
        ] {
            let value = session.eval(source).unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            assert_eq!(
                bridge.read(&mut session, &value, 0..1).unwrap().kind,
                Kind::Number(number)
            );
        }
        macros
            .define(
                r#"(defmacro staged-function-facts []
          (let [old (get (get (get &env :ns) :defs) 'fixed)
                pending (get (get (get &env :suss/catalog) :defs) 'fixed)]
            (list 'quote [(get old :fn-var) (contains? pending :fn-var)])))"#,
            )
            .unwrap();
        let value = session
            .eval_with_macros("(def fixed (staged-function-facts))", &mut macros)
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let Kind::Vector(items) = bridge.read(&mut session, &value, 0..1).unwrap().kind else {
            panic!("Expected staged declaration facts");
        };
        assert_eq!(
            items.iter().map(|item| &item.kind).collect::<Vec<_>>(),
            [&Kind::Bool(true), &Kind::Bool(false)]
        );
        let value = session
            .eval_with_macros("(function-facts fixed)", &mut macros)
            .unwrap();
        let mut actual = bridge.read(&mut session, &value, 0..1).unwrap();
        normalize(&mut actual);
        assert_eq!(
            actual,
            expected("after-alias", "alias"),
            "Scalar redefinition removes function fields; old snapshots retain them"
        );
    }
}
