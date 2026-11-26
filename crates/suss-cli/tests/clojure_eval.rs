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

#[test]
fn eval_namespace_changes_follow_real_caller_binding_frames() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/namespace-primary/cases.json"
    ))
    .unwrap();
    assert_eq!(rows.len(), 15);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        session.enter_namespace("user").unwrap();
        macros.enter_namespace("user").unwrap();
        suss_compile::portable_repl::evaluate_script_compiled(
            &mut session,
            &mut macros,
            include_str!("../../../results/m12-eval/namespace-primary/setup.sus"),
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
            eprintln!(
                "eval-namespace-primary-match {:?} {}",
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

#[test]
fn public_eval_function_result_survives_gc_and_reads_the_live_root() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        session.enter_namespace("user").unwrap();
        session
            .eval_with_macros("(def eval-retained-root 1)", &mut macros)
            .unwrap();
        let function = session
            .eval_with_macros("(eval '(fn [x] (+ eval-retained-root x)))", &mut macros)
            .unwrap();
        session
            .eval_with_macros("(def eval-retained-root 41)", &mut macros)
            .unwrap();
        drop(macros);
        session.collect().unwrap();
        let argument = session.eval("1").unwrap();
        let value = session.invoke(&function, &[&argument]).unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Long(42)
        );
    }
}

#[test]
fn actual_aot_public_eval_reports_the_missing_explicit_compiler_capability() {
    aot_missing_capability_report(
        "(ns eval-capability) (def report (fn [] (try (str (eval '(+ 1N 1/2))) (catch :default e (.-message e)))))",
        "Compiled evaluation capability is unavailable");
}

#[test]
fn actual_aot_namespace_catalog_reports_the_missing_explicit_capability() {
    aot_missing_capability_report(
        "(ns eval-capability) (def report (fn [] (try (str (ns-interns *ns*)) (catch :default e (.-message e)))))",
        "Native namespace catalog capability is unavailable");
}

fn aot_missing_capability_report(program: &str, required: &str) {
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("app.sus"), program).unwrap();
    std::fs::write(
        root.path().join("app.wit"),
        "package test:eval-capability; world w { export report: func() -> string; }",
    )
    .unwrap();
    let compiled = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args([
            "--no-cache",
            "compile",
            "app.sus",
            "-w",
            "app.wit",
            "--export",
            "report=eval-capability/report",
            "-o",
            "app.wasm",
        ])
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let bytes = std::fs::read(root.path().join("app.wasm")).unwrap();
    let mut depth = 0;
    let mut component = false;
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        match payload.unwrap() {
            wasmparser::Payload::Version { encoding, .. } => {
                if depth == 0 {
                    component = encoding == wasmparser::Encoding::Component;
                }
                depth += 1;
            }
            wasmparser::Payload::End(_) => depth -= 1,
            wasmparser::Payload::ComponentImportSection(section) if depth == 1 => {
                assert_eq!(
                    section.count(),
                    0,
                    "the selected world has no compiler or other host imports"
                );
            }
            _ => {}
        }
    }
    assert!(component);
    assert_eq!(depth, 0);
    let executed = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args(["--no-cache", "run", "app.wasm", "--invoke", "report"])
        .output()
        .unwrap();
    assert!(
        executed.status.success(),
        "{}",
        String::from_utf8_lossy(&executed.stderr)
    );
    assert_eq!(
        String::from_utf8(executed.stdout).unwrap().trim_end(),
        required
    );
}

#[test]
fn public_eval_and_repeated_reentry_consume_the_original_operation_allowance() {
    use std::{sync::mpsc, thread, time::Duration};
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        session.eval_with_macros("(eval 1)", &mut macros).unwrap();
        let mut functions = Vec::new();
        for source in [
            "(fn [] (eval '(loop [] (recur))))",
            "(fn [] (loop [] (eval 1) (recur)))",
        ] {
            functions.push(session.eval_with_macros(source, &mut macros).unwrap());
        }
        for function in functions {
            session.set_operation_fuel(200_000);
            let interrupt = session.interrupt_handle();
            let (done, waiting) = mpsc::channel();
            let watchdog = thread::spawn(move || {
                if waiting.recv_timeout(Duration::from_secs(5)).is_err() {
                    interrupt.interrupt();
                }
            });
            let error = session.invoke(&function, &[]).unwrap_err();
            let _ = done.send(());
            watchdog.join().unwrap();
            assert!(
                matches!(&error, suss_compile::portable_session::SessionError::Trap(error)
                if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
                "eval must exhaust the original allowance before the watchdog: {error}"
            );
            session.set_operation_fuel(10_000_000);
            let value = session
                .eval_with_macros("(eval '(+ 1N 1/2))", &mut macros)
                .unwrap();
            let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
            assert_eq!(
                decoder.decode_session(&mut session, &value).unwrap(),
                Observation::Ratio("3".into(), "2".into())
            );
        }
    }
}

#[test]
fn public_eval_cancels_running_guest_code_and_leaves_the_session_usable() {
    use std::{sync::mpsc, thread, time::Duration};
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    let (events, waiting) = mpsc::channel();
    let entered = events.clone();
    // Signal only: the observer never re-enters the Store or compiler.
    session
        .install_observation_hook(move |_, _| {
            let _ = entered.send(true);
            Ok(0)
        })
        .unwrap();
    let function = session
        .eval_with_macros(
            "(fn [] (eval '(let [] (suss.harness.observation/capture 1) (loop [] (recur)))))",
            &mut macros,
        )
        .unwrap();
    // Cancellation is tested separately from the bounded allowance test above.
    session.set_operation_fuel(u64::MAX);
    let interrupt = session.interrupt_handle();
    let controller = thread::spawn(move || match waiting.recv_timeout(Duration::from_secs(5)) {
        Ok(true) => {
            interrupt.interrupt();
            true
        }
        Ok(false) => false,
        Err(_) => {
            interrupt.interrupt();
            false
        }
    });
    let error = session.invoke(&function, &[]).unwrap_err();
    let _ = events.send(false);
    assert!(
        controller.join().unwrap(),
        "the actual compiled eval body must reach its observer"
    );
    assert!(error.is_interrupt(), "{error}");
    session.set_operation_fuel(10_000_000);
    let value = session
        .eval_with_macros("(eval '(+ 1N 1/2))", &mut macros)
        .unwrap();
    let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
    assert_eq!(
        decoder.decode_session(&mut session, &value).unwrap(),
        Observation::Ratio("3".into(), "2".into())
    );
}

#[test]
fn reentrant_macro_eval_cancels_in_its_own_store_and_returns_its_lease() {
    use std::{sync::mpsc, thread, time::Duration};
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    let (events, waiting) = mpsc::channel();
    let entered = events.clone();
    macros
        .install_observation_hook(move |_, _| {
            let _ = entered.send(true);
            Ok(0)
        })
        .unwrap();
    macros.define("(defmacro eval-cancel [] (eval '(let [] (suss.harness.observation/capture 1) (loop [] (recur)))))").unwrap();
    macros.set_operation_fuel(u64::MAX);
    let interrupt = macros.interrupt_handle();
    let controller = thread::spawn(move || match waiting.recv_timeout(Duration::from_secs(5)) {
        Ok(true) => {
            interrupt.interrupt();
            true
        }
        Ok(false) => false,
        Err(_) => {
            interrupt.interrupt();
            false
        }
    });
    let error = session
        .eval_with_macros("(eval-cancel)", &mut macros)
        .unwrap_err();
    let _ = events.send(false);
    assert!(
        controller.join().unwrap(),
        "the isolated compiled Macro eval body must execute"
    );
    assert!(error.to_string().contains("Interrupted"), "{error}");
    macros.set_operation_fuel(10_000_000);
    let value = session
        .eval_with_macros("(eval '(+ 1N 1/2))", &mut macros)
        .unwrap();
    let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
    assert_eq!(
        decoder.decode_session(&mut session, &value).unwrap(),
        Observation::Ratio("3".into(), "2".into())
    );
}

#[test]
fn public_eval_diagnostics_preserve_the_actual_phase_and_unresolved_symbol() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let error = session
            .eval_with_macros("(eval '(+ eval-diagnostic-missing 1))", &mut macros)
            .unwrap_err();
        let message = suss_compile::portable_repl::error_display(&mut session, &error);
        let phase = format!("{:?}", session.phase());
        assert!(
            message.contains(&format!("Unresolved {phase} name eval-diagnostic-missing")),
            "{message}"
        );
        // Form data has no source text to fabricate a byte span from. Preserve
        // the compiler's actual diagnostic rather than replacing it with a pass.
        assert!(message.contains("bytes"), "{message}");
    }
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    macros
        .define("(defmacro eval-diagnostic [] (eval 'eval-diagnostic-missing))")
        .unwrap();
    let error = session
        .eval_with_macros("(eval-diagnostic)", &mut macros)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Unresolved Macro name eval-diagnostic-missing"),
        "{error}"
    );
}

#[test]
fn eval_compiles_nested_function_and_var_constants_including_quotation() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/review-corrections-primary/cases.json"
    ))
    .unwrap();
    assert_eq!(rows.len(), 14);
    // The four macro declaration controls also occur in the dedicated 22-row
    // declaration test. This group isolates nested constants; the original
    // 34-row body and its count/skip rules are unchanged.
    let selected: Vec<_> = rows
        .iter()
        .filter(|row| {
            !matches!(
                row["name"].as_str().unwrap(),
                "evaluated-macro"
                    | "evaluated-macro-argument"
                    | "evaluated-macro-var"
                    | "macro-prefix-survives-error"
            )
        })
        .collect();
    assert_eq!(selected.len(), 10);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        let mut retained = Vec::new();
        for row in &selected {
            let source = row["expression"]
                .as_str()
                .unwrap()
                .replace("(catch ClassCastException ", "(catch :default ");
            let value = session
                .eval_with_macros(&source, &mut macros)
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
                "eval-review-primary-match {:?} {}",
                session.phase(),
                row["name"]
            );
            retained.push(value);
        }
        session.collect().unwrap();
        for (row, value) in selected.iter().zip(&retained) {
            let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
            decoder.refill(20_000);
            assert_eq!(
                canonical(decoder.decode_session(&mut session, value).unwrap()),
                canonical(expected(&forms[0])),
                "{} after GC",
                row["name"]
            );
        }
        // The returned function still reads its retained constant after a
        // collection, a later compilation and the public macro owner's drop.
        let function = session
            .eval_with_macros(
                "(let [f (fn [x] (+ x 1))] (eval (list 'fn [] (list f 41))))",
                &mut macros,
            )
            .unwrap();
        drop(macros);
        session.collect().unwrap();
        session.eval("(+ 1 2)").unwrap();
        let value = session.invoke(&function, &[]).unwrap();
        decoder.refill(20_000);
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Long(42)
        );
    }
}

#[test]
fn reentrant_macro_eval_keeps_its_own_nested_constants() {
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    macros.define("(defmacro nested-native-constants [] [(eval (list + 1 2)) (let [f (fn [x] (+ x 1))] ((eval (list 'fn [] (list f 41))))) (identical? (var map) (first (eval [(var map)])))])").unwrap();
    let value = session
        .eval_with_macros("(nested-native-constants)", &mut macros)
        .unwrap();
    session.collect().unwrap();
    let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
    assert_eq!(
        decoder.decode_session(&mut session, &value).unwrap(),
        Observation::Vector(vec![
            Observation::Long(3),
            Observation::Long(42),
            Observation::Bool(true)
        ])
    );
}

#[test]
fn evaluated_macro_declarations_publish_real_vars_functions_and_one_expansion() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/macro-declaration-primary/cases.json"
    ))
    .unwrap();
    let definitions: Vec<String> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/macro-declaration-primary/macros.json"
    ))
    .unwrap();
    assert_eq!(rows.len(), 22);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros.define(&definitions[0]).unwrap();
        session
            .eval_with_macros("(declaration-init)", &mut macros)
            .unwrap();
        for definition in &definitions[1..] {
            macros.define(definition).unwrap();
        }
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        let mut retained = Vec::new();
        for row in &rows {
            let source = row["expression"]
                .as_str()
                .unwrap()
                .replace("(catch Exception ", "(catch :default ")
                .replace("(catch Throwable ", "(catch :default ")
                .replace("(Exception. ", "(js/Error. ");
            let value = session
                .eval_with_macros(&source, &mut macros)
                .unwrap_or_else(|error| {
                    panic!(
                        "{}: {}",
                        row["name"],
                        suss_compile::portable_repl::error_display(&mut session, &error)
                    )
                });
            let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
            assert_eq!(forms.len(), 1);
            decoder.refill(40_000);
            assert_eq!(
                canonical(decoder.decode_session(&mut session, &value).unwrap()),
                canonical(expected(&forms[0])),
                "{} {:?}",
                row["name"],
                session.phase()
            );
            eprintln!(
                "macro-declaration-primary-match {:?} {}",
                session.phase(),
                row["name"]
            );
            retained.push(value);
        }
        session.collect().unwrap();
        for (row, value) in rows.iter().zip(&retained) {
            let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
            decoder.refill(40_000);
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
fn evaluated_macro_function_survives_gc_owner_drop_and_another_macro_service() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let function = session.eval_with_macros(
            "(do (eval '(defmacro retained-macro-root [x] (list '+ x 1))) @(eval '(var retained-macro-root)))",
            &mut macros,
        ).unwrap();
        drop(macros);
        session.collect().unwrap();
        let mut other = CompiledMacros::new().unwrap();
        other
            .define("(defmacro retained-macro-root [x] 999)")
            .unwrap();
        session.eval_with_macros("(+ 1 2)", &mut other).unwrap();
        let nil = session.eval("nil").unwrap();
        let argument = session.eval("41").unwrap();
        let value = session.invoke(&function, &[&nil, &nil, &argument]).unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::List(vec![
                Observation::Symbol(None, "+".encode_utf16().collect()),
                Observation::Long(41),
                Observation::Long(1)
            ])
        );
    }
}

#[test]
fn actual_reentrant_macro_eval_publishes_in_its_own_store() {
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    macros.define("(defmacro declare-in-macro [] (let [v (eval '(defmacro inner-native-declaration [x] (list '+ x 1)))] (list 'quote [(.isMacro v) (eval '(inner-native-declaration 41)) (@v nil nil 1)])))").unwrap();
    let value = session
        .eval_with_macros("(declare-in-macro)", &mut macros)
        .unwrap();
    session.collect().unwrap();
    let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
    assert_eq!(
        decoder.decode_session(&mut session, &value).unwrap(),
        Observation::Vector(vec![
            Observation::Bool(true),
            Observation::Long(42),
            Observation::List(vec![
                Observation::Symbol(None, "+".encode_utf16().collect()),
                Observation::Long(1),
                Observation::Long(1)
            ])
        ])
    );
}

#[test]
fn eval_macro_function_calls_share_the_original_caller_allowance() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        session.eval("(def raw-budget-observations (atom 0))").unwrap();
        session
            .eval_with_macros(
                "(eval '(defmacro raw-budget-once [] (swap! raw-budget-observations inc) 1))",
                &mut macros,
            )
            .unwrap();
        let repeated = session
            .eval_with_macros(
                "(let [f @(eval '(var raw-budget-once))] (fn [] (loop [] (f nil nil) (recur))))",
                &mut macros,
            )
            .unwrap();
        let looping = session.eval_with_macros("(do (eval '(defmacro raw-budget-loop [] (swap! raw-budget-observations inc) (loop [] (recur)))) @(eval '(var raw-budget-loop)))", &mut macros).unwrap();
        let nil = session.eval("nil").unwrap();
        for (function, args) in [(&repeated, vec![]), (&looping, vec![&nil, &nil])] {
            session.eval("(reset! raw-budget-observations 0)").unwrap();
            session.set_operation_fuel(200_000);
            let start = std::time::Instant::now();
            let interrupt = session.interrupt_handle();
            let (done, waiting) = std::sync::mpsc::channel();
            let watchdog = std::thread::spawn(move || {
                if waiting
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .is_err()
                {
                    interrupt.interrupt();
                }
            });
            let error = session.invoke(function, &args).unwrap_err();
            let _ = done.send(());
            watchdog.join().unwrap();
            assert!(
                matches!(&error, suss_compile::portable_session::SessionError::Trap(error)
                if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
                "the original caller allowance must exhaust before the watchdog: {error}"
            );
            assert!(start.elapsed() < std::time::Duration::from_secs(5));
            session.set_operation_fuel(10_000_000);
            let ran = session.eval("(pos? @raw-budget-observations)").unwrap();
            let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
            assert_eq!(decoder.decode_session(&mut session, &ran).unwrap(), Observation::Bool(true),
                "the actual caller-owned Macro body must run before exhaustion");
            let value = session
                .eval_with_macros("(eval '(+ 1N 1/2))", &mut macros)
                .unwrap();
            let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
            assert_eq!(
                decoder.decode_session(&mut session, &value).unwrap(),
                Observation::Ratio("3".into(), "2".into())
            );
        }
    }
}

#[test]
fn runtime_cancellation_reaches_eval_macro_code_and_preserves_source_service() {
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    let (signal, observed) = std::sync::mpsc::sync_channel(1);
    let runtime_signal = signal.clone();
    session.install_observation_hook(move |_, _| {
        let _ = runtime_signal.try_send(());
        Ok(0)
    }).unwrap();
    macros
        .install_observation_hook(move |_, _| {
            let _ = signal.try_send(());
            Ok(0)
        })
        .unwrap();
    let function = session.eval_with_macros("(do (eval '(defmacro raw-cancel-loop [] (suss.harness.observation/capture 77) (loop [] (recur)))) @(eval '(var raw-cancel-loop)))", &mut macros).unwrap();
    let nil = session.eval("nil").unwrap();
    session.set_operation_fuel(u64::MAX);
    macros.set_operation_fuel(u64::MAX);
    let interrupt = session.interrupt_handle();
    let controller = std::thread::spawn(move || {
        let ran = observed
            .recv_timeout(std::time::Duration::from_secs(5))
            .is_ok();
        interrupt.interrupt();
        ran
    });
    let error = session.invoke(&function, &[&nil, &nil]).unwrap_err();
    assert!(
        controller.join().unwrap(),
        "the compiled Macro body must reach its signal-only observer"
    );
    assert!(error.to_string().contains("Interrupted"), "{error}");
    session.set_operation_fuel(10_000_000);
    macros.set_operation_fuel(10_000_000);
    let value = session
        .eval_with_macros("(eval '(+ 1N 1/2))", &mut macros)
        .unwrap();
    let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
    assert_eq!(
        decoder.decode_session(&mut session, &value).unwrap(),
        Observation::Ratio("3".into(), "2".into())
    );
}

#[test]
fn public_alias_preserves_primary_state_errors_and_argument_order() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/alias-primary/cases.json"
    )).unwrap();
    assert_eq!(rows.len(), 15);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        // The primary creates these namespaces. Establish the same objects via
        // ordinary native ns forms; create-ns is a separate public API gap.
        suss_compile::portable_repl::evaluate_script_compiled(
            &mut session, &mut macros,
            "(ns alias-target) (ns alias-other) (ns alias-user) (ns user)", None,
        ).unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        let mut retained = Vec::new();
        for row in &rows {
            let source = row["expression"].as_str().unwrap()
                .replace("(create-ns ", "(the-ns ")
                .replace("clojure.core", "suss.core")
                .replace("(catch Throwable ", "(catch :default ")
                .replace("(.getMessage e)", "(ex-message e)");
            let value = session.eval_with_macros(&source, &mut macros).unwrap_or_else(|error| {
                panic!("{}: {}", row["name"], suss_compile::portable_repl::error_display(&mut session, &error))
            });
            let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
            assert_eq!(forms.len(), 1);
            decoder.refill(40_000);
            assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), canonical(expected(&forms[0])), "{} {:?}", row["name"], session.phase());
            eprintln!("alias-primary-match {:?} {}", session.phase(), row["name"]);
            retained.push(value);
        }
        session.collect().unwrap();
        for (row, value) in rows.iter().zip(&retained) {
            let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
            decoder.refill(40_000);
            assert_eq!(canonical(decoder.decode_session(&mut session, value).unwrap()), canonical(expected(&forms[0])));
        }
    }
}

#[test]
fn evaluated_macro_environment_keeps_nil_top_level_and_actual_lexical_names() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/macro-environment-primary/cases.json"
    )).unwrap();
    assert_eq!(rows.len(), 3);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        let mut retained = Vec::new();
        for row in &rows {
            let value = session.eval_with_macros(row["expression"].as_str().unwrap(), &mut macros).unwrap();
            let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
            assert_eq!(forms.len(), 1);
            decoder.refill(40_000);
            assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), canonical(expected(&forms[0])), "{} {:?}", row["name"], session.phase());
            eprintln!("macro-environment-primary-match {:?} {}", session.phase(), row["name"]);
            retained.push(value);
        }
        session.collect().unwrap();
        for (row, value) in rows.iter().zip(&retained) {
            let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
            decoder.refill(40_000);
            assert_eq!(canonical(decoder.decode_session(&mut session, value).unwrap()), canonical(expected(&forms[0])));
        }
    }
}

#[test]
fn failed_macro_replacement_preserves_live_metadata_and_successful_nested_effects() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/macro-failure-primary/cases.json"
    )).unwrap();
    assert_eq!(rows.len(), 5);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        for row in &rows {
            let source = row["expression"].as_str().unwrap().replace("(Exception. ", "(js/Error. ").replace("(catch Throwable ", "(catch :default ");
            let value = session.eval_with_macros(&source, &mut macros).unwrap_or_else(|error| {
                panic!("{}: {}", row["name"], suss_compile::portable_repl::error_display(&mut session, &error))
            });
            let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
            assert_eq!(forms.len(), 1);
            session.collect().unwrap();
            decoder.refill(40_000);
            assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), canonical(expected(&forms[0])), "{} {:?}", row["name"], session.phase());
            eprintln!("macro-failure-primary-match {:?} {}", session.phase(), row["name"]);
        }
    }
}

#[test]
fn symbol_conversion_uses_public_var_names_and_clojure_two_argument_admission() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/symbol-primary/cases.json"
    )).unwrap();
    assert_eq!(rows.len(), 27);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        let mut retained = Vec::new();
        for row in &rows {
            let source = row["expression"].as_str().unwrap().replace("(catch Throwable ", "(catch :default ");
            let value = session.eval_with_macros(&source, &mut macros).unwrap_or_else(|error| {
                panic!("{}: {}", row["name"], suss_compile::portable_repl::error_display(&mut session, &error))
            });
            let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
            assert_eq!(forms.len(), 1);
            decoder.refill(40_000);
            assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), canonical(expected(&forms[0])), "{} {:?}", row["name"], session.phase());
            eprintln!("symbol-primary-match {:?} {}", session.phase(), row["name"]);
            retained.push(value);
        }
        session.collect().unwrap();
        for (row, value) in rows.iter().zip(&retained) {
            let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
            decoder.refill(40_000);
            assert_eq!(canonical(decoder.decode_session(&mut session, value).unwrap()), canonical(expected(&forms[0])));
        }
    }
}

#[test]
fn live_var_metadata_and_roots_survive_replacing_the_core_indexed_seq_constructor() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        session
            .eval("(ns suss.core) (def IndexedSeq nil)")
            .unwrap();
        session.enter_namespace("user").unwrap();
        let original = session.eval("(def ^{:probe 17} retained 42)").unwrap();
        let observe = session
            .eval("(fn [v] [(var? v) @v (:probe (meta v)) (= v (var retained))])")
            .unwrap();
        for (root, metadata) in [(42, 17), (43, 18)] {
            if root == 43 {
                session.eval("(def ^{:probe 18} retained 43)").unwrap();
            }
            session.collect().unwrap();
            let value = session.invoke(&observe, &[&original]).unwrap();
            decoder.refill(40_000);
            assert_eq!(
                decoder.decode_session(&mut session, &value).unwrap(),
                Observation::Vector(vec![
                    Observation::Bool(true),
                    Observation::Long(root),
                    Observation::Long(metadata),
                    Observation::Bool(true),
                ]),
                "{:?}", session.phase()
            );
        }
    }
}

#[test]
fn var_metadata_uses_original_core_data_without_changing_user_resolution() {
    for constant in ["", "^:const "] {
        for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
            let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
            session
                .eval("(def ^{:probe [0 [0]]} metadata-target 0)")
                .unwrap();
            let retained = session.eval("(var metadata-target)").unwrap();
            let observe = session.eval("(let [vector vector var? var? meta meta get get key :probe computed :computed] (fn [v] (vector (var? v) @v (get (meta v) key) (get (meta v) computed))))").unwrap();
            session.eval(&format!("(ns suss.core) (def count (fn [x] 99)) (def {constant}PersistentVector nil) (def {constant}Keyword nil)")).unwrap();
            session.enter_namespace("user").unwrap();
            for (root, metadata) in [(42, 17), (43, 18)] {
                session.eval(&format!("(def ^{{:probe [{metadata} [{}]] :computed (let [f count] (f 123))}} metadata-target {root}) (def public-count-result (let [f count] (f 123)))", metadata + 2)).unwrap();
                session.collect().unwrap();
                let value = session.invoke(&observe, &[&retained]).unwrap();
                decoder.refill(40_000);
                assert_eq!(
                    decoder.decode_session(&mut session, &value).unwrap(),
                    Observation::Vector(vec![
                        Observation::Bool(true),
                        Observation::Long(root),
                        Observation::Vector(vec![
                            Observation::Long(metadata),
                            Observation::Vector(vec![Observation::Long(metadata + 2)]),
                        ]),
                        Observation::Long(99),
                    ]),
                    "{:?} {constant}", session.phase()
                );
                let ordinary = session.eval("public-count-result").unwrap();
                assert_eq!(
                    decoder.decode_session(&mut session, &ordinary).unwrap(),
                    Observation::Long(99)
                );
            }
        }
    }
}

#[test]
fn eval_preserves_existing_scalar_roots_in_runtime_macro_and_reentry() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/scalar-identity-primary/cases.json"
    )).unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
    let primary = expected(&forms[0]);
    let expression = row["expression"].as_str().unwrap();
    assert_eq!(row["expected"].as_str().unwrap().trim(), include_str!(
        "../../../results/m12-eval/scalar-identity-primary/primary.stdout.log"
    ).trim());
    assert!(include_str!("../../../results/m12-eval/scalar-identity-primary/oracle.clj")
        .ends_with(&format!("(prn {expression})\n")));
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        let value = session.eval(expression).unwrap();
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), primary, "{:?}", session.phase());
        let observer = session.eval("(fn [x] (identical? x (eval x)))").unwrap();
        for source in ["9223372036854775808N", "1/3", "1.50M", r#"#uuid "00112233-4455-6677-8899-aabbccddeeff""#] {
            let original = session.eval(source).unwrap();
            session.collect().unwrap();
            let value = session.invoke(&observer, &[&original]).unwrap();
            decoder.refill(40_000);
            assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::Bool(true), "{:?} {source}", session.phase());
        }
    }
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    macros.define(&format!("(defmacro scalar-self-roots [] {expression})")).unwrap();
    let value = session.eval_with_macros("(scalar-self-roots)", &mut macros).unwrap();
    session.collect().unwrap();
    let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
    assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), primary);
}

#[test]
fn eval_retains_actual_class_values_in_generated_numeric_forms() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/class-data-primary/cases.json"
    )).unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    let expression = row["expression"].as_str().unwrap();
    assert_eq!(row["expected"].as_str().unwrap().trim(), include_str!(
        "../../../results/m12-eval/class-data-primary/primary.stdout.log"
    ).trim());
    assert!(include_str!("../../../results/m12-eval/class-data-primary/oracle.clj")
        .ends_with(&format!("(prn {expression})\n")));
    let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
    let primary = expected(&forms[0]);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        let value = session.eval(expression).unwrap();
        session.collect().unwrap();
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), primary, "{:?}", session.phase());
        let observer = session.eval("(fn [t x] [(identical? t (eval t)) (eval (list 'instance? t x))])").unwrap();
        for source in ["42", "1.0", "9223372036854775808N", "1/3", "1.50M", "(biginteger 7)"] {
            let original = session.eval(source).unwrap();
            let class_of = session.eval("type").unwrap();
            let class = session.invoke(&class_of, &[&original]).unwrap();
            session.collect().unwrap();
            let value = session.invoke(&observer, &[&class, &original]).unwrap();
            decoder.refill(40_000);
            assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::Vector(vec![Observation::Bool(true), Observation::Bool(true)]), "{:?} {source}", session.phase());
        }
    }
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    macros.define(&format!("(defmacro numeric-class-data [] {expression})")).unwrap();
    let value = session.eval_with_macros("(numeric-class-data)", &mut macros).unwrap();
    session.collect().unwrap();
    let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
    assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), primary);
}


fn promotion_review_controls(name: &str, source: &str, primary: &str, count: usize) {
    let forms = suss_reader::forms::read_forms(primary.trim()).unwrap();
    assert_eq!(forms.len(), 1);
    let Kind::Vector(rows) = &forms[0].kind else { panic!("primary must retain all ordered controls") };
    assert_eq!(rows.len(), count);
    let wanted = canonical(expected(&forms[0]));
    let mut failures = Vec::new();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 80_000).unwrap();
        let source = source.replace("(catch Throwable ", "(catch :default ");
        match session.eval_with_macros(&source, &mut macros) {
            Ok(value) => {
                session.collect().unwrap();
                let actual = canonical(decoder.decode_session(&mut session, &value).unwrap());
                if actual != wanted { failures.push(format!("{name} {phase:?}: expected {wanted:?}, actual {actual:?}")); }
            }
            Err(error) => failures.push(format!("{name} {phase:?}: {}", suss_compile::portable_repl::error_display(&mut session, &error))),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn eval_function_constant_materialization_preserves_primary_identity_and_errors() {
    promotion_review_controls("function-constants",
        include_str!("../../../results/m12-eval/promotion-review-findings/function-constants/expression.sus"),
        include_str!("../../../results/m12-eval/promotion-review-findings/function-constants/primary.stdout"), 6);
}

#[test]
fn evaluated_macro_language_exceptions_preserve_primary_message_and_data() {
    promotion_review_controls("macro-exception-data",
        include_str!("../../../results/m12-eval/promotion-review-findings/macro-exception-data/expression.sus"),
        include_str!("../../../results/m12-eval/promotion-review-findings/macro-exception-data/primary.stdout"), 2);
}

#[test]
fn evaluated_macro_calls_inherit_the_primary_caller_math_context() {
    promotion_review_controls("macro-math-context",
        include_str!("../../../results/m12-eval/promotion-review-findings/macro-math-context/expression.sus"),
        include_str!("../../../results/m12-eval/promotion-review-findings/macro-math-context/primary.stdout"), 3);
}


fn supplemental_materialization_context_controls(start: usize, end: usize) {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/materialization-and-context-primary/cases.json")).unwrap();
    assert_eq!(rows.len(), 8);
    let stdout = include_str!("../../../results/m12-eval/materialization-and-context-primary/stdout.log");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), rows.iter().map(|row| row["observed_edn"].as_str().unwrap()).collect::<Vec<_>>());
    let source = format!("[{}]", rows[start..end].iter().map(|row| row["expression"].as_str().unwrap()).collect::<Vec<_>>().join(" "));
    let primary = format!("[{}]", rows[start..end].iter().map(|row| row["observed_edn"].as_str().unwrap()).collect::<Vec<_>>().join(" "));
    promotion_review_controls("supplemental-materialization-context", &source, &primary, end-start);
}

#[test]
fn eval_math_context_scopes_preserve_rounding_nil_and_exception_restoration() {
    supplemental_materialization_context_controls(5, 8);
}

#[test]
fn eval_function_constant_initialization_preserves_unit_effects_and_aliases() {
    supplemental_materialization_context_controls(0, 5);
}


fn independent_exception_function_primary(rows: &str, stdout: &str, name: &str) {
    let rows: Vec<serde_json::Value> = serde_json::from_str(rows).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(stdout.lines().collect::<Vec<_>>(), rows.iter().map(|row| row["observed_edn"].as_str().unwrap()).collect::<Vec<_>>());
    // Each primary expression returns normally. An outer owned catch makes
    // unexpected native errors visible without preventing later controls.
    let source = format!("[{}]", rows.iter().map(|row| format!("(try {} (catch :default unexpected :unexpected-native-error))", row["expression"].as_str().unwrap())).collect::<Vec<_>>().join(" "));
    let primary = format!("[{}]", rows.iter().map(|row| row["observed_edn"].as_str().unwrap()).collect::<Vec<_>>().join(" "));
    promotion_review_controls(name, &source, &primary, 3);
}

#[test]
fn evaluated_macro_exceptions_preserve_exact_data_cause_plain_and_nil_messages() {
    independent_exception_function_primary(
        include_str!("../../../results/m12-eval/exception-transfer-primary/cases.json"),
        include_str!("../../../results/m12-eval/exception-transfer-primary/stdout.log"), "exception-extra-primary");
}

#[test]
fn eval_metadata_functions_preserve_top_roots_and_nested_unit_boundaries() {
    independent_exception_function_primary(
        include_str!("../../../results/m12-eval/function-metadata-primary/cases.json"),
        include_str!("../../../results/m12-eval/function-metadata-primary/stdout.log"), "metadata-function-primary");
}


#[test]
fn eval_function_pool_context_preserves_direct_def_and_distinct_nested_pools() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/function-def-and-pool-primary/cases.json")).unwrap();
    assert_eq!(rows.len(), 5);
    let stdout = include_str!("../../../results/m12-eval/function-def-and-pool-primary/stdout.log");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), rows.iter().map(|row| row["observed_edn"].as_str().unwrap()).collect::<Vec<_>>());
    let primary = format!("[{}]", rows.iter().map(|row| row["observed_edn"].as_str().unwrap()).collect::<Vec<_>>().join(" "));
    let adapters: Vec<serde_json::Value> = serde_json::from_str(include_str!("../../../results/m12-eval/function-def-and-pool-primary/native-adapter.json")).unwrap();
    assert_eq!(adapters.len(), 5);
    for (row, adapter) in rows.iter().zip(&adapters) { assert_eq!(row["name"], adapter["name"]); assert_eq!(row["observed_edn"], adapter["observed_edn"]); }
    let source = format!("[{}]", adapters.iter().map(|row| format!("(try {} (catch :default unexpected :unexpected-native-error))", row["native_expression"].as_str().unwrap())).collect::<Vec<_>>().join(" "));
    assert_eq!(include_str!("../../../results/m12-eval/function-def-and-pool-primary/adapted-stdout.log"), stdout);
    promotion_review_controls("function-def-context-and-pools", &source, &primary, 5);
}


#[test]
fn eval_function_pool_initialization_preserves_construction_effects_and_arity_sharing() {
    independent_exception_function_primary(
        include_str!("../../../results/m12-eval/function-pool-initialization-primary/cases.json"),
        include_str!("../../../results/m12-eval/function-pool-initialization-primary/stdout.log"), "function-pool-construction-primary");
}

#[test]
fn eval_invalid_math_context_is_validated_at_consumption_and_preserves_binding_data() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/lazy-math-context-primary/cases.json")).unwrap();
    assert_eq!(rows.len(), 20);
    let stdout = include_str!("../../../results/m12-eval/lazy-math-context-primary/primary.stdout");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), rows.iter().map(|row| row["observed_edn"].as_str().unwrap()).collect::<Vec<_>>());
    let primary = format!("[{}]", rows.iter().map(|row| row["observed_edn"].as_str().unwrap()).collect::<Vec<_>>().join(" "));
    promotion_review_controls("lazy-math-context",
        include_str!("../../../results/m12-eval/lazy-math-context-primary/expression.sus"), &primary, 20);
}

#[test]
fn actual_macro_eval_preserves_invalid_math_context_consumption_and_binding_data() {
    let primary = format!("[{}]", include_str!("../../../results/m12-eval/lazy-math-context-primary/primary.stdout").lines().collect::<Vec<_>>().join(" "));
    let forms = suss_reader::forms::read_forms(&primary).unwrap();
    let wanted = canonical(expected(&forms[0]));
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let source = include_str!("../../../results/m12-eval/lazy-math-context-primary/expression.sus");
        macros.define(&format!("(defmacro lazy-context-in-actual-macro [] (list 'quote {source}))")).unwrap();
        let value = session.eval_with_macros("(lazy-context-in-actual-macro)", &mut macros).unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 80_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), wanted, "{:?}", session.phase());
    }
}


#[test]
fn unbound_dynamic_frames_preserve_values_effects_and_eval_context() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/unbound-dynamic-primary/cases.json")).unwrap();
    assert_eq!(rows.len(), 6);
    let stdout = include_str!("../../../results/m12-eval/unbound-dynamic-primary/primary.stdout");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), rows.iter().map(|row| row["observed_edn"].as_str().unwrap()).collect::<Vec<_>>());
    let primary = format!("[{}]", stdout.lines().collect::<Vec<_>>().join(" "));
    promotion_review_controls("unbound-dynamic-frames",
        include_str!("../../../results/m12-eval/unbound-dynamic-primary/expression.sus"), &primary, 6);
}

#[test]
fn actual_macro_eval_preserves_unbound_dynamic_frames() {
    let primary = format!("[{}]", include_str!("../../../results/m12-eval/unbound-dynamic-primary/primary.stdout").lines().collect::<Vec<_>>().join(" "));
    let forms = suss_reader::forms::read_forms(&primary).unwrap();
    let wanted = canonical(expected(&forms[0]));
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let source = include_str!("../../../results/m12-eval/unbound-dynamic-primary/expression.sus");
        macros.define(&format!("(defmacro unbound-frames-in-actual-macro [] (list 'quote {source}))")).unwrap();
        let value = session.eval_with_macros("(unbound-frames-in-actual-macro)", &mut macros).unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 80_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), wanted, "{:?}", session.phase());
    }
}


#[test]
fn binding_initializers_preserve_definition_eval_and_outer_frame_effects() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/binding-initializer-effects-primary/cases.json")).unwrap();
    assert_eq!(rows.len(), 9);
    let stdout = include_str!("../../../results/m12-eval/binding-initializer-effects-primary/primary.stdout");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), rows.iter().map(|row| row["observed_edn"].as_str().unwrap()).collect::<Vec<_>>());
    let primary = format!("[{}]", stdout.lines().collect::<Vec<_>>().join(" "));
    promotion_review_controls("binding-initializer-effects",
        include_str!("../../../results/m12-eval/binding-initializer-effects-primary/expression.sus"), &primary, 9);
}

#[test]
fn actual_macro_eval_preserves_binding_initializer_effects() {
    let primary = format!("[{}]", include_str!("../../../results/m12-eval/binding-initializer-effects-primary/primary.stdout").lines().collect::<Vec<_>>().join(" "));
    let forms = suss_reader::forms::read_forms(&primary).unwrap();
    let wanted = canonical(expected(&forms[0]));
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let source = include_str!("../../../results/m12-eval/binding-initializer-effects-primary/expression.sus");
        macros.define(&format!("(defmacro initializer-effects-in-actual-macro [] (list 'quote {source}))")).unwrap();
        let value = session.eval_with_macros("(initializer-effects-in-actual-macro)", &mut macros).unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 80_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), wanted, "{:?}", session.phase());
    }
}

#[test]
fn binding_bodies_keep_root_definitions_separate_from_dynamic_assignment() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/binding-root-effects-primary/cases.json")).unwrap();
    assert_eq!(rows.len(), 8);
    let stdout = include_str!("../../../results/m12-eval/binding-root-effects-primary/primary.stdout");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), rows.iter().map(|row| row["observed_edn"].as_str().unwrap()).collect::<Vec<_>>());
    let primary = format!("[{}]", stdout.lines().collect::<Vec<_>>().join(" "));
    promotion_review_controls("binding-root-effects",
        include_str!("../../../results/m12-eval/binding-root-effects-primary/expression.sus"), &primary, 8);
}

#[test]
fn actual_macro_eval_preserves_binding_body_root_definitions() {
    let primary = format!("[{}]", include_str!("../../../results/m12-eval/binding-root-effects-primary/primary.stdout").lines().collect::<Vec<_>>().join(" "));
    let forms = suss_reader::forms::read_forms(&primary).unwrap();
    let wanted = canonical(expected(&forms[0]));
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let source = include_str!("../../../results/m12-eval/binding-root-effects-primary/expression.sus");
        macros.define(&format!("(defmacro root-effects-in-actual-macro [] (list 'quote {source}))")).unwrap();
        let value = session.eval_with_macros("(root-effects-in-actual-macro)", &mut macros).unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 80_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), wanted, "{:?}", session.phase());
    }
}

#[test]
fn root_redefinitions_restore_roots_and_respect_existing_thread_bindings() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/with-redefs-root-primary/cases.json")).unwrap();
    assert_eq!(rows.len(), 12);
    let stdout = include_str!("../../../results/m12-eval/with-redefs-root-primary/primary.stdout");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), rows.iter().map(|row| row["observed_edn"].as_str().unwrap()).collect::<Vec<_>>());
    let primary = format!("[{}]", stdout.lines().collect::<Vec<_>>().join(" "));
    promotion_review_controls("with-redefs-roots",
        include_str!("../../../results/m12-eval/with-redefs-root-primary/expression.sus"), &primary, 12);
}

#[test]
fn actual_macro_eval_distinguishes_root_redefinitions_from_thread_bindings() {
    let primary = format!("[{}]", include_str!("../../../results/m12-eval/with-redefs-root-primary/primary.stdout").lines().collect::<Vec<_>>().join(" "));
    let forms = suss_reader::forms::read_forms(&primary).unwrap();
    let wanted = canonical(expected(&forms[0]));
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let source = include_str!("../../../results/m12-eval/with-redefs-root-primary/expression.sus");
        macros.define(&format!("(defmacro redefs-roots-in-actual-macro [] (list 'quote {source}))")).unwrap();
        let value = session.eval_with_macros("(redefs-roots-in-actual-macro)", &mut macros).unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 80_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), wanted, "{:?}", session.phase());
    }
}

#[test]
fn root_macro_status_and_source_duplicate_effects_match_primary() {
    promotion_review_controls("macro-root-and-source-duplicates",
        include_str!("../../../results/m12-eval/macro-root-and-duplicate-primary/expression.sus"),
        include_str!("../../../results/m12-eval/macro-root-and-duplicate-primary/primary.stdout"), 13);
}

#[test]
fn actual_macro_eval_preserves_root_macro_status_and_source_duplicate_effects() {
    let primary = include_str!("../../../results/m12-eval/macro-root-and-duplicate-primary/primary.stdout");
    let forms = suss_reader::forms::read_forms(primary).unwrap();
    let wanted = canonical(expected(&forms[0]));
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let source = include_str!("../../../results/m12-eval/macro-root-and-duplicate-primary/expression.sus");
        macros.define(&format!("(defmacro root-macro-duplicate-controls [] (list 'quote {source}))")).unwrap();
        let value = session.eval_with_macros("(root-macro-duplicate-controls)", &mut macros).unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 80_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), wanted, "{:?}", session.phase());
    }
}

#[test]
fn root_metadata_identity_survives_macro_status_clearing() {
    promotion_review_controls("root-metadata-identity",
        include_str!("../../../results/m12-eval/metadata-identity-primary/expression.sus"),
        include_str!("../../../results/m12-eval/metadata-identity-primary/primary.stdout"), 3);
}

#[test]
fn root_thread_selected_macro_bindings_preserve_forms_identity_and_effects() {
    promotion_review_controls("thread-selected-macro",
        include_str!("../../../results/m12-eval/thread-macro-binding-primary/expression.sus"),
        include_str!("../../../results/m12-eval/thread-macro-binding-primary/primary.stdout"), 9);
}

#[test]
fn actual_macro_eval_preserves_root_metadata_identity() {
    let primary = include_str!("../../../results/m12-eval/metadata-identity-primary/primary.stdout");
    let forms = suss_reader::forms::read_forms(primary).unwrap();
    let wanted = canonical(expected(&forms[0]));
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let source = include_str!("../../../results/m12-eval/metadata-identity-primary/expression.sus");
        macros.define(&format!("(defmacro root-metadata-controls [] (list 'quote {source}))")).unwrap();
        let value = session.eval_with_macros("(root-metadata-controls)", &mut macros).unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 80_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), wanted);
    }
}

#[test]
fn actual_macro_eval_preserves_root_thread_selected_macro_bindings() {
    let primary = include_str!("../../../results/m12-eval/thread-macro-binding-primary/primary.stdout");
    let forms = suss_reader::forms::read_forms(primary).unwrap();
    let wanted = canonical(expected(&forms[0]));
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let source = include_str!("../../../results/m12-eval/thread-macro-binding-primary/expression.sus");
        macros.define(&format!("(defmacro root-thread-binding-controls [] (list 'quote {source}))")).unwrap();
        let value = session.eval_with_macros("(root-thread-binding-controls)", &mut macros).unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 80_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), wanted);
    }
}

#[test]
fn root_thread_macro_inherits_caller_fuel_and_restores_binding_after_exhaustion() {
    use std::{sync::{Arc, atomic::{AtomicUsize, Ordering}, mpsc}, thread, time::Duration};
    {
        let mut session = Session::new_repl().unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let observer = count.clone();
        session.install_observation_hook(move |_, _| { observer.fetch_add(1, Ordering::SeqCst); Ok(0) }).unwrap();
        let function = session.eval_with_macros(
            "(do (declare thread-fuel-macro) (eval '(defmacro ^:dynamic thread-fuel-macro [] 1)) (let [looping (fn [& xs] (suss.harness.observation/capture 1) (loop [] (recur)))] (fn [] (binding [thread-fuel-macro looping] (eval '(user/thread-fuel-macro))))))",
            &mut macros).unwrap();
        session.set_operation_fuel(200_000);
        let interrupt = session.interrupt_handle();
        let (done, waiting) = mpsc::channel();
        let watchdog = thread::spawn(move || { if waiting.recv_timeout(Duration::from_secs(5)).is_err() { interrupt.interrupt(); } });
        let error = session.invoke(&function, &[]).unwrap_err();
        let _ = done.send(()); watchdog.join().unwrap();
        assert!(matches!(&error, suss_compile::portable_session::SessionError::Trap(error)
            if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{}", suss_compile::portable_repl::error_display(&mut session, &error));
        assert!(count.load(Ordering::SeqCst) > 0, "the actual selected transformer must execute");
        session.set_operation_fuel(10_000_000);
        let value = session.eval_with_macros("(eval '(user/thread-fuel-macro))", &mut macros).unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::Long(1));
    }
}

#[test]
fn root_thread_macro_cancels_actual_caller_code_and_returns_macro_service() {
    use std::{sync::mpsc, thread, time::Duration};
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    let (events, waiting) = mpsc::channel(); let entered = events.clone();
    session.install_observation_hook(move |_, _| { let _ = entered.send(true); Ok(0) }).unwrap();
    let function = session.eval_with_macros(
        "(do (declare thread-cancel-macro) (eval '(defmacro ^:dynamic thread-cancel-macro [] 1)) (let [looping (fn [& xs] (suss.harness.observation/capture 1) (loop [] (recur)))] (fn [] (binding [thread-cancel-macro looping] (eval '(user/thread-cancel-macro))))))",
        &mut macros).unwrap();
    session.set_operation_fuel(u64::MAX);
    let interrupt = session.interrupt_handle();
    let controller = thread::spawn(move || { let entered = waiting.recv_timeout(Duration::from_secs(5)) == Ok(true); interrupt.interrupt(); entered });
    let error = session.invoke(&function, &[]).unwrap_err(); let _ = events.send(false);
    assert!(controller.join().unwrap(), "the selected caller transformer must reach its observer: {}", suss_compile::portable_repl::error_display(&mut session, &error));
    assert!(error.is_interrupt(), "{error}");
    session.set_operation_fuel(10_000_000);
    let value = session.eval_with_macros("(eval '(user/thread-cancel-macro))", &mut macros).unwrap();
    let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
    assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::Long(1));
}

#[test]
fn root_expanded_function_and_collection_contexts_preserve_primary_identity() {
    promotion_review_controls("expanded-function-collection-contexts",
        include_str!("../../../results/m12-eval/expanded-macro-collection-primary/expression.sus"),
        include_str!("../../../results/m12-eval/expanded-macro-collection-primary/primary.stdout"), 4);
}

#[test]
fn actual_macro_eval_preserves_root_expanded_function_and_collection_identity() {
    let primary = include_str!("../../../results/m12-eval/expanded-macro-collection-primary/primary.stdout");
    let forms = suss_reader::forms::read_forms(primary).unwrap();
    let wanted = canonical(expected(&forms[0]));
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let source = include_str!("../../../results/m12-eval/expanded-macro-collection-primary/expression.sus");
        macros.define(&format!("(defmacro expanded-function-collection-controls [] (list 'quote {source}))")).unwrap();
        let value = session.eval_with_macros("(expanded-function-collection-controls)", &mut macros).unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 80_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), wanted);
    }
}


#[test]
fn eval_preserves_self_evaluating_objects_and_false_macro_metadata() {
    promotion_review_controls("self-values-and-false-metadata",
        include_str!("../../../results/m12-eval/self-values-and-false-metadata-primary/expression.sus"),
        include_str!("../../../results/m12-eval/self-values-and-false-metadata-primary/primary.stdout"), 13);
}

#[test]
fn actual_macro_eval_preserves_self_values_and_false_macro_metadata() {
    let primary = include_str!("../../../results/m12-eval/self-values-and-false-metadata-primary/primary.stdout");
    let forms = suss_reader::forms::read_forms(primary).unwrap();
    let wanted = canonical(expected(&forms[0]));
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let source = include_str!("../../../results/m12-eval/self-values-and-false-metadata-primary/expression.sus");
        macros.define(&format!("(defmacro self-value-metadata-controls [] (list 'quote {source}))")).unwrap();
        let value = session.eval_with_macros("(self-value-metadata-controls)", &mut macros).unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 80_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), wanted);
    }
}

#[test]
fn eval_self_values_retain_their_actual_roots_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let evaluate = session.eval_with_macros("(fn [x] (eval x))", &mut macros).unwrap();
        let identity = session.eval("(fn [x y] (identical? x y))").unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        for source in ["(atom 7)", "(delay 7)", "(ex-info \"held\" {:x 1})", "(the-ns 'user)"] {
            let original = session.eval_with_macros(source, &mut macros).unwrap();
            session.collect().unwrap();
            let evaluated = session.invoke(&evaluate, &[&original]).unwrap();
            session.collect().unwrap();
            let result = session.invoke(&identity, &[&original, &evaluated]).unwrap();
            decoder.refill(20_000);
            assert_eq!(decoder.decode_session(&mut session, &result).unwrap(), Observation::Bool(true),
                "{:?} {source}", session.phase());
        }
    }
}

#[test]
fn root_macro_flags_stay_private_when_ordinary_source_enters_core() {
    fn expect(session: &mut Session, source: &str, wanted: &str) {
        let value = session.eval(source).unwrap_or_else(|error| panic!("{source}: {error}"));
        session.collect().unwrap();
        let mut decoder = Decoder::capture(session, 30_000).unwrap();
        let forms = suss_reader::forms::read_forms(wanted).unwrap();
        assert_eq!(canonical(decoder.decode_session(session, &value).unwrap()),
            canonical(expected(&forms[0])), "{source}");
    }
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(eval '(defmacro source-core-flag-macro [] 1))").unwrap();
        for source in [
            "(suss.bootstrap/binding-getter-macro? nil)",
            "(suss.bootstrap/binding-getter-macro-set! nil true)",
        ] {
            let error = session.eval(source).unwrap_err();
            assert!(error.to_string().contains("Var macro flags require compiled core"), "{error}");
        }
        session.eval("(def source-core-flag-var (var source-core-flag-macro))").unwrap();
        session.enter_namespace("suss.core").unwrap();
        // The host may extract a canonical getter to exercise this boundary;
        // entering the core namespace alone cannot read built-in fields.
        expect(&mut session,
            "(try (.-val user/source-core-flag-var) (catch :default e (ex-message e)))",
            "\"Built-in type internals are private to suss.core\"");
        session.eval_privileged("(def source-core-native-getter (.-val user/source-core-flag-var))").unwrap();
        expect(&mut session, r#"
          (let [trace (atom []) getter source-core-native-getter]
            [(suss.bootstrap/binding-getter-macro?
               (do (swap! trace conj :read) getter))
             (identical? getter
               (suss.bootstrap/binding-getter-macro-set!
                 (do (swap! trace conj :getter) getter)
                 (do (swap! trace conj :flag) false)))
             @trace (.isMacro (var user/source-core-flag-macro))
             (eval '(user/source-core-flag-macro))])"#,
            "[true true [:read :getter :flag] true 1]");
        for source in [
            "(suss.bootstrap/binding-getter-macro?)",
            "(suss.bootstrap/binding-getter-macro-set! nil)",
        ] {
            let error = session.eval(source).unwrap_err();
            assert!(error.to_string().contains("Invalid private native object adapter arity"), "{error}");
        }
        expect(&mut session,
            "(with-redefs [user/source-core-flag-macro (fn [] 42)] (eval '(user/source-core-flag-macro)))",
            "42");
        expect(&mut session, r#"
          (let [getter source-core-native-getter]
            [(identical? getter (suss.bootstrap/binding-getter-macro-set! getter true))
             (.isMacro (var user/source-core-flag-macro))])"#,
            "[true false]");
        session.enter_namespace("user").unwrap();
    }
}

#[test]
fn native_eval_macros_use_caller_state_and_recheck_live_authorization() {
    promotion_review_controls("caller-owned-macro-state",
        include_str!("../../../results/m12-eval/caller-owned-macro-primary/expression.sus"),
        include_str!("../../../results/m12-eval/caller-owned-macro-primary/primary.stdout"), 5);
}

#[test]
fn actual_macro_eval_uses_caller_state_and_live_authorization() {
    let source = include_str!("../../../results/m12-eval/caller-owned-macro-primary/expression.sus");
    let forms = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/caller-owned-macro-primary/primary.stdout")).unwrap();
    let wanted = canonical(expected(&forms[0]));
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros.define(&format!("(defmacro native-caller-state-probe [] (list 'quote {source}))")).unwrap();
        let value = session.eval_with_macros("(native-caller-state-probe)", &mut macros).unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 80_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), wanted);
    }
}

#[test]
fn native_eval_macro_catalog_survives_source_entries_without_changing_namespace_roots() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let namespace = session.eval("(fn [] (ns-name *ns*))").unwrap();
        let before = session.invoke(&namespace, &[]).unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        let before = decoder.decode_session(&mut session, &before).unwrap();
        session.eval_with_macros("(eval '(defmacro native-next-source [] 42))", &mut macros).unwrap();
        session.collect().unwrap();
        let value = session.eval_with_macros("(native-next-source)", &mut macros).unwrap();
        decoder.refill(40_000);
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::Long(42));
        let value = session.invoke(&namespace, &[]).unwrap();
        decoder.refill(40_000);
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), before);
        let value = session.eval_with_macros("(do (eval '(defmacro native-source-ns [] (list 'quote (ns-name *ns*)))) (eval '(native-source-ns)))", &mut macros).unwrap();
        decoder.refill(40_000);
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::Symbol(None, "user".encode_utf16().collect()));
    }
}

#[test]
fn native_eval_macro_effects_survive_outer_analysis_and_truthy_metadata() {
    promotion_review_controls("caller-macro-effects",
        include_str!("../../../results/m12-eval/caller-macro-effects-primary/expression.sus"),
        include_str!("../../../results/m12-eval/caller-macro-effects-primary/primary.stdout"), 7);
}

#[test]
fn actual_macro_eval_preserves_compilation_effects_and_truthy_metadata() {
    let source = include_str!("../../../results/m12-eval/caller-macro-effects-primary/expression.sus")
        .replace("(catch Throwable ", "(catch :default ");
    let forms = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/caller-macro-effects-primary/primary.stdout")).unwrap();
    let wanted = canonical(expected(&forms[0]));
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros.define(&format!("(defmacro native-macro-effects-probe [] (list 'quote {source}))")).unwrap();
        let value = session.eval_with_macros("(native-macro-effects-probe)", &mut macros).unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 80_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), wanted);
    }
}

#[test]
fn native_source_macro_declarations_keep_the_isolated_source_host() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("isolation.sus"), "(ns isolation) (defmacro step [] (swap! user/source-phase-state inc) 42) (def answer (step))").unwrap();
    for macro_phase in [false, true] {
        let options = suss_compile::portable_session::SessionOptions {
            source_paths: vec![root.path().to_owned()], ..Default::default()
        };
        let mut session = if macro_phase { Session::new_macro_with_options(options) } else { Session::new_repl_with_options(options) }.unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros.define("(defmacro source-phase-seed [] (eval '(def source-phase-state (atom 0))) 42)").unwrap();
        session.eval_with_macros("(source-phase-seed)", &mut macros).unwrap();
        session.eval("(def source-phase-state (atom 0))").unwrap();
        session.eval_with_macros("(eval '(+ 0 0))", &mut macros).unwrap();
        session.eval_with_macros("(ns user (:require [isolation])) isolation/answer", &mut macros).unwrap();
        macros.define("(defmacro source-phase-read [] (list 'quote @source-phase-state))").unwrap();
        let value = session.eval_with_macros("[@source-phase-state (source-phase-read)]", &mut macros).unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Vector(vec![Observation::Long(0), Observation::Long(1)]));
    }
}

#[test]
fn native_source_macro_authority_retains_original_compiler_helpers() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        session.eval_with_macros(r#"
          (with-redefs [suss.core/intern-namespace (fn [& xs] (throw :redirected-interner))
                        suss.core/enter-macro-namespace! (fn [& xs] (throw :redirected-setter))
                        suss.core/ex-message (fn [& xs] (throw :redirected-message))]
            (eval '(defmacro native-retained-helper [] 42)))"#, &mut macros).unwrap_or_else(|error| panic!("{}", suss_compile::portable_repl::error_display(&mut session, &error)));
        let value = session.eval_with_macros("(native-retained-helper)", &mut macros).unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::Long(42));
    }
}

#[test]
fn native_source_macro_context_tracks_each_module_without_changing_roots() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("caller.sus"), include_str!("../../../results/m12-eval/native-source-context-primary/caller.sus")).unwrap();
    for macro_phase in [false, true] {
        let options = suss_compile::portable_session::SessionOptions {
            source_paths: vec![root.path().to_owned()], ..Default::default()
        };
        let mut session = if macro_phase { Session::new_macro_with_options(options) } else { Session::new_repl_with_options(options) }.unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        let namespace = session.eval("(fn [] (ns-name *ns*))").unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        let before = session.invoke(&namespace, &[]).unwrap();
        let before = decoder.decode_session(&mut session, &before).unwrap();
        // Accepted namespace contract: the raw native root is suss.core.
        // Source execution binds user or the namespace currently being loaded.
        assert_eq!(before, Observation::Symbol(None, "suss.core".encode_utf16().collect()));
        session.eval_with_macros("(eval '(defmacro native-source-context [] (list 'quote (ns-name *ns*))))", &mut macros).unwrap();
        let value = session.eval_with_macros("(ns user (:require [caller])) caller/answer", &mut macros).unwrap();
        decoder.refill(40_000);
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::Symbol(None, "caller".encode_utf16().collect()));
        let value = session.invoke(&namespace, &[]).unwrap();
        decoder.refill(40_000);
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), before);
        let value = session.eval_with_macros("(ns-name *ns*)", &mut macros).unwrap();
        decoder.refill(40_000);
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::Symbol(None, "user".encode_utf16().collect()));
    }
}

#[test]
fn native_source_macro_reassertion_supersedes_staged_foreign_owner() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("mixed.sus"), include_str!("../../../results/m12-eval/native-source-context-primary/mixed.sus")).unwrap();
    for macro_phase in [false, true] {
        let options = suss_compile::portable_session::SessionOptions {
            source_paths: vec![root.path().to_owned()], ..Default::default()
        };
        let mut session = if macro_phase { Session::new_macro_with_options(options) } else { Session::new_repl_with_options(options) }.unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        session.enter_namespace("mixed").unwrap();
        session.eval_with_macros("(eval '(defmacro shared [] 1))", &mut macros).unwrap();
        session.enter_namespace("user").unwrap();
        session.eval_with_macros("(eval '(defmacro native-reassert-shared [] (eval '(defmacro shared [] 42)) :done))", &mut macros).unwrap();
        let value = session.eval_with_macros("(ns user (:require [mixed])) mixed/answer", &mut macros).unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::Long(42));
    }
}

#[test]
fn isolated_source_macro_fuel_remains_bounded_after_native_eval() {
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let observations = Arc::new(AtomicUsize::new(0));
        let observer = observations.clone();
        macros.install_observation_hook(move |_, _| { observer.fetch_add(1, Ordering::SeqCst); Ok(0) }).unwrap();
        session.eval_with_macros("(eval '(+ 0 0))", &mut macros).unwrap();
        macros.define("(defmacro isolated-source-fuel-loop [] (suss.harness.observation/capture 3) (loop [] (recur)))").unwrap();
        macros.set_operation_fuel(200_000);
        let interrupt = session.interrupt_handle();
        let (done, waiting) = std::sync::mpsc::channel();
        let watchdog = std::thread::spawn(move || {
            if waiting.recv_timeout(std::time::Duration::from_secs(5)).is_err() { interrupt.interrupt(); }
        });
        let start = std::time::Instant::now();
        let error = session.eval_with_macros("(isolated-source-fuel-loop)", &mut macros).unwrap_err();
        let _ = done.send(());
        watchdog.join().unwrap();
        assert!(error.to_string().to_lowercase().contains("fuel"), "{error}");
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        assert!(observations.load(Ordering::SeqCst) > 0, "actual source Macro body must run");
        macros.set_operation_fuel(10_000_000);
        session.set_operation_fuel(10_000_000);
        macros.define("(defmacro isolated-source-fuel-resumed [] 42)").unwrap();
        let value = session.eval_with_macros("(isolated-source-fuel-resumed)", &mut macros).unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::Long(42));
    }
}

#[test]
fn isolated_source_macro_cancellation_returns_its_service_after_native_eval() {
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    let (signal, observed) = std::sync::mpsc::sync_channel(1);
    macros.install_observation_hook(move |_, _| { let _ = signal.try_send(()); Ok(0) }).unwrap();
    session.eval_with_macros("(eval '(+ 0 0))", &mut macros).unwrap();
    macros.define("(defmacro isolated-source-cancel-loop [] (suss.harness.observation/capture 77) (loop [] (recur)))").unwrap();
    session.set_operation_fuel(u64::MAX);
    macros.set_operation_fuel(u64::MAX);
    let interrupt = session.interrupt_handle();
    let controller = std::thread::spawn(move || {
        let ran = observed.recv_timeout(std::time::Duration::from_secs(5)).is_ok();
        interrupt.interrupt();
        ran
    });
    let error = session.eval_with_macros("(isolated-source-cancel-loop)", &mut macros).unwrap_err();
    assert!(controller.join().unwrap(), "actual source Macro body must reach its observer");
    assert!(error.to_string().contains("Interrupted"), "{error}");
    session.set_operation_fuel(10_000_000);
    macros.set_operation_fuel(10_000_000);
    macros.define("(defmacro isolated-source-cancel-resumed [] 42)").unwrap();
    let value = session.eval_with_macros("(isolated-source-cancel-resumed)", &mut macros).unwrap();
    let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
    assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::Long(42));
}

#[test]
fn native_eval_keeps_source_compilation_allowance_separate_from_runtime_fuel() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        session.eval_with_macros("(eval '(+ 0 0))", &mut macros).unwrap();
        session.set_operation_fuel(20_000);
        macros.set_operation_fuel(1_000_000);
        let source = format!("{}42", " ".repeat(50_000));
        let value = session.eval_with_macros(&source, &mut macros).unwrap();
        session.set_operation_fuel(10_000_000);
        macros.set_operation_fuel(10_000_000);
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::Long(42));
    }
}


#[test]
fn namespace_catalog_requires_the_registered_namespace_identity() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let forged = session.eval_privileged("(Namespace. (symbol \"suss.core\") (array))").unwrap();
        let query = session.eval_with_macros("(fn [n] (ns-interns n))", &mut macros).unwrap();
        let error = session.invoke(&query, &[&forged]).unwrap_err();
        assert!(suss_compile::portable_repl::error_display(&mut session, &error)
            .contains("registered canonical Namespace"));
        let value = session.eval_with_macros("(let [n (:ns (meta #'identity))] (identical? n (the-ns (ns-name n))))", &mut macros).unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::Bool(true));
    }
}

#[test]
fn native_eval_def_metadata_obeys_primary_root_and_publication_boundaries() {
    promotion_review_controls("generated-def-metadata",
        include_str!("../../../results/m12-eval/analyzed-var-native-metadata-primary/expression.sus"),
        include_str!("../../../results/m12-eval/analyzed-var-native-metadata-primary/primary.stdout"), 3);
    promotion_review_controls("failed-def-metadata",
        include_str!("../../../results/m12-eval/analyzed-var-native-metadata-primary/failure-expression.sus"),
        include_str!("../../../results/m12-eval/analyzed-var-native-metadata-primary/failure-vector-primary.stdout"), 2);
}

#[test]
fn namespace_catalog_core_functions_reuse_live_var_cells() {
    promotion_review_controls("core-function-catalog",
        include_str!("../../../results/m12-eval/namespace-core-functions-primary/expression.sus"),
        include_str!("../../../results/m12-eval/namespace-core-functions-primary/primary.stdout"), 3);
}

#[test]
fn native_eval_namespace_catalog_preserves_analysis_interning_boundaries() {
    let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/namespace-declaration-boundaries-primary/cases.json"
    )).unwrap();
    let primary = suss_reader::forms::read_forms(include_str!(
        "../../../results/m12-eval/namespace-declaration-boundaries-primary/primary.stdout"
    )).unwrap();
    let Kind::Vector(rows) = &primary[0].kind else { panic!("primary vector") };
    assert_eq!(cases.len(), 5);
    assert_eq!(rows.len(), cases.len());
    let mut failures = Vec::new();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        for (case, required) in cases.iter().zip(rows) {
            let name = case["name"].as_str().unwrap();
            let body = case["expression"].as_str().unwrap().replace("(catch Throwable ", "(catch :default ");
            let program = format!("(eval '{body})");
            match session.eval_with_macros(&program, &mut macros) {
                Ok(value) => {
                    session.collect().unwrap();
                    decoder.refill(40_000);
                    let actual = canonical(decoder.decode_session(&mut session, &value).unwrap());
                    let required = canonical(expected(required));
                    if actual == required { eprintln!("namespace-boundary-primary-match {phase:?} {name}"); }
                    else { failures.push(format!("{phase:?} {name}: {actual:?}, expected {required:?}")); }
                }
                Err(error) => failures.push(format!("{phase:?} {name}: {}", suss_compile::portable_repl::error_display(&mut session, &error))),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn namespace_catalog_returns_actual_user_core_and_macro_vars() {
    let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/namespace-interns-primary/cases.json"
    )).unwrap();
    let primary = suss_reader::forms::read_forms(include_str!(
        "../../../results/m12-eval/namespace-interns-primary/primary.stdout"
    )).unwrap();
    let Kind::Vector(rows) = &primary[0].kind else { panic!("primary vector") };
    assert_eq!(cases.len(), 2);
    assert_eq!(rows.len(), cases.len());
    let mut failures = Vec::new();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        for (case, required) in cases.iter().zip(rows) {
            let name = case["name"].as_str().unwrap();
            match session.eval_with_macros(case["expression"].as_str().unwrap(), &mut macros) {
                Ok(value) => {
                    session.collect().unwrap();
                    decoder.refill(40_000);
                    let actual = canonical(decoder.decode_session(&mut session, &value).unwrap());
                    let required = canonical(expected(required));
                    if actual == required { eprintln!("namespace-catalog-primary-match {phase:?} {name}"); }
                    else { failures.push(format!("{phase:?} {name}: {actual:?}, expected {required:?}")); }
                }
                Err(error) => failures.push(format!("{phase:?} {name}: {}", suss_compile::portable_repl::error_display(&mut session, &error))),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn isolated_source_macros_preserve_nested_immutable_metadata_aliases() {
    let primary = suss_reader::forms::read_forms(include_str!(
        "../../../results/m12-eval/immutable-native-graph-primary/primary.stdout"
    )).unwrap();
    let Kind::Vector(rows) = &primary[0].kind else { panic!("primary vector") };
    assert_eq!(rows.len(), 9);
    let wanted = canonical(expected(&primary[0]));
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        // Compile the transformer in the isolated service, then pass actual
        // caller functions and captures through public eval's form-data path.
        macros.define(include_str!(
            "../../../results/m12-eval/immutable-native-graph-primary/setup.sus"
        )).unwrap();
        let value = session.eval_with_macros(include_str!(
            "../../../results/m12-eval/immutable-native-graph-primary/expression.sus"
        ), &mut macros).unwrap_or_else(|error| panic!("{}", suss_compile::portable_repl::error_display(&mut session, &error)));
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), wanted,
            "{:?} isolated source metadata aliases", session.phase());
    }
}

#[test]
fn isolated_source_macros_preserve_actual_function_var_and_namespace_values() {
    let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/isolated-source-call-primary/cases.json"
    )).unwrap();
    let primary = suss_reader::forms::read_forms(include_str!(
        "../../../results/m12-eval/isolated-source-call-primary/primary.stdout"
    )).unwrap();
    let Kind::Vector(expected_rows) = &primary[0].kind else { panic!("primary vector") };
    assert_eq!(cases.len(), 20);
    assert_eq!(expected_rows.len(), cases.len());
    let mut failures = Vec::new();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        // These must be true isolated source transformers. Defining them with
        // caller eval would sidestep the Store boundary this regression tests.
        let setup = include_str!("../../../results/m12-eval/isolated-source-call-primary/setup.sus");
        for declaration in suss_reader::forms::read_forms(setup).unwrap() {
            if let Err(error) = macros.define(&setup[declaration.span.clone()]) {
                failures.push(format!("{phase:?} source declaration at {:?}: {error}", declaration.span));
            }
        }
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        let mut retained = Vec::new();
        for (row, expected_row) in cases.iter().zip(expected_rows) {
            let name = row["name"].as_str().unwrap();
            let program = row["expression"].as_str().unwrap().replace("(catch Throwable ", "(catch :default ");
            match session.eval_with_macros(&program, &mut macros) {
                Ok(value) => {
                    decoder.refill(20_000);
                    match decoder.decode_session(&mut session, &value) {
                        Ok(observed) => {
                            let observed = canonical(observed);
                            let required = canonical(expected(expected_row));
                            if observed == required {
                                eprintln!("isolated-source-primary-match {phase:?} {name}");
                                retained.push((value, expected_row, name));
                            } else { failures.push(format!("{phase:?} {name}: {observed:?}, expected {required:?}")); }
                        }
                        Err(error) => failures.push(format!("{phase:?} {name}: {error:?}")),
                    }
                }
                Err(error) => failures.push(format!("{phase:?} {name}: {}", suss_compile::portable_repl::error_display(&mut session, &error))),
            }
        }
        session.collect().unwrap();
        for (value, expected_row, name) in retained {
            decoder.refill(20_000);
            assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), canonical(expected(expected_row)), "{phase:?} {name} after GC");
        }
    }
    assert!(failures.is_empty(), "isolated source value mismatches:\n{}", failures.join("\n"));
}

#[test]
fn isolated_source_callback_preserves_typed_cancellation_and_recovers() {
    use std::{sync::{Mutex, mpsc}, thread, time::Duration};
    use suss_compile::portable_session::SessionError;
    {
        let mut session = Session::new_repl().unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros.define("(defmacro isolated-callback-cancel [f] (list 'quote (f)))").unwrap();
        let (entered, waiting) = mpsc::sync_channel(1);
        let (requested, resume) = mpsc::sync_channel(1);
        let resume = Mutex::new(resume);
        session.install_observation_hook(move |_, _| {
            entered.send(()).map_err(|e| wasmtime::Error::msg(e.to_string()))?;
            resume.lock().unwrap().recv_timeout(Duration::from_secs(5))
                .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
            Ok(0)
        }).unwrap();
        let function = session.eval_with_macros(
            "(fn [] (try (eval (list 'user/isolated-callback-cancel (fn [] (suss.harness.observation/capture 1) (loop [] (recur))))) (catch :default e [:incorrectly-caught (ex-message e)])))",
            &mut macros).unwrap();
        let interrupt = session.interrupt_handle();
        let controller = thread::spawn(move || {
            let observed = waiting.recv_timeout(Duration::from_secs(5)).is_ok();
            interrupt.interrupt();
            let _ = requested.send(());
            observed
        });
        let result = session.invoke(&function, &[]);
        let observed = controller.join().unwrap();
        if !observed {
            let detail = match &result {
                Err(error) => suss_compile::portable_repl::error_display(&mut session, error),
                Ok(value) => {
                    let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
                    format!("{:?}", decoder.decode_session(&mut session, value))
                }
            };
            panic!("actual source callback must execute before cancellation: {detail}");
        }
        let error = result.expect_err("cancellation must remain uncatchable through the source macro");
        assert!(matches!(&error, SessionError::Trap(error)
            if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::Interrupt)),
            "callback cancellation must retain the typed interrupt: {error}");
        let recovered = session.eval_with_macros("(eval '(+ 40 2))", &mut macros).unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        assert_eq!(decoder.decode_session(&mut session, &recovered).unwrap(), Observation::Long(42));
    }
}

#[test]
fn isolated_source_function_metadata_retains_identity_between_macro_requests() {
    let setup = include_str!("../../../results/m12-eval/function-metadata-map-identity-primary/setup.sus");
    let expected = expected(&suss_reader::forms::read_forms(include_str!(
        "../../../results/m12-eval/function-metadata-map-identity-primary/primary.stdout"
    )).unwrap()[0]);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        for form in suss_reader::forms::read_forms(setup).unwrap() {
            macros.define(&setup[form.span.clone()]).unwrap();
        }
        let value = session.eval_with_macros(
            "(let [f (with-meta (fn [] 42) {:tag :kept})] (eval (list 'identity-remember f)) (eval (list 'identity-check f)))",
            &mut macros).unwrap();
        for _ in 0..2 {
            let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
            assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), expected,
                "source-held functions and their actual metadata retain identity across requests");
            session.collect().unwrap();
        }
    }
}

#[test]
fn isolated_source_nested_function_metadata_retains_actual_callbacks() {
    let setup = include_str!("../../../results/m12-eval/nested-function-metadata-primary/setup.sus");
    let query = include_str!("../../../results/m12-eval/nested-function-metadata-primary/query.sus");
    let required = expected(&suss_reader::forms::read_forms(include_str!(
        "../../../results/m12-eval/nested-function-metadata-primary/primary.stdout"
    )).unwrap()[0]);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        for declaration in suss_reader::forms::read_forms(setup).unwrap() {
            macros.define(&setup[declaration.span.clone()]).unwrap();
        }
        let value = session.eval_with_macros(query, &mut macros).unwrap_or_else(|error| {
            panic!("{phase:?}: {}", suss_compile::portable_repl::error_display(&mut session, &error))
        });
        for _ in 0..2 {
            let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
            assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), required,
                "{phase:?}: nested metadata aliases must preserve the actual callback and its captured effects");
            session.collect().unwrap();
        }
    }
}

#[test]
fn native_eval_unbound_values_retain_actual_var_identity_and_callable_behavior() {
    promotion_review_controls("unbound-values",
        include_str!("../../../results/m12-eval/unbound-values-behavior-primary/expression.sus"),
        include_str!("../../../results/m12-eval/unbound-values-behavior-primary/primary.stdout"), 4);
}

#[test]
fn native_eval_alter_var_root_preserves_actual_root_and_failure_boundaries() {
    promotion_review_controls("alter-var-root",
        include_str!("../../../results/m12-eval/alter-var-root-primary/expression.sus"),
        include_str!("../../../results/m12-eval/alter-var-root-primary/primary.stdout"), 4);
}

#[test]
fn isolated_source_callbacks_preserve_real_unbound_values_and_root_writes() {
    let source = include_str!("../../../results/m12-eval/isolated-unbound-primary/expression.sus");
    let setup = include_str!("../../../results/m12-eval/isolated-unbound-primary/setup.sus");
    let forms = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/isolated-unbound-primary/primary.stdout")).unwrap();
    assert_eq!(forms.len(), 1);
    let wanted = canonical(expected(&forms[0]));
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        macros.define(setup).unwrap();
        let source = source.replace("(catch Throwable ", "(catch :default ");
        let value = session.eval_with_macros(&source, &mut macros)
            .unwrap_or_else(|error| panic!("{phase:?}: {}", suss_compile::portable_repl::error_display(&mut session, &error)));
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), wanted, "{phase:?}");
    }
}

#[test]
fn source_macro_var_roots_match_primary_identity_replacement_and_namespace_context() {
    let source = include_str!("../../../results/m12-eval/macro-root-projection-primary/expression.sus");
    let primary = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/macro-root-projection-primary/primary.stdout")).unwrap();
    let input = suss_reader::forms::read_forms(source).unwrap();
    let Kind::Vector(cases) = &input[0].kind else { panic!("input vector") };
    let Kind::Vector(rows) = &primary[0].kind else { panic!("primary vector") };
    assert_eq!(cases.len(), 5);
    assert_eq!(rows.len(), cases.len());
    let mut failures = Vec::new();
    for (index, (case, required)) in cases.iter().zip(rows).enumerate() {
      for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        macros.define(include_str!("../../../results/m12-eval/macro-root-projection-primary/setup.sus")).unwrap();
        // Match the primary's existing caller namespace without requiring a
        // new public create-ns adapter or sharing any Namespace marker value.
        session.eval_with_macros("(ns projected.caller)", &mut macros).unwrap();
        session.eval_with_macros("(ns user)", &mut macros).unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
            let program = &source[case.span.clone()];
            match session.eval_with_macros(program, &mut macros) {
                Ok(value) => {
                    session.collect().unwrap();
                    decoder.refill(40_000);
                    let actual = canonical(decoder.decode_session(&mut session, &value).unwrap());
                    let required = canonical(expected(required));
                    if actual == required { eprintln!("macro-root-primary-match {phase:?} {index}"); }
                    else { failures.push(format!("{phase:?} macro root {index}: actual {actual:?}, expected {required:?}")); }
                }
                Err(error) => failures.push(format!("{phase:?} macro root {index}: {}", suss_compile::portable_repl::error_display(&mut session, &error))),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}


#[test]
fn source_macro_function_namespace_preserves_unused_and_non_namespace_values() {
    let setup = include_str!("../../../results/m12-eval/function-namespace-boundaries-primary/setup.sus");
    let source = include_str!("../../../results/m12-eval/function-namespace-boundaries-primary/expression.sus");
    let forms = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/function-namespace-boundaries-primary/primary.stdout")).unwrap();
    assert_eq!(forms.len(), 1);
    let wanted = canonical(expected(&forms[0]));
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        for declaration in suss_reader::forms::read_forms(setup).unwrap() {
            macros.define(&setup[declaration.span.clone()]).unwrap();
        }
        let value = session.eval_with_macros(source, &mut macros).unwrap_or_else(|error| {
            panic!("{phase:?}: {}", suss_compile::portable_repl::error_display(&mut session, &error))
        });
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), wanted, "{phase:?}");
    }
}

#[test]
fn isolated_source_lazy_sequences_preserve_realization_effects_and_cache_after_gc() {
    let setup = include_str!("../../../results/m12-eval/live-sequence-effects-primary/setup.sus");
    let forms = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/live-sequence-effects-primary/primary.stdout")).unwrap();
    let Kind::Vector(rows) = &forms[0].kind else { panic!("primary vector") };
    assert_eq!(rows.len(), 2);
    let Kind::Vector(retained_rows) = &rows[1].kind else { panic!("retained vector") };
    assert_eq!(retained_rows.len(), 2);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        macros.define(setup).unwrap();
        let simple = session.eval_with_macros(include_str!("../../../results/m12-eval/live-sequence-effects-primary/simple.sus"), &mut macros).unwrap_or_else(|error| {
            panic!("{phase:?} simple: {}", suss_compile::portable_repl::error_display(&mut session, &error))
        });
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &simple).unwrap()), canonical(expected(&rows[0])), "{phase:?} simple");
        let observer = session.eval_with_macros(include_str!("../../../results/m12-eval/live-sequence-effects-primary/retained.sus"), &mut macros).unwrap_or_else(|error| {
            panic!("{phase:?} retained: {}", suss_compile::portable_repl::error_display(&mut session, &error))
        });
        for (index, required) in retained_rows.iter().enumerate() {
            session.collect().unwrap();
            let observed = session.invoke(&observer, &[]).unwrap_or_else(|error| panic!("{phase:?} invocation {index}: {error}"));
            session.collect().unwrap();
            decoder.refill(40_000);
            assert_eq!(canonical(decoder.decode_session(&mut session, &observed).unwrap()), canonical(expected(required)), "{phase:?} invocation {index}");
        }
    }
}


#[test]
fn namespace_catalog_includes_source_macro_vars_without_prior_var_references() {
    let setup = include_str!("../../../results/m12-eval/catalog-without-macro-reference-primary/setup.sus");
    let source = include_str!("../../../results/m12-eval/catalog-without-macro-reference-primary/expression.sus");
    let forms = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/catalog-without-macro-reference-primary/primary.stdout")).unwrap();
    assert_eq!(forms.len(), 1);
    let wanted = canonical(expected(&forms[0]));
    let mut failures = Vec::new();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        macros.define(setup).unwrap();
        match session.eval_with_macros(source, &mut macros) {
            Ok(value) => {
                session.collect().unwrap();
                let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
                let actual = canonical(decoder.decode_session(&mut session, &value).unwrap());
                if actual != wanted { failures.push(format!("{phase:?}: {actual:?}, expected {wanted:?}")); }
            }
            Err(error) => failures.push(format!("{phase:?}: {}", suss_compile::portable_repl::error_display(&mut session, &error))),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn staged_source_scripts_publish_real_macro_var_roots_after_linking() {
    let source = include_str!("../../../results/m12-eval/source-link-macro-vars-primary/script.sus");
    let primary = include_str!("../../../results/m12-eval/source-link-macro-vars-primary/primary.stdout");
    let mut failures = Vec::new();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        match suss_compile::portable_repl::evaluate_script_compiled(&mut session, &mut macros, source, None) {
            Ok(actual) if actual.trim() == primary.trim() => {}
            Ok(actual) => failures.push(format!("{phase:?}: {actual}, expected {primary}")),
            Err(error) => failures.push(format!("{phase:?}: {}", suss_compile::portable_repl::error_display(&mut session, &error))),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn staged_source_modules_publish_real_macro_var_roots_after_linking() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("staged_vars.sus"), include_str!("../../../results/m12-eval/source-link-macro-vars-primary/staged_vars.sus")).unwrap();
    let forms = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/source-link-macro-vars-primary/primary.stdout")).unwrap();
    let wanted = canonical(expected(&forms[0]));
    let mut failures = Vec::new();
    for macro_phase in [false, true] {
        let options = suss_compile::portable_session::SessionOptions {
            source_paths: vec![root.path().to_owned()], ..Default::default()
        };
        let mut session = if macro_phase { Session::new_macro_with_options(options) } else { Session::new_repl_with_options(options) }.unwrap();
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        match session.eval_with_macros("(ns user (:require [staged-vars])) staged-vars/result", &mut macros) {
            Ok(value) => {
                session.collect().unwrap();
                let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
                let actual = canonical(decoder.decode_session(&mut session, &value).unwrap());
                if actual != wanted { failures.push(format!("{phase:?}: {actual:?}, expected {wanted:?}")); }
            }
            Err(error) => failures.push(format!("{phase:?}: {}", suss_compile::portable_repl::error_display(&mut session, &error))),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}


#[test]
fn staged_source_redefinitions_preserve_var_identity_and_reject_unlinked_roots() {
    let first = include_str!("../../../results/m12-eval/source-link-redefinition-ordered-primary/first.sus");
    let second = include_str!("../../../results/m12-eval/source-link-redefinition-ordered-primary/second.sus");
    let forms = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/source-link-redefinition-ordered-primary/primary.stdout")).unwrap();
    let Kind::Vector(rows) = &forms[0].kind else { panic!("primary vector") };
    assert_eq!(rows.len(), 2);
    let mut failures = Vec::new();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        for (index, (source, required)) in [first, second].into_iter().zip(rows).enumerate() {
            if index == 1 {
                session.eval_with_macros(include_str!("../../../results/m12-eval/source-link-redefinition-ordered-primary/native-change.sus"), &mut macros).unwrap();
            }
            suss_compile::portable_repl::evaluate_script_compiled(&mut session, &mut macros, source, None)
                .unwrap_or_else(|error| panic!("{phase:?} stage {index}: {}", suss_compile::portable_repl::error_display(&mut session, &error)));
            let input = suss_reader::forms::read_forms(source).unwrap();
            let query = &source[input.last().unwrap().span.clone()];
            let value = session.eval_with_macros(query, &mut macros).unwrap();
            session.collect().unwrap();
            let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
            assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), canonical(expected(required)), "{phase:?} stage {index}");
        }
        // Compiled source prepares every unit before linking or Runtime work.
        // This checks its declaration journal, distinct from JVM load-string.
        suss_compile::portable_repl::evaluate_script_compiled(&mut session, &mut macros,
            include_str!("../../../results/m12-eval/source-link-redefinition-ordered-primary/rejected.sus"), None).unwrap_err();
        let input = suss_reader::forms::read_forms(second).unwrap();
        let query = &second[input.last().unwrap().span.clone()];
        let value = session.eval_with_macros(query, &mut macros).unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), canonical(expected(&rows[1])), "{phase:?} rejected source retains linked root and metadata");
        // A fresh caller must see the restored source declaration too, rather
        // than benefiting only from the earlier caller's retained canonical Var.
        let mut fresh = if phase == suss_compile::portable::resolve::Phase::Macro {
            Session::new_macro().unwrap()
        } else { Session::new_repl().unwrap() };
        let value = fresh.eval_with_macros("(let [v (get (ns-interns 'user) 'staged-reload)] [((deref v) nil nil) (:doc (meta v))])", &mut macros).unwrap();
        fresh.collect().unwrap();
        let mut decoder = Decoder::capture(&mut fresh, 40_000).unwrap();
        let Kind::Vector(required) = &rows[1].kind else { panic!("second row") };
        let wanted = Observation::Vector(vec![expected(&required[1]), expected(&required[2])]);
        let actual = decoder.decode_session(&mut fresh, &value).unwrap();
        if actual != wanted { failures.push(format!("{phase:?} fresh caller after rejection: {actual:?}, expected {wanted:?}")); }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}


#[test]
fn forwarded_source_functions_retain_actual_current_namespace_identity() {
    let setup = include_str!("../../../results/m12-eval/function-namespace-identity-primary/setup.sus");
    let source = include_str!("../../../results/m12-eval/function-namespace-identity-primary/expression.sus");
    let forms = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/function-namespace-identity-primary/primary.stdout")).unwrap();
    let wanted = canonical(expected(&forms[0]));
    let mut failures = Vec::new();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        for declaration in suss_reader::forms::read_forms(setup).unwrap() {
            macros.define(&setup[declaration.span.clone()]).unwrap();
        }
        let value = session.eval_with_macros(source, &mut macros).unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        let actual = canonical(decoder.decode_session(&mut session, &value).unwrap());
        if actual != wanted { failures.push(format!("{phase:?}: {actual:?}, expected {wanted:?}")); }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn public_namespace_loaders_publish_canonical_source_macro_vars_and_reload_roots() {
    let root = tempfile::tempdir().unwrap();
    let first = include_str!("../../../results/m12-eval/public-loader-source-vars-primary/first.sus");
    let second = include_str!("../../../results/m12-eval/public-loader-source-vars-primary/second.sus");
    let forms = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/public-loader-source-vars-primary/primary.stdout")).unwrap();
    let Kind::Vector(rows) = &forms[0].kind else { panic!("primary vector") };
    assert_eq!(rows.len(), 3);
    let mut failures = Vec::new();
    for macro_phase in [false, true] {
        std::fs::write(root.path().join("staged_vars.sus"), first).unwrap();
        let options = suss_compile::portable_session::SessionOptions {
            source_paths: vec![root.path().to_owned()], ..Default::default()
        };
        let mut session = if macro_phase { Session::new_macro_with_options(options) } else { Session::new_repl_with_options(options) }.unwrap();
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        if let Err(error) = session.load_namespace_with_macros("staged-vars", &mut macros) {
            failures.push(format!("{phase:?} load: {}", suss_compile::portable_repl::error_display(&mut session, &error)));
            continue;
        }
        let value = session.eval_with_macros("staged-vars/result", &mut macros).unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), canonical(expected(&rows[0])), "{phase:?} first load");
        let held = session.eval_with_macros("(var staged-vars/staged-var)", &mut macros).unwrap();
        std::fs::write(root.path().join("staged_vars.sus"), second).unwrap();
        if let Err(error) = session.reload_namespace_with_macros("staged-vars", false, &mut macros) {
            failures.push(format!("{phase:?} reload: {}", suss_compile::portable_repl::error_display(&mut session, &error)));
            continue;
        }
        session.collect().unwrap();
        let value = session.eval_with_macros("staged-vars/result", &mut macros).unwrap();
        decoder.refill(40_000);
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), canonical(expected(&rows[1])), "{phase:?} reload");
        let current = session.eval_with_macros("(var staged-vars/staged-var)", &mut macros).unwrap();
        let identity = session.eval_with_macros("(fn [a b] (identical? a b))", &mut macros).unwrap();
        let same = session.invoke(&identity, &[&held, &current]).unwrap();
        decoder.refill(40_000);
        assert_eq!(decoder.decode_session(&mut session, &same).unwrap(), expected(&rows[2]), "{phase:?} reload preserves Var");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn source_declaration_rollback_preserves_later_metadata_effects() {
    let base = include_str!("../../../results/m12-eval/source-journal-metadata-effect-primary/base.sus");
    let rejected = include_str!("../../../results/m12-eval/source-journal-metadata-effect-primary/native-rejected.sus");
    let forms = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/source-journal-metadata-effect-primary/primary.stdout")).unwrap();
    let wanted = expected(&forms[0]);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        suss_compile::portable_repl::evaluate_script_compiled(&mut session, &mut macros, base, None).unwrap();
        suss_compile::portable_repl::evaluate_script_compiled(&mut session, &mut macros, rejected, None).unwrap_err();
        let mut fresh = if phase == suss_compile::portable::resolve::Phase::Macro {
            Session::new_macro().unwrap()
        } else { Session::new_repl().unwrap() };
        let value = fresh.eval_with_macros("(:doc (meta (get (ns-interns 'user) 'staged-reload)))", &mut macros).unwrap();
        fresh.collect().unwrap();
        let mut decoder = Decoder::capture(&mut fresh, 40_000).unwrap();
        assert_eq!(decoder.decode_session(&mut fresh, &value).unwrap(), wanted, "{phase:?}: retain subsequent metadata effect rather than restoring compiler declaration metadata");
    }
}

#[test]
fn source_macro_import_reload_updates_retained_canonical_var_and_preserves_old_function() {
    let root = tempfile::tempdir().unwrap();
    let first = include_str!("../../../results/m12-eval/imported-macro-reload-primary/first.sus");
    let second = include_str!("../../../results/m12-eval/imported-macro-reload-primary/second.sus");
    let setup = include_str!("../../../results/m12-eval/imported-macro-reload-primary/setup.sus");
    let expression = include_str!("../../../results/m12-eval/imported-macro-reload-primary/expression.sus");
    let primary = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/imported-macro-reload-primary/primary.stdout")).unwrap();
    assert_eq!(primary.len(), 2);
    let mut failures = Vec::new();
    for macro_phase in [false, true] {
        std::fs::write(root.path().join("tools.sus"), first).unwrap();
        let options = suss_compile::portable_session::SessionOptions {
            source_paths: vec![root.path().to_owned()], ..Default::default()
        };
        let mut session = if macro_phase { Session::new_macro_with_options(options) }
            else { Session::new_repl_with_options(options) }.unwrap();
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        session.eval_with_macros(setup, &mut macros).unwrap();
        for (index, wanted) in primary.iter().enumerate() {
            if index == 1 {
                std::fs::write(root.path().join("tools.sus"), second).unwrap();
                session.eval_with_macros("(ns user (:require-macros ^{:reload :reload} [tools :as t]))", &mut macros).unwrap();
            }
            let value = session.eval_with_macros(expression, &mut macros).unwrap();
            session.collect().unwrap();
            let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
            let actual = canonical(decoder.decode_session(&mut session, &value).unwrap());
            let wanted = canonical(expected(wanted));
            if actual != wanted { failures.push(format!("{phase:?} stage{index}: {actual:?}, expected {wanted:?}")); }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn source_macro_imports_preserve_once_loading_completed_prefixes_and_reload_recovery() {
    let root = tempfile::tempdir().unwrap();
    let first = include_str!("../../../results/m12-eval/imported-macro-reload-prefix-primary/first.sus");
    let second = include_str!("../../../results/m12-eval/imported-macro-reload-prefix-primary/second.sus");
    let rejected = include_str!("../../../results/m12-eval/imported-macro-reload-prefix-primary/rejected.sus");
    let repaired = include_str!("../../../results/m12-eval/imported-macro-reload-prefix-primary/repaired.sus");
    let setup = include_str!("../../../results/m12-eval/imported-macro-reload-prefix-primary/setup.sus");
    let expression = include_str!("../../../results/m12-eval/imported-macro-reload-prefix-primary/expression.sus");
    let primary = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/imported-macro-reload-prefix-primary/primary.stdout")).unwrap();
    assert_eq!(primary.len(), 5);
    let mut failures = Vec::new();
    for macro_phase in [false, true] {
        let file = root.path().join("tools.sus");
        std::fs::write(&file, first).unwrap();
        let options = suss_compile::portable_session::SessionOptions {
            source_paths: vec![root.path().to_owned()], ..Default::default()
        };
        let mut session = if macro_phase { Session::new_macro_with_options(options) }
            else { Session::new_repl_with_options(options) }.unwrap();
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        session.eval_with_macros(setup, &mut macros).unwrap();
        for (index, wanted) in primary.iter().enumerate() {
            match index {
                0 => {}
                1 => {
                    std::fs::write(&file, second).unwrap();
                    session.eval_with_macros("(ns user (:require-macros [tools :as t]))", &mut macros).unwrap();
                }
                2 | 4 => {
                    if index == 4 { std::fs::write(&file, repaired).unwrap(); }
                    session.eval_with_macros("(ns user (:require-macros ^{:reload :reload} [tools :as t]))", &mut macros).unwrap();
                }
                3 => {
                    std::fs::write(&file, rejected).unwrap();
                    let error = session.eval_with_macros("(ns user (:require-macros ^{:reload :reload} [tools :as t]))", &mut macros).unwrap_err();
                    assert!(error.to_string().contains("missing-after-completed-macro"), "{phase:?}: {error}");
                }
                _ => unreachable!(),
            }
            let value = session.eval_with_macros(expression, &mut macros).unwrap();
            session.collect().unwrap();
            let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
            let actual = canonical(decoder.decode_session(&mut session, &value).unwrap());
            let wanted = canonical(expected(wanted));
            if actual != wanted { failures.push(format!("{phase:?} stage{index}: {actual:?}, expected {wanted:?}")); }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn completed_macro_imports_supersede_rejected_staged_declarations_in_fresh_callers() {
    let root = tempfile::tempdir().unwrap();
    let first = include_str!("../../../results/m12-eval/imported-macro-reload-primary/first.sus");
    let second = include_str!("../../../results/m12-eval/imported-macro-reload-primary/second.sus");
    for macro_phase in [false, true] {
      for rejected in [
        r#"(ns tools) (defmacro answer "staged" [] 99) (ns user (:require-macros ^{:reload :reload} [tools :as t])) missing-after-import"#,
        r#"(ns tools) (defmacro spare [] nil) (ns user (:require-macros ^{:reload :reload} [tools :as t])) (ns tools) (defmacro answer "rejected-after-import" [] 99) (ns user) missing-after-import"#,
      ] {
        std::fs::write(root.path().join("tools.sus"), first).unwrap();
        let options = suss_compile::portable_session::SessionOptions {
            source_paths: vec![root.path().to_owned()], ..Default::default()
        };
        let mut session = if macro_phase { Session::new_macro_with_options(options.clone()) }
            else { Session::new_repl_with_options(options.clone()) }.unwrap();
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        session.eval_with_macros("(ns user (:require-macros [tools :as t]))", &mut macros).unwrap();
        std::fs::write(root.path().join("tools.sus"), second).unwrap();
        // Compiled source declarations are staged. Required namespaces that
        // really finish loading remain published after a later source error,
        // including when they replace the same staged Macro cell.
        let error = suss_compile::portable_repl::evaluate_script_compiled(&mut session, &mut macros,
            rejected, None).unwrap_err();
        assert!(error.to_string().contains("missing-after-import"), "{phase:?}: {error}");
        let mut fresh = if macro_phase { Session::new_macro_with_options(options) }
            else { Session::new_repl_with_options(options) }.unwrap();
        let value = fresh.eval_with_macros("(ns user (:require-macros [tools :as t])) [((deref #'t/answer) nil nil) (:doc (meta #'t/answer)) (eval '(t/answer))]", &mut macros).unwrap();
        fresh.collect().unwrap();
        let mut decoder = Decoder::capture(&mut fresh, 40_000).unwrap();
        assert_eq!(decoder.decode_session(&mut fresh, &value).unwrap(), Observation::Vector(vec![Observation::Long(23), Observation::String("second".encode_utf16().collect()), Observation::Long(23)]), "{phase:?}: retain completed dependency rather than the rejected staged binding");
      }
    }
}

#[test]
fn root_audit_staged_source_preserves_actual_macro_root_effect_before_later_failure() {
    let cases = [
        ("replacement",
         include_str!("../../../results/m12-eval/staged-macro-root-effect-primary/effect.sus"),
         include_str!("../../../results/m12-eval/staged-macro-root-effect-primary/rejected.sus"),
         include_str!("../../../results/m12-eval/staged-macro-root-effect-primary/primary.stdout")),
        ("identical",
         include_str!("../../../results/m12-eval/staged-macro-root-identity-primary/effect.sus"),
         include_str!("../../../results/m12-eval/staged-macro-root-identity-primary/rejected.sus"),
         include_str!("../../../results/m12-eval/staged-macro-root-identity-primary/primary.stdout")),
        ("metadata",
         include_str!("../../../results/m12-eval/staged-macro-metadata-root-primary/effect.sus"),
         include_str!("../../../results/m12-eval/staged-macro-metadata-root-primary/rejected.sus"),
         include_str!("../../../results/m12-eval/staged-macro-metadata-root-primary/primary.stdout")),
    ];
    let expression = include_str!("../../../results/m12-eval/staged-macro-root-effect-primary/expression.sus");
    let mut failures = Vec::new();
    for (label, effects, rejected, primary) in cases {
      let primary = suss_reader::forms::read_forms(primary).unwrap();
      let wanted = canonical(expected(&primary[0]));
      for restage in [false, true] {
       for macro_phase in [false, true] {
        let mut macros = CompiledMacros::new().unwrap();
        macros.enter_namespace("tools").unwrap();
        macros.define("(defmacro answer [] 17)").unwrap();
        if !effects.is_empty() { macros.define(effects).unwrap(); }
        let mut session = if macro_phase { Session::new_macro() } else { Session::new_repl() }.unwrap();
        let phase = session.phase();
        // The source compiler stages declarations independently from the JVM
        // load-string boundary. Any additional unlinked declaration must undo
        // to the completed runtime effect, whose value the primary records.
        let rejected = if restage { rejected.replace("missing-after-root-effect", "(defmacro answer [] 77) missing-after-root-effect") } else { rejected.to_owned() };
        let error = suss_compile::portable_repl::evaluate_script_compiled(&mut session, &mut macros, &rejected, None).unwrap_err();
        assert!(error.to_string().contains("missing-after-root-effect"), "{phase:?} {label}: {error}");
        let mut fresh = if macro_phase { Session::new_macro() } else { Session::new_repl() }.unwrap();
        fresh.eval_with_macros("(ns tools)", &mut macros).unwrap();
        fresh.eval_with_macros("(ns user)", &mut macros).unwrap();
        let value = fresh.eval_with_macros(expression, &mut macros).unwrap();
        fresh.collect().unwrap();
        let mut decoder = Decoder::capture(&mut fresh, 40_000).unwrap();
        let actual = canonical(decoder.decode_session(&mut fresh, &value).unwrap());
        if actual != wanted { failures.push(format!("{phase:?} {label} restage={restage}: {actual:?}, expected {wanted:?}")); }
       }
      }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn root_audit_macro_reload_to_ordinary_definition_updates_retained_var_root_and_status() {
    let root = tempfile::tempdir().unwrap();
    let first = include_str!("../../../results/m12-eval/macro-to-ordinary-reload-primary/first.sus");
    let ordinary = include_str!("../../../results/m12-eval/macro-to-ordinary-reload-primary/ordinary.sus");
    let setup = include_str!("../../../results/m12-eval/macro-to-ordinary-reload-primary/setup.sus");
    let expression = include_str!("../../../results/m12-eval/macro-to-ordinary-reload-primary/expression.sus");
    let primary = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/macro-to-ordinary-reload-primary/primary.stdout")).unwrap();
    assert_eq!(primary.len(), 2);
    let mut failures = Vec::new();
    for macro_phase in [false, true] {
        let file = root.path().join("tools.sus");
        std::fs::write(&file, first).unwrap();
        let options = suss_compile::portable_session::SessionOptions { source_paths: vec![root.path().to_owned()], ..Default::default() };
        let mut session = if macro_phase { Session::new_macro_with_options(options) } else { Session::new_repl_with_options(options) }.unwrap();
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        session.eval_with_macros(setup, &mut macros).unwrap();
        for (index, expected_row) in primary.iter().enumerate() {
            if index == 1 {
                std::fs::write(&file, ordinary).unwrap();
                session.eval_with_macros("(ns user (:require-macros ^{:reload :reload} [tools :as t]))", &mut macros).unwrap();
            }
            let value = session.eval_with_macros(expression, &mut macros).unwrap();
            session.collect().unwrap();
            let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
            let actual = canonical(decoder.decode_session(&mut session, &value).unwrap());
            let wanted = canonical(expected(expected_row));
            if actual != wanted { failures.push(format!("{phase:?} stage{index}: {actual:?}, expected {wanted:?}")); }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
