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
fn component(source: &str, exports: &[(&str, &str)], public_wit: &str) -> Component {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.sus");
    let wit = directory.path().join("api.wit");
    std::fs::write(&path, source).unwrap();
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
            imports: [
                ("storage#[constructor]counter", "host/open"),
                ("storage#[method]counter.read", "host/read"),
                ("storage#[static]counter.echo", "host/echo"),
            ]
            .into_iter()
            .map(|(path, source)| (path.into(), Symbol::parse(source)))
            .collect(),
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
