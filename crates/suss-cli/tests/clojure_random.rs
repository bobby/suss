//! Public random APIs: primary transformations and ordinary provider behavior.
use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};

fn effects() -> Vec<serde_json::Value> {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-random/public-effects/cases.json"
    ))
    .unwrap();
    assert_eq!(rows.len(), 16);
    rows
}
fn observation(value: &suss_reader::edn::Value) -> Observation {
    use suss_reader::edn::Value;
    match value {
        Value::Bool(v) => Observation::Bool(*v),
        Value::Long(v) => Observation::Long(*v),
        Value::Number(v) => Observation::Number(v.to_bits()),
        Value::String(v) => Observation::String(v.encode_utf16().collect()),
        Value::Keyword(v) => Observation::Keyword(
            v.namespace.as_ref().map(|s| s.encode_utf16().collect()),
            v.name.encode_utf16().collect(),
        ),
        Value::Vector(v) => Observation::Vector(v.iter().map(observation).collect()),
        other => panic!("unassessed primary observation {other:?}"),
    }
}

#[test]
fn public_random_effects_and_normal_provider_in_both_phases() {
    let rows = effects();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        let mut retained = Vec::new();
        for row in &rows {
            let expression = row["expression"]
                .as_str()
                .unwrap()
                .replace("(catch Throwable ", "(catch :default ");
            retained.push(
                session
                    .eval_with_macros(&expression, &mut macros)
                    .unwrap_or_else(|error| panic!("{row}: {error}")),
            );
        }
        session.eval("(def rand (fn [& xs] (throw :rand))) (def rand-int (fn [& xs] (throw :rand-int))) (def pr-str (fn [& xs] (throw :printer))) (def = (fn [& xs] (throw :equality)))").unwrap();
        session.collect().unwrap();
        for (value, row) in retained.iter().zip(&rows) {
            let expected =
                suss_reader::edn::read_one(row["expected"].as_str().unwrap(), &Default::default())
                    .unwrap();
            assert_eq!(
                decoder.decode_session(&mut session, value).unwrap(),
                observation(&expected),
                "{row}"
            );
        }
    }
}

#[test]
fn public_aot_numeric_transformations_use_each_controlled_provider_word() {
    use wasmtime::{
        Config, Engine, Store,
        component::{Component, Linker},
    };
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-random/object-transformations-boxed/cases.json"
    ))
    .unwrap();
    assert_eq!(rows.len(), 126);
    // Parse bounds as ordinary data at execution time. One real compiled core
    // function exercises all categories without126 duplicated source bodies.
    let source = r#"(ns random-controls)
        (defn report [operation text]
          (try (let [bound (read-string text)
                     x (if (= operation "rand") (rand bound) (rand-int bound))]
                 [:value (pr-str x) (cond (double? x) :double (integer? x) :integer :else :other)])
               (catch :default e [:error])))
        (defn ^:export result [operation text] (pr-str (report operation text)))"#;
    let wit = format!(
        "{}\nworld controls {{ import random; export result: func(operation: string, text: string) -> string; }}",
        include_str!("../../../vendor/wasi/wasi-wit-0.3.1/cli/deps/random.wit")
    );
    let bytes = suss_compile::Compiler::new().compile(source, &wit).unwrap();
    let mut config = Config::new();
    config
        .wasm_gc(true)
        .wasm_function_references(true)
        .wasm_tail_call(true)
        .wasm_exceptions(true)
        .wasm_component_model(true)
        .consume_fuel(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let engine = Engine::new(&config).unwrap();
    let compiled = Component::new(&engine, &bytes).unwrap();
    let word = Arc::new(AtomicU64::new(0));
    let draws = Arc::new(AtomicUsize::new(0));
    let mut linker = Linker::new(&engine);
    let mut provider = linker.instance("wasi:random/random@0.3.1").unwrap();
    let selected = word.clone();
    let consumed = draws.clone();
    provider
        .func_wrap("get-random-u64", move |_, ()| {
            consumed.fetch_add(1, Ordering::SeqCst);
            Ok((selected.load(Ordering::SeqCst),))
        })
        .unwrap();
    provider
        .func_wrap(
            "get-random-bytes",
            |_, (_n,): (u64,)| -> wasmtime::Result<(Vec<u8>,)> {
                panic!("scalar random sampling must request one word");
            },
        )
        .unwrap();
    let mut store = Store::new(&engine, ());
    store
        .set_fuel(suss_compile::portable::DEFAULT_OPERATION_FUEL)
        .unwrap();
    let instance = linker.instantiate(&mut store, &compiled).unwrap();
    assert_eq!(draws.load(Ordering::SeqCst), 0);
    let result = instance
        .get_typed_func::<(String, String), (String,)>(&mut store, "result")
        .unwrap();
    for (index, row) in rows.iter().enumerate() {
        word.store(
            match row["draw"].as_str().unwrap() {
                "0.0" => 0,
                "0.5" => 1 << 63,
                "0.9999999999999999" => u64::MAX,
                value => panic!("unassessed draw {value}"),
            },
            Ordering::SeqCst,
        );
        store
            .set_fuel(suss_compile::portable::DEFAULT_OPERATION_FUEL)
            .unwrap();
        let (actual,) = result
            .call(
                &mut store,
                (
                    row["operation"].as_str().unwrap().to_owned(),
                    row["bound"].as_str().unwrap().to_owned(),
                ),
            )
            .unwrap();
        let actual = suss_reader::edn::read_one(&actual, &Default::default()).unwrap();
        let expected =
            suss_reader::edn::read_one(row["expected"].as_str().unwrap(), &Default::default())
                .unwrap();
        assert_eq!(actual, expected, "{row}");
        assert_eq!(draws.load(Ordering::SeqCst), index + 1, "{row}");
    }
}

#[test]
fn actual_cli_aot_executes_public_random_calls_with_the_standard_provider() {
    use std::process::Command;
    let rows = effects();
    let root = tempfile::tempdir().unwrap();
    for rows in rows.chunks(8) {
        let mut source = String::from("(ns random-aot)\n");
        for (index, row) in rows.iter().enumerate() {
            source.push_str(&format!(
                "(def result{index} {})\n",
                row["expression"]
                    .as_str()
                    .unwrap()
                    .replace("(catch Throwable ", "(catch :default ")
            ));
        }
        source.push_str("(def report (fn [] (pr-str [");
        for index in 0..rows.len() {
            source.push_str(&format!(" result{index}"));
        }
        source.push_str("])))\n");
        source.push_str("(def samples (fn [] (pr-str [");
        for _ in 0..16 {
            source.push_str(" (rand)");
        }
        source.push_str("])))\n");
        std::fs::write(root.path().join("app.sus"), source).unwrap();
        let wit = format!(
            "{}\nworld controls {{ import random; export report: func() -> string; export samples: func() -> string; }}",
            include_str!("../../../vendor/wasi/wasi-wit-0.3.1/cli/deps/random.wit")
        );
        std::fs::write(root.path().join("app.wit"), wit).unwrap();
        let compiled = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args([
                "--no-cache",
                "compile",
                "app.sus",
                "-w",
                "app.wit",
                "--export",
                "report=random-aot/report",
                "--export",
                "samples=random-aot/samples",
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
        let actual = suss_reader::edn::read_one(
            String::from_utf8(executed.stdout).unwrap().trim_end(),
            &Default::default(),
        )
        .unwrap();
        let wanted = format!(
            "[{}]",
            rows.iter()
                .map(|r| r["expected"].as_str().unwrap())
                .collect::<Vec<_>>()
                .join(" ")
        );
        let expected = suss_reader::edn::read_one(&wanted, &Default::default()).unwrap();
        assert_eq!(actual, expected);
        let mut streams = Vec::new();
        for _ in 0..2 {
            let samples = Command::new(env!("CARGO_BIN_EXE_suss"))
                .current_dir(root.path())
                .args(["--no-cache", "run", "app.wasm", "--invoke", "samples"])
                .output()
                .unwrap();
            assert!(
                samples.status.success(),
                "{}",
                String::from_utf8_lossy(&samples.stderr)
            );
            let samples = suss_reader::edn::read_one(
                String::from_utf8(samples.stdout).unwrap().trim_end(),
                &Default::default(),
            )
            .unwrap();
            let suss_reader::edn::Value::Vector(samples) = samples else {
                panic!("{samples:?}")
            };
            assert_eq!(samples.len(), 16);
            let bits = samples
                .into_iter()
                .map(|sample| {
                    let suss_reader::edn::Value::Number(value) = sample else {
                        panic!("{sample:?}")
                    };
                    assert!((0.0..1.0).contains(&value));
                    value.to_bits()
                })
                .collect::<Vec<_>>();
            assert!(bits.iter().any(|value| *value != bits[0]));
            streams.push(bits);
        }
        assert_ne!(
            streams[0], streams[1],
            "fresh actual CLI instances must not replay a default seed"
        );
    }
}
