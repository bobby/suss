//! Source numeric overloads, independently of the backend's boxing (#484).
use suss_cli::portable_session::Session;

const DEFINITIONS: &str = "
  (def generic-div (fn [x y] (/ x y)))
  (def primitive-div (fn [^double x ^double y] (/ x y)))";

// JVM Clojure 1.12.6 observations. A literal or hinted double selects a
// primitive overload even when its other operand comes from an Object call.
const CASES: &[(&str, &str)] = &[
    ("(generic-div 1.0 0.0)", "[:division-error \"Divide by zero\"]"),
    ("(generic-div -1.0 -0.0)", "[:division-error \"Divide by zero\"]"),
    ("(generic-div 0.0 0.0)", "[:division-error \"Divide by zero\"]"),
    ("(generic-div ##NaN 0.0)", "##NaN"),
    ("(generic-div ##Inf -0.0)", "[:division-error \"Divide by zero\"]"),
    ("(primitive-div 1.0 0.0)", "##Inf"),
    ("(primitive-div -1.0 0.0)", "##-Inf"),
    ("(primitive-div -1.0 -0.0)", "##Inf"),
    ("(primitive-div 0.0 0.0)", "##NaN"),
    ("(primitive-div ##NaN 0.0)", "##NaN"),
    ("(/ 1.0 (identity 0.0))", "##Inf"),
    ("(/ (identity 1.0) 0.0)", "##Inf"),
    ("(let [x (identity 1.0) y (identity 0.0)] (/ x y))", "[:division-error \"Divide by zero\"]"),
    ("(let [x (if true 1.0 2.0) y 0.0] (/ x y))", "##Inf"),
    ("(generic-div 1.0 2.0)", "0.5"),
    ("(primitive-div 1.0 2.0)", "0.5"),
    ("(generic-div -0.0 1.0)", "-0.0"),
    ("(generic-div ##NaN 1.0)", "##NaN"),
    ("(generic-div ##Inf 2.0)", "##Inf"),
    ("(/ 0.0)", "[:division-error \"Divide by zero\"]"),
    ("(/ 1.0 0.0 1.0)", "##Inf"),
    ("(/ 1.0 2.0 0.0)", "##Inf"),
    ("(/ 1 (identity 0.0))", "[:division-error \"Divide by zero\"]"),
    ("(let [x 1.0 f (fn [y] (/ x y))] (f 0.0))", "##Inf"),
    ("(let [x (try 1.0) y (try 0.0)] (/ x y))", "[:division-error \"Divide by zero\"]"),
    ("(let [events (atom [])] [(try (/ (do (swap! events conj :a) (identity 1.0)) (do (swap! events conj :b) (identity 0.0))) (catch :default e :division-error)) @events])", "[:division-error [:a :b]]"),
    ("(let [events (atom [])] [(try (/ (do (swap! events conj :a) 1) (do (swap! events conj :b) 0) (do (swap! events conj :c) 1.0)) (catch :default e :division-error)) @events])", "[:division-error [:a :b]]"),
    ("(do (def active-div (fn [x y] (/ x y))) (let [old active-div] (def active-div (fn [x y] (/ (double x) y))) [(try (old 1.0 0.0) (catch :default e :division-error)) (active-div 1.0 0.0)]))", "[:division-error ##Inf]"),
    ("(loop [x 1.0 n 0] (if (zero? n) (recur (identity x) 1) (/ x (identity 0.0))))", "[:division-error \"Divide by zero\"]"),
    ("(loop [x 1.0 n 0] (if (zero? n) (recur 1 (inc n)) (/ x (identity 0.0))))", "[:division-error \"Divide by zero\"]"),
    ("(loop [x 1.0 n 0] (if (zero? n) (/ x (identity 0.0)) (recur 1 (inc n))))", "[:division-error \"Divide by zero\"]"),
    ("(loop [x 1.0 n 0] (let [alias x] (if (zero? n) (recur (identity x) 1) (/ alias (identity 0.0)))))", "[:division-error \"Divide by zero\"]"),
    ("(loop [x 1.0 n 0] (let [f (fn [y] (/ x y))] (if (zero? n) (recur (identity x) 1) (f 0.0))))", "[:division-error \"Divide by zero\"]"),
    ("(loop [x 1.0 n 0] (if (zero? n) (recur (identity x) 1) (loop [alias x] (/ alias (identity 0.0)))))", "[:division-error \"Divide by zero\"]"),
    ("(loop [x 1.0 n 0] (if (zero? n) (recur 2.0 1) (/ x (identity 0.0))))", "##Inf"),
    ("(loop [x (float 1.0) n 0] (if (zero? n) (recur (float 2.0) 1) (/ x (identity 0.0))))", "##Inf"),
    ("(loop [x 1.0 y (/ x (identity 0.0)) n 0] (if (zero? n) (recur (identity x) y 1) y))", "[:division-error \"Divide by zero\"]"),
    ("(loop [x 1.0 y (/ x (identity 0.0)) n 0] (if (zero? n) (recur 2.0 y 1) y))", "##Inf"),
];

fn observed(source: &str) -> String {
    format!("(pr-str (try {source} (catch :default e [:division-error (ex-message e)])))")
}

#[test]
fn runtime_and_macro_preserve_source_division_overloads() {
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
fn aot_wrappers_keep_the_inner_helpers_hinted_or_generic() {
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    let mut source = format!("(ns division)\n{DEFINITIONS}\n");
    let mut wit = String::from("package test:division; world w {\n");
    for (index, (expression, expected)) in CASES.iter().enumerate() {
        // Only the unused WIT argument is synthesized as a primitive. The
        // two helper signatures above retain their genuine source hints.
        source.push_str(&format!("(def case{index} (fn [unused] (if (= {} {expected:?}) 1 0)))\n", observed(expression)));
        wit.push_str(&format!("export case{index}: func(unused: u32) -> u32;\n"));
    }
    wit.push('}');
    std::fs::write(root.path().join("division.sus"), source).unwrap();
    std::fs::write(root.path().join("division.wit"), wit).unwrap();
    let mut compile = Command::new(env!("CARGO_BIN_EXE_suss"));
    compile.current_dir(root.path()).args(["compile", "division.sus", "-w", "division.wit", "-o", "division.wasm"]);
    for index in 0..CASES.len() {
        compile.args(["--export", &format!("case{index}=division/case{index}")]);
    }
    let output = compile.output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    for (index, (expression, _)) in CASES.iter().enumerate() {
        let output = Command::new(env!("CARGO_BIN_EXE_suss")).current_dir(root.path())
            .args(["run", "division.wasm", "--invoke", &format!("case{index}"), "--", "0"])
            .output().unwrap();
        assert!(output.status.success(), "{expression}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "1", "{expression}");
    }
}
