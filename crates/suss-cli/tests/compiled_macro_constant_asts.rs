//! Executed source-macro projections of genuine scalar initializer AST facts.
use serde_json::{Value, json};
use suss_cli::{
    portable_macro_data::FormBridge, portable_macros::CompiledMacros, portable_session::Session,
};
use suss_reader::forms::{Form, Kind};

fn data(form: &Form) -> Value {
    match &form.kind {
        Kind::Nil => Value::Null,
        Kind::Bool(value) => json!(value),
        Kind::Number(value) => json!(value),
        Kind::String(value) => json!(String::from_utf16(value).unwrap()),
        Kind::Keyword(value) => json!([
            "keyword",
            value.namespace.as_ref().map_or_else(
                || format!(":{}", value.name),
                |ns| format!(":{ns}/{}", value.name)
            )
        ]),
        Kind::Vector(values) => json!(values.iter().map(data).collect::<Vec<_>>()),
        _ => panic!("unsupported constant AST projection: {form:?}"),
    }
}

#[test]
fn scalar_initializer_operations_and_values_match_pinned_source_ast_without_execution() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/constant-ast-observations.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let expressions = [
        ("nil", "nil"),
        ("boolean", "false"),
        ("number", "42"),
        ("string", "\"hello\""),
        ("keyword", ":word"),
    ];
    assert_eq!(corpus["cases"].as_array().unwrap().len(), expressions.len());
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(
                r#"(defmacro inspect-constant [label name]
          (let [init (get (get (get &env :locals) name) :init)]
            (list 'quote [label
              ["op" (contains? init :op) (get init :op)]
              ["val" (contains? init :val) (get init :val)]
              ["children" (contains? init :children) (get init :children)]])))"#,
            )
            .unwrap();
        let source = format!(
            "[{}]",
            expressions
                .iter()
                .map(|(label, expr)| format!(
                    "(let [observed {expr}] (inspect-constant \"{label}\" observed))"
                ))
                .collect::<Vec<_>>()
                .join("\n")
        );
        let value = session.eval_with_macros(&source, &mut macros).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let actual = data(&bridge.read(&mut session, &value, 0..1).unwrap());
        // Numbers in the language are binary64; JSON corpus integers project
        // to that same numeric type, without conflating nil, false and absence.
        let mut expected = corpus["cases"].clone();
        expected[2][2][2] = json!(42.0);
        assert_eq!(actual, expected);
        let effect = session
            .eval_with_macros(
                r#"(def effects 0)
          (let [observed (do (set! effects (+ effects 1)) 42)]
            [(inspect-constant "effect" observed) observed effects])"#,
                &mut macros,
            )
            .unwrap();
        session.collect().unwrap();
        let effect = data(&bridge.read(&mut session, &effect, 0..1).unwrap());
        assert_eq!(effect[1], json!(42.0));
        assert_eq!(
            effect[2],
            json!(1.0),
            "inspection must not rerun initialization"
        );
        assert_eq!(
            effect[0][2],
            json!(["val", false, null]),
            "an effectful initializer must not be fabricated as a scalar constant"
        );
    }
}

#[test]
fn scalar_ast_values_preserve_binary64_and_utf16_without_physical_value_inference() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(
                r#"(defmacro scalar-value [name]
          (let [init (get (get (get &env :locals) name) :init)]
            (list 'quote [(get init :op) (contains? init :val) (get init :val)])))"#,
            )
            .unwrap();
        let value = session
            .eval_with_macros(
                r#"[
          (let [observed -0.0] (scalar-value observed))
          (let [observed ##Inf] (scalar-value observed))
          (let [observed ##-Inf] (scalar-value observed))
          (let [observed ##NaN] (scalar-value observed))
          (let [observed "\ud800x\udc00"] (scalar-value observed))]"#,
                &mut macros,
            )
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let decoded = bridge.read(&mut session, &value, 0..1).unwrap();
        let Kind::Vector(rows) = decoded.kind else {
            panic!("scalar AST rows")
        };
        assert_eq!(rows.len(), 5);
        for row in &rows {
            let Kind::Vector(fields) = &row.kind else {
                panic!("scalar AST fields")
            };
            assert_eq!(fields.len(), 3);
            assert!(
                matches!(&fields[0].kind, Kind::Keyword(k) if k.namespace.is_none() && k.name == "const")
            );
            assert!(
                matches!(fields[1].kind, Kind::Bool(true)),
                "val is present, even for nonfinite source numbers"
            );
        }
        for (row, bits) in rows[..4].iter().zip([
            (-0.0_f64).to_bits(),
            f64::INFINITY.to_bits(),
            f64::NEG_INFINITY.to_bits(),
            f64::NAN.to_bits(),
        ]) {
            let Kind::Vector(fields) = &row.kind else {
                unreachable!()
            };
            let Kind::Number(number) = fields[2].kind else {
                panic!("binary64 AST value")
            };
            assert_eq!(number.to_bits(), bits);
        }
        let Kind::Vector(fields) = &rows[4].kind else {
            unreachable!()
        };
        assert!(matches!(&fields[2].kind, Kind::String(units) if units == &[0xd800, 0x78, 0xdc00]));
    }
}

#[test]
fn reviewed_scalar_facts_keep_source_values_and_do_not_classify_local_reads_as_constants() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(
                r#"(defmacro inspect-source [name]
          (let [init (get (get (get &env :locals) name) :init)]
            (list 'quote [(contains? init :op) (get init :op)
                          (contains? init :val) (get init :val)
                          (get init :form) (contains? init :suss/lowering)])))"#,
            )
            .unwrap();
        let value = session
            .eval_with_macros(
                r#"[
          (let [observed true] (inspect-source observed))
          (let [observed :app/word] (inspect-source observed))
          (let [observed "a\n\"b"] (inspect-source observed))
          (let [observed false copy observed] (inspect-source copy))
          (let [observed 42 copy (+ observed 0)] (inspect-source copy))]"#,
                &mut macros,
            )
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let decoded = bridge.read(&mut session, &value, 0..1).unwrap();
        let Kind::Vector(rows) = decoded.kind else {
            panic!("source AST rows")
        };
        assert_eq!(rows.len(), 5);
        for row in &rows[..3] {
            let Kind::Vector(fields) = &row.kind else {
                panic!("source fields")
            };
            assert!(matches!(fields[0].kind, Kind::Bool(true)));
            assert!(
                matches!(&fields[1].kind, Kind::Keyword(k) if k.namespace.is_none() && k.name == "const")
            );
            assert!(matches!(fields[2].kind, Kind::Bool(true)));
            assert_eq!(
                fields[3].kind, fields[4].kind,
                "val retains the source form rather than physical lowering"
            );
            assert!(
                matches!(fields[5].kind, Kind::Bool(true)),
                "native lowering remains separately present"
            );
        }
        let Kind::Vector(fields) = &rows[1].kind else {
            unreachable!()
        };
        assert!(
            matches!(&fields[3].kind, Kind::Keyword(k) if k.namespace.as_deref() == Some("app") && k.name == "word")
        );
        let Kind::Vector(fields) = &rows[2].kind else {
            unreachable!()
        };
        assert!(matches!(&fields[3].kind, Kind::String(s) if s == &[0x61, 10, 0x22, 0x62]));
        for (index, row) in rows[3..].iter().enumerate() {
            let Kind::Vector(fields) = &row.kind else {
                panic!("nonconstant fields")
            };
            if index == 0 {
                assert!(matches!(fields[0].kind, Kind::Bool(true)));
                assert!(matches!(&fields[1].kind, Kind::Keyword(k)
                    if k.namespace.is_none() && k.name == "local"));
            } else {
                assert!(matches!(fields[0].kind, Kind::Bool(false)));
                assert!(matches!(fields[1].kind, Kind::Nil));
            }
            assert!(
                matches!(fields[2].kind, Kind::Bool(false)),
                "a scalar runtime result does not fabricate a source constant"
            );
            assert!(matches!(fields[3].kind, Kind::Nil));
            assert!(matches!(fields[5].kind, Kind::Bool(true)));
        }
    }
}
