//! Execute canonical signatures whose arguments exceed sixteen flat fields.
#![cfg(not(target_family = "wasm"))]

use std::sync::OnceLock;
use suss_compile::Compiler;
use wasmtime::{
    Config, Engine, ResourceLimiter, Store,
    component::{Component, Instance, Linker, Val},
};

#[derive(Default)]
struct Limits {
    canonical_peak: usize,
}
impl ResourceLimiter for Limits {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        _: Option<usize>,
    ) -> wasmtime::Result<bool> {
        // The existing numeric helper starts at17 pages; the GC heap is8MiB.
        // The separate canonical transfer memory must stay in its first page.
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

fn instantiate(source: &str, exports: &str) -> (Store<Limits>, Instance) {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    let engine = ENGINE.get_or_init(|| {
        let mut config = Config::new();
        config
            .wasm_gc(true)
            .gc_heap_initial_size(8 * 1024 * 1024)
            .wasm_function_references(true)
            .wasm_tail_call(true)
            .wasm_exceptions(true)
            .wasm_component_model(true)
            .wasm_component_model_async(true)
            .consume_fuel(true)
            .cranelift_opt_level(wasmtime::OptLevel::None);
        Engine::new(&config).unwrap()
    });
    let wit = format!("package test:indirect; world api {{ {exports} }}");
    let bytes = Compiler::new().compile(source, &wit).unwrap();
    let component = Component::new(engine, bytes).unwrap();
    assert_eq!(component.component_type().imports(engine).count(), 0);
    let mut store = Store::new(engine, Limits::default());
    store.limiter(|limits| limits);
    store.set_fuel(100_000_000).unwrap();
    let instance = Linker::new(engine)
        .instantiate(&mut store, &component)
        .unwrap();
    store.set_fuel(100_000_000).unwrap();
    (store, instance)
}

#[test]
fn wit_indirect_options_preserve_all_tags_payloads_and_alignment() {
    let (mut store, instance) = instantiate(
        "(defn ^:export values [a b c d e f g h i]
            [(if (= a [:none]) -1 (nth a 1))
             (if (= b [:none]) -1 (nth b 1))
             (if (= c [:none]) -1 (nth c 1))
             (if (= d [:none]) -1 (nth d 1))
             (if (= e [:none]) -1 (nth e 1))
             (if (= f [:none]) -1 (nth f 1))
             (if (= g [:none]) -1 (nth g 1))
             (if (= h [:none]) -1 (nth h 1))
             (if (= i [:none]) -1 (nth i 1))])",
        "export values: func(a: option<s64>, b: option<s64>, c: option<s64>,
             d: option<s64>, e: option<s64>, f: option<s64>, g: option<s64>,
             h: option<s64>, i: option<s64>) -> list<s64>;",
    );
    let function = instance.get_func(&mut store, "values").unwrap();
    for iteration in 0..10_000 {
        store.set_fuel(100_000_000).unwrap();
        let args = (0..9)
            .map(|i| {
                Val::Option(if (iteration + i) % 3 == 0 {
                    None
                } else {
                    Some(Box::new(Val::S64((1_i64 << 54) + iteration * 10 + i)))
                })
            })
            .collect::<Vec<_>>();
        let expected = Val::List(
            (0..9)
                .map(|i| {
                    Val::S64(if (iteration + i) % 3 == 0 {
                        -1
                    } else {
                        (1_i64 << 54) + iteration * 10 + i
                    })
                })
                .collect(),
        );
        let mut result = [Val::Bool(false)];
        function.call(&mut store, &args, &mut result).unwrap();
        assert_eq!(result[0], expected);
        function.post_return(&mut store).unwrap();
        if iteration % 128 == 0 {
            store.gc(None).unwrap();
        }
    }
    assert_eq!(store.data().canonical_peak, 65536);
}

fn complete<F: std::future::Future>(future: F) -> F::Output {
    use std::{
        pin::pin,
        sync::Arc,
        task::{Context, Poll, Wake, Waker},
    };
    struct WakeCurrent(std::thread::Thread);
    impl Wake for WakeCurrent {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(WakeCurrent(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "canonical async call timed out"
                );
                std::thread::park_timeout(std::time::Duration::from_millis(100));
            }
        }
    }
}

#[test]
fn wit_indirect_lists_support_core_conj_reduce_and_canonical_async_completion() {
    let params = "a: list<s32>, b: list<s32>, c: list<s32>, d: list<s32>, e: list<s32>, f: list<s32>, g: list<s32>, h: list<s32>, i: list<s32>";
    let (mut store, instance) = instantiate(
        "(defn ^:export append-count [a b c d e f g h i]
            (conj i (reduce + (map count [a b c d e f g h i]))))
         (defn ^:export async-append-count [a b c d e f g h i]
            (append-count a b c d e f g h i))",
        &format!(
            "export append-count: func({params}) -> list<s32>;
                  export async-append-count: async func({params}) -> list<s32>;"
        ),
    );
    let sync = instance.get_func(&mut store, "append-count").unwrap();
    let asynchronous = instance.get_func(&mut store, "async-append-count").unwrap();
    let args = (0..9)
        .map(|i| Val::List(vec![Val::S32(i); i as usize]))
        .collect::<Vec<_>>();
    let mut expected = vec![Val::S32(8); 8];
    expected.push(Val::S32(36));
    for _ in 0..32 {
        let mut results = [Val::Bool(false)];
        sync.call(&mut store, &args, &mut results).unwrap();
        assert_eq!(results[0], Val::List(expected.clone()));
        sync.post_return(&mut store).unwrap();
        complete(asynchronous.call_async(&mut store, &args, &mut results)).unwrap();
        assert_eq!(results[0], Val::List(expected.clone()));
        store.gc(None).unwrap();
    }
    assert_eq!(store.data().canonical_peak, 65536);
}

#[test]
fn wit_indirect_mixed_arguments_preserve_memory_layout_and_owned_values() {
    let (mut store, instance) = instantiate(
        "(def saved (atom nil))
         (defn ^:export check [a b c d e f g h i j k]
           (reset! saved [d g j])
           (and (= a 255) (= b [:some -0.5]) (= c \"🦀\")
                (= d \"left λ\") (= e -9007199254740993)
                (= f [:some 18446744073709551615N]) (= g [-32768 0 32767])
                (= h 1.5) (= i [:some \"𐀀\"]) (= j \"right\\u0000\") (= k 65535)))
         (defn ^:export recall [] [(nth @saved 0) (nth @saved 2)])",
        "export check: func(a: u8, b: option<f64>, c: char, d: string, e: s64,
             f: option<u64>, g: list<s16>, h: f32, i: option<char>, j: string, k: u16) -> bool;
         export recall: func() -> list<string>;",
    );
    let check = instance.get_func(&mut store, "check").unwrap();
    let args = [
        Val::U8(255),
        Val::Option(Some(Box::new(Val::Float64(-0.5)))),
        Val::Char('🦀'),
        Val::String("left λ".into()),
        Val::S64(-9007199254740993),
        Val::Option(Some(Box::new(Val::U64(u64::MAX)))),
        Val::List(vec![Val::S16(i16::MIN), Val::S16(0), Val::S16(i16::MAX)]),
        Val::Float32(1.5),
        Val::Option(Some(Box::new(Val::Char('\u{10000}')))),
        Val::String("right\0".into()),
        Val::U16(u16::MAX),
    ];
    let recall = instance
        .get_typed_func::<(), (Vec<String>,)>(&mut store, "recall")
        .unwrap();
    for _ in 0..32 {
        let mut result = [Val::Bool(false)];
        check.call(&mut store, &args, &mut result).unwrap();
        assert_eq!(result, [Val::Bool(true)]);
        check.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
        assert_eq!(
            recall.call(&mut store, ()).unwrap().0,
            ["left λ", "right\0"]
        );
        recall.post_return(&mut store).unwrap();
    }
}
