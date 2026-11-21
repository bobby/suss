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
