//! Retained pinned identifier conversion and its complete Var dependency.
#![cfg(not(target_family = "wasm"))]
#[path = "support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};
use suss_compile::portable_session::Session;
use suss_reader::forms::{Form, Kind, read_forms};

fn expected(form: &Form) -> Observation {
    match &form.kind {
        Kind::Nil => Observation::Nil,
        Kind::Bool(value) => Observation::Bool(*value),
        Kind::Number(value) => Observation::Number(value.to_bits()),
        Kind::String(units) => Observation::String(units.clone()),
        Kind::Symbol(value) => Observation::Symbol(
            value.namespace.as_ref().map(|s| s.encode_utf16().collect()),
            value.name.encode_utf16().collect(),
        ),
        Kind::Keyword(value) => Observation::Keyword(
            value.namespace.as_ref().map(|s| s.encode_utf16().collect()),
            value.name.encode_utf16().collect(),
        ),
        Kind::Vector(items) => Observation::Vector(items.iter().map(expected).collect()),
        Kind::Map(items) => Observation::Map(
            items
                .chunks_exact(2)
                .map(|pair| (expected(&pair[0]), expected(&pair[1])))
                .collect(),
        ),
        _ => panic!("Unsupported expected fixture: {form:?}"),
    }
}

fn check(session: &mut Session, decoder: &mut Decoder, source: &str, expectation: &str) {
    // The portable reader preserves empty strings and lone UTF-16 surrogates;
    // the prototype EDN parser cannot represent these observation fixtures.
    let forms = read_forms(expectation).unwrap();
    assert_eq!(forms.len(), 1);
    let expectation = expected(&forms[0]);
    let value = session
        .eval(source)
        .unwrap_or_else(|e| panic!("{source}: {e}"));
    session.collect().unwrap();
    let actual = decoder.decode_session(session, &value).unwrap();
    assert_eq!(actual, expectation, "{source}");
}

#[test]
fn retained_symbol_conversion_preserves_all_branches_and_utf16_in_both_phases() {
    for macro_phase in [false, true] {
        let mut session = if macro_phase {
            Session::new_macro().unwrap()
        } else {
            Session::new_repl().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
        for (source, expected) in [
            (r#"(symbol "c/d")"#, "c/d"),
            (r#"(symbol "plain")"#, "plain"),
            (r#"(symbol "/")"#, "/"),
            (
                r#"(let [s (symbol "c/d/e")] [(.-ns s) (.-name s)])"#,
                r#"["c" "d/e"]"#,
            ),
            (
                r#"[(.-ns (symbol "")) (.-name (symbol ""))]"#,
                r#"[nil ""]"#,
            ),
            (
                r#"[(.-ns (symbol "c/")) (.-name (symbol "c/"))]"#,
                r#"["c" ""]"#,
            ),
            (
                r#"[(.-ns (symbol "/name")) (.-name (symbol "/name"))]"#,
                r#"[nil "/name"]"#,
            ),
            (
                r#"[(.-ns (symbol "😀/\uD800x\uDC00")) (.-name (symbol "😀/\uD800x\uDC00"))]"#,
                r#"["😀" "\uD800x\uDC00"]"#,
            ),
            ("(symbol :a/b)", "a/b"),
            ("(symbol :plain)", "plain"),
            ("(let [s 'a/b] (identical? s (symbol s)))", "true"),
            (r#"(symbol "a" "b")"#, "a/b"),
            (r#"(symbol nil "b")"#, "b"),
            ("(symbol (new suss.core/Var (fn [] 42) 'a/b nil))", "a/b"),
            (
                "[(var? 'a/b) (var? nil) (var? (new suss.core/Var (fn [] 42) 'a/b nil))]",
                "[false false true]",
            ),
            (
                "(try (symbol 42) (catch :default e (.-message e)))",
                r#""no conversion to symbol""#,
            ),
            (
                "(try (symbol nil) (catch :default e (.-message e)))",
                r#""no conversion to symbol""#,
            ),
        ] {
            check(&mut session, &mut decoder, source, expected);
        }
    }
}

#[test]
fn retained_var_keeps_thunk_lookup_metadata_equality_hash_and_invocation_arities() {
    for macro_phase in [false, true] {
        let mut session = if macro_phase {
            Session::new_macro().unwrap()
        } else {
            Session::new_repl().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
        check(
            &mut session,
            &mut decoder,
            "(let [a (atom 1) v (new suss.core/Var (fn [] @a) 'a/b {:m 17}) w (with-meta v {:m 19})] (reset! a 42) [@v @w (meta v) (meta w) (= v w) (= (hash v) (hash w))])",
            "[42 42 {:m 17} {:m 19} true true]",
        );
        check(
            &mut session,
            &mut decoder,
            "(let [target (fn [] 42)] (set! (.-cljs$lang$macro target) true) (.isMacro (new suss.core/Var (fn [] target) 'a/b nil)))",
            "true",
        );
        for arity in 0..=20 {
            let parameters = (0..arity)
                .map(|i| format!("a{i}"))
                .collect::<Vec<_>>()
                .join(" ");
            let arguments = (0..arity)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            let source = format!(
                "(let [v (new suss.core/Var (fn [] (fn [{parameters}] {arity})) 'a/b nil)] (v {arguments}))"
            );
            check(&mut session, &mut decoder, &source, &arity.to_string());
        }
        check(
            &mut session,
            &mut decoder,
            "(let [v (new suss.core/Var (fn [] (fn [& args] (count args))) 'a/b nil)] (apply v [0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22]))",
            "23",
        );
    }
}

#[test]
fn private_utf16_storage_handles_empty_search_surrogates_and_invalid_bounds() {
    let mut session = Session::new_repl().unwrap();
    let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
    for (source, expected) in [
        (r#"(suss.bootstrap/string-index-of "😀/\uD800x" "/")"#, "2"),
        (r#"(suss.bootstrap/string-index-of "ababa" "aba")"#, "0"),
        (r#"(suss.bootstrap/string-index-of "abc" "")"#, "0"),
        (r#"(suss.bootstrap/string-index-of "" "a")"#, "-1"),
        (r#"(suss.bootstrap/string-index-of "abc" "abcd")"#, "-1"),
        (r#"(suss.bootstrap/string-index-of "abc" "d")"#, "-1"),
        (
            r#"(suss.bootstrap/string-slice "😀/\uD800x" 1 4)"#,
            r#""\uDE00/\uD800""#,
        ),
        (r#"(suss.bootstrap/string-slice "abc" 3 3)"#, r#""""#),
        (
            r#"(try (suss.bootstrap/string-slice "abc" -1 2) (catch :default e 42))"#,
            "42",
        ),
        (
            r#"(try (suss.bootstrap/string-slice "abc" 0 4) (catch :default e 42))"#,
            "42",
        ),
        (
            r#"(try (suss.bootstrap/string-slice "abc" 2 1) (catch :default e 42))"#,
            "42",
        ),
        (
            r#"(try (suss.bootstrap/string-slice "abc" 0.5 2) (catch :default e 42))"#,
            "42",
        ),
        (
            r#"(try (suss.bootstrap/string-slice "abc" ##NaN 2) (catch :default e 42))"#,
            "42",
        ),
        (
            r#"(try (suss.bootstrap/string-slice "abc" 0 ##Inf) (catch :default e 42))"#,
            "42",
        ),
        (
            r#"(try (suss.bootstrap/string-index-of 17 "/") (catch :default e 42))"#,
            "42",
        ),
        (
            r#"(try (suss.bootstrap/string-index-of "abc" 17) (catch :default e 42))"#,
            "42",
        ),
        (
            r#"(let [a (atom [])] [(suss.bootstrap/string-slice (do (swap! a conj :receiver) "abc") (do (swap! a conj :start) 0) (do (swap! a conj :end) 2)) @a])"#,
            r#"["ab" [:receiver :start :end]]"#,
        ),
    ] {
        check(&mut session, &mut decoder, source, expected);
    }
}

#[test]
fn retained_identifier_dependency_matches_shared_pinned_corpus_in_both_phases() {
    fn tagged(value: &Observation) -> serde_json::Value {
        use serde_json::json;
        match value {
            Observation::Nil => json!({"tag": "nil"}),
            Observation::Bool(value) => json!({"tag": "bool", "value": value}),
            Observation::Number(bits) => json!({"tag": "f64", "bits": format!("{bits:016x}")}),
            Observation::String(units) => json!({"tag": "string", "units": units}),
            Observation::Vector(items) => {
                json!({"tag": "vector", "items": items.iter().map(tagged).collect::<Vec<_>>()})
            }
            other => panic!("Unexpected primary corpus observation: {other:?}"),
        }
    }
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/symbol-conversion-cases.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 40);
    for macro_phase in [false, true] {
        let mut session = if macro_phase {
            Session::new_macro().unwrap()
        } else {
            Session::new_repl().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
        for case in cases {
            let value = session
                .eval(case["source"].as_str().unwrap())
                .unwrap_or_else(|e| panic!("phase={macro_phase} {}: {e}", case["id"]));
            session.collect().unwrap();
            let actual = decoder.decode_session(&mut session, &value).unwrap();
            assert_eq!(
                tagged(&actual),
                case["expected"],
                "phase={macro_phase} {}",
                case["id"]
            );
        }
    }
}
