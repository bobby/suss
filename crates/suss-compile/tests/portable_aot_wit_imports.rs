//! Execute explicitly mapped host calls through the compiled Runtime pipeline.
#![cfg(not(target_family = "wasm"))]
use std::sync::OnceLock;
use suss_compile::portable_aot::{AotOptions, compile_file_with_options};
use suss_reader::Symbol;
use wasmtime::{
    Config, Engine, Store, StoreContextMut,
    component::{Component, Linker},
};

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut config = Config::new();
        config
            .wasm_gc(true)
            .gc_heap_initial_size(8 * 1024 * 1024)
            .wasm_function_references(true)
            .wasm_tail_call(true)
            .wasm_exceptions(true)
            .wasm_component_model(true)
            .wasm_component_model_map(true)
            .wasm_component_model_fixed_length_lists(true)
            .wasm_component_model_async(true)
            .consume_fuel(true)
            .cranelift_opt_level(wasmtime::OptLevel::None);
        Engine::new(&config).unwrap()
    })
}

fn compile(source: &str, declarations: &str, mapping: &str) -> Component {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("app.sus");
    let wit = directory.path().join("api.wit");
    std::fs::write(&source_path, source).unwrap();
    std::fs::write(
        &wit,
        format!("package test:host-values; world api {{ {declarations} }}"),
    )
    .unwrap();
    let options = AotOptions {
        exports: vec![("run".into(), Symbol::namespaced("app", "run"))],
        imports: vec![(mapping.into(), Symbol::namespaced("host", mapping))],
        ..Default::default()
    };
    let bytes = compile_file_with_options(
        &source_path,
        &wit,
        Some("api"),
        &[directory.path().to_owned()],
        &options,
    )
    .unwrap();
    Component::new(engine(), bytes).unwrap()
}

#[derive(Default)]
struct State {
    calls: usize,
    canonical_peak: usize,
}
impl wasmtime::ResourceLimiter for State {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        _: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if current >= 17 * 65536 || desired == 17 * 65536 || desired == 8 * 1024 * 1024 {
            Ok(desired <= 64 * 1024 * 1024)
        } else {
            self.canonical_peak = self.canonical_peak.max(desired);
            Ok(desired <= 65536)
        }
    }
    fn table_growing(&mut self, _: usize, _: usize, _: Option<usize>) -> wasmtime::Result<bool> {
        Ok(true)
    }
}
fn store() -> Store<State> {
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    store.limiter(|state| state);
    store
}

#[test]
fn imported_exact_scalar_functions_initialize_once_and_remain_callable() {
    let component = compile(
        "(ns app) (def n (host/increment 9007199254740993))
         (defn ^:export run [x] (+ (host/increment x) n))",
        "import increment: func(value: s64) -> s64; export run: func(value: s64) -> s64;",
        "increment",
    );
    let mut linker = Linker::new(engine());
    linker
        .root()
        .func_wrap(
            "increment",
            |mut context: StoreContextMut<'_, State>, (value,): (i64,)| {
                context.data_mut().calls += 1;
                Ok((value.checked_add(1).unwrap(),))
            },
        )
        .unwrap();
    let mut store = store();
    assert_eq!(store.data().calls, 0);
    let instance = linker.instantiate(&mut store, &component).unwrap();
    assert_eq!(store.data().calls, 1);
    let run = instance
        .get_typed_func::<(i64,), (i64,)>(&mut store, "run")
        .unwrap();
    for value in [0, 7, 9007199254740993] {
        assert_eq!(
            run.call(&mut store, (value,)).unwrap().0,
            value + 1 + 9007199254740994
        );
        store.gc(None).unwrap();
    }
    assert_eq!(store.data().calls, 4);
    assert_eq!(store.data().canonical_peak, 65536);
}

#[test]
fn imported_nested_values_and_typed_errors_keep_owned_canonical_buffers_bounded() {
    let component = compile(
        "(ns app) (defn ^:export run [x]
           (let [y (host/mirror x)] (if (= :ok (first y))
             (assoc y 1 (update (second y) 1 conj 18446744073709551615N)) y)))",
        "type row = tuple<string, list<u64>, option<bool>>;
         import mirror: func(value: row) -> result<row, string>;
         export run: func(value: row) -> result<row, string>;",
        "mirror",
    );
    type Row = (String, Vec<u64>, Option<bool>);
    let mut linker = Linker::new(engine());
    linker
        .root()
        .func_wrap(
            "mirror",
            |mut context: StoreContextMut<'_, State>, (row,): (Row,)| {
                context.data_mut().calls += 1;
                Ok((if row.0 == "error" {
                    Err("host λ🦀".to_owned())
                } else {
                    Ok(row)
                },))
            },
        )
        .unwrap();
    let mut store = store();
    let instance = linker.instantiate(&mut store, &component).unwrap();
    assert_eq!(store.data().calls, 0);
    let run = instance
        .get_typed_func::<(Row,), (Result<Row, String>,)>(&mut store, "run")
        .unwrap();
    for i in 0..1000 {
        if i % 100 == 0 {
            store.set_fuel(100_000_000).unwrap();
            store.gc(None).unwrap();
        }
        let flag = [None, Some(false), Some(true)][i % 3];
        let row = (
            "λ\0🦀".to_owned(),
            vec![0, 9007199254740993, u64::MAX],
            flag,
        );
        let mut expected = row.clone();
        expected.1.push(u64::MAX);
        assert_eq!(run.call(&mut store, (row,)).unwrap().0, Ok(expected));
    }
    assert_eq!(
        run.call(&mut store, (("error".into(), vec![], None),))
            .unwrap()
            .0,
        Err("host λ🦀".into())
    );
    assert_eq!(store.data().calls, 1001);
    assert_eq!(store.data().canonical_peak, 65536);
}

#[test]
fn unsupported_or_missing_import_authority_fails_before_effectful_macros() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.sus");
    let wit = directory.path().join("api.wit");
    std::fs::write(
        &path,
        "(defmacro effect [] (throw \"macro-ran\")) (defn ^:export run [] (effect))",
    )
    .unwrap();
    for (signature, imports, expected) in [
        (
            "import f: func(x: future<string>);",
            vec![("f".into(), Symbol::namespaced("host", "f"))],
            "future",
        ),
        (
            "import f: func();",
            vec![("unknown".into(), Symbol::namespaced("host", "f"))],
            "mapping",
        ),
        (
            "import f: func();",
            vec![("f".into(), Symbol::parse("f"))],
            "namespace-qualified",
        ),
        (
            "import f: func();",
            vec![("f".into(), Symbol::namespaced("suss.core", "f"))],
            "outside core",
        ),
    ] {
        std::fs::write(
            &wit,
            format!("package test:import-guards; world api {{ {signature} export run: func(); }}"),
        )
        .unwrap();
        let error = compile_file_with_options(
            &path,
            &wit,
            None,
            &[],
            &AotOptions {
                imports,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("macro-ran"), "{error}");
    }
}
