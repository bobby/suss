//! Pinned source-selected primitive versus Object behavior, with no guest
//! printer or equality deciding the native observations.
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};

fn cases() -> Vec<serde_json::Value> {
    let cases = serde_json::from_str::<Vec<serde_json::Value>>(include_str!(
        "../../../results/m12-number-source-objects/primary/cases.json"
    ))
    .unwrap();
    assert_eq!(cases.len(), 10);
    cases
}
fn expected(value: &suss_reader::edn::Value) -> Observation {
    match value {
        suss_reader::edn::Value::Bool(value) => Observation::Bool(*value),
        suss_reader::edn::Value::Vector(values) => {
            Observation::Vector(values.iter().map(expected).collect())
        }
        other => panic!("unassessed primary value {other:?}"),
    }
}
#[test]
fn number_source_primitive_and_object_contexts_match_the_primary_in_both_phases() {
    let mut failures = Vec::new();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        for row in cases() {
            let value = session
                .eval_with_macros(row["expression"].as_str().unwrap(), &mut macros)
                .unwrap();
            session.collect().unwrap();
            decoder.refill(20_000);
            let actual = decoder.decode_session(&mut session, &value).unwrap();
            let primary = suss_reader::edn::read_one(
                row["observed_edn"].as_str().unwrap(),
                &Default::default(),
            )
            .unwrap();
            let expected = expected(&primary);
            if actual != expected {
                failures.push(format!(
                    "{}: expected {expected:?}, actual {actual:?}",
                    row["name"]
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
#[test]
fn actual_aot_number_source_primitive_and_object_contexts_match_the_primary() {
    use std::process::Command;
    let cases = cases();
    let source = cases
        .iter()
        .map(|row| row["expression"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join(" ");
    let expected = format!(
        "[{}]",
        cases
            .iter()
            .map(|row| row["observed_edn"].as_str().unwrap())
            .collect::<Vec<_>>()
            .join(" ")
    );
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("app.sus"),
        format!("(ns number-source-objects) (def report (fn [] (pr-str [{source}])))"),
    )
    .unwrap();
    std::fs::write(
        root.path().join("app.wit"),
        "package test:number-source-objects; world w { export report: func() -> string; }",
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
            "report=number-source-objects/report",
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
        expected
    );
}
