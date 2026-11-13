//! PersistentVector's -nth asks the runtime's trie walk first (#378); a miss,
//! or a nil element, takes the pinned path. Each case's AOT result must equal
//! the REPL's, and fixed expected values. Operands derive from the argument
//! `n`, 0.
use std::process::Command;
use suss_cli::portable_session::Session;

const DEFINITIONS: &str = "(def safe (fn [f] (try (f) (catch :default e :error))))
(def every-index (fn [size] (let [v (vec (range size))] (every? (fn [k] (= k (nth v k))) (range size)))))
(def probe (fn [v ks] (mapv (fn [k] (nth v k :nf)) ks)))";
const CASES: &[(&str, &str)] = &[
    ("sizes", "(mapv every-index [1 31 32 33 64 1024 1056 1057 2048 33000])"),
    ("edges", "(probe (vec (range 33000)) [0 31 32 1023 1024 32767 32768 32999 33000 -1])"),
    ("nil-elements", "(let [v [nil 1 nil false]] [(nth v 0) (nth v 1) (nth v 2) (nth v 3) (nth v 0 :nf)])"),
    ("errors", "[(safe (fn [] (nth [1 2] 2))) (safe (fn [] (nth [1 2] -1))) (safe (fn [] (nth [] 0))) (nth [1 2] 5 :nf)]"),
    ("index-kinds", "[(nth [10 20 30] 1.0) (nth [10 20 30] (+ n 2)) (safe (fn [] (nth [10 20 30] nil)))]"),
    ("built", "(let [a (reduce conj [] (range 100)) b (assoc a 50 :x) c (persistent! (reduce conj! (transient []) (range 70)))] [(nth a 99) (nth b 50) (nth b 49) (nth c 69) (nth (subvec a 10 20) 3) (nth (vector-of :long 1 2 3) 2)])"),
    ("vector-fn", "[([:a :b :c] 1) (get [:a :b] 1) (get [:a :b] 5 :nf) (first (rseq [1 2 3]))]"),
];

#[test]
fn vector_nth_matches_the_repl() {
    let root = tempfile::tempdir().unwrap();
    let mut app = format!("(ns app)\n{DEFINITIONS}\n");
    for (name, source) in CASES {
        app.push_str(&format!("(defn case-{name} [n] (pr-str {source}))\n"));
    }
    std::fs::write(root.path().join("app.sus"), &app).unwrap();
    let exports = CASES.iter().map(|(name, _)| format!("export case-{name}: func(n: u32) -> string;")).collect::<Vec<_>>();
    std::fs::write(root.path().join("app.wit"), format!("package test:vnth; world w {{ {} }}", exports.join(" "))).unwrap();
    let mut compile = Command::new(env!("CARGO_BIN_EXE_suss"));
    compile.current_dir(root.path()).args(["compile", "app.sus", "-w", "app.wit", "-o", "app.wasm"]);
    for (name, _) in CASES {
        compile.args(["--export", &format!("case-{name}=app/case-{name}")]);
    }
    let output = compile.output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let mut repl = Session::new_repl().unwrap();
    repl.set_operation_fuel(200_000_000);
    repl.eval(DEFINITIONS).unwrap();
    for (name, source) in CASES {
        let output = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args(["run", "app.wasm", "--invoke", &format!("case-{name}"), "--", "0"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{source}: {}", String::from_utf8_lossy(&output.stderr));
        let aot = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let value = repl.eval(&format!("((fn [n] (pr-str {source})) 0)")).unwrap_or_else(|error| panic!("{source}: {error}"));
        let live = repl
            .inspect(&value, |mut store, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                Ok(array.elems(&mut store)?.map(|unit| unit.unwrap_i32() as u16).collect::<Vec<_>>())
            })
            .unwrap();
        let live = String::from_utf16(&live).unwrap();
        assert_eq!(aot, live, "{name}: {source}");
        // The expected values, independent of either implementation.
        let expected = match *name {
            "sizes" => Some("[true true true true true true true true true true]"),
            "edges" => Some("[0 31 32 1023 1024 32767 32768 32999 :nf :nf]"),
            "nil-elements" => Some("[nil 1 nil false nil]"),
            "built" => Some("[99 :x 49 69 13 3]"),
            _ => None,
        };
        if let Some(expected) = expected {
            assert_eq!(aot, expected, "{name}: {source}");
        }
    }
}
