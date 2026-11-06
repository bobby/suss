use std::time::Instant;
use wasmtime::*;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let collector = args.get(1).map(String::as_str).unwrap_or("drc");
    let n: i64 = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(50_000_000);
    let reps: usize = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(5);
    let mut config = Config::new();
    config.wasm_gc(true).wasm_function_references(true).wasm_tail_call(true).wasm_exceptions(true);
    config.collector(match collector { "null" => Collector::Null, "drc" => Collector::DeferredReferenceCounting, "copying" => Collector::Copying, _ => Collector::Auto });
    let engine = Engine::new(&config)?;
    let _ = wasm_encoder::Module::new(); // version pin only
    let module = Module::new(&engine, wat::parse_str(include_str!("bench.wat"))?)?;
    let mut store = Store::new(&engine, ());
    let inst = Instance::new(&mut store, &module, &[])?;
    inst.get_typed_func::<(), ()>(&mut store, "setup")?.call(&mut store, ())?;
    println!("wasmtime 49.0.1 collector={collector} n={n} reps={reps}");
    let cases: &[(&str, i64)] = &[
        ("d_i64_plain", 0), ("d_i64_checked", 0),
        ("a_f64box", 0), ("b_i64box", 0),
        ("c_i31", 0), ("c_i31_boxedrange", 1 << 40),
        ("e_call_f64only", 0), ("e_poly_i31", 0), ("e_poly_i64box", 1 << 40), ("e_poly_f64box", 0),
    ];
    for &(name, start) in cases {
        let export = name.strip_suffix("_boxedrange").unwrap_or(name);
        let f = inst.get_typed_func::<(i64, i64), i64>(&mut store, export)?;
        let _ = std::fs::write("/proc/self/clear_refs", "5");
        let mut best = f64::MAX;
        let mut res = 0;
        for _ in 0..reps {
            let t = Instant::now();
            res = f.call(&mut store, (n, start))?;
            best = best.min(t.elapsed().as_secs_f64());
        }
        assert_eq!(res, start + n, "{name}");
        let hwm = std::fs::read_to_string("/proc/self/status").unwrap().lines().find(|l| l.starts_with("VmHWM")).unwrap_or("").split_whitespace().nth(1).unwrap_or("0").parse::<u64>().unwrap_or(0);
        println!("{name:20} {:7.3} ns/iter  (best of {reps}, {:.3}s)  peakRSS {} MiB", best * 1e9 / n as f64, best, hwm / 1024);
    }
    Ok(())
}
