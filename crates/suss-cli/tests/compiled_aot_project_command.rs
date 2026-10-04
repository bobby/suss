//! Actual project commands and typed components through compiled source phases.
use std::{path::Path, process::Command, sync::OnceLock};
use wasmtime::component::{Component, Linker};
use wasmtime::{AsContextMut, Config, Engine, Store};

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
fn command(root: &Path, flags: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root)
        .args(["compile", "--config", "project/deps.sus"])
        .args(flags)
        .output()
        .unwrap()
}
fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("project/src/app")).unwrap();
    root
}
fn write(root: &Path, path: &str, source: &str) {
    std::fs::write(root.join("project").join(path), source).unwrap();
}
fn success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn configured_entry_world_and_mappings_execute_compiled_phases_without_partial_output() {
    let root = fixture();
    write(
        root.path(),
        "src/dep.cljs",
        "(ns dep) (def counter 0) (set! counter (+ counter 1))",
    );
    write(
        root.path(),
        "src/tools.cljs",
        "(ns tools (:require [dep :as d])) (defmacro twice [x] `(+ ~x ~x ~(- d/counter 1)))",
    );
    write(
        root.path(),
        "src/app/core.cljc",
        "#?(:cljs (ns app.core (:require [dep :as d]) (:require-macros [tools :refer [twice]])) :suss (ns wrong)) (def seen 0) (def ^{:export 7} calculate (fn [x] (do (set! seen (+ seen 1)) (twice (+ x 2))))) (def ^:export effects (fn [] (+ seen (* 100 (- d/counter 1))))) (set! seen (+ seen 1))",
    );
    write(
        root.path(),
        "api.wit",
        "package test:project@1.2.3; interface operations { calculate: func(x: u32) -> u32; } world unused {} world chosen { export api: operations; export effects: func() -> f64; }",
    );
    write(
        root.path(),
        "deps.sus",
        "{:src-paths [\"src\" \"./src\"] :worlds {:a-good {:namespace app.core :wit \"api.wit\" :wit-world \"chosen\" :exports {\"api#calculate\" app.core/calculate} :output \"app.wasm\"} :z-bad {:namespace absent.core :wit \"absent.wit\" :output \"bad.wasm\"}}}",
    );
    success(&command(root.path(), &["--world", ":a-good"]));
    let engine = engine();
    let component = Component::from_file(&engine, root.path().join("project/app.wasm")).unwrap();
    let api = component.get_export_index(None, "api").unwrap();
    let calculate = component.get_export_index(Some(&api), "calculate").unwrap();
    for _ in 0..2 {
        let mut store = Store::new(&engine, ());
        store.set_fuel(40_000_000).unwrap();
        let instance = Linker::<()>::new(&engine)
            .instantiate(&mut store, &component)
            .unwrap();
        let calculate = instance
            .get_typed_func::<(u32,), (u32,)>(&mut store, &calculate)
            .unwrap();
        let effects = instance
            .get_typed_func::<(), (f64,)>(&mut store, "effects")
            .unwrap();
        assert_eq!(effects.call(&mut store, ()).unwrap().0, 1.0);
        assert_eq!(calculate.call(&mut store, (19,)).unwrap().0, 42);
        store.gc(None).unwrap();
        assert_eq!(calculate.call(&mut store, (1,)).unwrap().0, 6);
        assert_eq!(effects.call(&mut store, ()).unwrap().0, 3.0);
    }
    write(root.path(), "app.wasm", "prior good");
    write(root.path(), "bad.wasm", "prior bad");
    let output = command(root.path(), &[]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("absent.core"));
    assert_eq!(
        std::fs::read(root.path().join("project/app.wasm")).unwrap(),
        b"prior good"
    );
    assert_eq!(
        std::fs::read(root.path().join("project/bad.wasm")).unwrap(),
        b"prior bad"
    );
}

#[test]
fn retained_world_groups_prepare_shared_roots_once_and_preserve_all_source_effects() {
    let root = fixture();
    #[cfg(unix)]
    std::os::unix::fs::symlink(".", root.path().join("project/src/again")).unwrap();
    write(
        root.path(),
        "src/a.cljs",
        "(ns a (gen-world :bundle) (:require [b :as b])) (def ^:export calculate (fn [x] (+ x b/counter))) (def ^:export effects (fn [] b/counter))",
    );
    write(
        root.path(),
        "src/b.cljc",
        "#?(:cljs (ns b (gen-world :bundle))) (def counter 0) (set! counter (+ counter 1))",
    );
    write(
        root.path(),
        "src/c.sus",
        "(ns c (gen-world :bundle) (:require [b :as b])) (set! b/counter (+ b/counter 10))",
    );
    write(
        root.path(),
        "api.wit",
        "package test:group; world api { export calculate: func(x: u32) -> u32; export effects: func() -> f64; }",
    );
    write(
        root.path(),
        "deps.sus",
        "{:src-paths [\"src\" \"./src\"] :worlds {:bundle {:wit \"api.wit\" :output \"app.wasm\"}}}",
    );
    success(&command(root.path(), &[]));
    let engine = engine();
    let component = Component::from_file(&engine, root.path().join("project/app.wasm")).unwrap();
    for _ in 0..2 {
        let mut store = Store::new(&engine, ());
        store.set_fuel(40_000_000).unwrap();
        let instance = Linker::<()>::new(&engine)
            .instantiate(&mut store, &component)
            .unwrap();
        let calculate = instance
            .get_typed_func::<(u32,), (u32,)>(&mut store, "calculate")
            .unwrap();
        let effects = instance
            .get_typed_func::<(), (f64,)>(&mut store, "effects")
            .unwrap();
        assert_eq!(effects.call(&mut store, ()).unwrap().0, 11.0);
        assert_eq!(calculate.call(&mut store, (31,)).unwrap().0, 42);
        store.gc(None).unwrap();
        assert_eq!(calculate.call(&mut store, (1,)).unwrap().0, 12);
        assert_eq!(effects.call(&mut store, ()).unwrap().0, 11.0);
    }
}

#[test]
fn project_selection_and_headers_reject_before_macro_effects_and_preserve_output() {
    let root = fixture();
    write(root.path(), "api.wit", "package test:errors; world api {}");
    let ordinary = "{:src-paths [\"src\"] :worlds {:bundle {:namespace app.core :wit \"api.wit\" :output \"app.wasm\"}}}";
    for (source, config, expected) in [
        (
            "(ns app.core (gen-world :other)) (defmacro bomb [] (throw 17)) (bomb)",
            ordinary,
            "targets :other",
        ),
        (
            "(ns app.core (gen-world :bundle :extra)) (defmacro bomb [] (throw 17)) (bomb)",
            ordinary,
            "exactly one world keyword",
        ),
        (
            "(ns app.core (gen-world :bundle) (gen-world :bundle)) (defmacro bomb [] (throw 17)) (bomb)",
            ordinary,
            "Duplicate namespace clause",
        ),
        (
            "(ns wrong) (defmacro bomb [] (throw 17)) (bomb)",
            ordinary,
            "does not match requested",
        ),
        (
            "(defmacro bomb [] (throw 17)) (bomb)",
            ordinary,
            "leading ns",
        ),
        (
            "(ns app.core)",
            "{:src-paths [\"src\"] :deps {:lib \"1\"} :worlds {:bundle {:namespace app.core :wit \"api.wit\" :output \"app.wasm\"}}}",
            ":deps cannot be ignored",
        ),
        (
            "(ns app.core)",
            "{:src-paths [\"src\"] :worlds {:bundle {:namespace app.core :wit \"api.wit\" :exports {\"f\" unqualified} :output \"app.wasm\"}}}",
            "qualified source symbols",
        ),
    ] {
        write(root.path(), "src/app/core.sus", source);
        write(root.path(), "deps.sus", config);
        write(root.path(), "app.wasm", "prior artifact");
        let output = command(root.path(), &[]);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            std::fs::read(root.path().join("project/app.wasm")).unwrap(),
            b"prior artifact"
        );
    }
    write(root.path(), "deps.sus", ordinary);
    write(root.path(), "src/app/core.sus", "(ns app.core)");
    write(root.path(), "src/app/core.cljs", "(ns app.core)");
    let output = command(root.path(), &[]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Ambiguous"));
    let output = command(root.path(), &["--world", ":missing"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("World ':missing' not found"));
    for flags in [
        ["--src", "other"],
        ["--wit", "absent.wit"],
        ["--output", "ignored.wasm"],
    ] {
        let output = command(root.path(), &flags);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("Project mode takes WIT, source paths and outputs from configuration")
        );
    }
    assert_eq!(
        std::fs::read(root.path().join("project/app.wasm")).unwrap(),
        b"prior artifact"
    );
}

#[test]
fn project_compilation_defers_original_runtime_exception_to_each_fresh_store() {
    let root = fixture();
    write(
        root.path(),
        "src/app/core.sus",
        "(ns app.core (gen-world :bundle)) (throw 17) (throw 99)",
    );
    write(
        root.path(),
        "api.wit",
        "package test:deferred; world api {}",
    );
    write(
        root.path(),
        "deps.sus",
        "{:worlds {:bundle {:wit \"api.wit\" :output \"app.wasm\"}}}",
    );
    success(&command(root.path(), &[]));
    let engine = engine();
    let component = Component::from_file(&engine, root.path().join("project/app.wasm")).unwrap();
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
        assert_eq!(payload.field(&mut store, 0).unwrap().unwrap_f64(), 17.0);
    }
}

#[test]
fn explicitly_selected_core_source_is_not_skipped_as_bootstrap_initialization() {
    let root = fixture();
    std::fs::create_dir_all(root.path().join("project/src/suss")).unwrap();
    write(
        root.path(),
        "src/suss/core.sus",
        "(ns suss.core) (def ^:export calculate (fn [x] (+ x 23)))",
    );
    write(
        root.path(),
        "api.wit",
        "package test:core-entry; world api { export calculate: func(x: u32) -> u32; }",
    );
    write(
        root.path(),
        "deps.sus",
        "{:worlds {:bundle {:namespace suss.core :wit \"api.wit\" :output \"app.wasm\"}}}",
    );
    success(&command(root.path(), &[]));
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path().join("project"))
        .args([
            "compile",
            "--namespace",
            "suss.core",
            "-w",
            "api.wit",
            "-o",
            "namespace.wasm",
        ])
        .output()
        .unwrap();
    success(&output);
    let engine = engine();
    for path in ["app.wasm", "namespace.wasm"] {
        let component =
            Component::from_file(&engine, root.path().join("project").join(path)).unwrap();
        for _ in 0..2 {
            let mut store = Store::new(&engine, ());
            store.set_fuel(40_000_000).unwrap();
            let instance = Linker::<()>::new(&engine)
                .instantiate(&mut store, &component)
                .unwrap();
            let calculate = instance
                .get_typed_func::<(u32,), (u32,)>(&mut store, "calculate")
                .unwrap();
            assert_eq!(calculate.call(&mut store, (19,)).unwrap().0, 42);
            store.gc(None).unwrap();
            assert_eq!(calculate.call(&mut store, (1,)).unwrap().0, 24);
        }
    }
}
