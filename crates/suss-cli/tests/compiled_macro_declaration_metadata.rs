//! Compare actual compiled macro declaration records with the pinned corpus.
use suss_cli::{
    portable_macro_data::FormBridge,
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionOptions},
};
use suss_compile::portable::resolve::Phase;
use suss_reader::forms::{Form, Kind};

const TOOLS: &str = r#"(ns tools)
(def observations {})
(def fields [["name" :name] ["ns" :ns] ["tag" :tag] ["ret-tag" :ret-tag]
  ["fn-var" :fn-var] ["variadic?" :variadic?] ["max-fixed-arity" :max-fixed-arity]
  ["method-params" :method-params] ["arglists" :arglists] ["arglists-meta" :arglists-meta]
  ["private" :private] ["dynamic" :dynamic] ["doc" :doc] ["declared" :declared]
  ["line" :line] ["column" :column] ["file" :file] ["meta" :meta]])
(def project (fn [defs names]
  (loop [i 0 rows []]
    (if (< i (count names))
      (let [sym (nth names i) info (get defs sym)
            properties (loop [j 0 items []]
              (if (< j (count fields))
                (let [field (nth fields j) key (nth field 1)]
                  (recur (+ j 1) (conj items [(nth field 0)
                    (contains? info key) (get info key)])))
                items))]
        (recur (+ i 1) (conj rows [sym (contains? defs sym) properties])))
      rows))))
(defmacro observe [label names]
  (set! observations
    (assoc observations label
      [label (get (get &env :ns) :name)
       (project (get (get &env :ns) :defs) names)
       (project (get (get &env :suss/catalog) :defs) names)]))
  42)
(defmacro observe-locals [label names] 42)
(defmacro observation [labels]
  (list 'quote
    (loop [i 0 rows []]
      (if (< i (count labels))
        (recur (+ i 1) (conj rows (get observations (nth labels i))))
        rows))))
"#;

#[test]
fn declaration_metadata_merge_and_tag_precedence_match_pinned_analyzer() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
        session
            .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
            .unwrap();
        session.enter_namespace("user").unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(
                r#"(defmacro projection [name]
          (let [info (get (get (get &env :suss/catalog) :defs) name)]
            (list 'quote [(get info :name) (get info :tag) (get info :ret-tag)
                          (get info :private) (get info :doc)
                          (contains? (get info :meta) :test)
                          (get (get info :meta) :test)])))"#,
            )
            .unwrap();
        for (source, expected) in [
            (
                "(def ^:dynamic f (fn [] 7)) (projection f)",
                "[user/f nil number nil nil false nil]",
            ),
            (
                "(def ^{:tag false} g (fn [] 7)) (projection g)",
                "[user/g false number nil nil false nil]",
            ),
            (
                "(def ^{:tag false} scalar 7) (projection scalar)",
                "[user/scalar number nil nil nil false nil]",
            ),
            (
                "(def ^{:name custom :test true} raw 7) (projection raw)",
                "[custom number nil nil nil false nil]",
            ),
            (
                "(def ^{:top-fn {:name replaced :private false :doc \"override\" :meta {:test :replacement} :ret-tag wrong}} overlay (fn [] 7)) (projection overlay)",
                "[replaced nil number false \"override\" true :replacement]",
            ),
        ] {
            let value = session.eval_with_macros(source, &mut macros).unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            session.collect().unwrap();
            let mut actual = bridge.read(&mut session, &value, 0..1).unwrap();
            let mut expected = suss_reader::forms::read_forms(expected).unwrap().remove(0);
            normalize(&mut actual);
            normalize(&mut expected);
            assert_eq!(actual, expected, "{phase:?}: {source}");
        }
        macros.define("(defmacro raw-meta [name] (list 'quote (get (get (get (get &env :suss/catalog) :defs) name) :meta)))").unwrap();
        let value = session
            .eval_with_macros(
                "(def ^{:meta [:raw]} pending (raw-meta pending)) pending",
                &mut macros,
            )
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let mut actual = bridge.read(&mut session, &value, 0..1).unwrap();
        normalize(&mut actual);
        assert_eq!(
            actual.kind,
            Kind::Vector(vec![form(Kind::Keyword(suss_reader::Keyword::new("raw")))])
        );
        macros.define("(defmacro core-file [name] (let [info (get (get (get &env :suss/catalog) :defs) name)] (list 'quote [(get info :file) (get (get info :meta) :file)])))").unwrap();
        session.enter_namespace("cljs.core").unwrap();
        let value = session
            .eval_with_macros(
                "(def core-probe 7) (user/core-file core-probe)",
                &mut macros,
            )
            .unwrap();
        let mut actual = bridge.read(&mut session, &value, 0..1).unwrap();
        normalize(&mut actual);
        let file = form(Kind::String("cljs/core.cljs".encode_utf16().collect()));
        assert_eq!(actual.kind, Kind::Vector(vec![file.clone(), file]));
    }
}

fn form(kind: Kind) -> Form {
    Form {
        span: 0..0,
        metadata: vec![],
        kind,
    }
}
fn plain(values: impl IntoIterator<Item = Form>) -> Form {
    form(Kind::Vector(values.into_iter().collect()))
}
fn data(value: &serde_json::Value) -> Form {
    form(match value {
        serde_json::Value::Null => Kind::Nil,
        serde_json::Value::Bool(value) => Kind::Bool(*value),
        serde_json::Value::Number(value) => Kind::Number(value.as_f64().unwrap()),
        serde_json::Value::String(value) => Kind::String(value.encode_utf16().collect()),
        serde_json::Value::Array(values) if values.len() == 2 => {
            match values[0].as_str().unwrap() {
                "vector" => Kind::Vector(values[1].as_array().unwrap().iter().map(data).collect()),
                "seq" => Kind::List(values[1].as_array().unwrap().iter().map(data).collect()),
                "map" => Kind::Map(
                    values[1]
                        .as_array()
                        .unwrap()
                        .iter()
                        .flat_map(|pair| [data(&pair[0]), data(&pair[1])])
                        .collect(),
                ),
                "set" => Kind::Set(values[1].as_array().unwrap().iter().map(data).collect()),
                "symbol" | "keyword" => {
                    let keyword = values[0] == "keyword";
                    let spelling = values[1].as_str().unwrap();
                    let spelling = if keyword {
                        spelling.strip_prefix(':').unwrap()
                    } else {
                        spelling
                    };
                    let (namespace, name) = spelling
                        .split_once('/')
                        .map_or((None, spelling), |(ns, name)| (Some(ns.to_owned()), name));
                    if keyword {
                        Kind::Keyword(suss_reader::Keyword {
                            namespace,
                            name: name.to_owned(),
                        })
                    } else {
                        Kind::Symbol(suss_reader::Symbol {
                            namespace,
                            name: name.to_owned(),
                        })
                    }
                }
                other => panic!("Unknown primary declaration data {other}"),
            }
        }
        _ => panic!("Invalid primary declaration data"),
    })
}
fn expected(row: &serde_json::Value) -> Form {
    let declarations =
        |values: &serde_json::Value| {
            plain(values.as_array().unwrap().iter().map(|decl| {
                plain([
                    data(&decl[0]),
                    data(&decl[1]),
                    plain(
                        decl[2].as_array().unwrap().iter().map(|field| {
                            plain([data(&field[0]), data(&field[1]), data(&field[2])])
                        }),
                    ),
                ])
            }))
        };
    plain([
        data(&row[0]),
        data(&row[1]),
        declarations(&row[2]),
        declarations(&row[3]),
    ])
}
fn normalize(value: &mut Form) {
    value.span = 0..0;
    value.metadata.clear();
    match &mut value.kind {
        Kind::List(items) | Kind::Vector(items) | Kind::Set(items) => {
            for item in items {
                normalize(item);
            }
        }
        Kind::Map(items) => {
            for item in items.iter_mut() {
                normalize(item);
            }
            let mut pairs = items
                .chunks_exact(2)
                .map(|pair| (pair[0].clone(), pair[1].clone()))
                .collect::<Vec<_>>();
            pairs.sort_by_key(|(key, _)| match &key.kind {
                Kind::Keyword(key) => format!(":{key}"),
                Kind::Symbol(key) => key.to_string(),
                _ => panic!("Unobserved primary metadata key"),
            });
            *items = pairs
                .into_iter()
                .flat_map(|(key, value)| [key, value])
                .collect();
        }
        _ => {}
    }
}
fn vector(value: &mut Form) -> &mut Vec<Form> {
    let Kind::Vector(items) = &mut value.kind else {
        panic!("Expected observation vector");
    };
    items
}
fn text(value: &Form) -> String {
    let Kind::String(units) = &value.kind else {
        panic!("Expected observation string");
    };
    String::from_utf16(units).unwrap()
}
fn normalize_file(value: &mut Form, path: &str) {
    assert_eq!(text(value), path, "Native canonical source provenance");
    value.kind = Kind::String(
        "tests/oracle/src/suss_oracle/declaration_runner.cljs"
            .encode_utf16()
            .collect(),
    );
}
fn metadata_file(value: &mut Form, path: &str) {
    let Kind::Map(items) = &mut value.kind else {
        panic!("Expected metadata map");
    };
    for pair in items.chunks_exact_mut(2) {
        if matches!(&pair[0].kind,Kind::Keyword(key) if key.namespace.is_none() && key.name=="file")
        {
            normalize_file(&mut pair[1], path);
        }
    }
}
fn source_files(observation: &mut Form, path: &str) {
    // The primary helper prints only these identifier slots. Native transport
    // retains actual Symbols; this exact host projection avoids requiring the
    // still-unported public str implementation to inspect declaration facts.
    fn identifier(value: &mut Form) {
        let Kind::Symbol(symbol) = &value.kind else {
            panic!("Expected actual namespace/name Symbol");
        };
        value.kind = Kind::String(symbol.to_string().encode_utf16().collect());
    }
    identifier(&mut vector(observation)[1]);
    for declarations in &mut vector(observation)[2..] {
        for declaration in vector(declarations) {
            identifier(&mut vector(declaration)[0]);
            for property in vector(&mut vector(declaration)[2]) {
                let property = vector(property);
                let name = text(&property[0]);
                if property[1].kind != Kind::Bool(true) {
                    continue;
                }
                match name.as_str() {
                    "file" => normalize_file(&mut property[2], path),
                    "meta" => metadata_file(&mut property[2], path),
                    "arglists-meta" => {
                        let (Kind::List(items) | Kind::Vector(items)) = &mut property[2].kind
                        else {
                            panic!("Expected argument metadata list");
                        };
                        for metadata in items {
                            if metadata.kind != Kind::Nil {
                                metadata_file(metadata, path);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[test]
fn compiled_declaration_records_match_actual_primary_snapshot_and_catalog_fields() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/declaration-environment-observations.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 29);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let source = include_str!("../../../tests/oracle/src/suss_oracle/declaration_runner.cljs");
    let lines = source.lines().collect::<Vec<_>>();
    let stop = lines
        .iter()
        .position(|line| line.starts_with("(defn -main"))
        .unwrap();
    // Keep all definition/name/argument-list source coordinates unchanged.
    let source = format!(
        "(ns suss-oracle.declaration-runner (:require-macros [tools :refer [observe observe-locals]]))\n\n{}\n",
        lines[2..stop].join("\n")
    );
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("tools.sus"), TOOLS).unwrap();
    std::fs::create_dir(root.path().join("suss_oracle")).unwrap();
    let file = root.path().join("suss_oracle/declaration_runner.sus");
    std::fs::write(&file, source).unwrap();
    let path = file.canonicalize().unwrap().to_str().unwrap().to_owned();
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
        // The observer constructs up to 12 records of 18 three-element field
        // projections. Keep a finite budget appropriate to this real workload.
        macros.set_operation_fuel(100_000_000);
        session
            .load_namespace_with_macros("suss-oracle.declaration-runner", &mut macros)
            .unwrap();
        // Batch at most two observations within the bridge's unchanged
        // per-form bound. Every field of every case is still compared exactly.
        for rows in corpus["cases"].as_array().unwrap().chunks(2) {
            let labels = rows
                .iter()
                .map(|row| format!("{:?}", row[0].as_str().unwrap()))
                .collect::<Vec<_>>()
                .join(" ");
            let value = session
                .eval_with_macros(&format!("(tools/observation [{labels}])"), &mut macros)
                .unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            session.collect().unwrap();
            let mut batch = bridge.read(&mut session, &value, 0..1).unwrap();
            assert_eq!(vector(&mut batch).len(), rows.len());
            for (actual, row) in vector(&mut batch).iter_mut().zip(rows) {
                let label = row[0].as_str().unwrap();
                source_files(actual, &path);
                normalize(actual);
                let mut primary = expected(row);
                normalize(&mut primary);
                let actual = vector(actual);
                let primary = vector(&mut primary);
                assert_eq!(&actual[..2], &primary[..2], "Observation identity {label}");
                for (group, (actual, primary)) in ["snapshot", "catalog"]
                    .iter()
                    .zip(actual[2..].iter_mut().zip(primary[2..].iter_mut()))
                {
                    let actual = vector(actual);
                    let primary = vector(primary);
                    assert_eq!(
                        actual.len(),
                        primary.len(),
                        "Declaration count {label} {group}"
                    );
                    for (actual, primary) in actual.iter_mut().zip(primary) {
                        let actual = vector(actual);
                        let primary = vector(primary);
                        let name = text(&actual[0]);
                        assert_eq!(
                            &actual[..2],
                            &primary[..2],
                            "Declaration presence {label} {group} {name}"
                        );
                        let actual = vector(&mut actual[2]);
                        let primary = vector(&mut primary[2]);
                        assert_eq!(actual.len(), primary.len());
                        for (actual, primary) in actual.iter().zip(primary) {
                            assert_eq!(actual, primary, "Field {label} {group} {name}");
                        }
                    }
                }
            }
        }
    }
}
