//! Times the HAMT kernels in hamt.wat on wasmtime 49.0.1 (see README.md).
use std::time::Instant;
use wasmtime::*;

const TEMPLATE: &str = include_str!("hamt.wat");
const COUNT: i32 = 1024;
const KERNELS: &[&str] = &["control", "get_typed", "get_dispatch", "assoc_typed", "assoc_dispatch"];

/// Replaces each `@NAME(a, b, …)` in `text`, splitting arguments at top-level
/// commas, with `expand(NAME, args)`.
fn macros(text: &str, expand: &dyn Fn(&str, &[&str]) -> String) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find('@') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let name_len = after.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(after.len());
        let name = &after[..name_len];
        if !after[name_len..].starts_with('(') {
            out.push_str(&expand(name, &[]));
            rest = &after[name_len..];
            continue;
        }
        let body = &after[name_len + 1..];
        let (mut depth, mut start, mut args) = (0, 0, Vec::new());
        let mut end = None;
        for (i, c) in body.char_indices() {
            match c {
                '(' => depth += 1,
                ')' if depth == 0 => { args.push(body[start..i].trim()); end = Some(i); break; }
                ')' => depth -= 1,
                ',' if depth == 0 => { args.push(body[start..i].trim()); start = i + 1; }
                _ => {}
            }
        }
        let end = end.unwrap_or_else(|| panic!("unclosed @{name}("));
        out.push_str(&expand(name, &args));
        rest = &body[end + 1..];
    }
    out.push_str(rest);
    out
}

fn variant(block: &str, suffix: &str, dispatch: bool) -> String {
    macros(block, &|name, args| match (name, dispatch) {
        ("S", _) => suffix.to_string(),
        ("HASH", false) => format!("(call $hash_direct {})", args[0]),
        ("HASH", true) => format!("(call_ref $hashfn {} (global.get $hash_ref))", args[0]),
        ("EQUIV", false) => format!("(call $equiv_direct {} {})", args[0], args[1]),
        ("EQUIV", true) => format!("(call_ref $equivfn {} {} (global.get $equiv_ref))", args[0], args[1]),
        ("GET", false) => format!("(call $get_typed_impl {} {})", args[0], args[1]),
        ("GET", true) => format!("(call_ref $getfn {} {} (global.get $get_ref))", args[0], args[1]),
        ("ASSOC", false) => format!("(call $assoc_typed_impl {} {} {})", args[0], args[1], args[2]),
        ("ASSOC", true) => format!("(call_ref $assocfn {} {} {} (global.get $assoc_ref))", args[0], args[1], args[2]),
        _ => panic!("unknown macro @{name}"),
    })
}

fn module_text() -> String {
    let (head, rest) = TEMPLATE.split_once("  ;; VARIANT BEGIN\n").unwrap();
    let (block, tail) = rest.split_once("  ;; VARIANT END\n").unwrap();
    format!("{head}{}{}{tail}", variant(block, "typed", false), variant(block, "dispatch", true))
}

fn instance(collector: &str) -> Result<(Store<()>, Instance)> {
    let mut config = Config::new();
    config.wasm_gc(true).wasm_function_references(true).wasm_tail_call(true).wasm_exceptions(true);
    config.collector(match collector {
        "null" => Collector::Null,
        "drc" => Collector::DeferredReferenceCounting,
        "copying" => Collector::Copying,
        _ => Collector::Auto,
    });
    // GC_INITIAL_MIB sets Config::gc_heap_initial_size, as in ../numeric-representation.
    // Suss's default (portable_session::GC_HEAP_INITIAL_SIZE) is 64 MiB; wasmtime's own
    // default is far smaller, and the copying collector then collects almost every assoc.
    let mib = std::env::var("GC_INITIAL_MIB").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(64);
    config.gc_heap_initial_size(mib << 20);
    // HAMT_PERFMAP=1 writes /tmp/perf-PID.map for `perf` or macOS `sample`.
    if std::env::var_os("HAMT_PERFMAP").is_some() {
        config.profiler(ProfilingStrategy::PerfMap);
    }
    let engine = Engine::new(&config)?;
    let module = Module::new(&engine, wat::parse_str(module_text())?)?;
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[])?;
    let found = instance.get_typed_func::<i32, i32>(&mut store, "setup")?.call(&mut store, COUNT)?;
    assert_eq!(found, COUNT, "setup found {found} of {COUNT} keys");
    Ok((store, instance))
}

fn run(store: &mut Store<()>, instance: &Instance, kernel: &str, n: i32) -> Result<f64> {
    let f = instance.get_typed_func::<i32, i32>(&mut *store, kernel)?;
    let start = Instant::now();
    let result = f.call(&mut *store, n)?;
    let seconds = start.elapsed().as_secs_f64();
    assert_eq!(result, n, "{kernel} returned {result}, expected {n}");
    if kernel.starts_with("assoc") {
        let get = instance.get_typed_func::<i32, i32>(&mut *store, "sink_get")?;
        let last = n - 1;
        assert_eq!(get.call(&mut *store, last & (COUNT - 1))?, last, "{kernel}: last assoc lost");
        let other = (last + 1) & (COUNT - 1);
        // Each assoc starts from the original map, so other keys keep their values.
        assert_eq!(get.call(&mut *store, other)?, other, "{kernel}: other key changed");
    }
    Ok(seconds)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("wat") {
        print!("{}", module_text());
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("cwasm") {
        // cwasm FILE: the precompiled module (ELF), for a disassembler.
        let mut config = Config::new();
        config.wasm_gc(true).wasm_function_references(true).wasm_tail_call(true).wasm_exceptions(true);
        let engine = Engine::new(&config)?;
        std::fs::write(&args[1], engine.precompile_module(&wat::parse_str(module_text())?)?)?;
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("one") {
        // one KERNEL N [collector]: a single run, for an external instruction count.
        let (kernel, n) = (&args[1], args[2].parse::<i32>()?);
        let (mut store, instance) = instance(args.get(3).map(String::as_str).unwrap_or("auto"))?;
        run(&mut store, &instance, kernel, n)?;
        return Ok(());
    }
    let collector = args.first().map(String::as_str).unwrap_or("auto");
    let n: i32 = args.get(1).map(|s| s.parse()).transpose()?.unwrap_or(10_000_000);
    let reps: usize = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(5);
    let (mut store, instance) = instance(collector)?;
    println!("wasmtime 49.0.1 collector={collector} gc_initial_mib={} n={n} reps={reps} map={COUNT} keys",
        std::env::var("GC_INITIAL_MIB").unwrap_or_else(|_| "64".into()));
    let mut best = std::collections::HashMap::new();
    for kernel in KERNELS {
        run(&mut store, &instance, kernel, n / 10)?; // warm up
        let seconds = (0..reps).map(|_| run(&mut store, &instance, kernel, n)).collect::<Result<Vec<_>>>()?;
        best.insert(*kernel, seconds.iter().cloned().fold(f64::MAX, f64::min) * 1e9 / n as f64);
    }
    println!("\n| Kernel | ns per iteration | less control |\n| --- | --- | --- |");
    for kernel in KERNELS {
        println!("| `{kernel}` | {:.2} | {:.2} |", best[kernel], best[kernel] - best["control"]);
    }
    Ok(())
}
