use serde_json::{json, Value};
use suss_cli::{
    portable_macro_data::FormBridge, portable_macros::CompiledMacros, portable_session::Session,
};
use suss_reader::forms::{Form, Kind};

// Lossless projection for this corpus. Reader locations and symbol metadata are
// intentionally outside its scope. Unknown data kinds fail rather than match.
fn data(form: &Form) -> Value {
    match &form.kind {
        Kind::Nil => Value::Null,
        Kind::Bool(value) => json!(value),
        Kind::Number(value) => json!(value),
        Kind::String(value) => json!(String::from_utf16(value).unwrap()),
        Kind::Symbol(symbol) => json!([
            "symbol",
            symbol
                .namespace
                .as_ref()
                .map_or_else(|| symbol.name.clone(), |ns| format!("{ns}/{}", symbol.name))
        ]),
        Kind::Vector(items) => json!(["vector", items.iter().map(data).collect::<Vec<_>>()]),
        Kind::Set(items) => {
            let mut items = items.iter().map(data).collect::<Vec<_>>();
            items.sort_by_key(Value::to_string);
            json!(["set", items])
        }
        _ => panic!("unsupported decoded source inference projection: {form:?}"),
    }
}
fn canonical(mut value: Value) -> Value {
    if let Value::Number(number) = &value {
        // Ordinary Suss numbers are binary64. Booleans remain a distinct kind.
        return json!(number.as_f64().unwrap());
    }
    if let Value::Array(items) = &mut value {
        for item in items.iter_mut() {
            *item = canonical(item.take());
        }
        if items.len() == 2 && items.first() == Some(&json!("set")) {
            items[1]
                .as_array_mut()
                .unwrap()
                .sort_by_key(Value::to_string);
        }
    }
    value
}

#[test]
fn compiled_source_inference_matches_pinned_tags_without_changing_executed_storage_or_effects() {
    let expressions = [
        ("nil", "nil"),
        ("boolean", "false"),
        ("number", "42"),
        ("string", "\"hello\""),
        ("keyword", ":word"),
        ("quoted-symbol", "'word"),
        ("vector", "[1 2]"),
        ("map", "{:a 1}"),
        ("set", "#{1 2}"),
        ("quoted-list", "'(1 2)"),
        ("empty-list", "'()"),
        ("arithmetic-string-storage", "(+ \"a\" 1)"),
        ("do", "(do 1 \"last\")"),
        ("let", "(let [x 7] x)"),
        ("loop", "(loop [x 7] x)"),
        ("if-constant-true", "(if true 1 \"s\")"),
        ("if-constant-false", "(if false 1 \"s\")"),
        ("if-unknown-union", "(if *dynamic* 1 \"s\")"),
        ("if-unknown-same", "(if *dynamic* 1 2)"),
        ("try-body", "(try 1 (catch :default problem \"caught\"))"),
        ("var-number", "scalar"),
        ("var-function", "fixed"),
        ("dynamic-number", "*dynamic*"),
        ("invoke-unknown-return", "(fixed 9)"),
        ("invoke-inferred-number-with-string-storage", "(numeric)"),
        ("fn-number", "(fn [] 42)"),
        ("fn-unknown", "(fn [x] x)"),
        ("fn-parameter-hint", "(fn [^number x] x)"),
        ("fn-mixed-methods", "(fn ([] 1) ([x] \"s\"))"),
        ("local-number", "(let [x 1] x)"),
        ("local-hint", "(let [^string x 1] x)"),
        ("local-any", "(let [x *dynamic*] x)"),
    ];
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/analysis-tag-observations.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), expressions.len());
    let expected = cases
        .iter()
        .zip(expressions)
        .map(|(case, (label, _))| {
            assert_eq!(case[0], label);
            let fields = case[1].as_array().unwrap();
            let tag = fields.iter().find(|field| field[0] == "tag").unwrap();
            let ret = fields
                .iter()
                .find(|field| field[0] == "inferred-ret-tag")
                .unwrap();
            canonical(json!(["vector", [label, tag[1], tag[2], ret[1], ret[2]]]))
        })
        .collect::<Vec<_>>();
    let observed = expressions
        .iter()
        .map(|(label, expression)| {
            format!("(let [observed {expression}] (observe-tags \"{label}\" observed))")
        })
        .collect::<Vec<_>>()
        .join("\n");
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(
                r#"(defmacro observe-tags [label name]
          (let [init (get (get (get &env :locals) name) :init)
                row [label (contains? init :tag) (get init :tag)
                     (contains? init :inferred-ret-tag) (get init :inferred-ret-tag)]]
            (list 'do (list 'set! 'facts (list 'conj 'facts (list 'quote row))) name)))"#,
            )
            .unwrap();
        let source = format!(
            r#"
          (def facts [])
          (def scalar 7)
          (def fixed (fn [x] x))
          (def numeric (fn [] (+ "a" 1)))
          (def ^:dynamic *dynamic* 8)
          (def observed-results [{observed}])
          facts"#
        );
        let value = session.eval_with_macros(&source, &mut macros).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let actual = bridge.read(&mut session, &value, 0..1).unwrap();
        let actual = data(&actual);
        assert_eq!(actual[0], "vector");
        let rows = actual[1].as_array().unwrap();
        assert_eq!(
            rows.len(),
            expected.len(),
            "every source inference observation is required"
        );
        for ((actual, expected), (label, _)) in rows.iter().zip(&expected).zip(expressions) {
            assert_eq!(actual, expected, "native source tags after GC: {label}");
        }
        let runtime = session
            .eval(
                r#"[(count observed-results)
          (nth observed-results 11) (nth observed-results 24)
          ((nth observed-results 25)) ((nth observed-results 26) 10)
          ((nth observed-results 27) 11) ((nth observed-results 28))
          ((nth observed-results 28) 12) (nth observed-results 29)
          (nth observed-results 30) (nth observed-results 31)]"#,
            )
            .unwrap();
        session.collect().unwrap();
        let runtime = bridge.read(&mut session, &runtime, 0..1).unwrap();
        assert_eq!(
            data(&runtime),
            canonical(json!(["vector", corpus["result"]])),
            "actual runtime projections retain storage semantics"
        );
        let count = session.eval("(count facts)").unwrap();
        assert!(matches!(
            bridge.read(&mut session, &count, 0..1).unwrap().kind,
            Kind::Number(32.0)
        ));
        macros
            .define(
                r#"(defmacro effect-tag [name]
          (let [tag (get (get (get (get &env :locals) name) :init) :tag)]
            [(list 'quote tag) name 'effects]))"#,
            )
            .unwrap();
        let effect = session
            .eval_with_macros(
                r#"(def effects 0)
          (let [observed (do (set! effects (+ effects 1)) (+ "a" 1))]
            (effect-tag observed))"#,
                &mut macros,
            )
            .unwrap();
        session.collect().unwrap();
        let effect = bridge.read(&mut session, &effect, 0..1).unwrap();
        assert_eq!(
            data(&effect),
            canonical(json!(["vector", [["symbol", "number"], "a1", 1]])),
            "inferred number and executed string stay distinct; initializer effects run once"
        );
    }
}

#[test]
fn compiled_source_invocation_tags_distinguish_global_aggregate_and_local_methods() {
    // Pinned analyzer.cljc retains aggregate :ret-tag on global function vars;
    // direct and local source functions retain individual method tags instead.
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros.define(r#"(defmacro invocation-tag [name]
          (list 'quote (get (get (get (get &env :locals) name) :init) :tag)))"#).unwrap();
        let value = session.eval_with_macros(r#"
          (def mixed (fn ([] 1) ([x] "s")))
          [(let [observed (mixed)] (invocation-tag observed))
           (let [observed (mixed 2)] (invocation-tag observed))
           (let [f (fn ([] 1) ([x] "s")) observed (f)] (invocation-tag observed))
           (let [f (fn ([] 1) ([x] "s")) observed (f 2)] (invocation-tag observed))
           (let [observed ((fn ([] 1) ([x] "s")))] (invocation-tag observed))
           (mixed) (mixed 2)]"#, &mut macros).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let value = bridge.read(&mut session, &value, 0..1).unwrap();
        assert_eq!(data(&value), canonical(json!(["vector", [
            ["symbol", "any"], ["symbol", "any"], ["symbol", "number"],
            ["symbol", "string"], ["symbol", "number"], 1, "s"
        ]])));
    }
}
