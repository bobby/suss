//! Cached numeric boxes survive branches and refresh on dynamic definitions (#337/#351).
use suss_cli::portable_session::Session;

const DEFINITIONS: &str = "";

// All four cases are checked against the pinned Clojure1.12.6 primary.
// Distinct Object uses of a primitive double receive distinct boxes; backend
// storage reuse cannot merge those source Objects. See the retained reference
// under results/m12-number-source-objects/box-reuse-primary.
const CASES: &[(&str, &str)] = &[
    ("(let [x (* -0.0 1.0) y (+ 3000000000 5) f (fn [v] v)] (loop [i 0 out []] (if (< i 4) (let [a (f x) b (if (zero? (bit-and i 1)) (f y) (identity y))] (recur (inc i) (conj out [a b]))) [x y out])))", "[-0.0 3000000005 [[-0.0 3000000005] [-0.0 3000000005] [-0.0 3000000005] [-0.0 3000000005]]]"),
    ("(let [f (fn [v] v)] (loop [i 0 d 1.5 w 3000000000 out []] (if (< i 4) (let [a (f d) b (f w) a2 (if (zero? (bit-and i 1)) (f d) (identity d)) b2 (f w)] (recur (inc i) (+ d 0.5) (+ w 2) (conj out [a b a2 b2]))) out)))", "[[1.5 3000000000 1.5 3000000000] [2.0 3000000002 2.0 3000000002] [2.5 3000000004 2.5 3000000004] [3.0 3000000006 3.0 3000000006]]"),
    ("(let [f (fn [v] v) x (+ 1.25 0.5)] [(if true (f x) :unused) (if false :unused (f x)) (f x) (identical? (f x) (f x))])", "[1.75 1.75 1.75 false]"),
    ("(let [x (+ ##NaN 0.0) f (fn [v] v) same (identity identical?) a (if (identity true) (f x) nil) b (if (identity false) nil (f x))] [(= a b) (== a b) (identical? a b) (same a b) (count (hash-set a b)) (get {a 1} b :miss)])", "[false false false false 2 :miss]"),
];

fn observed(source: &str) -> String {
    format!("(pr-str (try {source} (catch :default e [:hint-error (ex-message e)])))")
}

#[test]
fn runtime_and_macro_preserve_numeric_box_reuse() {
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
fn aot_preserves_numeric_box_reuse() {
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    let mut source = format!("(ns boxreuse)\n{DEFINITIONS}\n");
    let mut wit = String::from("package test:boxreuse; world w {\n");
    for (index, (expression, expected)) in CASES.iter().enumerate() {
        // Only the unused WIT argument is synthesized as a primitive. The
        // numeric values come from the inner source expressions.
        source.push_str(&format!("(def case{index} (fn [unused] (if (= {} {expected:?}) 1 0)))\n", observed(expression)));
        wit.push_str(&format!("export case{index}: func(unused: u32) -> u32;\n"));
    }
    wit.push('}');
    std::fs::write(root.path().join("boxreuse.sus"), source).unwrap();
    std::fs::write(root.path().join("boxreuse.wit"), wit).unwrap();
    let mut compile = Command::new(env!("CARGO_BIN_EXE_suss"));
    compile.current_dir(root.path()).args(["compile", "boxreuse.sus", "-w", "boxreuse.wit", "-o", "boxreuse.wasm"]);
    for index in 0..CASES.len() {
        compile.args(["--export", &format!("case{index}=boxreuse/case{index}")]);
    }
    let output = compile.output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    for (index, (expression, _)) in CASES.iter().enumerate() {
        let output = Command::new(env!("CARGO_BIN_EXE_suss")).current_dir(root.path())
            .args(["run", "boxreuse.wasm", "--invoke", &format!("case{index}"), "--", "0"])
            .output().unwrap();
        assert!(output.status.success(), "{expression}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "1", "{expression}");
    }
}
