//! Execute optional canonical values through the compiled source pipeline.
#![cfg(not(target_family = "wasm"))]

use std::sync::OnceLock;
use suss_compile::Compiler;
use wasmtime::{
    component::{Component, Instance, Linker},
    AsContextMut, Config, Engine, Store,
};

fn instantiate(source: &str, exports: &str) -> (Store<()>, Instance) {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    let engine = ENGINE.get_or_init(|| {
        let mut config = Config::new();
        config
            .wasm_gc(true)
            .wasm_function_references(true)
            .wasm_tail_call(true)
            .wasm_exceptions(true)
            .wasm_component_model(true)
            .wasm_component_model_async(true)
            .consume_fuel(true)
            .cranelift_opt_level(wasmtime::OptLevel::None);
        Engine::new(&config).unwrap()
    });
    let wit = format!("package test:optional; world api {{ {exports} }}");
    let bytes = Compiler::new().compile(source, &wit).unwrap();
    let component = Component::new(engine, bytes).unwrap();
    assert_eq!(component.component_type().imports(engine).count(), 0);
    let mut store = Store::new(engine, ());
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
fn asynchronous_option_arguments_preserve_none_and_false() {
    let (mut store, instance) = instantiate(
        "(defn ^:export choose [x fallback] (if (= x [:none]) fallback (if (nth x 1) 1 0)))",
        "export choose: async func(x: option<bool>, fallback: s32) -> s32;",
    );
    let function = instance
        .get_typed_func::<(Option<bool>, i32), (i32,)>(&mut store, "choose")
        .unwrap();
    assert!(function.func().ty(&store).async_());
    for (value, expected) in [(None, 42), (Some(false), 0), (Some(true), 1)] {
        assert_eq!(
            complete(function.call_async(&mut store, (value, 42))).unwrap(),
            (expected,)
        );
        store.gc(None).unwrap();
    }
}

#[test]
fn asynchronous_option_float_results_preserve_canonical_bits() {
    let (mut store, instance) = instantiate(
        "(defn ^:export identity-option [x] x)",
        "export identity-option: async func(x: option<f64>) -> option<f64>;",
    );
    let function = instance
        .get_typed_func::<(Option<f64>,), (Option<f64>,)>(&mut store, "identity-option")
        .unwrap();
    assert!(function.func().ty(&store).async_());
    for value in [
        None,
        Some(42.5),
        Some(-0.0),
        Some(f64::from_bits(0x7ff8_0000_0000_0042)),
    ] {
        let actual = complete(function.call_async(&mut store, (value,)))
            .unwrap()
            .0;
        assert_eq!(actual.map(f64::to_bits), value.map(f64::to_bits));
        store.gc(None).unwrap();
    }
}

#[test]
fn mixed_async_option_exports_use_distinct_completion_signatures() {
    let (mut store, instance) = instantiate(
        "(defn ^:export flag [x] x) (defn ^:export number [x] x) (defn ^:export twice [x] (* x 2))",
        "export flag: async func(x: option<bool>) -> option<bool>; export number: async func(x: option<f64>) -> option<f64>; export twice: async func(x: f64) -> f64;",
    );
    let flag = instance
        .get_typed_func::<(Option<bool>,), (Option<bool>,)>(&mut store, "flag")
        .unwrap();
    let number = instance
        .get_typed_func::<(Option<f64>,), (Option<f64>,)>(&mut store, "number")
        .unwrap();
    let twice = instance
        .get_typed_func::<(f64,), (f64,)>(&mut store, "twice")
        .unwrap();
    for _ in 0..3 {
        assert_eq!(
            complete(flag.call_async(&mut store, (Some(false),))).unwrap(),
            (Some(false),)
        );
        assert_eq!(
            complete(number.call_async(&mut store, (Some(42.5),))).unwrap(),
            (Some(42.5),)
        );
        assert_eq!(
            complete(flag.call_async(&mut store, (None,))).unwrap(),
            (None,)
        );
        assert_eq!(
            complete(twice.call_async(&mut store, (21.0,))).unwrap(),
            (42.0,)
        );
        store.gc(None).unwrap();
    }
}

#[test]
fn mixed_scalar_and_optional_exports_use_their_actual_component_type_indices() {
    let (mut store, instance) = instantiate(
        "(defn ^:export before [x] (+ x 1)) (defn ^:export optional [x] x) (defn ^:export after [x] (* x 2))",
        "export before: func(x: s32) -> s32; export optional: func(x: option<s32>) -> option<s32>; export after: func(x: s32) -> s32;",
    );
    let before = instance
        .get_typed_func::<(i32,), (i32,)>(&mut store, "before")
        .unwrap();
    let optional = instance
        .get_typed_func::<(Option<i32>,), (Option<i32>,)>(&mut store, "optional")
        .unwrap();
    let after = instance
        .get_typed_func::<(i32,), (i32,)>(&mut store, "after")
        .unwrap();
    assert_eq!(before.call(&mut store, (41,)).unwrap(), (42,));
    before.post_return(&mut store).unwrap();
    assert_eq!(optional.call(&mut store, (Some(7),)).unwrap(), (Some(7),));
    optional.post_return(&mut store).unwrap();
    store.gc(None).unwrap();
    assert_eq!(after.call(&mut store, (21,)).unwrap(), (42,));
    after.post_return(&mut store).unwrap();
}

#[test]
fn optional_integer_parameters_preserve_none_some_and_argument_order() {
    let (mut store, instance) = instantiate(
        "(defn ^:export choose [x fallback] (if (= x [:none]) fallback (nth x 1)))",
        "export choose: func(x: option<s32>, fallback: s32) -> s32;",
    );
    let choose = instance
        .get_typed_func::<(Option<i32>, i32), (i32,)>(&mut store, "choose")
        .unwrap();
    for (x, fallback, expected) in [
        (None, 42, 42),
        (Some(0), 42, 0),
        (Some(i32::MIN), 42, i32::MIN),
        (Some(i32::MAX), -1, i32::MAX),
    ] {
        assert_eq!(choose.call(&mut store, (x, fallback)).unwrap(), (expected,));
        choose.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
    }
}

#[test]
fn optional_results_distinguish_nil_false_and_true_across_repeated_calls() {
    let (mut store, instance) = instantiate(
        "(defn ^:export identity-option [x] x)",
        "export identity-option: func(x: option<bool>) -> option<bool>;",
    );
    let function = instance
        .get_typed_func::<(Option<bool>,), (Option<bool>,)>(&mut store, "identity-option")
        .unwrap();
    for _ in 0..100 {
        for value in [None, Some(false), Some(true)] {
            assert_eq!(function.call(&mut store, (value,)).unwrap(), (value,));
            function.post_return(&mut store).unwrap();
            store.gc(None).unwrap();
        }
    }
}

#[test]
fn optional_float_results_preserve_bits_and_release_canonical_return_state() {
    for _ in 0..2 {
        let (mut store, instance) = instantiate(
            "(defn ^:export identity-option [x] x)",
            "export identity-option: func(x: option<f64>) -> option<f64>;",
        );
        let function = instance
            .get_typed_func::<(Option<f64>,), (Option<f64>,)>(&mut store, "identity-option")
            .unwrap();
        for value in [
            None,
            Some(0.0),
            Some(-0.0),
            Some(42.5),
            Some(f64::INFINITY),
            Some(f64::from_bits(0x7ff8_0000_0000_0042)),
        ] {
            let actual = function.call(&mut store, (value,)).unwrap().0;
            assert_eq!(actual.map(f64::to_bits), value.map(f64::to_bits));
            function.post_return(&mut store).unwrap();
            store.gc(None).unwrap();
        }
    }
}

#[test]
fn optional_integer_results_preserve_alignment_sign_and_unsigned_ranges() {
    macro_rules! roundtrip {
        ($wit:literal, $rust:ty, $values:expr) => {{
            let (mut store, instance) = instantiate(
                "(defn ^:export identity-option [x] x)",
                concat!(
                    "export identity-option: func(x: option<",
                    $wit,
                    ">) -> option<",
                    $wit,
                    ">;"
                ),
            );
            let function = instance
                .get_typed_func::<(Option<$rust>,), (Option<$rust>,)>(&mut store, "identity-option")
                .unwrap();
            for value in $values {
                assert_eq!(function.call(&mut store, (value,)).unwrap(), (value,));
                function.post_return(&mut store).unwrap();
                store.gc(None).unwrap();
            }
        }};
    }
    roundtrip!("s8", i8, [None, Some(i8::MIN), Some(-1), Some(i8::MAX)]);
    roundtrip!("s16", i16, [None, Some(i16::MIN), Some(-1), Some(i16::MAX)]);
    roundtrip!("u16", u16, [None, Some(0), Some(u16::MAX)]);
    roundtrip!("u32", u32, [None, Some(0), Some(u32::MAX)]);
}

#[test]
fn optional_f32_results_preserve_signed_zero_and_payload_alignment() {
    let (mut store, instance) = instantiate(
        "(defn ^:export identity-option [x] x)",
        "export identity-option: func(x: option<f32>) -> option<f32>;",
    );
    let function = instance
        .get_typed_func::<(Option<f32>,), (Option<f32>,)>(&mut store, "identity-option")
        .unwrap();
    for value in [None, Some(0.0), Some(-0.0), Some(42.5), Some(f32::INFINITY)] {
        let actual = function.call(&mut store, (value,)).unwrap().0;
        assert_eq!(actual.map(f32::to_bits), value.map(f32::to_bits));
        function.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
    }
}

#[test]
fn eight_optional_parameters_preserve_the_last_flattened_payload() {
    let (mut store, instance) = instantiate(
        "(defn ^:export last-option [a b c d e f g h] h)",
        "export last-option: func(a: option<f64>, b: option<f64>, c: option<f64>, d: option<f64>, e: option<f64>, f: option<f64>, g: option<f64>, h: option<f64>) -> option<f64>;",
    );
    type Args = (
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
    );
    let function = instance
        .get_typed_func::<Args, (Option<f64>,)>(&mut store, "last-option")
        .unwrap();
    for last in [None, Some(42.0)] {
        assert_eq!(
            function
                .call(
                    &mut store,
                    (
                        None,
                        Some(1.0),
                        None,
                        Some(3.0),
                        None,
                        Some(5.0),
                        None,
                        last
                    )
                )
                .unwrap(),
            (last,)
        );
        function.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
    }
}

#[test]
fn unsupported_option_shapes_fail_before_effectful_source_macros() {
    let source = "(defmacro fail [] (throw 17)) (defn ^:export value [& xs] (fail))";
    for (signature, diagnostic) in [
        ("export value: func(x: option<list<f64>>) -> f64;", "list"),
        (
            "export value: func(a: option<f64>, b: option<f64>, c: option<f64>, d: option<f64>, e: option<f64>, f: option<f64>, g: option<f64>, h: option<f64>, i: option<f64>) -> f64;",
            "indirect parameter",
        ),
        (
            "export value: async func(a: option<f64>, b: option<f64>, c: option<f64>, d: option<f64>, e: option<f64>, f: option<f64>, g: option<f64>, h: option<f64>, i: option<f64>) -> f64;",
            "indirect parameter",
        ),
    ] {
        let wit = format!("package test:optional; world api {{ {signature} }}");
        let error = Compiler::new().compile(source, &wit).unwrap_err();
        assert!(
            matches!(error, suss_compile::CompileError::Unsupported(_)),
            "{error}"
        );
        assert!(error.to_string().contains(diagnostic), "{error}");
    }
}

#[test]
fn optional_small_integer_result_rejects_invalid_payload_with_language_exception() {
    // A failed canonical lifted call locks its instance in pinned Wasmtime,
    // independently of the Suss exception payload. Use fresh instances for
    // failures; successful calls below retain repeated-call/post-return coverage.
    for bad in [256.0, -2.0, 1.5, f64::NAN, f64::INFINITY] {
        let (mut store, instance) = instantiate(
            "(defn ^:export value [x] (if (= x -1) [:none] [:some x]))",
            "export value: func(x: f64) -> option<u8>;",
        );
        let function = instance
            .get_typed_func::<(f64,), (Option<u8>,)>(&mut store, "value")
            .unwrap();
        assert_eq!(function.call(&mut store, (42.0,)).unwrap(), (Some(42),));
        function.post_return(&mut store).unwrap();
        assert_eq!(function.call(&mut store, (-1.0,)).unwrap(), (None,));
        function.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
        assert_eq!(function.call(&mut store, (42.0,)).unwrap(), (Some(42),));
        function.post_return(&mut store).unwrap();
        assert!(function.call(&mut store, (bad,)).is_err());
        let exception = store
            .as_context_mut()
            .take_pending_exception()
            .expect("language boundary exception rather than a numeric trap");
        let fields = exception.fields(&mut store).unwrap().collect::<Vec<_>>();
        assert_eq!(fields.len(), 1);
        let payload = fields[0]
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
        let message = payload.field(&mut store, 1).unwrap();
        let message = message
            .unwrap_anyref()
            .unwrap()
            .as_array(&store)
            .unwrap()
            .unwrap();
        let units = message
            .elems(&mut store)
            .unwrap()
            .map(|unit| unit.unwrap_i32() as u16)
            .collect::<Vec<_>>();
        assert_eq!(
            String::from_utf16(&units).unwrap(),
            "WIT export value returned an incompatible scalar value"
        );
        assert!(function
            .call(&mut store, (42.0,))
            .unwrap_err()
            .to_string()
            .contains("cannot enter component instance"));
    }
}

#[test]
fn option_boundary_uses_tagged_vectors_in_source() {
    let (mut store, instance) = instantiate(
        "(defn ^:export inspect [x] (if (= x [:none]) 7 (if (= x [:some false]) 8 (if (= x [:some true]) 9 -1))))",
        "export inspect: func(x: option<bool>) -> s32;",
    );
    let function = instance
        .get_typed_func::<(Option<bool>,), (i32,)>(&mut store, "inspect")
        .unwrap();
    for (value, expected) in [(None, 7), (Some(false), 8), (Some(true), 9)] {
        assert_eq!(function.call(&mut store, (value,)).unwrap(), (expected,));
        function.post_return(&mut store).unwrap();
    }
}

#[test]
fn malformed_option_results_are_language_errors_not_none() {
    for form in [
        "nil",
        "false",
        "42",
        "[]",
        "[:some]",
        "[:none 42]",
        "[:wrong 42]",
        "[:some 1 2]",
        "'(:some 42)",
        "[:other/some 42]",
        "[:some nil]",
    ] {
        let (mut store, instance) = instantiate(
            &format!("(defn ^:export malformed [] {form})"),
            "export malformed: func() -> option<s32>;",
        );
        let function = instance
            .get_typed_func::<(), (Option<i32>,)>(&mut store, "malformed")
            .unwrap();
        assert!(function.call(&mut store, ()).is_err(), "{form}");
        let exception = store
            .as_context_mut()
            .take_pending_exception()
            .expect("language schema exception");
        let fields = exception.fields(&mut store).unwrap().collect::<Vec<_>>();
        let message = fields[0]
            .unwrap_anyref()
            .unwrap()
            .as_array(&store)
            .unwrap()
            .unwrap();
        let units = message
            .elems(&mut store)
            .unwrap()
            .map(|unit| unit.unwrap_i32() as u16)
            .collect::<Vec<_>>();
        let expected = if form == "[:some nil]" {
            "WIT scalar option payload cannot be nil"
        } else {
            "WIT option requires [:none] or [:some value]"
        };
        assert_eq!(String::from_utf16(&units).unwrap(), expected, "{form}");
    }
}

#[test]
fn option_schema_ignores_user_namespace_function_shadows() {
    let (mut store, instance) = instantiate(
        "(defn vector? [x] false) (defn count [x] 0) (defn nth [x y] (throw 17)) (defn nil? [x] true) (defn = [x y] false) (defn ^:export identity-option [x] x)",
        "export identity-option: func(x: option<bool>) -> option<bool>;",
    );
    let function = instance
        .get_typed_func::<(Option<bool>,), (Option<bool>,)>(&mut store, "identity-option")
        .unwrap();
    for value in [None, Some(false), Some(true)] {
        assert_eq!(function.call(&mut store, (value,)).unwrap(), (value,));
        function.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
    }
}

#[test]
fn option_schema_survives_core_function_and_constructor_redefinition() {
    let source = "(ns suss.core) (def vector? (fn [x] true)) (def count (fn [x] 1)) (def nth (fn [x i] :none)) (def = (fn [x y] true)) (def nil? (fn [x] true)) (def PersistentVector nil) (def Keyword nil) (ns user) (defn ^:export identity-option [x] x)";
    let (mut store, instance) = instantiate(
        source,
        "export identity-option: func(x: option<bool>) -> option<bool>;",
    );
    let function = instance
        .get_typed_func::<(Option<bool>,), (Option<bool>,)>(&mut store, "identity-option")
        .unwrap();
    for value in [None, Some(false), Some(true)] {
        assert_eq!(function.call(&mut store, (value,)).unwrap(), (value,));
        function.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
    }
    let (mut store, instance) = instantiate(
        "(ns suss.core) (def vector? (fn [x] true)) (def count (fn [x] 1)) (def nth (fn [x i] :none)) (def = (fn [x y] true)) (ns user) (defn ^:export malformed [] 42)",
        "export malformed: func() -> option<s32>;",
    );
    let function = instance
        .get_typed_func::<(), (Option<i32>,)>(&mut store, "malformed")
        .unwrap();
    assert!(function.call(&mut store, ()).is_err());
    assert!(store.as_context_mut().take_pending_exception().is_some());
}
