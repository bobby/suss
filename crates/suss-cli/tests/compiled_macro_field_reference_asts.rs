//! Field AST facts are observed from actual analyzed source in both phases.
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
                // Private diagnostic fields are outside the portable source record.
                .filter(|pair| !matches!(&pair[0].kind, Kind::Keyword(k) if k.namespace.as_deref() == Some("suss")))
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
fn field_reference_asts_match_pinned_flags_tags_positions_and_identity() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/field-reference-ast-observations.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 9);
    let mut expected = corpus["cases"].clone();
    language_numbers(&mut expected);
    let runner =
        include_str!("../../../tests/oracle/src/suss_oracle/field_reference_ast_runner.cljs");
    // Retain exact body lines/columns; only replace the development ns header.
    let body = runner.lines().skip(2).collect::<Vec<_>>().join("\n");
    let body = body.split("(defn -main").next().unwrap();
    let source = format!("(ns user)\n;; oracle header padding\n{body}\nresults");
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(
                r#"(defmacro observe [label binding field-name]
          (let [ast (get (get (get &env :locals) binding) :init)
                info (get ast :info)
                lexical (get (get &env :locals) field-name)
                field (fn [record key] [(name key) (contains? record key) (get record key)])]
            (list 'quote [label
              [(field ast :op) (field ast :local) (field ast :tag)
               (field ast :children) (field ast :val)]
              [(field info :local) (field info :field) (field info :mutable)
               (field info :unsynchronized-mutable) (field info :volatile-mutable)
               (field info :tag) (field info :shadow) (field info :line) (field info :column)]
              [(= info lexical) (identical? info lexical)
               (= (get ast :name) (get info :name))
               (contains? info :init) (contains? ast :init)]])))"#,
            )
            .unwrap();
        let value = session.eval_with_macros(&source, &mut macros).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let actual = projection(&bridge.read(&mut session, &value, 0..1).unwrap());
        for (actual, expected) in actual
            .as_array()
            .unwrap()
            .iter()
            .zip(expected.as_array().unwrap())
        {
            assert_eq!(actual, expected, "field AST case {}", expected[0]);
        }
        assert_eq!(actual.as_array().unwrap().len(), 9);
    }
}
