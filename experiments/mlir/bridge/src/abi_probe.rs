//! Original experimental probe based on the repository's runtime_abi test.
//! Unexecuted until the isolated bridge is compiled and run.
use suss_compile::runtime_abi;
use wasmtime::{Engine, Linker, Module, Store};

fn initializer(manifest: &runtime_abi::Manifest) -> Vec<u8> {
    use wasm_encoder::*;
    let mut types = runtime_abi::prelude();
    // Current public ABI prelude contains ten types in one recursive group.
    // TypeSection::len counts groups, so it is not the next type index.
    types.ty().function([], []);
    let mut imports = ImportSection::new();
    imports.import("host", "mark", EntityType::Function(10));
    let mut functions = FunctionSection::new();
    functions.function(10);
    let mut function = Function::new([]);
    function
        .instruction(&Instruction::Call(0))
        .instruction(&Instruction::End);
    let mut code = CodeSection::new();
    code.function(&function);
    let mut module = wasm_encoder::Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&StartSection { function_index: 1 })
        .section(&code)
        .section(&manifest.section());
    module.finish()
}

pub fn verify_before_initializer(engine: &Engine) -> super::Result<()> {
    let expected = runtime_abi::Manifest::default();
    let mut store = Store::new(engine, 0u32);
    store.set_fuel(10_000)?;
    let mut linker = Linker::new(engine);
    linker.func_wrap("host", "mark", |mut caller: wasmtime::Caller<'_, u32>| {
        *caller.data_mut() += 1;
    })?;
    let load = |bytes: &[u8], store: &mut Store<u32>| -> super::Result<()> {
        runtime_abi::verify_artifact(bytes, &expected)?;
        let module = Module::new(engine, bytes)?;
        linker.instantiate(store, &module)?;
        Ok(())
    };
    for changed in [
        runtime_abi::Manifest {
            runtime_abi: 1,
            ..expected.clone()
        },
        runtime_abi::Manifest {
            compiler: "incompatible-experimental-compiler".into(),
            ..expected.clone()
        },
    ] {
        let error =
            load(&initializer(&changed), &mut store).expect_err("incompatible artifact loaded");
        super::require(
            error.to_string().contains("version mismatch"),
            "unexpected ABI rejection reason",
        )?;
        super::require(*store.data() == 0, "initializer ran before ABI rejection")?;
    }
    // Positive control proves the initializer actually has an observable effect.
    load(&initializer(&expected), &mut store)?;
    super::require(
        *store.data() == 1,
        "compatible initializer did not execute exactly once",
    )?;
    Ok(())
}
