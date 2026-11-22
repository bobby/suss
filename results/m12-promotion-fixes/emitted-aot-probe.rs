#[test]
fn emitted_macro_biginteger_source_executes_as_an_aot_component() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("emitted.sus");
    let source = r#"(ns emittedinteger.probe)
      (defmacro asbig [x] (biginteger x))
      (defmacro children [] (list 'quote [(numerator 2/7) (denominator 2/7)]))
      (def small (asbig 1))
      (def wide (asbig 9223372036854775808N))
      (def pair (children))
      (def check (fn [unused]
        (if (and (= small 1N) (= wide 9223372036854775808N) (= pair [2N 7N])
                 (instance? java.math.BigInteger small)
                 (instance? java.math.BigInteger wide)
                 (instance? java.math.BigInteger (nth pair 0))
                 (instance? java.math.BigInteger (nth pair 1))) 1 0)))"#;
    let fragments = analyze_source(source, &path, &[], None, None).unwrap();
    let document = hir_document(&fragments, &path).unwrap();
    let emitted = emitted_source(&document).unwrap();
    assert!(emitted.contains("#suss/biginteger"));
    std::fs::write(&path, emitted).unwrap();
    std::fs::write(dir.path().join("probe.wit"), "package test:emittedinteger; world w { export check: func(unused: u32) -> u32; }").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_suss")).current_dir(dir.path())
      .args(["compile","emitted.sus","-w","probe.wit","--export","check=emittedinteger.probe/check","-o","probe.wasm"]).output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let result = Command::new(env!("CARGO_BIN_EXE_suss")).current_dir(dir.path())
      .args(["run","probe.wasm","--invoke","check","--","0"]).output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), "1");
}
