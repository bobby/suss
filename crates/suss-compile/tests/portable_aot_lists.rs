//! Execute resolved WIT lists through owned persistent vector values.
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
    let wit = format!("package test:lists; world api {{ {exports} }}");
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
    // Large vector adapters currently use the universal source closure ABI.
    // A measured 1057-element call costs 199101049 fuel; bound each test call
    // separately so repeated cleanup assertions retain the same transfer budget.
    store.set_fuel(500_000_000).unwrap();
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
fn lists_preserve_checked_scalar_values_and_mixed_parameter_order() {
    let (mut store, instance) = instantiate(
        "(defn ^:export echo [a marker b flag] (if flag a b)) (defn ^:export count-list [xs] (count xs)) (defn ^:export unsigned [xs] xs) (defn ^:export booleans [xs] xs) (defn ^:export floats [xs] xs) (defn ^:export u8s [xs] xs) (defn ^:export s8s [xs] xs) (defn ^:export u16s [xs] xs) (defn ^:export s16s [xs] xs) (defn ^:export f32s [xs] xs)",
        "export echo: func(a: list<s32>, marker: f64, b: list<s32>, flag: bool) -> list<s32>; export count-list: func(xs: list<s32>) -> s32; export unsigned: func(xs: list<u32>) -> list<u32>; export booleans: func(xs: list<bool>) -> list<bool>; export floats: func(xs: list<f64>) -> list<f64>; export u8s: func(xs: list<u8>) -> list<u8>; export s8s: func(xs: list<s8>) -> list<s8>; export u16s: func(xs: list<u16>) -> list<u16>; export s16s: func(xs: list<s16>) -> list<s16>; export f32s: func(xs: list<f32>) -> list<f32>;",
    );
    let echo = instance
        .get_typed_func::<(&[i32], f64, &[i32], bool), (Vec<i32>,)>(&mut store, "echo")
        .unwrap();
    let count = instance
        .get_typed_func::<(&[i32],), (i32,)>(&mut store, "count-list")
        .unwrap();
    for (a, b) in [
        (vec![], vec![i32::MIN, 0, i32::MAX]),
        (vec![42, -1], vec![]),
    ] {
        for flag in [false, true] {
            assert_eq!(
                echo.call(&mut store, (&a, 42.5, &b, flag)).unwrap().0,
                if flag { a.clone() } else { b.clone() }
            );
            echo.post_return(&mut store).unwrap();
            store.gc(None).unwrap();
        }
        assert_eq!(count.call(&mut store, (&b,)).unwrap(), (b.len() as i32,));
        count.post_return(&mut store).unwrap();
    }
    let unsigned = instance
        .get_typed_func::<(&[u32],), (Vec<u32>,)>(&mut store, "unsigned")
        .unwrap();
    let values = [0, u32::MAX, 0x8000_0000];
    assert_eq!(unsigned.call(&mut store, (&values,)).unwrap().0, values);
    unsigned.post_return(&mut store).unwrap();
    let booleans = instance
        .get_typed_func::<(&[bool],), (Vec<bool>,)>(&mut store, "booleans")
        .unwrap();
    assert_eq!(
        booleans
            .call(&mut store, (&[false, true, false],))
            .unwrap()
            .0,
        [false, true, false]
    );
    booleans.post_return(&mut store).unwrap();
    let floats = instance
        .get_typed_func::<(&[f64],), (Vec<f64>,)>(&mut store, "floats")
        .unwrap();
    let values = [
        -0.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::from_bits(0x7ff8_1234_5678_9abc),
    ];
    let actual = floats.call(&mut store, (&values,)).unwrap().0;
    assert_eq!(
        actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        values.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
    );
    floats.post_return(&mut store).unwrap();
    macro_rules! integer_list {
        ($name:literal, $ty:ty, $values:expr) => {{
            let function = instance
                .get_typed_func::<(&[$ty],), (Vec<$ty>,)>(&mut store, $name)
                .unwrap();
            let values = $values;
            assert_eq!(function.call(&mut store, (&values,)).unwrap().0, values);
            function.post_return(&mut store).unwrap();
        }};
    }
    integer_list!("u8s", u8, [0, 128, u8::MAX]);
    integer_list!("s8s", i8, [i8::MIN, -1, 0, i8::MAX]);
    integer_list!("u16s", u16, [0, 32768, u16::MAX]);
    integer_list!("s16s", i16, [i16::MIN, -1, 0, i16::MAX]);
    let floats32 = instance
        .get_typed_func::<(&[f32],), (Vec<f32>,)>(&mut store, "f32s")
        .unwrap();
    let values = [-0.0, f32::MIN, f32::MAX, f32::INFINITY, f32::NEG_INFINITY];
    let actual = floats32.call(&mut store, (&values,)).unwrap().0;
    assert_eq!(
        actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        values.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
    );
    floats32.post_return(&mut store).unwrap();
}

#[test]
fn incoming_lists_build_real_vector_tries_and_retain_owned_values_after_gc() {
    let (mut store, instance) = instantiate(
        "(def saved (atom [])) (defn ^:export remember [xs] (do (reset! saved xs) xs)) (defn ^:export recall [] (deref saved)) (defn ^:export at [index] (nth (deref saved) index)) (defn ^:export is-vector [] (vector? (deref saved)))",
        "export remember: func(xs: list<s32>) -> list<s32>; export recall: func() -> list<s32>; export at: func(index: s32) -> s32; export is-vector: func() -> bool;",
    );
    let remember = instance
        .get_typed_func::<(&[i32],), (Vec<i32>,)>(&mut store, "remember")
        .unwrap();
    let recall = instance
        .get_typed_func::<(), (Vec<i32>,)>(&mut store, "recall")
        .unwrap();
    let at = instance
        .get_typed_func::<(i32,), (i32,)>(&mut store, "at")
        .unwrap();
    let is_vector = instance
        .get_typed_func::<(), (bool,)>(&mut store, "is-vector")
        .unwrap();
    for length in [0, 1, 31, 32, 33, 1024, 1056, 1057, 2048] {
        store.set_fuel(500_000_000).unwrap();
        let values = (0..length).map(|n| n * 7 - 42).collect::<Vec<i32>>();
        assert_eq!(remember.call(&mut store, (&values,)).unwrap().0, values);
        remember.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
        assert_eq!(is_vector.call(&mut store, ()).unwrap(), (true,));
        is_vector.post_return(&mut store).unwrap();
        for index in [0, 31, 32, 1023, 1024, length - 1] {
            if index >= 0 && index < length {
                assert_eq!(
                    at.call(&mut store, (index,)).unwrap(),
                    (values[index as usize],)
                );
                at.post_return(&mut store).unwrap();
            }
        }
        for _ in 0..20 {
            store.set_fuel(500_000_000).unwrap();
            assert_eq!(recall.call(&mut store, ()).unwrap().0, values);
            recall.post_return(&mut store).unwrap();
            store.gc(None).unwrap();
        }
    }
}

#[test]
fn list_results_accept_persistent_vectors_and_subvector_views() {
    let (mut store, instance) = instantiate(
        "(defn ^:export values [] [1 2 3]) (defn ^:export nested [] (new suss.core/Subvec nil (new suss.core/Subvec nil [0 1 2 3] 1 4 nil) 1 2 nil)) (defn ^:export view [xs start end] (subvec xs start end))",
        "export values: func() -> list<s32>; export nested: func() -> list<s32>; export view: func(xs: list<s32>, start: s32, end: s32) -> list<s32>;",
    );
    let values = instance
        .get_typed_func::<(), (Vec<i32>,)>(&mut store, "values")
        .unwrap();
    assert_eq!(values.call(&mut store, ()).unwrap().0, [1, 2, 3]);
    values.post_return(&mut store).unwrap();
    let nested = instance
        .get_typed_func::<(), (Vec<i32>,)>(&mut store, "nested")
        .unwrap();
    assert_eq!(nested.call(&mut store, ()).unwrap().0, [2]);
    nested.post_return(&mut store).unwrap();
    let view = instance
        .get_typed_func::<(&[i32], i32, i32), (Vec<i32>,)>(&mut store, "view")
        .unwrap();
    let input = (0..1057).collect::<Vec<i32>>();
    for (start, end) in [(0, 0), (31, 34), (1023, 1057), (0, 1057)] {
        store.set_fuel(500_000_000).unwrap();
        assert_eq!(
            view.call(&mut store, (&input, start, end)).unwrap().0,
            input[start as usize..end as usize]
        );
        view.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
    }
}

#[test]
fn string_list_payloads_are_owned_unicode_and_cleanup_nested_buffers() {
    let (mut store, instance) = instantiate(
        "(def saved (atom [])) (defn ^:export echo [xs] (do (reset! saved xs) xs)) (defn ^:export recall [] (deref saved))",
        "export echo: func(xs: list<string>) -> list<string>; export recall: func() -> list<string>;",
    );
    let echo = instance
        .get_typed_func::<(&[String],), (Vec<String>,)>(&mut store, "echo")
        .unwrap();
    let recall = instance
        .get_typed_func::<(), (Vec<String>,)>(&mut store, "recall")
        .unwrap();
    for input in [
        vec![],
        vec!["".into(), "😀λ\0e\u{301}".into(), "漢字".into()],
        vec!["λ".repeat(4096); 3],
    ] {
        assert_eq!(echo.call(&mut store, (&input,)).unwrap().0, input);
        echo.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
        for _ in 0..20 {
            store.set_fuel(500_000_000).unwrap();
            assert_eq!(recall.call(&mut store, ()).unwrap().0, input);
            recall.post_return(&mut store).unwrap();
            store.gc(None).unwrap();
        }
    }
}

#[test]
fn asynchronous_lists_preserve_completion_indices_and_nested_string_cleanup() {
    let (mut store, instance) = instantiate(
        "(defn ^:export integers [xs] xs) (defn ^:export strings [xs] xs) (defn ^:export scalar [n] (+ n 1))",
        "export integers: async func(xs: list<s32>) -> list<s32>; export strings: async func(xs: list<string>) -> list<string>; export scalar: async func(n: s32) -> s32;",
    );
    let integers = instance
        .get_typed_func::<(&[i32],), (Vec<i32>,)>(&mut store, "integers")
        .unwrap();
    let strings = instance
        .get_typed_func::<(&[String],), (Vec<String>,)>(&mut store, "strings")
        .unwrap();
    let scalar = instance
        .get_typed_func::<(i32,), (i32,)>(&mut store, "scalar")
        .unwrap();
    let ints = (0..1057).collect::<Vec<i32>>();
    let text = vec!["λ".repeat(4096), "😀\0漢字".into(), "".into()];
    for n in 0..20 {
        store.set_fuel(500_000_000).unwrap();
        assert_eq!(
            complete(integers.call_async(&mut store, (&ints,)))
                .unwrap()
                .0,
            ints
        );
        assert_eq!(
            complete(strings.call_async(&mut store, (&text,)))
                .unwrap()
                .0,
            text
        );
        assert_eq!(
            complete(scalar.call_async(&mut store, (n,))).unwrap(),
            (n + 1,)
        );
        store.gc(None).unwrap();
    }
}

#[test]
fn malformed_list_shapes_and_elements_raise_decoded_language_errors() {
    for (payload, ty) in [
        ("nil", "s32"),
        ("false", "s32"),
        ("42", "s32"),
        ("(list 1 2)", "s32"),
        ("[1 2.5]", "s32"),
        ("[-1]", "u32"),
        ("[256]", "u8"),
        ("[nil]", "bool"),
        ("[42]", "string"),
        (r#"["ok" "\uD800"]"#, "string"),
        (
            r#"(new suss.core/PersistentVector nil 1 5 (.-EMPTY_NODE suss.core/PersistentVector) "x" nil)"#,
            "string",
        ),
        (
            "(new suss.core/PersistentVector nil 33 5 (new suss.core/VectorNode nil nil) (array 1) nil)",
            "s32",
        ),
        ("(new suss.core/Subvec nil [1] 0 2 nil)", "s32"),
        (
            "(new suss.core/Subvec nil (new suss.core/Subvec nil [1] 0 2 nil) 0 1 nil)",
            "s32",
        ),
        (
            "(new suss.core/Subvec nil (new suss.core/Subvec nil (new suss.core/Subvec nil [1 2] 0 1 nil) 0 2 nil) 0 1 nil)",
            "s32",
        ),
    ] {
        let source = format!("(defn ^:export bad [] {payload})");
        let (mut store, instance) =
            instantiate(&source, &format!("export bad: func() -> list<{ty}>;"));
        let bad = instance.get_func(&mut store, "bad").unwrap();
        let mut result = [wasmtime::component::Val::Bool(false)];
        assert!(
            bad.call(&mut store, &[], &mut result).is_err(),
            "accepted {payload}"
        );
        let exception = store
            .as_context_mut()
            .take_pending_exception()
            .expect("language exception, not cast/canonical trap");
        let fields = exception.fields(&mut store).unwrap().collect::<Vec<_>>();
        let payload = fields[0].unwrap_anyref().unwrap();
        let array = if let Some(message) = payload.as_array(&store).unwrap() {
            // The private compiled schema throws a source UTF-16 diagnostic.
            message
        } else {
            // Checked scalar/string lowering throws the shared LanguageError.
            let object = payload.as_struct(&store).unwrap().unwrap();
            let message = object.field(&mut store, 1).unwrap();
            message
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)
                .unwrap()
                .unwrap()
        };
        let units = array
            .elems(&mut store)
            .unwrap()
            .map(|v| v.unwrap_i32() as u16)
            .collect::<Vec<_>>();
        let message = String::from_utf16(&units).unwrap();
        assert!(
            message.contains("WIT") || message.contains("list"),
            "{message}"
        );
    }
}

#[test]
fn list_aliases_and_protocol_calls_use_the_resolved_type_and_real_source_vector() {
    let (mut store, instance) = instantiate(
        "(defn ^:export first-of [xs] (-first (seq xs))) (defn ^:export echo [xs] xs)",
        "type values = list<s32>; type words = list<string>; export first-of: func(xs: values) -> s32; export echo: func(xs: words) -> words;",
    );
    let first = instance
        .get_typed_func::<(&[i32],), (i32,)>(&mut store, "first-of")
        .unwrap();
    assert_eq!(first.call(&mut store, (&[42, -1, 7],)).unwrap(), (42,));
    first.post_return(&mut store).unwrap();
    let echo = instance
        .get_typed_func::<(&[String],), (Vec<String>,)>(&mut store, "echo")
        .unwrap();
    let input = vec!["😀λ".into(), "".into()];
    assert_eq!(echo.call(&mut store, (&input,)).unwrap().0, input);
    echo.post_return(&mut store).unwrap();
}

#[test]
fn indirect_list_signatures_reject_before_effectful_macros() {
    for asynchronous in ["", "async "] {
        let wit = format!(
            "package test:lists; world api {{ export value: {asynchronous}func(a: list<s32>, b: list<s32>, c: list<s32>, d: list<s32>, e: list<s32>, f: list<s32>, g: list<s32>, h: list<s32>, i: list<s32>) -> list<s32>; }}"
        );
        let error = Compiler::new()
            .compile(
                "(defmacro fail [] (throw 17)) (defn ^:export value [& xs] (fail))",
                &wit,
            )
            .unwrap_err()
            .to_string();
        assert!(error.contains("indirect parameter"), "{error}");
        assert!(!error.contains("17"), "source macro executed: {error}");
    }
}

#[test]
fn list_schema_retains_core_nominal_identity_after_user_redefinition() {
    let source = "(defn ^:export echo [xs] xs) (defn ^:export bad [] nil) (ns suss.core) (def count (fn* [x] 0)) (def nth (fn* [x index] 99)) (def vector? (fn* [x] true)) (def PersistentVector nil) (def Subvec nil) (ns user)";
    let exports = "export echo: func(xs: list<s32>) -> list<s32>; export bad: func() -> list<s32>;";
    let (mut store, instance) = instantiate(source, exports);
    let echo = instance
        .get_typed_func::<(&[i32],), (Vec<i32>,)>(&mut store, "echo")
        .unwrap();
    let input = (0..1057).collect::<Vec<i32>>();
    assert_eq!(echo.call(&mut store, (&input,)).unwrap().0, input);
    echo.post_return(&mut store).unwrap();
    store.gc(None).unwrap();
    let (mut store, instance) = instantiate(source, exports);
    let bad = instance
        .get_typed_func::<(), (Vec<i32>,)>(&mut store, "bad")
        .unwrap();
    assert!(bad.call(&mut store, ()).is_err());
    assert!(
        store.as_context_mut().take_pending_exception().is_some(),
        "schema language exception required"
    );
}

#[test]
fn eight_list_parameters_preserve_the_last_flattened_payload() {
    let (mut store, instance) = instantiate(
        "(defn ^:export last-list [a b c d e f g h] h)",
        "export last-list: func(a: list<s32>, b: list<s32>, c: list<s32>, d: list<s32>, e: list<s32>, f: list<s32>, g: list<s32>, h: list<s32>) -> list<s32>;",
    );
    let last = instance
        .get_typed_func::<(
            &[i32],
            &[i32],
            &[i32],
            &[i32],
            &[i32],
            &[i32],
            &[i32],
            &[i32],
        ), (Vec<i32>,)>(&mut store, "last-list")
        .unwrap();
    for expected in [vec![], vec![i32::MIN, 42, i32::MAX]] {
        assert_eq!(
            last.call(
                &mut store,
                (&[1], &[2], &[3], &[4], &[5], &[6], &[7], &expected)
            )
            .unwrap()
            .0,
            expected
        );
        last.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
    }
}
