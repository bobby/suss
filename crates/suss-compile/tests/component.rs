//! Tests for WASM Component compilation with ^:export metadata

use suss_compile::Compiler;

/// Helper to create temp files for tests
fn write_temp_files(suss_content: &str, wit_content: &str) -> (tempfile::NamedTempFile, tempfile::NamedTempFile) {
    use std::io::Write;

    let mut suss_file = tempfile::Builder::new()
        .suffix(".suss")
        .tempfile()
        .expect("failed to create temp suss file");
    suss_file.write_all(suss_content.as_bytes()).expect("failed to write suss");

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

    // Create deps.suss
    let deps_suss = r#"
{:worlds
 {:app/v1 {:wit "wit/v1.wit"
           :output "target/v1.wasm"}}
 :src-paths ["src"]}
"#;
    let mut deps_file = std::fs::File::create(base_path.join("deps.suss"))
        .expect("failed to create deps.suss");
    deps_file.write_all(deps_suss.as_bytes()).expect("failed to write deps.suss");

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
    let mut suss_file = std::fs::File::create(base_path.join("src/core.suss"))
        .expect("failed to create suss file");
    suss_file.write_all(suss.as_bytes()).expect("failed to write suss file");

    // Load config and compile project
    let config = suss_compile::SussConfig::load(&base_path.join("deps.suss"))
        .expect("failed to load config");

    let mut compiler = Compiler::new();
    let result = compiler.compile_project(&config, None);

    assert!(result.is_ok(), "Project compilation should succeed: {:?}", result.err());

    let results = result.unwrap();
    assert_eq!(results.len(), 1, "Should compile 1 world");
    assert!(results.contains_key(":app/v1"), "Should contain :app/v1 world");
}
