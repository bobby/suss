//! Genuine collection source nodes and child edges, independently decoded.
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

fn textual_unordered_children(rows: &mut Value) {
    // Explicit, reviewed correspondences for these three constant fixtures.
    // Keep each entire reference child unchanged and map keys/vals paired.
    // Raw primary evidence is never rewritten or generically sorted.
    for (index, label, permutation, edges) in [
        (5, "set", vec![0, 2, 1], vec!["items"]),
        (
            12,
            "factory-map",
            vec![4, 7, 8, 5, 6, 1, 3, 2, 0],
            vec!["keys", "vals"],
        ),
        (
            13,
            "factory-set",
            vec![0, 2, 6, 5, 3, 7, 4, 1, 8],
            vec!["items"],
        ),
    ] {
        assert_eq!(rows[index][0], label);
        let children = rows[index][1][2].as_array_mut().unwrap();
        assert_eq!(children.len(), edges.len());
        for (edge, name) in children.iter_mut().zip(edges) {
            assert_eq!(edge[0], name);
            assert_eq!(edge[1], true);
            assert_eq!(edge[2][0], "many");
            let original = edge[2][1].as_array().unwrap().clone();
            assert_eq!(original.len(), permutation.len());
            let mut indices = permutation.clone();
            indices.sort();
            assert_eq!(indices, (0..original.len()).collect::<Vec<_>>());
            edge[2][1] = json!(
                permutation
                    .iter()
                    .map(|&i| original[i].clone())
                    .collect::<Vec<_>>()
            );
        }
    }
}
fn portable_arithmetic_child(rows: &mut Value) {
    // The pinned + macro expands to host-specific js*. Suss retains the
    // analyzed source invocation of its intrinsic. Keep the raw corpus intact,
    // preserve both entire argument records, and assert the exact replacement.
    assert_eq!(rows[10][0], "effect-child");
    let call = &mut rows[10][1][2][0][2][1][0][2][0][2][1][0][2][1][2][1];
    assert_eq!(call[0][0], json!(["op", true, ["keyword", ":js"]]));
    assert_eq!(
        call[0][2],
        json!([
            "form",
            true,
            [
                "seq",
                [["symbol", "js*"], "(~{} + ~{})", ["symbol", "effects"], 1.0]
            ]
        ])
    );
    assert_eq!(call[2][0][0], "args");
    assert_eq!(call[2][0][2][0], "many");
    let arguments = call[2][0].clone();
    let callee = json!([
        [
            ["op", true, ["keyword", ":var"]],
            ["tag", false, null],
            ["form", true, ["symbol", "+"]],
            ["children", false, null],
            ["literal?", false, null]
        ],
        [false, false, null],
        []
    ]);
    *call = json!([
        [
            ["op", true, ["keyword", ":invoke"]],
            ["tag", true, ["symbol", "number"]],
            [
                "form",
                true,
                ["seq", [["symbol", "+"], ["symbol", "effects"], 1.0]]
            ],
            [
                "children",
                true,
                ["vector", [["keyword", ":fn"], ["keyword", ":args"]]]
            ],
            ["literal?", false, null]
        ],
        [false, false, null],
        [["fn", true, ["one", callee]], arguments]
    ]);
}

#[test]
fn collection_source_nodes_retain_original_children_before_factory_lowering() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/collection-source-ast-observations.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 14);
    let mut expected = corpus["cases"].clone();
    language_numbers(&mut expected);
    textual_unordered_children(&mut expected);
    portable_arithmetic_child(&mut expected);
    let runner =
        include_str!("../../../tests/oracle/src/suss_oracle/collection_source_ast_runner.cljs");
    let body = runner.lines().skip(2).collect::<Vec<_>>().join("\n");
    let body = body.split("(defn -main").next().unwrap();
    let source = format!("(ns suss-oracle.collection-source-ast-runner)\n; header padding\n{body}");
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(40_000_000);
        let mut macros = CompiledMacros::new().unwrap();
        // Projecting 33 children and constructing all 14 quoted rows exceeds
        // the default 10M operation budget. Keep an explicit finite test allowance;
        // depth/shape checks and every semantic assertion remain unchanged.
        macros.set_operation_fuel(40_000_000);
        macros
            .enter_namespace("suss-oracle.collection-source-ast-runner")
            .unwrap();
        macros.define(r#"(defmacro observe [label binding]
          (let [project (fn project [ast depth]
              (do (if (> depth 8) (throw "Collection AST projection depth exceeded") nil)
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
            assert_eq!(actual, *expected, "source collection case {}", expected[0]);
        }
        let effects = session.eval("effects").unwrap();
        assert_eq!(
            datum(&bridge.read(&mut session, &effects, 0..1).unwrap()),
            json!(1.0)
        );
    }
}

#[test]
fn unordered_literal_ast_edges_and_runtime_effects_preserve_textual_order() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros.define(r#"(defmacro ordered-source [binding]
          (let [ast (get (get (get &env :locals) binding) :init)
                collect (fn [nodes]
                  (loop [nodes (seq nodes) result []]
                    (if nodes
                      (let [child (first nodes)]
                        (recur (next nodes) (conj result [(get child :op) (get (get child :ret) :form)])))
                      result)))]
            (list 'quote [(get ast :op) (collect (get ast :items))
                         (collect (get ast :keys)) (collect (get ast :vals))])))"#).unwrap();
        session.eval("(def ordered-effects [])").unwrap();
        for map in [false, true] {
            session.eval("(set! ordered-effects [])").unwrap();
            let mut entries = Vec::new();
            for index in 0..9 {
                let marker = if map { index * 2 } else { index };
                entries.push(format!(
                    "(do (set! ordered-effects (conj ordered-effects {marker})) {index})"
                ));
                if map {
                    entries.push(format!(
                        "(do (set! ordered-effects (conj ordered-effects {})) {})",
                        marker + 1,
                        index + 100
                    ));
                }
            }
            let literal = format!(
                "{}{}{}",
                if map { "{" } else { "#{" },
                entries.join(" "),
                "}"
            );
            let value = session
                .eval_with_macros(
                    &format!("(let [copy {literal}] (ordered-source copy))"),
                    &mut macros,
                )
                .unwrap();
            session.collect().unwrap();
            let observation = FormBridge::new(&mut session).unwrap()
                .read(&mut session, &value, 0..1)
                .unwrap();
            let rows = |offset| {
                json!([
                    "vector",
                    (0..9)
                        .map(|i| json!(["vector", [["keyword", ":do"], (i + offset) as f64]]))
                        .collect::<Vec<_>>()
                ])
            };
            let empty = json!(["vector", []]);
            let expected = if map {
                json!(["vector", [["keyword", ":map"], empty, rows(0), rows(100)]])
            } else {
                json!(["vector", [["keyword", ":set"], rows(0), empty, empty]])
            };
            assert_eq!(datum(&observation), expected);
            let effects = session.eval("ordered-effects").unwrap();
            session.collect().unwrap();
            let effects = FormBridge::new(&mut session).unwrap()
                .read(&mut session, &effects, 0..1)
                .unwrap();
            assert_eq!(
                datum(&effects),
                json!([
                    "vector",
                    (0..if map { 18 } else { 9 })
                        .map(|i| i as f64)
                        .collect::<Vec<_>>()
                ]),
                "every entry runs once in textual order"
            );
        }
    }
}
