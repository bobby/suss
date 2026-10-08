#![cfg(unix)]
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
// Generated canonical transport plus actual compiled source: no WAT guest.
use std::{
    future::Future,
    pin::pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};
use suss_compile::portable::{self, command::async_component, resolve::Phase};
use wasmtime::{
    Config, Engine, Store,
    component::{Component, Linker, Val},
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
    generated_fragments(&[source])
}
fn generated_fragments(sources: &[&str]) -> Vec<u8> {
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
    // The shipped async library includes source-backed collection wrappers.
    // Prepare it in the real Runtime core environment, and execute that core
    // initializer first, just as the public AOT source preparation does.
    let mut core = portable::bootstrap::shipped(Phase::Runtime)
        .unwrap()
        .clone();
    let cell = core
        .environment
        .declare_cell(Phase::Runtime, "host", "increment")
        .unwrap();
    core.cells.push(cell.clone());
    let mut env = core.environment.clone();
    let mut fragments = vec![core];
    for source in sources {
        let fragment = portable::prepare_fragment(source, &env, Phase::Runtime).unwrap();
        env = fragment.environment.clone();
        fragments.push(fragment);
    }
    let main = env
        .resolve(
            Phase::Runtime,
            &suss_reader::Symbol::namespaced("app", "run"),
            0..0,
        )
        .unwrap()
        .global()
        .clone();
    async_component::component(
        &fragments,
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
fn source_cancel_closes_polled_pending_io_before_component_return() {
    let app = "(ns app) (def effects 0) (def run (fn [x] (suss.async/future* (let [read (host/increment 0) gate (host/increment 1)] (suss.async/await* gate) (suss.async/cancel! read) (try (suss.async/await* read) (catch :default error 42) (finally (set! effects (+ effects 1))))))))";
    let bytes = generated_fragments(&[include_str!("../src/portable/stdlib/async.sus"), app]);
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
    let component = Component::new(&engine, bytes).unwrap();
    let (reader, mut peer) = UnixStream::pair().unwrap();
    reader.set_nonblocking(true).unwrap();
    let polls = Arc::new(AtomicUsize::new(0));
    let read_polls = polls.clone();
    let reader = std::sync::Mutex::new(Some(reader));
    let mut linker = Linker::new(&engine);
    linker
        .root()
        .func_new_concurrent("increment", move |_, _, params, results| {
            let polls = read_polls.clone();
            let [Val::U32(kind)] = params else {
                panic!("scalar input")
            };
            // Move the sole read handle into the pending operation.
            let mut owned = if *kind == 0 {
                Some(reader.lock().unwrap().take().expect("one read"))
            } else {
                None
            };
            Box::pin(async move {
                if let Some(mut socket) = owned.take() {
                    std::future::poll_fn(|_| {
                        let mut byte = [0];
                        match socket.read(&mut byte) {
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                                polls.fetch_add(1, Ordering::SeqCst);
                                Poll::<()>::Pending
                            }
                            other => panic!("read must remain pending until cancelled: {other:?}"),
                        }
                    })
                    .await;
                    unreachable!("read cannot complete")
                } else {
                    std::future::poll_fn(|cx| {
                        if polls.load(Ordering::SeqCst) > 0 {
                            Poll::Ready(())
                        } else {
                            cx.waker().wake_by_ref();
                            Poll::<()>::Pending
                        }
                    })
                    .await;
                    results[0] = Val::U32(0);
                    Ok(())
                }
            })
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
            .unwrap()
            .unwrap();
    assert_eq!(output, (42,));
    assert!(
        polls.load(Ordering::SeqCst) > 0,
        "a real nonblocking read returned WouldBlock before source cancellation"
    );
    // Store and connector remain alive: only cancelling the pending read
    // can close the peer's sole reader.
    let error = peer
        .write_all(b"late completion")
        .expect_err("pending read handle must be closed before Store drop");
    assert!(
        matches!(
            error.kind(),
            std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
        ),
        "{error}"
    );
    store.gc(None).unwrap();
}
