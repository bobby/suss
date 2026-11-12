//! `(= x y)` and `(hash x)` test for longs in i31 range inline in AOT builds
//! (#387): two i31s are `=` exactly when identical, and a number's hash is
//! `number-hash`. Every other operand calls core. Each case's AOT result must
//! equal the REPL's, which calls core's `=` and `hash` through their live vars.
//! Operands derive from the argument `n`, 0, so nothing folds at compile time.
use std::process::Command;
use suss_cli::portable_session::Session;

const DEFINITIONS: &str = "(deftype Always [] IEquiv (-equiv [_ _] true))
(deftype Hashed [h] IHash (-hash [_] h))";
/// Case name and an expression of `n` whose value is a long.
const CASES: &[(&str, &str)] = &[
    // Both i31.
    ("eq-same", "(if (= (+ n 1) 1) 1 0)"),
    ("eq-different", "(if (= (+ n 1) 2) 1 0)"),
    ("eq-negative", "(if (= (- n 5) -5) 1 0)"),
    ("eq-i31-max", "(if (= (+ n 1073741823) 1073741823) 1 0)"),
    ("eq-i31-min", "(if (= (- n 1073741824) -1073741824) 1 0)"),
    // Boxed longs, doubles and mixed numbers take core's path.
    ("eq-boxed", "(if (= (+ n 1073741824) 1073741824) 1 0)"),
    ("eq-boxed-large", "(if (= (+ n 3000000000) (* 3 1000000000)) 1 0)"),
    ("eq-long-double", "(if (= (+ n 1) 1.0) 1 0)"),
    ("eq-double", "(if (= (+ n 0.5) 0.5) 1 0)"),
    ("eq-nan", "(if (= (/ (+ n 0.0) 0.0) (/ (+ n 0.0) 0.0)) 1 0)"),
    ("eq-zero-negative-zero", "(if (= (* n 0.0) (* n -0.0)) 1 0)"),
    // Non-numbers.
    ("eq-nil-long", "(if (= (when (pos? n) 1) 0) 1 0)"),
    ("eq-nil-nil", "(if (= (when (pos? n) 1) nil) 1 0)"),
    ("eq-long-string", "(if (= (+ n 1) (str 1)) 1 0)"),
    ("eq-long-vector", "(if (= (+ n 1) [1]) 1 0)"),
    ("eq-vectors", "(if (= [(+ n 1) 2] [1 2]) 1 0)"),
    ("eq-strings", "(if (= (str \"a\" n) \"a0\") 1 0)"),
    ("eq-keywords", "(if (= (keyword (str \"k\" n)) :k0) 1 0)"),
    ("eq-user-equiv", "(if (= (Always.) (+ n 1)) 1 0)"),
    ("eq-long-user-equiv", "(if (= (+ n 1) (Always.)) 1 0)"),
    // Order of evaluation and single evaluation of each operand.
    ("eq-effects", "(let [a (atom [])] (= (do (swap! a conj 1) (+ n 1)) (do (swap! a conj 2) 1)) (+ (* 10 (first @a)) (second @a)))"),
    // hash of i31s, boxed longs and other values.
    ("hash-zero", "(hash (+ n 0))"),
    ("hash-one", "(hash (+ n 1))"),
    ("hash-negative", "(hash (- n 1))"),
    ("hash-i31-max", "(hash (+ n 1073741823))"),
    ("hash-i31-min", "(hash (- n 1073741824))"),
    ("hash-boxed", "(hash (+ n 1073741824))"),
    ("hash-boxed-large", "(hash (+ n 3000000000))"),
    ("hash-double", "(hash (+ n 1.5))"),
    ("hash-whole-double", "(hash (+ n 2.0))"),
    ("hash-nil", "(hash (when (pos? n) 1))"),
    ("hash-keyword", "(hash (keyword (str \"k\" n)))"),
    ("hash-string", "(hash (str \"a\" n))"),
    ("hash-vector", "(hash [(+ n 1) 2])"),
    ("hash-user", "(hash (Hashed. (+ n 42)))"),
    ("hash-effects", "(let [a (atom 0)] (hash (do (swap! a inc) (+ n 1))) @a)"),
];

#[test]
fn inline_equiv_and_hash_match_core() {
    let root = tempfile::tempdir().unwrap();
    let mut app = format!("(ns app)\n{DEFINITIONS}\n");
    for (name, source) in CASES {
        app.push_str(&format!("(defn case-{name} [n] {source})\n"));
    }
    std::fs::write(root.path().join("app.sus"), &app).unwrap();
    let exports = CASES.iter().map(|(name, _)| format!("export case-{name}: func(n: u32) -> s64;")).collect::<Vec<_>>();
    std::fs::write(root.path().join("app.wit"), format!("package test:eq; world w {{ {} }}", exports.join(" "))).unwrap();
    let mut compile = Command::new(env!("CARGO_BIN_EXE_suss"));
    compile.current_dir(root.path()).args(["compile", "app.sus", "-w", "app.wit", "-o", "app.wasm"]);
    for (name, _) in CASES {
        compile.args(["--export", &format!("case-{name}=app/case-{name}")]);
    }
    let output = compile.output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let mut repl = Session::new_repl().unwrap();
    repl.set_operation_fuel(100_000_000);
    repl.eval(DEFINITIONS).unwrap();
    for (name, source) in CASES {
        let output = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args(["run", "app.wasm", "--invoke", &format!("case-{name}"), "--", "0"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{source}: {}", String::from_utf8_lossy(&output.stderr));
        let aot = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let value = repl.eval(&format!("(str ((fn [n] {source}) 0))")).unwrap_or_else(|error| panic!("{source}: {error}"));
        let live = repl
            .inspect(&value, |mut store, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                Ok(array.elems(&mut store)?.map(|unit| unit.unwrap_i32() as u16).collect::<Vec<_>>())
            })
            .unwrap();
        assert_eq!(aot, String::from_utf16(&live).unwrap(), "{name}: {source}");
    }
}
