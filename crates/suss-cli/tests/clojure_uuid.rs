//! UUID values and tagged readers follow executed JVM controls.
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

fn check_cases(fixtures: &[&str]) {
    let cases: Vec<(String, String)> = fixtures
        .iter()
        .flat_map(|fixture| serde_json::from_str::<Vec<serde_json::Value>>(fixture).unwrap().into_iter()
            .filter(|row| row.get("name").and_then(|v| v.as_str()).is_none_or(|name| name.starts_with("uuid-")))
            .map(|row| (row["expression"].as_str().unwrap().to_owned(), row["expected"].as_str().unwrap().to_owned())))
        .collect();
    let mut failures = Vec::new();
    for phase in ["Runtime", "Macro"] {
        let mut session = if phase == "Runtime" {
            Session::new_repl().unwrap()
        } else {
            Session::new_macro().unwrap()
        };
        session.set_operation_fuel(100_000_000);
        let mut macros = CompiledMacros::new().unwrap();
        session.eval_with_macros("(ns uuid-controls (:require [suss.edn]))", &mut macros).unwrap();
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
fn uuid_values_and_readers_follow_primary_in_both_phases() {
    check_cases(&[include_str!("../../../results/m12-reader-followup/reader-cases.json")]);
}

#[test]
fn clojure_edn_namespace_exposes_the_same_data_reader_in_both_phases() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        session.eval_with_macros(
            "(ns edn-compatibility (:require [clojure.edn :as edn] [suss.edn :as suss-edn]))",
            &mut macros,
        ).unwrap();
        assert_eq!(observed(&mut session, &mut macros, r##"[
          (identical? edn/read-string suss-edn/read-string)
          (str (edn/read-string "#uuid \"1-2-3-4-5\""))
          (edn/read-string "1.25M")
          (edn/read-string {:eof :done} " ")
          (edn/read-string {:readers {'uuid (fn [x] [:override x])}}
                           "#uuid \"short\"")
          (edn/read-string {:default (fn [tag x] [tag x])} "#custom [1 2]")
          (try (edn/read-string "#=(+ 1 2)") :evaluated (catch :default e :rejected))]"##).unwrap(),
          "[true \"00000001-0002-0003-0004-000000000005\" 1.25M :done [:override \"short\"] [custom [1 2]] :rejected]");
    }
}

#[test]
fn all_uuid_runtime_parser_inputs_and_lossless_errors_follow_primary() {
    let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-reader-followup/uuid-runtime-cases.json")).unwrap();
    assert_eq!(cases.len(), 235);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let mut macros = CompiledMacros::new().unwrap();
        session.eval_with_macros("(ns uuid-controls (:require [suss.edn]))", &mut macros).unwrap();
        for (index, row) in cases.iter().enumerate() {
            let source = row["expression"].as_str().unwrap();
            let value = session.eval_with_macros(&format!("(pr-str {source})"), &mut macros).unwrap();
            session.collect().unwrap();
            let units = session.inspect(&value, |mut store, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                Ok(array.elems(&mut store)?.map(|v| v.unwrap_i32() as u16).collect::<Vec<_>>())
            }).unwrap();
            let expected: Vec<u16> = serde_json::from_value(row["expected_units"].clone()).unwrap();
            assert_eq!(units, expected, "parser case{index}: {source}");
        }
    }
}
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;

#[test]
fn retained_uuid_storage_is_nominal_exact_and_independent_after_gc() {
    use portable_decode::{Decoder, Observation};
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        session.eval_with_macros("(ns uuid-controls (:require [suss.edn]))", &mut macros).unwrap();
        macros.enter_namespace("uuid-controls").unwrap();
        macros.define("(defmacro uuid-forward [form] form)").unwrap();
        let value = session.eval_with_macros(r##"(do
          (def retained [(uuid-forward #uuid "80000000-0000-0001-ffff-fffffffffffe")
                        '#uuid "7fffffff-ffff-fffe-8000-000000000001"
                        (read-string "#uuid \"1-2-3-4-5\"")])
          retained)"##, &mut macros).unwrap();
        // Direct storage observations must not call these public operations.
        session.eval("(def = (fn [& xs] (throw :equality))) (def hash (fn [& xs] (throw :hash))) (def pr-str (fn [& xs] (throw :print))) (def str (fn [& xs] (throw :str))) (def uuid? (fn [x] false))").unwrap();
        session.collect().unwrap();
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::Vector(vec![
            Observation::Uuid { most: i64::MIN + 1, least: -2 },
            Observation::Uuid { most: i64::MAX - 1, least: i64::MIN + 1 },
            Observation::Uuid { most: 0x100020003, least: 0x4000000000005 },
        ]));
        for source in ["(new UUID nil 0)", "(new UUID 0 0.0)"] {
            if let Ok(malformed) = session.eval(source) {
                assert!(decoder.decode_session(&mut session, &malformed).is_err(), "{source}");
            }
        }
    }
}

#[test]
fn signed_boundaries_keys_and_quotation_follow_primary() {
    check_cases(&[include_str!("../../../results/m12-reader-followup/uuid-boundary-cases.json")]);
}
#[test]
fn uuid_fields_and_private_width_adapters_remain_sealed() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for source in [
            "(.-most #uuid \"1-2-3-4-5\")",
            "(set! (.-least #uuid \"1-2-3-4-5\") 0)",
            "(.-prototype UUID)",
        ] {
            assert_eq!(observed(&mut session, &mut CompiledMacros::new().unwrap(),
                &format!("(try {source} (catch :default e (ex-message e)))")).unwrap(),
                "\"Built-in type internals are private to suss.core\"", "{source}");
        }
        for source in ["(suss.bootstrap/long-bit-or 1 2)", "(suss.core/uuid-from-parts 0 0)"] {
            assert!(session.eval(source).is_err(), "{source}");
        }
    }
}

fn check_aot_cases(fixtures: &[&str]) {
    use std::process::Command;
    let cases = fixtures.iter()
    .flat_map(|fixture| serde_json::from_str::<Vec<serde_json::Value>>(fixture).unwrap().into_iter()
        .filter(|row| row.get("name").and_then(|v| v.as_str()).is_none_or(|name| name.starts_with("uuid-")))
        .map(|row| (row["expression"].as_str().unwrap().to_owned(), row["expected"].as_str().unwrap().to_owned())))
    .collect::<Vec<_>>();
    let dir = tempfile::tempdir().unwrap();
    let mut source = String::from("(ns caseprobe (:require [suss.edn]))\n");
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
        "package test:caseprobe; world w { export check: func(unused: u32) -> u32; }",
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
            "check=caseprobe/check",
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

#[test]
fn actual_aot_executes_uuid_values_readers_and_signed_boundaries() {
    check_aot_cases(&[
        include_str!("../../../results/m12-reader-followup/reader-cases.json"),
        include_str!("../../../results/m12-reader-followup/uuid-boundary-cases.json"),
    ]);
}

#[test]
fn ordinary_source_core_loads_uuid_readers_in_both_phases() {
    use suss_cli::portable_session::{SessionError, SessionOptions};
    use suss_compile::portable::resolve::Phase;
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
        let accessor = session.eval("suss.core/ex-message").unwrap();
        if let Err(error) = session.eval(include_str!("../../../runtime/core-import/suss/core.sus")) {
            let text = if let SessionError::Language(payload) = &error {
                session.invoke(&accessor, &[payload]).and_then(|value| session.inspect(&value, |mut store, value| {
                    let Some(reference) = value.unwrap_anyref() else { return Ok("nil diagnostic".into()); };
                    let Some(array) = reference.as_array(&store)? else { return Ok("non-string diagnostic".into()); };
                    let units = array.elems(&mut store)?.map(|v| v.unwrap_i32() as u16).collect::<Vec<_>>();
                    Ok(String::from_utf16_lossy(&units))
                })).unwrap_or_else(|e| e.to_string())
            } else { error.to_string() };
            panic!("{phase:?} source core load: {text}");
        }
        session.enter_namespace("user").unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        assert_eq!(observed(&mut session, &mut macros,
            "(let [x (read-string \"#uuid \\\"1-2-3-4-5\\\"\")] [(uuid? x) (str x) (hash x)])").unwrap(),
            "[true \"00000001-0002-0003-0004-000000000005\" 393223]");
    }
}
