//! Literal const declarations freeze compiled readers, rather than silently ignoring metadata.
use suss_cli::portable_session::Session;

fn cases() -> Vec<(String, String)> {
    let values: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-constant-definitions/cases.json"
    )).unwrap();
    values.into_iter().map(|row| (row["source"].as_str().unwrap().to_owned(), row["expected"].as_str().unwrap().to_owned())).collect()
}
fn printed(session: &mut Session, source: &str) -> String {
    let value = session.eval(source).unwrap_or_else(|e| panic!("{source}: {e}"));
    session.collect().unwrap();
    let units = session.inspect(&value, |mut store, value| {
        let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
        Ok(array.elems(&mut store)?.map(|unit| unit.unwrap_i32() as u16).collect::<Vec<_>>())
    }).unwrap();
    String::from_utf16(&units).unwrap()
}
#[test]
fn literal_constants_freeze_readers_in_runtime_and_macro() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for (source, expected) in cases() {
            let forms = suss_reader::forms::read_forms(&source).unwrap();
            let last = forms.last().unwrap().span.clone();
            let wrapped = format!("{} (pr-str {})", &source[..last.start], &source[last]);
            assert_eq!(printed(&mut session, &wrapped), expected, "{source}");
        }
        // Unsupported forms must remain explicit compile errors, never normal
        // live vars presented as constants or unexecuted values folded to literals.
        for source in ["(def ^:const computed-constant (+ 1 2))", "(if false (def ^:const conditional-constant 1) nil)"] {
            assert!(session.eval(source).is_err(), "{source}");
        }
        // Separate fragments exercise declarations retained in the phase environment.
        session.eval("(def ^:const retained-constant 21) (def retained-reader (fn [] retained-constant))").unwrap();
        assert!(session.eval("(if false (def retained-constant 0) nil)").is_err());
        session.eval("(def retained-constant 42)").unwrap();
        assert_eq!(printed(&mut session, "(pr-str [(retained-reader) retained-constant])"), "[21 42]");
        // A compile failure must not install a replacement declaration.
        assert!(session.eval("(def ^:const retained-constant 99) unresolved-constant-failure").is_err());
        session.eval("(def retained-live-reader (fn [] retained-constant)) (def retained-constant 43)").unwrap();
        assert_eq!(printed(&mut session, "(pr-str [(retained-reader) (retained-live-reader) retained-constant])"), "[21 43 43]");
    }
}
#[test]
fn aot_executes_literal_constants() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let mut source = String::from("(ns constprobe)\n");
    for (i, (expression, expected)) in cases().iter().enumerate() {
        let forms = suss_reader::forms::read_forms(expression).unwrap();
        let last = forms.last().unwrap().span.clone();
        source.push_str(&format!("{}\n(def result{i} (= (pr-str {}) {expected:?}))\n", &expression[..last.start], &expression[last]));
    }
    source.push_str("(def check (fn [unused] (if (and");
    for i in 0..cases().len() { source.push_str(&format!(" result{i}")); }
    source.push_str(") 1 0)))\n");
    std::fs::write(dir.path().join("probe.sus"), source).unwrap();
    std::fs::write(dir.path().join("probe.wit"), "package test:constprobe; world w { export check: func(unused: u32) -> u32; }").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suss")).current_dir(dir.path())
        .args(["compile", "probe.sus", "-w", "probe.wit", "--export", "check=constprobe/check", "-o", "probe.wasm"]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let output = Command::new(env!("CARGO_BIN_EXE_suss")).current_dir(dir.path())
        .args(["run", "probe.wasm", "--invoke", "check", "--", "0"]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "1");
}
