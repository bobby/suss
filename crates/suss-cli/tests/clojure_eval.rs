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
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("app.sus"),
        "(ns eval-capability) (def report (fn [] (try (str (eval '(+ 1N 1/2))) (catch :default e (.-message e)))))").unwrap();
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
        "Compiled evaluation capability is unavailable"
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
fn isolated_macro_function_calls_share_the_original_caller_allowance() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let observations = Arc::new(AtomicUsize::new(0));
        let observer = observations.clone();
        macros
            .install_observation_hook(move |_, _| {
                observer.fetch_add(1, Ordering::SeqCst);
                Ok(0)
            })
            .unwrap();
        session
            .eval_with_macros(
                "(eval '(defmacro raw-budget-once [] (suss.harness.observation/capture 1) 1))",
                &mut macros,
            )
            .unwrap();
        let repeated = session
            .eval_with_macros(
                "(let [f @(eval '(var raw-budget-once))] (fn [] (loop [] (f nil nil) (recur))))",
                &mut macros,
            )
            .unwrap();
        let looping = session.eval_with_macros("(do (eval '(defmacro raw-budget-loop [] (suss.harness.observation/capture 2) (loop [] (recur)))) @(eval '(var raw-budget-loop)))", &mut macros).unwrap();
        let nil = session.eval("nil").unwrap();
        for (function, args) in [(&repeated, vec![]), (&looping, vec![&nil, &nil])] {
            observations.store(0, Ordering::SeqCst);
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
            assert!(
                observations.load(Ordering::SeqCst) > 0,
                "the actual Macro body must run before exhaustion"
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
fn runtime_cancellation_reaches_an_isolated_macro_function_and_returns_the_lease() {
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    let (signal, observed) = std::sync::mpsc::sync_channel(1);
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
            let observe = session.eval("(let [vector vector var? var? meta meta get get key :probe] (fn [v] (vector (var? v) @v (get (meta v) key))))").unwrap();
            session.eval(&format!("(ns suss.core) (def count (fn [x] 99)) (def {constant}PersistentVector nil) (def {constant}Keyword nil)")).unwrap();
            session.enter_namespace("user").unwrap();
            for (root, metadata) in [(42, 17), (43, 18)] {
                session.eval(&format!("(def ^{{:probe [{metadata} [{}]]}} metadata-target {root}) (def public-count-result (let [f count] (f 123)))", metadata + 2)).unwrap();
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
