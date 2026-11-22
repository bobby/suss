use std::{env, fs, path::Path};
use wasmtime::{Collector, Config, Engine, Instance, Module, Store};

fn limbs(text: &str) -> Vec<u32> {
    assert_eq!(text.len() % 8, 0, "hex is full little-endian u32 chunks");
    (0..text.len()).step_by(8).map(|at| u32::from_str_radix(&text[at..at + 8], 16).unwrap()).collect()
}
fn hex(limbs: &[u32]) -> String {
    if limbs.is_empty() { "-".into() } else { limbs.iter().map(|limb| format!("{limb:08x}")).collect() }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args().collect::<Vec<_>>();
    assert!(args.len() >= 4, "MODULE COLLECTOR prepare|verify|run ...");
    let mut config = Config::new();
    config.wasm_gc(true).wasm_function_references(true).wasm_tail_call(true);
    config.collector(match args[2].as_str() {
        "null" => Collector::Null, "drc" => Collector::DeferredReferenceCounting,
        "copying" => Collector::Copying, "auto" => Collector::Auto, _ => panic!("collector"),
    });
    let engine = Engine::new(&config)?;
    let cache = format!("{}.{}.cwasm", args[1], args[2]);
    let module = if args[3] == "prepare" {
        let module = Module::from_file(&engine, &args[1]).map_err(|error| format!("{error:?}\n{error:#}"))?;
        fs::write(&cache, module.serialize()?)?;
        println!("prepared {cache}");
        return Ok(());
    } else {
        // Only our locally serialized, source-hashed artifact is accepted.
        assert!(Path::new(&cache).exists(), "run prepare first");
        unsafe { Module::deserialize_file(&engine, &cache)? }
    };
    let (a, b) = (limbs(&args[4]), limbs(&args[5]));
    assert!(a.len() <= 4096 && b.len() <= 4096);
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[])?;
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    let pages = (0x230000u64 + 65535) / 65536;
    let current = memory.size(&store);
    if pages > current { memory.grow(&mut store, pages - current)?; }
    for (base, values) in [(0x200000, &a), (0x210000, &b)] {
        for (index, value) in values.iter().enumerate() {
            memory.write(&mut store, base + index * 4, &value.to_le_bytes())?;
        }
    }
    instance.get_typed_func::<(i32, i32), ()>(&mut store, "setup")?
        .call(&mut store, (a.len() as i32, b.len() as i32))?;
    let names = if args[3] == "verify" {
        vec!["gc_add", "bridge_add", "linear_add", "gc_mul", "bridge_mul", "linear_mul"]
    } else { vec![args[6].as_str()] };
    let count = if args[3] == "verify" { 1 } else { args[7].parse::<i32>()? };
    for name in names {
        let checksum = instance.get_typed_func::<i32, i64>(&mut store, name)?.call(&mut store, count)? as u64;
        let used = instance.get_global(&mut store, "used").unwrap().get(&mut store).unwrap_i32() as usize;
        if args[3] == "verify" {
            let cap = if name.ends_with("add") { a.len().max(b.len()) + 1 } else { a.len() + b.len() };
            let result = if name.starts_with("linear_") {
                let bytes = &memory.data(&store)[0x220000..0x220000 + cap * 4];
                bytes.chunks_exact(4).map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap())).collect::<Vec<_>>()
            } else {
                let value = instance.get_global(&mut store, "sink").unwrap().get(&mut store);
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                array.elems(&mut store)?.map(|value| value.unwrap_i32() as u32).collect::<Vec<_>>()
            };
            println!("{name} {used} {} {checksum}", hex(&result));
        } else {
            println!("{checksum}");
        }
    }
    if env::var_os("SUSS_BIGINT_RSS").is_some() {
        let status = fs::read_to_string("/proc/self/status")?;
        let kib = status.lines().find(|line| line.starts_with("VmHWM:"))
            .unwrap().split_whitespace().nth(1).unwrap().parse::<u64>()?;
        eprintln!("peak-bytes={}", kib * 1024);
    }
    Ok(())
}
