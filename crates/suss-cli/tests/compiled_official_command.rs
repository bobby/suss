//! Execute the official command artifact emitted by `compile --main`.
use std::{
    path::Path,
    process::{Command, Output},
};
use wasmtime::component::{
    Component,
    types::{ComponentItem, Type},
};
use wasmtime::{Config, Engine};

fn compile(root: &Path, source: &str) {
    std::fs::write(root.join("app.sus"), source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root)
        .args(["compile", "app.sus", "--main", "app", "-o", "app.wasm"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run(root: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root)
        .args(["run", "app.wasm", "--"])
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn main_emits_official_async_result_and_normal_number_completion_succeeds() {
    let root = tempfile::tempdir().unwrap();
    compile(root.path(), "(ns app) (def -main (fn [] 73))");
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
    let engine = Engine::new(&config).unwrap();
    let component = Component::from_file(&engine, root.path().join("app.wasm")).unwrap();
    let interface = component
        .get_export_index(None, "wasi:cli/run@0.3.1")
        .expect("official versioned command interface");
    let (ComponentItem::ComponentFunc(run_type), _) = component
        .get_export(Some(&interface), "run")
        .expect("official run function")
    else {
        panic!("run must be a function")
    };
    assert!(run_type.async_());
    assert_eq!(run_type.params().len(), 0);
    let results = run_type.results().collect::<Vec<_>>();
    assert_eq!(results.len(), 1);
    let Type::Result(result) = &results[0] else {
        panic!("run must return result")
    };
    assert!(result.ok().is_none() && result.err().is_none());
    let output = run(root.path(), &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "ordinary return is command completion"
    );
    for value in ["false", "nil", "##NaN"] {
        compile(
            root.path(),
            &format!("(ns app) (def -main (fn [] {value}))"),
        );
        let output = run(root.path(), &[]);
        assert!(
            output.status.success(),
            "ordinary {value} return: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn main_receives_only_user_strings_in_order_and_uncaught_exception_fails() {
    let root = tempfile::tempdir().unwrap();
    compile(
        root.path(),
        "(ns app) (def -main (fn [first second third] (if (= first \"\") (if (= second \"λ雪🦊\") (if (= third \"--x\") 73 (throw 17)) (throw 17)) (throw 17))))",
    );
    let output = run(root.path(), &["", "λ雪🦊", "--x"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    for invoke in ["wasi:cli/run@0.3.1#run", "run"] {
        let output = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args([
                "run",
                "app.wasm",
                "--invoke",
                invoke,
                "--",
                "",
                "λ雪🦊",
                "--x",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "explicit {invoke}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
    }
    let failed = run(root.path(), &["", "--x", "λ雪🦊"]);
    assert!(
        !failed.status.success(),
        "source exception must fail the command"
    );
    assert!(failed.stdout.is_empty());
}

#[test]
fn main_uses_compiled_macros_source_paths_and_preflights_selection_before_output() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("deps")).unwrap();
    std::fs::write(root.path().join("deps/math.sus"), "(ns math) (def bias 21)").unwrap();
    let source = "(ns app (:require [math :as m])) (defmacro twice [x] `(+ ~x ~x)) (def -main (fn [] (if (= (twice m/bias) 42) 73 (throw 17))))";
    std::fs::write(root.path().join("app.sus"), source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args([
            "compile", "app.sus", "--main", "app", "--src", "deps", "-o", "app.wasm",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = run(root.path(), &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let artifact = std::fs::read(root.path().join("app.wasm")).unwrap();
    for flags in [
        ["--wit", "missing.wit"],
        ["--world", "ignored"],
        ["--config", "missing.sus"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args(["compile", "app.sus", "--main", "app", "-o", "app.wasm"])
            .args(flags)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("official command world"));
        assert_eq!(
            std::fs::read(root.path().join("app.wasm")).unwrap(),
            artifact
        );
    }
    std::fs::write(
        root.path().join("app.sus"),
        "(ns wrong) (def -main (fn [] 73))",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args(["compile", "app.sus", "--main", "app", "-o", "app.wasm"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("namespace"));
    assert_eq!(
        std::fs::read(root.path().join("app.wasm")).unwrap(),
        artifact
    );
}

#[test]
fn explicit_exit_uses_u8_status_and_invalid_values_are_catchable_language_errors() {
    let root = tempfile::tempdir().unwrap();
    for code in [0, 73, 255] {
        compile(
            root.path(),
            &format!(
                "(ns app) (def -main (fn [] (do (wasi.cli/exit-with-code {code}) (throw 17))))"
            ),
        );
        let output = run(root.path(), &[]);
        assert_eq!(
            output.status.code(),
            Some(code),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
    compile(
        root.path(),
        "(ns app (:require [wasi.cli :as cli])) (cli/exit-with-code 19) (def -main (fn [] (throw 17)))",
    );
    assert_eq!(run(root.path(), &[]).status.code(), Some(19));
    for invalid in ["-1", "256", "1.5", "nil", "false", "##NaN", "##Inf"] {
        compile(
            root.path(),
            &format!(
                "(ns app) (def -main (fn [] (try (wasi.cli/exit-with-code {invalid}) (catch :default e 73))))"
            ),
        );
        let output = run(root.path(), &[]);
        assert!(
            output.status.success(),
            "invalid status {invalid}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn runtime_exit_capability_is_unavailable_to_compiled_macro_phase() {
    let root = tempfile::tempdir().unwrap();
    compile(root.path(), "(ns app) (def -main (fn [] 73))");
    let artifact = std::fs::read(root.path().join("app.wasm")).unwrap();
    std::fs::write(
        root.path().join("app.sus"),
        "(ns app) (defmacro die [] (wasi.cli/exit-with-code 73)) (def -main (fn [] (die)))",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args(["compile", "app.sus", "--main", "app", "-o", "app.wasm"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_ne!(
        output.status.code(),
        Some(73),
        "Macro evaluation must not acquire Runtime exit capability"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Macro"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(root.path().join("app.wasm")).unwrap(),
        artifact
    );
}

#[test]
fn invalid_official_command_shape_is_rejected_before_guest_initialization() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("app.sus"),
        "(ns app) (def run (fn [] 73)) (throw 17)",
    )
    .unwrap();
    for declaration in ["func() -> u32", "async func() -> u32"] {
        std::fs::write(root.path().join("api.wit"), format!("package wasi:cli@0.3.1; interface run {{ run: {declaration}; }} world selected {{ export run; }}")).unwrap();
        let compiled = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args([
                "compile",
                "app.sus",
                "-w",
                "api.wit",
                "--export",
                "wasi:cli/run@0.3.1#run=app/run",
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
        for invoke in [None, Some("run"), Some("wasi:cli/run@0.3.1#run")] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_suss"));
            command.current_dir(root.path()).args(["run", "app.wasm"]);
            if let Some(invoke) = invoke {
                command.args(["--invoke", invoke]);
            }
            let output = command.output().unwrap();
            assert!(!output.status.success());
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(
                error.contains("Official command requires async func() -> result"),
                "{declaration}: {error}"
            );
            assert!(
                !error.contains("instantiate"),
                "guest initializer must not run: {error}"
            );
            assert!(output.stdout.is_empty());
        }
    }
}
