//! Execute retained export facts and selected-WIT source mapping through compiled phases.
use std::{path::Path, process::Command, sync::OnceLock};
use suss_cli::{
    portable_macro_data::FormBridge, portable_macros::CompiledMacros, portable_session::Session,
};
use suss_reader::forms::Kind;
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};

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
fn compile(root: &Path, flags: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root)
        .args(["compile", "app.sus", "-w", "api.wit", "-o", "app.wasm"])
        .args(flags)
        .output()
        .unwrap()
}
#[test]
fn export_metadata_maps_freestanding_functions_and_preserves_live_effects() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("app.sus"), "(ns app) (def seen 0) (def ^:export calculate (fn [x] (do (set! seen (+ seen 1)) (* x 2)))) (def ^{:export \"effects\"} observe (fn [] seen)) (def ^{:export false} ignored (fn [] 99)) (set! seen (+ seen 1))").unwrap();
    std::fs::write(root.path().join("api.wit"), "package test:metadata@1.2.3; world api-world { export calculate: func(x: u32) -> u32; export effects: func() -> f64; }").unwrap();
    let output = compile(root.path(), &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args([
            "compile",
            "-n",
            "app",
            "--src",
            ".",
            "-w",
            "api.wit",
            "-o",
            "namespace.wasm",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let engine = engine();
    for path in ["app.wasm", "namespace.wasm"] {
        let component = Component::from_file(&engine, root.path().join(path)).unwrap();
        let calculate = component.get_export_index(None, "calculate").unwrap();
        let effects = component.get_export_index(None, "effects").unwrap();
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
                .get_typed_func::<(), (f64,)>(&mut store, &effects)
                .unwrap();
            assert_eq!(effects.call(&mut store, ()).unwrap().0, 1.0);
            assert_eq!(calculate.call(&mut store, (21,)).unwrap().0, 42);
            store.gc(None).unwrap();
            assert_eq!(calculate.call(&mut store, (3,)).unwrap().0, 6);
            assert_eq!(effects.call(&mut store, ()).unwrap().0, 3.0);
        }
    }
}

#[test]
fn source_export_metadata_matches_macro_snapshots_in_both_phases() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        session.enter_namespace("export-probe").unwrap();
        macros.enter_namespace("export-probe").unwrap();
        macros.define("(defmacro inspect-export [name] (let [d (get (get (get &env :ns) :defs) name)] (list 'quote [(get d :export) (get (get d :meta) :export)])))").unwrap();
        let value = session
            .eval_with_macros(
                "(def ^:export staged (inspect-export staged)) staged",
                &mut macros,
            )
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let Kind::Vector(items) = bridge.read(&mut session, &value, 0..1).unwrap().kind else {
            panic!("compiled provisional facts")
        };
        assert!(matches!(items[0].kind, Kind::Nil));
        assert!(matches!(items[1].kind, Kind::Nil));
        let value = session
            .eval_with_macros("(inspect-export staged)", &mut macros)
            .unwrap();
        session.collect().unwrap();
        let Kind::Vector(items) = bridge.read(&mut session, &value, 0..1).unwrap().kind else {
            panic!("compiled completed facts")
        };
        assert!(matches!(items[0].kind, Kind::Bool(true)));
        assert!(matches!(items[1].kind, Kind::Bool(true)));
        let value = session
            .eval_with_macros(
                "(def ^{:export false} ^:export disabled 1) (inspect-export disabled)",
                &mut macros,
            )
            .unwrap();
        session.collect().unwrap();
        let Kind::Vector(items) = bridge.read(&mut session, &value, 0..1).unwrap().kind else {
            panic!("metadata precedence")
        };
        assert!(matches!(items[0].kind, Kind::Bool(false)));
        assert!(matches!(items[1].kind, Kind::Bool(false)));
    }
}

#[test]
fn interface_metadata_requires_explicit_mapping_and_preserves_prior_output() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("app.sus"),
        "(ns app) (def ^:export calculate (fn [x] x))",
    )
    .unwrap();
    std::fs::write(root.path().join("api.wit"), "package test:ambiguous; interface operations { calculate: func(x: u32) -> u32; } world api-world { export left: operations; export right: operations; }").unwrap();
    std::fs::write(root.path().join("app.wasm"), b"prior artifact").unwrap();
    let output = compile(root.path(), &[]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("exactly one"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(root.path().join("app.wasm")).unwrap(),
        b"prior artifact"
    );
    let output = compile(
        root.path(),
        &[
            "--export",
            "left#calculate=app/calculate",
            "--export",
            "right#calculate=app/calculate",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let engine = engine();
    let component = Component::from_file(&engine, root.path().join("app.wasm")).unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(40_000_000).unwrap();
    let instance = Linker::<()>::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    for name in ["left", "right"] {
        let interface = component.get_export_index(None, name).unwrap();
        let function = component
            .get_export_index(Some(&interface), "calculate")
            .unwrap();
        let function = instance
            .get_typed_func::<(u32,), (u32,)>(&mut store, &function)
            .unwrap();
        assert_eq!(function.call(&mut store, (17,)).unwrap().0, 17);
    }
}

#[test]
fn duplicate_freestanding_markers_require_an_explicit_authoritative_mapping() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("app.sus"), "(ns app) (def ^{:export \"calculate\"} first-choice (fn [x] (+ x 1))) (def ^{:export \"calculate\"} second-choice (fn [x] (+ x 20)))").unwrap();
    std::fs::write(
        root.path().join("api.wit"),
        "package test:duplicate; world api-world { export calculate: func(x: u32) -> u32; }",
    )
    .unwrap();
    std::fs::write(root.path().join("app.wasm"), b"prior artifact").unwrap();
    let output = compile(root.path(), &[]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Ambiguous WIT export mapping"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(root.path().join("app.wasm")).unwrap(),
        b"prior artifact"
    );
    let output = compile(root.path(), &["--export", "calculate=app/second-choice"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let engine = engine();
    let component = Component::from_file(&engine, root.path().join("app.wasm")).unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(40_000_000).unwrap();
    let instance = Linker::<()>::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let calculate = instance
        .get_typed_func::<(u32,), (u32,)>(&mut store, "calculate")
        .unwrap();
    assert_eq!(calculate.call(&mut store, (17,)).unwrap().0, 37);
}

#[test]
fn partial_explicit_mapping_overrides_unsupported_metadata() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("api.wit"), "package test:precedence; world api-world { export chosen: func() -> u32; export inferred: func() -> u32; }").unwrap();
    for (marker, mapping) in [("other/custom", "chosen=app/chosen"), ("17", "chosen=chosen"), ("\"\"", "chosen=app/chosen")] {
        std::fs::write(root.path().join("app.sus"), format!("(ns app) (def ^{{:export {marker}}} chosen (fn [] 31)) (def ^:export inferred (fn [] 42))")).unwrap();
        std::fs::write(root.path().join("app.wasm"), b"prior artifact").unwrap();
        let unmapped = compile(root.path(), &[]);
        assert!(!unmapped.status.success(), "marker {marker}");
        assert_eq!(std::fs::read(root.path().join("app.wasm")).unwrap(), b"prior artifact");
        let output = compile(root.path(), &["--export", mapping]);
        assert!(
            output.status.success(),
            "marker {marker}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let engine = engine();
        let component = Component::from_file(&engine, root.path().join("app.wasm")).unwrap();
        let mut store = Store::new(&engine, ());
        store.set_fuel(40_000_000).unwrap();
        let instance = Linker::<()>::new(&engine)
            .instantiate(&mut store, &component)
            .unwrap();
        for (name, expected) in [("chosen", 31), ("inferred", 42)] {
            let function = instance
                .get_typed_func::<(), (u32,)>(&mut store, name)
                .unwrap();
            assert_eq!(function.call(&mut store, ()).unwrap().0, expected);
        }
    }
}
