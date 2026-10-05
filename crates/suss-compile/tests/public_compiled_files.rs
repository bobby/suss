//! Execute public file, namespace and project APIs through their actual artifacts.
#![cfg(not(target_family = "wasm"))]

use std::{path::Path, sync::OnceLock};
use suss_compile::{Compiler, SussConfig};
use wasmtime::{
    component::{Component, Linker},
    AsContextMut, Config, Engine, Store,
};

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
                .wasm_component_model_async(true)
                .wasm_component_model_implements(true)
                .consume_fuel(true)
                .cranelift_opt_level(wasmtime::OptLevel::None);
            Engine::new(&config).unwrap()
        })
        .clone()
}

fn write(root: &Path, name: &str, contents: &str) {
    std::fs::write(root.join(name), contents).unwrap();
}
const WIT: &str = "package test:public-files; world api { export calculate: func(x: f64) -> f64; }";

fn execute(bytes: Vec<u8>, interface: Option<&str>, observations: &[(f64, f64)]) {
    let engine = engine();
    let component = Component::new(&engine, bytes).unwrap();
    assert_eq!(component.component_type().imports(&engine).count(), 0);
    for _ in 0..2 {
        let mut store = Store::new(&engine, ());
        store.set_fuel(100_000_000).unwrap();
        let instance = Linker::new(&engine)
            .instantiate(&mut store, &component)
            .unwrap();
        let function = if let Some(interface) = interface {
            let interface = instance
                .get_export_index(&mut store, None, interface)
                .unwrap();
            let function = instance
                .get_export_index(&mut store, Some(&interface), "calculate")
                .unwrap();
            instance
                .get_typed_func::<(f64,), (f64,)>(&mut store, &function)
                .unwrap()
        } else {
            instance
                .get_typed_func::<(f64,), (f64,)>(&mut store, "calculate")
                .unwrap()
        };
        for &(input, expected) in observations {
            assert_eq!(
                function.call(&mut store, (input,)).unwrap().0.to_bits(),
                expected.to_bits()
            );
            store.gc(None).unwrap();
        }
    }
}

#[test]
fn public_file_preserves_macro_filename_lexical_environment_and_retained_effects() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("app.sus");
    let source = format!(
        r#"
        (defmacro require-source-local [x]
          (if (= (get (meta &form) :file) {:?})
            (if (contains? (get &env :locals) x) x (throw 17)) (throw 17)))
        (def seen (atom 0))
        (defn ^:export calculate [x]
          (let [answer (+ x (swap! seen inc))] (require-source-local answer)))
    "#,
        path.to_str().unwrap()
    );
    write(root.path(), "app.sus", &source);
    write(root.path(), "api.wit", WIT);
    let bytes = Compiler::new()
        .compile_files(
            path.to_str().unwrap(),
            root.path().join("api.wit").to_str().unwrap(),
        )
        .unwrap();
    execute(bytes, None, &[(41.0, 42.0), (41.0, 43.0), (41.0, 44.0)]);
}

#[test]
fn public_file_resolves_wit_before_effectful_macro_expansion() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "app.sus",
        "(defmacro fail [] (throw 17)) (defn ^:export calculate [x] (fail))",
    );
    write(root.path(), "api.wit", "invalid WIT");
    let error = Compiler::new()
        .compile_files(
            root.path().join("app.sus").to_str().unwrap(),
            root.path().join("api.wit").to_str().unwrap(),
        )
        .unwrap_err();
    assert!(
        matches!(error, suss_compile::CompileError::Wit(_)),
        "{error}"
    );
}

#[test]
fn public_file_keeps_io_and_unsupported_boundary_errors_distinct() {
    use suss_compile::CompileError;
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "api.wit", WIT);
    let source = root.path().join("app.sus");
    let wit = root.path().join("api.wit");
    let error = Compiler::new()
        .compile_files(source.to_str().unwrap(), wit.to_str().unwrap())
        .unwrap_err();
    assert!(matches!(error, CompileError::Io(_)), "{error}");
    write(root.path(), "app.sus", "(defn ^:export calculate [x] x)");
    let error = Compiler::new()
        .compile_files(
            source.to_str().unwrap(),
            root.path().join("missing.wit").to_str().unwrap(),
        )
        .unwrap_err();
    assert!(matches!(error, CompileError::Io(_)), "{error}");
    write(
        root.path(),
        "app.sus",
        "(defmacro fail [] (throw 17)) (defn ^:export calculate [x] (fail))",
    );
    write(
        root.path(),
        "api.wit",
        "package test:unsupported-file; world api { export calculate: func(x: list<map<string, u32>>) -> f64; }",
    );
    let error = Compiler::new()
        .compile_files(source.to_str().unwrap(), wit.to_str().unwrap())
        .unwrap_err();
    assert!(matches!(error, CompileError::Unsupported(_)), "{error}");
    assert!(
        error.to_string().contains("list<map<string, u32>>"),
        "{error}"
    );
}

#[test]
fn public_namespace_executes_portable_dependency_and_separate_macro_import() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "app.sus",
        "(ns app (:require [math :as m]) (:require-macros [tools :refer [twice]])) (defn ^:export calculate [x] (twice (+ x m/bias)))",
    );
    write(
        root.path(),
        "math.cljs",
        "(ns math) (def bias #?(:suss 21 :cljs 99))",
    );
    write(
        root.path(),
        "tools.cljc",
        "(ns tools) (defmacro twice [x] `(+ ~x ~x))",
    );
    write(root.path(), "api.wit", WIT);
    let bytes = Compiler::new()
        .compile_with_namespaces(
            "app",
            &[root.path().to_owned()],
            root.path().join("api.wit").to_str().unwrap(),
        )
        .unwrap();
    execute(bytes, None, &[(0.0, 42.0), (1.0, 44.0)]);
}

#[test]
fn public_namespace_rejects_ambiguous_source_before_macro_effects() {
    let root = tempfile::tempdir().unwrap();
    let source = "(ns app) (defmacro fail [] (throw 17)) (defn ^:export calculate [x] (fail))";
    write(root.path(), "app.sus", source);
    write(root.path(), "app.cljs", source);
    write(root.path(), "api.wit", WIT);
    let error = Compiler::new()
        .compile_with_namespaces(
            "app",
            &[root.path().to_owned()],
            root.path().join("api.wit").to_str().unwrap(),
        )
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Ambiguous") || error.contains("ambiguous"),
        "{error}"
    );
    assert!(
        error.contains("app.sus") && error.contains("app.cljs"),
        "{error}"
    );
}

#[test]
fn public_namespace_reports_mismatched_declaration_before_macro_effects() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("app.sus");
    write(
        root.path(),
        "app.sus",
        "(ns different) (defmacro fail [] (throw 17)) (defn ^:export calculate [x] (fail))",
    );
    write(root.path(), "api.wit", WIT);
    let error = Compiler::new()
        .compile_with_namespaces(
            "app",
            &[root.path().to_owned()],
            root.path().join("api.wit").to_str().unwrap(),
        )
        .unwrap_err();
    assert!(
        matches!(error, suss_compile::CompileError::Semantic(_)),
        "{error}"
    );
    let message = error.to_string();
    assert!(
        message.contains("Declared namespace different does not match requested app"),
        "{message}"
    );
    assert!(message.contains(path.to_str().unwrap()), "{message}");
}

#[test]
fn public_project_honors_configured_namespace_selected_world_and_explicit_mapping() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    write(
        root.path(),
        "src/app.cljs",
        "(ns app) (defmacro twice [x] `(+ ~x ~x)) (defn calculate [x] (twice (+ x 21)))",
    );
    write(
        root.path(),
        "api.wit",
        "package test:public-project; interface operations { calculate: func(x: f64) -> f64; } world unused {} world chosen { export api: operations; }",
    );
    write(
        root.path(),
        "deps.sus",
        "{:src-paths [\"src\"] :worlds {:good {:namespace app :wit \"api.wit\" :wit-world \"chosen\" :exports {\"api#calculate\" app/calculate} :output \"app.wasm\"} :unselected {:namespace absent :wit \"absent.wit\" :output \"absent.wasm\"}}}",
    );
    let config = SussConfig::load(&root.path().join("deps.sus")).unwrap();
    let mut artifacts = Compiler::new()
        .compile_project(&config, Some(":good"))
        .unwrap();
    assert_eq!(artifacts.len(), 1);
    execute(
        artifacts.remove(":good").unwrap(),
        Some("api"),
        &[(0.0, 42.0), (1.0, 44.0)],
    );
    assert!(
        !root.path().join("app.wasm").exists(),
        "library compilation returns bytes without publishing output"
    );
    assert!(!root.path().join("absent.wasm").exists());
}

#[test]
fn public_project_diagnoses_unimplemented_published_dependencies_instead_of_ignoring_them() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    write(
        root.path(),
        "src/app.sus",
        "(ns app (gen-world :good)) (defn ^:export calculate [x] (+ x 1))",
    );
    write(root.path(), "api.wit", WIT);
    write(
        root.path(),
        "deps.sus",
        "{:src-paths [\"src\"] :deps {:lib \"1\"} :worlds {:good {:wit \"api.wit\" :output \"app.wasm\"}}}",
    );
    let config = SussConfig::load(&root.path().join("deps.sus")).unwrap();
    let error = Compiler::new()
        .compile_project(&config, None)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Published project dependency loading remains unimplemented"),
        "{error}"
    );
    assert!(!root.path().join("app.wasm").exists());
}

#[test]
fn public_file_defers_runtime_initialization_and_preserves_exception_payload() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "app.sus",
        "(defmacro payload [] 42) (def initialized (throw (payload)))",
    );
    write(
        root.path(),
        "api.wit",
        "package test:public-init; world api {}",
    );
    let bytes = Compiler::new()
        .compile_files(
            root.path().join("app.sus").to_str().unwrap(),
            root.path().join("api.wit").to_str().unwrap(),
        )
        .unwrap();
    let engine = engine();
    let component = Component::new(&engine, bytes).unwrap();
    assert_eq!(component.component_type().imports(&engine).count(), 0);
    let mut store = Store::new(&engine, ());
    store.set_fuel(100_000_000).unwrap();
    assert!(Linker::new(&engine)
        .instantiate(&mut store, &component)
        .is_err());
    let exception = store
        .as_context_mut()
        .take_pending_exception()
        .expect("language initializer exception");
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
        42.0_f64.to_bits()
    );
}

#[test]
fn public_file_ordinary_tail_calls_keep_bounded_stack_and_finally_cleanup() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "app.sus",
        r#"
      (def cleaned (atom 0))
      (defn walk [n total]
        (if (<= n 0) total (walk (dec n) (+ total 1))))
      (defn pending [n]
        (if (<= n 0) 0
          (try (pending (dec n)) (finally (swap! cleaned inc)))))
      (defn ^:export calculate [x]
        (let [answer (try (if (< x 0) (pending (- x)) (walk x 0))
                      (finally (swap! cleaned inc)))]
          (+ answer @cleaned)))
    "#,
    );
    write(root.path(), "api.wit", WIT);
    let bytes = Compiler::new()
        .compile_files(
            root.path().join("app.sus").to_str().unwrap(),
            root.path().join("api.wit").to_str().unwrap(),
        )
        .unwrap();
    execute(
        bytes,
        None,
        &[(100_000.0, 100_001.0), (0.0, 2.0), (7.0, 10.0), (-3.0, 7.0)],
    );
}

#[test]
fn public_file_tail_calls_read_live_global_after_redefinition() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "app.sus",
        r#"
      (defn original [n]
        (if (== n 0) 7
          (do (set! original (fn [m] (+ m 40))) (original (dec n)))))
      (def saved original)
      (defn ^:export calculate [x] (saved x))
    "#,
    );
    write(root.path(), "api.wit", WIT);
    let bytes = Compiler::new()
        .compile_files(
            root.path().join("app.sus").to_str().unwrap(),
            root.path().join("api.wit").to_str().unwrap(),
        )
        .unwrap();
    execute(bytes, None, &[(3.0, 42.0), (2.0, 41.0), (0.0, 7.0)]);
}

#[test]
fn public_file_mod_preserves_negative_divisor_fraction_and_signed_zero() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "app.sus",
        "(defn ^:export calculate [x] (mod x -3))",
    );
    write(root.path(), "api.wit", WIT);
    let bytes = Compiler::new()
        .compile_files(
            root.path().join("app.sus").to_str().unwrap(),
            root.path().join("api.wit").to_str().unwrap(),
        )
        .unwrap();
    execute(
        bytes,
        None,
        &[(5.0, -1.0), (-5.0, -2.0), (1.5, -1.5), (0.0, -0.0)],
    );
}

#[test]
fn public_file_mod_retains_compiled_numeric_macro_semantics_after_js_mod_redefinition() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "app.sus",
        r#"
      (ns suss.core)
      (def saved-mod mod)
      (def js-mod (fn* [n d] 99))
      (def ^:export calculate (fn* [x] (saved-mod x -3)))
    "#,
    );
    write(root.path(), "api.wit", WIT);
    let bytes = Compiler::new()
        .compile_files(
            root.path().join("app.sus").to_str().unwrap(),
            root.path().join("api.wit").to_str().unwrap(),
        )
        .unwrap();
    execute(
        bytes,
        None,
        &[(5.0, -1.0), (-5.0, -2.0), (1.5, -1.5), (0.0, -0.0)],
    );
}
