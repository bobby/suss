//! BigInt language values execute through the compiled pipeline in both phases.
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
fn bigint_literals_and_signed_arithmetic_execute_in_both_phases() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // The 16,384-bit schoolbook/product printing cases intentionally
        // exceed the ordinary interactive operation budget.
        session.set_operation_fuel(1_000_000_000);
        let cases: Vec<(String, String)> = serde_json::from_str(include_str!(
            "../../../results/m12-bigint-values/cases.json"
        ))
        .unwrap();
        for (source, expected) in cases {
            assert_eq!(text(&mut session, &source), expected, "{source}");
        }
    }
}

#[test]
fn macro_arguments_and_results_are_real_bigints() {
    use suss_cli::portable_macros::CompiledMacros;
    let mut macros = CompiledMacros::new().unwrap();
    macros.define("(defmacro bigint-add [x] (+ x 1N))").unwrap();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let value = session
            .eval_with_macros("(pr-str (bigint-add 18446744073709551616N))", &mut macros)
            .unwrap();
        session.collect().unwrap();
        let observed = session
            .inspect(&value, |mut store, value| {
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
        assert_eq!(observed, "18446744073709551617N");
    }
}

#[test]
fn aot_executes_the_bigint_matrix() {
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    let cases: Vec<(String, String)> = serde_json::from_str(include_str!(
        "../../../results/m12-bigint-values/cases.json"
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
fn retained_magnitudes_survive_collections_without_mutation() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def retained-bigint (let [x 18446744073709551616N] (fn [y] [x (+ x y) (- x y) (* x y)])))").unwrap();
        for _ in 0..3 {
            session.collect().unwrap();
        }
        assert_eq!(
            text(&mut session, "(pr-str (retained-bigint 1))"),
            "[18446744073709551616N 18446744073709551617N 18446744073709551615N 18446744073709551616N]"
        );
        for _ in 0..3 {
            session.collect().unwrap();
        }
        assert_eq!(
            text(&mut session, "(pr-str (retained-bigint -1))"),
            "[18446744073709551616N 18446744073709551615N 18446744073709551617N -18446744073709551616N]"
        );
    }
}
