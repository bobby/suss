//! Execute artifacts produced by the native compile-file command.
use std::{
    path::Path,
    process::{Command, Output},
    sync::OnceLock,
};
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
fn compile(root: &Path, flags: &[&str]) -> Output {
    compile_with_wit(root, "api.wit", flags)
}
fn compile_with_wit(root: &Path, wit: &str, flags: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root)
        .args(["compile", "app.sus", "-w", wit, "-o", "app.wasm"])
        .args(flags)
        .output()
        .unwrap()
}

fn invoke(root: &Path, export: &str, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root)
        .args(["run", "app.wasm", "--invoke", export, "--"])
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn compile_file_uses_compiled_macros_dependencies_and_exact_interface_mappings() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("math.sus"), "(ns math) (def bias 2)").unwrap();
    std::fs::write(root.path().join("app.sus"), "(ns app (:require [math :as m])) (def seen 0) (defmacro twice [x] `(+ ~x ~x)) (def calculate (fn [x] (do (set! seen (+ seen 1)) (twice (+ x m/bias))))) (def effects (fn [] seen)) (def choose (fn [flag left right] (if flag (+ left right) (- left right)))) (def echo (fn [x] x)) (set! seen (+ seen 1))").unwrap();
    std::fs::create_dir_all(root.path().join("wit/deps")).unwrap();
    std::fs::write(root.path().join("wit/deps/command.wit"), "package test:command@1.2.3; interface api { calculate: func(x: u32) -> u32; effects: func() -> f64; choose: func(flag: bool, left: f32, right: s16) -> f64; }").unwrap();
    std::fs::write(
        root.path().join("wit/api.wit"),
        "package test:app@0.1.0; world api-world { export test:command/api@1.2.3; export run: func(x: s32) -> s32; } world other {}",
    )
    .unwrap();
    let output = compile_with_wit(
        root.path(),
        "wit",
        &[
            "--wit-world",
            "api-world",
            "--src",
            ".",
            "--export",
            "test:command/api@1.2.3#calculate=app/calculate",
            "--export",
            "test:command/api@1.2.3#effects=app/effects",
            "--export",
            "test:command/api@1.2.3#choose=app/choose",
            "--export",
            "run=app/echo",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let engine = engine();
    let component = Component::from_file(&engine, root.path().join("app.wasm")).unwrap();
    let api = component
        .get_export_index(None, "test:command/api@1.2.3")
        .unwrap();
    let calculate = component.get_export_index(Some(&api), "calculate").unwrap();
    let effects = component.get_export_index(Some(&api), "effects").unwrap();
    for _ in 0..2 {
        let mut store = Store::new(&engine, ());
        store.set_fuel(100_000_000).unwrap();
        let instance = Linker::new(&engine)
            .instantiate(&mut store, &component)
            .unwrap();
        let calculate = instance
            .get_typed_func::<(u32,), (u32,)>(&mut store, &calculate)
            .unwrap();
        let effects = instance
            .get_typed_func::<(), (f64,)>(&mut store, &effects)
            .unwrap();
        assert_eq!(effects.call(&mut store, ()).unwrap().0, 1.0);
        effects.post_return(&mut store).unwrap();
        assert_eq!(calculate.call(&mut store, (19,)).unwrap().0, 42);
        calculate.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
        assert_eq!(calculate.call(&mut store, (1,)).unwrap().0, 6);
        calculate.post_return(&mut store).unwrap();
        assert_eq!(effects.call(&mut store, ()).unwrap().0, 3.0);
        effects.post_return(&mut store).unwrap();
    }
    for (path, arguments, expected) in [
        ("test:command/api@1.2.3#calculate", vec!["19"], "42\n"),
        ("test:command/api@1.2.3#effects", vec![], "1\n"),
        (
            "test:command/api@1.2.3#choose",
            vec!["true", "1.25", "-8"],
            "-6.75\n",
        ),
        (
            "test:command/api@1.2.3#choose",
            vec!["false", "1.25", "-8"],
            "9.25\n",
        ),
        ("run", vec!["-2147483648"], "-2147483648\n"),
    ] {
        let output = invoke(root.path(), path, &arguments);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
    }
    for (path, arguments, expected_error) in [
        (
            "test:command/api@1.2.3#calculate",
            vec!["oops"],
            "Invalid u32 argument",
        ),
        (
            "test:command/api@1.2.3#calculate",
            vec!["4294967296"],
            "Invalid u32 argument",
        ),
        (
            "test:command/api@1.2.3#calculate",
            vec![],
            "expects 1 arguments, got 0",
        ),
        (
            "test:command/api@1.2.3#choose",
            vec!["no", "1.25", "-8"],
            "Invalid bool argument",
        ),
        (
            "test:command/api@1.2.3#choose",
            vec!["true", "bad", "-8"],
            "Invalid f32 argument",
        ),
        (
            "test:command/api@1.2.3#choose",
            vec!["true", "1.25", "32768"],
            "Invalid s16 argument",
        ),
        (
            "test:command/api@1.2.3#missing",
            vec![],
            "not found in component",
        ),
    ] {
        let output = invoke(root.path(), path, &arguments);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected_error),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn compile_file_defers_runtime_throw_and_preserves_output_on_invalid_selection_or_mapping() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("app.sus"),
        "(ns app) (def calculate (fn [x] x)) (throw 17) (throw 99)",
    )
    .unwrap();
    std::fs::write(
        root.path().join("api.wit"),
        "package test:command; world api { export calculate: func(x: f64) -> f64; } world empty {}",
    )
    .unwrap();
    for (flags, expected_error) in [
        (vec![], "Failed to select WIT world"),
        (vec!["--wit-world", "absent"], "Failed to select WIT world"),
        (
            vec!["--wit-world", "api"],
            "WIT export calculate needs exactly one explicit Suss var mapping",
        ),
        (
            vec!["--wit-world", "api", "--export", "=app/calculate"],
            "Invalid export mapping",
        ),
        (
            vec![
                "--wit-world",
                "api",
                "--export",
                "calculate=app/calculate",
                "--export",
                "calculate=app/calculate",
            ],
            "WIT export calculate needs exactly one explicit Suss var mapping",
        ),
        (
            vec![
                "--wit-world",
                "api",
                "--export",
                "calculate=app/calculate",
                "--export",
                "unknown=app/calculate",
            ],
            "Unknown WIT export mapping",
        ),
    ] {
        std::fs::write(root.path().join("app.wasm"), b"prior artifact").unwrap();
        let output = compile(root.path(), &flags);
        assert!(
            !output.status.success(),
            "invalid flags accepted: {flags:?}"
        );
        assert_eq!(
            std::fs::read(root.path().join("app.wasm")).unwrap(),
            b"prior artifact"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(expected_error), "{flags:?}: {stderr}");
    }
    let output = compile(
        root.path(),
        &["--wit-world", "api", "--export", "calculate=app/calculate"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let engine = engine();
    let component = Component::from_file(&engine, root.path().join("app.wasm")).unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(100_000_000).unwrap();
    assert!(
        Linker::new(&engine)
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

#[test]
fn typed_invocation_validates_selection_and_arguments_before_guest_initializers() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("app.sus"),
        "(ns app) (def echo (fn [x] x)) (throw 17)",
    )
    .unwrap();
    std::fs::write(root.path().join("api.wit"), "package test:preflight; interface group { echo: func(x: s32) -> s32; } world api { export echo: func(x: s32) -> s32; export group; }").unwrap();
    let output = compile(
        root.path(),
        &[
            "--export",
            "echo=app/echo",
            "--export",
            "test:preflight/group#echo=app/echo",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for (export, arguments, expected) in [
        ("echo", vec!["oops"], "Invalid s32 argument"),
        ("echo", vec!["2147483648"], "Invalid s32 argument"),
        ("echo", vec![], "expects 1 arguments, got 0"),
        ("missing", vec!["7"], "not found in component"),
        ("test:preflight/group#echo", vec!["oops"], "Invalid s32 argument"),
        ("test:preflight/group", vec![], "is not a function"),
        ("echo#missing", vec![], "not found in component"),
    ] {
        let output = invoke(root.path(), export, &arguments);
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{export} {arguments:?}: {stderr}"
        );
        assert!(
            !stderr.contains("Failed to instantiate"),
            "guest initialization ran before input rejection: {stderr}"
        );
        assert!(output.stdout.is_empty());
    }
    // With valid inputs initialization still runs, and its language throw fails
    // the command rather than being silently discarded by preflight validation.
    let valid = invoke(root.path(), "echo", &["7"]);
    assert!(!valid.status.success());
    assert!(String::from_utf8_lossy(&valid.stderr).contains("Failed to instantiate"));
    assert!(valid.stdout.is_empty());
}

#[test]
fn typed_invocation_rejects_malformed_integer_instead_of_substituting_zero() {
    use suss_compile::portable;
    use suss_reader::Symbol;
    let root = tempfile::tempdir().unwrap();
    let fragments = suss_cli::portable_aot::prepare_source(
        "(ns app) (def echo (fn [x] x)) (def add (fn [x] (+ x 1)))",
        None,
        &[],
    )
    .unwrap();
    let mut resolve = portable::aot::Resolve::new();
    let package = resolve.push_str("api.wit", "package test:arguments; world api { export echo: func(x: s32) -> s32; export add: func(x: f64) -> f64; }").unwrap();
    let world = resolve.select_world(&[package], Some("api")).unwrap();
    let bytes = portable::aot::component(
        &fragments,
        &resolve,
        world,
        &[
            ("echo".into(), Symbol::namespaced("app", "echo")),
            ("add".into(), Symbol::namespaced("app", "add")),
        ],
    )
    .unwrap();
    std::fs::write(root.path().join("app.wasm"), bytes).unwrap();
    let invalid = invoke(root.path(), "echo", &["oops"]);
    assert!(
        !invalid.status.success(),
        "malformed s32 was accepted; stdout={:?}",
        String::from_utf8_lossy(&invalid.stdout)
    );
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("Invalid s32 argument"));
    assert!(invalid.stdout.is_empty());
    let floating = invoke(root.path(), "add", &["1.5"]);
    assert!(
        floating.status.success(),
        "{}",
        String::from_utf8_lossy(&floating.stderr)
    );
    assert_eq!(String::from_utf8(floating.stdout).unwrap(), "2.5\n");
}
