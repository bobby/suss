use suss_cli::portable_session::Session;

fn cases() -> Vec<(String, String)> {
    serde_json::from_str(include_str!(
        "../../../results/m12-promotion-fixes/cases.json"
    ))
    .unwrap()
}

#[test]
fn unchecked_ratios_and_integer_categories_match_jvm_in_both_phases() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for (source, expected) in cases() {
            let value = session
                .eval(&format!("(pr-str {source})"))
                .unwrap_or_else(|e| panic!("{source}: {e}"));
            session.collect().unwrap();
            let found = session
                .inspect(&value, |mut store, value| {
                    let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                    let units = array
                        .elems(&mut store)?
                        .map(|v| v.unwrap_i32() as u16)
                        .collect::<Vec<_>>();
                    Ok(String::from_utf16(&units).unwrap())
                })
                .unwrap();
            assert_eq!(found, expected, "{source}");
        }
    }
}

#[test]
fn aot_executes_unchecked_ratios_and_integer_categories() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let mut source = String::from("(ns uncheckedprobe)\n(def check (fn [unused] (if (and\n");
    for (expression, expected) in cases() {
        source.push_str(&format!("(= (pr-str {expression}) {expected:?})\n"));
    }
    source.push_str(") 1 0)))\n");
    std::fs::write(dir.path().join("probe.sus"), source).unwrap();
    std::fs::write(
        dir.path().join("probe.wit"),
        "package test:uncheckedprobe; world w { export check: func(unused: u32) -> u32; }",
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
            "check=uncheckedprobe/check",
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
