//! Decimal arithmetic and dynamic precision follow pinned JVM expectations.
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};

const MACRO_DEFINITIONS: &str =
    "(defmacro subtraction-control [operation x y] (list operation x y))";

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

fn check_cases(fixture: &str) {
    let cases: Vec<(String, String)> = serde_json::from_str(fixture).unwrap();
    let mut failures = Vec::new();
    for phase in ["Runtime", "Macro"] {
        let mut session = if phase == "Runtime" {
            Session::new_repl().unwrap()
        } else {
            Session::new_macro().unwrap()
        };
        session.set_operation_fuel(100_000_000);
        let mut macros = CompiledMacros::new().unwrap();
        macros.define(MACRO_DEFINITIONS).unwrap();
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
fn decimal_arithmetic_and_precision_match_clojure_in_both_phases() {
    check_cases(include_str!(
        "../../../results/m12-decimal-arithmetic/cases.json"
    ));
}

#[test]
fn decimal_equality_hash_order_case_and_signs_match_clojure() {
    check_cases(include_str!(
        "../../../results/m12-decimal-integration/equality-cases.json"
    ));
}

#[test]
fn decimal_bigdec_conversions_match_clojure() {
    check_cases(include_str!(
        "../../../results/m12-decimal-integration/bigdec-cases.json"
    ));
}

#[test]
fn decimal_additional_precision_and_dynamic_context_match_clojure() {
    check_cases(include_str!(
        "../../../results/m12-decimal-integration/additional-arithmetic-cases.json"
    ));
}

#[test]
fn decimal_subtraction_preserves_source_and_boxed_overloads() {
    check_cases(include_str!(
        "../../../results/m12-decimal-integration/subtraction-dispatch-cases.json"
    ));
}

#[test]
fn subtraction_boundaries_preserve_checked_promoting_and_unchecked_dispatch() {
    check_cases(include_str!(
        "../../../results/m12-decimal-integration/subtraction-boundaries-cases.json"
    ));
}

#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;

#[test]
fn decimal_fields_are_independently_decoded_after_gc() {
    let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-decimal-integration/decimal-fields-cases.json"
    ))
    .unwrap();
    for phase in ["Runtime", "Macro"] {
        let mut session = if phase == "Runtime" {
            Session::new_repl().unwrap()
        } else {
            Session::new_macro().unwrap()
        };
        let mut decoder = portable_decode::Decoder::capture_numeric(&mut session, 10000).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        for row in &cases {
            let source = row["source"].as_str().unwrap();
            let value = session
                .eval_with_macros(source, &mut macros)
                .unwrap_or_else(|e| panic!("{phase}: {source}: {e}"));
            session.collect().unwrap();
            decoder.refill(10000);
            let actual = decoder
                .decode_session(&mut session, &value)
                .unwrap_or_else(|e| panic!("{phase}: {source}: {e}"));
            assert_eq!(
                actual,
                portable_decode::Observation::Decimal {
                    unscaled: row["unscaled"].as_str().unwrap().into(),
                    scale: i32::try_from(row["scale"].as_i64().unwrap()).unwrap(),
                },
                "{phase}: {source}"
            );
        }
    }
}

#[test]
fn aot_executes_decimal_arithmetic_and_subtraction_dispatch() {
    use std::process::Command;
    let fixtures = [
        include_str!("../../../results/m12-decimal-arithmetic/cases.json"),
        include_str!("../../../results/m12-decimal-integration/equality-cases.json"),
        include_str!("../../../results/m12-decimal-integration/bigdec-cases.json"),
        include_str!("../../../results/m12-decimal-integration/additional-arithmetic-cases.json"),
        include_str!("../../../results/m12-decimal-integration/subtraction-dispatch-cases.json"),
        include_str!("../../../results/m12-decimal-integration/subtraction-boundaries-cases.json"),
        include_str!(
            "../../../results/m12-decimal-integration/subtraction-source-facts-cases.json"
        ),
    ];
    let cases = fixtures
        .iter()
        .flat_map(|fixture| serde_json::from_str::<Vec<(String, String)>>(fixture).unwrap())
        .collect::<Vec<_>>();
    let root = tempfile::tempdir().unwrap();
    let mut source = format!("(ns decimalarithprobe)\n{MACRO_DEFINITIONS}\n");
    for (index, (expression, expected)) in cases.iter().enumerate() {
        let expression = expression.replace("(catch Throwable ", "(catch :default ");
        source.push_str(&format!("(def result{index} (= (pr-str (try {expression} (catch :default e [:error (ex-message e)]))) {expected:?}))\n"));
    }
    // Report the first mismatching primary control. Initializers execute each
    // expression independently with ordinary AOT limits and genuine source hints.
    source.push_str("(def check (fn [unused] (cond\n");
    for index in 0..cases.len() {
        source.push_str(&format!("(not result{index}) {}\n", index + 1));
    }
    source.push_str(":else 0)))\n");
    std::fs::write(root.path().join("probe.sus"), source).unwrap();
    std::fs::write(
        root.path().join("probe.wit"),
        "package test:decimalarithprobe; world w { export check: func(unused: u32) -> u32; }",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args([
            "compile",
            "probe.sus",
            "-w",
            "probe.wit",
            "--export",
            "check=decimalarithprobe/check",
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
        .current_dir(root.path())
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

#[test]
fn subtraction_source_facts_survive_captures_loops_methods_and_macro_transport() {
    check_cases(include_str!(
        "../../../results/m12-decimal-integration/subtraction-source-facts-cases.json"
    ));
}
