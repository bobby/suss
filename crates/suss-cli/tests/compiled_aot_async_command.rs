//! Native CLI execution of asynchronous scalar component exports.
use std::process::{Command, Output};
fn invoke(root: &std::path::Path, name: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root)
        .args(["run", "app.wasm", "--invoke", name, "--"])
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn native_async_invocation_executes_compiled_macros_and_validates_inputs() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("app.sus"), "(ns app) (def seen 0) (defmacro twice [x] `(+ ~x ~x)) (def calculate (fn [x] (do (set! seen (+ seen 1)) (twice x)))) (def invert (fn [flag] (if flag false true))) (def bump (fn [] (set! seen (+ seen 1))))").unwrap();
    std::fs::write(root.path().join("api.wit"), "package test:async-cli; interface api { calculate: async func(x: u32) -> u32; invert: async func(flag: bool) -> bool; } world selected { export api; export bump: async func(); }").unwrap();
    let compiled = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args([
            "compile",
            "app.sus",
            "-w",
            "api.wit",
            "-o",
            "app.wasm",
            "--export",
            "test:async-cli/api#calculate=app/calculate",
            "--export",
            "test:async-cli/api#invert=app/invert",
            "--export",
            "bump=app/bump",
        ])
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    for (name, args, expected) in [
        ("test:async-cli/api#calculate", vec!["21"], "42\n"),
        ("test:async-cli/api#invert", vec!["false"], "true\n"),
        ("bump", vec![], ""),
    ] {
        let output = invoke(root.path(), name, &args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
    }
    for args in [
        vec![],
        vec!["-1"],
        vec!["4294967296"],
        vec!["nonsense"],
        vec!["21", "22"],
    ] {
        let output = invoke(root.path(), "test:async-cli/api#calculate", &args);
        assert!(!output.status.success());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(
            error.contains("expects") || error.contains("Parameter"),
            "{error}"
        );
        assert!(!error.contains("unimplemented"), "{error}");
    }
}

#[test]
fn async_input_validation_precedes_guest_initializers() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("app.sus"),
        "(ns app) (def calculate (fn [x] x)) (throw 17)",
    )
    .unwrap();
    std::fs::write(
        root.path().join("api.wit"),
        "package test:async-validation; world api { export calculate: async func(x: u32) -> u32; }",
    )
    .unwrap();
    let compiled = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args([
            "compile",
            "app.sus",
            "-w",
            "api.wit",
            "-o",
            "app.wasm",
            "--export",
            "calculate=app/calculate",
        ])
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    for args in [vec![], vec!["-1"], vec!["nonsense"], vec!["1", "2"]] {
        let output = invoke(root.path(), "calculate", &args);
        assert!(!output.status.success());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(
            error.contains("expects") || error.contains("Parameter"),
            "{error}"
        );
        assert!(!error.contains("instantiate"), "{error}");
    }
    let valid = invoke(root.path(), "calculate", &["1"]);
    assert!(!valid.status.success());
    assert!(String::from_utf8_lossy(&valid.stderr).contains("Failed to instantiate component"));
}
