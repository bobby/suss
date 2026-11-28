//! A resource declaration and its users share one independent Rust identity.
#![cfg(not(target_family = "wasm"))]
use std::sync::OnceLock;
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
            .consume_fuel(true)
            .cranelift_opt_level(wasmtime::OptLevel::None);
        Engine::new(&config).unwrap()
    })
}
struct Item(i64);
#[derive(Default)]
struct State {
    table: ResourceTable,
    drops: usize,
    reads: usize,
    relayed: Option<u32>,
}
fn component() -> Component {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("app.sus");
    let wit = directory.path().join("api.wit");
    std::fs::write(
        &wit,
        r#"package test:resource-dependency;
        interface objects { resource item { constructor(value:s64); value:func()->s64; } }
        interface consumer {
            use objects.{item as ticket};
            read:func(value:borrow<ticket>)->s64;
            relay:func(value:own<ticket>)->own<ticket>;
        }
        world api {
            import objects; import consumer; use objects.{item};
            export run:func(value:s64)->s64;
            export fail:func()->s64;
            export inspect:func(value:borrow<item>)->s64;
            export roundtrip:func(value:own<item>)->own<item>;
        }"#,
    )
    .unwrap();
    std::fs::write(
        &source,
        r#"(ns app)
        (defn run [value]
          (suss.resource/call-with-scope
            (fn [] (let [item (host/new value)] (+ (host/value item) (host/read item))))))
        (defn inspect [value] (host/read value))
        (defn fail []
          (try
            (suss.resource/call-with-scope
              (fn [] (let [item (host/new 17)] (host/read item) (throw "stop"))))
            (catch :default error 77)))
        (defn roundtrip [value]
          (let [result (host/relay value)]
            (when-not (try (host/read value) false (catch :default error true))
              (throw "moved alias remained usable"))
            result))"#,
    )
    .unwrap();
    let bytes = compile_file_with_options(
        &source,
        &wit,
        Some("api"),
        &[],
        &AotOptions {
            imports: [
                (
                    "test:resource-dependency/objects#[constructor]item",
                    "host/new",
                ),
                (
                    "test:resource-dependency/objects#[method]item.value",
                    "host/value",
                ),
                ("test:resource-dependency/consumer#read", "host/read"),
                ("test:resource-dependency/consumer#relay", "host/relay"),
            ]
            .into_iter()
            .map(|(name, source)| (name.into(), Symbol::parse(source)))
            .collect(),
            exports: ["run", "fail", "inspect", "roundtrip"]
                .into_iter()
                .map(|name| (name.into(), Symbol::parse(&format!("app/{name}"))))
                .collect(),
            ..Default::default()
        },
    )
    .unwrap();
    Component::new(engine(), bytes).unwrap()
}
fn linker() -> Linker<State> {
    let mut linker = Linker::new(engine());
    let mut objects = linker.instance("test:resource-dependency/objects").unwrap();
    objects
        .resource(
            "item",
            ResourceType::host::<Item>(),
            |mut cx: StoreContextMut<'_, State>, rep| {
                cx.data_mut().table.delete(Resource::<Item>::new_own(rep))?;
                cx.data_mut().drops += 1;
                Ok(())
            },
        )
        .unwrap();
    objects
        .func_wrap(
            "[constructor]item",
            |mut cx: StoreContextMut<'_, State>, (value,): (i64,)| {
                Ok((cx.data_mut().table.push(Item(value))?,))
            },
        )
        .unwrap();
    objects
        .func_wrap(
            "[method]item.value",
            |cx: StoreContextMut<'_, State>, (value,): (Resource<Item>,)| {
                assert!(!value.owned());
                Ok((cx.data().table.get(&value)?.0,))
            },
        )
        .unwrap();
    let mut consumer = linker
        .instance("test:resource-dependency/consumer")
        .unwrap();
    // This is an alias of the same host type, not a second resource identity.
    consumer
        .resource(
            "ticket",
            ResourceType::host::<Item>(),
            |_: StoreContextMut<'_, State>, _| {
                panic!("an alias must not install a second destructor")
            },
        )
        .unwrap();
    consumer
        .func_wrap(
            "read",
            |mut cx: StoreContextMut<'_, State>, (value,): (Resource<Item>,)| {
                assert!(!value.owned());
                let result = cx.data().table.get(&value)?.0;
                cx.data_mut().reads += 1;
                Ok((result,))
            },
        )
        .unwrap();
    consumer
        .func_wrap(
            "relay",
            |mut cx: StoreContextMut<'_, State>, (value,): (Resource<Item>,)| {
                assert!(value.owned());
                cx.data_mut().relayed = Some(value.rep());
                Ok((value,))
            },
        )
        .unwrap();
    linker
}
#[test]
fn imported_resource_dependencies_preserve_borrows_moves_identity_and_one_destructor() {
    let component = component();
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    let instance = linker().instantiate(&mut store, &component).unwrap();
    let run = instance
        .get_typed_func::<(i64,), (i64,)>(&mut store, "run")
        .unwrap();
    for value in [0, -17, 9007199254740993] {
        assert_eq!(run.call(&mut store, (value,)).unwrap().0, value * 2);
        store.gc(None).unwrap();
    }
    assert_eq!(store.data().drops, 3);
    let fail = instance
        .get_typed_func::<(), (i64,)>(&mut store, "fail")
        .unwrap();
    assert_eq!(fail.call(&mut store, ()).unwrap().0, 77);
    assert_eq!(store.data().drops, 4);
    let handle = store.data_mut().table.push(Item(i64::MIN)).unwrap();
    let rep = handle.rep();
    let inspect = instance
        .get_typed_func::<(Resource<Item>,), (i64,)>(&mut store, "inspect")
        .unwrap();
    assert_eq!(
        inspect
            .call(&mut store, (Resource::new_borrow(rep),))
            .unwrap()
            .0,
        i64::MIN
    );
    let roundtrip = instance
        .get_typed_func::<(Resource<Item>,), (Resource<Item>,)>(&mut store, "roundtrip")
        .unwrap();
    let (returned,) = roundtrip.call(&mut store, (handle,)).unwrap();
    assert!(returned.owned());
    assert_eq!(returned.rep(), rep);
    assert_eq!(store.data().relayed, Some(rep));
    assert_eq!(store.data().table.get(&returned).unwrap().0, i64::MIN);
    assert_eq!((store.data().reads, store.data().drops), (5, 4));
    store.data_mut().table.delete(returned).unwrap();
}
