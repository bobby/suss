//! Compiled macro expansion feeds the ordinary portable AOT artifact boundary.
use suss_cli::portable_macros::CompiledMacros;
use suss_compile::portable::{self, resolve::Phase, SourceOrigin};
use suss_reader::{forms::read_forms, Symbol};
use wasmtime::{Config, Engine, Store};

#[test]
fn compiled_macro_source_expands_into_an_executable_component_without_replay() {
    let mut macros = CompiledMacros::new().unwrap();
    macros.define("(defmacro twice [x] `(+ ~x ~x))").unwrap();
    let source = "(ns user) (def seen 0) (def calculate (fn [x] (twice (do (set! seen (+ seen 1)) x)))) (def effects (fn [] seen))";
    let core = portable::bootstrap::shipped(Phase::Runtime).unwrap();
    let user = portable::prepare_fragment_forms_with_origin(
        read_forms(source).unwrap(),
        0..source.len(),
        &core.environment,
        Phase::Runtime,
        &mut macros,
        Some(&SourceOrigin::new(source, Some("aot.sus".into()))),
    )
    .unwrap();
    let manifest = portable::artifact_identity::read(&user.wasm).unwrap();
    assert!(manifest
        .macro_dependencies
        .unwrap()
        .iter()
        .any(|(name, _)| name == "macro:user/twice"));
    let mut resolve = portable::aot::Resolve::new();
    let package = resolve.push_str("api.wit", "package test:compiled-aot; world api { export calculate: func(x: f64) -> f64; export effects: func() -> f64; }").unwrap();
    let world = *resolve.packages[package].worlds.values().next().unwrap();
    let bytes = portable::aot::component(
        &[core.clone(), user],
        &resolve,
        world,
        &[
            ("calculate".into(), Symbol::new("calculate")),
            ("effects".into(), Symbol::new("effects")),
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
        .wasm_component_model_implements(true)
        .consume_fuel(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let engine = Engine::new(&config).unwrap();
    let component = wasmtime::component::Component::new(&engine, bytes).unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(100_000_000).unwrap();
    let instance = wasmtime::component::Linker::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let effects = instance
        .get_typed_func::<(), (f64,)>(&mut store, "effects")
        .unwrap();
    assert_eq!(
        effects.call(&mut store, ()).unwrap().0.to_bits(),
        0.0_f64.to_bits()
    );
    effects.post_return(&mut store).unwrap();
    let calculate = instance
        .get_typed_func::<(f64,), (f64,)>(&mut store, "calculate")
        .unwrap();
    assert_eq!(
        calculate.call(&mut store, (21.0,)).unwrap().0.to_bits(),
        42.0_f64.to_bits()
    );
    calculate.post_return(&mut store).unwrap();
    assert_eq!(
        effects.call(&mut store, ()).unwrap().0.to_bits(),
        2.0_f64.to_bits()
    );
    effects.post_return(&mut store).unwrap();
    store.gc(None).unwrap();
    assert_eq!(
        calculate.call(&mut store, (3.0,)).unwrap().0.to_bits(),
        6.0_f64.to_bits()
    );
    calculate.post_return(&mut store).unwrap();
    assert_eq!(
        effects.call(&mut store, ()).unwrap().0.to_bits(),
        4.0_f64.to_bits()
    );
    effects.post_return(&mut store).unwrap();
}
