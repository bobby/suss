//! Quote AST records preserve source data without resolving or executing it.
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
fn projection(form: &Form) -> Value {
    json!(
        vector(form)
            .iter()
            .map(|row| {
                let row = vector(row);
                assert_eq!(row.len(), 4);
                let fields = |record: &Form| {
                    vector(record)
                        .iter()
                        .map(|field| {
                            let field = vector(field);
                            assert_eq!(field.len(), 3);
                            json!([datum(&field[0]), datum(&field[1]), datum(&field[2])])
                        })
                        .collect::<Vec<_>>()
                };
                json!([
                    datum(&row[0]),
                    fields(&row[1]),
                    fields(&row[2]),
                    vector(&row[3]).iter().map(datum).collect::<Vec<_>>()
                ])
            })
            .collect::<Vec<_>>()
    )
}
fn language_numbers(value: &mut Value) {
    match value {
        Value::Number(number) => *value = json!(number.as_f64().unwrap()),
        Value::Array(values) => values.iter_mut().for_each(language_numbers),
        _ => {}
    }
}

#[test]
fn quote_and_literal_child_records_match_pinned_without_datum_analysis() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/quote-ast-observations.json"
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
    let runner = include_str!("../../../tests/oracle/src/suss_oracle/quote_ast_runner.cljs");
    let source = runner
        .split_once("(def results\n")
        .unwrap()
        .1
        .split_once("\n(defn -main")
        .unwrap()
        .0
        .trim()
        .strip_suffix(')')
        .unwrap();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(
                r#"(defmacro observe [label binding]
          (let [ast (get (get (get &env :locals) binding) :init)
                expr (get ast :expr)
                field (fn [record key]
                        [(name key) (contains? record key) (get record key)])]
            (list 'quote [label
              [(field ast :op) (field ast :literal?) (field ast :form)
               (field ast :tag) (field ast :children) (field ast :val)]
              [(field expr :op) (field expr :literal?) (field expr :form)
               (field expr :tag) (field expr :val) (field expr :children)]
              [(= (get expr :form) (get expr :val))
               (= (get ast :env) (get expr :env))
               (= (get ast :tag) (get expr :tag)) (contains? expr :info)
               (identical? (get expr :form) (get expr :val))
               (identical? (if (seq? (get ast :form))
                             (second (get ast :form)) nil) (get expr :val))
               (get (meta (get expr :val)) :purpose)]])))"#,
            )
            .unwrap();
        let value = session
            .eval_with_macros(&format!("(def effects 0)\n{source}"), &mut macros)
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let actual = projection(&bridge.read(&mut session, &value, 0..1).unwrap());
        let actual = actual.as_array().unwrap();
        let expected = expected.as_array().unwrap();
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert_eq!(actual, expected, "quoted source case {}", expected[0]);
        }
        let effects = session.eval("effects").unwrap();
        session.collect().unwrap();
        assert_eq!(
            datum(&bridge.read(&mut session, &effects, 0..1).unwrap()),
            json!(0.0),
            "AST inspection must not execute quoted effectful-looking data"
        );
    }
}
