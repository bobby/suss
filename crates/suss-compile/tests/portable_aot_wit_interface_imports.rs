//! Execute named and versioned interface providers with their actual type identities.
#![cfg(not(target_family = "wasm"))]
use std::sync::OnceLock;
use suss_compile::portable_aot::{AotOptions, compile_file_with_options};
use suss_reader::Symbol;
use wasmtime::{
    Config, Engine, Store, StoreContextMut,
    component::{Component, Linker, Val, types::ComponentItem},
};

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut config = Config::new();
        config
            .wasm_gc(true)
            .gc_heap_initial_size(8 * 1024 * 1024)
            .wasm_function_references(true)
            .wasm_tail_call(true)
            .wasm_exceptions(true)
            .wasm_component_model(true)
            .wasm_component_model_async(true)
            .wasm_component_model_map(true)
            .wasm_component_model_fixed_length_lists(true)
            .wasm_component_model_implements(true)
            .consume_fuel(true)
            .cranelift_opt_level(wasmtime::OptLevel::None);
        Engine::new(&config).unwrap()
    })
}
fn compile(source: &str, wit: &str, mappings: &[(&str, &str)]) -> Component {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.sus");
    let wit_path = directory.path().join("api.wit");
    std::fs::write(&path, source).unwrap();
    std::fs::write(&wit_path, wit).unwrap();
    let options = AotOptions {
        exports: vec![("run".into(), Symbol::namespaced("app", "run"))],
        imports: mappings
            .iter()
            .map(|(path, source)| ((*path).into(), Symbol::parse(source)))
            .collect(),
        ..Default::default()
    };
    let bytes = compile_file_with_options(
        &path,
        &wit_path,
        Some("api"),
        &[directory.path().to_owned()],
        &options,
    )
    .unwrap();
    Component::new(engine(), bytes).unwrap()
}
#[derive(Default)]
struct State {
    order: Vec<&'static str>,
}
fn store() -> Store<State> {
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    store
}

#[test]
fn versioned_interface_alias_preserves_nominal_types_and_native_source_updates() {
    let component = compile(
        "(ns app) (defn run [x] (let [y (host/touch x)] (update (assoc y :state :active) :permissions conj :write)))",
        "package test:service@1.2.3; interface data { enum state { idle, active } flags permissions { read, write } record row { id: u64, label: string, state: state, permissions: permissions } type rows = list<row>; touch: func(value: row) -> row; } world api { import operations: data; use data.{row}; export run: func(value: row) -> row; }",
        &[
            ("operations#touch", "host/touch"),
            ("test:service/data@1.2.3#touch", "host/original-touch"),
        ],
    );
    let annotations = component
        .component_type()
        .imports(engine())
        .map(|(name, item)| (name.to_owned(), item.implements.map(str::to_owned)))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        annotations["operations"].as_deref(),
        Some("test:service/data@1.2.3")
    );
    assert_eq!(annotations["test:service/data@1.2.3"], None);
    let interfaces = component
        .component_type()
        .imports(engine())
        .filter_map(|(name, item)| match item.ty {
            ComponentItem::ComponentInstance(instance) => Some((name.to_owned(), instance)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(interfaces.len(), 2);
    let operations = interfaces
        .iter()
        .find(|(name, _)| name == "operations")
        .unwrap();
    assert!(
        interfaces
            .iter()
            .any(|(name, _)| name == "test:service/data@1.2.3")
    );
    let names = operations
        .1
        .exports(engine())
        .map(|(name, _)| name.to_owned())
        .collect::<Vec<_>>();
    for name in ["state", "permissions", "row", "rows", "touch"] {
        assert!(names.iter().any(|actual| actual == name), "{names:?}");
    }
    let mut linker = Linker::new(engine());
    linker
        .instance("test:service/data@1.2.3")
        .unwrap()
        .func_new("touch", |_context, _ty, _params, _results| {
            panic!("source called the original provider instead of its selected alias")
        })
        .unwrap();
    linker
        .instance("operations")
        .unwrap()
        .func_new(
            "touch",
            |mut context: StoreContextMut<'_, State>, _ty, params, results| {
                context.data_mut().order.push("touch");
                let mut row = params[0].clone();
                let Val::Record(fields) = &mut row else {
                    panic!("wrong host argument")
                };
                let Val::String(label) = &mut fields[1].1 else {
                    panic!("wrong host label")
                };
                label.push_str(" host");
                results[0] = row;
                Ok(())
            },
        )
        .unwrap();
    let mut store = store();
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let run = instance.get_func(&mut store, "run").unwrap();
    let input = Val::Record(vec![
        ("id".into(), Val::U64(u64::MAX)),
        ("label".into(), Val::String("λ\0🦀".into())),
        ("state".into(), Val::Enum("idle".into())),
        ("permissions".into(), Val::Flags(vec!["read".into()])),
    ]);
    let mut expected = input.clone();
    let Val::Record(fields) = &mut expected else {
        unreachable!()
    };
    fields[1].1 = Val::String("λ\0🦀 host".into());
    fields[2].1 = Val::Enum("active".into());
    fields[3].1 = Val::Flags(vec!["read".into(), "write".into()]);
    for _ in 0..20 {
        store.set_fuel(100_000_000).unwrap();
        let mut output = [Val::Bool(false)];
        run.call(&mut store, &[input.clone()], &mut output).unwrap();
        assert_eq!(output[0], expected);
        store.gc(None).unwrap();
    }
    assert_eq!(store.data().order, vec!["touch"; 20]);
}

#[test]
fn mixed_world_functions_and_two_aliases_of_one_interface_keep_provider_indices_and_order() {
    let component = compile(
        "(ns app) (defn run [x] (+ (host/seed x) (host/left x) (host/right x)))",
        "package test:aliases@2.0.0; interface operations { step: func(x: s64) -> s64; } world api { import seed: func(x: s64) -> s64; import left: operations; import right: operations; export run: func(x: s64) -> s64; }",
        &[
            ("seed", "host/seed"),
            ("left#step", "host/left"),
            ("right#step", "host/right"),
        ],
    );
    assert_eq!(component.component_type().imports(engine()).count(), 3);
    let mut linker = Linker::new(engine());
    linker
        .root()
        .func_wrap(
            "seed",
            |mut context: StoreContextMut<'_, State>, (x,): (i64,)| {
                context.data_mut().order.push("seed");
                Ok((x + 1,))
            },
        )
        .unwrap();
    linker
        .instance("left")
        .unwrap()
        .func_wrap(
            "step",
            |mut context: StoreContextMut<'_, State>, (x,): (i64,)| {
                context.data_mut().order.push("left");
                Ok((x + 10,))
            },
        )
        .unwrap();
    linker
        .instance("right")
        .unwrap()
        .func_wrap(
            "step",
            |mut context: StoreContextMut<'_, State>, (x,): (i64,)| {
                context.data_mut().order.push("right");
                Ok((x + 100,))
            },
        )
        .unwrap();
    let mut store = store();
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let run = instance
        .get_typed_func::<(i64,), (i64,)>(&mut store, "run")
        .unwrap();
    for x in [0, 9007199254740993] {
        assert_eq!(run.call(&mut store, (x,)).unwrap().0, 3 * x + 111);
        store.gc(None).unwrap();
    }
    assert_eq!(store.data().order, ["seed", "left", "right"].repeat(2));
}

#[test]
fn shared_nominal_types_alias_the_actual_earlier_interface_provider() {
    let component = compile(
        "(ns app) (defn run [x] (let [y (host/touch x)] (assoc y :label (str (:label y) \"!\"))))",
        "package test:shared@1.2.3; interface model { enum state { idle, active } record row { id: u64, label: string, state: state } } interface operations { use model.{row}; touch: func(value: row) -> row; } world api { import model; import operations; use model.{row}; export run: func(value: row) -> row; }",
        &[("test:shared/operations@1.2.3#touch", "host/touch")],
    );
    let mut linker = Linker::new(engine());
    linker.instance("test:shared/model@1.2.3").unwrap();
    linker
        .instance("test:shared/operations@1.2.3")
        .unwrap()
        .func_new(
            "touch",
            |mut context: StoreContextMut<'_, State>, _ty, params, results| {
                context.data_mut().order.push("touch");
                let mut row = params[0].clone();
                let Val::Record(fields) = &mut row else {
                    panic!("wrong row")
                };
                let Val::String(label) = &mut fields[1].1 else {
                    panic!("wrong label")
                };
                label.push_str(" host");
                results[0] = row;
                Ok(())
            },
        )
        .unwrap();
    let mut store = store();
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let run = instance.get_func(&mut store, "run").unwrap();
    let input = Val::Record(vec![
        ("id".into(), Val::U64(u64::MAX)),
        ("label".into(), Val::String("λ🦀".into())),
        ("state".into(), Val::Enum("idle".into())),
    ]);
    let mut expected = input.clone();
    let Val::Record(fields) = &mut expected else {
        unreachable!()
    };
    fields[1].1 = Val::String("λ🦀 host!".into());
    let mut output = [Val::Bool(false)];
    run.call(&mut store, &[input], &mut output).unwrap();
    assert_eq!(output[0], expected);
    assert_eq!(store.data().order, ["touch"]);
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
fn asynchronous_export_calls_synchronous_interface_and_returns_nested_owned_values() {
    let component = compile(
        "(ns app) (defn run [xs] (conj (host/echo xs) [42]))",
        "package test:interface-async; interface operations { echo: func(xs: list<list<s64>>) -> list<list<s64>>; } world api { import operations; export run: async func(xs: list<list<s64>>) -> list<list<s64>>; }",
        &[("test:interface-async/operations#echo", "host/echo")],
    );
    let mut linker = Linker::new(engine());
    linker
        .instance("test:interface-async/operations")
        .unwrap()
        .func_new(
            "echo",
            |mut context: StoreContextMut<'_, State>, _ty, params, results| {
                context.data_mut().order.push("echo");
                results[0] = params[0].clone();
                Ok(())
            },
        )
        .unwrap();
    let mut store = store();
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let run = instance.get_func(&mut store, "run").unwrap();
    let input = Val::List(vec![
        Val::List(vec![Val::S64(i64::MIN), Val::S64(i64::MAX)]),
        Val::List(vec![]),
    ]);
    let Val::List(mut expected) = input.clone() else {
        unreachable!()
    };
    expected.push(Val::List(vec![Val::S64(42)]));
    let expected = Val::List(expected);
    for _ in 0..20 {
        store.set_fuel(100_000_000).unwrap();
        let mut output = [Val::Bool(false)];
        complete(run.call_async(&mut store, &[input.clone()], &mut output)).unwrap();
        assert_eq!(output[0], expected);
        store.gc(None).unwrap();
    }
    assert_eq!(store.data().order, vec!["echo"; 20]);
}

#[test]
fn interface_authority_and_selected_types_reject_before_source_macro_effects() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.sus");
    let wit = directory.path().join("api.wit");
    std::fs::write(
        &path,
        r#"(ns app) (defmacro effect [] (throw "macro-ran")) (defn run [] (effect))"#,
    )
    .unwrap();
    for (declarations, mappings, expected) in [
        (
            "interface operations { first: func(); second: func(); } world api { import operations; export run: func(); }",
            vec![("test:guards/operations#first", "host/first")],
            "second needs an explicit source var mapping",
        ),
        (
            "interface operations { type pending = future<string>; first: func(); } world api { import operations; export run: func(); }",
            vec![("test:guards/operations#first", "host/first")],
            "future",
        ),
        (
            "interface operations { first: async func(); } world api { import operations; export run: func(); }",
            vec![("test:guards/operations#first", "host/first")],
            "freestanding",
        ),
        (
            "interface operations { resource item; } world api { import operations; export run: func(); }",
            vec![("unknown", "host/first")],
            "resource",
        ),
        (
            "interface operations { type pending = future<string>; } world api { import operations; export run: func(); }",
            vec![],
            "future",
        ),
        (
            "interface operations { resource item; } world api { import operations; export run: func(); }",
            vec![],
            "resource",
        ),
    ] {
        std::fs::write(&wit, format!("package test:guards; {declarations}")).unwrap();
        let error = compile_file_with_options(
            &path,
            &wit,
            None,
            &[],
            &AotOptions {
                imports: mappings
                    .into_iter()
                    .map(|(path, source)| (path.into(), Symbol::parse(source)))
                    .collect(),
                exports: vec![("run".into(), Symbol::namespaced("app", "run"))],
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("macro-ran"), "{error}");
    }
}

#[test]
fn imported_provider_and_exported_interface_keep_separate_component_instance_indices() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("app.sus");
    let wit = directory.path().join("api.wit");
    std::fs::write(&source, "(ns app) (defn run [x] (host/step x))").unwrap();
    std::fs::write(&wit, "package test:both@1.0.0; interface operations { step: func(x: s64) -> s64; } interface public { run: func(x: s64) -> s64; } world api { import operations; export public; }").unwrap();
    let bytes = compile_file_with_options(
        &source,
        &wit,
        None,
        &[],
        &AotOptions {
            imports: vec![(
                "test:both/operations@1.0.0#step".into(),
                Symbol::namespaced("host", "step"),
            )],
            exports: vec![(
                "test:both/public@1.0.0#run".into(),
                Symbol::namespaced("app", "run"),
            )],
            ..Default::default()
        },
    )
    .unwrap();
    let component = Component::new(engine(), bytes).unwrap();
    let mut linker = Linker::new(engine());
    linker
        .instance("test:both/operations@1.0.0")
        .unwrap()
        .func_wrap(
            "step",
            |mut context: StoreContextMut<'_, State>, (x,): (i64,)| {
                context.data_mut().order.push("step");
                Ok((x + 1,))
            },
        )
        .unwrap();
    let mut store = store();
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let (_, interface) = instance
        .get_export(&mut store, None, "test:both/public@1.0.0")
        .unwrap();
    let (_, function) = instance
        .get_export(&mut store, Some(&interface), "run")
        .unwrap();
    let run = instance
        .get_typed_func::<(i64,), (i64,)>(&mut store, function)
        .unwrap();
    assert_eq!(
        run.call(&mut store, (9007199254740993,)).unwrap().0,
        9007199254740994
    );
    assert_eq!(store.data().order, ["step"]);
}

#[test]
fn schema_only_interface_data_values_need_no_function_authority_or_runtime_provider() {
    let bytes = suss_compile::Compiler::new().compile(
        "(defn ^:export run [x] (update (assoc x :state :active) :values conj 42))",
        "package test:types-only@1.2.3; interface model { enum state { idle, active } record row { id: u64, state: state, values: list<s64> } } world api { import model; use model.{row}; export run: func(value: row) -> row; }",
    ).unwrap();
    let component = Component::new(engine(), bytes).unwrap();
    let declared = component
        .component_type()
        .imports(engine())
        .map(|(name, _)| name.to_owned())
        .collect::<Vec<_>>();
    assert_eq!(declared, ["test:types-only/model@1.2.3"]);
    let mut store = store();
    // Schema-only instance imports carry no runtime function or resource.
    let instance = Linker::new(engine())
        .instantiate(&mut store, &component)
        .unwrap();
    let run = instance.get_func(&mut store, "run").unwrap();
    let input = Val::Record(vec![
        ("id".into(), Val::U64(u64::MAX)),
        ("state".into(), Val::Enum("idle".into())),
        (
            "values".into(),
            Val::List(vec![Val::S64(i64::MIN), Val::S64(i64::MAX)]),
        ),
    ]);
    let expected = Val::Record(vec![
        ("id".into(), Val::U64(u64::MAX)),
        ("state".into(), Val::Enum("active".into())),
        (
            "values".into(),
            Val::List(vec![Val::S64(i64::MIN), Val::S64(i64::MAX), Val::S64(42)]),
        ),
    ]);
    for _ in 0..20 {
        store.set_fuel(100_000_000).unwrap();
        let mut output = [Val::Bool(false)];
        run.call(&mut store, &[input.clone()], &mut output).unwrap();
        assert_eq!(output[0], expected);
        store.gc(None).unwrap();
    }
}

#[test]
fn schema_only_interface_keeps_automatic_random_provider_and_export_indices() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("app.sus");
    let wit = directory.path().join("wit");
    std::fs::create_dir_all(wit.join("deps")).unwrap();
    std::fs::write(&source, "(ns app) (defn run [] (rand))").unwrap();
    std::fs::write(wit.join("deps/random.wit"), "package wasi:random@0.3.1; interface random { get-random-bytes: func(len: u64) -> list<u8>; get-random-u64: func() -> u64; }").unwrap();
    std::fs::write(wit.join("api.wit"), "package test:typed-random; interface model { enum state { idle, active } } world api { import model; import wasi:random/random@0.3.1; export run: func() -> f64; }").unwrap();
    let bytes = compile_file_with_options(
        &source,
        &wit,
        Some("api"),
        &[],
        &AotOptions {
            exports: vec![("run".into(), Symbol::namespaced("app", "run"))],
            ..Default::default()
        },
    )
    .unwrap();
    let component = Component::new(engine(), bytes).unwrap();
    let mut linker = Linker::new(engine());
    let mut random = linker.instance("wasi:random/random@0.3.1").unwrap();
    random
        .func_wrap(
            "get-random-u64",
            |mut context: StoreContextMut<'_, State>, ()| {
                let index = context.data().order.len();
                context.data_mut().order.push("random");
                Ok(([0_u64, 1 << 63, u64::MAX][index],))
            },
        )
        .unwrap();
    random
        .func_wrap(
            "get-random-bytes",
            |_, (_length,): (u64,)| -> wasmtime::Result<(Vec<u8>,)> {
                panic!("scalar sampling must request words")
            },
        )
        .unwrap();
    let mut store = store();
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let run = instance
        .get_typed_func::<(), (f64,)>(&mut store, "run")
        .unwrap();
    for bits in [0, 0x3fe0000000000000, 0x3fefffffffffffff] {
        assert_eq!(run.call(&mut store, ()).unwrap().0.to_bits(), bits);
        store.gc(None).unwrap();
    }
    assert_eq!(store.data().order, ["random"; 3]);
}
