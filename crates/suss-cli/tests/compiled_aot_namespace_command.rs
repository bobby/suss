//! Execute components compiled from namespace entrypoints through the native CLI.
use std::{
    path::Path,
    process::{Command, Output},
    sync::OnceLock,
};
use wasmtime::component::{Component, Linker};
use wasmtime::{AsContextMut, Config, Engine, Store};

fn compile(root: &Path, namespace: &str, flags: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root)
        .args([
            "compile",
            "-n",
            namespace,
            "-w",
            "api.wit",
            "--wit-world",
            "api-world",
            "--export",
            "api#calculate=app.core/calculate",
            "--export",
            "api#effects=app.core/effects",
            "-o",
            "app.wasm",
        ])
        .args(flags)
        .output()
        .unwrap()
}
fn engine() -> Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut config = Config::new();
            config
                .wasm_gc(true)
                .wasm_function_references(true)
                .wasm_tail_call(true)
                .wasm_exceptions(true)
                .wasm_component_model(true)
                .wasm_component_model_implements(true)
                .consume_fuel(true)
                .cranelift_opt_level(wasmtime::OptLevel::None);
            Engine::new(&config).unwrap()
        })
        .clone()
}
fn wit(root: &Path) {
    std::fs::write(root.join("api.wit"), "package test:namespace; interface operations { calculate: func(x: u32) -> u32; effects: func() -> f64; } world api-world { export api: operations; } world other {}").unwrap();
}

#[test]
fn namespace_command_uses_default_root_compiled_macros_and_runtime_dependencies() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("src");
    std::fs::create_dir_all(source.join("app")).unwrap();
    std::fs::write(
        source.join("dep.cljs"),
        "(ns dep) (def bias 2) (def counter 0) (set! counter (+ counter 1))",
    )
    .unwrap();
    std::fs::write(
        source.join("tools.cljs"),
        "(ns tools (:require [dep :as d])) (defmacro twice [x] `(+ ~x ~x ~(- d/counter 1)))",
    )
    .unwrap();
    std::fs::write(source.join("app/core.cljc"), "#?(:cljs (ns app.core (:require [dep :as d]) (:require-macros [tools :as t])) :clj (ns wrong)) (def seen 0) (def calculate (fn [x] (do (set! seen (+ seen 1)) (t/twice (+ x d/bias))))) (def effects (fn [] (+ seen (* 100 (- d/counter 1))))) (set! seen (+ seen 1))").unwrap();
    wit(root.path());
    let output = compile(root.path(), "app.core", &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let engine = engine();
    let component = Component::from_file(&engine, root.path().join("app.wasm")).unwrap();
    let interface = component.get_export_index(None, "api").unwrap();
    let calculate_index = component
        .get_export_index(Some(&interface), "calculate")
        .unwrap();
    let effects_index = component
        .get_export_index(Some(&interface), "effects")
        .unwrap();
    for _ in 0..2 {
        let mut store = Store::new(&engine, ());
        store.set_fuel(40_000_000).unwrap();
        let instance = Linker::<()>::new(&engine)
            .instantiate(&mut store, &component)
            .unwrap();
        let calculate = instance
            .get_typed_func::<(u32,), (u32,)>(&mut store, &calculate_index)
            .unwrap();
        let effects = instance
            .get_typed_func::<(), (f64,)>(&mut store, &effects_index)
            .unwrap();
        assert_eq!(effects.call(&mut store, ()).unwrap().0, 1.0);
        assert_eq!(calculate.call(&mut store, (19,)).unwrap().0, 42);
        store.gc(None).unwrap();
        assert_eq!(calculate.call(&mut store, (1,)).unwrap().0, 6);
        assert_eq!(effects.call(&mut store, ()).unwrap().0, 3.0);
    }
}

#[test]
fn namespace_command_rejects_wrong_missing_or_ambiguous_source_without_replacing_output() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src/app")).unwrap();
    wit(root.path());
    let output_path = root.path().join("app.wasm");
    std::fs::write(&output_path, b"prior artifact").unwrap();
    for (text, expected) in [
        (
            "(ns wrong) (defmacro bomb [] (throw 17)) (bomb)",
            "Declared namespace wrong does not match requested app.core",
        ),
        (
            "(def calculate (fn [x] x))",
            "Source module requires a leading ns declaration",
        ),
    ] {
        std::fs::write(root.path().join("src/app/core.sus"), text).unwrap();
        let output = compile(root.path(), "app.core", &[]);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(std::fs::read(&output_path).unwrap(), b"prior artifact");
    }
    for flags in [&["unused.sus"][..], &["--main", "app.core"][..]] {
        let output = compile(root.path(), "app.core", flags);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("--namespace cannot be combined with a source file or --main")
        );
        assert_eq!(std::fs::read(&output_path).unwrap(), b"prior artifact");
    }
    std::fs::write(root.path().join("src/app/core.sus"), "(ns app.core)").unwrap();
    std::fs::write(root.path().join("src/app/core.cljs"), "(ns app.core)").unwrap();
    let output = compile(root.path(), "app.core", &[]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Ambiguous source for namespace app.core")
    );
    let output = compile(root.path(), "missing", &[]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("No source for namespace missing"));
    assert_eq!(std::fs::read(&output_path).unwrap(), b"prior artifact");
}

#[test]
fn namespace_command_defers_runtime_throw_until_component_instantiation() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("sources/app")).unwrap();
    std::fs::write(
        root.path().join("sources/app/core.sus"),
        "(ns app.core) (def calculate (fn [x] x)) (def effects (fn [] 1)) (throw 17) (throw 99)",
    )
    .unwrap();
    wit(root.path());
    let output = compile(
        root.path(),
        "app.core",
        &["--src", "sources", "--src", "./sources"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let engine = engine();
    let component = Component::from_file(&engine, root.path().join("app.wasm")).unwrap();
    for _ in 0..2 {
        let mut store = Store::new(&engine, ());
        store.set_fuel(40_000_000).unwrap();
        assert!(
            Linker::<()>::new(&engine)
                .instantiate(&mut store, &component)
                .is_err()
        );
        let exception = store.as_context_mut().take_pending_exception().unwrap();
        let fields = exception.fields(&mut store).unwrap().collect::<Vec<_>>();
        assert_eq!(fields.len(), 1);
        let payload = fields[0]
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
        assert_eq!(
            payload.field(&mut store, 0).unwrap().unwrap_f64().to_bits(),
            17.0_f64.to_bits()
        );
    }
}

#[test]
fn namespace_command_rejects_missing_wit_and_project_options_before_routing() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src/app")).unwrap();
    std::fs::write(
        root.path().join("src/app/core.sus"),
        "(ns app.core) (def calculate (fn [x] x)) (def effects (fn [] 1))",
    )
    .unwrap();
    wit(root.path());
    let artifact = root.path().join("app.wasm");
    std::fs::write(&artifact, b"prior artifact").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args(["compile", "--namespace", "app.core", "-o", "app.wasm"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--namespace requires -w/--wit"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read(&artifact).unwrap(), b"prior artifact");
    for flags in [
        &["--world", "ignored"][..],
        &["--config", "ignored.sus"][..],
    ] {
        let output = compile(root.path(), "app.core", flags);
        assert!(
            !output.status.success(),
            "Project options must not be silently ignored"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("--namespace cannot be combined with project --world or --config"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(std::fs::read(&artifact).unwrap(), b"prior artifact");
    }
}
