//! Execute opaque resource ownership against an independent Rust resource table.
#![cfg(not(target_family = "wasm"))]
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicUsize, Ordering},
};
use suss_compile::portable_aot::{AotOptions, compile_file_with_options};
use suss_reader::Symbol;
use wasmtime::{
    Config, Engine, Store, StoreContextMut,
    component::{Component, Linker, Resource, ResourceTable, ResourceType},
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
            .consume_fuel(true)
            .cranelift_opt_level(wasmtime::OptLevel::None);
        Engine::new(&config).unwrap()
    })
}
#[derive(Default)]
struct State {
    table: ResourceTable,
    live: Arc<AtomicUsize>,
    opened: usize,
    dropped: usize,
    canonical_peak: usize,
    order: Vec<&'static str>,
}
struct Counter {
    value: u64,
    live: Arc<AtomicUsize>,
}
impl Drop for Counter {
    fn drop(&mut self) {
        assert!(self.live.fetch_sub(1, Ordering::SeqCst) > 0);
    }
}
struct Other {
    _value: u64,
    live: Arc<AtomicUsize>,
}
impl Drop for Other {
    fn drop(&mut self) {
        assert!(self.live.fetch_sub(1, Ordering::SeqCst) > 0);
    }
}
fn component(source: &str, exports: &[(&str, &str)], public_wit: &str) -> Component {
    component_with_other(source, exports, public_wit, false)
}
fn component_with_other(
    source: &str,
    exports: &[(&str, &str)],
    public_wit: &str,
    other: bool,
) -> Component {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.sus");
    let wit = directory.path().join("api.wit");
    std::fs::write(&path, source).unwrap();
    let other_wit = if other {
        "resource other { constructor(value: u64); }"
    } else {
        ""
    };
    std::fs::write(
        &wit,
        format!(
            r#"package test:resource-boundary@1.0.0;
        world api {{ import storage: interface {{
            resource counter {{
                constructor(value: u64);
                read: func() -> u64;
                echo: static func(value: u64) -> u64;
            }}
            {other_wit}
        }} {public_wit} }}"#
        ),
    )
    .unwrap();
    let bytes = compile_file_with_options(
        &path,
        &wit,
        Some("api"),
        &[],
        &AotOptions {
            imports: {
                let mut mappings = vec![
                    ("storage#[constructor]counter", "host/open"),
                    ("storage#[method]counter.read", "host/read"),
                    ("storage#[static]counter.echo", "host/echo"),
                ];
                if other {
                    mappings.push(("storage#[constructor]other", "host/open-other"));
                }
                mappings
                    .into_iter()
                    .map(|(path, source)| (path.into(), Symbol::parse(source)))
                    .collect()
            },
            exports: exports
                .iter()
                .map(|(name, source)| ((*name).into(), Symbol::parse(source)))
                .collect(),
            ..Default::default()
        },
    )
    .unwrap();
    Component::new(engine(), bytes).unwrap()
}
fn linker() -> Linker<State> {
    linker_with_other(false)
}
fn linker_with_other(other: bool) -> Linker<State> {
    let mut linker = Linker::new(engine());
    let mut storage = linker.instance("storage").unwrap();
    storage
        .resource(
            "counter",
            ResourceType::host::<Counter>(),
            |mut context: StoreContextMut<'_, State>, rep| {
                context
                    .data_mut()
                    .table
                    .delete(Resource::<Counter>::new_own(rep))?;
                context.data_mut().dropped += 1;
                context.data_mut().order.push("drop");
                Ok(())
            },
        )
        .unwrap();
    storage
        .func_wrap(
            "[constructor]counter",
            |mut context: StoreContextMut<'_, State>, (value,): (u64,)| {
                let live = context.data().live.clone();
                live.fetch_add(1, Ordering::SeqCst);
                let resource = context.data_mut().table.push(Counter { value, live })?;
                context.data_mut().opened += 1;
                context.data_mut().order.push("open");
                Ok((resource,))
            },
        )
        .unwrap();
    storage
        .func_wrap(
            "[method]counter.read",
            |mut context: StoreContextMut<'_, State>, (counter,): (Resource<Counter>,)| {
                assert!(!counter.owned(), "method self must borrow the owner");
                let value = context.data().table.get(&counter)?.value;
                context.data_mut().order.push("read");
                Ok((value,))
            },
        )
        .unwrap();
    storage
        .func_wrap(
            "[static]counter.echo",
            |mut context: StoreContextMut<'_, State>, (value,): (u64,)| {
                context.data_mut().order.push("static");
                Ok((value,))
            },
        )
        .unwrap();
    if other {
        storage
            .resource(
                "other",
                ResourceType::host::<Other>(),
                |mut context: StoreContextMut<'_, State>, rep| {
                    context
                        .data_mut()
                        .table
                        .delete(Resource::<Other>::new_own(rep))?;
                    context.data_mut().dropped += 1;
                    context.data_mut().order.push("drop-other");
                    Ok(())
                },
            )
            .unwrap();
        storage
            .func_wrap(
                "[constructor]other",
                |mut context: StoreContextMut<'_, State>, (value,): (u64,)| {
                    let live = context.data().live.clone();
                    live.fetch_add(1, Ordering::SeqCst);
                    let resource = context.data_mut().table.push(Other {
                        _value: value,
                        live,
                    })?;
                    context.data_mut().opened += 1;
                    context.data_mut().order.push("open-other");
                    Ok((resource,))
                },
            )
            .unwrap();
    }
    linker
}
#[test]
fn imported_constructor_method_static_and_explicit_close_preserve_exact_values_and_drop_once() {
    let component = component(
        "(ns app) (defn run [value] (let [handle (host/open value) result (host/read handle)] (suss.resource/close! handle) (host/echo result)))",
        &[("run", "app/run")],
        "export run: func(value: u64) -> u64;",
    );
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    let live = store.data().live.clone();
    let instance = linker().instantiate(&mut store, &component).unwrap();
    let run = instance
        .get_typed_func::<(u64,), (u64,)>(&mut store, "run")
        .unwrap();
    for _ in 0..20 {
        for value in [0, 9007199254740993, u64::MAX] {
            store.set_fuel(100_000_000).unwrap();
            assert_eq!(run.call(&mut store, (value,)).unwrap().0, value);
            assert_eq!(live.load(Ordering::SeqCst), 0);
            store.gc(None).unwrap();
        }
    }
    assert_eq!((store.data().opened, store.data().dropped), (60, 60));
    assert_eq!(
        store.data().order,
        ["open", "read", "drop", "static"].repeat(60)
    );
    drop(store);
    assert_eq!(live.load(Ordering::SeqCst), 0);
}

#[test]
fn source_numeric_forgeries_and_closed_aliases_fail_before_host_calls() {
    let component = component(
        "(ns app)
         (defn rejected? [f] (try (f) false (catch :default error true)))
         (defn run [value]
           (let [handle (host/open value) alias handle result (host/read handle)
                 forgeries (every? (fn [x] (rejected? (fn [] (host/read x)))) [0 1 [] {} nil false])]
             (suss.resource/close! handle)
             (if (and forgeries (rejected? (fn [] (host/read alias)))
                      (rejected? (fn [] (suss.resource/close! alias))))
               (host/echo result)
               (throw \"resource validation failed\"))))",
        &[("run", "app/run")],
        "export run: func(value: u64) -> u64;",
    );
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    let live = store.data().live.clone();
    let instance = linker().instantiate(&mut store, &component).unwrap();
    let run = instance
        .get_typed_func::<(u64,), (u64,)>(&mut store, "run")
        .unwrap();
    for value in [0, 9007199254740993, u64::MAX] {
        store.set_fuel(100_000_000).unwrap();
        assert_eq!(run.call(&mut store, (value,)).unwrap().0, value);
        assert_eq!(live.load(Ordering::SeqCst), 0);
        store.gc(None).unwrap();
    }
    assert_eq!((store.data().opened, store.data().dropped), (3, 3));
    assert_eq!(
        store.data().order,
        ["open", "read", "drop", "static"].repeat(3)
    );
}

#[test]
fn unsupported_resource_domains_reject_before_source_macro_effects() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("app.sus");
    let wit = directory.path().join("api.wit");
    std::fs::write(
        &source,
        "(ns app) (defmacro effect [] (throw \"macro-ran\")) (defn run [] (effect))",
    )
    .unwrap();
    for (declarations, mappings, expected) in [
        (
            "interface store { resource item { constructor(); } } world api { import left: store; import right: store; export run: func(); }",
            vec![
                ("left#[constructor]item", "host/left"),
                ("right#[constructor]item", "host/right"),
            ],
            "contextual resource identities",
        ),
        (
            "interface store { resource item; } interface operations { use store.{item}; read: func(x: borrow<item>); } world api { import store; import operations; export run: func(); }",
            vec![("test:resource-guards/operations#read", "host/read")],
            "Foreign resource dependencies",
        ),
        (
            "interface store { resource item; consume: func(x: own<item>); } world api { import store; export run: func(); }",
            vec![("test:resource-guards/store#consume", "host/consume")],
            "move tracking",
        ),
        (
            "interface store { resource item; peek: func() -> borrow<item>; } world api { import store; export run: func(); }",
            vec![("test:resource-guards/store#peek", "host/peek")],
            "contains a `borrow<T>`",
        ),
        (
            "interface store { resource item { constructor(); } } world api { import store; use store.{item}; export run: func(x: borrow<item>); }",
            vec![("test:resource-guards/store#[constructor]item", "host/open")],
            "Exported resource handles",
        ),
    ] {
        std::fs::write(
            &wit,
            format!("package test:resource-guards; {declarations}"),
        )
        .unwrap();
        let error = compile_file_with_options(
            &source,
            &wit,
            Some("api"),
            &[],
            &AotOptions {
                imports: mappings
                    .into_iter()
                    .map(|(path, name)| (path.into(), Symbol::parse(name)))
                    .collect(),
                exports: vec![("run".into(), Symbol::namespaced("app", "run"))],
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("compiled macro invocation"), "{error}");
    }
    std::fs::write(
        &wit,
        "package test:resource-guards; world api { import ping: func(); export run: func(); }",
    )
    .unwrap();
    let error = compile_file_with_options(
        &source,
        &wit,
        Some("api"),
        &[],
        &AotOptions {
            imports: vec![("ping".into(), Symbol::namespaced("host", "ping"))],
            exports: vec![("run".into(), Symbol::namespaced("app", "run"))],
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(
        error.contains("compiled macro invocation")
            && error.contains("Uncaught language exception"),
        "macro calibration did not execute: {error}"
    );
}

#[test]
fn opaque_handles_cannot_cross_declared_resource_types() {
    let component = component_with_other(
        "(ns app) (defn run [value]
           (let [counter (host/open value) other (host/open-other value) result (host/read counter)
                 rejected (try (host/read other) false (catch :default error true))]
             (suss.resource/close! other)
             (suss.resource/close! counter)
             (if rejected result (throw \"cross-type handle was accepted\"))))",
        &[("run", "app/run")],
        "export run: func(value: u64) -> u64;",
        true,
    );
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    let live = store.data().live.clone();
    let instance = linker_with_other(true)
        .instantiate(&mut store, &component)
        .unwrap();
    let run = instance
        .get_typed_func::<(u64,), (u64,)>(&mut store, "run")
        .unwrap();
    for value in [0, 9007199254740993, u64::MAX] {
        store.set_fuel(100_000_000).unwrap();
        assert_eq!(run.call(&mut store, (value,)).unwrap().0, value);
        assert_eq!(live.load(Ordering::SeqCst), 0);
        store.gc(None).unwrap();
    }
    assert_eq!((store.data().opened, store.data().dropped), (6, 6));
    assert_eq!(
        store.data().order,
        ["open", "open-other", "read", "drop-other", "drop"].repeat(3)
    );
}

#[test]
fn source_try_finally_releases_owned_resources_before_caught_language_errors_return() {
    let component = component(
        "(ns app)
         (defn run [value]
           (try
             (let [handle (host/open value)]
               (try (host/read handle) (throw :source-failure)
                    (finally (suss.resource/close! handle))))
             (catch :default error (host/echo value))))",
        &[("run", "app/run")],
        "export run: func(value: u64) -> u64;",
    );
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    let live = store.data().live.clone();
    let instance = linker().instantiate(&mut store, &component).unwrap();
    let run = instance
        .get_typed_func::<(u64,), (u64,)>(&mut store, "run")
        .unwrap();
    for value in [0, 9007199254740993, u64::MAX] {
        store.set_fuel(100_000_000).unwrap();
        assert_eq!(run.call(&mut store, (value,)).unwrap().0, value);
        assert_eq!(live.load(Ordering::SeqCst), 0);
        store.gc(None).unwrap();
    }
    assert_eq!((store.data().opened, store.data().dropped), (3, 3));
    assert_eq!(
        store.data().order,
        ["open", "read", "drop", "static"].repeat(3)
    );
}

impl wasmtime::ResourceLimiter for State {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        _: Option<usize>,
    ) -> wasmtime::Result<bool> {
        // Runtime numeric/GC memories have their own initialization sizes.
        // Small canonical argument/result buffers must reuse one transfer page.
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

#[test]
fn ten_thousand_successful_and_failed_resource_operations_return_live_and_transfer_counters_to_baseline()
 {
    let component = component(
        "(ns app) (defn run [value fail]
           (try
             (let [handle (host/open value)]
               (try
                 (let [result (host/read handle)]
                   (if fail (throw :language-failure) [:ok (host/echo result)]))
                 (finally (suss.resource/close! handle))))
             (catch :default error [:err \"language-failure\"])))",
        &[("run", "app/run")],
        "export run: func(value: u64, fail: bool) -> result<u64, string>;",
    );
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    store.limiter(|state| state);
    let live = store.data().live.clone();
    let instance = linker().instantiate(&mut store, &component).unwrap();
    let run = instance
        .get_typed_func::<(u64, bool), (Result<u64, String>,)>(&mut store, "run")
        .unwrap();
    for index in 0..10_000 {
        let value = [0, 9007199254740993, u64::MAX][index % 3];
        let fail = index % 2 != 0;
        store.set_fuel(100_000_000).unwrap();
        assert_eq!(
            run.call(&mut store, (value, fail)).unwrap().0,
            if fail {
                Err("language-failure".into())
            } else {
                Ok(value)
            }
        );
        assert_eq!(live.load(Ordering::SeqCst), 0);
        if index % 100 == 0 {
            store.gc(None).unwrap();
        }
    }
    store.gc(None).unwrap();
    assert_eq!(
        (store.data().opened, store.data().dropped),
        (10_000, 10_000)
    );
    assert_eq!(
        store
            .data()
            .order
            .iter()
            .filter(|name| **name == "read")
            .count(),
        10_000
    );
    assert_eq!(
        store
            .data()
            .order
            .iter()
            .filter(|name| **name == "static")
            .count(),
        5_000
    );
    assert_eq!(store.data().canonical_peak, 65536);
}
