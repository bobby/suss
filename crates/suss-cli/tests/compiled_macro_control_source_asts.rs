//! Genuine control source nodes and child edges, independently decoded.
use serde_json::{Value, json};
use suss_cli::{
    portable_macro_data::FormBridge, portable_macros::CompiledMacros, portable_session::Session,
};
use suss_reader::forms::{Form, Kind};

fn vector(form: &Form) -> &[Form] {
    let Kind::Vector(items) = &form.kind else {
        panic!("expected projection vector: {form:?}")
    };
    items
}
fn datum(form: &Form) -> Value {
    match &form.kind {
        Kind::Nil => Value::Null,
        Kind::Bool(value) => json!(value),
        Kind::Number(value) => json!(value),
        Kind::String(value) => json!(String::from_utf16(value).unwrap()),
        Kind::Symbol(value) => json!(["symbol", value.to_string()]),
        Kind::Keyword(value) => json!([
            "keyword",
            value.namespace.as_ref().map_or_else(
                || format!(":{}", value.name),
                |ns| format!(":{ns}/{}", value.name)
            )
        ]),
        Kind::Vector(values) => json!(["vector", values.iter().map(datum).collect::<Vec<_>>()]),
        Kind::List(values) => json!(["seq", values.iter().map(datum).collect::<Vec<_>>()]),
        Kind::Set(values) => {
            let mut values = values.iter().map(datum).collect::<Vec<_>>();
            values.sort_by_key(Value::to_string);
            json!(["set", values])
        }
        Kind::Map(values) => {
            let mut values = values
                .chunks_exact(2)
                .map(|pair| json!([datum(&pair[0]), datum(&pair[1])]))
                .collect::<Vec<_>>();
            values.sort_by_key(|pair| pair[0].to_string());
            json!(["map", values])
        }
        _ => panic!("unsupported quoted projection: {form:?}"),
    }
}

fn node(form: &Form) -> Value {
    let parts = vector(form);
    assert_eq!(parts.len(), 3);
    let fields = vector(&parts[0])
        .iter()
        .map(|field| {
            let field = vector(field);
            assert_eq!(field.len(), 3);
            json!([datum(&field[0]), datum(&field[1]), datum(&field[2])])
        })
        .collect::<Vec<_>>();
    let literal = vector(&parts[1]);
    let edges = vector(&parts[2])
        .iter()
        .map(|edge| {
            let edge = vector(edge);
            let child = vector(&edge[2]);
            let cardinality = datum(&child[0]);
            let value = if cardinality == "many" {
                json!(vector(&child[1]).iter().map(node).collect::<Vec<_>>())
            } else {
                assert_eq!(cardinality, "one");
                node(&child[1])
            };
            json!([datum(&edge[0]), datum(&edge[1]), [cardinality, value]])
        })
        .collect::<Vec<_>>();
    json!([
        fields,
        [datum(&literal[0]), datum(&literal[1]), datum(&literal[2])],
        edges
    ])
}
fn language_numbers(value: &mut Value) {
    match value {
        Value::Number(number) => *value = json!(number.as_f64().unwrap()),
        Value::Array(values) => values.iter_mut().for_each(language_numbers),
        _ => {}
    }
}

// A private compiler-generated catch symbol uses Suss's own deterministic
// allocation counter. Keep the raw pinned e16497 observation unchanged and
// compare a separate view with exactly two explicitly checked occurrences.
fn native_catch_expected(expected: &Value, actual: &Value) -> Value {
    fn payload(row: &Value) -> &Value {
        let edge = row[1][2]
            .as_array()
            .unwrap()
            .iter()
            .find(|edge| edge[0] == "catch")
            .unwrap();
        assert_eq!(edge[1], true);
        assert_eq!(edge[2][0], "one");
        let form = &edge[2][1][0][2][2];
        assert_eq!(form[0], "seq");
        assert_eq!(form[1][0], json!(["symbol", "let*"]));
        assert_eq!(form[1][1][0], "vector");
        assert_eq!(form[1][1][1][0], json!(["symbol", "error"]));
        let symbol = &form[1][1][1][1];
        assert_eq!(symbol[0], "symbol");
        symbol
    }
    fn replace(value: &mut Value, before: &Value, after: &Value) -> usize {
        if value == before {
            *value = after.clone();
            return 1;
        }
        match value {
            Value::Array(items) => items
                .iter_mut()
                .map(|value| replace(value, before, after))
                .sum(),
            _ => 0,
        }
    }
    let reference = payload(expected);
    assert_eq!(reference, &json!(["symbol", "e16497"]));
    let native = payload(actual);
    let name = native[1].as_str().unwrap();
    let suffix = name
        .strip_prefix("$exception")
        .expect("native private payload prefix");
    assert!(!suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()));
    assert_ne!(
        reference, native,
        "generated spelling variance must remain explicit"
    );
    let mut expected = expected.clone();
    assert_eq!(
        replace(&mut expected, reference, native),
        2,
        "exact two reference payload uses"
    );
    let mut checked_actual = actual.clone();
    assert_eq!(
        replace(&mut checked_actual, native, reference),
        2,
        "exact two native payload uses"
    );
    expected
}

fn check_source_cases() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/control-source-ast-observations.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 15);
    assert_eq!(corpus["effects"], 2);
    let mut expected = corpus["cases"].clone();
    language_numbers(&mut expected);
    let runner =
        include_str!("../../../tests/oracle/src/suss_oracle/control_source_ast_runner.cljs");
    let body = runner.lines().skip(2).collect::<Vec<_>>().join("\n");
    let body = body.split("(defn -main").next().unwrap();
    let source = format!("(ns suss-oracle.control-source-ast-runner)\n; header padding\n{body}");
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // Finite test allowance for the complete quoted declared-child corpus;
        // shipped operation defaults and projection depth remain unchanged.
        session.set_operation_fuel(40_000_000);
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(40_000_000);
        macros
            .enter_namespace("suss-oracle.control-source-ast-runner")
            .unwrap();
        macros.define(r#"(defmacro observe [label binding]
          (let [project (fn project [ast depth]
              (do (if (> depth 8) (throw "Control AST projection depth exceeded") nil)
              (let [field (fn [key] [(name key) (contains? ast key) (get ast key)])
                    literal (= :const (get ast :op))
                    edges (loop [keys (seq (get ast :children)) output []]
                      (if keys
                        (let [key (first keys) value (get ast key)
                              children (if (vector? value)
                                ["many" (loop [items (seq value) rows []]
                                  (if items
                                    (recur (next items) (conj rows (project (first items) (+ depth 1))))
                                    rows))]
                                ["one" (project value (+ depth 1))])]
                          (recur (next keys) (conj output [(name key) (contains? ast key) children])))
                        output))]
                [[(field :op) (field :tag) (field :form) (field :children) (field :literal?)]
                 [(contains? ast :val) literal (if literal (get ast :val) nil)] edges])))]
            (list 'quote [label (project (get (get (get &env :locals) binding) :init) 0)])))"#).unwrap();
        session.eval_with_macros(&source, &mut macros).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        for (index, expected) in expected.as_array().unwrap().iter().enumerate() {
            let value = session.eval(&format!("(nth results {index})")).unwrap();
            session.collect().unwrap();
            let form = bridge.read(&mut session, &value, 0..1).unwrap();
            let row = vector(&form);
            let actual = json!([datum(&row[0]), node(&row[1])]);
            let native_expected = if expected[0] == "try-catch" {
                native_catch_expected(expected, &actual)
            } else {
                expected.clone()
            };
            assert_eq!(
                actual, native_expected,
                "source control case {}",
                expected[0]
            );
        }
        let value = session.eval("effects").unwrap();
        session.collect().unwrap();
        let actual = bridge.read(&mut session, &value, 0..1).unwrap();
        assert!(
            matches!(actual.kind, Kind::Number(value) if value.to_bits() == 2.0_f64.to_bits()),
            "cleanup state: {actual:?}"
        );
    }
}

#[test]
fn control_source_nodes_match_genuine_primary_children_and_forms_after_gc() {
    check_source_cases();
}

#[test]
fn function_metadata_keeps_wrapper_and_inner_method_edges() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(
                r#"(defmacro inspect-function [binding]
            (let [ast (get (get (get &env :locals) binding) :init)
                  expression (get ast :expr)
                  method (first (get expression :methods))]
              (list 'quote [(get ast :op) (get ast :children)
                            (get expression :op) (get expression :children)
                            (get method :op) (get method :children)])))"#,
            )
            .unwrap();
        let value = session
            .eval_with_macros(
                r#"(let [copy ^{:doc "function"} (fn* [x] x)] (inspect-function copy))"#,
                &mut macros,
            )
            .unwrap();
        session.collect().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let actual = bridge.read(&mut session, &value, 0..1).unwrap();
        assert_eq!(
            datum(&actual),
            json!([
                "vector",
                [
                    ["keyword", ":with-meta"],
                    ["vector", [["keyword", ":meta"], ["keyword", ":expr"]]],
                    ["keyword", ":fn"],
                    ["vector", [["keyword", ":methods"]]],
                    ["keyword", ":fn-method"],
                    ["vector", [["keyword", ":params"], ["keyword", ":body"]]]
                ]
            ])
        );
    }
}

#[test]
fn catch_alpha_view_rejects_inconsistent_or_non_private_payload_names() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/control-source-ast-observations.json"
    ))
    .unwrap();
    let reference = &corpus["cases"][13];
    fn rename(value: &mut Value) {
        if value == &json!(["symbol", "e16497"]) {
            *value = json!(["symbol", "$exception34"]);
        } else if let Value::Array(values) = value {
            values.iter_mut().for_each(rename);
        }
    }
    let mut native = reference.clone();
    rename(&mut native);
    assert_eq!(native_catch_expected(reference, &native), native);
    let mut inconsistent = native.clone();
    let catch = &mut inconsistent[1][2][1][2][1];
    catch[2][0][2][1][0][2][0][2][1][0][2][2] = json!(["symbol", "$exception35"]);
    assert!(std::panic::catch_unwind(|| native_catch_expected(reference, &inconsistent)).is_err());
    let mut non_private = native.clone();
    non_private[1][2][1][2][1][0][2][2][1][1][1][1] = json!(["symbol", "error"]);
    assert!(std::panic::catch_unwind(|| native_catch_expected(reference, &non_private)).is_err());
}

#[test]
fn method_recurrence_flags_survive_gc_in_both_caller_phases() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/oracle/method-recurrence-observations.json");
    let corpus: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let reference = corpus["cases"].as_array().unwrap();
    assert_eq!(reference.len(), 7);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(
                r#"(defmacro inspect-recurrence [binding]
          (let [methods (get (get (get (get &env :locals) binding) :init) :methods)
                rows (loop [methods (seq methods) rows []]
                  (if methods
                    (let [method (first methods)]
                      (recur (next methods)
                        (conj rows [(contains? method :recurs) (get method :recurs)
                                    (contains? (get method :body) :body?) (get (get method :body) :body?)
                                    (if (get (get method :env) :context)
                                      (name (get (get method :env) :context)) nil)
                                    (contains? (get (get method :env) :locals) 'x)])))
                    rows))]
            (list 'quote rows)))"#,
            )
            .unwrap();
        for (index, (label, function, call)) in [
            ("plain", "(fn* [x] x)", "(copy 42)"),
            (
                "method-recur",
                "(fn* [x] (if x (recur nil) 42))",
                "(copy 1)",
            ),
            (
                "nested-loop",
                "(fn* [x] (loop [y x] (if y (recur nil) 42)))",
                "(copy 1)",
            ),
            (
                "nested-function",
                "(fn* [x] (fn* [y] (if y (recur nil) x)))",
                "((copy 42) 1)",
            ),
            (
                "multiple-methods",
                "(fn* ([x] (if x (recur nil) 42)) ([x y] y))",
                "(copy 1 42)",
            ),
            (
                "variadic",
                "(fn* [x & xs] (if x (recur nil xs) 42))",
                "(copy 1 2)",
            ),
            (
                "unselected-recur",
                "(fn* [x] (if false (recur x) 42))",
                "(copy 1)",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let expected = &reference[index];
            assert_eq!(expected[0][0], label);
            let source = format!("(let [copy {function}] [(inspect-recurrence copy) {call}])");
            let value = session.eval_with_macros(&source, &mut macros).unwrap();
            session.collect().unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let actual = bridge.read(&mut session, &value, 0..source.len()).unwrap();
            let rows = expected[0][1]
                .as_array()
                .unwrap()
                .iter()
                .map(|fields| json!(["vector", fields]))
                .collect::<Vec<_>>();
            let mut result = expected[1].clone();
            language_numbers(&mut result);
            assert_eq!(
                datum(&actual),
                json!(["vector", [["vector", rows], result]]),
                "{function}"
            );
        }
    }
}
