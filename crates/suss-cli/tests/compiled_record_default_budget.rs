//! Public records/types must fit unattended CLI defaults, including cold calls.
use std::io::Write;
use std::process::{Command, Output, Stdio};

const DEFINITIONS: &str = "(defrecord R [a b])\n(defrecord S [x])\n(deftype T [v])\n";
const VALUE: &str = "(+ (:a (->R 3 4)) (:x (map->S {:x 5})) (.-v (T. 9)))";

fn successful(output: Output) -> String {
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn eval_and_script_records_fit_default_compilation_budget() {
    let source = format!("{DEFINITIONS}(println {VALUE})");
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("records.sus"), &source).unwrap();
    for args in [vec!["-e", source.as_str()], vec!["records.sus"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path()).args(args).output().unwrap();
        assert_eq!(successful(output).trim(), "17");
    }
}

#[test]
fn aot_records_fit_default_budget_and_execute() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("records.sus"),
        format!("(ns records) {DEFINITIONS}(def value (fn [] {VALUE}))")).unwrap();
    std::fs::write(root.path().join("api.wit"),
        "package test:default-records; world api { export value: func() -> s64; }").unwrap();
    successful(Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path()).args(["compile", "records.sus", "-w", "api.wit",
            "--wit-world", "api", "--export", "value=records/value", "-o", "records.wasm"])
        .output().unwrap());
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path()).args(["run", "records.wasm", "--invoke", "value"])
        .output().unwrap();
    assert_eq!(successful(output).trim(), "17");
}

#[test]
fn repeated_repl_records_keep_membership_transport_below_one_tenth_of_default() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_suss"))
        .arg("repl").env("SUSS_MACRO_FUEL_TRACE", "1")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().unwrap();
    child.stdin.take().unwrap().write_all(format!("{DEFINITIONS}{VALUE}\n").as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    let trace = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(!trace.contains("Error:"), "{trace}");
    assert_eq!(successful(output).lines().last(), Some("17"));
    let mut previous = None;
    let mut costs = Vec::new();
    for line in trace.lines() {
        if !line.starts_with("macro-fuel suss.core/defrecord ") { continue; }
        if let Some(fuel) = line.strip_prefix("macro-fuel suss.core/defrecord caller-form: ") {
            previous = Some(fuel.parse::<u64>().unwrap());
        } else if let Some(fuel) = line.strip_prefix("macro-fuel suss.core/defrecord caller-environment: ") {
            costs.push(previous.take().expect("caller-form trace") - fuel.parse::<u64>().unwrap());
        }
    }
    assert_eq!(costs.len(), 2, "both cold and subsequent calls must be measured: {trace}");
    assert!(costs.iter().all(|cost| *cost < 1_000_000), "membership fuel: {costs:?}");
    eprintln!("record membership fuel: {costs:?}");
}
