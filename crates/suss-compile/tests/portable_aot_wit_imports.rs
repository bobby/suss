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

#[test]
fn sync_import_arguments_validate_before_allocation_and_keep_source_effect_order() {
    let component = compile(
        "(ns app) (def effects (atom []))
         (def big (loop [n 16 text \"x\"] (if (zero? n) text (recur (dec n) (str text text)))))
         (defn ^:export run []
           (try (host/accept [(do (swap! effects conj 1) big) (do (swap! effects conj 2) 256)])
                (catch :default error @effects)))",
        "import accept: func(value: tuple<string, u8>) -> u32;
         export run: func() -> list<u8>;",
        "accept",
    );
    let mut linker = Linker::new(engine());
    linker
        .root()
        .func_wrap(
            "accept",
            |mut context: StoreContextMut<'_, State>, (_row,): ((String, u8),)| {
                context.data_mut().calls += 1;
                Ok((42_u32,))
            },
        )
        .unwrap();
    let mut store = store();
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let run = instance
        .get_typed_func::<(), (Vec<u8>,)>(&mut store, "run")
        .unwrap();
    for i in 1..=20 {
        store.set_fuel(100_000_000).unwrap();
        assert_eq!(run.call(&mut store, ()).unwrap().0, [1_u8, 2].repeat(i));
        store.gc(None).unwrap();
    }
    assert_eq!(store.data().calls, 0);
    assert_eq!(store.data().canonical_peak, 65536);
}

#[test]
fn sync_import_indirect_parameter_records_and_result_buffers_follow_guest_import_abi() {
    use wasmtime::component::Val;
    let parameters = (0..19)
        .map(|i| format!("p{i}: s64"))
        .collect::<Vec<_>>()
        .join(", ");
    let component = compile(
        "(ns app) (defn ^:export run [xs] (apply host/rotate xs))",
        &format!(
            "import rotate: func({parameters}) -> list<s64>; export run: func(xs: list<s64>) -> list<s64>;"
        ),
        "rotate",
    );
    let mut linker = Linker::new(engine());
    linker
        .root()
        .func_new(
            "rotate",
            |mut context: StoreContextMut<'_, State>, _ty, parameters, results| {
                context.data_mut().calls += 1;
                assert_eq!(parameters.len(), 19);
                let mut values = parameters.to_vec();
                values.rotate_left(1);
                results[0] = Val::List(values);
                Ok(())
            },
        )
        .unwrap();
    let mut store = store();
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let run = instance
        .get_typed_func::<(Vec<i64>,), (Vec<i64>,)>(&mut store, "run")
        .unwrap();
    let input = (0..19).map(|i| 9007199254740993 + i).collect::<Vec<i64>>();
    let mut expected = input.clone();
    expected.rotate_left(1);
    for _ in 0..1000 {
        store.set_fuel(100_000_000).unwrap();
        assert_eq!(run.call(&mut store, (input.clone(),)).unwrap().0, expected);
        store.gc(None).unwrap();
    }
    assert_eq!(store.data().calls, 1000);
    assert_eq!(store.data().canonical_peak, 65536);
}

#[test]
fn sync_import_runtime_mappings_do_not_grant_macro_phase_authority() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.sus");
    let wit = directory.path().join("api.wit");
    std::fs::write(
        &path,
        "(ns app) (defmacro call-host [] (host/increment 0)) (defn run [] (call-host))",
    )
    .unwrap();
    std::fs::write(&wit, "package test:phase-authority; world api { import increment: func(value: s64) -> s64; export run: func() -> s64; }").unwrap();
    let options = AotOptions {
        exports: vec![("run".into(), Symbol::namespaced("app", "run"))],
        imports: vec![("increment".into(), Symbol::namespaced("host", "increment"))],
        ..Default::default()
    };
    let error =
        compile_file_with_options(&path, &wit, None, &[directory.path().to_owned()], &options)
            .unwrap_err();
    assert!(
        error.contains("host")
            && (error.contains("Macro") || error.contains("macro") || error.contains("namespace")),
        "{error}"
    );
}

#[test]
fn sync_import_nominal_records_flags_variants_and_maps_keep_public_type_identities() {
    use wasmtime::component::Val;
    let component = compile(
        "(ns app) (defn ^:export run [x]
           (let [y (host/touch x)] (update (assoc y :state :active) :permissions conj :write)))",
        "enum state { idle, active } flags permissions { read, write }
         variant event { idle, payload(tuple<u64, char>) }
         record row { state: state, permissions: permissions, event: event, values: map<string, list<u8>> }
         import touch: func(value: row) -> row; export run: func(value: row) -> row;", "touch");
    let mut linker = Linker::new(engine());
    linker
        .root()
        .func_new(
            "touch",
            |mut context: StoreContextMut<'_, State>, _ty, params, results| {
                context.data_mut().calls += 1;
                assert_eq!(params.len(), 1);
                results[0] = params[0].clone();
                Ok(())
            },
        )
        .unwrap();
    let mut store = store();
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let run = instance.get_func(&mut store, "run").unwrap();
    let input = Val::Record(vec![
        ("state".into(), Val::Enum("idle".into())),
        ("permissions".into(), Val::Flags(vec!["read".into()])),
        (
            "event".into(),
            Val::Variant(
                "payload".into(),
                Some(Box::new(Val::Tuple(vec![
                    Val::U64(u64::MAX),
                    Val::Char('🦀'),
                ]))),
            ),
        ),
        (
            "values".into(),
            Val::Map(vec![(
                Val::String("λ\0🦀".into()),
                Val::List(vec![Val::U8(255), Val::U8(0)]),
            )]),
        ),
    ]);
    let mut expected = input.clone();
    let Val::Record(fields) = &mut expected else {
        unreachable!()
    };
    fields[0].1 = Val::Enum("active".into());
    fields[1].1 = Val::Flags(vec!["read".into(), "write".into()]);
    let mut output = [Val::Bool(false)];
    for _ in 0..20 {
        store.set_fuel(100_000_000).unwrap();
        run.call(&mut store, &[input.clone()], &mut output).unwrap();
        assert_eq!(output[0], expected);
        store.gc(None).unwrap();
    }
    assert_eq!(store.data().calls, 20);
    assert_eq!(store.data().canonical_peak, 65536);
}

#[test]
fn configured_project_import_mappings_select_the_declared_provider_cells() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("src")).unwrap();
    std::fs::write(
        directory.path().join("src/app.sus"),
        "(ns app) (defn run [x] (host/step x))",
    )
    .unwrap();
    std::fs::write(directory.path().join("api.wit"), "package test:project-imports; world api { import left: func(x: s64) -> s64; import right: func(x: s64) -> s64; export run: func(x: s64) -> s64; }").unwrap();
    let mut compiler = suss_compile::Compiler::new();
    for (mapping, expected) in [
        ("{\"left\" host/step \"right\" host/spare}", 42_i64),
        ("{\"left\" host/spare \"right\" host/step}", 141),
    ] {
        let configuration = format!(
            "{{:src-paths [\"src\"] :worlds {{:app {{:namespace app :wit \"api.wit\" :wit-world \"api\" :imports {mapping} :exports {{\"run\" app/run}} :output \"app.wasm\"}}}}}}"
        );
        let config =
            suss_compile::SussConfig::parse(&configuration, directory.path().to_owned()).unwrap();
        let bytes = compiler
            .compile_project(&config, Some(":app"))
            .unwrap()
            .remove(":app")
            .unwrap();
        let component = Component::new(engine(), bytes).unwrap();
        let mut linker = Linker::new(engine());
        linker
            .root()
            .func_wrap("left", |_: StoreContextMut<'_, State>, (x,): (i64,)| {
                Ok((x + 1,))
            })
            .unwrap();
        linker
            .root()
            .func_wrap("right", |_: StoreContextMut<'_, State>, (x,): (i64,)| {
                Ok((x + 100,))
            })
            .unwrap();
        let mut store = store();
        let instance = linker.instantiate(&mut store, &component).unwrap();
        let run = instance
            .get_typed_func::<(i64,), (i64,)>(&mut store, "run")
            .unwrap();
        assert_eq!(run.call(&mut store, (41,)).unwrap().0, expected);
        assert!(
            !directory.path().join("app.wasm").exists(),
            "library compilation returns bytes without publishing"
        );
    }
}
