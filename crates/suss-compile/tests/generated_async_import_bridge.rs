//! Generated canonical transport plus actual compiled source: no WAT guest.
use std::{
    future::Future,
    pin::pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};
use suss_compile::portable::{
    self,
    command::async_component,
    resolve::{Environment, Phase},
};
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
    let mut env = Environment::default();
    let cell = env
        .declare_cell(Phase::Runtime, "host", "increment")
        .unwrap();
    let mut fragments = Vec::new();
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
fn generated_scalar_import_resumes_actual_source_future_and_releases_each_transfer() {
    let bytes = generated_component(
        "(ns app) (def run (fn [x] (suss.async/future* (+ (suss.async/await* (host/increment x)) 1))))",
    );
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
    let mut linker = Linker::new(&engine);
    let polls = Arc::new(AtomicUsize::new(0));
    let count = polls.clone();
    linker
        .root()
        .func_new_concurrent("increment", move |_, _, params, results| {
            let count = count.clone();
            Box::pin(async move {
                let mut first = true;
                std::future::poll_fn(|cx| {
                    count.fetch_add(1, Ordering::SeqCst);
                    if first {
                        first = false;
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    } else {
                        Poll::Ready(())
                    }
                })
                .await;
                let [Val::U32(value)] = params else {
                    panic!("resolved scalar input")
                };
                results[0] = Val::U32(value + 1);
                Ok(())
            })
        })
        .unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(2_000_000).unwrap();
    let instance = drive(linker.instantiate_async(&mut store, &component)).unwrap();
    let run = instance
        .get_typed_func::<(u32,), (u32,)>(&mut store, "run")
        .unwrap();
    for input in [40, 17, 0, u32::MAX - 2] {
        let output = drive(
            store.run_concurrent(async |accessor| run.call_concurrent(accessor, (input,)).await),
        )
        .unwrap()
        .unwrap();
        assert_eq!(output, (input + 2,));
        store.gc(None).unwrap();
    }
    assert!(
        polls.load(Ordering::SeqCst) >= 8,
        "each generated import genuinely suspended"
    );
}

#[test]
fn source_completion_cancel_drains_canonical_host_operation_before_return() {
    // Compile the shipped public API source, rather than inventing a test stub.
    let app = "(ns app) (def trace 0) (def run (fn [x] (suss.async/future* (let [dependency (host/increment x)] (suss.async/cancel! dependency) (let [result (try (suss.async/await* dependency) (catch :default payload 42) (finally (set! trace (+ trace 1))))] (+ result trace))))))";
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
    struct OwnedOperation(Arc<AtomicUsize>);
    impl Drop for OwnedOperation {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let starts = Arc::new(AtomicUsize::new(0));
    let polls = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut linker = Linker::new(&engine);
    let (started, polled, dropped) = (starts.clone(), polls.clone(), drops.clone());
    linker
        .root()
        .func_new_concurrent("increment", move |_, _, _, _| {
            started.fetch_add(1, Ordering::SeqCst);
            // Own resources before first poll: cancellation before polling must also
            // release them. This operation cannot complete by returning a value.
            let operation = OwnedOperation(dropped.clone());
            let polled = polled.clone();
            Box::pin(async move {
                let _operation = operation;
                std::future::poll_fn(|_| {
                    polled.fetch_add(1, Ordering::SeqCst);
                    Poll::<()>::Pending
                })
                .await;
                unreachable!("pending operation must be cancelled")
            })
        })
        .unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(2_000_000).unwrap();
    let instance = drive(linker.instantiate_async(&mut store, &component)).unwrap();
    let run = instance
        .get_typed_func::<(u32,), (u32,)>(&mut store, "run")
        .unwrap();
    for round in 1..=3 {
        let output = drive(
            store.run_concurrent(async |accessor| run.call_concurrent(accessor, (17,)).await),
        )
        .unwrap()
        .unwrap();
        // Catchable cancellation and finally effects execute exactly once;
        // effects persist in the same source runtime across successive calls.
        assert_eq!(output, (42 + round,));
        assert_eq!(starts.load(Ordering::SeqCst), round as usize);
        assert_eq!(
            drops.load(Ordering::SeqCst),
            round as usize,
            "operation must be reclaimed before task.return, not Store drop"
        );
        store.gc(None).unwrap();
    }
    // Pending status is guaranteed by the producer; polling may be preempted by
    // cancellation, so no minimum poll count is required for that legitimate race.
    assert!(
        polls.load(Ordering::SeqCst) <= 100,
        "cancelled pending operations must not be polled indefinitely"
    );
}

#[test]
fn parent_return_drains_cancelled_child_finally_before_retiring_invocation() {
    let app = "(ns app) (def trace 0) (def run (fn [x] (suss.async/future* (if (== x 0) trace (let [child (suss.async/future* (try (set! trace 1) (suss.async/await* (host/increment 10)) (finally (suss.async/await* (host/increment 20)) (suss.async/await* (host/increment 30)) (set! trace (+ trace 1)))))] (loop [] (if (== trace 0) (recur) nil)) (suss.async/cancel! child) 42)))))";
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
    let starts = Arc::new(std::sync::Mutex::new(Vec::<u32>::new()));
    let drops = Arc::new(std::sync::Mutex::new(Vec::<u32>::new()));
    let completions = Arc::new(std::sync::Mutex::new(Vec::<u32>::new()));
    struct Operation {
        value: u32,
        drops: Arc<std::sync::Mutex<Vec<u32>>>,
    }
    impl Drop for Operation {
        fn drop(&mut self) {
            self.drops.lock().unwrap().push(self.value);
        }
    }
    let mut linker = Linker::new(&engine);
    let (started, dropped, completed) = (starts.clone(), drops.clone(), completions.clone());
    linker
        .root()
        .func_new_concurrent("increment", move |_, _, params, results| {
            let [Val::U32(value)] = params else {
                panic!("scalar operation selector")
            };
            let value = *value;
            started.lock().unwrap().push(value);
            let operation = Operation {
                value,
                drops: dropped.clone(),
            };
            let completed = completed.clone();
            Box::pin(async move {
                let _operation = operation;
                // Body I/O cannot complete. Finally I/O actually returns Pending
                // once, then succeeds; the final marker is observed only after it.
                let mut first = true;
                std::future::poll_fn(|cx| {
                    if value == 10 {
                        return Poll::<()>::Pending;
                    }
                    if value == 20 && first {
                        first = false;
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    } else {
                        Poll::Ready(())
                    }
                })
                .await;
                completed.lock().unwrap().push(value);
                results[0] = Val::U32(value);
                Ok(())
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
        drive(store.run_concurrent(async |accessor| run.call_concurrent(accessor, (1,)).await))
            .unwrap()
            .unwrap();
    assert_eq!(output, (42,));
    assert_eq!(
        *starts.lock().unwrap(),
        vec![10, 20, 30],
        "child finally must run through its pending cleanup before parent task.return"
    );
    assert_eq!(
        *completions.lock().unwrap(),
        vec![20, 30],
        "cleanup succeeds; the cancelled body operation cannot supply a late value"
    );
    let mut released = drops.lock().unwrap().clone();
    released.sort_unstable();
    assert_eq!(
        released,
        vec![10, 20, 30],
        "each host operation is reclaimed exactly once before invocation retirement"
    );
    store.gc(None).unwrap();
    let output =
        drive(store.run_concurrent(async |accessor| run.call_concurrent(accessor, (0,)).await))
            .unwrap()
            .unwrap();
    assert_eq!(
        output,
        (2,),
        "finally effects complete once in their original invocation"
    );
    assert_eq!(
        *starts.lock().unwrap(),
        vec![10, 20, 30],
        "retired child work cannot resume in the next canonical context"
    );
}

#[test]
fn retirement_cancel_wave_drains_unawaited_child_spawned_in_finally() {
    // An awaited cleanup child must survive the first cancellation wave. The
    // unawaited child created by finally needs a later wave after its cancelled
    // owner has finished; otherwise invocation retirement waits forever.
    let app = "(ns app) (def trace 0) (def run (fn [x] (suss.async/future* (if (== x 0) trace (let [child (suss.async/future* (try (set! trace 1) (suss.async/await* (host/increment 10)) (finally (suss.async/future* (try (set! trace 10) (suss.async/await* (suss.async/completion)) (finally (set! trace (+ trace 100))))) (loop [] (if (== trace 1) (recur) nil)) (suss.async/await* (suss.async/future* (do (suss.async/await* (host/increment 20)) (suss.async/await* (host/increment 30))))) (set! trace (+ trace 1)))))] (loop [] (if (== trace 0) (recur) nil)) (suss.async/cancel! child) 42)))))";
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
    let starts = Arc::new(std::sync::Mutex::new(Vec::<u32>::new()));
    let drops = Arc::new(std::sync::Mutex::new(Vec::<u32>::new()));
    let completions = Arc::new(std::sync::Mutex::new(Vec::<u32>::new()));
    struct Operation {
        value: u32,
        drops: Arc<std::sync::Mutex<Vec<u32>>>,
    }
    impl Drop for Operation {
        fn drop(&mut self) {
            self.drops.lock().unwrap().push(self.value);
        }
    }
    let mut linker = Linker::new(&engine);
    let (started, dropped, completed) = (starts.clone(), drops.clone(), completions.clone());
    linker
        .root()
        .func_new_concurrent("increment", move |_, _, params, results| {
            let [Val::U32(value)] = params else {
                panic!("scalar operation selector")
            };
            let value = *value;
            started.lock().unwrap().push(value);
            let operation = Operation {
                value,
                drops: dropped.clone(),
            };
            let completed = completed.clone();
            Box::pin(async move {
                let _operation = operation;
                // Body I/O cannot complete. Finally I/O actually returns Pending
                // once, then succeeds; the final marker is observed only after it.
                let mut first = true;
                std::future::poll_fn(|cx| {
                    if value == 10 {
                        return Poll::<()>::Pending;
                    }
                    if value == 20 && first {
                        first = false;
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    } else {
                        Poll::Ready(())
                    }
                })
                .await;
                completed.lock().unwrap().push(value);
                results[0] = Val::U32(value);
                Ok(())
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
        drive(store.run_concurrent(async |accessor| run.call_concurrent(accessor, (1,)).await))
            .unwrap()
            .unwrap();
    assert_eq!(output, (42,));
    assert_eq!(
        *starts.lock().unwrap(),
        vec![10, 20, 30],
        "child finally must run through its pending cleanup before parent task.return"
    );
    assert_eq!(
        *completions.lock().unwrap(),
        vec![20, 30],
        "cleanup succeeds; the cancelled body operation cannot supply a late value"
    );
    let mut released = drops.lock().unwrap().clone();
    released.sort_unstable();
    assert_eq!(
        released,
        vec![10, 20, 30],
        "each host operation is reclaimed exactly once before invocation retirement"
    );
    store.gc(None).unwrap();
    let output =
        drive(store.run_concurrent(async |accessor| run.call_concurrent(accessor, (0,)).await))
            .unwrap()
            .unwrap();
    assert_eq!(
        output,
        (111,),
        "awaited cleanup and unawaited child unwind both finish in the original invocation"
    );
    assert_eq!(
        *starts.lock().unwrap(),
        vec![10, 20, 30],
        "retired child work cannot resume in the next canonical context"
    );
}

#[test]
fn public_component_host_quarantines_import_error_and_preserves_primary_error() {
    let bytes = generated_component(
        "(ns app) (def run (fn [x] (suss.async/future* (suss.async/await* (host/increment x)))))",
    );
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
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let mut linker = Linker::new(&engine);
    linker
        .root()
        .func_new_concurrent("increment", move |_, _, _, _| {
            observed.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { wasmtime::bail!("primary canonical host import failure") })
        })
        .unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(2_000_000).unwrap();
    let instance = drive(linker.instantiate_async(&mut store, &component)).unwrap();
    let mut host = async_component::ScalarHost::new(store, instance, "run").unwrap();
    let error = drive(host.call(7)).unwrap_err();
    assert!(
        format!("{error:#}").contains("primary canonical host import failure"),
        "{error:#}"
    );
    assert!(host.is_quarantined());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let second = drive(host.call(8)).unwrap_err();
    assert!(format!("{second:#}").contains("ownership is quarantined"));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "no host operation may be replayed after uncertainty"
    );
    // The Store stays owned until explicit disposal; no recovery hook is called
    // on Wasmtime's trap-poisoned concurrent state.
    drop(host);
}

#[test]
fn public_component_host_abandoned_pending_call_retains_quarantine() {
    let bytes = generated_component(
        "(ns app) (def run (fn [x] (suss.async/future* (suss.async/await* (host/increment x)))))",
    );
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
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let mut linker = Linker::new(&engine);
    linker
        .root()
        .func_new_concurrent("increment", move |_, _, _, _| {
            observed.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                std::future::pending::<()>().await;
                Ok(())
            })
        })
        .unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(2_000_000).unwrap();
    let instance = drive(linker.instantiate_async(&mut store, &component)).unwrap();
    let mut host = async_component::ScalarHost::new(store, instance, "run").unwrap();
    {
        let mut call = pin!(host.call(7));
        let waker = Waker::from(Arc::new(WakeThread(std::thread::current())));
        let mut cx = Context::from_waker(&waker);
        assert!(call.as_mut().poll(&mut cx).is_pending());
        // Drop the Rust call future while the guest and host I/O are pending.
    }
    assert!(host.is_quarantined());
    let before = calls.load(Ordering::SeqCst);
    assert_eq!(before, 1, "must actually start the pending import");
    let error = drive(host.call(8)).unwrap_err();
    assert!(format!("{error:#}").contains("ownership is quarantined"));
    assert_eq!(calls.load(Ordering::SeqCst), before);
    drop(host);
}
