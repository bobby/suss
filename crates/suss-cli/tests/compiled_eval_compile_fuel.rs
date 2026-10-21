//! The eval frontend accepts compilation headroom without changing its default.
use std::process::Command;

#[test]
fn eval_command_compile_fuel_preserves_default_and_honors_zero() {
    for explicit in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_suss"));
        command.args(["-e", "(+ 20 22)"]);
        if explicit { command.args(["--compile-fuel", "100000000000"]); }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, b"42\n");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .args(["-e", "(throw 17)", "--compile-fuel", "0"])
        .output().unwrap();
    assert!(!output.status.success(), "zero compilation allowance must be honored");
    assert!(output.stdout.is_empty(), "{output:?}");
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.to_ascii_lowercase().contains("fuel"), "{error}");
    assert!(!error.contains("Uncaught language exception"), "Runtime must not execute: {error}");
}
