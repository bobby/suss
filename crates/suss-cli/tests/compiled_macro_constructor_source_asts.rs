//! Constructor source facts, child edges and once-only field initialization.
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
        Value::Object(values) => values.values_mut().for_each(language_numbers),
        _ => {}
    }
}

#[test]
fn constructor_source_nodes_match_pinned_children_tags_and_class_facts_after_gc() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/constructor-source-ast-observations.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 9);
    let mut expected = corpus.clone();
    language_numbers(&mut expected);
    let runner =
        include_str!("../../../tests/oracle/src/suss_oracle/constructor_source_ast_runner.cljs");
    let body = runner.lines().skip(2).collect::<Vec<_>>().join("\n");
    let body = body.split("(defn -main").next().unwrap();
    let source =
        format!("(ns suss-oracle.constructor-source-ast-runner)\n; header padding\n{body}");
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        // Finite allowance for projecting the complete nine-case source corpus.
        // The default 10M run traps; production defaults and depth stay unchanged.
        macros.set_operation_fuel(100_000_000);
        macros
            .enter_namespace("suss-oracle.constructor-source-ast-runner")
            .unwrap();
        macros.define(r#"(defmacro observe [label binding]
          (let [project (fn project [ast depth]
              (do (if (> depth 8) (throw "Constructor AST projection depth exceeded") nil)
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
                 [(contains? ast :val) literal (if literal (get ast :val) nil)] edges])))
                ast (get (get (get &env :locals) binding) :init)
                info (get (get ast :class) :info)
                field (fn [key] [(name key) (contains? info key) (get info key)])]
            (list 'quote [label (project ast 0)
              [(field :type) (field :num-fields) (field :record) (field :private)]])))"#).unwrap();
        session.eval_with_macros(&source, &mut macros).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        for index in 0..9 {
            let value = session.eval(&format!("(nth results {index})")).unwrap();
            session.collect().unwrap();
            let form = bridge.read(&mut session, &value, 0..1).unwrap();
            let pair = vector(&form);
            assert_eq!(pair.len(), 2);
            let row = vector(&pair[0]);
            assert_eq!(row.len(), 3);
            let facts = vector(&row[2])
                .iter()
                .map(|field| {
                    let field = vector(field);
                    assert_eq!(field.len(), 3);
                    json!([datum(&field[0]), datum(&field[1]), datum(&field[2])])
                })
                .collect::<Vec<_>>();
            let actual = json!([datum(&row[0]), node(&row[1]), facts]);
            assert_eq!(actual, expected["cases"][index], "constructor case {index}");
            let actual_values = vector(&pair[1]).iter().map(datum).collect::<Vec<_>>();
            assert_eq!(json!(actual_values), expected["values"][index]);
        }
        let effects = session.eval("effects").unwrap();
        session.collect().unwrap();
        let effects = bridge.read(&mut session, &effects, 0..1).unwrap();
        let actual = vector(&effects).iter().map(datum).collect::<Vec<_>>();
        assert_eq!(
            json!(actual),
            expected["effects"],
            "initializer effects must not replay"
        );
    }
}

#[test]
fn constructor_operation_and_children_fit_default_fuel_in_both_caller_phases() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        // Exercise production defaults independently of the large recursive corpus.
        assert_eq!(session.options().fuel_per_operation, 10_000_000);
        macros
            .define(
                r#"(defmacro constructor-facts [local]
          (let [ast (get (get (get &env :locals) local) :init)]
            (list 'quote [(get ast :op) (get ast :children)])))"#,
            )
            .unwrap();
        let value = session.eval_with_macros(
            "(deftype DefaultFuelConstructor [x]) (let [value (new DefaultFuelConstructor 42)] [(constructor-facts value) (.-x value)])",
            &mut macros,
        ).unwrap();
        session.collect().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let form = bridge.read(&mut session, &value, 0..1).unwrap();
        assert_eq!(
            datum(&form),
            json!([
                "vector",
                [
                    [
                        "vector",
                        [
                            ["keyword", ":new"],
                            ["vector", [["keyword", ":class"], ["keyword", ":args"]]]
                        ]
                    ],
                    42.0
                ]
            ])
        );
    }
}
