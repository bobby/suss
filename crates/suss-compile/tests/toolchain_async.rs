//! M0-02 canonical stackless ABI feasibility; this does not implement Suss async.
use std::future::Future;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};

fn drive<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(result) => return result,
            Poll::Pending => {
                assert!(
                    Instant::now() < deadline,
                    "canonical fixture exceeded five seconds"
                );
                std::thread::yield_now();
            }
        }
    }
}

#[test]
fn canonical_callback_yields_resumes_and_returns_once() {
    let wat = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/callback-resume.wat"
    ))
    .expect("executing canonical callback fixture must exist");
    let mut config = Config::new();
    config
        .wasm_component_model_async(true)
        .wasm_component_model_async_stackful(false)
        .consume_fuel(true);
    let engine = Engine::new(&config).unwrap();
    let component = Component::new(&engine, wat).expect("validate stackless component");
    let mut linker = Linker::<Vec<u32>>::new(&engine);
    linker
        .root()
        .func_wrap("trace", |mut context, (step,): (u32,)| {
            context.data_mut().push(step);
            Ok(())
        })
        .unwrap();
    let mut store = Store::new(&engine, Vec::new());
    store.set_fuel(100_000).unwrap();
    let instance = drive(linker.instantiate_async(&mut store, &component)).unwrap();
    let run = instance
        .get_typed_func::<(u32,), (u32,)>(&mut store, "run")
        .unwrap();
    let result =
        drive(store.run_concurrent(async |accessor| run.call_concurrent(accessor, (41,)).await))
            .expect("drive canonical scheduler")
            .expect("guest callback result");
    assert_eq!(result, (42,));
    assert_eq!(
        store.data(),
        &[1, 2, 3],
        "entry, callback and task.return trace"
    );
}

struct CaptureFuture(std::sync::Arc<std::sync::Mutex<Option<u32>>>);
impl wasmtime::component::FutureConsumer<()> for CaptureFuture {
    type Item = u32;
    fn poll_consume(
        self: std::pin::Pin<&mut Self>,
        _: &mut Context<'_>,
        mut store: wasmtime::StoreContextMut<'_, ()>,
        mut source: wasmtime::component::Source<'_, u32>,
        _: bool,
    ) -> Poll<wasmtime::Result<()>> {
        let mut value = None;
        source.read(&mut store, &mut value)?;
        assert!(value.is_some(), "future must deliver its payload");
        *self.0.lock().unwrap() = value;
        Poll::Ready(Ok(()))
    }
}

#[test]
fn future_endpoint_roundtrips_without_flattening_its_payload() {
    use wasmtime::component::FutureReader;
    let wat = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/async-endpoint-roundtrip.wat"
    ))
    .expect("endpoint transfer fixture must exist");
    let mut config = Config::new();
    config
        .wasm_component_model_async(true)
        .wasm_component_model_async_stackful(false);
    let engine = Engine::new(&config).unwrap();
    let component = Component::new(&engine, wat).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = drive(Linker::new(&engine).instantiate_async(&mut store, &component)).unwrap();
    let echo = instance
        .get_typed_func::<(FutureReader<u32>,), (FutureReader<u32>,)>(&mut store, "future")
        .unwrap();
    let input = FutureReader::new(&mut store, async { wasmtime::error::Ok(42_u32) }).unwrap();
    let (output,) = drive(
        store.run_concurrent(async |accessor| echo.call_concurrent(accessor, (input,)).await),
    )
    .unwrap()
    .unwrap();
    let value = std::sync::Arc::new(std::sync::Mutex::new(None));
    output
        .pipe(&mut store, CaptureFuture(value.clone()))
        .unwrap();
    drive(store.run_concurrent(async |_| {
        std::future::poll_fn(|context| {
            if value.lock().unwrap().is_some() {
                Poll::Ready(())
            } else {
                context.waker().wake_by_ref();
                Poll::Pending
            }
        })
        .await;
    }))
    .unwrap();
    assert_eq!(*value.lock().unwrap(), Some(42));
}

struct CaptureStream(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
impl wasmtime::component::StreamConsumer<()> for CaptureStream {
    type Item = u8;
    fn poll_consume(
        self: std::pin::Pin<&mut Self>,
        _: &mut Context<'_>,
        store: wasmtime::StoreContextMut<'_, ()>,
        source: wasmtime::component::Source<'_, u8>,
        _: bool,
    ) -> Poll<wasmtime::Result<wasmtime::component::StreamResult>> {
        let mut source = source.as_direct(store);
        let count = source.remaining().len();
        self.0.lock().unwrap().extend_from_slice(source.remaining());
        source.mark_read(count);
        // This bounded consumer closes after the expected three bytes. EOF and
        // backpressure are separate gates, not certified by this transport test.
        let received = self.0.lock().unwrap().len();
        assert!(received <= 3, "unexpected extra stream bytes");
        Poll::Ready(Ok(if received == 3 {
            wasmtime::component::StreamResult::Dropped
        } else {
            wasmtime::component::StreamResult::Completed
        }))
    }
}

#[test]
fn stream_endpoint_roundtrips_and_delivers_boundary_bytes() {
    use wasmtime::component::StreamReader;
    let wat = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/async-endpoint-roundtrip.wat"
    ))
    .expect("endpoint transfer fixture must exist");
    let mut config = Config::new();
    config
        .wasm_component_model_async(true)
        .wasm_component_model_async_stackful(false);
    let engine = Engine::new(&config).unwrap();
    let component = Component::new(&engine, wat).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = drive(Linker::new(&engine).instantiate_async(&mut store, &component)).unwrap();
    let echo = instance
        .get_typed_func::<(StreamReader<u8>,), (StreamReader<u8>,)>(&mut store, "stream")
        .unwrap();
    let input = StreamReader::new(&mut store, vec![0_u8, 255, 42]).unwrap();
    let (output,) = drive(
        store.run_concurrent(async |accessor| echo.call_concurrent(accessor, (input,)).await),
    )
    .unwrap()
    .unwrap();
    let bytes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    output
        .pipe(&mut store, CaptureStream(bytes.clone()))
        .unwrap();
    drive(store.run_concurrent(async |_| {
        std::future::poll_fn(|context| {
            if bytes.lock().unwrap().len() == 3 {
                Poll::Ready(())
            } else {
                context.waker().wake_by_ref();
                Poll::Pending
            }
        })
        .await;
    }))
    .unwrap();
    assert_eq!(*bytes.lock().unwrap(), [0, 255, 42]);
}

#[test]
fn canonical_async_import_suspends_and_completes_through_callback() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use wasmtime::component::Val;
    let wat = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/callback-async-import.wat"
    ))
    .expect("executing async import fixture must exist");
    let mut config = Config::new();
    config
        .wasm_component_model_async(true)
        .wasm_component_model_async_stackful(false)
        .consume_fuel(true);
    let engine = Engine::new(&config).unwrap();
    let component = Component::new(&engine, wat).expect("validated async import and callback");
    let mut linker = Linker::<Vec<u32>>::new(&engine);
    linker
        .root()
        .func_wrap("trace", |mut context, (step,): (u32,)| {
            context.data_mut().push(step);
            Ok(())
        })
        .unwrap();
    let polls = Arc::new(AtomicUsize::new(0));
    let count = polls.clone();
    linker
        .root()
        .func_new_concurrent("increment", move |_, _, params, results| {
            let count = count.clone();
            Box::pin(async move {
                std::future::poll_fn(|context| {
                    if count.fetch_add(1, Ordering::SeqCst) == 0 {
                        context.waker().wake_by_ref();
                        Poll::Pending
                    } else {
                        Poll::Ready(())
                    }
                })
                .await;
                let [Val::U32(value)] = params else {
                    panic!("typed async input");
                };
                results[0] = Val::U32(value + 1);
                Ok(())
            })
        })
        .unwrap();
    let mut store = Store::new(&engine, Vec::new());
    store.set_fuel(100_000).unwrap();
    let instance = drive(linker.instantiate_async(&mut store, &component)).unwrap();
    let run = instance
        .get_typed_func::<(u32,), (u32,)>(&mut store, "run")
        .unwrap();
    let result =
        drive(store.run_concurrent(async |accessor| run.call_concurrent(accessor, (41,)).await))
            .unwrap()
            .unwrap();
    assert_eq!(result, (42,));
    assert_eq!(store.data(), &[1, 2, 3]);
    assert!(
        polls.load(Ordering::SeqCst) >= 2,
        "import must actually suspend"
    );
}

#[test]
fn guest_cancels_pending_host_import_and_observes_terminal_event() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Dropped(Arc<AtomicUsize>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let wat = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/callback-cancel-import.wat"
    ))
    .expect("executing guest cancellation fixture must exist");
    let mut config = Config::new();
    config
        .wasm_component_model_more_async_builtins(true)
        .wasm_component_model_async(true)
        .wasm_component_model_async_stackful(false)
        .consume_fuel(true);
    let engine = Engine::new(&config).unwrap();
    let component = Component::new(&engine, wat).expect("validated guest subtask cancellation");
    let mut linker = Linker::<Vec<u32>>::new(&engine);
    linker
        .root()
        .func_wrap("trace", |mut context, (step,): (u32,)| {
            context.data_mut().push(step);
            Ok(())
        })
        .unwrap();
    let drops = Arc::new(AtomicUsize::new(0));
    let count = drops.clone();
    let polls = Arc::new(AtomicUsize::new(0));
    let poll_count = polls.clone();
    linker
        .root()
        .func_new_concurrent("increment", move |_, _, _, _| {
            // Construct the guard before returning the future so abort-before-first-poll
            // also proves the allocated operation is reclaimed.
            let guard = Dropped(count.clone());
            let poll_count = poll_count.clone();
            Box::pin(async move {
                let _guard = guard;
                std::future::poll_fn(|_| {
                    poll_count.fetch_add(1, Ordering::SeqCst);
                    Poll::<()>::Pending
                })
                .await;
                unreachable!("this import may only finish through cancellation");
            })
        })
        .unwrap();
    let mut store = Store::new(&engine, Vec::new());
    store.set_fuel(100_000).unwrap();
    let instance = drive(linker.instantiate_async(&mut store, &component)).unwrap();
    let run = instance
        .get_typed_func::<(u32,), (u32,)>(&mut store, "run")
        .unwrap();
    let result =
        drive(store.run_concurrent(async |accessor| run.call_concurrent(accessor, (41,)).await))
            .unwrap()
            .unwrap();
    assert_eq!(result, (99,));
    assert!(
        polls.load(Ordering::SeqCst) >= 1,
        "host operation was pending before cancellation"
    );
    assert_eq!(store.data(), &[1, 2, 3]);
    assert_eq!(
        drops.load(Ordering::SeqCst),
        1,
        "cancellation reclaims the host future before Store teardown"
    );
}
