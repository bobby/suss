//! Portable numeric class values match JVM names, membership and class behavior.
use suss_cli::portable_session::Session;

fn cases() -> Vec<(String, String)> {
    let values: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-numeric-classes/cases.json"
    ))
    .unwrap();
    values
        .into_iter()
        .map(|row| {
            (
                row["source"].as_str().unwrap().to_owned(),
                row["expected"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn scoped_cast_cases() -> Vec<serde_json::Value> {
    serde_json::from_str(include_str!(
        "../../../results/m12-native-edn/scoped-cast-cases.json"
    ))
    .unwrap()
}
fn printed(session: &mut Session, source: &str) -> String {
    let value = session
        .eval(source)
        .unwrap_or_else(|e| panic!("{source}: {e}"));
    session.collect().unwrap();
    let units = session
        .inspect(&value, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(array
                .elems(&mut store)?
                .map(|unit| unit.unwrap_i32() as u16)
                .collect::<Vec<_>>())
        })
        .unwrap();
    String::from_utf16(&units).unwrap()
}
#[test]
fn numeric_classes_match_in_runtime_and_macro() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for (source, expected) in cases() {
            let forms = suss_reader::forms::read_forms(&source).unwrap();
            let last = forms.last().unwrap().span.clone();
            let wrapped = format!("{} (pr-str {})", &source[..last.start], &source[last]);
            assert_eq!(printed(&mut session, &wrapped), expected, "{source}");
        }
    }
}
#[test]
fn aot_executes_numeric_classes() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let mut source = String::from("(ns numclassprobe)\n");
    source.push_str(include_str!(
        "../../../results/m12-numeric-classes/protocol-definitions.sus"
    ));
    let protocol_cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-numeric-classes/protocol-cases.json"
    ))
    .unwrap();
    for (i, row) in protocol_cases.iter().enumerate() {
        source.push_str(&format!(
            "(def protocolresult{i} (= (pr-str {}) {:?}))\n",
            row["source"].as_str().unwrap(),
            row["expected"].as_str().unwrap()
        ));
    }
    for (i, (expression, expected)) in cases().iter().enumerate() {
        let forms = suss_reader::forms::read_forms(expression).unwrap();
        let last = forms.last().unwrap().span.clone();
        source.push_str(&format!(
            "{}\n(def result{i} (= (pr-str {}) {expected:?}))\n",
            &expression[..last.start],
            &expression[last]
        ));
    }
    for (i, row) in scoped_cast_cases().iter().enumerate() {
        source.push_str(&format!(
            "(def castresult{i} (= (pr-str {}) {:?}))\n",
            row["source"].as_str().unwrap(),
            row["expected"].as_str().unwrap()
        ));
    }
    source.push_str("(def check (fn [unused] (if (and");
    for i in 0..cases().len() {
        source.push_str(&format!(" result{i}"));
    }
    for i in 0..protocol_cases.len() {
        source.push_str(&format!(" protocolresult{i}"));
    }
    for i in 0..scoped_cast_cases().len() {
        source.push_str(&format!(" castresult{i}"));
    }
    source.push_str(") 1 0)))\n");
    std::fs::write(dir.path().join("probe.sus"), source).unwrap();
    std::fs::write(
        dir.path().join("probe.wit"),
        "package test:numclassprobe; world w { export check: func(unused: u32) -> u32; }",
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
            "check=numclassprobe/check",
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
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "1");
}

#[test]
fn numeric_class_protocols_follow_receiver_classes() {
    use suss_compile::portable_macros::CompiledMacros;
    let definitions = include_str!("../../../results/m12-numeric-classes/protocol-definitions.sus");
    let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-numeric-classes/protocol-cases.json"
    ))
    .unwrap();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        session.eval_with_macros(definitions, &mut macros).unwrap();
        for row in &cases {
            let source = row["source"].as_str().unwrap();
            assert_eq!(
                printed(&mut session, &format!("(pr-str {source})")),
                row["expected"].as_str().unwrap(),
                "{source}"
            );
        }
    }
}

#[test]
fn numeric_class_extensions_update_sites_after_gc() {
    use suss_compile::portable_macros::CompiledMacros;
    let steps: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-numeric-classes/live-cases.json"
    ))
    .unwrap();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        for step in &steps {
            session
                .eval_with_macros(step["setup"].as_str().unwrap(), &mut macros)
                .unwrap();
            session.collect().unwrap();
            let source = step["source"].as_str().unwrap();
            assert_eq!(
                printed(&mut session, &format!("(pr-str {source})")),
                step["expected"].as_str().unwrap(),
                "{source}"
            );
        }
    }
}

#[test]
fn count_guarded_suite_forms_have_independent_exact_numeric_controls() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for row in scoped_cast_cases() {
            let source = row["source"].as_str().unwrap();
            assert_eq!(
                printed(&mut session, &format!("(pr-str {source})")),
                row["expected"].as_str().unwrap(),
                "{}: {source}",
                row["id"]
            );
        }
    }
}
