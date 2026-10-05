//! Execute command artifacts produced by the public compiler library API.
#![cfg(not(target_family = "wasm"))]

use std::{
    path::Path,
    process::{Command, Output},
    sync::OnceLock,
};
use suss_compile::Compiler;
use wasmtime::{
    Config, Engine,
    component::{
        Component,
        types::{ComponentItem, Type},
    },
};

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut config = Config::new();
        config
            .wasm_gc(true)
            .wasm_function_references(true)
            .wasm_tail_call(true)
            .wasm_exceptions(true)
            .wasm_component_model(true)
            .wasm_component_model_async(true)
            .wasm_component_model_implements(true)
            .cranelift_opt_level(wasmtime::OptLevel::None);
        Engine::new(&config).unwrap()
    })
}

fn compile(root: &Path, source: &str) {
    let bytes = Compiler::new().compile_for_main(source, "app").unwrap();
    let component = Component::new(engine(), &bytes).unwrap();
    let interface = component
        .get_export_index(None, "wasi:cli/run@0.3.1")
        .expect("official asynchronous command interface");
    let (ComponentItem::ComponentFunc(run), _) = component
        .get_export(Some(&interface), "run")
        .expect("official run export")
    else {
        panic!("run must be a function")
    };
    assert!(run.async_());
    assert_eq!(run.params().len(), 0);
    let results = run.results().collect::<Vec<_>>();
    assert_eq!(results.len(), 1);
    let Type::Result(result) = &results[0] else {
        panic!("run must return result")
    };
    assert!(result.ok().is_none() && result.err().is_none());
    std::fs::write(root.join("app.wasm"), bytes).unwrap();
}

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root)
        .args(["run", "app.wasm", "--"])
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn public_command_validates_requested_namespace_before_macro_effects() {
    for (source, expected) in [
        (
            "(ns wrong) (defmacro fail [] (throw 17)) (def -main (fn [] (fail)))",
            "Declared namespace wrong does not match requested app",
        ),
        (
            "(defmacro fail [] (throw 17)) (def -main (fn [] (fail)))",
            "leading ns declaration",
        ),
    ] {
        let error = Compiler::new()
            .compile_for_main(source, "app")
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
        assert!(
            !error.contains("Macro exception"),
            "namespace selection precedes expansion: {error}"
        );
    }
}

#[test]
fn public_command_executes_compiled_lexical_macros_and_ordered_user_arguments() {
    let root = tempfile::tempdir().unwrap();
    compile(
        root.path(),
        r#"
        (ns app)
        (defmacro require-local [local]
          (if (get (:locals &env) local) local (throw 17)))
        (def seen (atom 0))
        (swap! seen inc)
        (defn -main [first second third]
          (let [answer (+ 41 @seen)]
            (if (= (require-local answer) 42)
              (if (= first "")
                (if (= second "λ雪🦊") (if (= third "--x") 73 (throw 17)) (throw 17))
                (throw 17))
              (throw 17))))
    "#,
    );
    let output = run(root.path(), &["", "λ雪🦊", "--x"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let failed = run(root.path(), &["", "--x", "λ雪🦊"]);
    assert!(!failed.status.success());
    assert!(failed.stdout.is_empty());
}

#[test]
fn public_command_defers_runtime_initialization_and_reports_uncaught_exceptions() {
    let root = tempfile::tempdir().unwrap();
    compile(root.path(), "(ns app) (throw 42) (defn -main [] 73)");
    let output = run(root.path(), &[]);
    assert!(
        !output.status.success(),
        "throwing initializer must execute only in the artifact host"
    );
    assert!(output.stdout.is_empty());
    compile(root.path(), "(ns app) (defn -main [] (throw 17))");
    assert!(!run(root.path(), &[]).status.success());
}

#[test]
fn public_command_preserves_normal_completion_and_explicit_exit_policy() {
    let root = tempfile::tempdir().unwrap();
    for value in ["73", "false", "nil", "##NaN"] {
        compile(root.path(), &format!("(ns app) (defn -main [] {value})"));
        let output = run(root.path(), &[]);
        assert!(
            output.status.success(),
            "normal {value} completion: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
    }
    for status in [0, 73, 255] {
        compile(
            root.path(),
            &format!("(ns app) (defn -main [] (do (wasi.cli/exit-with-code {status}) (throw 17)))"),
        );
        let output = run(root.path(), &[]);
        assert_eq!(
            output.status.code(),
            Some(status),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
    for invalid in ["-1", "256", "1.5", "nil", "false", "##NaN", "##Inf"] {
        compile(
            root.path(),
            &format!(
                "(ns app) (defn -main [] (if (= (try (wasi.cli/exit-with-code {invalid}) (catch :default e 42)) 42) (wasi.cli/exit-with-code 73) (throw 17)))"
            ),
        );
        let output = run(root.path(), &[]);
        assert_eq!(
            output.status.code(),
            Some(73),
            "invalid {invalid} must be caught before explicit exit: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn public_command_does_not_grant_runtime_exit_to_macro_phase() {
    let error = Compiler::new()
        .compile_for_main(
            "(ns app) (defmacro die [] (wasi.cli/exit-with-code 73)) (defn -main [] (die))",
            "app",
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("Macro"), "{error}");
}
