//! Executing regressions from the independent #524 acceptance review.
#![cfg(not(target_family = "wasm"))]
use std::sync::OnceLock;
use suss_compile::Compiler;
use wasmtime::{
    Config, Engine, Store,
    component::{Component, Linker, Val},
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
            .wasm_component_model_implements(true)
            .wasm_component_model_map(true)
            .wasm_component_model_fixed_length_lists(true)
            .consume_fuel(true)
            .cranelift_opt_level(wasmtime::OptLevel::None);
        Engine::new(&config).unwrap()
    })
}
fn component(source: &str, wit: &str) -> Component {
    let bytes = Compiler::new().compile(source, wit).unwrap();
    Component::new(engine(), bytes).unwrap()
}
fn store() -> Store<()> {
    let mut store = Store::new(engine(), ());
    store.set_fuel(100_000_000).unwrap();
    store
}

#[test]
fn schema_only_provider_preserves_nested_records_and_later_type_aliases() {
    let component = component(
        "(defn ^:export run [x y] (assoc x :x y))",
        "package test:review-aliases@1.2.3; interface data { record inner { v: u8 } record outer { x: inner } } world api { use data.{outer, inner as late}; export run: func(x: outer, y: late) -> outer; }",
    );
    let imports = component
        .component_type()
        .imports(engine())
        .map(|(name, _)| name.to_owned())
        .collect::<Vec<_>>();
    assert_eq!(imports, ["test:review-aliases/data@1.2.3"]);
    let mut store = store();
    let instance = Linker::new(engine())
        .instantiate(&mut store, &component)
        .unwrap();
    let run = instance.get_func(&mut store, "run").unwrap();
    let inner = |v| Val::Record(vec![("v".into(), Val::U8(v))]);
    let outer = |v| Val::Record(vec![("x".into(), inner(v))]);
    for value in [0, 1, 255] {
        let mut output = [Val::Bool(false)];
        run.call(&mut store, &[outer(17), inner(value)], &mut output)
            .unwrap();
        assert_eq!(output, [outer(value)]);
        store.gc(None).unwrap();
    }
}

#[test]
fn fixed_numeric_lists_accept_matching_packed_arrays_and_unsigned_byte_bits() {
    let component = component(
        "(defn ^:export run [] (byte-array [-1 1 2]))",
        "package test:review-fixed; world api { type bytes = list<u8, 3>; export run: func() -> bytes; }",
    );
    let mut store = store();
    let instance = Linker::new(engine())
        .instantiate(&mut store, &component)
        .unwrap();
    let run = instance.get_func(&mut store, "run").unwrap();
    let mut output = [Val::Bool(false)];
    run.call(&mut store, &[], &mut output).unwrap();
    assert_eq!(
        output,
        [Val::FixedLengthList(vec![
            Val::U8(255),
            Val::U8(1),
            Val::U8(2)
        ])]
    );
}

#[test]
fn fixed_lengths_packed_kinds_and_tuple_array_rejection_remain_checked() {
    let component = component(
        "(defn ^:export bad-length [] (byte-array [1 2])) (defn ^:export bad-kind [] (int-array [1 2 3])) (defn ^:export bad-tuple [] (byte-array [1 2 3]))",
        "package test:review-fixed-errors; world api { type bytes = list<u8, 3>; export bad-length: func() -> bytes; export bad-kind: func() -> bytes; export bad-tuple: func() -> tuple<u8,u8,u8>; }",
    );
    for name in ["bad-length", "bad-kind", "bad-tuple"] {
        let mut store = store();
        let instance = Linker::new(engine())
            .instantiate(&mut store, &component)
            .unwrap();
        let run = instance.get_func(&mut store, name).unwrap();
        let error = run
            .call(&mut store, &[], &mut [Val::Bool(false)])
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<wasmtime::Trap>(),
            Some(&wasmtime::Trap::UncaughtException),
            "{name}: {error:#}"
        );
        assert!(store.take_pending_exception().is_some(), "{name}");
    }
}

#[test]
fn nested_fixed_packed_kinds_preserve_exact_integer_and_float_bits() {
    let component = component(
        "(defn ^:export run [] [(byte-array [-1 -128 0]) (short-array [-32768 32767]) (int-array [-2147483648 2147483647]) (long-array [-9223372036854775808 9007199254740993 9223372036854775807]) (float-array [-0.0 1.5]) (double-array [-0.0 1.5])])",
        "package test:review-fixed-kinds; world api { type row = tuple<list<u8,3>,list<s16,2>,list<s32,2>,list<s64,3>,list<f32,2>,list<f64,2>>; export run: func() -> row; }",
    );
    let mut store = store();
    let instance = Linker::new(engine())
        .instantiate(&mut store, &component)
        .unwrap();
    let run = instance.get_func(&mut store, "run").unwrap();
    for _ in 0..3 {
        let mut output = [Val::Bool(false)];
        run.call(&mut store, &[], &mut output).unwrap();
        let Val::Tuple(fields) = &output[0] else {
            panic!("{output:?}")
        };
        assert_eq!(
            fields[0],
            Val::FixedLengthList(vec![Val::U8(255), Val::U8(128), Val::U8(0)])
        );
        assert_eq!(
            fields[1],
            Val::FixedLengthList(vec![Val::S16(i16::MIN), Val::S16(i16::MAX)])
        );
        assert_eq!(
            fields[2],
            Val::FixedLengthList(vec![Val::S32(i32::MIN), Val::S32(i32::MAX)])
        );
        assert_eq!(
            fields[3],
            Val::FixedLengthList(vec![
                Val::S64(i64::MIN),
                Val::S64(9007199254740993),
                Val::S64(i64::MAX)
            ])
        );
        let Val::FixedLengthList(floats) = &fields[4] else {
            panic!("{fields:?}")
        };
        let Val::Float32(zero) = floats[0] else {
            panic!("{floats:?}")
        };
        assert_eq!(zero.to_bits(), (-0.0_f32).to_bits());
        assert_eq!(floats[1], Val::Float32(1.5));
        let Val::FixedLengthList(doubles) = &fields[5] else {
            panic!("{fields:?}")
        };
        let Val::Float64(zero) = doubles[0] else {
            panic!("{doubles:?}")
        };
        assert_eq!(zero.to_bits(), (-0.0_f64).to_bits());
        assert_eq!(doubles[1], Val::Float64(1.5));
        store.gc(None).unwrap();
    }
}

#[test]
fn schema_provider_keeps_exported_interface_instance_indices() {
    use suss_compile::portable_aot::{AotOptions, compile_file_with_options};
    use suss_reader::Symbol;
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("app.sus");
    let wit = directory.path().join("api.wit");
    std::fs::write(&source, "(ns app) (defn echo [row] (update row :n inc))").unwrap();
    std::fs::write(&wit, "package test:review-public@1.2.3; interface data { record row { n:s64 } } interface api { use data.{row}; echo:func(value:row)->row; } world api-world { import data; export public:api; }").unwrap();
    let bytes = compile_file_with_options(
        &source,
        &wit,
        Some("api-world"),
        &[],
        &AotOptions {
            exports: vec![("public#echo".into(), Symbol::parse("app/echo"))],
            ..Default::default()
        },
    )
    .unwrap();
    let component = Component::new(engine(), bytes).unwrap();
    let imports = component
        .component_type()
        .imports(engine())
        .map(|(name, _)| name.to_owned())
        .collect::<Vec<_>>();
    assert_eq!(imports, ["test:review-public/data@1.2.3"]);
    let mut store = store();
    let instance = Linker::new(engine())
        .instantiate(&mut store, &component)
        .unwrap();
    let public = instance
        .get_export_index(&mut store, None, "public")
        .unwrap();
    let echo = instance
        .get_export_index(&mut store, Some(&public), "echo")
        .unwrap();
    let echo = instance.get_func(&mut store, echo).unwrap();
    let mut output = [Val::Bool(false)];
    echo.call(
        &mut store,
        &[Val::Record(vec![("n".into(), Val::S64(9007199254740993))])],
        &mut output,
    )
    .unwrap();
    assert_eq!(
        output,
        [Val::Record(vec![("n".into(), Val::S64(9007199254740994))])]
    );
}

#[test]
fn schema_provider_and_random_capability_keep_distinct_instance_indices() {
    use suss_compile::portable_aot::{AotOptions, compile_file_with_options};
    use suss_reader::Symbol;
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("app.sus");
    let wit = directory.path().join("wit");
    std::fs::create_dir_all(wit.join("deps")).unwrap();
    std::fs::write(&source, "(ns app) (defn run [row] (do (rand) row))").unwrap();
    std::fs::write(wit.join("deps/random.wit"), "package wasi:random@0.3.1; interface random { get-random-bytes:func(len:u64)->list<u8>; get-random-u64:func()->u64; }").unwrap();
    std::fs::write(wit.join("api.wit"), "package test:review-random; interface data { record row { n:s64 } } world api { import data; import wasi:random/random@0.3.1; use data.{row}; export run:func(value:row)->row; }").unwrap();
    let bytes = compile_file_with_options(
        &source,
        &wit,
        Some("api"),
        &[],
        &AotOptions {
            exports: vec![("run".into(), Symbol::parse("app/run"))],
            ..Default::default()
        },
    )
    .unwrap();
    let component = Component::new(engine(), bytes).unwrap();
    let imports = component
        .component_type()
        .imports(engine())
        .map(|(name, _)| name.to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        imports,
        ["test:review-random/data", "wasi:random/random@0.3.1"]
    );
    let mut linker = Linker::new(engine());
    let mut random = linker.instance("wasi:random/random@0.3.1").unwrap();
    random
        .func_wrap(
            "get-random-u64",
            |mut context: wasmtime::StoreContextMut<'_, u32>, ()| {
                *context.data_mut() += 1;
                Ok((u64::MAX,))
            },
        )
        .unwrap();
    random
        .func_wrap(
            "get-random-bytes",
            |_, (_length,): (u64,)| -> wasmtime::Result<(Vec<u8>,)> {
                panic!("unexpected byte request")
            },
        )
        .unwrap();
    let mut store = Store::new(engine(), 0_u32);
    store.set_fuel(100_000_000).unwrap();
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let run = instance.get_func(&mut store, "run").unwrap();
    let row = Val::Record(vec![("n".into(), Val::S64(9007199254740993))]);
    let mut output = [Val::Bool(false)];
    run.call(&mut store, &[row.clone()], &mut output).unwrap();
    assert_eq!(output, [row]);
    assert_eq!(*store.data(), 1);
}
