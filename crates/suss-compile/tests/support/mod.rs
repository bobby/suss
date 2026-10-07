//! Test infrastructure shared by semantic suites (not production runtime code).
#![allow(dead_code)]
use std::sync::OnceLock;
use wasmtime::{Config, Engine};



pub fn engine() -> Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut config = Config::new();
            config
                .wasm_gc(true)
                .wasm_function_references(true)
                .wasm_tail_call(true)
                .wasm_exceptions(true)
                .cranelift_opt_level(wasmtime::OptLevel::None);
            Engine::new(&config).expect("test engine")
        })
        .clone()
}
