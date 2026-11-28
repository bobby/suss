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
    consumed: usize,
    last_opened_rep: u32,
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
    component_with_features(source, exports, public_wit, other, false)
}
fn component_with_features(
    source: &str,
    exports: &[(&str, &str)],
    public_wit: &str,
    other: bool,
    transfers: bool,
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
    let transfer_wit = if transfers {
        r#"
        record transfer { left: own<counter>, right: option<own<counter>> }
        recorded: func(items: list<transfer>) -> result<own<counter>, string>;
        consume: func(handle: own<counter>) -> u64;
        consume-error: func(handle: own<counter>) -> result<u64, string>;
        peek-pair: func(left: borrow<counter>, right: borrow<counter>) -> u64;
        checked: func(handle: own<counter>, later: u8) -> u64;
        pair: func(left: own<counter>, right: own<counter>) -> u64;
        nested: func(items: list<tuple<own<counter>, option<own<counter>>>>) -> u64;
        mixed: func(owner: own<counter>, alias: borrow<counter>) -> u64;
        mixed-reverse: func(alias: borrow<counter>, owner: own<counter>) -> u64;
        roundtrip: func(handle: own<counter>) -> own<counter>;
    "#
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
            {transfer_wit}
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
                if transfers {
                    mappings.extend([
                        ("storage#consume", "host/consume"),
                        ("storage#recorded", "host/recorded"),
                        ("storage#consume-error", "host/consume-error"),
                        ("storage#peek-pair", "host/peek-pair"),
                        ("storage#checked", "host/checked"),
                        ("storage#pair", "host/pair"),
                        ("storage#nested", "host/nested"),
                        ("storage#mixed", "host/mixed"),
                        ("storage#mixed-reverse", "host/mixed-reverse"),
                        ("storage#roundtrip", "host/roundtrip"),
                    ]);
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
                context.data_mut().last_opened_rep = resource.rep();
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
fn consume(
    context: &mut StoreContextMut<'_, State>,
    resource: Resource<Counter>,
) -> wasmtime::Result<u64> {
    assert!(resource.owned(), "consume must transfer ownership");
    let counter = context.data_mut().table.delete(resource)?;
    context.data_mut().consumed += 1;
    context.data_mut().order.push("consume");
    Ok(counter.value)
}
#[derive(
    wasmtime::component::ComponentType, wasmtime::component::Lift, wasmtime::component::Lower,
)]
#[component(record)]
struct Transfer {
    left: Resource<Counter>,
    right: Option<Resource<Counter>>,
}

fn transfer_linker() -> Linker<State> {
    let mut linker = linker();
    let mut storage = linker.instance("storage").unwrap();
    storage
        .func_wrap(
            "recorded",
            |mut cx: StoreContextMut<'_, State>, (mut items,): (Vec<Transfer>,)| {
                assert_eq!(
                    items.len(),
                    1,
                    "duplicate record owners must reject before the host"
                );
                let item = items.pop().unwrap();
                assert!(item.left.owned());
                let value = cx.data().table.get(&item.left)?.value;
                assert_eq!(consume(&mut cx, item.right.expect("right owner"))?, value);
                cx.data_mut().order.push("recorded");
                Ok((Ok::<Resource<Counter>, String>(item.left),))
            },
        )
        .unwrap();
    storage
        .func_wrap(
            "consume",
            |mut cx: StoreContextMut<'_, State>, (resource,): (Resource<Counter>,)| {
                Ok((consume(&mut cx, resource)?,))
            },
        )
        .unwrap();
    storage
        .func_wrap(
            "consume-error",
            |mut cx: StoreContextMut<'_, State>, (resource,): (Resource<Counter>,)| {
                consume(&mut cx, resource)?;
                Ok((Err::<u64, String>("host-failure".into()),))
            },
        )
        .unwrap();
    storage
        .func_wrap(
            "peek-pair",
            |mut cx: StoreContextMut<'_, State>,
             (left, right): (Resource<Counter>, Resource<Counter>)| {
                assert!(!left.owned() && !right.owned());
                assert_eq!(left.rep(), right.rep());
                let value = cx.data().table.get(&left)?.value;
                assert_eq!(value, cx.data().table.get(&right)?.value);
                cx.data_mut().order.push("peek-pair");
                Ok((value,))
            },
        )
        .unwrap();
    storage
        .func_wrap(
            "checked",
            |mut cx: StoreContextMut<'_, State>, (resource, later): (Resource<Counter>, u8)| {
                assert_eq!(later, 7);
                Ok((consume(&mut cx, resource)?,))
            },
        )
        .unwrap();
    storage
        .func_wrap(
            "pair",
            |mut cx: StoreContextMut<'_, State>,
             (left, right): (Resource<Counter>, Resource<Counter>)| {
                let value = consume(&mut cx, left)?;
                assert_eq!(consume(&mut cx, right)?, value);
                Ok((value,))
            },
        )
        .unwrap();
    storage
        .func_wrap(
            "nested",
            |mut cx: StoreContextMut<'_, State>,
             (items,): (Vec<(Resource<Counter>, Option<Resource<Counter>>)>,)| {
                let mut value = 0;
                for (left, right) in items {
                    value = consume(&mut cx, left)?;
                    if let Some(right) = right {
                        assert_eq!(consume(&mut cx, right)?, value);
                    }
                }
                Ok((value,))
            },
        )
        .unwrap();
    storage
        .func_wrap(
            "mixed",
            |_: StoreContextMut<'_, State>,
             _: (Resource<Counter>, Resource<Counter>)|
             -> wasmtime::Result<(u64,)> {
                panic!("conflicting own and borrow must reject before host invocation")
            },
        )
        .unwrap();
    storage
        .func_wrap(
            "mixed-reverse",
            |_: StoreContextMut<'_, State>,
             _: (Resource<Counter>, Resource<Counter>)|
             -> wasmtime::Result<(u64,)> {
                panic!("conflicting borrow and own must reject before host invocation")
            },
        )
        .unwrap();
    storage
        .func_wrap(
            "roundtrip",
            |mut cx: StoreContextMut<'_, State>, (resource,): (Resource<Counter>,)| {
                assert!(resource.owned());
                let rep = resource.rep();
                assert_eq!(
                    rep,
                    cx.data().last_opened_rep,
                    "host roundtrip must preserve the original physical resource"
                );
                cx.data_mut().order.push("roundtrip");
                Ok((resource,))
            },
        )
        .unwrap();
    linker
}

#[test]
fn own_moves_invalidate_aliases_and_rejected_calls_preserve_all_owners() {
    let component = component_with_features(
        r#"(ns app)
        (defn rejected? [f] (try (f) false (catch :default error true)))
        (defn verify [ok] (when-not ok (throw "ownership assertion failed")))
        (defn run [value]
          (let [a (host/open value) alias a result (host/consume a)]
            (verify (= result value))
            (verify (rejected? (fn [] (host/read alias))))
            (verify (rejected? (fn [] (suss.resource/close! alias)))))
          (let [b (host/open value)]
            (verify (rejected? (fn [] (host/pair b b))))
            (verify (= value (host/read b)))
            (verify (rejected? (fn [] (host/checked b 256))))
            (verify (= value (host/read b)))
            (verify (rejected? (fn [] (host/mixed b b))))
            (verify (= value (host/read b)))
            (verify (rejected? (fn [] (host/mixed-reverse b b))))
            (verify (= value (host/read b)))
            (verify (= value (host/checked b 7))))
          (let [c (host/open value) d (host/open value)]
            (verify (rejected? (fn [] (host/nested [[c [:some d]] [c [:none]]]))))
            (verify (= value (host/read c)))
            (verify (= value (host/read d)))
            (verify (= value (host/nested [[c [:some d]]]))))
          (let [e (host/open value) f (host/roundtrip e)]
            (verify (rejected? (fn [] (host/read e))))
            (verify (rejected? (fn [] (suss.resource/close! e))))
            (verify (= value (host/read f)))
            (verify (= value (host/peek-pair f f)))
            (suss.resource/close! f))
          (let [g (host/open value) h (host/open value)]
            (verify (= value (host/pair g h)))
            (verify (rejected? (fn [] (host/read g))))
            (verify (rejected? (fn [] (host/read h)))))
          value)"#,
        &[("run", "app/run")],
        "export run: func(value: u64) -> u64;",
        false,
        true,
    );
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    let live = store.data().live.clone();
    let instance = transfer_linker()
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
    assert_eq!(
        (
            store.data().opened,
            store.data().consumed,
            store.data().dropped
        ),
        (21, 18, 3)
    );
    assert_eq!(
        store.data().order,
        [
            "open",
            "consume",
            "open",
            "read",
            "read",
            "read",
            "read",
            "consume",
            "open",
            "open",
            "read",
            "read",
            "consume",
            "consume",
            "open",
            "roundtrip",
            "read",
            "peek-pair",
            "drop",
            "open",
            "open",
            "consume",
            "consume"
        ]
        .repeat(3)
    );
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
            "interface store { resource item; wait: async func(); } world api { import store; export run: func(); }",
            vec![("test:resource-guards/store#wait", "host/wait")],
            "Asynchronous resource function lifetimes",
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

#[test]
fn ten_thousand_own_roundtrips_and_rejections_preserve_host_identity_and_release_transfer_memory() {
    let component = component_with_features(
        r#"(ns app)
        (defn rejected? [f] (try (f) false (catch :default error true)))
        (defn verify [ok] (when-not ok (throw "ownership assertion failed")))
        (defn run [value fail]
          (let [original (host/open value) handle (host/roundtrip original)]
            (verify (rejected? (fn [] (host/read original))))
            (verify (rejected? (fn [] (host/nested [[handle [:some handle]]]))))
            (verify (= value (host/read handle)))
            (let [result (if fail (host/consume-error handle)
                           [:ok (host/nested [[handle [:none]]])])]
              (verify (rejected? (fn [] (host/read handle))))
              (verify (rejected? (fn [] (suss.resource/close! handle))))
              result)))"#,
        &[("run", "app/run")],
        "export run: func(value: u64, fail: bool) -> result<u64, string>;",
        false,
        true,
    );
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    store.limiter(|state| state);
    let live = store.data().live.clone();
    let instance = transfer_linker()
        .instantiate(&mut store, &component)
        .unwrap();
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
                Err("host-failure".into())
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
        (
            store.data().opened,
            store.data().consumed,
            store.data().dropped
        ),
        (10_000, 10_000, 0)
    );
    assert_eq!(
        store.data().order,
        ["open", "roundtrip", "read", "consume"].repeat(10_000)
    );
    assert_eq!(store.data().canonical_peak, 65536);
}

#[test]
fn named_resource_records_project_native_maps_and_transfer_owned_result_payloads() {
    let component = component_with_features(
        r#"(ns app)
        (defn rejected? [f] (try (f) false (catch :default error true)))
        (defn verify [ok] (when-not ok (throw "ownership assertion failed")))
        (defn run [value]
          (let [a (host/open value) b (host/open value)
                item (assoc {:left a} :right [:some b])]
            (verify (rejected? (fn [] (host/recorded [item {:left a :right [:none]}]))))
            (verify (rejected? (fn [] (host/recorded [(assoc item :extra 0)]))))
            (verify (rejected? (fn [] (host/recorded [{:left a}]))))
            (verify (= value (host/read a)))
            (verify (= value (host/read b)))
            (let [result (host/recorded [item]) owner (nth result 1)]
              (verify (= :ok (nth result 0)))
              (verify (rejected? (fn [] (host/read a))))
              (verify (rejected? (fn [] (suss.resource/close! b))))
              (let [result (host/read owner)] (suss.resource/close! owner) result))))"#,
        &[("run", "app/run")],
        "export run: func(value: u64) -> u64;",
        false,
        true,
    );
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    let live = store.data().live.clone();
    let instance = transfer_linker()
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
    assert_eq!(
        (
            store.data().opened,
            store.data().consumed,
            store.data().dropped
        ),
        (6, 3, 3)
    );
    assert_eq!(
        store.data().order,
        [
            "open", "open", "read", "read", "consume", "recorded", "read", "drop"
        ]
        .repeat(3)
    );
}

#[test]
fn runtime_initializer_owns_one_resource_across_exports_and_gc_until_explicit_close() {
    let component = component(
        r#"(ns app)
        (def owner (host/open 18446744073709551615N))
        (defn read [] (host/read owner))
        (defn close [] (suss.resource/close! owner))
        (defn rejected []
          (and (try (host/read owner) false (catch :default error true))
               (try (suss.resource/close! owner) false (catch :default error true))))"#,
        &[
            ("read", "app/read"),
            ("close", "app/close"),
            ("rejected", "app/rejected"),
        ],
        "export read: func() -> u64; export close: func(); export rejected: func() -> bool;",
    );
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    let live = store.data().live.clone();
    let instance = linker().instantiate(&mut store, &component).unwrap();
    assert_eq!((store.data().opened, store.data().dropped), (1, 0));
    assert_eq!(live.load(Ordering::SeqCst), 1);
    let read = instance
        .get_typed_func::<(), (u64,)>(&mut store, "read")
        .unwrap();
    let close = instance
        .get_typed_func::<(), ()>(&mut store, "close")
        .unwrap();
    let rejected = instance
        .get_typed_func::<(), (bool,)>(&mut store, "rejected")
        .unwrap();
    for _ in 0..3 {
        store.gc(None).unwrap();
        store.set_fuel(100_000_000).unwrap();
        assert_eq!(read.call(&mut store, ()).unwrap().0, u64::MAX);
        assert_eq!(live.load(Ordering::SeqCst), 1);
    }
    store.set_fuel(100_000_000).unwrap();
    close.call(&mut store, ()).unwrap();
    store.gc(None).unwrap();
    assert_eq!(live.load(Ordering::SeqCst), 0);
    assert!(rejected.call(&mut store, ()).unwrap().0);
    assert_eq!((store.data().opened, store.data().dropped), (1, 1));
    assert_eq!(store.data().order, ["open", "read", "read", "read", "drop"]);
}
