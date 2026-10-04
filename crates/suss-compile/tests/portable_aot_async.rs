//! Execute asynchronous canonical exports over ordinary compiled source bodies.
use std::{
    future::Future,
    pin::pin,
    sync::{Arc, OnceLock},
    task::{Context, Poll, Wake, Waker},
};
use suss_compile::portable::{
    self,
    resolve::{Environment, Phase},
};
use suss_reader::Symbol;
use wasmtime::component::{Component, Linker};
use wasmtime::{AsContextMut, Config, Engine, Store};

struct ThreadWake(std::thread::Thread);
impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}
fn block_on<T>(future: impl Future<Output = T>) -> T {
    let mut future = pin!(future);
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "asynchronous call exceeded bounded test deadline"
                );
                std::thread::park_timeout(std::time::Duration::from_millis(100));
            }
        }
    }
}
fn engine() -> Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut config = Config::new();
            config
                .wasm_gc(true)
                .wasm_function_references(true)
                .wasm_tail_call(true)
                .wasm_exceptions(true)
                .wasm_component_model(true)
                .wasm_component_model_async(true)
                .wasm_component_model_implements(true)
                .consume_fuel(true)
                .cranelift_opt_level(wasmtime::OptLevel::None);
            Engine::new(&config).unwrap()
        })
        .clone()
}
#[test]
fn async_exports_complete_once_and_share_live_cells_with_sync_exports_after_gc() {
    let first = portable::prepare_fragment(
        "(ns app) (def effects 0) (def f (fn [x] (do (set! effects (+ effects 1)) (+ x 1)))) (def old f)",
        &Environment::default(), Phase::Runtime).unwrap();
    let second = portable::prepare_fragment(
        "(def f (fn [x] (+ x 100))) (def captured (fn [x] (old x))) (def seen (fn [] effects)) (def invert (fn [x] (if x false true))) (def bump (fn [] (set! effects (+ effects 10)))) (def echo32 (fn [x] x)) (set! effects (+ effects 1))",
        &first.environment, Phase::Runtime).unwrap();
    let mut resolve = portable::aot::Resolve::new();
    let package = resolve.push_str("api.wit", "package test:async-exports; interface calls { current: async func(x: f64) -> f64; invert: async func(x: bool) -> bool; } world api { export captured: async func(x: f64) -> f64; export seen: func() -> f64; export bump: async func(); export callback: async func(x: f32) -> f32; export calls; }").unwrap();
    let world = resolve.select_world(&[package], Some("api")).unwrap();
    let mappings = [
        ("captured", "captured"),
        ("seen", "seen"),
        ("bump", "bump"),
        ("callback", "echo32"),
        ("test:async-exports/calls#current", "f"),
        ("test:async-exports/calls#invert", "invert"),
    ]
    .into_iter()
    .map(|(path, var)| (path.to_owned(), Symbol::namespaced("app", var)))
    .collect::<Vec<_>>();
    let bytes = portable::aot::component(&[first, second], &resolve, world, &mappings).unwrap();
    let imports: u32 = wasmparser::Parser::new(0)
        .parse_all(&bytes)
        .map(|payload| match payload.unwrap() {
            wasmparser::Payload::ComponentImportSection(section) => section.count(),
            _ => 0,
        })
        .sum();
    assert_eq!(
        imports, 0,
        "async canonical intrinsics must not add hidden host imports"
    );
    let engine = engine();
    let component = Component::new(&engine, &bytes).unwrap();
    for _ in 0..2 {
        let mut store = Store::new(&engine, ());
        store.set_fuel(20_000_000).unwrap();
        let instance = Linker::new(&engine)
            .instantiate(&mut store, &component)
            .unwrap();
        let captured = instance
            .get_typed_func::<(f64,), (f64,)>(&mut store, "captured")
            .unwrap();
        let seen = instance
            .get_typed_func::<(), (f64,)>(&mut store, "seen")
            .unwrap();
        let bump = instance
            .get_typed_func::<(), ()>(&mut store, "bump")
            .unwrap();
        let current_index = instance
            .get_export_index(&mut store, None, "test:async-exports/calls")
            .unwrap();
        let current_index = instance
            .get_export_index(&mut store, Some(&current_index), "current")
            .unwrap();
        let current = instance
            .get_typed_func::<(f64,), (f64,)>(&mut store, &current_index)
            .unwrap();
        assert!(captured.func().ty(&store).async_());
        assert!(current.func().ty(&store).async_());
        assert!(bump.func().ty(&store).async_());
        assert!(!seen.func().ty(&store).async_());
        assert_eq!(seen.call(&mut store, ()).unwrap().0, 1.0);
        seen.post_return(&mut store).unwrap();
        assert_eq!(
            block_on(captured.call_async(&mut store, (41.0,)))
                .unwrap()
                .0,
            42.0
        );
        assert_eq!(
            block_on(current.call_async(&mut store, (41.0,))).unwrap().0,
            141.0
        );
        let callback = instance
            .get_typed_func::<(f32,), (f32,)>(&mut store, "callback")
            .unwrap();
        for value in [1.25_f32, -0.0_f32, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                block_on(callback.call_async(&mut store, (value,)))
                    .unwrap()
                    .0
                    .to_bits(),
                value.to_bits()
            );
        }
        store.gc(None).unwrap();
        block_on(bump.call_async(&mut store, ())).unwrap();
        assert_eq!(
            block_on(captured.call_async(&mut store, (5.0,))).unwrap().0,
            6.0
        );
        assert_eq!(seen.call(&mut store, ()).unwrap().0, 13.0);
        seen.post_return(&mut store).unwrap();
        let interface = instance
            .get_export_index(&mut store, None, "test:async-exports/calls")
            .unwrap();
        let invert_index = instance
            .get_export_index(&mut store, Some(&interface), "invert")
            .unwrap();
        let invert = instance
            .get_typed_func::<(bool,), (bool,)>(&mut store, &invert_index)
            .unwrap();
        assert_eq!(
            block_on(invert.call_async(&mut store, (false,))).unwrap(),
            (true,)
        );
        assert_eq!(
            block_on(invert.call_async(&mut store, (true,))).unwrap(),
            (false,)
        );
    }
}

#[test]
fn async_errors_preserve_boundary_and_source_language_payloads() {
    for (source, result_type, expected) in [
        ("(def fail (fn [] false))", " -> f64", None),
        ("(def fail (fn [] (throw 17)))", "", Some(17.0_f64)),
    ] {
        let fragment =
            portable::prepare_fragment(source, &Environment::default(), Phase::Runtime).unwrap();
        let mut resolve = portable::aot::Resolve::new();
        let package = resolve.push_str("api.wit", &format!("package test:async-errors; world api {{ export fail: async func(){result_type}; }}")).unwrap();
        let world = resolve.select_world(&[package], Some("api")).unwrap();
        let bytes = portable::aot::component(
            &[fragment],
            &resolve,
            world,
            &[("fail".to_owned(), Symbol::new("fail"))],
        )
        .unwrap();
        let engine = engine();
        let component = Component::new(&engine, bytes).unwrap();
        for _ in 0..2 {
            let mut store = Store::new(&engine, ());
            store.set_fuel(20_000_000).unwrap();
            let instance = Linker::new(&engine)
                .instantiate(&mut store, &component)
                .unwrap();
            let function = instance.get_func(&mut store, "fail").unwrap();
            let mut results = if expected.is_none() {
                vec![wasmtime::component::Val::Float64(0.0)]
            } else {
                vec![]
            };
            assert!(block_on(function.call_async(&mut store, &[], &mut results)).is_err());
            let exception = store
                .as_context_mut()
                .take_pending_exception()
                .expect("actual language exception payload");
            let fields = exception.fields(&mut store).unwrap().collect::<Vec<_>>();
            assert_eq!(fields.len(), 1);
            let payload = fields[0]
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)
                .unwrap()
                .unwrap();
            if let Some(expected) = expected {
                assert_eq!(
                    payload.field(&mut store, 0).unwrap().unwrap_f64().to_bits(),
                    expected.to_bits()
                );
            } else {
                let message = payload.field(&mut store, 1).unwrap();
                let array = message
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)
                    .unwrap()
                    .unwrap();
                let units = array
                    .elems(&mut store)
                    .unwrap()
                    .map(|unit| unit.unwrap_i32() as u16)
                    .collect::<Vec<_>>();
                assert_eq!(
                    String::from_utf16(&units).unwrap(),
                    "WIT export fail returned an incompatible scalar value"
                );
            }
        }
    }
}
