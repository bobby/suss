//! Measurement harness only; historical compiler sources and lockfile are unchanged.
use sha2::{Digest, Sha256};
use suss_compile::portable::{self, resolve::Phase};
use suss_reader::Symbol;
use wasmtime::{Config, Engine, Store};
use wasmtime::component::{Component, Linker};
fn code_bytes(bytes: &[u8]) -> usize {
    wasmparser::Parser::new(0).parse_all(bytes).map(|payload| match payload.unwrap() {
        wasmparser::Payload::CodeSectionEntry(body) => body.as_bytes().len(),
        _ => 0,
    }).sum()
}
fn main() {
    let output = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    std::fs::create_dir_all(&output).unwrap();
    let mut config = Config::new();
    config.wasm_gc(true).wasm_function_references(true).wasm_tail_call(true)
        .wasm_exceptions(true).wasm_component_model(true)
        .wasm_component_model_implements(true).consume_fuel(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let mut cache_config = wasmtime::CacheConfig::new();
    cache_config.with_directory(output.parent().unwrap().join("m11-call-native-cache"));
    config.cache(Some(wasmtime::Cache::new(cache_config).unwrap()));
    let engine = Engine::new(&config).unwrap();
    let core = portable::bootstrap::shipped(Phase::Runtime).unwrap().clone();
    let wit = "package test:call-size; world api { export run: func() -> f64; }";
    std::fs::write(output.join("api.wit"), wit).unwrap();
    let mut resolve = wit_parser::Resolve::new();
    let package = resolve.push_str("api.wit", wit).unwrap();
    let world = *resolve.packages[package].worlds.values().next().unwrap();
    let mut rows = Vec::new();
    for arity in [0,4] {
        for count in [0,1,2,10] {
            let (params, operands, increment) = if arity == 0 { ("", "", "1") }
                else { ("a b c d", " 1 2 3 4", "a") };
            let calls = format!("(f{operands}) ").repeat(count);
            let source = format!("(ns calls) (def effects 0) (def f (fn [{params}] (set! effects (+ effects {increment})))) (def run (fn [] (do {calls}effects)))");
            let fragment = portable::prepare_fragment(&source, &core.environment, Phase::Runtime).unwrap();
            let bytes = portable::aot::component(&[core.clone(), fragment], &resolve, world,
                &[("run".to_owned(), Symbol::namespaced("calls", "run"))]).unwrap();
            let component = Component::new(&engine, &bytes).unwrap();
            let mut store = Store::new(&engine, ());
            store.set_fuel(20_000_000).unwrap();
            let instance = Linker::new(&engine).instantiate(&mut store, &component).unwrap();
            let run = instance.get_typed_func::<(), (f64,)>(&mut store, "run").unwrap();
            let actual = run.call(&mut store, ()).unwrap().0;
            run.post_return(&mut store).unwrap();
            assert_eq!(actual.to_bits(), (count as f64).to_bits(), "actual source calls must increment effects");
            let name = format!("arity-{arity}-calls-{count}");
            std::fs::write(output.join(format!("{name}.sus")), &source).unwrap();
            std::fs::write(output.join(format!("{name}.wasm")), &bytes).unwrap();
            rows.push(serde_json::json!({"arity":arity,"calls":count,"bytes":bytes.len(),
                "code_bytes":code_bytes(&bytes),"source":source,"result":actual,
                "source_sha256":format!("{:x}",Sha256::digest(source.as_bytes())),
                "component_sha256":format!("{:x}",Sha256::digest(&bytes))}));
        }
    }
    let report = serde_json::to_string_pretty(&rows).unwrap();
    std::fs::write(output.join("report.json"), format!("{report}\n")).unwrap();
    println!("{report}");
}
