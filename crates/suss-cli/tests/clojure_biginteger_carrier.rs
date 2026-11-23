use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};

fn cases() -> Vec<(String, String)> {
    serde_json::from_str(include_str!(
        "../../../results/m12-promotion-fixes/carrier-cases.json"
    ))
    .unwrap()
}

#[test]
fn bigint_carrier_preserves_category_in_source_runtime_macro_and_edn() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        session
            .eval_with_macros(
                "(ns carrierprobe (:require [suss.edn :as edn]))",
                &mut macros,
            )
            .unwrap();
        for (source, expected) in cases() {
            let value = session
                .eval_with_macros(&format!("(pr-str {source})"), &mut macros)
                .unwrap_or_else(|e| panic!("{source}: {e}"));
            session.collect().unwrap();
            let actual = session
                .inspect(&value, |mut store, value| {
                    let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                    let units = array
                        .elems(&mut store)?
                        .map(|v| v.unwrap_i32() as u16)
                        .collect::<Vec<_>>();
                    Ok(String::from_utf16(&units).unwrap())
                })
                .unwrap();
            assert_eq!(actual, expected, "{source}");
        }
        for source in [
            "#suss/biginteger 1",
            "#suss/biginteger []",
            "#suss/biginteger \"\"",
            "#suss/biginteger \"0x10\"",
            "#suss/biginteger \"1N\"",
        ] {
            for reader in ["read-string", "edn/read-string"] {
                let value = session.eval_with_macros(&format!("(try ({reader} {source:?}) false (catch :default e (= (.-message e) \"BigInteger literal expects a decimal string.\")))"), &mut macros).unwrap();
                session
                    .inspect(&value, |mut store, value| {
                        assert_eq!(
                            suss_cli::portable_session::sentinel_tag(&mut store, &value)?,
                            Some(4),
                            "{reader}: {source}"
                        );
                        Ok(())
                    })
                    .unwrap();
            }
        }
    }
}

#[test]
fn aot_executes_typed_biginteger_carrier_without_host_reader() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let mut source = String::from(
        "(ns carrierprobe (:require [suss.edn :as edn]))\n(def check (fn [unused] (if (and\n",
    );
    for (expression, expected) in cases() {
        source.push_str(&format!("(= (pr-str {expression}) {expected:?})\n"));
    }
    source.push_str(") 1 0)))\n");
    std::fs::write(dir.path().join("probe.sus"), source).unwrap();
    std::fs::write(
        dir.path().join("probe.wit"),
        "package test:carrierprobe; world w { export check: func(unused: u32) -> u32; }",
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(dir.path())
        .args([
            "compile",
            "probe.sus",
            "-w",
            "probe.wit",
            "--export",
            "check=carrierprobe/check",
            "-o",
            "probe.wasm",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(dir.path())
        .args(["run", "probe.wasm", "--invoke", "check", "--", "0"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), "1");
}
