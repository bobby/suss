//! Exact integer dispatch, promotion and runtime reading follow JVM Clojure.
use suss_cli::portable_session::Session;

fn text(session: &mut Session, source: &str) -> String {
    let value = session
        .eval(source)
        .unwrap_or_else(|e| panic!("{source}: {e}"));
    session.collect().unwrap();
    session
        .inspect(&value, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let units = array
                .elems(&mut store)?
                .map(|v| v.unwrap_i32() as u16)
                .collect::<Vec<_>>();
            Ok(String::from_utf16(&units).unwrap())
        })
        .unwrap()
}

#[test]
fn integer_dispatch_promotion_and_reader_match_jvm_in_both_phases() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // The 16,384-bit schoolbook/product printing cases intentionally
        // exceed the ordinary interactive operation budget.
        session.set_operation_fuel(1_000_000_000);
        let cases: Vec<(String, String)> = serde_json::from_str(include_str!(
            "../../../results/m12-bigint-integers/cases.json"
        ))
        .unwrap();
        for (source, expected) in cases {
            assert_eq!(text(&mut session, &source), expected, "{source}");
        }
    }
}

#[test]
fn aot_executes_integer_dispatch_promotion_and_runtime_reading() {
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    let cases: Vec<(String, String)> = serde_json::from_str(include_str!(
        "../../../results/m12-bigint-integers/cases.json"
    ))
    .unwrap();
    // A distinct failure index proves every case was reached on success.
    let mut body = cases.len().to_string();
    for (index, (source, expected)) in cases.iter().enumerate().rev() {
        body = format!("(if (= {source} {expected:?}) {body} {index})");
    }
    std::fs::write(
        root.path().join("bigints.sus"),
        format!("(ns bigints) (def check (fn [unused] {body}))"),
    )
    .unwrap();
    std::fs::write(
        root.path().join("bigints.wit"),
        "package test:bigints; world w { export check: func(unused: u32) -> u32; }",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args([
            "compile",
            "bigints.sus",
            "-w",
            "bigints.wit",
            "-o",
            "bigints.wasm",
            "--export",
            "check=bigints/check",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args(["run", "bigints.wasm", "--invoke", "check", "--", "0"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        cases.len().to_string()
    );
}

#[test]
fn renamed_promoting_core_vars_keep_their_inline_operations() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(ns promote.alias (:refer-clojure :rename {inc' next-int dec' previous-int}))").unwrap();
        assert_eq!(text(&mut session, "(pr-str [(next-int 9223372036854775807) (previous-int -9223372036854775808)])"),
            "[9223372036854775808N -9223372036854775809N]");
    }
}
