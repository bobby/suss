//! Independent Rust owners outlive individual calls into the guest.
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
            .wasm_component_model_map(true)
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
    constructors: usize,
    last_constructed_rep: Option<u32>,
    reads: usize,
    consumes: usize,
    canonical_drops: usize,
    canonical_peak: usize,
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
fn linker() -> Linker<State> {
    let mut linker = Linker::new(engine());
    let mut storage = linker.instance("test:borrow-boundary/store@1.0.0").unwrap();
    storage
        .resource(
            "counter",
            ResourceType::host::<Counter>(),
            |mut cx: StoreContextMut<'_, State>, rep| {
                cx.data_mut()
                    .table
                    .delete(Resource::<Counter>::new_own(rep))?;
                cx.data_mut().canonical_drops += 1;
                Ok(())
            },
        )
        .unwrap();
    storage
        .func_wrap(
            "[constructor]counter",
            |mut cx: StoreContextMut<'_, State>, (value,): (u64,)| {
                let live = cx.data().live.clone();
                live.fetch_add(1, Ordering::SeqCst);
                let resource = cx.data_mut().table.push(Counter { value, live })?;
                cx.data_mut().last_constructed_rep = Some(resource.rep());
                cx.data_mut().constructors += 1;
                Ok((resource,))
            },
        )
        .unwrap();
    storage
        .func_wrap(
            "[method]counter.read",
            |mut cx: StoreContextMut<'_, State>, (handle,): (Resource<Counter>,)| {
                assert!(!handle.owned());
                let value = cx.data().table.get(&handle)?.value;
                cx.data_mut().reads += 1;
                Ok((value,))
            },
        )
        .unwrap();
    storage
        .func_wrap(
            "consume",
            |mut cx: StoreContextMut<'_, State>, (handle,): (Resource<Counter>,)| {
                assert!(handle.owned());
                let counter = cx.data_mut().table.delete(handle)?;
                cx.data_mut().consumes += 1;
                Ok((counter.value,))
            },
        )
        .unwrap();
    linker
}
fn component() -> Component {
    component_with(
        r#"package test:borrow-boundary@1.0.0;
        interface store {
            resource counter { constructor(value: u64); read: func() -> u64; }
            consume: func(handle: own<counter>) -> u64;
        }
        world api {
            import store;
            use store.{counter};
            export save: func(handle: borrow<counter>) -> u64;
            export reuse: func(handle: borrow<counter>) -> u64;
            export expired: func() -> bool;
        }"#,
        r#"(ns app)
        (def retained (atom nil))
        (defn rejected? [f] (try (f) false (catch :default error true)))
        (defn verify [ok] (when-not ok (throw "borrow assertion failed")))
        (defn save [handle]
          (reset! retained handle)
          (verify (rejected? (fn [] (suss.resource/close! handle))))
          (verify (rejected? (fn [] (host/consume handle))))
          (host/read handle))
        (defn reuse [handle]
          (verify (rejected? (fn [] (host/read @retained))))
          (verify (rejected? (fn [] (suss.resource/close! @retained))))
          (verify (rejected? (fn [] (host/consume @retained))))
          (host/read handle))
        (defn expired []
          (and (rejected? (fn [] (host/read @retained)))
               (rejected? (fn [] (suss.resource/close! @retained)))
               (rejected? (fn [] (host/consume @retained)))))"#,
        &[
            ("save", "app/save"),
            ("reuse", "app/reuse"),
            ("expired", "app/expired"),
        ],
    )
}
fn component_with(wit_text: &str, app_source: &str, exports: &[(&str, &str)]) -> Component {
    component_with_imports(wit_text, app_source, exports, true)
}
fn component_with_imports(
    wit_text: &str,
    app_source: &str,
    exports: &[(&str, &str)],
    host_functions: bool,
) -> Component {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("app.sus");
    let wit = directory.path().join("api.wit");
    std::fs::write(&wit, wit_text).unwrap();
    std::fs::write(&source, app_source).unwrap();
    let bytes = compile_file_with_options(
        &source,
        &wit,
        Some("api"),
        &[],
        &AotOptions {
            imports: [
                (
                    "test:borrow-boundary/store@1.0.0#[constructor]counter",
                    "host/open",
                ),
                (
                    "test:borrow-boundary/store@1.0.0#[method]counter.read",
                    "host/read",
                ),
                ("test:borrow-boundary/store@1.0.0#consume", "host/consume"),
            ]
            .into_iter()
            .filter(|_| host_functions)
            .map(|(path, symbol)| (path.into(), Symbol::parse(symbol)))
            .collect(),
            exports: exports
                .iter()
                .map(|(name, symbol)| ((*name).into(), Symbol::parse(symbol)))
                .collect(),
            ..Default::default()
        },
    )
    .unwrap();
    if !host_functions {
        let imported: u32 = wasmparser::Parser::new(0)
            .parse_all(&bytes)
            .map(|payload| match payload.unwrap() {
                wasmparser::Payload::ComponentImportSection(section) => section.count(),
                _ => 0,
            })
            .sum();
        assert_eq!(
            imported, 1,
            "only the selected resource provider is imported"
        );
    }
    Component::new(engine(), bytes).unwrap()
}

#[test]
fn resource_only_declarations_install_lifetimes_without_any_handle_value_codecs() {
    let component = component_with_imports(
        r#"package test:borrow-boundary@1.0.0;
        interface store { resource counter; }
        world api { import store; export ping: func(value: u64) -> u64; }"#,
        r#"(ns app)
        (defn ping [value] (suss.resource/call-with-scope (fn [] value)))"#,
        &[("ping", "app/ping")],
        false,
    );
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    let instance = linker().instantiate(&mut store, &component).unwrap();
    let ping = instance
        .get_typed_func::<(u64,), (u64,)>(&mut store, "ping")
        .unwrap();
    for value in [0, 9007199254740993, u64::MAX] {
        assert_eq!(ping.call(&mut store, (value,)).unwrap().0, value);
        store.gc(None).unwrap();
    }
    assert_eq!(
        (
            store.data().constructors,
            store.data().reads,
            store.data().consumes,
            store.data().canonical_drops
        ),
        (0, 0, 0, 0)
    );
    assert_eq!(store.data().live.load(Ordering::SeqCst), 0);
}

#[test]
fn resource_only_provider_wires_destructors_without_host_function_mappings() {
    let component = component_with_imports(
        r#"package test:borrow-boundary@1.0.0;
        interface store { resource counter; }
        world api { import store; use store.{counter};
            export echo: func(handle: own<counter>) -> own<counter>;
            export release: func(handle: own<counter>);
            export accept: func(handle: borrow<counter>) -> bool;
            export rejected: func() -> bool;
        }"#,
        r#"(ns app) (def retained (atom nil))
        (defn rejected-close? [handle]
          (try (suss.resource/close! handle) false (catch :default error true)))
        (defn echo [handle] (reset! retained handle) handle)
        (defn release [handle]
          (reset! retained handle)
          (suss.resource/close! handle))
        (defn accept [handle]
          (reset! retained handle)
          (rejected-close? handle))
        (defn rejected [] (rejected-close? @retained))"#,
        &[
            ("echo", "app/echo"),
            ("release", "app/release"),
            ("accept", "app/accept"),
            ("rejected", "app/rejected"),
        ],
        false,
    );
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    let live = store.data().live.clone();
    let instance = linker().instantiate(&mut store, &component).unwrap();
    let echo = instance
        .get_typed_func::<(Resource<Counter>,), (Resource<Counter>,)>(&mut store, "echo")
        .unwrap();
    let release = instance
        .get_typed_func::<(Resource<Counter>,), ()>(&mut store, "release")
        .unwrap();
    let accept = instance
        .get_typed_func::<(Resource<Counter>,), (bool,)>(&mut store, "accept")
        .unwrap();
    let rejected = instance
        .get_typed_func::<(), (bool,)>(&mut store, "rejected")
        .unwrap();
    for value in [0, 9007199254740993, u64::MAX] {
        live.fetch_add(1, Ordering::SeqCst);
        let owner = store
            .data_mut()
            .table
            .push(Counter {
                value,
                live: live.clone(),
            })
            .unwrap();
        let rep = owner.rep();
        assert!(
            accept
                .call(&mut store, (Resource::<Counter>::new_borrow(rep),))
                .unwrap()
                .0
        );
        assert_eq!(live.load(Ordering::SeqCst), 1);
        let returned = echo.call(&mut store, (owner,)).unwrap().0;
        assert!(returned.owned());
        assert_eq!(returned.rep(), rep);
        assert_eq!(store.data().table.get(&returned).unwrap().value, value);
        assert!(rejected.call(&mut store, ()).unwrap().0);
        store.gc(None).unwrap();
        release.call(&mut store, (returned,)).unwrap();
        assert_eq!(live.load(Ordering::SeqCst), 0);
        assert!(rejected.call(&mut store, ()).unwrap().0);
    }
    assert_eq!(
        (
            store.data().constructors,
            store.data().reads,
            store.data().consumes,
            store.data().canonical_drops
        ),
        (0, 0, 0, 3)
    );
}

#[test]
fn exported_borrows_expire_after_return_and_stay_expired_during_later_calls() {
    let component = component();
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    let live = store.data().live.clone();
    let instance = linker().instantiate(&mut store, &component).unwrap();
    let save = instance
        .get_typed_func::<(Resource<Counter>,), (u64,)>(&mut store, "save")
        .unwrap();
    let reuse = instance
        .get_typed_func::<(Resource<Counter>,), (u64,)>(&mut store, "reuse")
        .unwrap();
    let expired = instance
        .get_typed_func::<(), (bool,)>(&mut store, "expired")
        .unwrap();
    for (index, value) in [0, 9007199254740993, u64::MAX].into_iter().enumerate() {
        live.fetch_add(1, Ordering::SeqCst);
        let owner = store
            .data_mut()
            .table
            .push(Counter {
                value,
                live: live.clone(),
            })
            .unwrap();
        assert!(owner.owned());
        if index != 0 {
            store.set_fuel(100_000_000).unwrap();
            assert_eq!(
                reuse
                    .call(&mut store, (Resource::<Counter>::new_borrow(owner.rep()),))
                    .unwrap()
                    .0,
                value
            );
        }
        store.set_fuel(100_000_000).unwrap();
        assert_eq!(
            save.call(&mut store, (Resource::<Counter>::new_borrow(owner.rep()),))
                .unwrap()
                .0,
            value
        );
        store.gc(None).unwrap();
        store.set_fuel(100_000_000).unwrap();
        assert!(expired.call(&mut store, ()).unwrap().0);
        store.set_fuel(100_000_000).unwrap();
        assert_eq!(
            reuse
                .call(&mut store, (Resource::<Counter>::new_borrow(owner.rep()),))
                .unwrap()
                .0,
            value
        );
        store.gc(None).unwrap();
        assert!(expired.call(&mut store, ()).unwrap().0);
        assert_eq!(live.load(Ordering::SeqCst), 1);
        assert_eq!(store.data().table.get(&owner).unwrap().value, value);
        assert_eq!(
            (
                store.data().constructors,
                store.data().consumes,
                store.data().canonical_drops
            ),
            (0, 0, 0)
        );
        store.data_mut().table.delete(owner).unwrap();
        assert_eq!(live.load(Ordering::SeqCst), 0);
    }
    assert_eq!(store.data().reads, 8);
}

#[derive(
    wasmtime::component::ComponentType, wasmtime::component::Lift, wasmtime::component::Lower,
)]
#[component(record)]
struct Request {
    first: Resource<Counter>,
    second: Resource<Counter>,
    echo: u64,
}
#[test]
fn exported_interfaces_reborrow_nested_records_and_preserve_pointer_float_and_void_results() {
    let component = component_with(
        r#"package test:borrow-boundary@1.0.0;
        interface store {
            resource counter { constructor(value: u64); read: func() -> u64; }
            consume: func(handle: own<counter>) -> u64;
        }
        interface consumer {
            use store.{counter};
            record request { first: borrow<counter>, second: borrow<counter>, echo: u64 }
            process: func(item: request) -> tuple<u64, u64>;
            echoed: func(handle: borrow<counter>, value: f64) -> f64;
            touched: func(handle: borrow<counter>);
        }
        world api { import store; export consumer; export expired: func() -> bool; }"#,
        r#"(ns app)
        (def retained (atom nil))
        (defn process [item]
          (let [a (:first item) b (:second item)]
            (reset! retained [a b])
            (if (= (host/read a) (host/read b))
              [(:echo item) (host/read a)]
              (throw "borrowed record identity mismatch"))))
        (defn echoed [handle value] (host/read handle) (reset! retained [handle]) value)
        (defn touched [handle] (host/read handle) (reset! retained [handle]) nil)
        (defn expired [] (every? (fn [handle] (try (host/read handle) false (catch :default error true))) @retained))"#,
        &[
            ("test:borrow-boundary/consumer@1.0.0#process", "app/process"),
            ("test:borrow-boundary/consumer@1.0.0#echoed", "app/echoed"),
            ("test:borrow-boundary/consumer@1.0.0#touched", "app/touched"),
            ("expired", "app/expired"),
        ],
    );
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    let live = store.data().live.clone();
    live.fetch_add(1, Ordering::SeqCst);
    let owner = store
        .data_mut()
        .table
        .push(Counter {
            value: u64::MAX,
            live: live.clone(),
        })
        .unwrap();
    let instance = linker().instantiate(&mut store, &component).unwrap();
    let (_, consumer) = instance
        .get_export(&mut store, None, "test:borrow-boundary/consumer@1.0.0")
        .unwrap();
    let (_, process) = instance
        .get_export(&mut store, Some(&consumer), "process")
        .unwrap();
    let (_, echoed) = instance
        .get_export(&mut store, Some(&consumer), "echoed")
        .unwrap();
    let (_, touched) = instance
        .get_export(&mut store, Some(&consumer), "touched")
        .unwrap();
    let process = instance
        .get_typed_func::<(Request,), ((u64, u64),)>(&mut store, process)
        .unwrap();
    let echoed = instance
        .get_typed_func::<(Resource<Counter>, f64), (f64,)>(&mut store, echoed)
        .unwrap();
    let touched = instance
        .get_typed_func::<(Resource<Counter>,), ()>(&mut store, touched)
        .unwrap();
    let expired = instance
        .get_typed_func::<(), (bool,)>(&mut store, "expired")
        .unwrap();
    let request = Request {
        first: Resource::<Counter>::new_borrow(owner.rep()),
        second: Resource::<Counter>::new_borrow(owner.rep()),
        echo: 9007199254740993,
    };
    assert_eq!(
        process.call(&mut store, (request,)).unwrap().0,
        (9007199254740993, u64::MAX)
    );
    store.gc(None).unwrap();
    assert!(expired.call(&mut store, ()).unwrap().0);
    for value in [-0.0_f64, f64::MAX] {
        store.set_fuel(100_000_000).unwrap();
        assert_eq!(
            echoed
                .call(
                    &mut store,
                    (Resource::<Counter>::new_borrow(owner.rep()), value)
                )
                .unwrap()
                .0
                .to_bits(),
            value.to_bits()
        );
        store.gc(None).unwrap();
        assert!(expired.call(&mut store, ()).unwrap().0);
    }
    store.set_fuel(100_000_000).unwrap();
    touched
        .call(&mut store, (Resource::<Counter>::new_borrow(owner.rep()),))
        .unwrap();
    store.gc(None).unwrap();
    assert!(expired.call(&mut store, ()).unwrap().0);
    assert_eq!(
        (
            store.data().reads,
            store.data().constructors,
            store.data().consumes,
            store.data().canonical_drops
        ),
        (6, 0, 0, 0)
    );
    assert_eq!(live.load(Ordering::SeqCst), 1);
    store.data_mut().table.delete(owner).unwrap();
    assert_eq!(live.load(Ordering::SeqCst), 0);
}

#[test]
fn uncaught_borrow_export_errors_cross_the_component_barrier_without_closing_the_host_owner() {
    let component = component_with(
        r#"package test:borrow-boundary@1.0.0;
        interface store { resource counter { constructor(value: u64); read: func() -> u64; } consume: func(handle: own<counter>) -> u64; }
        world api { import store; use store.{counter}; export fail: func(handle: borrow<counter>) -> u64; }"#,
        "(ns app) (defn fail [handle] (host/read handle) (throw 17))",
        &[("fail", "app/fail")],
    );
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    let live = store.data().live.clone();
    live.fetch_add(1, Ordering::SeqCst);
    let owner = store
        .data_mut()
        .table
        .push(Counter {
            value: u64::MAX,
            live: live.clone(),
        })
        .unwrap();
    let instance = linker().instantiate(&mut store, &component).unwrap();
    let fail = instance
        .get_typed_func::<(Resource<Counter>,), (u64,)>(&mut store, "fail")
        .unwrap();
    let error = fail
        .call(&mut store, (Resource::<Counter>::new_borrow(owner.rep()),))
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<wasmtime::Trap>(),
        Some(&wasmtime::Trap::UncaughtException),
        "{error:#}"
    );
    // The pinned barrier traps, while the Store retains the original exception
    // for inspection. Retire this Store rather than resume the component.
    let exception = store
        .take_pending_exception()
        .expect("original language exception");
    let fields = exception.fields(&mut store).unwrap().collect::<Vec<_>>();
    assert_eq!(fields.len(), 1);
    assert_eq!(
        suss_compile::portable_session::runtime_number(&mut store, &fields[0]).unwrap(),
        Some(suss_compile::portable_session::RuntimeNumber::Long(17))
    );
    assert_eq!(
        (
            store.data().reads,
            store.data().constructors,
            store.data().consumes,
            store.data().canonical_drops
        ),
        (1, 0, 0, 0)
    );
    assert_eq!(live.load(Ordering::SeqCst), 1);
    assert_eq!(store.data().table.get(&owner).unwrap().value, u64::MAX);
    store.data_mut().table.delete(owner).unwrap();
    assert_eq!(live.load(Ordering::SeqCst), 0);
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

#[test]
fn ten_thousand_borrow_calls_and_typed_errors_release_entries_without_closing_the_host_owner() {
    let component = component_with(
        r#"package test:borrow-boundary@1.0.0;
        interface store { resource counter { constructor(value: u64); read: func() -> u64; } consume: func(handle: own<counter>) -> u64; }
        world api { import store; use store.{counter}; export run: func(handle: borrow<counter>, fail: bool) -> result<u64, string>; export expired: func() -> bool; }"#,
        r#"(ns app)
        (def retained (atom nil))
        (defn rejected? [f] (try (f) false (catch :default error true)))
        (defn run [handle fail]
          (when @retained
            (when-not (rejected? (fn [] (host/read @retained))) (throw "old borrow revived")))
          (reset! retained handle)
          (try
            (let [value (host/read handle)] (if fail (throw :borrow-failure) [:ok value]))
            (catch :default error (if (= error :borrow-failure) [:err "borrow-failure"] (throw error)))))
        (defn expired [] (rejected? (fn [] (host/read @retained))))"#,
        &[("run", "app/run"), ("expired", "app/expired")],
    );
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    store.limiter(|state| state);
    let live = store.data().live.clone();
    live.fetch_add(1, Ordering::SeqCst);
    let owner = store
        .data_mut()
        .table
        .push(Counter {
            value: 0,
            live: live.clone(),
        })
        .unwrap();
    let instance = linker().instantiate(&mut store, &component).unwrap();
    let run = instance
        .get_typed_func::<(Resource<Counter>, bool), (Result<u64, String>,)>(&mut store, "run")
        .unwrap();
    let expired = instance
        .get_typed_func::<(), (bool,)>(&mut store, "expired")
        .unwrap();
    for index in 0..10_000 {
        let value = [0, 9007199254740993, u64::MAX][index % 3];
        let failed = index % 2 != 0;
        store.data_mut().table.get_mut(&owner).unwrap().value = value;
        store.set_fuel(100_000_000).unwrap();
        assert_eq!(
            run.call(
                &mut store,
                (Resource::<Counter>::new_borrow(owner.rep()), failed)
            )
            .unwrap()
            .0,
            if failed {
                Err("borrow-failure".into())
            } else {
                Ok(value)
            }
        );
        assert!(expired.call(&mut store, ()).unwrap().0);
        assert_eq!(live.load(Ordering::SeqCst), 1);
        if index % 100 == 0 {
            store.gc(None).unwrap();
        }
    }
    store.gc(None).unwrap();
    assert_eq!(
        (
            store.data().reads,
            store.data().constructors,
            store.data().consumes,
            store.data().canonical_drops
        ),
        (10_000, 0, 0, 0)
    );
    assert_eq!(store.data().canonical_peak, 65536);
    store.data_mut().table.delete(owner).unwrap();
    assert_eq!(live.load(Ordering::SeqCst), 0);
}

#[test]
fn owned_export_results_move_to_rust_preserve_identity_and_invalidate_old_source_aliases() {
    let component = component_with(
        r#"package test:borrow-boundary@1.0.0;
        interface store { resource counter { constructor(value: u64); read: func() -> u64; } consume: func(handle: own<counter>) -> u64; }
        world api { import store; use store.{counter}; export create: func(value: u64) -> own<counter>; export moved: func() -> bool; }"#,
        r#"(ns app) (def retained (atom nil))
        (defn create [value] (let [handle (host/open value)] (reset! retained handle) handle))
        (defn moved []
          (and (try (host/read @retained) false (catch :default error true))
               (try (suss.resource/close! @retained) false (catch :default error true))))"#,
        &[("create", "app/create"), ("moved", "app/moved")],
    );
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    let live = store.data().live.clone();
    let instance = linker().instantiate(&mut store, &component).unwrap();
    let create = instance
        .get_typed_func::<(u64,), (Resource<Counter>,)>(&mut store, "create")
        .unwrap();
    let moved = instance
        .get_typed_func::<(), (bool,)>(&mut store, "moved")
        .unwrap();
    for value in [0, 9007199254740993, u64::MAX] {
        store.set_fuel(100_000_000).unwrap();
        let owner = create.call(&mut store, (value,)).unwrap().0;
        assert!(owner.owned());
        assert_eq!(Some(owner.rep()), store.data().last_constructed_rep);
        assert_eq!(store.data().table.get(&owner).unwrap().value, value);
        assert_eq!(live.load(Ordering::SeqCst), 1);
        store.gc(None).unwrap();
        assert!(moved.call(&mut store, ()).unwrap().0);
        assert_eq!((store.data().reads, store.data().canonical_drops), (0, 0));
        store.data_mut().table.delete(owner).unwrap();
        assert_eq!(live.load(Ordering::SeqCst), 0);
    }
    assert_eq!(
        (
            store.data().constructors,
            store.data().consumes,
            store.data().canonical_drops
        ),
        (3, 0, 0)
    );
}

#[test]
fn owned_result_lists_commit_after_borrow_cleanup_and_duplicate_owners_fail_before_handoff() {
    let component = component_with(
        r#"package test:borrow-boundary@1.0.0;
        interface store { resource counter { constructor(value: u64); read: func() -> u64; } consume: func(handle: own<counter>) -> u64; }
        world api { import store; use store.{counter}; export create: func(parent: borrow<counter>, duplicate: bool) -> list<own<counter>>; export moved: func() -> bool; export expired: func() -> bool; }"#,
        r#"(ns app) (def retained (atom nil)) (def parent-saved (atom nil))
        (defn rejected? [f] (try (f) false (catch :default error true)))
        (defn create [parent duplicate]
          (reset! parent-saved parent)
          (let [value (host/read parent) a (host/open value) b (host/open value)]
            (reset! retained [a b]) (if duplicate [a a] [a b])))
        (defn moved [] (every? (fn [handle] (and (rejected? (fn [] (host/read handle))) (rejected? (fn [] (suss.resource/close! handle))))) @retained))
        (defn expired [] (rejected? (fn [] (host/read @parent-saved))))"#,
        &[
            ("create", "app/create"),
            ("moved", "app/moved"),
            ("expired", "app/expired"),
        ],
    );
    for duplicate in [false, true] {
        let mut store = Store::new(engine(), State::default());
        store.set_fuel(100_000_000).unwrap();
        let live = store.data().live.clone();
        live.fetch_add(1, Ordering::SeqCst);
        let parent = store
            .data_mut()
            .table
            .push(Counter {
                value: u64::MAX,
                live: live.clone(),
            })
            .unwrap();
        let instance = linker().instantiate(&mut store, &component).unwrap();
        let create = instance
            .get_typed_func::<(Resource<Counter>, bool), (Vec<Resource<Counter>>,)>(
                &mut store, "create",
            )
            .unwrap();
        let result = create.call(
            &mut store,
            (Resource::<Counter>::new_borrow(parent.rep()), duplicate),
        );
        assert_eq!(
            (
                store.data().constructors,
                store.data().reads,
                store.data().canonical_drops
            ),
            (2, 1, 0)
        );
        assert_eq!(live.load(Ordering::SeqCst), 3);
        if duplicate {
            let error = result.unwrap_err();
            assert_eq!(
                error.downcast_ref::<wasmtime::Trap>(),
                Some(&wasmtime::Trap::UncaughtException),
                "{error:#}"
            );
            assert!(store.take_pending_exception().is_some());
            // No ownership was lifted to the host. Retiring the trapped Store
            // owns destruction of the still-resident table entries.
            drop(store);
            assert_eq!(live.load(Ordering::SeqCst), 0);
        } else {
            let owners = result.unwrap().0;
            assert_eq!(owners.len(), 2);
            assert_ne!(owners[0].rep(), owners[1].rep());
            assert_eq!(Some(owners[1].rep()), store.data().last_constructed_rep);
            for owner in &owners {
                assert!(owner.owned());
                assert_eq!(store.data().table.get(owner).unwrap().value, u64::MAX);
            }
            store.gc(None).unwrap();
            let moved = instance
                .get_typed_func::<(), (bool,)>(&mut store, "moved")
                .unwrap();
            let expired = instance
                .get_typed_func::<(), (bool,)>(&mut store, "expired")
                .unwrap();
            assert!(moved.call(&mut store, ()).unwrap().0);
            assert!(expired.call(&mut store, ()).unwrap().0);
            for owner in owners {
                store.data_mut().table.delete(owner).unwrap();
            }
            assert_eq!(live.load(Ordering::SeqCst), 1);
            store.data_mut().table.delete(parent).unwrap();
            assert_eq!(live.load(Ordering::SeqCst), 0);
        }
    }
}

#[test]
fn ten_thousand_owned_export_results_transfer_identity_without_lingering_guest_owners() {
    let component = component_with(
        r#"package test:borrow-boundary@1.0.0;
        interface store { resource counter { constructor(value: u64); read: func() -> u64; } consume: func(handle: own<counter>) -> u64; }
        world api { import store; use store.{counter}; export create: func(value: u64) -> own<counter>; export moved: func() -> bool; }"#,
        r#"(ns app) (def retained (atom nil))
        (defn create [value] (let [handle (host/open value)] (reset! retained handle) handle))
        (defn moved [] (and (try (host/read @retained) false (catch :default error true)) (try (suss.resource/close! @retained) false (catch :default error true))))"#,
        &[("create", "app/create"), ("moved", "app/moved")],
    );
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    store.limiter(|state| state);
    let live = store.data().live.clone();
    let instance = linker().instantiate(&mut store, &component).unwrap();
    let create = instance
        .get_typed_func::<(u64,), (Resource<Counter>,)>(&mut store, "create")
        .unwrap();
    let moved = instance
        .get_typed_func::<(), (bool,)>(&mut store, "moved")
        .unwrap();
    for index in 0..10_000 {
        let value = [0, 9007199254740993, u64::MAX][index % 3];
        store.set_fuel(100_000_000).unwrap();
        let owner = create.call(&mut store, (value,)).unwrap().0;
        assert!(owner.owned());
        assert_eq!(Some(owner.rep()), store.data().last_constructed_rep);
        assert_eq!(store.data().table.get(&owner).unwrap().value, value);
        assert_eq!(live.load(Ordering::SeqCst), 1);
        assert!(moved.call(&mut store, ()).unwrap().0);
        store.data_mut().table.delete(owner).unwrap();
        assert_eq!(live.load(Ordering::SeqCst), 0);
        if index % 100 == 0 {
            store.gc(None).unwrap();
        }
    }
    store.gc(None).unwrap();
    assert_eq!(
        (
            store.data().constructors,
            store.data().reads,
            store.data().consumes,
            store.data().canonical_drops
        ),
        (10_000, 0, 0, 0)
    );
    assert_eq!(store.data().canonical_peak, 65536);
}

#[test]
fn owned_export_inputs_can_be_retained_across_calls_until_guest_explicit_close() {
    let component = component_with(
        r#"package test:borrow-boundary@1.0.0;
        interface store { resource counter { constructor(value: u64); read: func() -> u64; } consume: func(handle: own<counter>) -> u64; }
        world api { import store; use store.{counter}; export save: func(handle: own<counter>) -> u64; export read: func() -> u64; export close: func(); export closed: func() -> bool; }"#,
        r#"(ns app) (def retained (atom nil))
        (defn save [handle] (reset! retained handle) (host/read handle))
        (defn read [] (host/read @retained))
        (defn close [] (suss.resource/close! @retained))
        (defn closed [] (and (try (host/read @retained) false (catch :default error true)) (try (suss.resource/close! @retained) false (catch :default error true))))"#,
        &[
            ("save", "app/save"),
            ("read", "app/read"),
            ("close", "app/close"),
            ("closed", "app/closed"),
        ],
    );
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    let live = store.data().live.clone();
    let instance = linker().instantiate(&mut store, &component).unwrap();
    let save = instance
        .get_typed_func::<(Resource<Counter>,), (u64,)>(&mut store, "save")
        .unwrap();
    let read = instance
        .get_typed_func::<(), (u64,)>(&mut store, "read")
        .unwrap();
    let close = instance
        .get_typed_func::<(), ()>(&mut store, "close")
        .unwrap();
    let closed = instance
        .get_typed_func::<(), (bool,)>(&mut store, "closed")
        .unwrap();
    for value in [0, 9007199254740993, u64::MAX] {
        live.fetch_add(1, Ordering::SeqCst);
        let owner = store
            .data_mut()
            .table
            .push(Counter {
                value,
                live: live.clone(),
            })
            .unwrap();
        let rep = owner.rep();
        store.set_fuel(100_000_000).unwrap();
        assert_eq!(save.call(&mut store, (owner,)).unwrap().0, value);
        for _ in 0..2 {
            store.gc(None).unwrap();
            assert_eq!(live.load(Ordering::SeqCst), 1);
            assert_eq!(
                store
                    .data()
                    .table
                    .get(&Resource::<Counter>::new_borrow(rep))
                    .unwrap()
                    .value,
                value
            );
            assert_eq!(read.call(&mut store, ()).unwrap().0, value);
        }
        close.call(&mut store, ()).unwrap();
        store.gc(None).unwrap();
        assert_eq!(live.load(Ordering::SeqCst), 0);
        assert!(closed.call(&mut store, ()).unwrap().0);
    }
    assert_eq!(
        (
            store.data().reads,
            store.data().constructors,
            store.data().consumes,
            store.data().canonical_drops
        ),
        (9, 0, 0, 3)
    );
}

#[test]
fn owned_inputs_roundtrip_to_rust_and_close_through_source_finally_on_success_or_error() {
    let component = component_with(
        r#"package test:borrow-boundary@1.0.0;
        interface store { resource counter { constructor(value: u64); read: func() -> u64; } consume: func(handle: own<counter>) -> u64; }
        world api { import store; use store.{counter}; export echo: func(handle: own<counter>) -> own<counter>; export use-value: func(handle: own<counter>, failed: bool) -> result<u64, string>; export moved: func() -> bool; }"#,
        r#"(ns app) (def retained (atom nil))
        (defn echo [handle] (reset! retained handle) handle)
        (defn moved [] (and (try (host/read @retained) false (catch :default error true)) (try (suss.resource/close! @retained) false (catch :default error true))))
        (defn use-value [handle failed]
          (try (try (let [value (host/read handle)] (if failed (throw :owned-failure) [:ok value]))
                    (finally (suss.resource/close! handle)))
               (catch :default error (if (= error :owned-failure) [:err "owned-failure"] (throw error)))))"#,
        &[
            ("echo", "app/echo"),
            ("use-value", "app/use-value"),
            ("moved", "app/moved"),
        ],
    );
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    let live = store.data().live.clone();
    let instance = linker().instantiate(&mut store, &component).unwrap();
    let echo = instance
        .get_typed_func::<(Resource<Counter>,), (Resource<Counter>,)>(&mut store, "echo")
        .unwrap();
    let use_value = instance
        .get_typed_func::<(Resource<Counter>, bool), (Result<u64, String>,)>(
            &mut store,
            "use-value",
        )
        .unwrap();
    let moved = instance
        .get_typed_func::<(), (bool,)>(&mut store, "moved")
        .unwrap();
    for value in [0, 9007199254740993, u64::MAX] {
        live.fetch_add(1, Ordering::SeqCst);
        let owner = store
            .data_mut()
            .table
            .push(Counter {
                value,
                live: live.clone(),
            })
            .unwrap();
        let rep = owner.rep();
        store.set_fuel(100_000_000).unwrap();
        let returned = echo.call(&mut store, (owner,)).unwrap().0;
        assert!(returned.owned());
        assert_eq!(returned.rep(), rep);
        assert_eq!(store.data().table.get(&returned).unwrap().value, value);
        assert!(moved.call(&mut store, ()).unwrap().0);
        store.data_mut().table.delete(returned).unwrap();
        assert_eq!(live.load(Ordering::SeqCst), 0);
        for failed in [false, true] {
            live.fetch_add(1, Ordering::SeqCst);
            let owner = store
                .data_mut()
                .table
                .push(Counter {
                    value,
                    live: live.clone(),
                })
                .unwrap();
            store.set_fuel(100_000_000).unwrap();
            assert_eq!(
                use_value.call(&mut store, (owner, failed)).unwrap().0,
                if failed {
                    Err("owned-failure".into())
                } else {
                    Ok(value)
                }
            );
            assert_eq!(live.load(Ordering::SeqCst), 0);
            store.gc(None).unwrap();
        }
    }
    assert_eq!(
        (
            store.data().reads,
            store.data().constructors,
            store.data().consumes,
            store.data().canonical_drops
        ),
        (6, 0, 0, 6)
    );
}

#[test]
fn resource_maps_reject_handle_values_before_source_macro_effects() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("app.sus");
    let wit = directory.path().join("api.wit");
    std::fs::write(
        &source,
        r#"(ns app) (defmacro effect [] (throw "macro-ran")) (defn run [entries] (effect))"#,
    )
    .unwrap();
    for value in [
        "own<counter>",
        "borrow<counter>",
        "list<own<counter>>",
        "option<own<counter>>",
        "tuple<u64, own<counter>>",
    ] {
        std::fs::write(&wit, format!("package test:resource-maps; interface store {{ resource counter; }} world api {{ import store; use store.{{counter}}; type table = map<string, {value}>; export run: func(entries: table) -> u64; }}")).unwrap();
        let error = compile_file_with_options(
            &source,
            &wit,
            Some("api"),
            &[],
            &AotOptions {
                exports: vec![("run".into(), Symbol::parse("app/run"))],
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(
            error.contains("map values cannot contain resource handles"),
            "{value}: {error}"
        );
        assert!(!error.contains("macro-ran"), "{value}: {error}");
    }
}
