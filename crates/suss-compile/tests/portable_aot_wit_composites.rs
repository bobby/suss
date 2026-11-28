//! Execute recursive WIT schemas as native persistent source values.
#![cfg(not(target_family = "wasm"))]
use std::sync::OnceLock;
use suss_compile::Compiler;
use wasmtime::{
    Config, Engine, Store,
    component::{Component, Instance, Linker, Val},
};

fn instantiate(source: &str, exports: &str) -> (Store<()>, Instance) {
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
    let wit = format!("package test:composites; world api {{ {exports} }}");
    let bytes = Compiler::new().compile(source, &wit).unwrap();
    let component = Component::new(engine, bytes).unwrap();
    assert_eq!(component.component_type().imports(engine).count(), 0);
    let mut store = Store::new(engine, ());
    store.set_fuel(100_000_000).unwrap();
    let instance = Linker::new(engine)
        .instantiate(&mut store, &component)
        .unwrap();
    store.set_fuel(100_000_000).unwrap();
    (store, instance)
}

#[test]
fn wit_tuple_fields_use_native_assoc_and_retain_values_after_gc() {
    let (mut store, instance) = instantiate(
        "(def saved (atom nil))
         (defn ^:export bump [x]
           (let [y (assoc x 0 (inc (nth x 0)))] (reset! saved y) y))
         (defn ^:export recall [] @saved)",
        "type row = tuple<s64, string, option<bool>>;
         export bump: func(x: row) -> row; export recall: func() -> row;",
    );
    type Row = (i64, String, Option<bool>);
    let bump = instance
        .get_typed_func::<(Row,), (Row,)>(&mut store, "bump")
        .unwrap();
    let recall = instance
        .get_typed_func::<(), (Row,)>(&mut store, "recall")
        .unwrap();
    for flag in [None, Some(false), Some(true)] {
        let text = "left λ\0🦀".to_owned();
        let expected = (9007199254740994_i64, text.clone(), flag);
        assert_eq!(
            bump.call(&mut store, ((9007199254740993, text, flag),))
                .unwrap(),
            (expected.clone(),)
        );
        store.gc(None).unwrap();
        assert_eq!(recall.call(&mut store, ()).unwrap(), (expected,));
    }
}

#[test]
fn wit_nested_options_results_and_void_results_single_field_tuples_remain_distinct() {
    let (mut store, instance) = instantiate(
        "(defn ^:export nested [x] x) (defn ^:export either [x] x)
         (defn ^:export void-result [x] x) (defn ^:export single [x] x)",
        "export nested: func(x: option<option<s64>>) -> option<option<s64>>;
         export either: func(x: result<f32, f64>) -> result<f32, f64>;
         type one = tuple<s64>;
         export void-result: func(x: result) -> result; export single: func(x: one) -> one;",
    );
    let nested = instance
        .get_typed_func::<(Option<Option<i64>>,), (Option<Option<i64>>,)>(&mut store, "nested")
        .unwrap();
    for value in [None, Some(None), Some(Some(i64::MIN)), Some(Some(i64::MAX))] {
        assert_eq!(nested.call(&mut store, (value,)).unwrap(), (value,));
    }
    let result = instance
        .get_typed_func::<(Result<f32, f64>,), (Result<f32, f64>,)>(&mut store, "either")
        .unwrap();
    for value in [Ok(-0.0_f32), Ok(1.5), Err(-0.0_f64), Err(f64::MAX)] {
        let actual = result.call(&mut store, (value,)).unwrap().0;
        match (actual, value) {
            (Ok(a), Ok(b)) => assert_eq!(a.to_bits(), b.to_bits()),
            (Err(a), Err(b)) => assert_eq!(a.to_bits(), b.to_bits()),
            pair => panic!("result case changed: {pair:?}"),
        }
    }
    let void_result = instance
        .get_typed_func::<(Result<(), ()>,), (Result<(), ()>,)>(&mut store, "void-result")
        .unwrap();
    for value in [Ok(()), Err(())] {
        assert_eq!(void_result.call(&mut store, (value,)).unwrap(), (value,));
    }
    let single = instance
        .get_typed_func::<((i64,),), ((i64,),)>(&mut store, "single")
        .unwrap();
    assert_eq!(
        single.call(&mut store, ((i64::MIN,),)).unwrap(),
        ((i64::MIN,),)
    );
}

#[test]
fn wit_records_and_nested_lists_use_native_maps_vectors_and_named_types() {
    let mut resolve = wit_parser::Resolve::new();
    let package = resolve
        .push_str(
            "api.wit",
            "package test:record-values@1.2.3;
      interface values {
        record entry { id: u64, label: string, enabled: option<bool> }
        type batch = list<entry>;
        amend: func(x: entry) -> entry;
        append: func(xs: batch) -> batch;
      } world api { export values; }",
        )
        .unwrap();
    let world = resolve.select_world(&[package], None).unwrap();
    let fragments = suss_compile::portable_aot::prepare_source(
        "(defn amend [x] (assoc x :label (str (:label x) \"!\")))
         (defn append [xs] (conj xs {:id 18446744073709551615N :label \"added 🦀\" :enabled [:none]}))",
        None, &[],
    ).unwrap();
    let bytes = suss_compile::portable::aot::component(
        &fragments,
        &resolve,
        world,
        &[
            (
                "test:record-values/values@1.2.3#amend".into(),
                suss_reader::Symbol::new("amend"),
            ),
            (
                "test:record-values/values@1.2.3#append".into(),
                suss_reader::Symbol::new("append"),
            ),
        ],
    )
    .unwrap();
    let mut config = Config::new();
    config
        .wasm_gc(true)
        .wasm_function_references(true)
        .wasm_tail_call(true)
        .wasm_exceptions(true)
        .wasm_component_model(true)
        .consume_fuel(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let engine = Engine::new(&config).unwrap();
    let component = Component::new(&engine, bytes).unwrap();
    assert_eq!(component.component_type().imports(&engine).count(), 0);
    let interface = component
        .get_export_index(None, "test:record-values/values@1.2.3")
        .unwrap();
    for name in ["entry", "batch"] {
        assert!(component.get_export_index(Some(&interface), name).is_some());
    }
    let amend = component
        .get_export_index(Some(&interface), "amend")
        .unwrap();
    let append = component
        .get_export_index(Some(&interface), "append")
        .unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(100_000_000).unwrap();
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let entry = |id, label: &str, enabled: Option<bool>| {
        Val::Record(vec![
            ("id".into(), Val::U64(id)),
            ("label".into(), Val::String(label.into())),
            (
                "enabled".into(),
                Val::Option(enabled.map(|b| Box::new(Val::Bool(b)))),
            ),
        ])
    };
    let input = entry(u64::MAX, "λ\0🦀", Some(false));
    let amend = instance.get_func(&mut store, &amend).unwrap();
    let mut result = [Val::Bool(false)];
    amend
        .call(&mut store, &[input.clone()], &mut result)
        .unwrap();
    assert_eq!(result[0], entry(u64::MAX, "λ\0🦀!", Some(false)));
    store.gc(None).unwrap();
    let append = instance.get_func(&mut store, &append).unwrap();
    append
        .call(&mut store, &[Val::List(vec![input.clone()])], &mut result)
        .unwrap();
    assert_eq!(
        result[0],
        Val::List(vec![input, entry(u64::MAX, "added 🦀", None)])
    );
}

#[test]
fn empty_canonical_products_are_diagnosed_before_source_macros() {
    let source = "(defmacro explode [] (throw 17)) (defn ^:export value [x] (explode))";
    for (wit, message) in [
        (
            "package test:invalid; world api { type empty = tuple<>; export value: func(x: empty) -> empty; }",
            "tuple requires at least one",
        ),
        (
            "package test:invalid; interface values { record empty {} value: func(x: empty) -> empty; } world api { export values; }",
            "record requires at least one",
        ),
    ] {
        let error = Compiler::new()
            .compile(source, wit)
            .unwrap_err()
            .to_string();
        assert!(error.contains(message), "{error}");
        assert!(
            !error.contains("macro expansion"),
            "source macro ran: {error}"
        );
    }
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
fn nested_lists_and_optional_lists_execute_core_conj_in_sync_and_async_modes() {
    let (mut store, instance) = instantiate(
        "(defn ^:export nested [xs] (conj xs [-1]))
         (defn ^:export async-nested [xs] (nested xs))
         (defn ^:export maybe [x] (if (= x [:none]) x [:some (conj (nth x 1) 1.5)]))
         (defn ^:export async-maybe [x] (maybe x))",
        "export nested: func(xs: list<list<s32>>) -> list<list<s32>>;
         export async-nested: async func(xs: list<list<s32>>) -> list<list<s32>>;
         export maybe: func(x: option<list<f64>>) -> option<list<f64>>;
         export async-maybe: async func(x: option<list<f64>>) -> option<list<f64>>;",
    );
    let input = Val::List(vec![
        Val::List(vec![Val::S32(i32::MIN), Val::S32(i32::MAX)]),
        Val::List(vec![]),
    ]);
    let expected = Val::List(vec![
        Val::List(vec![Val::S32(i32::MIN), Val::S32(i32::MAX)]),
        Val::List(vec![]),
        Val::List(vec![Val::S32(-1)]),
    ]);
    for (name, asynchronous) in [("nested", false), ("async-nested", true)] {
        let function = instance.get_func(&mut store, name).unwrap();
        for _ in 0..20 {
            let mut output = [Val::Bool(false)];
            if asynchronous {
                complete(function.call_async(&mut store, &[input.clone()], &mut output)).unwrap();
            } else {
                function
                    .call(&mut store, &[input.clone()], &mut output)
                    .unwrap();
            }
            assert_eq!(output[0], expected);
            store.gc(None).unwrap();
        }
    }
    for name in ["maybe", "async-maybe"] {
        let function = instance
            .get_typed_func::<(Option<Vec<f64>>,), (Option<Vec<f64>>,)>(&mut store, name)
            .unwrap();
        for value in [None, Some(vec![]), Some(vec![-1.0, 0.5])] {
            let expected = value.as_ref().map(|v| {
                let mut v = v.clone();
                v.push(1.5);
                v
            });
            let output = if name.starts_with("async-") {
                complete(function.call_async(&mut store, (value,))).unwrap()
            } else {
                function.call(&mut store, (value,)).unwrap()
            };
            assert_eq!(output, (expected,));
            store.gc(None).unwrap();
        }
    }
}

#[test]
fn large_tuple_arguments_and_async_results_use_indirect_canonical_records() {
    let members = vec!["s64"; 19].join(", ");
    let (mut store, instance) = instantiate(
        "(defn ^:export rotate [x] (conj (subvec x 1) (nth x 0)))
         (defn ^:export async-rotate [x] (rotate x))",
        &format!(
            "type row = tuple<{members}>; export rotate: func(x: row) -> row;
                  export async-rotate: async func(x: row) -> row;"
        ),
    );
    let values = (0..19)
        .map(|i| Val::S64(9007199254740993 + i))
        .collect::<Vec<_>>();
    let mut expected = values[1..].to_vec();
    expected.push(values[0].clone());
    for name in ["rotate", "async-rotate"] {
        let function = instance.get_func(&mut store, name).unwrap();
        for _ in 0..20 {
            let mut output = [Val::Bool(false)];
            if name.starts_with("async-") {
                complete(function.call_async(
                    &mut store,
                    &[Val::Tuple(values.clone())],
                    &mut output,
                ))
                .unwrap();
            } else {
                function
                    .call(&mut store, &[Val::Tuple(values.clone())], &mut output)
                    .unwrap();
            }
            assert_eq!(output[0], Val::Tuple(expected.clone()));
            store.gc(None).unwrap();
        }
    }
}
