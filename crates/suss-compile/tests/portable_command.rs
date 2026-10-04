//! Execute official command artifacts and verify their pinned upstream graph.
use suss_compile::portable;
use wit_parser::{FunctionKind, Type, TypeDefKind, WorldItem};

fn engine() -> wasmtime::Engine {
    static ENGINE: std::sync::OnceLock<wasmtime::Engine> = std::sync::OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut config = wasmtime::Config::new();
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
            wasmtime::Engine::new(&config).unwrap()
        })
        .clone()
}

#[test]
fn official_command_executes_normal_and_exception_completion_in_fresh_stores() {
    use portable::resolve::{Environment, Phase};
    use suss_reader::Symbol;
    use wasmtime::Store;
    use wasmtime::component::{Component, Linker};
    let engine = engine();
    for (source, expected) in [
        ("(ns app) (def -main (fn [] 73))", Ok(())),
        ("(ns app) (def -main (fn [] (throw 17)))", Err(())),
    ] {
        let fragment =
            portable::prepare_fragment(source, &Environment::default(), Phase::Runtime).unwrap();
        let bytes =
            portable::command::component(&[fragment], &Symbol::namespaced("app", "-main")).unwrap();
        let component = Component::new(&engine, bytes).unwrap();
        let mut linker = Linker::<()>::new(&engine);
        linker
            .instance("wasi:cli/environment@0.3.1")
            .unwrap()
            .func_wrap("get-arguments", |_, ()| {
                Ok((vec!["artifact λ雪🦊".to_owned()],))
            })
            .unwrap();
        for _ in 0..2 {
            let mut store = Store::new(&engine, ());
            store.set_fuel(20_000_000).unwrap();
            let instance = linker.instantiate(&mut store, &component).unwrap();
            let interface = instance
                .get_export_index(&mut store, None, "wasi:cli/run@0.3.1")
                .unwrap();
            let run = instance
                .get_export_index(&mut store, Some(&interface), "run")
                .unwrap();
            let run = instance
                .get_typed_func::<(), (Result<(), ()>,)>(&mut store, &run)
                .unwrap();
            assert!(run.func().ty(&store).async_());
            for _ in 0..3 {
                assert_eq!(
                    block_on(run.call_async(&mut store, ())).unwrap().0,
                    expected
                );
                store.gc(None).unwrap();
            }
        }
    }
}

#[test]
fn command_strings_retained_in_source_cells_survive_canonical_reuse_and_gc() {
    use portable::resolve::Phase;
    use suss_reader::Symbol;
    use wasmtime::Store;
    use wasmtime::component::{Component, Linker};
    let engine = engine();
    let core = portable::bootstrap::shipped(Phase::Runtime)
        .unwrap()
        .clone();
    let source = "(ns app) (def kept nil) (def -main (fn [x] (if kept (if (= kept \"λ雪🦊\") (if (= x \"second\") 73 (throw 17)) (throw 17)) (do (set! kept x) 73))))";
    let fragment = portable::prepare_fragment(source, &core.environment, Phase::Runtime).unwrap();
    let bytes =
        portable::command::component(&[core, fragment], &Symbol::namespaced("app", "-main"))
            .unwrap();
    let component = Component::new(&engine, bytes).unwrap();
    let mut linker = Linker::<Vec<String>>::new(&engine);
    linker
        .instance("wasi:cli/environment@0.3.1")
        .unwrap()
        .func_wrap("get-arguments", |store, ()| Ok((store.data().clone(),)))
        .unwrap();
    for _ in 0..2 {
        let mut store = Store::new(&engine, vec!["artifact".to_owned(), "λ雪🦊".to_owned()]);
        store.set_fuel(20_000_000).unwrap();
        let instance = linker.instantiate(&mut store, &component).unwrap();
        let interface = instance
            .get_export_index(&mut store, None, "wasi:cli/run@0.3.1")
            .unwrap();
        let run = instance
            .get_export_index(&mut store, Some(&interface), "run")
            .unwrap();
        let run = instance
            .get_typed_func::<(), (Result<(), ()>,)>(&mut store, &run)
            .unwrap();
        assert_eq!(block_on(run.call_async(&mut store, ())).unwrap().0, Ok(()));
        store.gc(None).unwrap();
        store.data_mut()[1] = "second".to_owned();
        for _ in 0..3 {
            assert_eq!(block_on(run.call_async(&mut store, ())).unwrap().0, Ok(()));
            store.gc(None).unwrap();
        }
        store.data_mut()[1] = "wrong".to_owned();
        assert_eq!(block_on(run.call_async(&mut store, ())).unwrap().0, Err(()));
    }
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    use std::{
        pin::pin,
        sync::Arc,
        task::{Context, Poll, Wake, Waker},
    };
    struct ThreadWake(std::thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
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
                    "command call exceeded test deadline"
                );
                std::thread::park_timeout(std::time::Duration::from_millis(100));
            }
        }
    }
}

#[test]
fn embedded_official_command_resolves_argument_and_status_types() {
    let (resolve, world) = portable::command::official_profile().unwrap();
    let selected = &resolve.worlds[world];
    let package = &resolve.packages[selected.package.unwrap()];
    assert_eq!(package.name.to_string(), "wasi:cli@0.3.1");
    assert_eq!(selected.name, "command");
    let run = selected
        .exports
        .iter()
        .find_map(|(key, item)| {
            if resolve.name_world_key(key) != "wasi:cli/run@0.3.1" {
                return None;
            }
            let WorldItem::Interface { id, .. } = item else {
                panic!("run must be an interface");
            };
            Some(&resolve.interfaces[*id].functions["run"])
        })
        .expect("official command run export");
    assert_eq!(run.kind, FunctionKind::AsyncFreestanding);
    assert!(run.params.is_empty());
    let Type::Id(result) = run.result.unwrap() else {
        panic!("run must return result");
    };
    let TypeDefKind::Result(result) = &resolve.types[result].kind else {
        panic!("run result shape");
    };
    assert!(result.ok.is_none() && result.err.is_none());
    let arguments = selected
        .imports
        .iter()
        .find_map(|(key, item)| {
            if resolve.name_world_key(key) != "wasi:cli/environment@0.3.1" {
                return None;
            }
            let WorldItem::Interface { id, .. } = item else {
                panic!("environment must be an interface");
            };
            Some(&resolve.interfaces[*id].functions["get-arguments"])
        })
        .expect("official argument import");
    assert_eq!(arguments.kind, FunctionKind::Freestanding);
    assert!(arguments.params.is_empty());
    let Type::Id(list) = arguments.result.unwrap() else {
        panic!("argument list");
    };
    assert!(matches!(
        &resolve.types[list].kind,
        TypeDefKind::List(Type::String)
    ));
    let imports = selected
        .imports
        .keys()
        .map(|key| resolve.name_world_key(key))
        .collect::<Vec<_>>();
    assert!(imports.iter().any(|name| name == "wasi:cli/exit@0.3.1"));
    assert!(
        imports
            .iter()
            .any(|name| name == "wasi:filesystem/types@0.3.1")
    );
    assert!(
        imports
            .iter()
            .any(|name| name == "wasi:sockets/types@0.3.1")
    );
}

#[test]
fn command_uses_live_main_and_initializes_fragments_once_in_order() {
    use portable::resolve::Phase;
    use suss_reader::Symbol;
    use wasmtime::{
        Store,
        component::{Component, Linker},
    };
    let core = portable::bootstrap::shipped(Phase::Runtime)
        .unwrap()
        .clone();
    let first = portable::prepare_fragment(
        "(ns app) (def seen 0) (def -main (fn [x] (throw 99))) (set! seen (+ seen 1))",
        &core.environment,
        Phase::Runtime,
    )
    .unwrap();
    let second = portable::prepare_fragment("(ns app) (def -main (fn [x] (let [first (= seen 11)] (set! seen (+ seen 1)) (if (= x (if first \"first\" \"later\")) 73 (throw 17))))) (set! seen (+ seen 10))", &first.environment, Phase::Runtime).unwrap();
    let bytes =
        portable::command::component(&[core, first, second], &Symbol::namespaced("app", "-main"))
            .unwrap();
    let engine = engine();
    let component = Component::new(&engine, bytes).unwrap();
    let mut linker = Linker::<Vec<String>>::new(&engine);
    linker
        .instance("wasi:cli/environment@0.3.1")
        .unwrap()
        .func_wrap("get-arguments", |store, ()| Ok((store.data().clone(),)))
        .unwrap();
    for _ in 0..2 {
        let mut store = Store::new(&engine, vec!["artifact".to_owned(), "first".to_owned()]);
        store.set_fuel(20_000_000).unwrap();
        let instance = linker.instantiate(&mut store, &component).unwrap();
        let interface = instance
            .get_export_index(&mut store, None, "wasi:cli/run@0.3.1")
            .unwrap();
        let run = instance
            .get_export_index(&mut store, Some(&interface), "run")
            .unwrap();
        let run = instance
            .get_typed_func::<(), (Result<(), ()>,)>(&mut store, &run)
            .unwrap();
        assert_eq!(block_on(run.call_async(&mut store, ())).unwrap().0, Ok(()));
        store.gc(None).unwrap();
        // Initializers must not reset seen to 11 on repeated command calls.
        assert_eq!(block_on(run.call_async(&mut store, ())).unwrap().0, Err(()));
        store.gc(None).unwrap();
        store.data_mut()[1] = "later".to_owned();
        assert_eq!(block_on(run.call_async(&mut store, ())).unwrap().0, Ok(()));
        store.gc(None).unwrap();
    }
}
