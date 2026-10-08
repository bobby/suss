//! Isolated comparison at the existing public Rust source/IR emission boundary.
use super::*;
use std::path::Path;

fn read(path: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(1_048_577)
        .read_to_end(&mut bytes)?;
    require(bytes.len() <= 1_048_576, "comparison input exceeds 1 MiB")?;
    Ok(bytes)
}
fn argument(args: &mut impl Iterator<Item = String>) -> Result<String> {
    args.next()
        .ok_or_else(|| "missing comparison argument".into())
}
fn save(directory: &str, producer: &[u8], caller: &[u8]) -> Result<()> {
    for bytes in [producer, caller] {
        portable::artifact_identity::verify(bytes, Default::default())?;
        runtime_abi::verify_artifact(bytes, &runtime_abi::Manifest::default())?;
    }
    std::fs::create_dir_all(directory)?;
    std::fs::write(Path::new(directory).join("producer.wasm"), producer)?;
    std::fs::write(Path::new(directory).join("caller.wasm"), caller)?;
    Ok(())
}
/// Compilation only. The independent runner validates and executes both outputs.
pub fn command(path: &str, args: &mut impl Iterator<Item = String>) -> Result<bool> {
    if path == "--emit-native" {
        let producer = String::from_utf8(read(&argument(args)?)?)?;
        let caller = String::from_utf8(read(&argument(args)?)?)?;
        let directory = argument(args)?;
        require(args.next().is_none(), "unexpected native emission argument")?;
        let mut env = Environment::default();
        env.declare_cell(Phase::Runtime, "bridge", "producer")?;
        let producer = portable::compile_in(&producer, &env, Phase::Runtime)?;
        let caller = portable::compile_in(&caller, &env, Phase::Runtime)?;
        save(&directory, &producer, &caller)?;
        return Ok(true);
    }
    if path == "--emit-graph" {
        let graph: Envelope = serde_json::from_slice(&read(&argument(args)?)?)?;
        let directory = argument(args)?;
        require(args.next().is_none(), "unexpected graph emission argument")?;
        require(
            graph.schema == "suss.mlir.bridge.v1"
                && graph.verified_mlir
                && graph.source_analysis.is_none(),
            "comparison requires numeric v1 exported graph",
        )?;
        require(
            graph.producer.parameters.is_empty(),
            "producer entry must be closed",
        )?;
        let mut env = Environment::default();
        let id = env.declare_cell(Phase::Runtime, "bridge", "producer")?;
        let mut budget = 8192;
        let (producer, ty) = reconstruct(&graph.producer, None, 0, &mut budget)?;
        require(
            graph.caller.parameters == vec![ty],
            "caller binding type differs from producer result",
        )?;
        let (caller, result) = reconstruct(&graph.caller, Some(&id), 0, &mut budget)?;
        require(result == (Ty::F64 {}), "caller must return f64")?;
        save(
            &directory,
            &portable::compile_ir(&producer)?,
            &portable::compile_ir(&caller)?,
        )?;
        return Ok(true);
    }
    if path == "--run-emitted" {
        let directory = argument(args)?;
        let expected = bits(&argument(args)?)?;
        require(
            args.next().is_none(),
            "unexpected emitted execution argument",
        )?;
        let mut config = Config::new();
        config
            .wasm_gc(true)
            .wasm_function_references(true)
            .wasm_tail_call(true)
            .wasm_exceptions(true)
            .consume_fuel(true)
            .cranelift_opt_level(wasmtime::OptLevel::None);
        let engine = Engine::new(&config)?;
        abi_probe::verify_before_initializer(&engine)?;
        let mut store = Store::new(&engine, ());
        store.set_fuel(1_000_000)?;
        let rt = Instance::new(
            &mut store,
            &Module::new(&engine, runtime_abi::module())?,
            &[],
        )?;
        let mut linker = Linker::new(&engine);
        linker.instance(&mut store, "suss.runtime", rt)?;
        let mut modules = Vec::new();
        for name in ["producer.wasm", "caller.wasm"] {
            let bytes = read(
                Path::new(&directory)
                    .join(name)
                    .to_str()
                    .ok_or("non-UTF8 path")?,
            )?;
            portable::artifact_identity::verify(&bytes, Default::default())?;
            runtime_abi::verify_artifact(&bytes, &runtime_abi::Manifest::default())?;
            modules.push(Module::new(&engine, bytes)?);
        }
        let first = linker.instantiate(&mut store, &modules[0])?;
        let closure = call(&mut store, first, "eval", &[])?;
        let cell = call(&mut store, rt, "binding-new", &[closure])?;
        let object = cell
            .unwrap_anyref()
            .ok_or("null cell")?
            .as_struct(&store)?
            .ok_or("cell is not struct")?;
        let ty = object.ty(&store)?;
        let global = Global::new(
            &mut store,
            GlobalType::new(
                ValType::Ref(RefType::new(false, ty.into())),
                Mutability::Const,
            ),
            cell,
        )?;
        linker.define(&store, "suss.bindings.runtime", "bridge/producer", global)?;
        store.gc(None)?;
        let second = linker.instantiate(&mut store, &modules[1])?;
        let value = call(&mut store, second, "eval", &[])?;
        store.gc(None)?;
        let number = value
            .unwrap_anyref()
            .ok_or("null result")?
            .as_struct(&store)?
            .ok_or("result is not number")?;
        let fields = number.fields(&mut store)?.collect::<Vec<_>>();
        let [Val::F64(actual)] = fields.as_slice() else {
            return Err("independent number decoding failed".into());
        };
        require(*actual == expected, "emitted actual bits mismatch")?;
        println!("actual_bits={actual:016x}");
        println!("abi_initializer_gate=true; shared_gc_after_collection=true");
        return Ok(true);
    }
    Ok(false)
}
