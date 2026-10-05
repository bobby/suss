//! Public compiler entry points execute the shared compiled macro/AOT pipeline.
#![cfg(not(target_family = "wasm"))]

use std::{
    future::Future,
    pin::pin,
    sync::{Arc, OnceLock},
    task::{Context, Poll, Wake, Waker},
};
use suss_compile::{CompileError, Compiler};
use wasmtime::component::{Component, Linker};
use wasmtime::{AsContextMut, Config, Engine, Store};

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
                .max_wasm_stack(2 * 1024 * 1024)
                .consume_fuel(true)
                .cranelift_opt_level(wasmtime::OptLevel::None);
            Engine::new(&config).unwrap()
        })
        .clone()
}

struct WakeCurrent(std::thread::Thread);
impl Wake for WakeCurrent {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}
fn block_on<T>(future: impl Future<Output = T>) -> T {
    let mut future = pin!(future);
    let waker = Waker::from(Arc::new(WakeCurrent(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => {
                assert!(std::time::Instant::now() < deadline, "async call timed out");
                std::thread::park_timeout(std::time::Duration::from_millis(100));
            }
        }
    }
}

#[test]
fn public_compiler_macros_receive_real_lexical_bindings_and_runtime_effects_do_not_replay() {
    let source = r#"
        (defmacro require-local [local]
          (if (get (:locals &env) local) local (throw 17)))
        (def seen (atom 0))
        (defn ^:export calculate [x]
          (let [answer (+ x (swap! seen inc))]
            (require-local answer)))
    "#;
    let bytes = Compiler::new()
        .compile(
            source,
            "package test:public-compiler; world api { export calculate: func(x: f64) -> f64; }",
        )
        .unwrap();
    let engine = engine();
    let component = Component::new(&engine, bytes).unwrap();
    assert_eq!(component.component_type().imports(&engine).count(), 0);
    let mut store = Store::new(&engine, ());
    store.set_fuel(100_000_000).unwrap();
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let calculate = instance
        .get_typed_func::<(f64,), (f64,)>(&mut store, "calculate")
        .unwrap();
    for expected in [42.0_f64, 43.0, 44.0] {
        assert_eq!(
            calculate.call(&mut store, (41.0,)).unwrap().0.to_bits(),
            expected.to_bits()
        );
        calculate.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
    }
}

#[test]
fn public_compiler_defers_runtime_initialization_until_component_instantiation() {
    let bytes = Compiler::new()
        .compile(
            "(defmacro payload [] 42) (def initialized (throw (payload)))",
            "package test:public-throw; world api {}",
        )
        .unwrap();
    let engine = engine();
    let component = Component::new(&engine, bytes).unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(100_000_000).unwrap();
    assert!(
        Linker::new(&engine)
            .instantiate(&mut store, &component)
            .is_err()
    );
    let exception = store
        .as_context_mut()
        .take_pending_exception()
        .expect("Runtime language exception");
    let fields = exception.fields(&mut store).unwrap().collect::<Vec<_>>();
    assert_eq!(fields.len(), 1);
    let payload = fields[0]
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap();
    assert_eq!(
        payload.field(&mut store, 0).unwrap().unwrap_f64().to_bits(),
        42.0_f64.to_bits()
    );
}

#[test]
fn public_compiler_resolves_wit_before_effectful_macro_expansion() {
    let error = Compiler::new()
        .compile("(defmacro fail [] (throw 99)) (fail)", "invalid WIT")
        .unwrap_err();
    assert!(matches!(error, CompileError::Wit(_)), "{error}");
}

#[test]
fn public_compiler_rejects_unsupported_boundary_before_effectful_macro_expansion() {
    let error = Compiler::new()
        .compile(
            "(defmacro fail [] (throw 99)) (fail)",
            "package test:public-unsupported; world api { export echo: func(x: list<map<string, u32>>); }",
        )
        .unwrap_err();
    assert!(matches!(error, CompileError::Unsupported(_)), "{error}");
    assert!(
        error.to_string().contains("list<map<string, u32>>"),
        "{error}"
    );
}

#[test]
fn file_compiler_rejects_unsupported_boundary_before_effectful_macro_expansion() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("entry.sus");
    let wit = root.path().join("api.wit");
    std::fs::write(&source, "(defmacro fail [] (throw 99)) (fail)").unwrap();
    std::fs::write(
        &wit,
        "package test:file-unsupported; world api { export echo: func(x: list<map<string, u32>>); }",
    )
    .unwrap();
    let error = suss_compile::portable_aot::compile_file(
        &source,
        &wit,
        None,
        &[root.path().to_owned()],
        &[],
    )
    .unwrap_err();
    assert!(error.contains("list<map<string, u32>>"), "{error}");
}

#[test]
fn public_compiler_loop_recur_executes_deep_iterations_with_bounded_stack() {
    let bytes = Compiler::new()
        .compile(
            "(defn ^:export count-down [n] (loop [i n total 0] (if (= i 0) total (recur (dec i) (inc total)))))",
            "package test:public-recur; world api { export count-down: func(n: f64) -> f64; }",
        )
        .unwrap();
    let engine = engine();
    let component = Component::new(&engine, bytes).unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(200_000_000).unwrap();
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let count = instance
        .get_typed_func::<(f64,), (f64,)>(&mut store, "count-down")
        .unwrap();
    for iterations in [100_000.0_f64, 0.0, 42.0] {
        assert_eq!(
            count.call(&mut store, (iterations,)).unwrap().0.to_bits(),
            iterations.to_bits()
        );
        count.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
    }
}

#[test]
fn public_compiler_async_scalar_exports_execute_and_retain_live_state_after_gc() {
    let bytes = Compiler::new()
        .compile(
            "(def seen (atom 0)) (defn ^:export echo [x] (+ x (swap! seen inc)))",
            "package test:public-async; world api { export echo: async func(x: u32) -> u32; }",
        )
        .unwrap();
    let engine = engine();
    let component = Component::new(&engine, bytes).unwrap();
    assert_eq!(component.component_type().imports(&engine).count(), 0);
    let mut store = Store::new(&engine, ());
    store.set_fuel(100_000_000).unwrap();
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let echo = instance
        .get_typed_func::<(u32,), (u32,)>(&mut store, "echo")
        .unwrap();
    assert!(echo.func().ty(&store).async_());
    for expected in [42, 43, 44] {
        assert_eq!(
            block_on(echo.call_async(&mut store, (41,))).unwrap().0,
            expected
        );
        store.gc(None).unwrap();
    }
}

#[test]
fn public_compiler_function_recur_uses_bounded_stack_and_simultaneous_rebinding() {
    let bytes = Compiler::new()
        .compile(
            "(defn ^:export rotate [n a b] (if (= n 0) (+ (* a 100) b) (recur (dec n) b a)))",
            "package test:public-function-recur; world api { export rotate: func(n: f64, a: f64, b: f64) -> f64; }",
        )
        .unwrap();
    let engine = engine();
    let component = Component::new(&engine, bytes).unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(200_000_000).unwrap();
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let rotate = instance
        .get_typed_func::<(f64, f64, f64), (f64,)>(&mut store, "rotate")
        .unwrap();
    for (iterations, expected) in [
        (0.0, 1234.0_f64),
        (1.0, 3412.0),
        (2.0, 1234.0),
        (100_000.0, 1234.0),
        (100_001.0, 3412.0),
    ] {
        // Each independently bounded call gets its own fuel allowance.
        store.set_fuel(200_000_000).unwrap();
        assert_eq!(
            rotate
                .call(&mut store, (iterations, 12.0, 34.0))
                .unwrap_or_else(|error| panic!("{iterations} iterations: {error:#}"))
                .0
                .to_bits(),
            expected.to_bits()
        );
        rotate.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
    }
}

#[test]
fn compiled_defn_preserves_docs_attributes_arities_and_lexical_shadowing() {
    let source = r#"
        (ns user (:require [cljs.core :as c]))
        (defmacro require-doc [name]
          (let [info (get (get (get &env :ns) :defs) name)]
            (if (= (get info :doc) "helper documentation")
              (if (get info :private) 42 (throw 18))
              (throw 17))))
        (c/defn helper "helper documentation" {:custom :leading}
          ([x] x)
          ([x y & more] (+ x y (count more)))
          {:private true})
        (defn ^:export calculate [x]
          (require-doc helper)
          (let [defn (fn [value] (+ value 1))]
            (+ (helper x) (helper 0 0 1 2) (defn 0))))
    "#;
    let bytes = Compiler::new()
        .compile(
            source,
            "package test:public-defn; world api { export calculate: func(x: f64) -> f64; }",
        )
        .unwrap();
    let engine = engine();
    let component = Component::new(&engine, bytes).unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(100_000_000).unwrap();
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let calculate = instance
        .get_typed_func::<(f64,), (f64,)>(&mut store, "calculate")
        .unwrap();
    for input in [41.0_f64, 0.0, -3.0] {
        assert_eq!(
            calculate.call(&mut store, (input,)).unwrap().0.to_bits(),
            (input + 3.0).to_bits()
        );
        calculate.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
    }
}

#[test]
fn source_macro_named_defn_shadows_automatic_core_macro() {
    let bytes = Compiler::new()
        .compile(
            "(defmacro defn [value] value) (def ^:export echo (fn [x] (defn (+ x 1))))",
            "package test:source-defn; world api { export echo: func(x: f64) -> f64; }",
        )
        .unwrap();
    let engine = engine();
    let component = Component::new(&engine, bytes).unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(100_000_000).unwrap();
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let echo = instance
        .get_typed_func::<(f64,), (f64,)>(&mut store, "echo")
        .unwrap();
    assert_eq!(
        echo.call(&mut store, (41.0,)).unwrap().0.to_bits(),
        42.0_f64.to_bits()
    );
    echo.post_return(&mut store).unwrap();
}

#[test]
fn excluding_core_defn_reports_unresolved_source_name() {
    let error = Compiler::new()
        .compile(
            "(ns user (:refer-clojure :exclude [defn])) (defn hidden [x] x)",
            "package test:excluded-defn; world api {}",
        )
        .unwrap_err();
    assert!(matches!(error, CompileError::Semantic(_)), "{error}");
    assert!(
        error.to_string().contains("Unresolved Runtime name defn"),
        "{error}"
    );
}
