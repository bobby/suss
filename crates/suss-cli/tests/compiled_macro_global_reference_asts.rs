//! Global-reference AST facts come from retained declaration revisions.
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
        Kind::Symbol(value) => json!(["symbol", value.to_string()]),
        Kind::Keyword(value) => json!([
            "keyword",
            value.namespace.as_ref().map_or_else(
                || format!(":{}", value.name),
                |ns| format!(":{ns}/{}", value.name)
            )
        ]),
        Kind::Vector(values) => json!(values.iter().map(data).collect::<Vec<_>>()),
        _ => panic!("unsupported global reference projection: {form:?}"),
    }
}

#[test]
fn captured_global_declarations_supply_var_ast_fields_without_catalog_mutation() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/global-reference-ast-observations.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 11);
    let mut expected = corpus["cases"].clone();
    // Accepted design3: suss.core is canonical and cljs.core shares bindings.
    // Change only the two exact core namespace/name spellings in expected
    // resolved records. Every other upstream value/presence bit stays exact.
    for row in &mut expected.as_array_mut().unwrap()[7..9] {
        for record in [1, 2] {
            row[record][1][2] = json!(["symbol", "suss.core/identity"]);
            row[record][2][2] = json!(["symbol", "suss.core"]);
        }
    }
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        session
            .enter_namespace("suss-oracle.global-reference-ast-runner")
            .unwrap();
        macros
            .enter_namespace("suss-oracle.global-reference-ast-runner")
            .unwrap();
        macros
            .define(
                r#"(defmacro observe-global [label binding catalog-name]
          (let [ast (get (get (get &env :locals) binding) :init)
                info (get ast :info)
                catalog (get (get (get (get ast :env) :ns) :defs)
                             catalog-name)
                field (fn [record key]
                        [(name key) (contains? record key) (get record key)])]
            (list 'quote [label
              [(field ast :op) (field ast :name) (field ast :ns) (field ast :tag)]
              [(field info :op) (field info :name) (field info :ns) (field info :tag) (field info :doc) (field info :declared) (field info :dynamic) (field info :fn-var) (field info :ret-tag)]
              [(field catalog :op) (field catalog :ns)]
              [(contains? ast :val) (contains? ast :children)
               (= (get ast :name) (get info :name))
               (= (get ast :ns) (get info :ns))]])))"#,
            )
            .unwrap();
        let value = session.eval_with_macros(r#"
          (def effects 0)
          (def ^{:doc "source declaration"} scalar
            (do (set! effects (+ effects 1)) 42))
          (def ^string hinted 1)
          (def ^:dynamic *dynamic* 7)
          (def fixed (fn [] 42))
          (declare pending)
          (def ^{:name forged :ns wrong :doc "raw metadata" :tag number} overridden 3)
          (def results [
            (let [copy scalar] (observe-global "scalar" copy scalar))
            (let [copy suss-oracle.global-reference-ast-runner/scalar] (observe-global "qualified" copy scalar))
            (let [copy hinted] (observe-global "hint" copy hinted))
            (let [copy *dynamic*] (observe-global "dynamic" copy *dynamic*))
            (let [copy fixed] (observe-global "function" copy fixed))
            (let [copy pending] (observe-global "declared" copy pending))
            (let [copy overridden] (observe-global "raw-metadata" copy overridden))
            (let [copy identity] (observe-global "core" copy identity))
            (let [copy cljs.core/identity] (observe-global "core-alias" copy identity))
            (let [copy scalar changed (def ^{:doc "changed declaration"} scalar false)]
              (observe-global "revision-before" copy scalar))
            (let [copy scalar] (observe-global "revision-after" copy scalar))])
          results"#, &mut macros).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let actual = data(&bridge.read(&mut session, &value, 0..1).unwrap());
        let actual = actual.as_array().unwrap();
        let expected = expected.as_array().unwrap();
        assert_eq!(actual.len(), expected.len());
        for (row, expected) in actual.iter().zip(expected) {
            assert_eq!(row, expected, "source-global case {}", expected[0]);
        }
        // Inspection of the old revision cannot replay its definition or read
        // the new runtime value to fabricate its source type/declaration.
        let live = session.eval("scalar").unwrap();
        session.collect().unwrap();
        assert_eq!(
            data(&bridge.read(&mut session, &live, 0..1).unwrap()),
            json!(false)
        );
        let effects = session.eval("effects").unwrap();
        session.collect().unwrap();
        assert_eq!(
            data(&bridge.read(&mut session, &effects, 0..1).unwrap()),
            json!(1.0),
            "inspection must not replay the captured declaration initializer"
        );
        macros
            .define(
                r#"(defmacro global-call-shape [binding]
          (let [ast (get (get (get &env :locals) binding) :init)]
            (list 'quote [(= (get ast :op) :var)
                          (contains? ast :val)
                          (contains? ast :name) (contains? ast :ns)])))"#,
            )
            .unwrap();
        let value = session
            .eval_with_macros(
                r#"(def call-effects 0)
          (def read-global (fn [] (do (set! call-effects (+ call-effects 1)) 42)))
          (let [copy (read-global)] [(global-call-shape copy) copy call-effects])"#,
                &mut macros,
            )
            .unwrap();
        session.collect().unwrap();
        let actual = data(&bridge.read(&mut session, &value, 0..1).unwrap());
        assert_eq!(
            actual[0],
            json!([false, false, false, false]),
            "an invocation must not inherit the global callee's var identity or a fabricated constant value"
        );
        assert_eq!(actual[1], json!(42.0));
        assert_eq!(actual[2], json!(1.0), "global call executes exactly once");
    }
}

#[test]
fn resolved_info_copies_identity_fields_and_shares_captured_nested_metadata() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        session.enter_namespace("review-global").unwrap();
        macros.enter_namespace("review-global").unwrap();
        macros
            .define(
                r#"(defmacro inspect-copy [a b]
          (let [left (get (get (get &env :locals) a) :init)
                right (get (get (get &env :locals) b) :init)
                info (get left :info)
                old (get (get (get (get left :env) :ns) :defs) 'value)
                current (get (get (get &env :ns) :defs) 'value)]
            (list 'quote [(identical? info (get right :info))
              (identical? info old)
              (identical? (get info :meta) (get old :meta))
              (identical? (get info :meta) (get current :meta))
              (get info :doc) (get old :ns) (get current :doc)
              (= (get info :name) 'review-global/value)
              (= (get info :ns) 'review-global)
              (contains? old :op)])))"#,
            )
            .unwrap();
        let value = session
            .eval_with_macros(
                r#"
          (def ^{:doc "before" :ns raw} value 17)
          (let [a value b value changed (def ^{:doc "after"} value false)]
            [(inspect-copy a b) value])"#,
                &mut macros,
            )
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        assert_eq!(
            data(&bridge.read(&mut session, &value, 0..1).unwrap()),
            json!([
                [
                    false,
                    false,
                    true,
                    true,
                    "before",
                    ["symbol", "raw"],
                    "before",
                    true,
                    true,
                    false
                ],
                false
            ])
        );
        let overlaid = session
            .eval_with_macros(
                r#"(def ^{:top-fn {:meta {:custom true}}} value (fn [] 17))
                    (let [a value b value] (inspect-copy a b))"#,
                &mut macros,
            )
            .unwrap();
        session.collect().unwrap();
        assert_eq!(
            data(&bridge.read(&mut session, &overlaid, 0..1).unwrap()),
            json!([
                false, false, true, true, null, null, null, true, true, false
            ]),
            "resolved copies share the final top-fn metadata overlay"
        );
    }
}
