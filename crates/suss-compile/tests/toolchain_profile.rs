//! M0-02 probes use upstream Rust APIs as well as the separately pinned CLI tools.
//! Resolving WIT does not claim that the Suss compiler generates these bindings.

use std::path::Path;
use wit_parser::Resolve;

#[test]
fn official_wasi_package_graphs_resolve_with_published_versions() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/wasi/wasi-wit-0.3.1");
    let lock: serde_json::Value =
        serde_json::from_str(include_str!("../../../docs/roadmap/wasi-wit-lock.json"))
            .expect("reviewed official WIT lock");
    for (name, package) in lock["packages"].as_object().expect("package graph") {
        let mut resolve = Resolve::default();
        let path = package["path"].as_str().expect("package path");
        let (id, _) = resolve
            .push_dir(root.join(path))
            .unwrap_or_else(|error| panic!("{name}: official WIT resolution failed: {error:#}"));
        assert_eq!(resolve.packages[id].name.to_string(), *name);
        let mut actual: Vec<_> = resolve
            .packages
            .iter()
            .map(|(_, p)| p.name.to_string())
            .collect();
        actual.sort();
        let mut expected: Vec<_> = lock["files"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(file, _)| file.starts_with(&format!("{path}/")))
            .map(|(_, entry)| entry["package"].as_str().unwrap().to_owned())
            .collect();
        expected.sort();
        expected.dedup();
        assert_eq!(actual, expected, "{name}: package versions/closure changed");
    }
}

#[test]
fn extended_profile_resolves_maps_async_future_stream_and_external_id() {
    let mut resolve = Resolve::default();
    let id = resolve
        .push_str(
            "profile.wit",
            include_str!("../../../tests/toolchain/profile.wit"),
        )
        .expect("candidate compiler tools must resolve the declared WASI 0.3.1 profile");
    let world = resolve
        .select_world(&[id], Some("profile"))
        .expect("selected profile world");
    assert_eq!(resolve.worlds[world].name, "profile");
    let kinds: Vec<_> = resolve
        .types
        .iter()
        .map(|(_, ty)| ty.kind.as_str())
        .collect();
    for required in ["map", "future", "stream"] {
        assert!(kinds.contains(&required), "missing profile type {required}");
    }
}

#[test]
fn prototype_reports_unsupported_boundary_shapes_instead_of_unknown_types() {
    // Parser support is distinct from implemented language adapters. These
    // diagnostics can be retired one shape at a time with executing fixtures.
    for (shape, diagnostic) in [
        ("map<string, u32>", "map"),
        ("future<u8>", "future"),
        ("stream<u8>", "stream"),
        ("error-context", "error-context"),
        ("list<map<string, u32>>", "map"),
    ] {
        let wit = format!(
            "package test:unsupported; world boundary {{ export echo: func(x: {shape}) -> {shape}; }}"
        );
        let error = suss_compile::Compiler::new()
            .compile("(defn ^:export echo [x] x)", &wit)
            .expect_err("prototype must not silently invent an adapter for unsupported WIT");
        assert!(
            matches!(error, suss_compile::CompileError::Unsupported(_)),
            "{shape}: {error}"
        );
        assert!(error.to_string().contains(diagnostic), "{shape}: {error}");
    }
}

#[test]
fn prototype_reports_async_exports_before_encoding() {
    let error = suss_compile::Compiler::new()
        .compile(
            "(defn ^:export echo [x] x)",
            "package test:unsupported; world boundary { export echo: async func(x: u32) -> u32; }",
        )
        .expect_err("async language lowering remains unimplemented");
    assert!(
        matches!(error, suss_compile::CompileError::Unsupported(_)),
        "{error}"
    );
    assert!(error.to_string().contains("async"), "{error}");
}

#[test]
fn canonical_map_values_cross_guest_memory_in_both_directions() {
    use std::collections::BTreeMap;
    use wasmtime::component::{Component, Linker, Val};
    use wasmtime::{Config, Engine, Store};
    let mut config = Config::new();
    config.wasm_component_model_map(true);
    let engine = Engine::new(&config).expect("candidate map feature");
    let component = Component::new(&engine, include_str!("fixtures/map-roundtrip.wat"))
        .expect("validated canonical map component");
    let mut store = Store::new(&engine, ());
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let echo = instance.get_func(&mut store, "echo").expect("map export");
    for input in [
        BTreeMap::new(),
        BTreeMap::from([
            ("".to_owned(), 0),
            ("雪🦀".to_owned(), u32::MAX),
            ("alpha".to_owned(), 42),
        ]),
    ] {
        let value = Val::Map(
            input
                .iter()
                .map(|(key, value)| (Val::String(key.clone()), Val::U32(*value)))
                .collect(),
        );
        let mut results = [Val::Bool(false)];
        echo.call(&mut store, &[value], &mut results)
            .expect("host map lowered and guest result lifted");
        let Val::Map(entries) = &results[0] else {
            panic!("expected lifted map, got {:?}", results[0]);
        };
        let actual: BTreeMap<_, _> = entries
            .iter()
            .map(|(key, value)| {
                let (Val::String(key), Val::U32(value)) = (key, value) else {
                    panic!("wrong map entry type");
                };
                (key.clone(), *value)
            })
            .collect();
        assert_eq!(actual, input);
        echo.post_return(&mut store).expect("finish canonical call");
    }
}

#[test]
fn rust_engine_executes_required_core_features() {
    use wasmtime::{Config, Engine, Instance, Module, Store};
    let mut config = Config::new();
    config
        .wasm_gc(true)
        .wasm_function_references(true)
        .wasm_tail_call(true)
        .wasm_exceptions(true)
        .consume_fuel(true);
    let engine = Engine::new(&config).unwrap();
    for (feature, wat) in [
        ("GC", include_str!("../../../tests/toolchain/gc.wat")),
        (
            "typed function references",
            include_str!("../../../tests/toolchain/function-references.wat"),
        ),
        (
            "tail calls",
            include_str!("../../../tests/toolchain/tail-call.wat"),
        ),
        (
            "exceptions",
            include_str!("../../../tests/toolchain/exceptions.wat"),
        ),
    ] {
        let module = Module::new(&engine, wat).unwrap_or_else(|e| panic!("{feature}: {e:#}"));
        let mut store = Store::new(&engine, ());
        store.set_fuel(100_000).unwrap();
        let instance = Instance::new(&mut store, &module, &[]).unwrap();
        let answer = instance
            .get_typed_func::<(), i32>(&mut store, "answer")
            .unwrap();
        assert_eq!(answer.call(&mut store, ()).unwrap(), 42, "{feature}");
    }
}

#[test]
fn implements_import_binds_and_calls_the_named_instance() {
    use wasmtime::component::{Component, Linker};
    use wasmtime::{Config, Engine, Store};
    let mut config = Config::new();
    config.wasm_component_model_implements(true);
    let engine = Engine::new(&config).unwrap();
    let component = Component::new(
        &engine,
        r#"
        (component
          (type $logger (instance (export "log" (func (param "value" u32)))))
          (import "primary" (implements "probe:runtime/logger") (instance $primary (type $logger)))
          (alias export $primary "log" (func $log))
          (core func $lower-log (canon lower (func $log)))
          (core module $guest
            (import "host" "log" (func $log (param i32)))
            (func (export "run") (param i32) (call $log (local.get 0))))
          (core instance $host (export "log" (func $lower-log)))
          (core instance $guest (instantiate $guest (with "host" (instance $host))))
          (type $run-type (func (param "value" u32)))
          (func $run (type $run-type) (canon lift (core func $guest "run")))
          (export "run" (func $run)))
    "#,
    )
    .expect("validated implements import");
    let mut linker = Linker::<Vec<String>>::new(&engine);
    linker
        .instance("primary")
        .unwrap()
        .func_wrap("log", |mut context, (value,): (u32,)| {
            context.data_mut().push(value.to_string());
            Ok(())
        })
        .unwrap();
    let mut store = Store::new(&engine, Vec::new());
    let instance = linker
        .instantiate(&mut store, &component)
        .expect("resolve named implements instance");
    let run = instance
        .get_typed_func::<(u32,), ()>(&mut store, "run")
        .unwrap();
    run.call(&mut store, (42,)).unwrap();
    run.post_return(&mut store).unwrap();
    assert_eq!(store.data(), &["42".to_owned()]);
}
