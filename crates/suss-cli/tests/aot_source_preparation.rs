//! Source preparation shares the script pipeline without host Runtime execution.
use std::sync::OnceLock;
use suss_cli::portable_aot::prepare_source;
use suss_compile::portable;
use suss_reader::Symbol;
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
                .consume_fuel(true)
                .cranelift_opt_level(wasmtime::OptLevel::None);
            Engine::new(&config).unwrap()
        })
        .clone()
}
fn component(fragments: &[portable::PreparedFragment], wit: &str, names: &[&str]) -> Component {
    let mut resolve = portable::aot::Resolve::new();
    let package = resolve.push_str("api.wit", wit).unwrap();
    let world = *resolve.packages[package].worlds.values().next().unwrap();
    let mappings = names
        .iter()
        .map(|name| (name.to_string(), Symbol::new(*name)))
        .collect::<Vec<_>>();
    let bytes = portable::aot::component(fragments, &resolve, world, &mappings).unwrap();
    Component::new(&engine(), bytes).unwrap()
}

#[test]
fn source_macro_and_runtime_dependency_feed_the_same_component_pipeline() {
    let root = tempfile::tempdir().unwrap();
    let math_source = "(ns math) (def bias 2)";
    std::fs::write(root.path().join("math.sus"), math_source).unwrap();
    let math_path = root.path().join("math.sus").canonicalize().unwrap();
    let source = "(ns app (:require [math :as m])) (def seen 0) (defmacro twice [x] `(+ ~x ~x)) (def calculate (fn [x] (twice (do (set! seen (+ seen 1)) (+ x m/bias))))) (def effects (fn [] seen)) (set! seen (+ seen 1))";
    let path = root.path().join("app.sus");
    let fragments =
        prepare_source(source, Some(path.clone()), &[root.path().to_path_buf()]).unwrap();
    let identities = fragments
        .iter()
        .map(|fragment| portable::artifact_identity::read(&fragment.wasm).unwrap())
        .collect::<Vec<_>>();
    assert!(identities.iter().any(|identity| {
        identity.source_path.as_deref() == path.to_str()
            && identity
                .macro_dependencies
                .as_ref()
                .is_some_and(|graph| graph.iter().any(|(name, _)| name == "macro:app/twice"))
    }));
    assert!(
        identities.iter().any(
            |identity| identity.source_path.as_deref() == math_path.to_str()
                && identity.source_sha256.as_deref()
                    == Some(portable::bootstrap::sha256(math_source.as_bytes()).as_str())
        )
    );
    let component = component(
        &fragments,
        "package test:source; world api { export calculate: func(x: f64) -> f64; export effects: func() -> f64; }",
        &["calculate", "effects"],
    );
    let engine = engine();
    let mut store = Store::new(&engine, ());
    store.set_fuel(100_000_000).unwrap();
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let effects = instance
        .get_typed_func::<(), (f64,)>(&mut store, "effects")
        .unwrap();
    assert_eq!(effects.call(&mut store, ()).unwrap().0, 1.0);
    effects.post_return(&mut store).unwrap();
    let calculate = instance
        .get_typed_func::<(f64,), (f64,)>(&mut store, "calculate")
        .unwrap();
    assert_eq!(calculate.call(&mut store, (21.0,)).unwrap().0, 46.0);
    calculate.post_return(&mut store).unwrap();
    assert_eq!(effects.call(&mut store, ()).unwrap().0, 3.0);
    effects.post_return(&mut store).unwrap();
    store.gc(None).unwrap();
    assert_eq!(calculate.call(&mut store, (1.0,)).unwrap().0, 6.0);
    calculate.post_return(&mut store).unwrap();
    assert_eq!(effects.call(&mut store, ()).unwrap().0, 5.0);
    effects.post_return(&mut store).unwrap();
}

#[test]
fn source_preparation_defers_runtime_throw_until_component_instantiation() {
    let fragments = prepare_source("(throw 17) (throw 99)", None, &[]).unwrap();
    let component = component(&fragments, "package test:deferred; world api {}", &[]);
    let engine = engine();
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
        .expect("Runtime throw must retain its language payload");
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
        17.0_f64.to_bits()
    );
}

#[test]
fn source_preparation_deduplicates_diamond_dependencies_and_isolates_macro_state() {
    let root = tempfile::tempdir().unwrap();
    for (name, source) in [
        (
            "common",
            "(ns common) (def calls 0) (set! calls (+ calls 1))",
        ),
        (
            "left",
            "(ns left (:require [common :as c])) (set! c/calls (+ c/calls 10)) (def first c/calls)",
        ),
        (
            "right",
            "(ns right (:require [common :as c])) (set! c/calls (+ c/calls 100)) (def first c/calls)",
        ),
        (
            "tools",
            "(ns tools (:require [common :as c])) (defmacro observed [] c/calls)",
        ),
    ] {
        std::fs::write(root.path().join(format!("{name}.sus")), source).unwrap();
    }
    let source = "(ns app (:require [left :as l] [right :as r] [common :as c]) (:require-macros [tools :as t])) (set! c/calls (+ c/calls 1)) (ns app (:require [left :as l] [right :as r] [common :as c]) (:require-macros [tools :as t])) (def effects (fn [] c/calls)) (def phase-value (fn [] (t/observed))) (def left-value (fn [] l/first)) (def right-value (fn [] r/first))";
    let fragments = prepare_source(source, None, &[root.path().to_owned()]).unwrap();
    let module_paths = fragments
        .iter()
        .filter_map(|fragment| {
            portable::artifact_identity::read(&fragment.wasm)
                .unwrap()
                .source_path
        })
        .collect::<Vec<_>>();
    assert_eq!(
        module_paths,
        ["common", "left", "right"].map(|name| root
            .path()
            .join(format!("{name}.sus"))
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned())
    );
    let component = component(
        &fragments,
        "package test:diamond; world api { export effects: func() -> f64; export phase-value: func() -> f64; export left-value: func() -> f64; export right-value: func() -> f64; }",
        &["effects", "phase-value", "left-value", "right-value"],
    );
    let mut store = Store::new(&engine(), ());
    store.set_fuel(100_000_000).unwrap();
    let instance = Linker::new(&engine())
        .instantiate(&mut store, &component)
        .unwrap();
    store.gc(None).unwrap();
    for (name, expected) in [
        ("effects", 112.0),
        ("phase-value", 1.0),
        ("left-value", 11.0),
        ("right-value", 111.0),
    ] {
        let function = instance
            .get_typed_func::<(), (f64,)>(&mut store, name)
            .unwrap();
        assert_eq!(function.call(&mut store, ()).unwrap().0, expected, "{name}");
        function.post_return(&mut store).unwrap();
    }
}
