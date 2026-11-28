//! Exact decimal form parsing uses the Clojure JVM behavior, in both phases.
use suss_cli::portable_session::Session;

#[test]
fn parse_long_preserves_exact_longs_and_primary_decimal_text_behavior() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let result = session
            .eval(
                r#"(pr-str (mapv parse-long
                    ["0" "9007199254740993" "9223372036854775807"
                     "-9223372036854775808" "9223372036854775808"
                     "bad" "+17" "١٧" "１７" " 17" "" "-0"]))"#,
            )
            .unwrap();
        session.collect().unwrap();
        let printed = session
            .inspect(&result, |mut store, value| {
                let string = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                let units = string
                    .elems(&mut store)?
                    .map(|v| v.unwrap_i32() as u16)
                    .collect::<Vec<_>>();
                Ok(String::from_utf16(&units).unwrap())
            })
            .unwrap();
        assert_eq!(
            printed,
            "[0 9007199254740993 9223372036854775807 -9223372036854775808 nil nil 17 17 17 nil nil 0]"
        );
    }
}

fn printed(session: &mut Session, source: &str) -> String {
    let value = session.eval(source).unwrap();
    session.collect().unwrap();
    session
        .inspect(&value, |mut store, value| {
            let string = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let units = string
                .elems(&mut store)?
                .map(|v| v.unwrap_i32() as u16)
                .collect::<Vec<_>>();
            Ok(String::from_utf16(&units).unwrap())
        })
        .unwrap()
}

#[test]
fn parse_long_matches_the_recorded_jvm_corpus_and_rejects_non_strings_in_both_phases() {
    let cases: Vec<(String, String)> = serde_json::from_str(include_str!(
        "../../../results/m5-controller-parsing/primary/cases.json"
    ))
    .unwrap();
    assert_eq!(cases.len(), 783);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        assert_eq!(
            printed(
                &mut session,
                r#"(pr-str [(let [f parse-long] (f "17")) (clojure.core/parse-long "17") (suss.core/parse-long "17") (let [parse-long (fn [_] :shadowed)] (parse-long "17"))])"#
            ),
            "[17 17 17 :shadowed]"
        );
        for batch in cases.chunks(64) {
            let source = format!(
                "(pr-str [{}])",
                batch
                    .iter()
                    .map(|(source, _)| source.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            let expected = format!(
                "[{}]",
                batch
                    .iter()
                    .map(|(_, expected)| expected.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            assert_eq!(printed(&mut session, &source), expected, "{source}");
        }
        assert_eq!(
            printed(
                &mut session,
                "(pr-str (mapv (fn [x] (try (parse-long x) false (catch :default e true))) [nil true false 17 17.0 17N 17/2 17M :x [] {} #{} (fn [] 17)]))"
            ),
            "[true true true true true true true true true true true true true]"
        );
        assert_eq!(
            printed(
                &mut session,
                "(pr-str [(parse-long (suss.bootstrap/string-from-unit 55296)) (parse-long (suss.bootstrap/string-from-unit 56320))])"
            ),
            "[nil nil]"
        );
    }
}

#[derive(Default)]
struct TransferPeak {
    canonical: usize,
    auxiliary: usize,
}
impl wasmtime::ResourceLimiter for TransferPeak {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        // The unoptimized runtime has a separate numeric-helper memory and
        // managed heap. Keep their bounds separate from canonical transfers.
        if current >= 17 * 65_536 || desired == 17 * 65_536 || desired == 8 * 1024 * 1024 {
            self.auxiliary = self.auxiliary.max(desired);
            Ok(desired <= 64 * 1024 * 1024)
        } else {
            self.canonical = self.canonical.max(desired);
            Ok(desired <= 65_536)
        }
    }
    fn table_growing(
        &mut self,
        _current: usize,
        _desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(true)
    }
}

#[test]
fn actual_component_parses_exact_form_ids_and_preserves_change_before_parse_error() {
    use suss_cli::portable_aot::{AotOptions, compile_file_with_options};
    use suss_reader::Symbol;
    use wasmtime::{
        Config, Engine, Store,
        component::{Component, Linker},
    };
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("app.sus");
    let wit = directory.path().join("api.wit");
    std::fs::write(
        &source,
        r#"(ns form-parser)
      (def changes (atom 0))
      (defn parse-value [text]
        (let [value (parse-long text)]
          (if (nil? value) [:none] [:some value])))
      (defn form-long [text]
        (if (or (nil? text) (= text "")) nil
          (let [value (parse-long text)]
            (if (nil? value)
              (throw (ex-info "Invalid form ID" {:kind :parse}))
              value))))
      (defn submit [field]
        (swap! changes inc)
        (try
          (let [text (if (= (nth field 0) :none) nil (nth field 1))
                value (form-long text)]
            [:ok (if (nil? value) [:none] [:some value])])
          (catch :default error
            (if (= (:kind (ex-data error)) :parse)
              [:err (ex-message error)]
              (throw error)))))
      (defn change-count [] @changes)"#,
    )
    .unwrap();
    std::fs::write(
        &wit,
        r#"package test:form-parser;
      world api {
        export parse-value: func(text: string) -> option<s64>;
        export submit: func(text: option<string>) -> result<option<s64>, string>;
        export change-count: func() -> u64;
      }"#,
    )
    .unwrap();
    let bytes = compile_file_with_options(
        &source,
        &wit,
        Some("api"),
        &[],
        &AotOptions {
            exports: [
                ("parse-value", "form-parser/parse-value"),
                ("submit", "form-parser/submit"),
                ("change-count", "form-parser/change-count"),
            ]
            .into_iter()
            .map(|(path, source)| (path.into(), Symbol::parse(source)))
            .collect(),
            ..Default::default()
        },
    )
    .unwrap();
    let mut config = Config::new();
    config
        .wasm_gc(true)
        .gc_heap_initial_size(8 * 1024 * 1024)
        .wasm_function_references(true)
        .wasm_tail_call(true)
        .wasm_exceptions(true)
        .wasm_component_model(true)
        .consume_fuel(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let engine = Engine::new(&config).unwrap();
    let component = Component::new(&engine, bytes).unwrap();
    assert_eq!(component.component_type().imports(&engine).count(), 0);
    let mut store = Store::new(&engine, TransferPeak::default());
    store.limiter(|state| state);
    store.set_fuel(100_000_000).unwrap();
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let parse = instance
        .get_typed_func::<(String,), (Option<i64>,)>(&mut store, "parse-value")
        .unwrap();
    let submit = instance
        .get_typed_func::<(Option<String>,), (Result<Option<i64>, String>,)>(&mut store, "submit")
        .unwrap();
    let count = instance
        .get_typed_func::<(), (u64,)>(&mut store, "change-count")
        .unwrap();
    let inputs: Vec<String> = serde_json::from_str(include_str!(
        "../../../results/m5-controller-parsing/primary/inputs.json"
    ))
    .unwrap();
    let cases: Vec<(String, String)> = serde_json::from_str(include_str!(
        "../../../results/m5-controller-parsing/primary/cases.json"
    ))
    .unwrap();
    assert_eq!(inputs.len(), cases.len());
    for (text, (_, expected)) in inputs.into_iter().zip(cases) {
        store.set_fuel(100_000_000).unwrap();
        let expected = if expected == "nil" {
            None
        } else {
            Some(expected.parse::<i64>().unwrap())
        };
        assert_eq!(
            parse.call(&mut store, (text.clone(),)).unwrap().0,
            expected,
            "{text:?}"
        );
    }
    for index in 0..10_000 {
        let (text, expected) = match index % 8 {
            0 => (None, Ok(None)),
            1 => (Some(String::new()), Ok(None)),
            2 => (Some("0".into()), Ok(Some(0))),
            3 => (
                Some("9007199254740993".into()),
                Ok(Some(9_007_199_254_740_993)),
            ),
            4 => (Some("-9223372036854775808".into()), Ok(Some(i64::MIN))),
            5 => (Some("１７".into()), Ok(Some(17))),
            6 => (
                Some("9223372036854775808".into()),
                Err("Invalid form ID".into()),
            ),
            _ => (Some("bad".into()), Err("Invalid form ID".into())),
        };
        store.set_fuel(100_000_000).unwrap();
        assert_eq!(submit.call(&mut store, (text,)).unwrap().0, expected);
        assert_eq!(count.call(&mut store, ()).unwrap().0, index + 1);
        if index % 127 == 0 {
            store.gc(None).unwrap();
        }
    }
    store.gc(None).unwrap();
    assert_eq!(store.data().canonical, 65_536);
    assert!(store.data().auxiliary >= 17 * 65_536);
}

#[test]
fn component_parses_before_storage_and_preserves_changes_across_host_errors() {
    use suss_cli::portable_aot::{AotOptions, compile_file_with_options};
    use suss_reader::Symbol;
    use wasmtime::{
        Config, Engine, Store, StoreContextMut,
        component::{Component, Linker},
    };
    #[derive(Default)]
    struct Storage {
        writes: Vec<Option<i64>>,
    }
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("app.sus");
    let wit = directory.path().join("api.wit");
    std::fs::write(
        &source,
        r#"(ns storage-parser)
      (def changes (atom 0))
      (defn submit [field]
        (swap! changes inc)
        (try
          (let [text (if (= (nth field 0) :none) nil (nth field 1))
                id (if (or (nil? text) (= text "")) nil
                     (let [value (parse-long text)]
                       (if (nil? value)
                         (throw (ex-info "Invalid form ID" {:kind :parse})) value)))]
            (storage/write-id (if (nil? id) [:none] [:some id])))
          (catch :default error
            (if (= (:kind (ex-data error)) :parse)
              [:err (ex-message error)] (throw error)))))
      (defn change-count [] @changes)"#,
    )
    .unwrap();
    std::fs::write(
        &wit,
        r#"package test:storage-parser;
      world api {
        import write-id: func(id: option<s64>) -> result<u64, string>;
        export submit: func(text: option<string>) -> result<u64, string>;
        export change-count: func() -> u64;
      }"#,
    )
    .unwrap();
    let bytes = compile_file_with_options(
        &source,
        &wit,
        Some("api"),
        &[],
        &AotOptions {
            exports: vec![
                ("submit".into(), Symbol::parse("storage-parser/submit")),
                (
                    "change-count".into(),
                    Symbol::parse("storage-parser/change-count"),
                ),
            ],
            imports: vec![("write-id".into(), Symbol::parse("storage/write-id"))],
            ..Default::default()
        },
    )
    .unwrap();
    let mut config = Config::new();
    config
        .wasm_gc(true)
        .gc_heap_initial_size(8 * 1024 * 1024)
        .wasm_function_references(true)
        .wasm_tail_call(true)
        .wasm_exceptions(true)
        .wasm_component_model(true)
        .consume_fuel(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let engine = Engine::new(&config).unwrap();
    let component = Component::new(&engine, bytes).unwrap();
    assert_eq!(component.component_type().imports(&engine).count(), 1);
    let mut linker = Linker::new(&engine);
    linker
        .root()
        .func_wrap(
            "write-id",
            |mut context: StoreContextMut<'_, Storage>, (id,): (Option<i64>,)| {
                context.data_mut().writes.push(id);
                let result = if id == Some(-17) {
                    Err("storage unavailable".to_owned())
                } else {
                    Ok(context.data().writes.len() as u64)
                };
                Ok((result,))
            },
        )
        .unwrap();
    let mut store = Store::new(&engine, Storage::default());
    store.set_fuel(100_000_000).unwrap();
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let submit = instance
        .get_typed_func::<(Option<String>,), (Result<u64, String>,)>(&mut store, "submit")
        .unwrap();
    let count = instance
        .get_typed_func::<(), (u64,)>(&mut store, "change-count")
        .unwrap();
    let cases = [
        (Some("bad"), Err("Invalid form ID"), vec![]),
        (
            Some("9007199254740993"),
            Ok(1),
            vec![Some(9_007_199_254_740_993)],
        ),
        (
            Some("9223372036854775808"),
            Err("Invalid form ID"),
            vec![Some(9_007_199_254_740_993)],
        ),
        (
            Some("-17"),
            Err("storage unavailable"),
            vec![Some(9_007_199_254_740_993), Some(-17)],
        ),
        (
            None,
            Ok(3),
            vec![Some(9_007_199_254_740_993), Some(-17), None],
        ),
        (
            Some(""),
            Ok(4),
            vec![Some(9_007_199_254_740_993), Some(-17), None, None],
        ),
        (
            Some("１７"),
            Ok(5),
            vec![Some(9_007_199_254_740_993), Some(-17), None, None, Some(17)],
        ),
        (
            Some("-9223372036854775808"),
            Ok(6),
            vec![
                Some(9_007_199_254_740_993),
                Some(-17),
                None,
                None,
                Some(17),
                Some(i64::MIN),
            ],
        ),
    ];
    for (index, (text, expected, writes)) in cases.into_iter().enumerate() {
        store.set_fuel(100_000_000).unwrap();
        let result = submit
            .call(&mut store, (text.map(str::to_owned),))
            .unwrap()
            .0;
        assert_eq!(
            result.as_ref().map(|n| *n).map_err(String::as_str),
            expected
        );
        assert_eq!(store.data().writes, writes);
        assert_eq!(count.call(&mut store, ()).unwrap().0, index as u64 + 1);
        store.gc(None).unwrap();
    }
}

#[test]
fn parse_long_character_platform_uses_the_actual_string_value() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        assert_eq!(
            printed(
                &mut session,
                r#"(pr-str [(string? \a) (= \a "a") (parse-long \a) (parse-long \7)])"#
            ),
            "[true true nil 7]"
        );
    }
}
