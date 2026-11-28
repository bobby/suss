//! Public num values, inline boundaries and ordinary alias effects.
use suss_cli::{
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionError},
};
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};

fn cases() -> Vec<serde_json::Value> {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-num-api/primary/cases.json"
    ))
    .unwrap();
    assert_eq!(rows.len(), 14, "retain the complete inline API primary");
    let bindings: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-num-api/binding-primary/cases.json"
    ))
    .unwrap();
    assert_eq!(bindings.len(), 6, "retain the independent binding controls");
    rows.into_iter().chain(bindings).collect()
}
fn stage_form(row: &serde_json::Value) -> Option<&'static str> {
    match row["name"].as_str().unwrap() {
        "direct-zero-compile-error" => Some("(num)"),
        "direct-two-compile-error" => Some("(num 1 2)"),
        _ => None,
    }
}
fn native_expression(row: &serde_json::Value) -> String {
    // Suss has owned language errors, not Java exception classes. Preserve
    // the primary effect/value boundary and check the real arity diagnostic.
    row["expression"].as_str().unwrap()
        .replace("(catch Throwable e", "(catch :default e")
        .replace("(catch clojure.lang.ArityException e :arity)",
            "(catch :default e (if (= \"Wrong number of args\" (subs (ex-message e) 0 20)) :arity :wrong-error))")
}
fn expected(value: &suss_reader::edn::Value) -> Observation {
    use suss_reader::edn::Value;
    match value {
        Value::Nil => Observation::Nil,
        Value::Long(value) => Observation::Long(*value),
        Value::Bool(value) => Observation::Bool(*value),
        Value::String(value) => Observation::String(value.encode_utf16().collect()),
        Value::Keyword(value) => Observation::Keyword(
            value
                .namespace
                .as_ref()
                .map(|value| value.encode_utf16().collect()),
            value.name.encode_utf16().collect(),
        ),
        Value::Vector(values) => Observation::Vector(values.iter().map(expected).collect()),
        other => panic!("unassessed primary result {other:?}"),
    }
}
#[test]
fn num_public_values_alias_effects_and_direct_inline_stages_match_the_primary() {
    let mut failures = Vec::new();
    for (phase, mut session) in [
        ("Runtime", Session::new_repl().unwrap()),
        ("Macro", Session::new_macro().unwrap()),
    ] {
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        let mut stages = 0;
        for row in cases() {
            let name = row["name"].as_str().unwrap();
            if let Some(form) = stage_form(&row) {
                stages += 1;
                match session.eval_with_macros(form, &mut macros) {
                    Err(SessionError::Compile(error))
                        if error.message.contains("Wrong number of args")
                            && error.message.contains("num") => {}
                    Err(error) => failures.push(format!(
                        "{phase} {name}: expected num compile-time arity error, got {error}"
                    )),
                    Ok(_) => failures.push(format!("{phase} {name}: unexpectedly evaluated")),
                }
                continue;
            }
            let value = session
                .eval_with_macros(&native_expression(&row), &mut macros)
                .unwrap();
            session.collect().unwrap();
            decoder.refill(20_000);
            let actual = decoder.decode_session(&mut session, &value).unwrap();
            let primary = suss_reader::edn::read_one(
                row["observed_edn"].as_str().unwrap(),
                &Default::default(),
            )
            .unwrap();
            let wanted = expected(&primary);
            if actual != wanted {
                failures.push(format!(
                    "{phase} {name}: expected {wanted:?}, actual {actual:?}"
                ));
            }
        }
        assert_eq!(
            stages, 2,
            "all14 API and6 binding controls include exactly2 compilation-stage checks"
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
#[test]
fn actual_aot_num_values_and_inline_compilation_stages_match_the_primary() {
    use std::process::Command;
    let rows = cases();
    let values = rows
        .iter()
        .filter(|row| stage_form(row).is_none())
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 18);
    let expressions = values
        .iter()
        .map(|row| native_expression(row))
        .collect::<Vec<_>>()
        .join(" ");
    let wanted = format!(
        "[{}]",
        values
            .iter()
            .map(|row| row["observed_edn"].as_str().unwrap())
            .collect::<Vec<_>>()
            .join(" ")
    );
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("app.wit"),
        "package test:num-api; world w { export report: func() -> string; }",
    )
    .unwrap();
    std::fs::write(
        root.path().join("app.sus"),
        format!("(ns num-api) (def report (fn [] (pr-str [{expressions}])))"),
    )
    .unwrap();
    let compile = || {
        Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args([
                "--no-cache",
                "compile",
                "app.sus",
                "-w",
                "app.wit",
                "--export",
                "report=num-api/report",
                "-o",
                "app.wasm",
            ])
            .output()
            .unwrap()
    };
    let compiled = compile();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let result = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args(["--no-cache", "run", "app.wasm", "--invoke", "report"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut failures = Vec::new();
    let actual = String::from_utf8(result.stdout).unwrap();
    if actual.trim_end() != wanted {
        failures.push(format!("value results: expected {wanted}, actual {actual}"));
    }
    let mut stages = 0;
    for row in &rows {
        if let Some(form) = stage_form(row) {
            stages += 1;
            std::fs::write(
                root.path().join("app.sus"),
                format!("(ns num-api) (def report (fn [] {form}))"),
            )
            .unwrap();
            let compiled = compile();
            let diagnostic = String::from_utf8_lossy(&compiled.stderr);
            if compiled.status.success()
                || !diagnostic.contains("Wrong number of args")
                || !diagnostic.contains("num")
            {
                failures.push(format!(
                    "{}: expected compile-time num arity failure, status {}, stderr {diagnostic}",
                    row["name"], compiled.status
                ));
            }
        }
    }
    assert_eq!(stages, 2);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
