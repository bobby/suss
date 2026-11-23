//! Decimal literal/read/class behavior against fresh Clojure expectations.
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};
fn cases() -> Vec<serde_json::Value> {
    serde_json::from_str(include_str!("../../../results/m12-decimal-ci/cases.json")).unwrap()
}
#[test]
fn decimal_values_and_classes_execute_in_runtime_and_macro() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // The 1,024-digit controls parse and print the coefficient repeatedly.
        // Use the printer tests' 100M allowance for this new stress matrix;
        // the original default-fuel failures remain in its evidence directory.
        session.set_operation_fuel(100_000_000);
        let mut macros = CompiledMacros::new().unwrap();
        for row in cases() {
            let setup = row["setup"].as_str().unwrap();
            if !setup.is_empty() {
                session.eval_with_macros(setup, &mut macros).unwrap();
            }
            let expression = row["source"].as_str().unwrap();
            let v = session
                .eval_with_macros(&format!("(pr-str {expression})"), &mut macros)
                .unwrap_or_else(|e| panic!("{expression}: {e}"));
            session.collect().unwrap();
            let found = session
                .inspect(&v, |mut store, value| {
                    let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                    Ok(String::from_utf16(
                        &array
                            .elems(&mut store)?
                            .map(|v| v.unwrap_i32() as u16)
                            .collect::<Vec<_>>(),
                    )
                    .unwrap())
                })
                .unwrap();
            assert_eq!(found, row["expected"].as_str().unwrap(), "{expression}");
        }
    }
}
#[test]
fn aot_executes_decimal_values_readers_and_classes() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let mut source = String::from("(ns decimalciprobe)\n");
    for (i, row) in cases().iter().enumerate() {
        source.push_str(row["setup"].as_str().unwrap());
        source.push_str(&format!(
            "\n(def result{i} (= (pr-str {}) {:?}))\n",
            row["source"].as_str().unwrap(),
            row["expected"].as_str().unwrap()
        ));
    }
    source.push_str("(def check (fn [unused] (if (and");
    for i in 0..cases().len() {
        source.push_str(&format!(" result{i}"));
    }
    source.push_str(") 1 0)))\n");
    std::fs::write(dir.path().join("probe.sus"), source).unwrap();
    std::fs::write(
        dir.path().join("probe.wit"),
        "package test:decimalciprobe; world w { export check: func(unused: u32) -> u32; }",
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(dir.path())
        .args([
            "compile",
            "probe.sus",
            "-w",
            "probe.wit",
            "--export",
            "check=decimalciprobe/check",
            "-o",
            "probe.wasm",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(dir.path())
        .args(["run", "probe.wasm", "--invoke", "check", "--", "0"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), "1");
}
