//! Exact integer dispatch, promotion and runtime reading follow JVM Clojure.
use suss_cli::portable_session::Session;

fn text(session: &mut Session, source: &str) -> String {
    let value = session
        .eval(source)
        .unwrap_or_else(|e| panic!("{source}: {e}"));
    session.collect().unwrap();
    session
        .inspect(&value, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let units = array
                .elems(&mut store)?
                .map(|v| v.unwrap_i32() as u16)
                .collect::<Vec<_>>();
            Ok(String::from_utf16(&units).unwrap())
        })
        .unwrap()
}

#[test]
fn ratios_match_jvm_in_both_phases() {
    for (phase, mut session) in [("Runtime", Session::new_repl().unwrap()), ("Macro", Session::new_macro().unwrap())] {
        // Eight rational operations on 1,024-bit operands exercise repeated
        // GC reference division/GCD. Retain the large corpus and give that
        // stress probe an explicit budget; production fuel is unchanged.
        session.set_operation_fuel(10_000_000_000);
        session.eval("(ns ratio.fixture (:require [suss.edn]))").unwrap();
        let cases: Vec<(String, String)> = serde_json::from_str(include_str!(
            "../../../results/m12-ratio-values/cases.json"
        ))
        .unwrap();
        for (index, (source, expected)) in cases.into_iter().enumerate() {
            eprintln!("{phase} Ratio case {index}");
            assert_eq!(text(&mut session, &source), expected, "{source}");
        }
    }
}

#[test]
fn aot_executes_exact_ratios() {
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    let cases: Vec<(String, String)> = serde_json::from_str(include_str!(
        "../../../results/m12-ratio-values/cases.json"
    ))
    .unwrap();
    // A distinct failure index proves every case was reached on success.
    let mut body = cases.len().to_string();
    for (index, (source, expected)) in cases.iter().enumerate().rev() {
        body = format!("(if (= {source} {expected:?}) {body} {index})");
    }
    std::fs::write(
        root.path().join("ratios.sus"),
        format!("(ns ratios (:require [suss.edn])) (def check (fn [unused] {body}))"),
    )
    .unwrap();
    std::fs::write(
        root.path().join("ratios.wit"),
        "package test:ratios; world w { export check: func(unused: u32) -> u32; }",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args([
            "compile",
            "ratios.sus",
            "-w",
            "ratios.wit",
            "-o",
            "ratios.wasm",
            "--export",
            "check=ratios/check",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args(["run", "ratios.wasm", "--invoke", "check", "--", "0"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        cases.len().to_string()
    );
}


#[test]
fn macro_arguments_and_results_retain_exact_ratio_values() {
    use suss_cli::portable_macros::CompiledMacros;
    let mut macros = CompiledMacros::new().unwrap();
    macros.define("(defmacro ratio-add [x] (+ x 1/3))").unwrap();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let value = session.eval_with_macros("(pr-str [(ratio-add 1/3) (ratio-add 2/3)])", &mut macros).unwrap();
        session.collect().unwrap();
        let observed = session.inspect(&value, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(String::from_utf16(&array.elems(&mut store)?.map(|v|v.unwrap_i32() as u16).collect::<Vec<_>>()).unwrap())
        }).unwrap();
        assert_eq!(observed, "[2/3 1N]");
    }
}

#[test]
fn retained_ratio_children_survive_gc_without_mutation() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def retained-ratio (let [x 1/3] (fn [y] [x (+ x y) (- x y) (* x y) (/ x y)])))").unwrap();
        for _ in 0..3 { session.collect().unwrap(); }
        assert_eq!(text(&mut session,"(pr-str (retained-ratio 2/3))"), "[1/3 1N -1/3 2/9 1/2]");
        for _ in 0..3 { session.collect().unwrap(); }
        assert_eq!(text(&mut session,"(pr-str (retained-ratio -2/3))"), "[1/3 -1/3 1N -2/9 -1/2]");
    }
}
