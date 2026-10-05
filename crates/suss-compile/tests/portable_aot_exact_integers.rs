//! Execute exact signed/unsigned WIT integers without binary64 loss.
#![cfg(not(target_family = "wasm"))]

use std::sync::OnceLock;
use suss_compile::Compiler;
use wasmtime::{
    component::{Component, Instance, Linker},
    AsContextMut, Config, Engine, ResourceLimiter, Store, StoreLimits, StoreLimitsBuilder,
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
    let wit = format!("package test:exact-integers; world api {{ {exports} }}");
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
    let expected_memories: &[u64] = if exports.contains("string") || exports.contains("list<") {
        &[17, 1]
    } else {
        &[17]
    };
    assert_eq!(
        memories, expected_memories,
        "only required numeric/canonical memories"
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
    store.set_fuel(100_000_000).unwrap();
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
fn full_width_signed_and_unsigned_inputs_remain_exact_after_gc() {
    let (mut store, instance) = instantiate(
        "(def saved (atom nil)) (defn ^:export signed [x] (do (reset! saved x) x)) (defn ^:export recall [] (deref saved)) (defn ^:export unsigned [x] x)",
        "export signed: func(x: s64) -> s64; export recall: func() -> s64; export unsigned: func(x: u64) -> u64;",
    );
    let signed = instance
        .get_typed_func::<(i64,), (i64,)>(&mut store, "signed")
        .unwrap();
    let recall = instance
        .get_typed_func::<(), (i64,)>(&mut store, "recall")
        .unwrap();
    let unsigned = instance
        .get_typed_func::<(u64,), (u64,)>(&mut store, "unsigned")
        .unwrap();
    for value in [
        i64::MIN,
        i64::MIN + 1,
        -9007199254740993,
        -1,
        0,
        1,
        9007199254740993,
        i64::MAX - 1,
        i64::MAX,
    ] {
        assert_eq!(signed.call(&mut store, (value,)).unwrap(), (value,));
        signed.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
        assert_eq!(recall.call(&mut store, ()).unwrap(), (value,));
        recall.post_return(&mut store).unwrap();
    }
    let mut bits = 0x9e3779b97f4a7c15u64;
    for _ in 0..128 {
        bits ^= bits << 13;
        bits ^= bits >> 7;
        bits ^= bits << 17;
        assert_eq!(unsigned.call(&mut store, (bits,)).unwrap(), (bits,));
        unsigned.post_return(&mut store).unwrap();
        assert_eq!(
            signed.call(&mut store, (bits as i64,)).unwrap(),
            (bits as i64,)
        );
        signed.post_return(&mut store).unwrap();
    }
}

#[test]
fn decimal_constructors_cover_full_ranges_without_number_parsing() {
    let (mut store, instance) = instantiate(
        "(defn ^:export signed [text] (suss.core/wit-s64 text)) (defn ^:export unsigned [text] (suss.core/wit-u64 text))",
        "export signed: func(text: string) -> s64; export unsigned: func(text: string) -> u64;",
    );
    let signed = instance
        .get_typed_func::<(&str,), (i64,)>(&mut store, "signed")
        .unwrap();
    let unsigned = instance
        .get_typed_func::<(&str,), (u64,)>(&mut store, "unsigned")
        .unwrap();
    for (text, value) in [
        ("-9223372036854775808", i64::MIN),
        ("9223372036854775807", i64::MAX),
        ("9007199254740993", 9007199254740993),
        ("-9007199254740993", -9007199254740993),
        ("-0", 0),
        ("00042", 42),
    ] {
        assert_eq!(signed.call(&mut store, (text,)).unwrap(), (value,));
        signed.post_return(&mut store).unwrap();
    }
    for (text, value) in [
        ("18446744073709551615", u64::MAX),
        ("9223372036854775808", 1u64 << 63),
        ("00042", 42),
        ("0", 0),
    ] {
        assert_eq!(unsigned.call(&mut store, (text,)).unwrap(), (value,));
        unsigned.post_return(&mut store).unwrap();
    }
}

#[test]
fn wrappers_are_distinct_and_checked_to_number_preserves_safe_values() {
    let (mut store, instance) = instantiate(
        "(defn ^:export signed [x] (if (if (suss.core/wit-s64? x) (not (number? x)) false) (suss.core/wit-to-number x) 0)) (defn ^:export unsigned [x] (if (suss.core/wit-u64? x) (suss.core/wit-to-number x) 0))",
        "export signed: func(x: s64) -> f64; export unsigned: func(x: u64) -> f64;",
    );
    let signed = instance
        .get_typed_func::<(i64,), (f64,)>(&mut store, "signed")
        .unwrap();
    let unsigned = instance
        .get_typed_func::<(u64,), (f64,)>(&mut store, "unsigned")
        .unwrap();
    for value in [-9007199254740991, -42, 0, 42, 9007199254740991] {
        assert_eq!(signed.call(&mut store, (value,)).unwrap(), (value as f64,));
        signed.post_return(&mut store).unwrap();
    }
    assert_eq!(
        unsigned.call(&mut store, (9007199254740991,)).unwrap(),
        (9007199254740991.0,)
    );
    unsigned.post_return(&mut store).unwrap();
}

fn assert_language_error(
    mut store: Store<BoundaryLimits>,
    instance: Instance,
    args: &[wasmtime::component::Val],
) {
    let function = instance.get_func(&mut store, "bad").unwrap();
    let mut results = [wasmtime::component::Val::Bool(false)];
    assert!(function.call(&mut store, args, &mut results).is_err());
    let exception = store
        .as_context_mut()
        .take_pending_exception()
        .expect("language exception, not canonical or cast trap");
    let fields = exception.fields(&mut store).unwrap().collect::<Vec<_>>();
    let value = fields[0].unwrap_anyref().unwrap();
    let array = if let Some(array) = value.as_array(&store).unwrap() {
        array
    } else {
        let object = value.as_struct(&store).unwrap().unwrap();
        object
            .field(&mut store, 1)
            .unwrap()
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
        message.contains("64") || message.contains("exact integer"),
        "{message}"
    );
}

#[test]
fn result_validation_rejects_numbers_wrong_wrappers_and_malformed_words() {
    for (body, ty) in [
        ("42", "s64"),
        ("nil", "u64"),
        ("(suss.core/wit-u64 \"42\")", "s64"),
        ("(suss.core/wit-s64 \"42\")", "u64"),
        ("(new suss.core/WitSigned64 0 1.5)", "s64"),
        ("(new suss.core/WitUnsigned64 -1 0)", "u64"),
        ("(new suss.core/WitSigned64 0 4294967296)", "s64"),
        ("(new suss.core/WitUnsigned64 nil 0)", "u64"),
        ("(new suss.core/WitSigned64 ##NaN 0)", "s64"),
        ("(new suss.core/WitUnsigned64 0 ##Inf)", "u64"),
        ("(new suss.core/WitSigned64 true 0)", "s64"),
        ("(suss.core/wit-s64 42)", "s64"),
    ] {
        let (store, instance) = instantiate(
            &format!("(defn ^:export bad [] {body})"),
            &format!("export bad: func() -> {ty};"),
        );
        assert_language_error(store, instance, &[]);
    }
}

#[test]
fn invalid_decimal_input_and_range_overflow_are_language_errors() {
    for (text, ty) in [
        ("", "s64"),
        ("+1", "s64"),
        ("--1", "s64"),
        ("1.5", "s64"),
        (" 1", "s64"),
        ("1 ", "u64"),
        ("1e3", "u64"),
        ("１２", "s64"),
        ("-1", "u64"),
        ("18446744073709551616", "u64"),
        ("9223372036854775808", "s64"),
        ("-9223372036854775809", "s64"),
    ] {
        let (store, instance) = instantiate(
            &format!("(defn ^:export bad [text] (suss.core/wit-{ty} text))"),
            &format!("export bad: func(text: string) -> {ty};"),
        );
        assert_language_error(
            store,
            instance,
            &[wasmtime::component::Val::String(text.into())],
        );
    }
}

#[test]
fn unsafe_to_number_conversion_fails_explicitly() {
    for (text, ty) in [
        ("9007199254740992", "s64"),
        ("-9007199254740992", "s64"),
        ("18446744073709551615", "u64"),
    ] {
        let (store, instance) = instantiate(
            &format!(
                "(defn ^:export bad [] (suss.core/wit-to-number (suss.core/wit-{ty} \"{text}\")))"
            ),
            "export bad: func() -> f64;",
        );
        assert_language_error(store, instance, &[]);
    }
}

#[test]
fn async_exact_results_use_distinct_completion_indices_and_mixed_parameter_positions() {
    let (mut store,instance)=instantiate("(defn ^:export signed [x marker flag] (if flag x (suss.core/wit-s64 \"-42\"))) (defn ^:export unsigned [marker x] x) (defn ^:export scalar [x] (+ x 1))","export signed: async func(x: s64, marker: f32, flag: bool) -> s64; export unsigned: async func(marker: string, x: u64) -> u64; export scalar: async func(x: s32) -> s32;");
    let signed = instance
        .get_typed_func::<(i64, f32, bool), (i64,)>(&mut store, "signed")
        .unwrap();
    let unsigned = instance
        .get_typed_func::<(&str, u64), (u64,)>(&mut store, "unsigned")
        .unwrap();
    let scalar = instance
        .get_typed_func::<(i32,), (i32,)>(&mut store, "scalar")
        .unwrap();
    for n in 0..20 {
        assert_eq!(
            complete(signed.call_async(&mut store, (i64::MIN, 42.5, true))).unwrap(),
            (i64::MIN,)
        );
        assert_eq!(
            complete(signed.call_async(&mut store, (i64::MAX, 42.5, false))).unwrap(),
            (-42,)
        );
        assert_eq!(
            complete(unsigned.call_async(&mut store, ("😀λ", u64::MAX))).unwrap(),
            (u64::MAX,)
        );
        assert_eq!(
            complete(scalar.call_async(&mut store, (n,))).unwrap(),
            (n + 1,)
        );
        store.gc(None).unwrap();
    }
}

#[test]
fn exact_aliases_and_private_schema_survive_public_constructor_redefinition() {
    let source = "(def original-class suss.core/WitSigned64) (defn ^:export signed [x] x) (defn ^:export unsigned [x] x) (defn ^:export bad [] (new original-class 0 1.5)) (ns suss.core) (def WitSigned64 nil) (def WitUnsigned64 nil) (def wit-valid-word? (fn* [x] true)) (ns user)";
    let exports = "type signed-value = s64; type unsigned-value = u64; export signed: func(x: signed-value) -> signed-value; export unsigned: func(x: unsigned-value) -> unsigned-value; export bad: func() -> signed-value;";
    let (mut store, instance) = instantiate(source, exports);
    let signed = instance
        .get_typed_func::<(i64,), (i64,)>(&mut store, "signed")
        .unwrap();
    let unsigned = instance
        .get_typed_func::<(u64,), (u64,)>(&mut store, "unsigned")
        .unwrap();
    assert_eq!(signed.call(&mut store, (i64::MIN,)).unwrap(), (i64::MIN,));
    signed.post_return(&mut store).unwrap();
    assert_eq!(unsigned.call(&mut store, (u64::MAX,)).unwrap(), (u64::MAX,));
    unsigned.post_return(&mut store).unwrap();
    let (store, instance) = instantiate(source, exports);
    assert_language_error(store, instance, &[]);
}

#[test]
fn exact_option_and_list_schemas_keep_separate_roots_and_mixed_flat_positions() {
    let (mut store, instance) = instantiate(
        "(defn ^:export choose [value flag words] (if (if (= (nth flag 0) :some) (if (nth flag 1) (= (count words) 1) false) false) value (suss.core/wit-s64 \"-42\"))) (defn ^:export async-choose [value flag words] (choose value flag words))",
        "export choose: func(value: s64, flag: option<bool>, words: list<string>) -> s64; export async-choose: async func(value: s64, flag: option<bool>, words: list<string>) -> s64;",
    );
    let choose = instance
        .get_typed_func::<(i64, Option<bool>, &[String]), (i64,)>(&mut store, "choose")
        .unwrap();
    let asynchronous = instance
        .get_typed_func::<(i64, Option<bool>, &[String]), (i64,)>(&mut store, "async-choose")
        .unwrap();
    for (flag, words, expected) in [
        (None, vec!["😀λ".into()], -42),
        (Some(false), vec!["😀λ".into()], -42),
        (Some(true), vec![], -42),
        (Some(true), vec!["😀λ".into()], i64::MIN),
    ] {
        assert_eq!(
            choose.call(&mut store, (i64::MIN, flag, &words)).unwrap(),
            (expected,)
        );
        choose.post_return(&mut store).unwrap();
        assert_eq!(
            complete(asynchronous.call_async(&mut store, (i64::MIN, flag, &words))).unwrap(),
            (expected,)
        );
        store.gc(None).unwrap();
    }
}

#[test]
fn private_exact_dispatch_survives_public_identity_redefinition() {
    let source = "(def original-class suss.core/WitSigned64) (defn ^:export signed [x] x) (defn ^:export unsigned [x] x) (defn ^:export bad [] (new original-class 0 1.5)) (ns suss.core) (def identical? (fn* [a b] false)) (ns user)";
    let exports = "export signed: func(x: s64) -> s64; export unsigned: func(x: u64) -> u64; export bad: func() -> s64;";
    let (mut store, instance) = instantiate(source, exports);
    let signed = instance
        .get_typed_func::<(i64,), (i64,)>(&mut store, "signed")
        .unwrap();
    let unsigned = instance
        .get_typed_func::<(u64,), (u64,)>(&mut store, "unsigned")
        .unwrap();
    assert_eq!(signed.call(&mut store, (i64::MIN,)).unwrap(), (i64::MIN,));
    signed.post_return(&mut store).unwrap();
    assert_eq!(unsigned.call(&mut store, (u64::MAX,)).unwrap(), (u64::MAX,));
    unsigned.post_return(&mut store).unwrap();
    let (store, instance) = instantiate(source, exports);
    assert_language_error(store, instance, &[]);
}
#[test]
fn private_exact_validation_survives_public_number_predicate_redefinition() {
    let source = "(def original-class suss.core/WitSigned64) (defn ^:export signed [x] x) (defn ^:export unsigned [x] x) (defn ^:export bad [] (new original-class 0 1.5)) (ns suss.core) (def number? (fn* [a] false)) (ns user)";
    let exports = "export signed: func(x: s64) -> s64; export unsigned: func(x: u64) -> u64; export bad: func() -> s64;";
    let (mut store, instance) = instantiate(source, exports);
    let signed = instance
        .get_typed_func::<(i64,), (i64,)>(&mut store, "signed")
        .unwrap();
    let unsigned = instance
        .get_typed_func::<(u64,), (u64,)>(&mut store, "unsigned")
        .unwrap();
    assert_eq!(signed.call(&mut store, (i64::MIN,)).unwrap(), (i64::MIN,));
    signed.post_return(&mut store).unwrap();
    assert_eq!(unsigned.call(&mut store, (u64::MAX,)).unwrap(), (u64::MAX,));
    unsigned.post_return(&mut store).unwrap();
    let (store, instance) = instantiate(source, exports);
    assert_language_error(store, instance, &[]);
}
