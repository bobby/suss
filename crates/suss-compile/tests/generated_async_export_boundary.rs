//! Generated canonical transport plus actual compiled source: no WAT guest.
use std::{
    future::Future,
    pin::pin,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
};
use suss_compile::portable::{
    self,
    command::async_component,
    resolve::{Environment, Phase},
};
use wasmtime::{
    Config, Engine, Store,
    component::{Component, Linker},
};
struct WakeThread(std::thread::Thread);
impl Wake for WakeThread {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}
fn drive<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let waker = Waker::from(Arc::new(WakeThread(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "generated transport stalled"
                );
                std::thread::park_timeout(std::time::Duration::from_millis(5));
            }
        }
    }
}
fn generated_component(source: &str) -> Vec<u8> {
    let mut resolve = wit_parser::Resolve::new();
    let package = resolve.push_str("bridge.wit", "package test:source-callback; world api { import increment: async func(value: u32) -> u32; export run: async func(value: u32) -> u32; }").unwrap();
    let world = resolve.select_world(&[package], Some("api")).unwrap();
    let import = resolve.worlds[world]
        .imports
        .values()
        .find_map(|item| match item {
            wit_parser::WorldItem::Function(f) => Some(f),
            _ => None,
        })
        .unwrap();
    let export = resolve.worlds[world]
        .exports
        .values()
        .find_map(|item| match item {
            wit_parser::WorldItem::Function(f) => Some(f),
            _ => None,
        })
        .unwrap();
    let mut env = Environment::default();
    let cell = env
        .declare_cell(Phase::Runtime, "host", "increment")
        .unwrap();
    let fragment = portable::prepare_fragment(source, &env, Phase::Runtime).unwrap();
    let main = fragment
        .environment
        .resolve(
            Phase::Runtime,
            &suss_reader::Symbol::namespaced("app", "run"),
            0..0,
        )
        .unwrap()
        .global()
        .clone();
    async_component::component(
        &[fragment],
        &resolve,
        async_component::ImportMapping {
            function: import,
            global: &cell,
        },
        async_component::ExportMapping {
            function: export,
            global: &main,
        },
    )
    .unwrap()
}
#[test]
fn async_u32_export_rejects_invalid_numbers_and_preserves_endpoints() {
    let mut config = Config::new();
    config
        .wasm_gc(true)
        .wasm_function_references(true)
        .wasm_tail_call(true)
        .wasm_exceptions(true)
        .wasm_component_model_async(true)
        .wasm_component_model_async_stackful(false)
        .wasm_component_model_more_async_builtins(true)
        .consume_fuel(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let engine = Engine::new(&config).unwrap();
    for (literal, expected) in [
        ("0", Some(0)),
        ("4294967295", Some(u32::MAX)),
        ("1.5", None),
        ("-1", None),
        ("4294967296", None),
        ("##NaN", None),
        ("##Inf", None),
        ("##-Inf", None),
    ] {
        let source = format!("(ns app) (def run (fn [x] (suss.async/future* {literal})))");
        let component = Component::new(&engine, generated_component(&source)).unwrap();
        let mut linker = Linker::new(&engine);
        linker
            .root()
            .func_new_concurrent("increment", |_, _, _, _| {
                Box::pin(async { panic!("unused host import must not execute") })
            })
            .unwrap();
        let mut store = Store::new(&engine, ());
        store.set_fuel(2_000_000).unwrap();
        let instance = drive(linker.instantiate_async(&mut store, &component)).unwrap();
        let run = instance
            .get_typed_func::<(u32,), (u32,)>(&mut store, "run")
            .unwrap();
        let output =
            drive(store.run_concurrent(async |accessor| run.call_concurrent(accessor, (0,)).await))
                .and_then(|output| output);
        match expected {
            Some(value) => assert_eq!(output.unwrap(), (value,), "literal {literal}"),
            None => {
                let error = output.expect_err("invalid result must not truncate or wrap");
                assert!(
                    error.downcast_ref::<wasmtime::Trap>().is_some(),
                    "expected boundary trap for {literal}: {error:#}"
                );
            }
        }
        store.gc(None).unwrap();
    }
}
