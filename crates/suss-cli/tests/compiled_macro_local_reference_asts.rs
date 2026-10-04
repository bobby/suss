//! Executed source-local AST projections; runtime results cannot classify ASTs.
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
        _ => panic!("unsupported reference projection: {form:?}"),
    }
}

#[test]
fn retained_local_declarations_supply_reference_ast_fields_without_reexecution() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/local-reference-ast-observations.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 6);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(
                r#"(defmacro observe-reference [label binding]
          (let [ast (get (get (get &env :locals) binding) :init)
                info (get ast :info)]
            (list 'quote [label
              [["op" (contains? ast :op) (get ast :op)]
               ["local" (contains? ast :local) (get ast :local)]
               ["arg-id" (contains? ast :arg-id) (get ast :arg-id)]
               ["variadic?" (contains? ast :variadic?) (get ast :variadic?)]]
              [(= info (get (get &env :locals) (get ast :form)))
               (= (get ast :name) (get info :name))
               (= (contains? ast :init) (contains? info :init))
               (= (get ast :init) (get info :init))
               (contains? ast :val) (contains? ast :children)
               (identical? info (get (get &env :locals) (get ast :form)))
               (identical? (get ast :init) (get info :init))]])))"#,
            )
            .unwrap();
        let value = session
            .eval_with_macros(
                r#"[
          (let [x 1 copy x] (observe-reference "let" copy))
          (let [x 1] (let [x false copy x] (observe-reference "shadow" copy)))
          (loop [x 1] (let [copy x] (observe-reference "loop" copy)))
          ((fn [x] (let [copy x] (observe-reference "arg" copy))) 42)
          ((fn [x & more] (let [copy more] (observe-reference "rest" copy))) 1 2)
          ((fn self [x] (let [copy self] (observe-reference "fn" copy))) 1)]"#,
                &mut macros,
            )
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let mut expected = corpus["cases"].clone();
        expected[3][1][2][2] = json!(0.0);
        expected[4][1][2][2] = json!(1.0);
        assert_eq!(
            data(&bridge.read(&mut session, &value, 0..1).unwrap()),
            expected
        );
        let value = session
            .eval_with_macros(
                r#"(def effects 0)
          (let [x (do (set! effects (+ effects 1)) false) copy x]
            [(observe-reference "let" copy) x effects])"#,
                &mut macros,
            )
            .unwrap();
        session.collect().unwrap();
        let actual = data(&bridge.read(&mut session, &value, 0..1).unwrap());
        assert_eq!(actual[0], expected[0]);
        assert_eq!(actual[1], json!(false));
        assert_eq!(
            actual[2],
            json!(1.0),
            "inspection must not rerun a retained initializer"
        );
        // A local callee is a reference within an invocation. The invocation
        // initializer itself must not acquire the callee's local AST fields.
        let value = session
            .eval_with_macros(
                r#"(def call-effects 0)
          (let [f (fn [] (do (set! call-effects (+ call-effects 1)) 42))
                copy (f)]
            [(observe-reference "invoke" copy) copy call-effects])"#,
                &mut macros,
            )
            .unwrap();
        session.collect().unwrap();
        let actual = data(&bridge.read(&mut session, &value, 0..1).unwrap());
        assert_eq!(
            actual[0][1],
            json!([
                ["op", false, null],
                ["local", false, null],
                ["arg-id", false, null],
                ["variadic?", false, null]
            ]),
            "invocation schema is still unfinished; do not classify a list as its local head"
        );
        assert_eq!(
            actual[0][2][4],
            json!(false),
            "no fabricated constant value"
        );
        assert_eq!(actual[1], json!(42.0));
        assert_eq!(actual[2], json!(1.0), "invocation executes exactly once");
    }
}

#[test]
fn reference_asts_keep_initializer_scope_metadata_and_variadic_self_identity() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(
                r#"(defmacro inspect-reference [binding]
          (let [ast (get (get &env :locals) binding :missing)
                ast (get ast :init)
                info (get ast :info)
                original (get (get (get ast :env) :locals) (get ast :form))
                current (get (get &env :locals) (get ast :form))]
            (list 'quote [(get ast :op) (get ast :local)
              (identical? info original) (identical? info current)
              (get (meta (get ast :name)) :review)
              (get (meta (get ast :form)) :review)
              (get (get ast :init) :val)
              (get ast :variadic?)
              (identical? (get ast :init) (get info :init))])))"#,
            )
            .unwrap();
        let value = session
            .eval_with_macros(
                r#"[
          (let [^{:review "declaration"} x 17
                copy ^{:review "reference"} x
                x false]
            (inspect-reference copy))
          ((fn self [x & rest]
             (let [copy self] (inspect-reference copy))) 1 2)
          (try (throw 42) (catch :default thrown
             (let [copy thrown] (inspect-reference copy))))]"#,
                &mut macros,
            )
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let actual = data(&bridge.read(&mut session, &value, 0..1).unwrap());
        assert_eq!(
            actual[0],
            json!([
                ["keyword", ":local"],
                ["keyword", ":let"],
                true,
                false,
                "declaration",
                "reference",
                17.0,
                null,
                true
            ])
        );
        assert_eq!(
            actual[1],
            json!([
                ["keyword", ":local"],
                ["keyword", ":fn"],
                true,
                true,
                null,
                null,
                null,
                true,
                true
            ])
        );
        assert_eq!(
            actual[2],
            json!([
                ["keyword", ":local"],
                ["keyword", ":let"],
                true,
                true,
                null,
                null,
                null,
                null,
                true
            ])
        );
    }
}
