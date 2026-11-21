//! Bound real global call-site growth, with compiled core/IFn and executed effects.
use suss_compile::portable::{self, resolve::Phase};
use suss_reader::Symbol;
use wasmtime::{Config, Engine, Store};
use wasmtime::component::{Component, Linker};

#[test]
fn global_call_sites_stay_small_with_real_ifn_dispatch() {
    let core = portable::bootstrap::shipped(Phase::Runtime).unwrap().clone();
    let mut config = Config::new();
    config.wasm_gc(true).wasm_function_references(true).wasm_tail_call(true)
        .wasm_exceptions(true).wasm_component_model(true)
        .wasm_component_model_implements(true).consume_fuel(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let engine = Engine::new(&config).unwrap();
    let mut resolve = wit_parser::Resolve::new();
    let package = resolve.push_str("api.wit",
        "package test:call-size; world api { export run: func() -> f64; }").unwrap();
    let world = *resolve.packages[package].worlds.values().next().unwrap();
    for (arity, params, operands, increment, maximum) in [
        (0, "", "", "1", 160),
        (4, "a b c d", " 1 2 3 4", "a", 220),
    ] {
        let mut sizes = Vec::new();
        for count in [2, 10] {
            let calls = format!("(f{operands}) ").repeat(count);
            // Explicitly keep the var callable at run time rather than linking
            // a known definition directly. Every measured call is non-tail.
            let source = format!("(ns bounds) (def effects 0) (def ^:redef f (fn [{params}] (set! effects (+ effects {increment})))) (def run (fn [] (do {calls}effects)))");
            let fragment = portable::prepare_fragment(&source, &core.environment, Phase::Runtime).unwrap();
            let bytes = portable::aot::component(&[core.clone(), fragment], &resolve, world,
                &[("run".to_owned(), Symbol::namespaced("bounds", "run"))]).unwrap();
            let code = wasmparser::Parser::new(0).parse_all(&bytes).map(|payload| match payload.unwrap() {
                wasmparser::Payload::CodeSectionEntry(body) => body.as_bytes().len(),
                _ => 0,
            }).sum::<usize>();
            let component = Component::new(&engine, &bytes).unwrap();
            let mut store = Store::new(&engine, ());
            store.set_fuel(20_000_000).unwrap();
            let instance = Linker::new(&engine).instantiate(&mut store, &component).unwrap();
            let run = instance.get_typed_func::<(), (f64,)>(&mut store, "run").unwrap();
            assert_eq!(run.call(&mut store, ()).unwrap().0.to_bits(), (count as f64).to_bits());
            run.post_return(&mut store).unwrap();
            sizes.push(code);
        }
        let growth = sizes[1].checked_sub(sizes[0]).expect("adding calls must add code");
        eprintln!("arity {arity}: {} code bytes for eight additional calls", growth);
        assert!(growth <= 8 * maximum, "arity {arity}: {growth} > {}", 8 * maximum);
    }
}
