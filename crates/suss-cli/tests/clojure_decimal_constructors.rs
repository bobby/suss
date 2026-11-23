//! Exact Decimal conversion paths follow fresh JVM expectations.
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};

fn observed(
    session: &mut Session,
    macros: &mut CompiledMacros,
    source: &str,
) -> Result<String, String> {
    let source = source.replace("(catch Throwable ", "(catch :default ");
    let source = format!("(pr-str (try {source} (catch :default e [:error (ex-message e)])))");
    let value = session
        .eval_with_macros(&source, macros)
        .map_err(|e| format!("{e:?}"))?;
    session.collect().map_err(|e| e.to_string())?;
    session
        .inspect(&value, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let units = array
                .elems(&mut store)?
                .map(|v| v.unwrap_i32() as u16)
                .collect::<Vec<_>>();
            Ok(String::from_utf16(&units).unwrap())
        })
        .map_err(|e| e.to_string())
}

fn check_cases(fixtures: &[&str]) {
    let cases: Vec<(String, String)> = fixtures
        .iter()
        .flat_map(|fixture| serde_json::from_str::<Vec<(String, String)>>(fixture).unwrap())
        .collect();
    let mut failures = Vec::new();
    for phase in ["Runtime", "Macro"] {
        let mut session = if phase == "Runtime" {
            Session::new_repl().unwrap()
        } else {
            Session::new_macro().unwrap()
        };
        session.set_operation_fuel(100_000_000);
        let mut macros = CompiledMacros::new().unwrap();
        for (i, (source, expected)) in cases.iter().enumerate() {
            let actual = observed(&mut session, &mut macros, source);
            if actual.as_ref() != Ok(expected) {
                failures.push(format!(
                    "{phase} case{i}: {source}\nexpected {expected}\nactual {actual:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn math_context_constructor_overloads_follow_primary_controls() {
    check_cases(&[
        include_str!("../../../results/m12-context-constructors/constructor-cases.json"),
        include_str!("../../../results/m12-context-constructors/extended-diagnostics-cases.json"),
    ]);
}

#[test]
fn aot_executes_math_context_constructor_overloads() {
    use std::process::Command;
    let cases = [
        include_str!("../../../results/m12-context-constructors/constructor-cases.json"),
        include_str!("../../../results/m12-context-constructors/extended-diagnostics-cases.json"),
    ]
    .iter()
    .flat_map(|fixture| serde_json::from_str::<Vec<(String, String)>>(fixture).unwrap())
    .collect::<Vec<_>>();
    let dir = tempfile::tempdir().unwrap();
    let mut source = String::from("(ns contextconstructorprobe)\n");
    for (i, (expression, expected)) in cases.iter().enumerate() {
        let expression = expression.replace("(catch Throwable ", "(catch :default ");
        source.push_str(&format!("(def result{i} (= (pr-str (try {expression} (catch :default e [:error (ex-message e)]))) {expected:?}))\n"));
    }
    source.push_str("(def check (fn [unused] (cond\n");
    for i in 0..cases.len() {
        source.push_str(&format!("(not result{i}) {}\n", i + 1));
    }
    source.push_str(":else 0)))\n");
    std::fs::write(dir.path().join("probe.sus"), source).unwrap();
    std::fs::write(
        dir.path().join("probe.wit"),
        "package test:contextconstructorprobe; world w { export check: func(unused: u32) -> u32; }",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(dir.path())
        .args([
            "compile",
            "probe.sus",
            "-w",
            "probe.wit",
            "--export",
            "check=contextconstructorprobe/check",
            "-o",
            "probe.wasm",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(dir.path())
        .args(["run", "probe.wasm", "--invoke", "check", "--", "0"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let first: usize = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap();
    assert_eq!(
        first,
        0,
        "AOT failed primary control: {:?}",
        first.checked_sub(1).and_then(|i| cases.get(i))
    );
}
