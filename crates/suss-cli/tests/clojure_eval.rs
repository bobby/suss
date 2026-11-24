//! Public eval must execute form data through the persistent compiled host.
use suss_compile::{portable_macros::CompiledMacros, portable_session::Session};
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};
use suss_reader::forms::{ExactValue, Form, Kind};

fn rows() -> Vec<serde_json::Value> {
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../../../results/m12-eval/primary/cases.json")).unwrap();
    assert_eq!(rows.len(), 24);
    rows
}
fn expected(form: &Form) -> Observation {
    match &form.kind {
        Kind::Nil => Observation::Nil,
        Kind::Bool(v) => Observation::Bool(*v),
        Kind::Long(v) => Observation::Long(*v),
        Kind::Number(v) => Observation::Number(v.to_bits()),
        Kind::Exact(v) => match &v.value {
            ExactValue::Integer { value, big: true } => Observation::BigInt(value.to_string()),
            ExactValue::Ratio {
                numerator,
                denominator,
            } => Observation::Ratio(numerator.to_string(), denominator.to_string()),
            ExactValue::Decimal { unscaled, scale } => Observation::Decimal {
                unscaled: unscaled.to_string(),
                scale: *scale,
            },
            other => panic!("unassessed exact primary {other:?}"),
        },
        Kind::String(v) => Observation::String(v.clone()),
        Kind::Keyword(v) => Observation::Keyword(
            v.namespace.as_ref().map(|s| s.encode_utf16().collect()),
            v.name.encode_utf16().collect(),
        ),
        Kind::Symbol(v) => Observation::Symbol(
            v.namespace.as_ref().map(|s| s.encode_utf16().collect()),
            v.name.encode_utf16().collect(),
        ),
        Kind::Vector(v) => Observation::Vector(v.iter().map(expected).collect()),
        Kind::List(v) => Observation::List(v.iter().map(expected).collect()),
        Kind::Set(v) => Observation::Set(v.iter().map(expected).collect()),
        Kind::Map(v) => {
            assert_eq!(v.len() % 2, 0);
            Observation::Map(
                v.chunks_exact(2)
                    .map(|pair| (expected(&pair[0]), expected(&pair[1])))
                    .collect(),
            )
        }
        other => panic!("unassessed primary eval value {other:?}"),
    }
}
// Set/map iteration order is not equality. Normalize only host observations;
// sequence order, scalar categories, floating bits and Decimal scale stay exact.
fn canonical(value: Observation) -> Observation {
    match value {
        Observation::Vector(v) => Observation::Vector(v.into_iter().map(canonical).collect()),
        Observation::List(v) => Observation::List(v.into_iter().map(canonical).collect()),
        Observation::Set(v) => {
            let mut v: Vec<_> = v.into_iter().map(canonical).collect();
            v.sort_by_cached_key(|value| format!("{value:?}"));
            Observation::Set(v)
        }
        Observation::Map(v) => {
            let mut v: Vec<_> = v
                .into_iter()
                .map(|(k, v)| (canonical(k), canonical(v)))
                .collect();
            v.sort_by_cached_key(|(key, _)| format!("{key:?}"));
            Observation::Map(v)
        }
        other => other,
    }
}
const INDEPENDENT: &[&str] = &[
    "numeric-literals",
    "quoted-numeric-application",
    "self-evaluating-values",
    "empty-list",
    "collection-values",
    "compiled-function",
    "existing-function-identity",
    "function-symbol",
    "lexical-locals-are-not-captured",
    "macro-and-loop",
    "recursive-eval",
    "lazy-result-not-realized",
    "dynamic-math-context",
    "wrong-arity-evaluates-arguments",
    "generated-data-is-compiled",
];
fn run(fixture: bool) {
    let selected: Vec<_> = rows()
        .into_iter()
        .filter(|row| {
            let independent = INDEPENDENT.contains(&row["name"].as_str().unwrap());
            independent != fixture
        })
        .collect();
    assert_eq!(selected.len(), if fixture { 9 } else { 15 });
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        session.enter_namespace("user").unwrap();
        macros.enter_namespace("user").unwrap();
        if fixture {
            // Construct equivalent fixture state through ordinary namespace and
            // definition forms. No eval implementation or result is injected.
            suss_compile::portable_repl::evaluate_script_compiled(
                &mut session,
                &mut macros,
                "(ns eval-fixture) (def value 73) (ns user)
                 (def eval-fixture-value 42) (def eval-fixture-effects (atom []))
                 (def eval-existing 99)",
                None,
            )
            .unwrap();
            macros
                .define("(defmacro eval-fixture-macro [] (list 'quote :expanded))")
                .unwrap();
        }
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        let mut retained = Vec::new();
        for row in &selected {
            let expression = row["expression"]
                .as_str()
                .unwrap()
                .replace("(catch Throwable ", "(catch :default ");
            let value = session
                .eval_with_macros(&expression, &mut macros)
                .unwrap_or_else(|error| {
                    panic!(
                        "{}: {}",
                        row["name"],
                        suss_compile::portable_repl::error_display(&mut session, &error)
                    )
                });
            // Check each unchanged control before a later unsupported control
            // can abort this group. Keep the original retained post-GC checks.
            let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
            assert_eq!(forms.len(), 1);
            decoder.refill(20_000);
            assert_eq!(
                canonical(decoder.decode_session(&mut session, &value).unwrap()),
                canonical(expected(&forms[0])),
                "{}",
                row["name"]
            );
            eprintln!("eval-primary-match {:?} {}", session.phase(), row["name"]);
            retained.push(value);
        }
        session.collect().unwrap();
        for (value, row) in retained.iter().zip(&selected) {
            let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
            assert_eq!(forms.len(), 1);
            decoder.refill(20_000);
            assert_eq!(
                canonical(decoder.decode_session(&mut session, value).unwrap()),
                canonical(expected(&forms[0])),
                "{}",
                row["name"]
            );
        }
    }
}
#[test]
fn compiled_eval_handles_literals_generated_calls_functions_and_dynamic_scope() {
    run(false);
}
#[test]
fn compiled_eval_shares_real_namespace_vars_macros_and_effect_boundaries() {
    run(true);
}
#[test]
fn compiled_eval_is_reentrant_inside_actual_compiled_macro_functions() {
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    macros.define("(defmacro eval-local [] 7)").unwrap();
    macros
        .define("(defmacro eval-number [] (eval '(+ 1N 1/2)))")
        .unwrap();
    macros
        .define("(defmacro eval-function [] (let [f (eval '(fn [x] (* x 2)))] (f 21)))")
        .unwrap();
    macros
        .define("(defmacro eval-macro [] (eval '(eval-local)))")
        .unwrap();
    let value = session
        .eval_with_macros("[(eval-number) (eval-function) (eval-macro)]", &mut macros)
        .unwrap();
    let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
    session.collect().unwrap();
    assert_eq!(
        decoder.decode_session(&mut session, &value).unwrap(),
        Observation::Vector(vec![
            Observation::Ratio("3".into(), "2".into()),
            Observation::Long(42),
            Observation::Long(7),
        ])
    );
}

#[test]
fn vars_retain_real_live_cells_identity_metadata_and_callability() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/live-vars-primary/cases.json"
    ))
    .unwrap();
    assert_eq!(rows.len(), 10);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        session.enter_namespace("user").unwrap();
        macros.enter_namespace("user").unwrap();
        suss_compile::portable_repl::evaluate_script_compiled(
            &mut session,
            &mut macros,
            include_str!("../../../results/m12-eval/live-vars-primary/setup.sus"),
            None,
        )
        .unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        let mut retained = Vec::new();
        for row in &rows {
            let expression = row["expression"]
                .as_str()
                .unwrap()
                .replace("(catch Throwable ", "(catch :default ");
            let value = session
                .eval_with_macros(&expression, &mut macros)
                .unwrap_or_else(|error| {
                    panic!(
                        "{}: {}",
                        row["name"],
                        suss_compile::portable_repl::error_display(&mut session, &error)
                    )
                });
            let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
            assert_eq!(forms.len(), 1);
            decoder.refill(20_000);
            assert_eq!(
                canonical(decoder.decode_session(&mut session, &value).unwrap()),
                canonical(expected(&forms[0])),
                "{}",
                row["name"]
            );
            eprintln!("var-primary-match {:?} {}", session.phase(), row["name"]);
            retained.push(value);
        }
        session.collect().unwrap();
        for (row, value) in rows.iter().zip(&retained) {
            let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
            decoder.refill(20_000);
            assert_eq!(
                canonical(decoder.decode_session(&mut session, value).unwrap()),
                canonical(expected(&forms[0])),
                "{} after GC",
                row["name"]
            );
        }
    }
}

#[test]
fn eval_preserves_primary_top_level_do_compilation_and_effect_boundaries() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/do-boundary-primary/cases.json"
    ))
    .unwrap();
    let definitions: Vec<String> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/do-boundary-primary/macros.json"
    ))
    .unwrap();
    assert_eq!(rows.len(), 11);
    assert_eq!(definitions.len(), 5);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        session.enter_namespace("user").unwrap();
        macros.enter_namespace("user").unwrap();
        suss_compile::portable_repl::evaluate_script_compiled(
            &mut session,
            &mut macros,
            include_str!("../../../results/m12-eval/do-boundary-primary/setup.sus"),
            None,
        )
        .unwrap();
        for definition in &definitions {
            macros
                .define(&definition.replace("(catch Throwable ", "(catch :default "))
                .unwrap();
        }
        session
            .eval_with_macros("(eval-do-init-counter)", &mut macros)
            .unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        let mut retained = Vec::new();
        for row in &rows {
            let expression = row["expression"]
                .as_str()
                .unwrap()
                .replace("(catch Throwable ", "(catch :default ");
            let value = session
                .eval_with_macros(&expression, &mut macros)
                .unwrap_or_else(|error| {
                    panic!(
                        "{}: {}",
                        row["name"],
                        suss_compile::portable_repl::error_display(&mut session, &error)
                    )
                });
            let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
            assert_eq!(forms.len(), 1);
            decoder.refill(20_000);
            assert_eq!(
                canonical(decoder.decode_session(&mut session, &value).unwrap()),
                canonical(expected(&forms[0])),
                "{}",
                row["name"]
            );
            eprintln!(
                "eval-do-primary-match {:?} {}",
                session.phase(),
                row["name"]
            );
            retained.push(value);
        }
        session.collect().unwrap();
        for (row, value) in rows.iter().zip(&retained) {
            let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
            decoder.refill(20_000);
            assert_eq!(
                canonical(decoder.decode_session(&mut session, value).unwrap()),
                canonical(expected(&forms[0])),
                "{} after GC",
                row["name"]
            );
        }
    }
}
