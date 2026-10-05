//! Execute Unicode WIT strings through owned UTF-16 source values.
#![cfg(not(target_family = "wasm"))]

use std::sync::OnceLock;
use suss_compile::Compiler;
use wasmtime::{
    AsContextMut, Config, Engine, ResourceLimiter, Store, StoreLimits, StoreLimitsBuilder,
    component::{Component, Instance, Linker},
};

struct BoundaryLimits {
    transfer: StoreLimits,
    numeric: StoreLimits,
}

impl ResourceLimiter for BoundaryLimits {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        // The shared numeric helper has an unbounded memory32 scratch memory
        // with a 17-page initial image. Wasmtime normalizes both memory maxima
        // to4GiB in limiter calls, so identify the checked numeric initial size
        // and the configured8MiB GC heap. Canonical memory starts at one page and cannot
        // grow beyond its independent128KiB transfer budget.
        if current >= 17 * 65536
            || (current == 0 && (desired == 17 * 65536 || desired == 8 * 1024 * 1024))
        {
            self.numeric.memory_growing(current, desired, maximum)
        } else {
            self.transfer.memory_growing(current, desired, maximum)
        }
    }

    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        self.numeric.table_growing(current, desired, maximum)
    }
}

fn instantiate(source: &str, exports: &str) -> (Store<BoundaryLimits>, Instance) {
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
    let wit = format!("package test:strings; world api {{ {exports} }}");
    let bytes = Compiler::new().compile(source, &wit).unwrap();
    let memories = wasmparser::Parser::new(0)
        .parse_all(&bytes)
        .filter_map(|payload| match payload.unwrap() {
            wasmparser::Payload::MemorySection(section) => Some(
                section
                    .into_iter()
                    .map(|ty| ty.unwrap().initial)
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(
        memories,
        [17, 1],
        "numeric scratch then canonical transfer memory"
    );
    let component = Component::new(engine, bytes).unwrap();
    assert_eq!(component.component_type().imports(engine).count(), 0);
    let limits = BoundaryLimits {
        transfer: StoreLimitsBuilder::new().memory_size(128 * 1024).build(),
        numeric: StoreLimitsBuilder::new()
            .memory_size(8 * 1024 * 1024)
            .build(),
    };
    let mut store = Store::new(engine, limits);
    store.limiter(|limits| limits);
    store.set_fuel(100_000_000).unwrap();
    let instance = Linker::new(engine)
        .instantiate(&mut store, &component)
        .unwrap();
    (store, instance)
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
fn strings_round_trip_unicode_and_mixed_flat_arguments() {
    let (mut store, instance) = instantiate(
        "(defn ^:export choose [a n b flag] (if flag a b)) (defn ^:export size [x] (count x))",
        "export choose: func(a: string, n: f64, b: string, flag: bool) -> string; export size: func(x: string) -> s32;",
    );
    let choose = instance
        .get_typed_func::<(&str, f64, &str, bool), (String,)>(&mut store, "choose")
        .unwrap();
    let size = instance
        .get_typed_func::<(&str,), (i32,)>(&mut store, "size")
        .unwrap();
    // Two simultaneously lowered inputs exceed one page; growth must preserve
    // the first allocation while the second is produced.
    let large = "x".repeat(20_000);
    let unicode = "λ".repeat(10_000);
    for (a, b) in [
        ("", "😀"),
        ("a\0b", "e\u{301}"),
        ("λ漢字", "💩𐐷"),
        (large.as_str(), unicode.as_str()),
    ] {
        for flag in [false, true] {
            assert_eq!(
                choose.call(&mut store, (a, 42.5, b, flag)).unwrap(),
                (if flag { a } else { b }.to_owned(),)
            );
            choose.post_return(&mut store).unwrap();
            store.gc(None).unwrap();
        }
        assert_eq!(
            size.call(&mut store, (b,)).unwrap(),
            (b.encode_utf16().count() as i32,)
        );
        size.post_return(&mut store).unwrap();
    }
}

#[test]
fn incoming_strings_are_owned_after_transfer_and_survive_gc_and_buffer_reuse() {
    let source = "(def saved (atom \"initial\")) (defn ^:export remember [x] (do (reset! saved x) x)) (defn ^:export recall [] (deref saved))";
    let exports = "export remember: func(x: string) -> string; export recall: func() -> string;";
    for _ in 0..2 {
        let (mut store, instance) = instantiate(source, exports);
        let remember = instance
            .get_typed_func::<(&str,), (String,)>(&mut store, "remember")
            .unwrap();
        let recall = instance
            .get_typed_func::<(), (String,)>(&mut store, "recall")
            .unwrap();
        for value in [
            "😀λ\0e\u{301}".to_owned(),
            "x".repeat(8192),
            "".to_owned(),
            "𐐷".repeat(1024),
        ] {
            assert_eq!(
                remember.call(&mut store, (value.as_str(),)).unwrap(),
                (value.clone(),)
            );
            remember.post_return(&mut store).unwrap();
            store.gc(None).unwrap();
            // Aggregate returns exceed the memory limit unless post-return frees
            // or reuses transfer storage; a bump-only allocator cannot pass.
            for _ in 0..20 {
                assert_eq!(recall.call(&mut store, ()).unwrap(), (value.clone(),));
                recall.post_return(&mut store).unwrap();
                store.gc(None).unwrap();
            }
        }
    }
}

#[test]
fn asynchronous_string_completion_preserves_unicode_and_repeated_calls() {
    let (mut store, instance) = instantiate(
        "(defn ^:export echo [x] x)",
        "export echo: async func(x: string) -> string;",
    );
    let echo = instance
        .get_typed_func::<(&str,), (String,)>(&mut store, "echo")
        .unwrap();
    assert!(echo.func().ty(&store).async_());
    for value in ["", "a\0b", "😀λ漢字e\u{301}"] {
        assert_eq!(
            complete(echo.call_async(&mut store, (value,))).unwrap(),
            (value.to_owned(),)
        );
        store.gc(None).unwrap();
    }
}

#[test]
fn outgoing_strings_reject_lone_surrogates_and_wrong_source_types() {
    for literal in [
        r#""\uD800""#,
        r#""\uDC00""#,
        r#""x\uD800y""#,
        "nil",
        "false",
        "42",
        "[]",
    ] {
        let source = format!("(defn ^:export invalid [] {literal})");
        let (mut store, instance) = instantiate(&source, "export invalid: func() -> string;");
        let invalid = instance
            .get_typed_func::<(), (String,)>(&mut store, "invalid")
            .unwrap();
        assert!(invalid.call(&mut store, ()).is_err(), "accepted {literal}");
        let exception = store
            .as_context_mut()
            .take_pending_exception()
            .expect("boundary language exception, not canonical Unicode or cast trap");
        let fields = exception.fields(&mut store).unwrap().collect::<Vec<_>>();
        assert_eq!(fields.len(), 1);
        let payload = fields[0]
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
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
        let message = String::from_utf16(&units).unwrap();
        assert!(message.contains("WIT export invalid"), "{message}");
        assert!(
            message.contains("string") || message.contains("surrogate"),
            "{message}"
        );
    }
}

#[test]
fn eight_string_parameters_preserve_last_payload_at_flat_signature_limit() {
    let (mut store, instance) = instantiate(
        "(defn ^:export last-string [a b c d e f g h] h)",
        "export last-string: func(a: string, b: string, c: string, d: string, e: string, f: string, g: string, h: string) -> string;",
    );
    let function = instance
        .get_typed_func::<(&str, &str, &str, &str, &str, &str, &str, &str), (String,)>(
            &mut store,
            "last-string",
        )
        .unwrap();
    for last in ["", "😀\0λ"] {
        assert_eq!(
            function
                .call(&mut store, ("a", "b", "c", "d", "e", "f", "g", last))
                .unwrap(),
            (last.to_owned(),)
        );
        function.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
    }
}

#[test]
fn unsupported_indirect_string_signatures_fail_before_macro_effects() {
    let source = "(defmacro fail [] (throw 17)) (defn ^:export value [& xs] (fail))";
    for async_flag in ["", "async "] {
        let wit = format!(
            "package test:strings; world api {{ export value: {async_flag}func(a: string, b: string, c: string, d: string, e: string, f: string, g: string, h: string, i: string) -> string; }}"
        );
        let error = Compiler::new().compile(source, &wit).unwrap_err();
        assert!(
            matches!(error, suss_compile::CompileError::Unsupported(_)),
            "{error}"
        );
        assert!(error.to_string().contains("indirect parameter"), "{error}");
    }
}

#[test]
fn string_arguments_preserve_caught_payloads_and_once_only_finally() {
    let (mut store, instance) = instantiate(
        "(def cleanups (atom 0)) (defn ^:export caught [x] (try (throw x) (catch :default payload payload) (finally (reset! cleanups (+ (deref cleanups) 1))))) (defn ^:export cleanup-count [] (deref cleanups))",
        "export caught: func(x: string) -> string; export cleanup-count: func() -> s32;",
    );
    let caught = instance
        .get_typed_func::<(&str,), (String,)>(&mut store, "caught")
        .unwrap();
    let count = instance
        .get_typed_func::<(), (i32,)>(&mut store, "cleanup-count")
        .unwrap();
    for (index, value) in ["", "😀\0λ", "e\u{301}", "漢字"].into_iter().enumerate() {
        assert_eq!(
            caught.call(&mut store, (value,)).unwrap(),
            (value.to_owned(),)
        );
        caught.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
        assert_eq!(count.call(&mut store, ()).unwrap(), (index as i32 + 1,));
        count.post_return(&mut store).unwrap();
    }
}

#[test]
fn mixed_async_string_option_and_scalar_exports_preserve_indices_and_lifetimes() {
    let (mut store, instance) = instantiate(
        "(defn ^:export text [x] x) (defn ^:export other [x] x) (defn ^:export sync-text [x] x) (defn ^:export flag [x] x) (defn ^:export twice [x] (* x 2))",
        "export text: async func(x: string) -> string; export other: async func(x: string) -> string; export sync-text: func(x: string) -> string; export flag: async func(x: option<bool>) -> option<bool>; export twice: async func(x: s32) -> s32;",
    );
    let text = instance
        .get_typed_func::<(&str,), (String,)>(&mut store, "text")
        .unwrap();
    let other = instance
        .get_typed_func::<(&str,), (String,)>(&mut store, "other")
        .unwrap();
    let sync = instance
        .get_typed_func::<(&str,), (String,)>(&mut store, "sync-text")
        .unwrap();
    let flag = instance
        .get_typed_func::<(Option<bool>,), (Option<bool>,)>(&mut store, "flag")
        .unwrap();
    let twice = instance
        .get_typed_func::<(i32,), (i32,)>(&mut store, "twice")
        .unwrap();
    for value in ["😀\0λ", "", "漢字e\u{301}"] {
        assert_eq!(
            complete(text.call_async(&mut store, (value,))).unwrap(),
            (value.to_owned(),)
        );
        assert_eq!(
            complete(flag.call_async(&mut store, (Some(false),))).unwrap(),
            (Some(false),)
        );
        assert_eq!(
            complete(other.call_async(&mut store, (value,))).unwrap(),
            (value.to_owned(),)
        );
        assert_eq!(
            sync.call(&mut store, (value,)).unwrap(),
            (value.to_owned(),)
        );
        sync.post_return(&mut store).unwrap();
        assert_eq!(
            complete(twice.call_async(&mut store, (21,))).unwrap(),
            (42,)
        );
        assert_eq!(
            complete(flag.call_async(&mut store, (None,))).unwrap(),
            (None,)
        );
        store.gc(None).unwrap();
    }
}

#[test]
fn async_string_parameters_with_scalar_and_void_results_match_completion_encoding() {
    let (mut store, instance) = instantiate(
        "(def saved (atom \"initial\")) (defn ^:export remember [x] (reset! saved x)) (defn ^:export units [x] (count x)) (defn ^:export recall [] (deref saved))",
        "export remember: async func(x: string); export units: async func(x: string) -> s32; export recall: func() -> string;",
    );
    let remember = instance
        .get_typed_func::<(&str,), ()>(&mut store, "remember")
        .unwrap();
    let units = instance
        .get_typed_func::<(&str,), (i32,)>(&mut store, "units")
        .unwrap();
    let recall = instance
        .get_typed_func::<(), (String,)>(&mut store, "recall")
        .unwrap();
    for value in ["", "😀\0λ", "e\u{301}"] {
        complete(remember.call_async(&mut store, (value,))).unwrap();
        assert_eq!(
            complete(units.call_async(&mut store, (value,))).unwrap(),
            (value.encode_utf16().count() as i32,)
        );
        store.gc(None).unwrap();
        assert_eq!(recall.call(&mut store, ()).unwrap(), (value.to_owned(),));
        recall.post_return(&mut store).unwrap();
    }
}

#[test]
fn tagged_string_options_distinguish_none_empty_and_owned_unicode_payloads() {
    let (mut store, instance) = instantiate(
        "(defn ^:export echo-option [x] x) (defn ^:export describe [x] (if (= (nth x 0) :none) \"missing\" (nth x 1)))",
        "export echo-option: func(x: option<string>) -> option<string>; export describe: func(x: option<string>) -> string;",
    );
    let echo = instance
        .get_typed_func::<(Option<&str>,), (Option<String>,)>(&mut store, "echo-option")
        .unwrap();
    let describe = instance
        .get_typed_func::<(Option<&str>,), (String,)>(&mut store, "describe")
        .unwrap();
    for value in [None, Some(""), Some("😀\0λ"), None, Some("漢字")] {
        assert_eq!(
            echo.call(&mut store, (value,)).unwrap(),
            (value.map(str::to_owned),)
        );
        echo.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
        assert_eq!(
            describe.call(&mut store, (value,)).unwrap(),
            (value.unwrap_or("missing").to_owned(),)
        );
        describe.post_return(&mut store).unwrap();
    }
}

#[test]
fn async_tagged_string_options_release_payloads_after_completion() {
    let (mut store, instance) = instantiate(
        "(defn ^:export echo-option [x] x)",
        "export echo-option: async func(x: option<string>) -> option<string>;",
    );
    let echo = instance
        .get_typed_func::<(Option<&str>,), (Option<String>,)>(&mut store, "echo-option")
        .unwrap();
    let large = "λ".repeat(8_192);
    for _ in 0..20 {
        for value in [Some(large.as_str()), None, Some(""), Some("😀\0λ")] {
            assert_eq!(
                complete(echo.call_async(&mut store, (value,))).unwrap(),
                (value.map(str::to_owned),)
            );
            store.gc(None).unwrap();
        }
    }
}
