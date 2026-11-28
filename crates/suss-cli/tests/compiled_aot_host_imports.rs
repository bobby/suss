//! Compile configured host authority through the actual file/namespace/project CLI.
#![cfg(not(target_family = "wasm"))]
use std::{path::Path, process::Command, sync::OnceLock};
use wasmtime::{
    Config, Engine, Store, StoreContextMut,
    component::{Component, Linker},
};

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut config = Config::new();
        config
            .wasm_gc(true)
            .wasm_function_references(true)
            .wasm_tail_call(true)
            .wasm_exceptions(true)
            .wasm_component_model(true)
            .consume_fuel(true)
            .cranelift_opt_level(wasmtime::OptLevel::None);
        Engine::new(&config).unwrap()
    })
}
fn execute(path: &Path, expected: i64) {
    let component = Component::from_file(engine(), path).unwrap();
    assert_eq!(component.component_type().imports(engine()).count(), 2);
    let mut linker = Linker::new(engine());
    linker
        .root()
        .func_wrap("left", |_: StoreContextMut<'_, ()>, (x,): (i64,)| {
            Ok((x + 1,))
        })
        .unwrap();
    linker
        .root()
        .func_wrap("right", |_: StoreContextMut<'_, ()>, (x,): (i64,)| {
            Ok((x + 100,))
        })
        .unwrap();
    let mut store = Store::new(engine(), ());
    store.set_fuel(100_000_000).unwrap();
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let run = instance
        .get_typed_func::<(i64,), (i64,)>(&mut store, "run")
        .unwrap();
    assert_eq!(run.call(&mut store, (41,)).unwrap().0, expected);
}

#[test]
fn cli_import_flags_and_project_configuration_select_providers_and_invalidate_cached_authority() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    std::fs::write(
        root.path().join("src/app.sus"),
        "(ns app) (defn run [x] (host/step x))",
    )
    .unwrap();
    std::fs::write(root.path().join("api.wit"), "package test:cli-imports; world api { import left: func(x: s64) -> s64; import right: func(x: s64) -> s64; export run: func(x: s64) -> s64; }").unwrap();
    for mode in ["file", "namespace", "project"] {
        for (left, right, expected) in [
            ("host/step", "host/spare", 42),
            ("host/spare", "host/step", 141),
        ] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_suss"));
            command
                .current_dir(root.path())
                .env("SUSS_BUILD_DIR", root.path().join("cache"))
                .arg("compile");
            let output = format!("{mode}.wasm");
            if mode == "project" {
                let config = format!(
                    "{{:src-paths [\"src\"] :worlds {{:app {{:namespace app :wit \"api.wit\" :wit-world \"api\" :imports {{\"left\" {left} \"right\" {right}}} :exports {{\"run\" app/run}} :output \"{output}\"}}}}}}"
                );
                std::fs::write(root.path().join("deps.sus"), config).unwrap();
                command.args(["--config", "deps.sus"]);
            } else {
                if mode == "file" {
                    command.arg("src/app.sus");
                } else {
                    command.args(["-n", "app"]);
                }
                command.args([
                    "--src",
                    "src",
                    "-w",
                    "api.wit",
                    "--wit-world",
                    "api",
                    "--export",
                    "run=app/run",
                    "--import",
                    &format!("left={left}"),
                    "--import",
                    &format!("right={right}"),
                    "-o",
                    &output,
                ]);
            }
            let result = command.output().unwrap();
            assert!(
                result.status.success(),
                "{mode}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            execute(&root.path().join(output), expected);
        }
    }
}
