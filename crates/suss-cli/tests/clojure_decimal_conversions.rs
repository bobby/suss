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
fn decimal_conversions_match_checked_and_unchecked_clojure_paths() {
    check_cases(include_str!(
        "../../../results/m12-decimal-conversions/conversion-cases.json"
    ));
}

#[test]
fn aot_executes_exact_decimal_conversion_paths() {
    use std::process::Command;
    let cases = [
        include_str!("../../../results/m12-decimal-conversions/conversion-cases.json"),
        include_str!("../../../results/m12-decimal-conversions/context-coercion-cases.json"),
    ]
    .iter()
    .flat_map(|fixture| serde_json::from_str::<Vec<(String, String)>>(fixture).unwrap())
    .collect::<Vec<_>>();
    let dir = tempfile::tempdir().unwrap();
    let mut source = String::from("(ns decimalconversionprobe)\n");
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
        "package test:decimalconversionprobe; world w { export check: func(unused: u32) -> u32; }",
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
            "check=decimalconversionprobe/check",
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

#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;

#[test]
fn direct_float_rounding_matches_primary_bits_and_preserves_decimal_storage_after_gc() {
    use portable_decode::{Decoder, Observation};
    let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-decimal-conversions/float-fields-cases.json"
    ))
    .unwrap();
    for phase in ["Runtime", "Macro"] {
        let mut session = if phase == "Runtime" {
            Session::new_repl().unwrap()
        } else {
            Session::new_macro().unwrap()
        };
        let mut decoder = Decoder::capture_numeric(&mut session, 10000).unwrap();
        for row in &cases {
            let source = format!("{}M", row["text"].as_str().unwrap());
            session
                .eval(&format!("(def float-source {source})"))
                .unwrap();
            let original = session.eval("float-source").unwrap();
            session.collect().unwrap();
            decoder.refill(10000);
            let before = decoder.decode_session(&mut session, &original).unwrap();
            assert!(matches!(before, Observation::Decimal { .. }));
            let actual = session.eval("(unchecked-float float-source)").unwrap();
            session.collect().unwrap();
            decoder.refill(10000);
            assert_eq!(
                decoder.decode_session(&mut session, &actual).unwrap(),
                Observation::Number(
                    u64::from_str_radix(row["f64_bits"].as_str().unwrap(), 16).unwrap()
                ),
                "{phase}: {source}"
            );
            let retained = session.eval("float-source").unwrap();
            session.collect().unwrap();
            decoder.refill(10000);
            assert_eq!(
                decoder.decode_session(&mut session, &retained).unwrap(),
                before,
                "{phase}: {source} must retain its exact fields"
            );
        }
    }
}

#[test]
fn pure_decimal_coercions_and_comparisons_ignore_unused_math_context() {
    check_cases(include_str!(
        "../../../results/m12-decimal-conversions/context-coercion-cases.json"
    ));
}

#[test]
fn reloaded_core_context_getter_reads_live_bindings_for_retained_functions() {
    const CORE: &str = include_str!("../../../runtime/core-import/suss/core.sus");
    for phase in ["Runtime", "Macro"] {
        let phase_kind = if phase == "Runtime" {
            suss_compile::portable::resolve::Phase::Runtime
        } else {
            suss_compile::portable::resolve::Phase::Macro
        };
        let mut session = Session::with_options_in(Default::default(), phase_kind).unwrap();
        session
            .eval_privileged(CORE)
            .unwrap_or_else(|e| panic!("{phase} initial source core: {e:?}"));
        session.enter_namespace("user").unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        session.eval("(def retained-context-reader (fn [] (str *math-context*))) (def retained-decimal-div (fn [] (/ 1M 3)))").unwrap();
        session
            .eval_privileged(CORE)
            .unwrap_or_else(|e| panic!("{phase} core reload: {e:?}"));
        // An initializer failure after installing the replacement must leave
        // retained functions able to read subsequent ordinary dynamic frames.
        assert!(
            matches!(
                session.eval_privileged(&format!("{CORE}\n(/ 1 0)")),
                Err(suss_cli::portable_session::SessionError::Language(_))
            ),
            "{phase}: deliberately failed reload"
        );
        session.enter_namespace("user").unwrap();
        session.collect().unwrap();
        for (source, expected) in [
            (
                "(with-precision 3 (retained-context-reader))",
                "\"precision=3 roundingMode=HALF_UP\"",
            ),
            (
                "(with-precision 3 (pr-str (retained-decimal-div)))",
                "\"0.333M\"",
            ),
        ] {
            assert_eq!(
                observed(&mut session, &mut macros, source),
                Ok(expected.into()),
                "{phase}: {source}"
            );
        }
    }
}
