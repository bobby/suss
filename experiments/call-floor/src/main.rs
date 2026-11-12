use std::time::Instant;
use wasmtime::*;
fn main() -> Result<()> {
    let n: i32 = std::env::args().nth(1).map(|s| s.parse().unwrap()).unwrap_or(50_000_000);
    let mut config = Config::new();
    config.wasm_gc(true).wasm_function_references(true).wasm_tail_call(true).wasm_exceptions(true);
    config.collector(Collector::Copying);
    let engine = Engine::new(&config)?;
    let module = Module::new(&engine, wat::parse_str(include_str!("k.wat"))?)?;
    let mut store = Store::new(&engine, ());
    let inst = Instance::new(&mut store, &module, &[])?;
    inst.get_typed_func::<(), ()>(&mut store, "setup")?.call(&mut store, ())?;
    let mut base = 0.0;
    for k in ["control", "cast_final", "cast_super", "callref_global", "callref_struct", "call_indirect"] {
        let f = inst.get_typed_func::<i32, i32>(&mut store, k)?;
        f.call(&mut store, n / 10)?;
        let best = (0..5).map(|_| { let t = Instant::now(); let r = f.call(&mut store, n).unwrap(); assert!(r == n || k == "control"); t.elapsed().as_secs_f64() }).fold(f64::MAX, f64::min);
        let ns = best * 1e9 / n as f64;
        if k == "control" { base = ns; }
        println!("{k:16} {ns:6.2} ns  (+{:.2})", ns - base);
    }
    Ok(())
}
