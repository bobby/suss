//! Executed inference boundaries compared with pinned primary observations.
use serde_json::{Value, json};
use suss_cli::{
    portable_macro_data::FormBridge, portable_macros::CompiledMacros, portable_session::Session,
};
use suss_reader::forms::{Form, Kind};

fn tag(form: &Form) -> Value {
    match &form.kind {
        Kind::Nil => Value::Null,
        Kind::Bool(value) => json!(value),
        Kind::Symbol(symbol) => json!([
            "symbol",
            symbol
                .namespace
                .as_ref()
                .map_or_else(|| symbol.name.clone(), |ns| format!("{ns}/{}", symbol.name))
        ]),
        _ => panic!("Unexpected primary tag data kind: {form:?}"),
    }
}
fn row(form: &Form) -> Value {
    let Kind::Vector(fields) = &form.kind else {
        panic!("Expected observation row")
    };
    assert_eq!(fields.len(), 5);
    let Kind::String(label) = &fields[0].kind else {
        panic!("Expected label")
    };
    let Kind::Bool(tag_present) = fields[1].kind else {
        panic!("Expected tag presence")
    };
    let Kind::Bool(return_present) = fields[3].kind else {
        panic!("Expected return presence")
    };
    json!([
        String::from_utf16(label).unwrap(),
        tag_present,
        tag(&fields[2]),
        return_present,
        tag(&fields[4])
    ])
}
#[test]
fn completed_declaration_hint_boundaries_match_actual_primary_expression_tags() {
    let expressions = [
        ("scalar-false-hint", "scalar"),
        ("dynamic-scalar-false-hint", "*scalar*"),
        ("function-false-hint", "callable"),
        ("dynamic-function", "*callable*"),
        ("scalar-truthy-hint", "hinted"),
        ("dynamic-scalar-truthy-hint", "*hinted*"),
        ("invoke-function-false-hint", "(callable)"),
        ("invoke-dynamic-function", "(*callable*)"),
        ("local-false-hint", "(let [^{:tag false} x 7] x)"),
        ("parameter-false-hint", "(fn [^{:tag false} x] x)"),
        ("raw-return-metadata", "(raw-return 27)"),
        ("computed-return-overrides-raw", "(computed-return)"),
        ("provisional-var-tag", "provisional"),
        ("provisional-return-metadata", "(provisional)"),
        ("scalar-nil-hint", "nil-scalar"),
        ("function-nil-hint", "nil-callable"),
        ("provisional-nil-var-tag", "nil-provisional"),
        ("provisional-nil-return", "(nil-provisional)"),
    ];
    // This file is generated only by the executed pinned primary probe.
    let expected: Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/source-hint-review-observations.json"
    ))
    .unwrap();
    assert_eq!(expected["schema"], 1);
    assert_eq!(
        expected["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let expected = &expected["cases"];
    assert_eq!(expected.as_array().unwrap().len(), expressions.len());
    let mut differences = Vec::new();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(
                r#"(defmacro hint-row [label name]
          (let [init (get (get (get &env :locals) name) :init)]
            (list 'quote [label (contains? init :tag) (get init :tag)
              (contains? init :inferred-ret-tag) (get init :inferred-ret-tag)])))"#,
            )
            .unwrap();
        session
            .eval(
                r#"(def effects 0)
          (def ^{:tag false} scalar (do (set! effects (+ effects 1)) 7))
          (def ^{:dynamic true :tag false} *scalar* 8)
          (def ^{:tag false} callable (fn [] 23))
          (def ^:dynamic *callable* (fn [] 24))
          (def ^string hinted 9)
          (def ^{:dynamic true :tag string} *hinted* 10)
          (def ^{:ret-tag string} raw-return (fn [x] x))
          (def ^{:ret-tag string} computed-return (fn [] 25))
          (def ^{:declared true :tag string :ret-tag boolean} provisional (fn [] 26))
          (def ^{:tag nil} nil-scalar 29)
          (def ^{:tag nil} nil-callable (fn [] 30))
          (def ^{:declared true :tag nil :ret-tag nil} nil-provisional (fn [] 31))"#,
            )
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        for (index, (label, expression)) in expressions.iter().enumerate() {
            assert_eq!(expected[index][0], *label);
            let value = session
                .eval_with_macros(
                    &format!("(let [observed {expression}] (hint-row \"{label}\" observed))"),
                    &mut macros,
                )
                .unwrap();
            session.collect().unwrap();
            let actual = row(&bridge.read(&mut session, &value, 0..1).unwrap());
            if actual != expected[index] {
                differences.push(format!("{label}: {actual} versus {}", expected[index]));
            }
        }
        for (source, number) in [
            ("scalar", 7.0),
            ("*scalar*", 8.0),
            ("(callable)", 23.0),
            ("(*callable*)", 24.0),
            ("hinted", 9.0),
            ("*hinted*", 10.0),
            ("(let [^{:tag false} x 7] x)", 7.0),
            ("((fn [^{:tag false} x] x) 30)", 30.0),
            ("effects", 1.0),
            ("(raw-return 27)", 27.0),
            ("(computed-return)", 25.0),
            ("(provisional)", 26.0),
            ("nil-scalar", 29.0),
            ("(nil-callable)", 30.0),
            ("(nil-provisional)", 31.0),
        ] {
            let value = session.eval(source).unwrap();
            session.collect().unwrap();
            assert_eq!(
                bridge.read(&mut session, &value, 0..1).unwrap().kind,
                Kind::Number(number),
                "Actual runtime storage: {source}"
            );
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}
