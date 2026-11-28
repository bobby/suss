//! Actual checked Pattern literals in the compiled Runtime and Macro hosts.
//! This scope does not claim matching APIs, JVM interop or runtime regex reading.
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};

#[test]
fn checked_pattern_literal_values_match_primary_in_both_phases() {
    let controls: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-pattern-values/value-primary/observations.json"
    ))
    .unwrap();
    let names = [
        "separate-literal-equality",
        "separate-literal-identity",
        "same-binding-equality",
        "same-binding-identity",
        "same-function-literal",
        "same-function-literal-equality",
        "different-function-literal",
        "loop-site-retention",
        "eval-pattern-self",
        "eval-quoted-pattern-self",
        "eval-generated-vector-pattern",
        "print-pattern",
        "str-pattern",
        "regex-backslash-print",
        "regex-backslash-str",
        "regex-inline-flags",
        "pattern-meta",
        "numeric-predicate-pattern",
    ];
    let mut failures = vec![];
    for phase in ["Runtime", "Macro"] {
        let mut session = if phase == "Runtime" {
            Session::new_repl().unwrap()
        } else {
            Session::new_macro().unwrap()
        };
        session.set_operation_fuel(100_000_000);
        let mut macros = CompiledMacros::new().unwrap();
        for name in names {
            let row = controls.iter().find(|row| row["name"] == name).unwrap();
            let source = format!("(pr-str {})", row["expression"].as_str().unwrap());
            let actual = session
                .eval_with_macros(&source, &mut macros)
                .and_then(|value| {
                    session.collect()?;
                    session.inspect(&value, |mut store, value| {
                        let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                        let units = array
                            .elems(&mut store)?
                            .map(|v| v.unwrap_i32() as u16)
                            .collect::<Vec<_>>();
                        Ok(String::from_utf16(&units).unwrap())
                    })
                });
            if actual.as_ref().ok().map(String::as_str) != row["observed_edn"].as_str() {
                failures.push(format!(
                    "{phase} {name}: expected {}, actual {actual:?}",
                    row["observed_edn"]
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn pattern_collection_values_match_all_primary_collection_controls() {
    let controls: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-pattern-values/collection-primary/observations.json"
    ))
    .unwrap();
    assert_eq!(controls.len(), 6);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        for row in &controls {
            let value = session
                .eval_with_macros(
                    &format!("(pr-str {})", row["expression"].as_str().unwrap()),
                    &mut macros,
                )
                .unwrap();
            session.collect().unwrap();
            let printed = session
                .inspect(&value, |mut store, value| {
                    let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                    Ok(String::from_utf16(
                        &array
                            .elems(&mut store)?
                            .map(|v| v.unwrap_i32() as u16)
                            .collect::<Vec<_>>(),
                    )
                    .unwrap())
                })
                .unwrap();
            assert_eq!(
                printed,
                row["observed_edn"].as_str().unwrap(),
                "{}",
                row["name"]
            );
        }
    }
}

#[test]
fn actual_aot_retains_literal_sites_and_primary_pattern_values() {
    use std::process::Command;
    let controls: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-pattern-values/value-primary/observations.json"
    ))
    .unwrap();
    let names = [
        "separate-literal-equality",
        "separate-literal-identity",
        "same-binding-equality",
        "same-binding-identity",
        "same-function-literal",
        "same-function-literal-equality",
        "different-function-literal",
        "loop-site-retention",
        "print-pattern",
        "str-pattern",
        "regex-backslash-print",
        "regex-backslash-str",
        "regex-inline-flags",
        "pattern-meta",
        "numeric-predicate-pattern",
    ];
    let mut cases: Vec<_> = names
        .iter()
        .map(|name| {
            controls
                .iter()
                .find(|row| row["name"] == *name)
                .unwrap()
                .clone()
        })
        .collect();
    cases.extend(
        serde_json::from_str::<Vec<serde_json::Value>>(include_str!(
            "../../../results/m12-pattern-values/collection-primary/observations.json"
        ))
        .unwrap(),
    );
    assert_eq!(cases.len(), 21);
    let source = cases
        .iter()
        .map(|row| row["expression"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join(" ");
    let expected = format!(
        "[{}]",
        cases
            .iter()
            .map(|row| row["observed_edn"].as_str().unwrap())
            .collect::<Vec<_>>()
            .join(" ")
    );
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("app.sus"),
        format!("(ns pattern-aot) (def report (fn [] (pr-str [{source}])))"),
    )
    .unwrap();
    std::fs::write(
        root.path().join("app.wit"),
        "package test:pattern-values; world w { export report: func() -> string; }",
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
            "report=pattern-aot/report",
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
    for _ in 0..2 {
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
            expected
        );
    }
}

#[test]
fn actual_pattern_storage_and_macro_transfer_remain_rooted_and_sealed() {
    use suss_compile::portable_session::{RuntimeNumber, runtime_number};
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(r##"(defmacro retained-pattern [] #"(?i)a")"##)
            .unwrap();
        let anchor = session.eval(r##"#"anchor""##).unwrap();
        let (descriptor, owner_type) = session
            .inspect(&anchor, |mut store, value| {
                let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let owner_type = object.ty(&store)?;
                let fields = object.fields(&mut store)?.collect::<Vec<_>>();
                assert_eq!(fields.len(), 4);
                Ok((
                    fields[0]
                        .unwrap_anyref()
                        .unwrap()
                        .to_owned_rooted(&mut store)?,
                    owner_type,
                ))
            })
            .unwrap();
        let mut retained = Vec::new();
        for source in [r##"#"(?i)a""##, r##"'#"(?i)a""##, "(retained-pattern)"] {
            retained.push(session.eval_with_macros(source, &mut macros).unwrap());
        }
        for source in [
            "(new Pattern \"[\" 0)",
            "(.-source #\"a\")",
            "(set! (.-flags #\"a\") 0)",
        ] {
            let value = session
                .eval(&format!(
                    "(try {source} :accepted (catch :default e (ex-message e)))"
                ))
                .unwrap();
            let message = session
                .inspect(&value, |mut store, value| {
                    let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                    Ok(String::from_utf16(
                        &array
                            .elems(&mut store)?
                            .map(|v| v.unwrap_i32() as u16)
                            .collect::<Vec<_>>(),
                    )
                    .unwrap())
                })
                .unwrap();
            assert_eq!(
                message, "Built-in type internals are private to suss.core",
                "{source}"
            );
        }
        assert!(
            session
                .eval("(suss.core/pattern-from-source \"[\" 0)")
                .is_err()
        );
        session.eval("(def = (fn [& xs] (throw :equality))) (def hash (fn [& xs] (throw :hash))) (def pr-str (fn [& xs] (throw :print))) (def str (fn [& xs] (throw :str))) (def Pattern nil)").unwrap();
        retained.push(session.eval(r##"#"(?i)a""##).unwrap());
        session.collect().unwrap();
        for value in retained {
            session
                .inspect(&value, |mut store, value| {
                    let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    assert!(wasmtime::StructType::eq(&object.ty(&store)?, &owner_type));
                    let fields = object.fields(&mut store)?.collect::<Vec<_>>();
                    assert_eq!(fields.len(), 4);
                    let captured = descriptor.to_rooted(&mut store);
                    assert!(wasmtime::Rooted::ref_eq(
                        &store,
                        fields[0].unwrap_anyref().unwrap(),
                        &captured
                    )?);
                    let args = fields[1]
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    assert_eq!(args.len(&store)?, 2);
                    let source = args
                        .get(&mut store, 0)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    let units = source
                        .elems(&mut store)?
                        .map(|unit| unit.unwrap_i32() as u16)
                        .collect::<Vec<_>>();
                    assert_eq!(units, "(?i)a".encode_utf16().collect::<Vec<_>>());
                    let flags = args.get(&mut store, 1)?;
                    assert!(matches!(
                        runtime_number(&mut store, &flags)?,
                        Some(RuntimeNumber::Long(2))
                    ));
                    Ok(())
                })
                .unwrap();
        }
    }
}
