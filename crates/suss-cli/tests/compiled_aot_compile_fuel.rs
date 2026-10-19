//! Explicit AOT compilation budgets do not change artifact Runtime execution.
use std::process::Command;

#[test]
fn aot_compile_fuel_file_and_namespace_preserve_defaults_and_failed_output() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    std::fs::write(
        root.path().join("src/budget.sus"),
        "(ns budget) (defmacro result [] 42) (def calculate (fn [] (result)))",
    )
    .unwrap();
    std::fs::write(
        root.path().join("api.wit"),
        "package test:compile-budget; world api { export calculate: func() -> f64; }",
    )
    .unwrap();
    for namespace in [false, true] {
        let compile = |fuel: Option<&str>| {
            let mut command = Command::new(env!("CARGO_BIN_EXE_suss"));
            command.current_dir(root.path()).arg("compile");
            if namespace {
                command.args(["-n", "budget", "--src", "src"]);
            } else {
                command.arg("src/budget.sus");
            }
            command.args([
                "-w",
                "api.wit",
                "--wit-world",
                "api",
                "--export",
                "calculate=budget/calculate",
                "-o",
                "out.wasm",
            ]);
            if let Some(fuel) = fuel {
                command.args(["--compile-fuel", fuel]);
            }
            command.output().unwrap()
        };
        let output = compile(None);
        assert!(
            output.status.success(),
            "default: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::write(root.path().join("out.wasm"), b"prior artifact").unwrap();
        let output = compile(Some("0"));
        assert!(
            !output.status.success(),
            "zero compilation limit must not be ignored"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .to_ascii_lowercase()
                .contains("fuel"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            std::fs::read(root.path().join("out.wasm")).unwrap(),
            b"prior artifact"
        );
        let output = compile(Some("10000000000"));
        assert!(
            output.status.success(),
            "explicit: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args(["run", "out.wasm", "--invoke", "calculate"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "42");
    }
}
