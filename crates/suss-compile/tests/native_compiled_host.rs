//! Compiler-library clients use the same compiled phase/session host as the CLI.
#![cfg(not(target_family = "wasm"))]

use suss_compile::{
    portable::resolve::Phase, portable_macro_data::FormBridge, portable_macros::CompiledMacros,
    portable_session::Session,
};
use suss_reader::forms::Kind;
use wasmtime::AsContextMut;

#[test]
fn compiler_embedding_executes_macros_and_retains_live_values_across_fragments() {
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    macros
        .define("(defmacro plus-two [x] (list '+ x 2))")
        .unwrap();
    let result = session
        .eval_with_macros(
            "(def state (atom 40)) (plus-two (swap! state inc))",
            &mut macros,
        )
        .unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    assert!(matches!(
        bridge.read(&mut session, &result, 0..1).unwrap().kind,
        Kind::Number(43.0)
    ));
    let saved = session.eval("(fn [] @state)").unwrap();
    session.eval("(swap! state inc)").unwrap();
    session.collect().unwrap();
    let result = session.invoke(&saved, &[]).unwrap();
    assert!(matches!(
        bridge.read(&mut session, &result, 0..1).unwrap().kind,
        Kind::Number(42.0)
    ));
    assert_eq!(session.phase(), Phase::Runtime);
    let mut isolated = Session::new_macro().unwrap();
    assert_eq!(isolated.phase(), Phase::Macro);
    assert!(
        isolated.invoke(&saved, &[]).is_err(),
        "a phase Store must reject another Store's rooted closure"
    );
}

#[test]
fn compiler_embedding_prepares_aot_without_executing_runtime_initializers() {
    let source = "(defmacro answer [] 42) (def initialized (throw (answer)))";
    let fragments = suss_compile::portable_aot::prepare_source(source, None, &[]).unwrap();
    let mut resolve = suss_compile::portable::aot::Resolve::new();
    let package = resolve
        .push_str("api.wit", "package test:compiler-host; world api {}")
        .unwrap();
    let world = *resolve.packages[package].worlds.values().next().unwrap();
    let bytes = suss_compile::portable::aot::component(&fragments, &resolve, world, &[]).unwrap();
    let mut config = wasmtime::Config::new();
    config
        .wasm_gc(true)
        .wasm_function_references(true)
        .wasm_tail_call(true)
        .wasm_exceptions(true)
        .wasm_component_model(true)
        .wasm_component_model_implements(true)
        .consume_fuel(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let engine = wasmtime::Engine::new(&config).unwrap();
    let component = wasmtime::component::Component::new(&engine, bytes).unwrap();
    let mut store = wasmtime::Store::new(&engine, ());
    store.set_fuel(100_000_000).unwrap();
    assert!(
        wasmtime::component::Linker::new(&engine)
            .instantiate(&mut store, &component)
            .is_err()
    );
    let exception = store
        .as_context_mut()
        .take_pending_exception()
        .expect("the Runtime initializer must throw a language exception");
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
        42.0_f64.to_bits()
    );
}
