//! Calling-convention kernels for ADR-0004 (#359), on the workspace's
//! wasmtime with `portable_session`'s Wasm features.
//!
//! Usage: calls [n] [reps] [kernel,...]   time kernels, best of reps
//!        calls --emit <path>             write calls.wasm for calls.mjs
use std::time::Instant;
use wasmtime::*;

const KERNELS: &[&str] = &[
    "inline", "direct", "indirect", "indirect_typed", "table_ref", "field_typed", "field_untyped", "proposed", "proposed_subtype", "today",
];

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let wasm = wat::parse_str(include_str!("../calls.wat"))?;
    if args.get(1).map(String::as_str) == Some("--emit") {
        std::fs::write(&args[2], &wasm)?;
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("--obj") {
        // Wasmtime's compiled module is an ELF object, for objdump -d.
        let mut config = Config::new();
        config.wasm_gc(true).wasm_function_references(true).wasm_tail_call(true).wasm_exceptions(true);
        std::fs::write(&args[2], Engine::new(&config)?.precompile_module(&wasm)?)?;
        return Ok(());
    }
    let n: i64 = args.get(1).map(|s| s.parse().unwrap()).unwrap_or(100_000_000);
    let reps: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(5);
    let only: Vec<&str> = args.get(3).map(|s| s.split(',').collect()).unwrap_or_default();
    let mut config = Config::new();
    config.wasm_gc(true).wasm_function_references(true).wasm_tail_call(true).wasm_exceptions(true);
    let engine = Engine::new(&config)?;
    let module = Module::new(&engine, &wasm)?;
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[])?;
    instance.get_typed_func::<(), ()>(&mut store, "setup")?.call(&mut store, ())?;
    for &name in KERNELS {
        if !only.is_empty() && !only.contains(&name) { continue; }
        let kernel = instance.get_typed_func::<i64, i64>(&mut store, name)?;
        let mut best = f64::MAX;
        for _ in 0..reps {
            let start = Instant::now();
            assert_eq!(kernel.call(&mut store, n)?, n, "{name}");
            best = best.min(start.elapsed().as_secs_f64());
        }
        println!("{name:16} {:7.3} ns/call", best * 1e9 / n.max(1) as f64);
    }
    Ok(())
}
