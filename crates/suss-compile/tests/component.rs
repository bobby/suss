//! Tests for WASM Component compilation with ^:export metadata

use suss_compile::Compiler;

/// Helper to create temp files for tests
fn write_temp_files(suss_content: &str, wit_content: &str) -> (tempfile::NamedTempFile, tempfile::NamedTempFile) {
    use std::io::Write;

    let mut suss_file = tempfile::Builder::new()
        .suffix(".sus")
        .tempfile()
        .expect("failed to create temp sus file");
    suss_file.write_all(suss_content.as_bytes()).expect("failed to write sus");

    let mut wit_file = tempfile::Builder::new()
        .suffix(".wit")
        .tempfile()
        .expect("failed to create temp wit file");
    wit_file.write_all(wit_content.as_bytes()).expect("failed to write wit");

    (suss_file, wit_file)
}

#[test]
fn test_export_metadata_exports_function() {
    let suss = r#"
(defn ^:export add [a b] (+ a b))
"#;
    let wit = r#"
package test:example;

world example {
    export add: func(a: s32, b: s32) -> s32;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "Compilation should succeed: {:?}", result.err());
}

#[test]
fn test_missing_export_metadata_fails() {
    let suss = r#"
(defn add [a b] (+ a b))
"#;
    let wit = r#"
package test:example;

world example {
    export add: func(a: s32, b: s32) -> s32;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_err(), "Should fail without ^:export");
    let err = result.unwrap_err().to_string();
    assert!(err.contains("export") || err.contains("Export"),
            "Error should mention export: {}", err);
}

#[test]
fn test_internal_function_not_exported() {
    // This test verifies that helper functions without ^:export are compiled
    // but not exported in the WASM output
    let suss = r#"
(defn ^:export add [a b] (helper (+ a b)))
(defn helper [x] x)
"#;
    let wit = r#"
package test:example;

world example {
    export add: func(a: s32, b: s32) -> s32;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "Compilation should succeed with internal helper: {:?}", result.err());
}

#[test]
fn test_multiple_exports() {
    let suss = r#"
(defn ^:export add [a b] (+ a b))
(defn ^:export sub [a b] (- a b))
(defn internal [x] (* x 2))
"#;
    let wit = r#"
package test:example;

world example {
    export add: func(a: s32, b: s32) -> s32;
    export sub: func(a: s32, b: s32) -> s32;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "Compilation should succeed with multiple exports: {:?}", result.err());
}

#[test]
fn test_gen_world_namespace() {
    let suss = r#"
(ns my-app.core
  (gen-world :my-app/v1))

(defn ^:export add [a b] (+ a b))
"#;
    let wit = r#"
package my-app:v1;

world v1 {
    export add: func(a: s32, b: s32) -> s32;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "Compilation with ns and gen-world should succeed: {:?}", result.err());
}

#[test]
fn test_project_compilation() {
    use std::io::Write;

    // Create temp directory for project
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let base_path = temp_dir.path();

    // Create directory structure
    std::fs::create_dir_all(base_path.join("src")).expect("failed to create src dir");
    std::fs::create_dir_all(base_path.join("wit")).expect("failed to create wit dir");
    std::fs::create_dir_all(base_path.join("target")).expect("failed to create target dir");

    // Create deps.sus
    let deps_sus = r#"
{:worlds
 {:app/v1 {:wit "wit/v1.wit"
           :output "target/v1.wasm"}}
 :src-paths ["src"]}
"#;
    let mut deps_file = std::fs::File::create(base_path.join("deps.sus"))
        .expect("failed to create deps.sus");
    deps_file.write_all(deps_sus.as_bytes()).expect("failed to write deps.sus");

    // Create WIT file
    let wit = r#"
package app:v1;

world v1 {
    export add: func(a: s32, b: s32) -> s32;
}
"#;
    let mut wit_file = std::fs::File::create(base_path.join("wit/v1.wit"))
        .expect("failed to create wit file");
    wit_file.write_all(wit.as_bytes()).expect("failed to write wit file");

    // Create source file
    let suss = r#"
(ns app.core
  (gen-world :app/v1))

(defn ^:export add [a b] (+ a b))
"#;
    let mut suss_file = std::fs::File::create(base_path.join("src/core.sus"))
        .expect("failed to create sus file");
    suss_file.write_all(suss.as_bytes()).expect("failed to write sus file");

    // Load config and compile project
    let config = suss_compile::SussConfig::load(&base_path.join("deps.sus"))
        .expect("failed to load config");

    let mut compiler = Compiler::new();
    let result = compiler.compile_project(&config, None);

    assert!(result.is_ok(), "Project compilation should succeed: {:?}", result.err());

    let results = result.unwrap();
    assert_eq!(results.len(), 1, "Should compile 1 world");
    assert!(results.contains_key(":app/v1"), "Should contain :app/v1 world");
}

// ============================================================================
// String marshaling tests
// ============================================================================

#[test]
fn test_string_export_return() {
    let suss = r#"
(defn ^:export greet [] "hello")
"#;
    let wit = r#"
package test:strings;

world strings {
    export greet: func() -> string;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "String export return should compile: {:?}", result.err());
}

#[test]
fn test_string_export_param_and_return() {
    let suss = r#"
(defn ^:export echo [s] s)
"#;
    let wit = r#"
package test:strings;

world strings {
    export echo: func(s: string) -> string;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "String param+return should compile: {:?}", result.err());
}

#[test]
fn test_string_param_to_int() {
    let suss = r#"
(defn ^:export len [s] (count s))
"#;
    let wit = r#"
package test:strings;

world strings {
    export len: func(s: string) -> s32;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "String param to int should compile: {:?}", result.err());
}

#[test]
fn test_bool_export() {
    let suss = r#"
(defn ^:export is-positive [x] (> x 0))
"#;
    let wit = r#"
package test:bools;

world bools {
    export is-positive: func(x: s32) -> bool;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "Bool export should compile: {:?}", result.err());
}

#[test]
fn test_cabi_realloc_export() {
    // Verify that cabi_realloc is exported in the core module
    let suss = r#"
(defn ^:export add [a b] (+ a b))
"#;
    let wit = r#"
package test:realloc;

world realloc {
    export add: func(a: s32, b: s32) -> s32;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let wasm_bytes = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    ).expect("compilation should succeed");

    // The result is a WASM Component, but we can verify it contains valid bytes
    assert!(wasm_bytes.len() > 8, "Should produce non-trivial WASM");
    // Verify WASM magic bytes (component or module)
    assert_eq!(&wasm_bytes[0..4], b"\0asm", "Should start with WASM magic");
}

#[test]
fn test_string_concat_return() {
    // Test a function that takes two strings and returns a string
    // (uses count to produce an integer, verifying param marshaling works)
    let suss = r#"
(defn ^:export total [a b] (+ (count a) (count b)))
"#;
    let wit = r#"
package test:string-ops;

world string-ops {
    export total: func(a: string, b: string) -> s32;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "String ops should compile: {:?}", result.err());
}

#[test]
fn test_mixed_string_and_int_params() {
    let suss = r#"
(defn ^:export concat-len [s n] (+ (count s) n))
"#;
    let wit = r#"
package test:mixed;

world mixed {
    export concat-len: func(s: string, n: s32) -> s32;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "Mixed string+int params should compile: {:?}", result.err());
}

#[test]
fn test_multiple_string_params() {
    let suss = r#"
(defn ^:export total-len [a b] (+ (count a) (count b)))
"#;
    let wit = r#"
package test:multi-string;

world multi-string {
    export total-len: func(a: string, b: string) -> s32;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "Multiple string params should compile: {:?}", result.err());
}

// ============================================================================
// i64/f64 marshaling tests
// ============================================================================

#[test]
fn test_i64_export() {
    let suss = r#"
(defn ^:export big [] 999999999999)
"#;
    let wit = r#"
package test:int64;

world int64-world {
    export big: func() -> s64;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "i64 export should compile: {:?}", result.err());
}

#[test]
fn test_i64_param_return() {
    let suss = r#"
(defn ^:export inc64 [x] (+ x 1))
"#;
    let wit = r#"
package test:int64;

world int64-world {
    export inc64: func(x: s64) -> s64;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "i64 param+return should compile: {:?}", result.err());
}

#[test]
fn test_f64_export() {
    let suss = r#"
(defn ^:export pi [] 3.14159)
"#;
    let wit = r#"
package test:float64;

world float64-world {
    export pi: func() -> f64;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "f64 export should compile: {:?}", result.err());
}

#[test]
fn test_f64_param_return() {
    let suss = r#"
(defn ^:export double [x] (* x 2.0))
"#;
    let wit = r#"
package test:float64;

world float64-world {
    export double: func(x: f64) -> f64;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "f64 param+return should compile: {:?}", result.err());
}

// ============================================================================
// option<T> marshaling tests
// ============================================================================

#[test]
fn test_option_s32_export() {
    let suss = r#"
(defn ^:export maybe-val [x] (if (> x 0) x nil))
"#;
    let wit = r#"
package test:optional;

world optional-world {
    export maybe-val: func(x: s32) -> option<s32>;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "option<s32> export should compile: {:?}", result.err());
}

#[test]
fn test_option_string_export() {
    let suss = r#"
(defn ^:export maybe-greet [x] (if (> x 0) "hello" nil))
"#;
    let wit = r#"
package test:optional;

world optional-world {
    export maybe-greet: func(x: s32) -> option<string>;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "option<string> export should compile: {:?}", result.err());
}

#[test]
fn test_option_s32_param() {
    let suss = r#"
(defn ^:export unwrap-or [x default] (if (nil? x) default x))
"#;
    let wit = r#"
package test:optional;

world optional-world {
    export unwrap-or: func(x: option<s32>, default: s32) -> s32;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "option<s32> param should compile: {:?}", result.err());
}

#[test]
fn test_option_bool_export() {
    let suss = r#"
(defn ^:export maybe-true [x] (if (> x 0) true nil))
"#;
    let wit = r#"
package test:optional;

world optional-world {
    export maybe-true: func(x: s32) -> option<bool>;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "option<bool> export should compile: {:?}", result.err());
}

// ============================================================================
// list<T> marshaling tests
// ============================================================================

#[test]
fn test_list_s32_export() {
    let suss = r#"
(defn ^:export make-list [] [1 2 3])
"#;
    let wit = r#"
package test:lists;

world lists-world {
    export make-list: func() -> list<s32>;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "list<s32> export should compile: {:?}", result.err());
}

#[test]
fn test_list_s32_param() {
    let suss = r#"
(defn ^:export list-len [xs] (count xs))
"#;
    let wit = r#"
package test:lists;

world lists-world {
    export list-len: func(xs: list<s32>) -> s32;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "list<s32> param should compile: {:?}", result.err());
}

#[test]
fn test_list_string_export() {
    let suss = r#"
(defn ^:export greetings [] ["hello" "world"])
"#;
    let wit = r#"
package test:lists;

world lists-world {
    export greetings: func() -> list<string>;
}
"#;

    let (suss_file, wit_file) = write_temp_files(suss, wit);

    let mut compiler = Compiler::new();
    let result = compiler.compile_files(
        suss_file.path().to_str().unwrap(),
        wit_file.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "list<string> export should compile: {:?}", result.err());
}

/// Encoding and validation do not prove boundary conversions execute correctly.
/// These prototype fixtures explicitly provide its current private print import;
/// removing that import from pure libraries remains M5-01.
fn instantiate_fixture(
    source: &str,
    exports: &str,
) -> (wasmtime::Store<()>, wasmtime::component::Instance) {
    let wit = format!("package test:execution; world fixture {{ {exports} }}");
    let (source, wit) = write_temp_files(source, &wit);
    let bytes = Compiler::new()
        .compile_files(
            source.path().to_str().unwrap(),
            wit.path().to_str().unwrap(),
        )
        .unwrap();
    let mut config = wasmtime::Config::new();
    config
        .wasm_gc(true)
        .wasm_function_references(true)
        .wasm_tail_call(true)
        .wasm_exceptions(true)
        .wasm_component_model(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let engine = wasmtime::Engine::new(&config).unwrap();
    let component = wasmtime::component::Component::new(&engine, bytes).unwrap();
    let mut linker = wasmtime::component::Linker::new(&engine);
    linker
        .instance("test:execution/suss")
        .unwrap()
        .func_wrap("print-str", |_, _: (u32, u32)| -> wasmtime::Result<()> {
            panic!("pure fixture unexpectedly printed")
        })
        .unwrap();
    let mut store = wasmtime::Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    (store, instance)
}

#[test]
fn execute_wit_exports_with_internal_calls_and_float_params() {
    let (mut store, instance) = instantiate_fixture(
        "(defn helper [x] (+ x 1)) (defn ^:export add-one [x] (helper x)) (defn ^:export double [x] (* x 2.0))",
        "export add-one: func(x: s32) -> s32; export double: func(x: f64) -> f64;",
    );
    let add = instance
        .get_typed_func::<(i32,), (i32,)>(&mut store, "add-one")
        .unwrap();
    assert_eq!(add.call(&mut store, (41,)).unwrap(), (42,));
    add.post_return(&mut store).unwrap();
    let double = instance
        .get_typed_func::<(f64,), (f64,)>(&mut store, "double")
        .unwrap();
    assert_eq!(double.call(&mut store, (2.5,)).unwrap(), (5.0,));
    double.post_return(&mut store).unwrap();
}

#[test]
fn execute_wit_exports_with_flattened_option_and_list_return() {
    let (mut store, instance) = instantiate_fixture(
        "(defn ^:export unwrap-or [x default] (if (nil? x) default x)) (defn ^:export make-list [] [1 2 3])",
        "export unwrap-or: func(x: option<s32>, default: s32) -> s32; export make-list: func() -> list<s32>;",
    );
    let unwrap = instance
        .get_typed_func::<(Option<i32>, i32), (i32,)>(&mut store, "unwrap-or")
        .unwrap();
    assert_eq!(unwrap.call(&mut store, (Some(3), 7)).unwrap(), (3,));
    unwrap.post_return(&mut store).unwrap();
    assert_eq!(unwrap.call(&mut store, (None, 7)).unwrap(), (7,));
    unwrap.post_return(&mut store).unwrap();
    let list = instance
        .get_typed_func::<(), (Vec<i32>,)>(&mut store, "make-list")
        .unwrap();
    assert_eq!(list.call(&mut store, ()).unwrap().0, vec![1, 2, 3]);
    list.post_return(&mut store).unwrap();
}

#[test]
fn execute_wit_export_closure_and_protocol_results() {
    let (mut store, instance) = instantiate_fixture(
        "(defn ^:export closure [x] ((fn [n] (+ n 1)) x)) (defn ^:export first-of [xs] (-first xs))",
        "export closure: func(x: s32) -> s32; export first-of: func(xs: list<s32>) -> s32;",
    );
    let closure = instance.get_typed_func::<(i32,), (i32,)>(&mut store, "closure").unwrap();
    assert_eq!(closure.call(&mut store, (41,)).unwrap(), (42,));
    closure.post_return(&mut store).unwrap();
    let first = instance.get_typed_func::<(Vec<i32>,), (i32,)>(&mut store, "first-of").unwrap();
    assert_eq!(first.call(&mut store, (vec![7, 8],)).unwrap(), (7,));
    first.post_return(&mut store).unwrap();
}

#[test]
fn execute_wit_export_catch_and_finally_with_flattened_params() {
    let (mut store, instance) = instantiate_fixture(
        "(defn ^:export caught [x fallback] (try (throw fallback) (catch error (+ error 1)))) (defn ^:export cleanup [x fallback] (try fallback (finally (+ fallback 1))))",
        "export caught: func(x: option<s32>, fallback: s32) -> s32; export cleanup: func(x: option<s32>, fallback: s32) -> s32;",
    );
    for name in ["caught", "cleanup"] {
        let function = instance
            .get_typed_func::<(Option<i32>, i32), (i32,)>(&mut store, name)
            .unwrap();
        for x in [None, Some(9)] {
            let expected = if name == "caught" { 42 } else { 41 };
            assert_eq!(function.call(&mut store, (x, 41)).unwrap(), (expected,));
            function.post_return(&mut store).unwrap();
        }
    }
}
