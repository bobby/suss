//! Execute selected named-self and parameter binding observations from the pinned corpus.
use suss_cli::{
    portable_macro_data::FormBridge,
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionOptions},
};
use suss_compile::portable::resolve::Phase;
use suss_reader::forms::{Form, Kind};

const TOOLS: &str = r#"(ns tools)
(def observations {})
(def fields [["name" :name] ["local" :local] ["tag" :tag] ["fn-var" :fn-var]
  ["variadic?" :variadic?] ["max-fixed-arity" :max-fixed-arity]
  ["method-params" :method-params] ["arglists" :arglists]])
(def project (fn [locals names]
  (loop [i 0 rows []]
    (if (< i (count names))
      (let [sym (nth names i) info (get locals sym)
            properties (loop [j 0 items []]
              (if (< j (count fields))
                (let [field (nth fields j) key (nth field 1)]
                  (recur (+ j 1) (conj items [(nth field 0)
                    (contains? info key) (get info key)])))
                items))]
        (recur (+ i 1) (conj rows [sym (contains? locals sym) properties])))
      rows))))
(defmacro observe-locals [label names]
  (set! observations (assoc observations label
    [label (get (get &env :ns) :name) (project (get &env :locals) names)]))
  42)
(defmacro observation [label] (list 'quote (get observations label)))
"#;

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
    plain([
        data(&row[0]),
        data(&row[1]),
        plain(row[2].as_array().unwrap().iter().map(|decl| {
            plain([
                data(&decl[0]),
                data(&decl[1]),
                plain(
                    decl[2]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|field| plain([data(&field[0]), data(&field[1]), data(&field[2])])),
                ),
            ])
        })),
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
fn selected_map(value: &mut Form, fields: &[&str]) {
    let Kind::Map(items) = &mut value.kind else {
        return;
    };
    *items = items
        .chunks_exact(2)
        .filter(|pair| {
            matches!(&pair[0].kind, Kind::Keyword(key) if key.namespace.is_none()
            && fields.contains(&key.name.as_str()))
        })
        .flat_map(|pair| [pair[0].clone(), pair[1].clone()])
        .collect();
}
fn method_binding_view(value: &mut Form) {
    // This is an explicit selected portable binding view, not a claim that
    // arbitrary binding ASTs, their whole environments, or extension fields
    // are identical. Preserve every selected field's presence and data kind.
    let (Kind::List(methods) | Kind::Vector(methods)) = &mut value.kind else {
        return;
    };
    for method in methods {
        let Kind::Vector(bindings) = &mut method.kind else {
            continue;
        };
        for binding in bindings {
            selected_map(
                binding,
                &[
                    "op",
                    "name",
                    "local",
                    "form",
                    "tag",
                    "shadow",
                    "arg-id",
                    "binding-form?",
                    "line",
                    "column",
                    "env",
                    "info",
                ],
            );
            if let Kind::Map(pairs) = &mut binding.kind {
                for pair in pairs.chunks_exact_mut(2) {
                    if let Kind::Keyword(key) = &pair[0].kind {
                        match key.name.as_str() {
                            "env" => selected_map(&mut pair[1], &["context", "line", "column"]),
                            "info" => selected_map(&mut pair[1], &["name", "shadow"]),
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}
fn view(observation: &mut Form, native_identifiers: bool) {
    fn identifier(value: &mut Form) {
        let Kind::Symbol(symbol) = &value.kind else {
            panic!("Actual identifier Symbol");
        };
        value.kind = Kind::String(symbol.to_string().encode_utf16().collect());
    }
    let parts = vector(observation);
    if native_identifiers {
        identifier(&mut parts[1]);
    }
    for declaration in vector(&mut parts[2]) {
        let declaration = vector(declaration);
        if native_identifiers {
            identifier(&mut declaration[0]);
        }
        for property in vector(&mut declaration[2]) {
            let property = vector(property);
            if text(&property[0]) == "method-params" {
                method_binding_view(&mut property[2]);
            }
        }
    }
    normalize(observation);
}

#[test]
fn named_self_and_method_argument_fields_match_the_pinned_local_observations() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/declaration-environment-observations.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    assert_eq!(corpus["locals"].as_array().unwrap().len(), 2);
    let fixture = include_str!("../../../tests/oracle/src/suss_oracle/declaration_runner.cljs");
    let lines = fixture.lines().collect::<Vec<_>>();
    let start = lines.iter().position(|line| *line == "(def named").unwrap();
    let stop = lines
        .iter()
        .position(|line| line.starts_with("(def after-named"))
        .unwrap();
    assert_eq!(start, 37, "Pinned parameter coordinates");
    let source = format!(
        "(ns suss-oracle.declaration-runner (:require-macros [tools :refer [observe-locals]]))\n{}{}\n",
        "\n".repeat(start - 1),
        lines[start..stop].join("\n")
    );
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("tools.sus"), TOOLS).unwrap();
    std::fs::create_dir(root.path().join("suss_oracle")).unwrap();
    std::fs::write(
        root.path().join("suss_oracle/declaration_runner.sus"),
        source,
    )
    .unwrap();
    let mut differences = Vec::new();
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(
            SessionOptions {
                source_paths: vec![root.path().to_owned()],
                ..SessionOptions::default()
            },
            phase,
        )
        .unwrap();
        // The full quoted method-binding observation executes retained map/list
        // construction in the caller as well as the native macro helper.
        session.set_operation_fuel(100_000_000);
        session
            .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
            .unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(100_000_000);
        session
            .load_namespace_with_macros("suss-oracle.declaration-runner", &mut macros)
            .unwrap();
        for row in corpus["locals"].as_array().unwrap() {
            let label = row[0].as_str().unwrap();
            let value = session
                .eval_with_macros(&format!("(tools/observation {label:?})"), &mut macros)
                .unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            session.collect().unwrap();
            let mut actual = bridge.read(&mut session, &value, 0..1).unwrap();
            let mut primary = expected(row);
            view(&mut actual, true);
            view(&mut primary, false);
            let actual = vector(&mut actual);
            let primary = vector(&mut primary);
            assert_eq!(
                &actual[..2],
                &primary[..2],
                "Local observation identity {label}"
            );
            let actual = vector(&mut actual[2]);
            let primary = vector(&mut primary[2]);
            assert_eq!(actual.len(), primary.len());
            for (actual, primary) in actual.iter_mut().zip(primary) {
                let actual = vector(actual);
                let primary = vector(primary);
                let name = text(&actual[0]);
                assert_eq!(&actual[..2], &primary[..2], "Local presence {label} {name}");
                let actual = vector(&mut actual[2]);
                let primary = vector(&mut primary[2]);
                assert_eq!(actual.len(), primary.len());
                for (actual, primary) in actual.iter().zip(primary) {
                    if actual != primary {
                        differences.push(format!(
                            "{phase:?} local field {label} {name}: actual {actual:?}, primary {primary:?}"
                        ));
                    }
                }
            }
        }
        for expression in [
            "(== (suss-oracle.declaration-runner/named 11) 11)",
            "(== (suss-oracle.declaration-runner/named 12 13 14) 13)",
        ] {
            let value = session.eval(expression).unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            assert_eq!(
                bridge.read(&mut session, &value, 0..1).unwrap().kind,
                Kind::Bool(true)
            );
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

#[test]
fn reviewed_self_shadow_and_duplicate_parameters_retain_both_passes_and_body_order() {
    // Actual pinned analyzer output is asserted by the retained development
    // probe tests/oracle/self-local-method-review-probe.clj.
    const REVIEW_TOOLS: &str = r#"(ns review-tools)
(def observations [])
(defmacro inspect-self [label parameter]
  (let [argument (get (get &env :locals) parameter)
        self-info (if (= parameter 'self) (get argument :shadow)
                    (get (get &env :locals) 'self))
        staged (get self-info :method-params)
        staged-self (get (nth (first staged) 0) :shadow)
        duplicate-shadow (get (nth (first (rest staged)) 1) :shadow)
        row [label (get argument :arg-id) (get argument :tag)
             (get (get argument :shadow) :local)
             (get (get argument :shadow) :arg-id)
             (get (get argument :shadow) :fn-var)
             (get (get argument :env) :context)
             (get self-info :fn-var) (get self-info :variadic?)
             (get self-info :max-fixed-arity)
             (get staged-self :local) (contains? staged-self :fn-var)
             (get duplicate-shadow :arg-id)]]
    (set! observations (conj observations row))
    parameter))
(defmacro observation [] (list 'quote observations))
"#;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("review_tools.sus"), REVIEW_TOOLS).unwrap();
    std::fs::write(
        root.path().join("review.sus"),
        "(ns review (:require-macros [review-tools :refer [inspect-self]]))\n\
         (def f (fn self ([self] (inspect-self :first self))\n\
           ([x x & rest] (inspect-self :second x))))",
    )
    .unwrap();
    let mut primary = suss_reader::forms::read_forms(
        "[[:first 0 nil :fn nil true :expr true true 2 :fn false 0]\n\
          [:second 1 nil :arg 0 nil :expr true true 2 :fn false 0]]",
    )
    .unwrap()
    .remove(0);
    normalize(&mut primary);
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(
            SessionOptions {
                source_paths: vec![root.path().to_owned()],
                ..SessionOptions::default()
            },
            phase,
        )
        .unwrap();
        session.set_operation_fuel(100_000_000);
        session
            .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
            .unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(100_000_000);
        session
            .load_namespace_with_macros("review", &mut macros)
            .unwrap();
        let value = session
            .eval_with_macros("(review-tools/observation)", &mut macros)
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let mut actual = bridge.read(&mut session, &value, 0..1).unwrap();
        normalize(&mut actual);
        assert_eq!(
            actual, primary,
            "{phase:?} both-pass shadows and expansion order"
        );
        for expression in ["(== (review/f 11) 11)", "(== (review/f 12 13 14) 13)"] {
            let value = session.eval(expression).unwrap();
            assert_eq!(
                bridge.read(&mut session, &value, 0..1).unwrap().kind,
                Kind::Bool(true)
            );
        }
    }
}
