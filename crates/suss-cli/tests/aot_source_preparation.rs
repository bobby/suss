//! Source preparation shares the script pipeline without host Runtime execution.
use std::sync::OnceLock;
use std::path::Path;
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
                .wasm_component_model_implements(true)
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
    assert_eq!(
        suss_compile::portable_session::runtime_number(&mut store, &fields[0]).unwrap(),
        Some(suss_compile::portable_session::RuntimeNumber::Long(17))
    );
}

#[test]
fn ordered_namespace_initialization_keeps_scopes_and_dynamic_bindings() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("dep.sus"), "(ns dep) (def x 7)").unwrap();
    let fragments = prepare_source(
        "(ns first)\n\
         (def initial (ns-name *ns*))\n\
         (def map 1)\n\
         (def shadow (suss.core/reader-resolve-symbol 'map))\n\
         (ns second)\n\
         (def loaded (ns-name *ns*))\n\
         (ns first (:require [dep :as d :refer [x]]) (:refer-clojure :exclude [filter]))\n\
         (def resolved [(suss.core/reader-resolve-symbol 'd/x)\n\
                        (suss.core/reader-resolve-symbol 'x)\n\
                        (suss.core/reader-resolve-symbol 'filter)])\n\
         (def caught (try (binding [*ns* (the-ns 'second)] (throw 1))\n\
                          (catch :default e (ns-name *ns*))))\n\
         (set! *ns* (the-ns 'user))\n\
         (def rebound (ns-name *ns*))\n\
         (def run (fn [] (pr-str [initial shadow second/loaded resolved caught rebound (ns-name *ns*)])))",
        None,
        &[root.path().to_owned()],
    ).unwrap();
    let component = component(&fragments,
        "package test:namespaces; world w { export run: func() -> string; }", &["run"]);
    let engine = engine();
    let mut store = Store::new(&engine, ());
    store.set_fuel(100_000_000).unwrap();
    let instance = Linker::new(&engine).instantiate(&mut store, &component).unwrap();
    let run = instance.get_typed_func::<(), (String,)>(&mut store, "run").unwrap();
    let output = run.call(&mut store, ()).unwrap().0;
    run.post_return(&mut store).unwrap();
    assert_eq!(output, "[first first/map second [dep/x dep/x first/filter] first first suss.core]");
}

#[test]
fn published_catalogs_prepare_independent_fragments_with_namespace_recovery() {
    use suss_compile::{portable_analyze::analyze_source, portable_session::{RuntimeNumber, runtime_number}, runtime_abi};
    use wasmtime::{Instance as CoreInstance, Linker as CoreLinker, Module, Rooted, Val as CoreVal};
    let source = "(ns catalog.app) (def kept 7)";
    let prepared = prepare_source(source, None, &[]).unwrap();
    let analysis = analyze_source(source, Path::new("catalog.sus"), &[], None, None).unwrap();
    for environment in [prepared.last().unwrap().environment.clone(), analysis.last().unwrap().environment.clone()] {
        // Neither catalog's AOT source has run. Ordinary preparation must
        // publish the missing namespace and own its binding, including errors.
        let fragment = portable::prepare_fragment("(throw 17)", &environment, portable::resolve::Phase::Runtime).unwrap();
        let bindings = portable::core_bindings::compile_with_cells(portable::resolve::Phase::Runtime, &fragment.cells).unwrap();
        let engine = engine();
        let mut store = Store::new(&engine, ());
        store.set_fuel(100_000_000).unwrap();
        let runtime = CoreInstance::new(&mut store, &Module::new(&engine, runtime_abi::module()).unwrap(), &[]).unwrap();
        let mut linker = CoreLinker::new(&engine);
        linker.instance(&mut store, "suss.runtime", runtime).unwrap();
        let bindings = linker.instantiate(&mut store, &Module::new(&engine, bindings.wasm).unwrap()).unwrap();
        linker.instance(&mut store, "suss.bindings.runtime", bindings).unwrap();
        let core = linker.instantiate(&mut store, &Module::new(&engine, &prepared[0].wasm).unwrap()).unwrap();
        let mut result = [CoreVal::null_any_ref()];
        core.get_func(&mut store, "eval").unwrap().call(&mut store, &[], &mut result).unwrap();
        let global = environment.resolve(portable::resolve::Phase::Runtime,
            &Symbol::namespaced("suss.core", "*ns*"), 0..0).unwrap().global().clone();
        let cell = bindings.get_global(&mut store, &global.import_name()).unwrap().get(&mut store);
        let get = runtime.get_func(&mut store, "binding-get").unwrap();
        get.call(&mut store, &[cell], &mut result).unwrap();
        let before = result[0];
        let compiled = portable::compile_in("(throw 17)", &environment, portable::resolve::Phase::Runtime).unwrap();
        for wasm in [fragment.wasm, compiled] {
            let instance = linker.instantiate(&mut store, &Module::new(&engine, wasm).unwrap()).unwrap();
            assert!(instance.get_func(&mut store, "eval").unwrap().call(&mut store, &[], &mut result).is_err());
            let exception = store.as_context_mut().take_pending_exception().expect("language throw");
            let fields = exception.fields(&mut store).unwrap().collect::<Vec<_>>();
            assert_eq!(runtime_number(&mut store, &fields[0]).unwrap(), Some(RuntimeNumber::Long(17)), "catalog cannot leak AOT execution policy");
            store.gc(None).unwrap();
            get.call(&mut store, &[cell], &mut result).unwrap();
            assert!(Rooted::ref_eq(&store, before.unwrap_anyref().unwrap(), result[0].unwrap_anyref().unwrap()).unwrap());
            runtime.get_func(&mut store, "dynamic-save").unwrap().call(&mut store, &[], &mut result).unwrap();
            assert!(result[0].unwrap_anyref().is_none());
        }
    }
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

#[test]
fn compiled_source_macro_executes_through_a_versioned_exported_interface() {
    let source = "(ns app) (def seen 0) (defmacro twice [x] `(+ ~x ~x)) (def calculate (fn [x] (do (set! seen (+ seen 1)) (twice x)))) (def effects (fn [] seen)) (set! seen (+ seen 1))";
    let fragments = prepare_source(source, None, &[]).unwrap();
    assert!(fragments.iter().any(|fragment| {
        portable::artifact_identity::read(&fragment.wasm)
            .unwrap()
            .macro_dependencies
            .as_ref()
            .is_some_and(|graph| graph.iter().any(|(name, _)| name == "macro:app/twice"))
    }));
    let mut resolve = portable::aot::Resolve::new();
    let package = resolve.push_str("api.wit", "package test:source-interface@0.2.0; interface api { calculate: func(x: u32) -> u32; effects: func() -> f64; } world api-world { export api; }").unwrap();
    let world = *resolve.packages[package].worlds.values().next().unwrap();
    let bytes = portable::aot::component(
        &fragments,
        &resolve,
        world,
        &[
            (
                "test:source-interface/api@0.2.0#calculate".into(),
                Symbol::new("calculate"),
            ),
            (
                "test:source-interface/api@0.2.0#effects".into(),
                Symbol::new("effects"),
            ),
        ],
    )
    .unwrap();
    let engine = engine();
    let component = Component::new(&engine, bytes).unwrap();
    let api = component
        .get_export_index(None, "test:source-interface/api@0.2.0")
        .unwrap();
    let calculate = component.get_export_index(Some(&api), "calculate").unwrap();
    let effects = component.get_export_index(Some(&api), "effects").unwrap();
    for _ in 0..2 {
        let mut store = Store::new(&engine, ());
        store.set_fuel(100_000_000).unwrap();
        let instance = Linker::new(&engine)
            .instantiate(&mut store, &component)
            .unwrap();
        let calculate = instance
            .get_typed_func::<(u32,), (u32,)>(&mut store, &calculate)
            .unwrap();
        let effects = instance
            .get_typed_func::<(), (f64,)>(&mut store, &effects)
            .unwrap();
        assert_eq!(
            effects.call(&mut store, ()).unwrap().0.to_bits(),
            1.0_f64.to_bits()
        );
        effects.post_return(&mut store).unwrap();
        assert_eq!(calculate.call(&mut store, (21,)).unwrap().0, 42);
        calculate.post_return(&mut store).unwrap();
        assert_eq!(
            effects.call(&mut store, ()).unwrap().0.to_bits(),
            2.0_f64.to_bits()
        );
        effects.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
        assert_eq!(calculate.call(&mut store, (3,)).unwrap().0, 6);
        calculate.post_return(&mut store).unwrap();
        assert_eq!(
            effects.call(&mut store, ()).unwrap().0.to_bits(),
            3.0_f64.to_bits()
        );
        effects.post_return(&mut store).unwrap();
    }
}
