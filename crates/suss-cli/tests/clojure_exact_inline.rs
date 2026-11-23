//! Exact operands exercise both primitive-parameter and boxed numeric routes.
use suss_cli::portable_session::Session;
use wasmparser::{CompositeInnerType, Parser, Payload, TypeRef, ValType};

fn cases() -> Vec<serde_json::Value> {
    serde_json::from_str(include_str!(
        "../../../results/m12-numeric-classes/inline-cases.json"
    ))
    .unwrap()
}

fn has_long_parameter_entry(source: &str) -> bool {
    use suss_compile::portable::{self, resolve::Phase};
    let mut environment = portable::bootstrap::shipped(Phase::Runtime)
        .unwrap()
        .environment
        .clone();
    environment.enter_namespace(Phase::Runtime, "user").unwrap();
    let bytes = portable::compile_in(source, &environment, Phase::Runtime).unwrap();
    let mut types = Vec::new();
    let mut function_types = Vec::new();
    let mut imported = 0;
    let mut body_index = 0;
    let mut found = false;
    for payload in Parser::new(0).parse_all(&bytes) {
        match payload.unwrap() {
            Payload::TypeSection(section) => {
                for group in section {
                    for ty in group.unwrap().into_types() {
                        types.push(match ty.composite_type.inner {
                            CompositeInnerType::Func(ty) => Some(ty),
                            _ => None,
                        });
                    }
                }
            }
            Payload::ImportSection(section) => {
                for import in section.into_imports() {
                    if let TypeRef::Func(ty) = import.unwrap().ty {
                        function_types.push(ty);
                        imported += 1;
                    }
                }
            }
            Payload::FunctionSection(section) => {
                function_types.extend(section.into_iter().map(Result::unwrap));
            }
            Payload::CodeSectionEntry(_) => {
                let ty = types[function_types[imported + body_index] as usize]
                    .as_ref()
                    .unwrap();
                body_index += 1;
                found |= ty.params().contains(&ValType::I64);
            }
            _ => {}
        }
    }
    found
}

#[test]
fn exact_operands_match_jvm_on_raw_and_boxed_paths() {
    let cases = cases();
    for row in &cases {
        assert_eq!(
            has_long_parameter_entry(row["function"].as_str().unwrap()),
            row["route"] == "raw",
            "{} {}",
            row["label"],
            row["route"]
        );
    }
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for row in &cases {
            let source = row["source"].as_str().unwrap();
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
            assert_eq!(found, row["expected"].as_str().unwrap(), "{source}");
        }
    }
}

#[test]
fn aot_executes_raw_and_boxed_exact_operand_paths() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let mut source = String::from("(ns exactinline)\n(def check (fn [unused] (if (and\n");
    for row in cases() {
        source.push_str(&format!(
            "(= (pr-str {}) {:?})\n",
            row["source"].as_str().unwrap(),
            row["expected"].as_str().unwrap()
        ));
    }
    source.push_str(") 1 0)))\n");
    std::fs::write(dir.path().join("probe.sus"), source).unwrap();
    std::fs::write(
        dir.path().join("probe.wit"),
        "package test:exactinline; world w { export check: func(unused: u32) -> u32; }",
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
            "check=exactinline/check",
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
