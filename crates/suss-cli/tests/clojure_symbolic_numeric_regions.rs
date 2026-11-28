//! Compiler-created regions retain the enclosing source function constant pool.
use suss_cli::portable_session::Session;

fn controls() -> Vec<serde_json::Value> {
    serde_json::from_str(include_str!(
        "../../../results/m12-symbolic-regions/primary-controls.json"
    ))
    .unwrap()
}

#[test]
fn symbolic_numeric_constants_retain_enclosing_try_binding_and_cleanup_regions() {
    let mut failures = Vec::new();
    for (phase, mut session) in [
        ("Runtime", Session::new_repl().unwrap()),
        ("Macro", Session::new_macro().unwrap()),
    ] {
        for row in controls() {
            let source = format!("(pr-str {})", row["native_expression"].as_str().unwrap());
            let value = session.eval(&source).unwrap();
            session.collect().unwrap();
            let actual = session
                .inspect(&value, |mut store, value| {
                    let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                    let units = array
                        .elems(&mut store)?
                        .map(|unit| unit.unwrap_i32() as u16)
                        .collect::<Vec<_>>();
                    Ok(String::from_utf16(&units).unwrap())
                })
                .unwrap();
            if actual != row["observed_edn"].as_str().unwrap() {
                failures.push(format!(
                    "{phase} {}: expected {}, actual {actual}",
                    row["name"], row["observed_edn"]
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn actual_aot_symbolic_constants_retain_enclosing_function_through_regions() {
    use std::process::Command;
    let directory = tempfile::tempdir().unwrap();
    let rows = controls();
    let body = rows
        .iter()
        .map(|row| row["native_expression"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join(" ");
    let expected = format!(
        "[{}]",
        rows.iter()
            .map(|row| row["observed_edn"].as_str().unwrap())
            .collect::<Vec<_>>()
            .join(" ")
    );
    std::fs::write(
        directory.path().join("app.sus"),
        format!("(ns symbolic-regions) (def report (fn [] (pr-str [{body}])))"),
    )
    .unwrap();
    std::fs::write(
        directory.path().join("app.wit"),
        "package test:symbolic-regions; world w { export report: func() -> string; }",
    )
    .unwrap();
    let compiled = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(directory.path())
        .args([
            "--no-cache", "compile", "app.sus", "-w", "app.wit", "--export",
            "report=symbolic-regions/report", "-o", "app.wasm",
        ])
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let executed = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(directory.path())
        .args(["--no-cache", "run", "app.wasm", "--invoke", "report"])
        .output()
        .unwrap();
    assert!(executed.status.success(), "{}", String::from_utf8_lossy(&executed.stderr));
    assert_eq!(String::from_utf8(executed.stdout).unwrap().trim_end(), expected);
}
