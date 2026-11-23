//! The shipped core, not a local replacement, executes the fresh primary controls.
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};

fn cases() -> Vec<serde_json::Value> {
    let original: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-core-dependencies/cases.json")).unwrap();
    let adapted: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-core-dependencies/probe-cases.json")).unwrap();
    let rows: Vec<_> = adapted.into_iter().filter(|row|
        row["name"].as_str().unwrap().starts_with("min-key-")).collect();
    assert_eq!(rows.len(), 32);
    for row in &rows {
        let primary = original.iter().find(|x| x["name"] == row["name"]).unwrap();
        assert_eq!(row["expected"], primary["expected"]);
        assert_eq!(row["original_expression"], primary["expression"]);
    }
    // Six pure map-indexed fixture constructors are materialized. Their
    // equivalence to every untouched primary expression was separately executed
    // on the pinned JVM; no min-key body or key-function trace is substituted.
    assert_eq!(rows.iter().filter(|r| r["expression"] != r["original_expression"]).count(), 6);
    rows
}

fn expected(value: &suss_reader::edn::Value) -> Observation {
    use suss_reader::edn::Value;
    match value {
        Value::Nil => Observation::Nil,
        Value::Bool(value) => Observation::Bool(*value),
        Value::Long(value) => Observation::Long(*value),
        Value::String(value) => Observation::String(value.encode_utf16().collect()),
        Value::Keyword(value) => Observation::Keyword(
            value.namespace.as_ref().map(|x| x.encode_utf16().collect()),
            value.name.encode_utf16().collect()),
        Value::Vector(values) => Observation::Vector(values.iter().map(expected).collect()),
        other => panic!("unassessed primary min-key observation: {other:?}"),
    }
}

#[test]
fn shipped_min_key_records_primary_values_and_key_effects_in_both_phases() {
    let rows = cases();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        let mut retained = Vec::new();
        for row in &rows {
            let source = row["expression"].as_str().unwrap().replace("(catch Throwable ", "(catch :default ");
            let value = session.eval_with_macros(&source, &mut macros)
                .unwrap_or_else(|error| panic!("{}: {error}", row["name"]));
            retained.push(value);
        }
        session.eval("(def min-key (fn [& xs] (throw :min-key))) (def pr-str (fn [& xs] (throw :print))) (def = (fn [& xs] (throw :equality)))").unwrap();
        session.collect().unwrap();
        for (value, row) in retained.iter().zip(&rows) {
            let primary = suss_reader::edn::read_one(row["expected"].as_str().unwrap(),
                &suss_reader::edn::Options::default()).unwrap();
            assert_eq!(decoder.decode_session(&mut session, value).unwrap(), expected(&primary),
                "{}", row["name"]);
        }
    }
}

#[test]
fn actual_aot_returns_min_key_values_and_key_effects_for_host_comparison() {
    use std::process::Command;
    let rows = cases();
    let root = tempfile::tempdir().unwrap();
    // Keep the ordinary default compile budget: each bounded component carries
    // eight complete controls, with no guest comparison deciding a result.
    for rows in rows.chunks(8) {
        let mut source = String::from("(ns min-key-aot)\n");
        for (index, row) in rows.iter().enumerate() {
            source.push_str(&format!("(def result{index} {})\n",
                row["expression"].as_str().unwrap().replace("(catch Throwable ", "(catch :default ")));
        }
        source.push_str("(def report (fn [] (pr-str [");
        for index in 0..rows.len() { source.push_str(&format!(" result{index}")); }
        source.push_str("])))\n");
        std::fs::write(root.path().join("app.sus"), source).unwrap();
        std::fs::write(root.path().join("app.wit"),
            "package test:min-key; world w { export report: func() -> string; }").unwrap();
        let compiled = Command::new(env!("CARGO_BIN_EXE_suss")).current_dir(root.path())
            .args(["--no-cache", "compile", "app.sus", "-w", "app.wit", "--export",
                "report=min-key-aot/report", "-o", "app.wasm"]).output().unwrap();
        assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
        let executed = Command::new(env!("CARGO_BIN_EXE_suss")).current_dir(root.path())
            .args(["--no-cache", "run", "app.wasm", "--invoke", "report"]).output().unwrap();
        assert!(executed.status.success(), "{}", String::from_utf8_lossy(&executed.stderr));
        let actual = suss_reader::edn::read_one(String::from_utf8(executed.stdout).unwrap().trim_end(),
            &suss_reader::edn::Options::default()).unwrap();
        let wanted = format!("[{}]", rows.iter().map(|r| r["expected"].as_str().unwrap())
            .collect::<Vec<_>>().join(" "));
        let wanted = suss_reader::edn::read_one(&wanted, &suss_reader::edn::Options::default()).unwrap();
        assert_eq!(actual, wanted);
    }
}
