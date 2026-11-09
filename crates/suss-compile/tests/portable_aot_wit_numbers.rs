//! WIT integers cross as longs and floats as doubles, range-checked when
//! leaving Suss; numeric lists also leave from packed arrays (ADR-0003 "WIT
//! boundary", #336).
#![cfg(not(target_family = "wasm"))]

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};
use suss_compile::Compiler;
use wasmtime::{
    component::{Component, Instance, Linker, Val},
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

fn compile(engine: &Engine, source: &str, exports: &str) -> Component {
    let wit = format!("package test:wit-numbers; world api {{ {exports} }}");
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
    // Option results return through canonical memory; option parameters don't.
    let expected_memories: &[u64] = if exports.contains("string")
        || exports.contains("list<")
        || exports.contains("-> option<")
    {
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
    component
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
    // A failed call poisons its instance, so error cases instantiate one
    // compiled component afresh for each call.
    static COMPILED: OnceLock<Mutex<HashMap<String, Component>>> = OnceLock::new();
    let key = format!("{source}\0{exports}");
    let cached = COMPILED.get_or_init(Default::default).lock().unwrap().get(&key).cloned();
    let component = cached.unwrap_or_else(|| {
        let component = compile(engine, source, exports);
        COMPILED.get_or_init(Default::default).lock().unwrap().insert(key, component.clone());
        component
    });
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

/// Integer types, their WIT names, and their ranges as longs: `u64` above
/// 2^63 - 1 needs BigInt (#338).
const INTEGERS: [(&str, i64, i64); 8] = [
    ("u8", 0, u8::MAX as i64),
    ("s8", i8::MIN as i64, i8::MAX as i64),
    ("u16", 0, u16::MAX as i64),
    ("s16", i16::MIN as i64, i16::MAX as i64),
    ("u32", 0, u32::MAX as i64),
    ("s32", i32::MIN as i64, i32::MAX as i64),
    ("u64", 0, i64::MAX),
    ("s64", i64::MIN, i64::MAX),
];

fn val(ty: &str, value: i64) -> Val {
    match ty {
        "u8" => Val::U8(value as u8),
        "s8" => Val::S8(value as i8),
        "u16" => Val::U16(value as u16),
        "s16" => Val::S16(value as i16),
        "u32" => Val::U32(value as u32),
        "s32" => Val::S32(value as i32),
        "u64" => Val::U64(value as u64),
        _ => Val::S64(value),
    }
}

fn call(store: &mut Store<BoundaryLimits>, instance: &Instance, name: &str, args: &[Val]) -> Val {
    let function = instance.get_func(&mut *store, name).unwrap();
    let mut results = [Val::Bool(false)];
    function.call(&mut *store, args, &mut results).unwrap_or_else(|error| panic!("{name}: {error:?}"));
    results.into_iter().next().unwrap()
}

#[test]
fn integers_cross_as_longs_at_every_range_boundary() {
    let mut source = String::from("(def saved (atom nil)) (defn ^:export recall [] (deref saved))");
    let mut exports = String::from("export recall: func() -> s64;");
    for (ty, _, _) in INTEGERS {
        source.push_str(&format!(
            " (defn ^:export same-{ty} [x] (do (reset! saved x) x)) (defn ^:export seven-{ty} [x] (= x 7))"
        ));
        exports.push_str(&format!(
            " export same-{ty}: func(x: {ty}) -> {ty}; export seven-{ty}: func(x: {ty}) -> bool;"
        ));
    }
    let (mut store, instance) = instantiate(&source, &exports);
    for (ty, min, max) in INTEGERS {
        for value in [min, min + 1, 0, 1, max - 1, max] {
            let result = call(&mut store, &instance, &format!("same-{ty}"), &[val(ty, value)]);
            assert_eq!(result, val(ty, value), "{ty} {value}");
            store.gc(None).unwrap();
            assert_eq!(call(&mut store, &instance, "recall", &[]), Val::S64(value), "{ty} {value}");
        }
        // A long, so equal to the long 7 and not to a double.
        assert_eq!(call(&mut store, &instance, &format!("seven-{ty}"), &[val(ty, 7)]), Val::Bool(true));
    }
}

#[test]
fn floats_cross_as_doubles_and_f32_rounds_when_leaving() {
    let (mut store, instance) = instantiate(
        "(defn ^:export same64 [x] x) (defn ^:export same32 [x] x) (defn ^:export double64 [x] (= x 7.0))
         (defn ^:export from-long [] 7) (defn ^:export round [] 0.1) (defn ^:export big [] 9223372036854775807)",
        "export same64: func(x: f64) -> f64; export same32: func(x: f32) -> f32; export double64: func(x: f64) -> bool;
         export from-long: func() -> f64; export round: func() -> f32; export big: func() -> f32;",
    );
    for value in [f64::MIN, -0.0, 1.5, f64::MAX, f64::INFINITY] {
        assert_eq!(call(&mut store, &instance, "same64", &[Val::Float64(value)]), Val::Float64(value));
    }
    assert!(matches!(call(&mut store, &instance, "same64", &[Val::Float64(f64::NAN)]), Val::Float64(x) if x.is_nan()));
    for value in [f32::MIN, f32::MIN_POSITIVE, 0.1, f32::MAX] {
        assert_eq!(call(&mut store, &instance, "same32", &[Val::Float32(value)]), Val::Float32(value));
    }
    assert_eq!(call(&mut store, &instance, "double64", &[Val::Float64(7.0)]), Val::Bool(true));
    // A long leaving as a float converts, as Clojure's double does.
    assert_eq!(call(&mut store, &instance, "from-long", &[]), Val::Float64(7.0));
    assert_eq!(call(&mut store, &instance, "round", &[]), Val::Float32(0.1));
    assert_eq!(call(&mut store, &instance, "big", &[]), Val::Float32(9.223372e18));
}

/// The message of the language error `name` throws.
fn thrown(mut store: Store<BoundaryLimits>, instance: Instance, name: &str, args: &[Val]) -> String {
    let function = instance.get_func(&mut store, name).unwrap();
    let mut results = [Val::Bool(false)];
    assert!(function.call(&mut store, args, &mut results).is_err(), "{name}");
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
    String::from_utf16(&units).unwrap()
}

/// One component with an export per body, each returning `ty`.
fn cases(bodies: &[String], ty: &str) -> (String, String) {
    let source = bodies
        .iter()
        .enumerate()
        .map(|(index, body)| format!("(defn ^:export bad-{index} [] {body})"))
        .collect::<String>();
    let exports = (0..bodies.len())
        .map(|index| format!("export bad-{index}: func() -> {ty};"))
        .collect::<String>();
    (source, exports)
}

#[test]
fn integer_results_must_be_longs_within_range() {
    let mut types = INTEGERS.map(|(ty, min, max)| (ty, Some((min, max)))).to_vec();
    types.extend([("f32", None), ("f64", None)]);
    for (ty, range) in types {
        let mut bodies = match range {
            Some(_) => ["1.5", "1.0", "nil", "\"1\"", "##NaN"].map(String::from).to_vec(),
            None => ["nil", "\"1\"", "true"].map(String::from).to_vec(),
        };
        if let Some((min, max)) = range {
            if min > i64::MIN {
                bodies.push(format!("{}", min - 1));
            }
            if max < i64::MAX {
                bodies.push(format!("{}", max + 1));
            }
        }
        let (source, exports) = cases(&bodies, ty);
        for (index, body) in bodies.iter().enumerate() {
            let (store, instance) = instantiate(&source, &exports);
            assert_eq!(
                thrown(store, instance, &format!("bad-{index}"), &[]),
                format!("WIT export bad-{index} returned an incompatible scalar value"),
                "{ty} {body}"
            );
        }
        if let Some((min, max)) = range {
            let (mut store, instance) = instantiate(
                &format!("(defn ^:export low [] {min}) (defn ^:export high [] {max})"),
                &format!("export low: func() -> {ty}; export high: func() -> {ty};"),
            );
            assert_eq!(call(&mut store, &instance, "low", &[]), val(ty, min));
            assert_eq!(call(&mut store, &instance, "high", &[]), val(ty, max));
        }
    }
}

#[test]
fn u64_arguments_above_the_long_range_need_big_int() {
    let source = "(defn ^:export scalar [x] x) (defn ^:export listed [x] (count x)) (defn ^:export optional [x] x)";
    let exports = "export scalar: func(x: u64) -> u64; export listed: func(x: list<u64>) -> u32; export optional: func(x: option<u64>) -> u32;";
    for value in [1u64 << 63, u64::MAX] {
        for (name, argument) in [
            ("scalar", Val::U64(value)),
            ("listed", Val::List(vec![Val::U64(1), Val::U64(value)])),
            ("optional", Val::Option(Some(Box::new(Val::U64(value))))),
        ] {
            let (store, instance) = instantiate(source, exports);
            assert_eq!(
                thrown(store, instance, name, &[argument]),
                format!("WIT export {name} received a u64 above 9223372036854775807, which needs unsupported BigInt")
            );
        }
    }
}

#[test]
fn numeric_lists_leave_from_vectors_and_matching_packed_arrays() {
    let (mut store, instance) = instantiate(
        "(defn ^:export bytes-u8 [] (byte-array [-1 0 1 127 -128]))
         (defn ^:export bytes-s8 [] (byte-array [-1 0 1 127 -128]))
         (defn ^:export shorts [] (short-array [-32768 32767]))
         (defn ^:export ints [] (int-array [-2147483648 2147483647]))
         (defn ^:export longs [] (long-array [-9223372036854775808 9223372036854775807 0]))
         (defn ^:export floats [] (float-array [0.1 1e300]))
         (defn ^:export doubles [] (double-array [0.1 ##-Inf]))
         (defn ^:export empty [] (long-array 0))
         (defn ^:export vector64 [] [-9223372036854775808 9223372036854775807])
         (defn ^:export unsigned64 [] [0 9223372036854775807])
         (defn ^:export sum [xs] (reduce + xs))",
        "export bytes-u8: func() -> list<u8>; export bytes-s8: func() -> list<s8>;
         export shorts: func() -> list<s16>; export ints: func() -> list<s32>;
         export longs: func() -> list<s64>; export floats: func() -> list<f32>;
         export doubles: func() -> list<f64>; export empty: func() -> list<s64>;
         export vector64: func() -> list<s64>; export unsigned64: func() -> list<u64>;
         export sum: func(xs: list<s64>) -> s64;",
    );
    let list = |values: Vec<Val>| Val::List(values);
    for (name, expected) in [
        // A byte's bits, as Java's byte I/O sends them.
        ("bytes-u8", list([255, 0, 1, 127, 128].map(Val::U8).to_vec())),
        ("bytes-s8", list([-1, 0, 1, 127, -128].map(Val::S8).to_vec())),
        ("shorts", list([i16::MIN, i16::MAX].map(Val::S16).to_vec())),
        ("ints", list([i32::MIN, i32::MAX].map(Val::S32).to_vec())),
        ("longs", list([i64::MIN, i64::MAX, 0].map(Val::S64).to_vec())),
        ("floats", list([0.1, f32::INFINITY].map(Val::Float32).to_vec())),
        ("doubles", list([0.1, f64::NEG_INFINITY].map(Val::Float64).to_vec())),
        ("empty", list(vec![])),
        ("vector64", list([i64::MIN, i64::MAX].map(Val::S64).to_vec())),
        ("unsigned64", list([0, i64::MAX as u64].map(Val::U64).to_vec())),
    ] {
        assert_eq!(call(&mut store, &instance, name, &[]), expected, "{name}");
        store.gc(None).unwrap();
    }
    assert_eq!(
        call(&mut store, &instance, "sum", &[list([i64::MAX, -1, i64::MIN].map(Val::S64).to_vec())]),
        Val::S64(-2)
    );
    let failures = [
        ("(int-array 1)", "s64", "returned a primitive array of another element type"),
        ("(long-array 1)", "s32", "returned a primitive array of another element type"),
        ("(double-array 1)", "f32", "returned a primitive array of another element type"),
        // u16, u32 and u64 lists take vectors only.
        ("(short-array 1)", "u16", ""),
        ("(int-array 1)", "u32", ""),
        ("(long-array 1)", "u64", ""),
        ("[1 2.5]", "s64", "returned an incompatible scalar value"),
        ("[-1]", "u64", "returned an incompatible scalar value"),
    ];
    let source = failures
        .iter()
        .enumerate()
        .map(|(index, (body, _, _))| format!("(defn ^:export bad-{index} [] {body})"))
        .collect::<String>();
    let exports = failures
        .iter()
        .enumerate()
        .map(|(index, (_, ty, _))| format!("export bad-{index}: func() -> list<{ty}>;"))
        .collect::<String>();
    for (index, (body, ty, message)) in failures.iter().enumerate() {
        let (store, instance) = instantiate(&source, &exports);
        let expected = if message.is_empty() {
            "WIT list requires a persistent vector".to_string()
        } else {
            format!("WIT export bad-{index} {message}")
        };
        assert_eq!(thrown(store, instance, &format!("bad-{index}"), &[]), expected, "{body} as list<{ty}>");
    }
}

#[test]
fn async_long_results_use_distinct_completion_indices_and_mixed_parameter_positions() {
    let (mut store, instance) = instantiate(
        "(defn ^:export signed [x marker flag] (if flag x -42)) (defn ^:export unsigned [marker x] x) (defn ^:export scalar [x] (+ x 1))",
        "export signed: async func(x: s64, marker: f32, flag: bool) -> s64; export unsigned: async func(marker: string, x: u64) -> u64; export scalar: async func(x: s32) -> s32;",
    );
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
            complete(unsigned.call_async(&mut store, ("😀λ", i64::MAX as u64))).unwrap(),
            (i64::MAX as u64,)
        );
        assert_eq!(
            complete(scalar.call_async(&mut store, (n,))).unwrap(),
            (n + 1,)
        );
        store.gc(None).unwrap();
    }
}

#[test]
fn aliased_long_types_and_option_and_list_schemas_keep_mixed_flat_positions() {
    let (mut store, instance) = instantiate(
        "(defn ^:export choose [value flag words] (if (if (= (nth flag 0) :some) (if (nth flag 1) (= (count words) 1) false) false) value -42)) (defn ^:export async-choose [value flag words] (choose value flag words))",
        "type wide = s64; export choose: func(value: wide, flag: option<bool>, words: list<string>) -> wide; export async-choose: async func(value: wide, flag: option<bool>, words: list<string>) -> wide;",
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
fn boundary_checks_survive_public_core_redefinition() {
    // The adapter tests values itself, so rebinding core's predicates changes
    // neither what crosses nor what is rejected.
    for redefinition in [
        "(def identical? (fn* [a b] false))",
        "(def number? (fn* [a] false))",
        "(def long-array (fn* [n] nil)) (def array? (fn* [a] false))",
    ] {
        let source = format!(
            "(defn ^:export signed [x] x) (defn ^:export unsigned [x] x) (defn ^:export bytes [] (byte-array [-1])) (defn ^:export bad [] 1.5) (ns suss.core) {redefinition} (ns user)"
        );
        let exports = "export signed: func(x: s64) -> s64; export unsigned: func(x: u64) -> u64; export bytes: func() -> list<u8>; export bad: func() -> s64;";
        let (mut store, instance) = instantiate(&source, exports);
        assert_eq!(call(&mut store, &instance, "signed", &[Val::S64(i64::MIN)]), Val::S64(i64::MIN));
        assert_eq!(call(&mut store, &instance, "unsigned", &[Val::U64(i64::MAX as u64)]), Val::U64(i64::MAX as u64));
        assert_eq!(call(&mut store, &instance, "bytes", &[]), Val::List(vec![Val::U8(255)]));
        let (store, instance) = instantiate(&source, exports);
        assert_eq!(
            thrown(store, instance, "bad", &[]),
            "WIT export bad returned an incompatible scalar value",
            "{redefinition}"
        );
    }
}

#[test]
fn f32_results_from_longs_round_once() {
    // 2^60 + 2^36 + 1 rounds differently through a double.
    let x: i64 = 1152921573326323713;
    assert_ne!(x as f32, (x as f64) as f32);
    let (mut store, instance) = instantiate(
        &format!("(defn ^:export single [] {x})"),
        "export single: func() -> f32;",
    );
    assert_eq!(call(&mut store, &instance, "single", &[]), Val::Float32(x as f32));
}

#[test]
fn long_options_round_trip_synchronously_and_asynchronously() {
    let (mut store, instance) = instantiate(
        "(defn ^:export signed [x] x) (defn ^:export unsigned [x] x) (defn ^:export async-signed [x] x)",
        "export signed: func(x: option<s64>) -> option<s64>; export unsigned: func(x: option<u64>) -> option<u64>;
         export async-signed: async func(x: option<s64>) -> option<s64>;",
    );
    let signed = instance
        .get_typed_func::<(Option<i64>,), (Option<i64>,)>(&mut store, "signed")
        .unwrap();
    let unsigned = instance
        .get_typed_func::<(Option<u64>,), (Option<u64>,)>(&mut store, "unsigned")
        .unwrap();
    let asynchronous = instance
        .get_typed_func::<(Option<i64>,), (Option<i64>,)>(&mut store, "async-signed")
        .unwrap();
    for value in [None, Some(i64::MIN), Some(-1), Some(0), Some(i64::MAX)] {
        assert_eq!(signed.call(&mut store, (value,)).unwrap(), (value,));
        assert_eq!(complete(asynchronous.call_async(&mut store, (value,))).unwrap(), (value,));
        store.gc(None).unwrap();
    }
    for value in [None, Some(0), Some(1), Some(i64::MAX as u64)] {
        assert_eq!(unsigned.call(&mut store, (value,)).unwrap(), (value,));
    }
}

#[test]
fn numeric_lists_round_trip_at_every_element_type_boundary() {
    let floats32 = [f32::MIN, -0.0, f32::from_bits(1), f32::MIN_POSITIVE, 0.1, f32::MAX, f32::INFINITY, f32::NAN];
    let floats64 = [f64::MIN, -0.0, f64::from_bits(1), f64::MIN_POSITIVE, 0.1, f64::MAX, f64::NEG_INFINITY, f64::NAN];
    let mut source = String::new();
    let mut exports = String::new();
    for ty in INTEGERS.map(|(ty, _, _)| ty).into_iter().chain(["f32", "f64"]) {
        source.push_str(&format!("(defn ^:export list-{ty} [xs] xs) (defn ^:export scalar-{ty} [x] x)"));
        exports.push_str(&format!(
            "export list-{ty}: func(xs: list<{ty}>) -> list<{ty}>; export scalar-{ty}: func(x: {ty}) -> {ty};"
        ));
    }
    let (mut store, instance) = instantiate(&source, &exports);
    // Debug strings compare NaN by its spelling and keep -0.0's sign.
    let mut check = |name: String, value: Val| {
        let result = call(&mut store, &instance, &name, &[value.clone()]);
        assert_eq!(format!("{result:?}"), format!("{value:?}"), "{name}");
        store.gc(None).unwrap();
    };
    for (ty, min, max) in INTEGERS {
        let values = [min, min + 1, 0, max - 1, max].map(|value| val(ty, value)).to_vec();
        check(format!("list-{ty}"), Val::List(values));
    }
    check("list-f32".into(), Val::List(floats32.map(Val::Float32).to_vec()));
    check("list-f64".into(), Val::List(floats64.map(Val::Float64).to_vec()));
    for value in floats32 {
        check("scalar-f32".into(), Val::Float32(value));
    }
}
