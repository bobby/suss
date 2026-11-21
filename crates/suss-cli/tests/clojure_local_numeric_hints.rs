//! Local numeric hints preserve objects and convert at primitive uses (#337).
use suss_cli::portable_session::Session;

const DEFINITIONS: &str = "(def receive-double (fn [^double x] x))";

// Fresh JVM Clojure 1.12.6 observations; the source strings retain metadata.
const CASES: &[(&str, &str)] = &[
    ("(let [^long x (identity 5.5)] [x (+ x 1) (inc x) (dec x) (- x) (+ x) (* x)])", "[5.5 6 6 4 -5 5.5 5.5]"),
    ("(let [^double x (identity 5)] [x (+ x 1) (inc x) (dec x)])", "[5 6.0 6.0 4.0]"),
    ("(let [^long x (identity 0.5)] [(zero? x) (zero? (identity x)) (== x 0.5) (= x 0.5) (< x 0.5)])", "[true false false true true]"),
    ("(let [^long x (identity 5.5)] [(bit-and x 1) (quot x 2) (rem x 2) (mod x 2) (abs x)])", "[1 2 1 1.5 5]"),
    ("(let [^long x (identity 5.5)] [(double x) (float x) (int x)])", "[5.0 5.0 5]"),
    ("(let [^long x (identity 5.5)] (receive-double x))", "5.5"),
    ("(let [^long x (identity 5.5) f receive-double] (f x))", "5.5"),
    ("(let [^long x (identity 5.5)] ((fn [^double y] y) x))", "5.5"),
    ("(let [^long x (identity 5.5)] [(/ x 1) (/ x)])", "[5 0.18181818181818182]"),
    ("(loop [^long x (identity 5.5) n 0] (if (zero? n) (recur (identity 7.5) 1) [x (+ x 1)]))", "[7.5 8]"),
    ("(let [^long x (identity 5.5) f (fn [] [x (+ x 1)])] (f))", "[5.5 6]"),
    ("(let [^long x (identity 5.5) y x] [x y (+ y 1)])", "[5.5 5.5 6]"),
    ("(let [^long x (identity 5.5)] [(if true x 1) (do x) x])", "[5.5 5.5 5.5]"),
    ("(let [^long x (identity 5.5) y (if true x 1)] [x y])", "[5.5 5.5]"),
    ("(let [^long x (identity 5.5)] ((fn ^double [] x)))", "5.5"),
    ("(let [^long x (identity 5.5) y (if true x 1)] [x y (+ y 1)])", "[5.5 5.5 6]"),
    ("(let [^long x (identity 5.5) y (do x)] [x y (+ y 1)])", "[5.5 5.5 6]"),
    ("(let [x (identity 5.5)] [x (+ ^long x 1) x])", "[5.5 6 5.5]"),
    ("(let [^long x (identity 5.5)] [(+ ^double x 1) x])", "[6.5 5.5]"),
    ("(let [^double x (identity 5)] [(quot x 2) (rem x 2) (mod x 2) (abs x)])", "[2.0 1.0 1 5.0]"),
    ("(let [^long x (identity 5.5)] [(long x) (unchecked-long x) (byte x) (short x)])", "[5 5 5 5]"),
    ("(let [^long x (identity 5.5) y (if true x 1.0)] [y (+ y 1)])", "[5.5 6.5]"),
    ("(let [^long x (identity 1.0E30) events (atom [])] [(try (+ x 1 (do (swap! events conj :later) 1)) (catch :default e :cast-error)) @events])", "[:cast-error []]"),
    ("(let [^long x (identity 1.0E30) events (atom [])] [(try (< x 1 (do (swap! events conj :later) 1)) (catch :default e :cast-error)) @events])", "[false [:later]]"),
    ("(let [^double x (identity -0.0)] [x (+ x 0) (/ 1 x)])", "[-0.0 0.0 ##-Inf]"),
    ("(let [^double x (identity ##NaN)] [(double? (+ x 1)) (= x x) (zero? x)])", "[true false false]"),
    ("(let [y (loop [^long x (identity 5.5)] x)] [y (+ y 1)])", "[5.5 6.5]"),
    ("(let [x 1] ^long x)", "1"),
    ("(let [^long x (loop [i 0] (if (< i 1) (recur (inc i)) 5.5))] [x (+ x 1)])", "[5.5 6]"),
    ("(let [^double x (identity 1)] (/ x (identity 0)))", "##Inf"),
    ("(let [^long x (identity 1.0)] (try (/ x (identity 0.0)) (catch :default e :zero-error)))", ":zero-error"),
];

fn observed(source: &str) -> String {
    format!("(pr-str (try {source} (catch :default e [:hint-error (ex-message e)])))")
}

#[test]
fn runtime_and_macro_preserve_local_hint_semantics() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval(DEFINITIONS).unwrap();
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
fn aot_preserves_local_hint_semantics() {
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    let mut source = format!("(ns localhints)\n{DEFINITIONS}\n");
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
    ("(let [^long x 1] (+ x 1))", "Can't type hint a local with a primitive initializer"),
    ("(let [^double x 1.0] (+ x 1))", "Can't type hint a local with a primitive initializer"),
    ("(let [x (double 1) ^long y x] y)", "Can't type hint a local with a primitive initializer"),
    ("(let [x 1] ^double x)", "Can't type hint a primitive local with a different type"),
    ("(loop [x 1.0 n 0] (if (zero? n) (recur (identity x) 1) (let [^long y x] y)))", "Can't type hint a local with a primitive initializer"),
    ("(loop [x 1.0 n 0] (let [f (fn [] (let [^long y x] y))] (if (zero? n) (recur (identity x) 1) (f))))", "Can't type hint a local with a primitive initializer"),
    ("(loop [x 1.0 n 0] (let [f (fn [] ^long x)] (if (zero? n) (recur (identity x) 1) (f))))", "Can't type hint a primitive local with a different type"),
];

#[test]
fn hint_diagnostics_use_initial_source_types_before_loop_reanalysis() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for (source, expected) in DECLARATION_ERRORS {
            let error = session.eval(source).err().unwrap_or_else(|| panic!("accepted {source}"));
            assert!(error.to_string().contains(expected), "{source}: {error}");
        }
    }
}

#[test]
fn aot_rejects_invalid_local_hint_declarations() {
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
