//! Checked character coercion follows source-selected Clojure numeric overloads.
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};

fn cases() -> Vec<serde_json::Value> {
    serde_json::from_str(include_str!(
        "../../../results/m12-character-coercion/primary/cases.json"
    ))
    .unwrap()
}
fn all_cases() -> Vec<serde_json::Value> {
    let mut rows = cases();
    rows.extend(
        serde_json::from_str::<Vec<serde_json::Value>>(include_str!(
            "../../../results/m12-character-coercion/float-primary/cases.json"
        ))
        .unwrap(),
    );
    rows.extend(
        serde_json::from_str::<Vec<serde_json::Value>>(include_str!(
            "../../../results/m12-character-coercion/object-float-probe/cases.json"
        ))
        .unwrap(),
    );
    rows
}
fn range_message(row: &serde_json::Value) -> &str {
    if row["platform"] == "adr0003-float-as-double" {
        assert_eq!(row["expression"], "((identity char) (float -70000.1))");
        assert_eq!(row["message"], "Value out of range for char: -70000.1");
        // ADR-0003 explicitly returns a Double from float. Preserve the
        // widened original operand; this is an observed platform difference.
        "Value out of range for char: -70000.1015625"
    } else {
        row["message"].as_str().unwrap()
    }
}
fn keyword(value: &str) -> Observation {
    Observation::Keyword(None, value.encode_utf16().collect())
}
fn expected(row: &serde_json::Value) -> Observation {
    // The accepted representation identifies a one-unit String and character.
    // Retain these three actual JVM disagreements; never relabel them matches.
    if row["input"] == "\"a\"" {
        assert_eq!(row["error_class"], "java.lang.ClassCastException");
        return Observation::Vector(vec![keyword("ok"), Observation::String(vec![97])]);
    }
    if let Some(unit) = row["unit"].as_u64() {
        assert!(
            unit <= u16::MAX as u64,
            "primary character must be a UTF-16 unit"
        );
        Observation::Vector(vec![keyword("ok"), Observation::String(vec![unit as u16])])
    } else {
        Observation::Vector(vec![
            keyword("throws"),
            Observation::String(range_message(row).encode_utf16().collect()),
        ])
    }
}

#[test]
fn character_source_overloads_match_primary_in_both_phases() {
    let cases = all_cases();
    assert_eq!(cases.len(), 369);
    assert_eq!(
        cases.iter().filter(|row| row["input"] == "\"a\"").count(),
        3
    );
    check_both_phases(&cases);
    println!("CHAR_PRIMARY_PER_PHASE exact=365 explicit_platform_differences=4 missing=0");
}

fn check_both_phases(cases: &[serde_json::Value]) {
    let mut failures = Vec::new();
    for phase in ["Runtime", "Macro"] {
        let mut session = if phase == "Runtime" {
            Session::new_repl()
        } else {
            Session::new_macro()
        }
        .unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        for row in cases {
            let source = format!(
                "(try [:ok {}] (catch :default e [:throws (ex-message e)]))",
                row["expression"].as_str().unwrap()
            );
            let value = session.eval_with_macros(&source, &mut macros).unwrap();
            session.collect().unwrap();
            decoder.refill(20_000);
            let actual = decoder.decode_session(&mut session, &value).unwrap();
            let expected = expected(row);
            if actual != expected {
                failures.push(format!(
                    "{phase} {} {}\nexpected {expected:?}\nactual {actual:?}",
                    row["name"], row["expression"]
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn exact_number_carriers_and_context_keep_character_coercion_in_all_routes() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-character-coercion/exact-number-primary/cases.json"
    ))
    .unwrap();
    assert_eq!(rows.len(), 32);
    check_both_phases(&rows);
    check_actual_aot(&rows);
}

#[test]
fn character_units_remain_retained_after_gc_without_public_operations() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        let value = session
            .eval_with_macros(
                "[(char 0) ((identity char) 55296) ((fn [x] (char x)) 65535)]",
                &mut macros,
            )
            .unwrap();
        session.eval("(def char? (fn [x] false)) (def char (fn [x] (throw :char))) (def pr-str (fn [& xs] (throw :print))) (def str (fn [& xs] (throw :str)))").unwrap();
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Vector(vec![
                Observation::String(vec![0]),
                Observation::String(vec![55296]),
                Observation::String(vec![65535]),
            ])
        );
    }
}

#[test]
fn character_arguments_keep_primary_effect_order_and_arity_failures() {
    use suss_reader::edn::{Options, Value, read_one};
    fn observation(value: &Value) -> Observation {
        match value {
            Value::Keyword(key) => Observation::Keyword(
                key.namespace.as_ref().map(|v| v.encode_utf16().collect()),
                key.name.encode_utf16().collect(),
            ),
            Value::String(value) => Observation::String(value.encode_utf16().collect()),
            Value::Vector(items) => Observation::Vector(items.iter().map(observation).collect()),
            other => panic!("unexpected primary control {other:?}"),
        }
    }
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-character-coercion/effects/cases.json"
    ))
    .unwrap();
    assert_eq!(rows.len(), 7);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        for row in &rows {
            let source = row["source"]
                .as_str()
                .unwrap()
                .replace("(catch Throwable ", "(catch :default ");
            let value = session.eval_with_macros(&source, &mut macros).unwrap();
            session.collect().unwrap();
            decoder.refill(20_000);
            let primary = read_one(row["expected"].as_str().unwrap(), &Options::default()).unwrap();
            assert_eq!(
                decoder.decode_session(&mut session, &value).unwrap(),
                observation(&primary),
                "{source}"
            );
        }
    }
}

#[test]
fn actual_aot_reports_character_storage_and_primary_errors() {
    check_actual_aot(&all_cases());
}

fn check_actual_aot(rows: &[serde_json::Value]) {
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    for rows in rows.chunks(32) {
        let mut source = String::from("(ns char-aot)\n");
        let mut expected = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            source.push_str(&format!("(def result{i} (try (let [c {}] [:ok [(.-length c) (.charCodeAt c 0)]]) (catch :default e [:throws (ex-message e)])))\n", row["expression"].as_str().unwrap()));
            if let Some(unit) = row["unit"]
                .as_u64()
                .or_else(|| (row["input"] == "\"a\"").then_some(97))
            {
                expected.push(format!("[:ok [1 {unit}]]"));
            } else {
                expected.push(format!(
                    "[:throws {}]",
                    serde_json::to_string(range_message(row)).unwrap()
                ));
            }
        }
        source.push_str("(def report (fn [] (pr-str [");
        for i in 0..rows.len() {
            source.push_str(&format!(" result{i}"));
        }
        source.push_str("])))\n");
        std::fs::write(root.path().join("app.sus"), source).unwrap();
        std::fs::write(
            root.path().join("app.wit"),
            "package test:character; world w { export report: func() -> string; }",
        )
        .unwrap();
        let compiled = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args([
                "--no-cache",
                "compile",
                "app.sus",
                "-w",
                "app.wit",
                "--export",
                "report=char-aot/report",
                "-o",
                "app.wasm",
            ])
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        let executed = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args(["--no-cache", "run", "app.wasm", "--invoke", "report"])
            .output()
            .unwrap();
        assert!(
            executed.status.success(),
            "{}",
            String::from_utf8_lossy(&executed.stderr)
        );
        assert_eq!(
            String::from_utf8(executed.stdout).unwrap().trim_end(),
            format!("[{}]", expected.join(" "))
        );
    }
}

#[test]
fn actual_aot_keeps_character_argument_effect_order() {
    use std::process::Command;
    use suss_reader::edn::{Options, Value, read_one};
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-character-coercion/effects/cases.json"
    ))
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut source = String::from("(ns char-effects)\n");
    for (i, row) in rows.iter().enumerate() {
        source.push_str(&format!(
            "(def result{i} {})\n",
            row["source"]
                .as_str()
                .unwrap()
                .replace("(catch Throwable ", "(catch :default ")
        ));
    }
    source.push_str("(def report (fn [] (pr-str [");
    for i in 0..rows.len() {
        source.push_str(&format!(" result{i}"));
    }
    source.push_str("])))\n");
    std::fs::write(root.path().join("app.sus"), source).unwrap();
    std::fs::write(
        root.path().join("app.wit"),
        "package test:character-effects; world w { export report: func() -> string; }",
    )
    .unwrap();
    let compiled = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args([
            "--no-cache",
            "compile",
            "app.sus",
            "-w",
            "app.wit",
            "--export",
            "report=char-effects/report",
            "-o",
            "app.wasm",
        ])
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let executed = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args(["--no-cache", "run", "app.wasm", "--invoke", "report"])
        .output()
        .unwrap();
    assert!(
        executed.status.success(),
        "{}",
        String::from_utf8_lossy(&executed.stderr)
    );
    let output = String::from_utf8(executed.stdout).unwrap();
    let expected = Value::Vector(
        rows.iter()
            .map(|row| read_one(row["expected"].as_str().unwrap(), &Options::default()).unwrap())
            .collect(),
    );
    assert_eq!(
        read_one(output.trim_end(), &Options::default()).unwrap(),
        expected
    );
}
