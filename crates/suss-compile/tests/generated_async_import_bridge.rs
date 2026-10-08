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
    // Initialization executes the entire shipped core plus async/app fragments.
    // Keep its finite allowance separate from the shared three-call exercise: a
    // larger core catalog must not consume the cancellation regression's budget.
    // Each phase remains bounded; invocation fuel is not replenished per round.
    store.set_fuel(2_000_000).unwrap();
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
    // Exercise the full shipped core and integrated stream-aware scheduler
    // within Session's default operation budget, rather than the old minimal
    // fixture's 2M budget. Cleanup/output/resource assertions remain mandatory.
    store.set_fuel(10_000_000).unwrap();
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

/// Only the caller is hand-written WAT. Its callee is the production generated
/// component with actual compiled Suss continuations and canonical import bridge.
// Wrap only the generated canonical callback. The source/runtime/driver bytes
// remain unchanged; the hook runs AFTER the production callback handles event 6,
// while import 20 is still gated. A cancellation request alone is not a witness.
fn event6_witness_component(generated: &[u8]) -> Vec<u8> {
    use wasm_encoder::*;
    let mut sections = Vec::new();
    let mut depth = 0;
    for payload in wasmparser::Parser::new(0).parse_all(generated) {
        let payload = payload.unwrap();
        if depth == 0 {
            if let Some((id, range)) = payload.as_section() {
                sections.push((id, range));
            }
        }
        match payload {
            wasmparser::Payload::ModuleSection { .. }
            | wasmparser::Payload::ComponentSection { .. } => depth += 1,
            wasmparser::Payload::End(_) if depth > 0 => depth -= 1,
            _ => {}
        }
    }
    // This fixture has exactly three fragments: shipped core, async, app.
    assert_eq!(
        sections
            .iter()
            .filter(|(id, _)| *id == u8::from(ComponentSectionId::CoreModule))
            .count(),
        9
    );
    let lift = sections
        .iter()
        .rposition(|(id, _)| *id == u8::from(ComponentSectionId::CanonicalFunction))
        .unwrap();
    let mut component = wasm_encoder::Component::new();
    for (index, (id, range)) in sections.into_iter().enumerate() {
        if index == lift {
            let mut types = ComponentTypeSection::new();
            types
                .function()
                .params([] as [(&str, PrimitiveValType); 0])
                .result(None);
            component.section(&types);
            let mut imports = ComponentImportSection::new();
            imports.import("event6-witness", ComponentTypeRef::Func(2));
            component.section(&imports);
            let mut canonical = CanonicalFunctionSection::new();
            canonical.lower(1, []);
            component.section(&canonical);
            let mut wrapper = Module::new();
            let mut types = TypeSection::new();
            types.ty().function([ValType::I32; 3], [ValType::I32]);
            types.ty().function([], []);
            let mut imports = ImportSection::new();
            imports.import("host", "callback", EntityType::Function(0));
            imports.import("host", "witness", EntityType::Function(1));
            let mut functions = FunctionSection::new();
            functions.function(0);
            let mut exports = ExportSection::new();
            exports.export("callback", ExportKind::Func, 2);
            let mut body = Function::new([(1, ValType::I32)]);
            for instruction in [
                Instruction::LocalGet(0),
                Instruction::LocalGet(1),
                Instruction::LocalGet(2),
                Instruction::Call(0),
                Instruction::LocalSet(3),
                Instruction::LocalGet(0),
                Instruction::I32Const(6),
                Instruction::I32Eq,
                Instruction::If(BlockType::Empty),
                Instruction::Call(1),
                Instruction::End,
                Instruction::LocalGet(3),
                Instruction::End,
            ] {
                body.instruction(&instruction);
            }
            let mut code = CodeSection::new();
            code.function(&body);
            wrapper
                .section(&types)
                .section(&imports)
                .section(&functions)
                .section(&exports)
                .section(&code);
            let wrapper = wrapper.finish();
            component.section(&RawSection {
                id: ComponentSectionId::CoreModule.into(),
                data: &wrapper,
            });
            let mut instances = InstanceSection::new();
            instances.export_items([
                ("callback", ExportKind::Func, 13),
                ("witness", ExportKind::Func, 14),
            ]);
            instances.instantiate(9, [("host", ModuleArg::Instance(10))]);
            component.section(&instances);
            let mut aliases = ComponentAliasSection::new();
            aliases.alias(Alias::CoreInstanceExport {
                instance: 11,
                kind: ExportKind::Func,
                name: "callback",
            });
            component.section(&aliases);
            let mut canonical = CanonicalFunctionSection::new();
            canonical.lift(
                12,
                1,
                [CanonicalOption::Async, CanonicalOption::Callback(15)],
            );
            component.section(&canonical);
        } else if id == u8::from(ComponentSectionId::Export) {
            let mut exports = ComponentExportSection::new();
            exports.export("run", ComponentExportKind::Func, 2, None);
            component.section(&exports);
        } else {
            component.section(&RawSection {
                id,
                data: &generated
                    [usize::try_from(range.start).unwrap()..usize::try_from(range.end).unwrap()],
            });
        }
    }
    let bytes = component.finish();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&bytes)
        .unwrap();
    bytes
}

fn cancellation_caller(callee: &[u8]) -> Vec<u8> {
    use wasm_encoder::{ComponentSectionId, RawSection};
    // Parsed from the adjacent WAT source with wasm-tools; an empty component
    // occupies the callee index without embedding binary bytes in text syntax.
    let outer = include_bytes!("fixtures/canonical_event6_caller.wasm");
    let mut component = wasm_encoder::Component::new();
    let mut depth = 0;
    let mut injected = false;
    for payload in wasmparser::Parser::new(0).parse_all(outer) {
        let payload = payload.unwrap();
        if depth == 0 {
            if let Some((id, range)) = payload.as_section() {
                let data = if id == u8::from(ComponentSectionId::Component) {
                    assert!(
                        !injected,
                        "caller fixture must have exactly one callee placeholder"
                    );
                    injected = true;
                    callee
                } else {
                    &outer
                        [usize::try_from(range.start).unwrap()..usize::try_from(range.end).unwrap()]
                };
                component.section(&RawSection { id, data });
            }
        }
        match payload {
            wasmparser::Payload::ModuleSection { .. }
            | wasmparser::Payload::ComponentSection { .. } => depth += 1,
            wasmparser::Payload::End(_) if depth > 0 => depth -= 1,
            _ => {}
        }
    }
    assert!(injected, "caller fixture must contain a callee placeholder");
    let bytes = component.finish();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&bytes)
        .unwrap();
    bytes
}

#[test]
fn component_caller_event6_drains_source_finally_before_cancellation_ack() {
    event6_cancellation_regression(false);
}

#[cfg(unix)]
#[test]
fn component_caller_event6_closes_real_pending_read_before_cancellation_ack() {
    event6_cancellation_regression(true);
}

fn event6_cancellation_regression(real_io: bool) {
    use std::sync::{Mutex, atomic::AtomicBool};
    #[cfg(unix)]
    use std::{
        collections::VecDeque,
        io::{Read, Write},
        os::unix::net::UnixStream,
    };
    #[cfg(not(unix))]
    assert!(!real_io, "real I/O supplement requires UnixStream");
    let app = r#"(ns app)
      (def trace 0)
      (def run (fn [mode] (suss.async/future*
        (if (== mode 0) trace
          (try
            (if (== mode 1) (suss.async/await* (host/increment 10)) nil)
            (catch :default error (set! trace (+ trace 1000)))
            (finally
              (set! trace (+ trace 1))
              (suss.async/await* (suss.async/future*
                (suss.async/await* (host/increment 20))))
              (suss.async/await* (host/increment 30))
              (set! trace (+ trace 10))
              (if (== mode 3) (throw 77) nil)))))))"#;
    let generated = generated_fragments(&[include_str!("../src/portable/stdlib/async.sus"), app]);
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
    let component = Component::new(
        &engine,
        cancellation_caller(&event6_witness_component(&generated)),
    )
    .unwrap();
    #[derive(Default)]
    struct Evidence {
        starts: Vec<u32>,
        pending: Vec<u32>,
        completions: Vec<u32>,
        drops: Vec<u32>,
        acknowledgments: usize,
    }
    struct Operation(u32, Arc<Mutex<Evidence>>);
    impl Drop for Operation {
        fn drop(&mut self) {
            self.1.lock().unwrap().drops.push(self.0);
        }
    }
    // The connector retains no duplicate reader: each sole fd is moved into
    // the corresponding pending canonical host future before its first poll.
    #[cfg(unix)]
    let (readers, peers) = {
        let mut readers = VecDeque::new();
        let mut peers = Vec::new();
        if real_io {
            for _ in 0..3 {
                let (reader, peer) = UnixStream::pair().unwrap();
                reader.set_nonblocking(true).unwrap();
                peer.set_nonblocking(true).unwrap();
                readers.push_back(reader);
                peers.push(peer);
            }
        }
        (Arc::new(Mutex::new(readers)), Arc::new(Mutex::new(peers)))
    };
    let evidence = Arc::new(Mutex::new(Evidence::default()));
    let target = Arc::new(AtomicUsize::new(10));
    let ready = Arc::new(AtomicBool::new(false));
    let gate = Arc::new(AtomicBool::new(false));
    let wake = Arc::new(Mutex::new(None::<Waker>));
    let mut linker = Linker::<()>::new(&engine);
    let (observed, selector, polled, released, wake_io) = (
        evidence.clone(),
        target.clone(),
        ready.clone(),
        gate.clone(),
        wake.clone(),
    );
    #[cfg(unix)]
    let pending_readers = readers.clone();
    linker
        .root()
        .func_new_concurrent("increment", move |_, _, params, results| {
            let [Val::U32(value)] = params else {
                panic!("scalar operation selector")
            };
            let value = *value;
            #[cfg(unix)]
            let mut socket = if real_io && value == 10 {
                Some(
                    pending_readers
                        .lock()
                        .unwrap()
                        .pop_front()
                        .expect("sole pending reader"),
                )
            } else {
                None
            };
            observed.lock().unwrap().starts.push(value);
            let operation = Operation(value, observed.clone());
            let (observed, selector, polled, released, wake_io) = (
                observed.clone(),
                selector.clone(),
                polled.clone(),
                released.clone(),
                wake_io.clone(),
            );
            Box::pin(async move {
                let _operation = operation;
                let mut first = true;
                std::future::poll_fn(|cx| {
                    #[cfg(unix)]
                    if let Some(socket) = socket.as_mut() {
                        let mut byte = [0];
                        match socket.read(&mut byte) {
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                                // This real read, rather than a synthetic Pending,
                                // is the witness that permits external cancellation.
                                if first {
                                    first = false;
                                    observed.lock().unwrap().pending.push(value);
                                    polled.store(true, Ordering::SeqCst);
                                }
                                return Poll::<()>::Pending;
                            }
                            other => {
                                panic!("body read must remain pending until event 6: {other:?}")
                            }
                        }
                    }
                    if first {
                        first = false;
                        observed.lock().unwrap().pending.push(value);
                        if value as usize == selector.load(Ordering::SeqCst) {
                            polled.store(true, Ordering::SeqCst);
                        }
                        cx.waker().wake_by_ref();
                        return Poll::<()>::Pending;
                    }
                    if value == 10 {
                        return Poll::Pending;
                    }
                    // For cancellation during pending finally, hold the awaited
                    // child until the generated callback has actually handled event 6.
                    if value == 20
                        && selector.load(Ordering::SeqCst) == 20
                        && !released.load(Ordering::SeqCst)
                    {
                        *wake_io.lock().unwrap() = Some(cx.waker().clone());
                        return Poll::Pending;
                    }
                    Poll::Ready(())
                })
                .await;
                observed.lock().unwrap().completions.push(value);
                results[0] = Val::U32(value);
                Ok(())
            })
        })
        .unwrap();
    let polled = ready.clone();
    linker
        .root()
        .func_wrap("ready", move |_, (): ()| {
            Ok((u32::from(polled.load(Ordering::SeqCst)),))
        })
        .unwrap();
    let event6_seen = Arc::new(AtomicBool::new(false));
    let (seen, observed, selector, released, wake_io) = (
        event6_seen.clone(),
        evidence.clone(),
        target.clone(),
        gate.clone(),
        wake.clone(),
    );
    linker
        .root()
        .func_wrap("event6-witness", move |_, (): ()| {
            assert!(
                !seen.swap(true, Ordering::SeqCst),
                "exactly one event6 callback per invocation"
            );
            assert!(
                !released.load(Ordering::SeqCst),
                "cleanup gate stays closed through event6 delivery"
            );
            if selector.load(Ordering::SeqCst) == 20 {
                let evidence = observed.lock().unwrap();
                assert_eq!(evidence.starts.last(), Some(&20));
                assert_eq!(evidence.pending.last(), Some(&20));
                assert_eq!(
                    evidence.completions.len(),
                    evidence.acknowledgments * 2,
                    "awaited cleanup import must still be pending after event6 callback"
                );
                assert!(
                    evidence.starts.len() > evidence.drops.len(),
                    "cleanup operation remains owned"
                );
            }
            released.store(true, Ordering::SeqCst);
            if let Some(waker) = wake_io.lock().unwrap().take() {
                waker.wake();
            }
            Ok(())
        })
        .unwrap();
    let observed = evidence.clone();
    let seen = event6_seen.clone();
    #[cfg(unix)]
    let acknowledgment_peers = peers.clone();
    linker
        .root()
        .func_wrap("ack", move |_, (): ()| {
            assert!(
                seen.load(Ordering::SeqCst),
                "event6 delivery precedes acknowledgment"
            );
            let mut evidence = observed.lock().unwrap();
            let mut starts = evidence.starts.clone();
            let mut drops = evidence.drops.clone();
            starts.sort_unstable();
            drops.sort_unstable();
            assert_eq!(
                drops, starts,
                "all operation guards dropped before canonical cancellation acknowledgment"
            );
            assert_eq!(
                evidence.completions.len(),
                2 * (evidence.acknowledgments + 1),
                "both awaited finally imports must complete before acknowledgment"
            );
            #[cfg(unix)]
            if real_io {
                let error = acknowledgment_peers.lock().unwrap()[evidence.acknowledgments]
                    .write_all(b"late completion")
                    .expect_err(
                        "sole read fd must close before canonical cancellation acknowledgment",
                    );
                assert!(
                    matches!(
                        error.kind(),
                        std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
                    ),
                    "{error}"
                );
            }
            evidence.acknowledgments += 1;
            Ok(())
        })
        .unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(10_000_000).unwrap();
    let instance = drive(linker.instantiate_async(&mut store, &component)).unwrap();
    let cancel = instance
        .get_typed_func::<(u32,), (u32,)>(&mut store, "cancel-run")
        .unwrap();
    let observe = instance
        .get_typed_func::<(u32,), (u32,)>(&mut store, "observe")
        .unwrap();
    let modes = if real_io { [1, 1, 1] } else { [1, 2, 3] };
    for (round, mode) in modes.into_iter().enumerate() {
        target.store(if mode == 1 { 10 } else { 20 }, Ordering::SeqCst);
        event6_seen.store(false, Ordering::SeqCst);
        ready.store(false, Ordering::SeqCst);
        gate.store(false, Ordering::SeqCst);
        let output = drive(
            store.run_concurrent(async |accessor| cancel.call_concurrent(accessor, (mode,)).await),
        )
        .unwrap()
        .unwrap();
        assert_eq!(output, (99,));
        assert!(
            ready.load(Ordering::SeqCst),
            "cancellation must follow a polled Pending import (real WouldBlock in the Unix supplement)"
        );
        assert_eq!(evidence.lock().unwrap().acknowledgments, round + 1);
        store.gc(None).unwrap();
        let output = drive(
            store.run_concurrent(async |accessor| observe.call_concurrent(accessor, (0,)).await),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            output,
            (11 * (round as u32 + 1),),
            "cleanup finishes once, own cancellation bypasses catch, next invocation retains state"
        );
    }
    let evidence = evidence.lock().unwrap();
    assert_eq!(
        evidence.starts,
        if real_io {
            vec![10, 20, 30, 10, 20, 30, 10, 20, 30]
        } else {
            vec![10, 20, 30, 20, 30, 20, 30]
        }
    );
    #[cfg(unix)]
    assert!(
        readers.lock().unwrap().is_empty(),
        "all sole reader fds transferred into host futures"
    );
    assert_eq!(
        evidence.pending, evidence.starts,
        "every import genuinely suspended"
    );
    assert_eq!(evidence.completions, [20, 30, 20, 30, 20, 30]);
}
