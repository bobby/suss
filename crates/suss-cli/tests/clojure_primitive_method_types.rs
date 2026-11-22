//! Primitive method checks use lexical source types, including captures (#337).
use suss_cli::portable_session::Session;

// Fresh JVM Clojure 1.12.6; results/m12-primitive-methods retains source and output.
const CASES: &[(&str, &str)] = &[
    ("(let [^double x (identity 5)] ((fn ^double [] (bit-and x 1))))", "1.0"),
    ("(let [^long x (identity 5.5)] ((fn ^double [] x)))", "5.5"),
    ("((fn [^long x] (if (zero? x) (recur (int 1)) x)) 0)", "1"),
    ("((fn [^double x] (if (zero? x) (recur (float 1)) x)) 0.0)", "1.0"),
    ("((fn ^long [x] (if (zero? x) (recur (identity 1)) x)) 0)", "1"),
    ("(let [x (float 1)] ((fn ^double [] x)))", "1.0"),
    ("(let [x (int 1)] ((fn ^long [] x)))", "1"),
    ("(let [x 1.0] ((fn ^double [] (loop [n 0] (if (zero? n) (recur 1) x)))))", "1.0"),
    ("(let [x 1.0] ((fn ^double ([] x) ([n] x))))", "1.0"),
    ("(let [x 1.0] ((fn ^long [x] x) 5.5))", "5"),
    ("((fn [^long x] (loop [y (identity 0)] (if (zero? y) (recur (identity 1)) y))) 0)", "1"),
    ("(let [^long y (identity 1)] ((fn [^long x] (if (zero? x) (recur (+ y 0)) x)) 0))", "1"),
    ("(let [^double x (identity 5)] ((fn f (^double [] (bit-and x 1)) (^long [n] n))))", "1.0"),
    ("(let [x 1.0] ((fn ^long [] (loop [y x n 0] (if (zero? n) (recur (identity y) 1) y)))))", "1"),
    ("((fn [x] ((fn ^long [] x))) 1.75)", "1"),
    ("((fn [x] ((fn ^double [] x))) 3)", "3.0"),
    ("((fn [x] (if (zero? x) (recur (identity 1)) x)) 0)", "1"),
];

fn observed(source: &str) -> String {
    format!("(pr-str (try {source} (catch :default e [:hint-error (ex-message e)])))")
}

#[test]
fn runtime_and_macro_preserve_primitive_method_types() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for (source, expected) in CASES {
            let value = session.eval(&observed(source)).unwrap_or_else(|e| panic!("{source}: {e}"));
            session.collect().unwrap();
            let units = session.inspect(&value, |mut store, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                Ok(array.elems(&mut store)?.map(|unit| unit.unwrap_i32() as u16).collect::<Vec<_>>())
            }).unwrap();
            assert_eq!(String::from_utf16(&units).unwrap(), *expected, "{source}");
        }
    }
}

#[test]
fn aot_preserves_primitive_method_types() {
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    let mut source = String::from("(ns localhints)\n");
    let mut wit = String::from("package test:localhints; world w {\n");
    for (index, (expression, expected)) in CASES.iter().enumerate() {
        // Only the unused WIT argument is synthesized as a primitive. The
        // helper and local bindings retain their genuine source hints.
        source.push_str(&format!("(def case{index} (fn [unused] (if (= {} {expected:?}) 1 0)))\n", observed(expression)));
        wit.push_str(&format!("export case{index}: func(unused: u32) -> u32;\n"));
    }
    wit.push('}');
    std::fs::write(root.path().join("localhints.sus"), source).unwrap();
    std::fs::write(root.path().join("localhints.wit"), wit).unwrap();
    let mut compile = Command::new(env!("CARGO_BIN_EXE_suss"));
    compile.current_dir(root.path()).args(["compile", "localhints.sus", "-w", "localhints.wit", "-o", "localhints.wasm"]);
    for index in 0..CASES.len() {
        compile.args(["--export", &format!("case{index}=localhints/case{index}")]);
    }
    let output = compile.output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    for (index, (expression, _)) in CASES.iter().enumerate() {
        let output = Command::new(env!("CARGO_BIN_EXE_suss")).current_dir(root.path())
            .args(["run", "localhints.wasm", "--invoke", &format!("case{index}"), "--", "0"])
            .output().unwrap();
        assert!(output.status.success(), "{expression}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "1", "{expression}");
    }
}

const DECLARATION_ERRORS: &[(&str, &str)] = &[
    ("(let [x 5.5] ((fn ^long [] x)))", "Mismatched primitive return, expected: long, had: double"),
    ("(let [x 1] ((fn ^double [] x)))", "Mismatched primitive return, expected: double, had: long"),
    ("(let [^long x (identity 5.5)] ((fn ^double [] (+ x 1))))", "Mismatched primitive return, expected: double, had: long"),
    ("(let [^double x (identity 5)] ((fn ^long [] (+ x 1))))", "Mismatched primitive return, expected: long, had: double"),
    ("(loop [x 1.0 n 0] (let [f (fn ^long [] x)] (if (zero? n) (recur (identity x) 1) (f))))", "Mismatched primitive return, expected: long, had: double"),
    ("((fn [^long x] (if (zero? x) (recur (identity 1)) x)) 0)", "recur arg for primitive local: x is not matching primitive, had: Object, needed: long"),
    ("((fn [^double x] (if (zero? x) (recur 1) x)) 0.0)", "recur arg for primitive local: x is not matching primitive, had: long, needed: double"),
    ("(let [^long y (identity 1)] ((fn [^long x] (if (zero? x) (recur y) x)) 0))", "recur arg for primitive local: x is not matching primitive, had: long, needed: long"),
    ("(let [x (byte 1)] ((fn ^long [] x)))", "Mismatched primitive return, expected: long, had: byte"),
    ("((fn [^long x] (if (zero? x) (recur (byte 1)) x)) 0)", "recur arg for primitive local: x is not matching primitive, had: byte, needed: long"),
    ("(let [x 1.0] ((fn f ^long [] x)))", "Mismatched primitive return, expected: long, had: double"),
    ("((fn [^double x] ((fn ^long [] x))) 1.0)", "Mismatched primitive return, expected: long, had: double"),
    ("((fn [^long x] ((fn ^double [] x))) 1)", "Mismatched primitive return, expected: double, had: long"),
    ("(let [^double y (identity 1)] ((fn [^double x] (if (zero? x) (recur y) x)) 0.0))", "recur arg for primitive local: x is not matching primitive, had: double, needed: double"),
    ("(let [x 1.0] ((fn f (^long [] x) (^double [n] x))))", "Mismatched primitive return, expected: long, had: double"),
];

#[test]
fn primitive_method_diagnostics_use_lexical_source_types() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for (source, expected) in DECLARATION_ERRORS {
            let error = session.eval(source).err().unwrap_or_else(|| panic!("accepted {source}"));
            assert!(error.to_string().contains(expected), "{source}: {error}");
        }
    }
}

#[test]
fn aot_rejects_invalid_primitive_method_types() {
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("invalid.wit"),
        "package test:invalid-hints; world w { export check: func() -> u32; }").unwrap();
    for (source, expected) in DECLARATION_ERRORS {
        std::fs::write(root.path().join("invalid.sus"),
            format!("(ns invalid) (def check (fn [] {source} 1))")).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_suss")).current_dir(root.path())
            .args(["compile", "invalid.sus", "-w", "invalid.wit", "-o", "invalid.wasm", "--export", "check=invalid/check"])
            .output().unwrap();
        assert!(!output.status.success(), "accepted {source}");
        assert!(String::from_utf8_lossy(&output.stderr).contains(expected), "{source}: {}", String::from_utf8_lossy(&output.stderr));
    }
}
