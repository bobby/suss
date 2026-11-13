//! Keyword literals read a fragment's cache of their interned keyword after
//! the first read, `(= :k x)` compiles to a reference test, and nil and
//! boolean nominal tests keep raw truth values that only boxed uses turn into
//! `true` or `false` (#387). Each case's AOT result must equal the REPL's,
//! and some fixed expected values. Operands derive from the argument `n`, 0.
use std::process::Command;
use suss_cli::portable_session::Session;

const DEFINITIONS: &str = "(def kw-a (fn [] :a))
(def kw-a2 (fn [] :a))
(def kw-q (fn [] :other.ns/k))
(def kw-auto (fn [] ::here))
(def is-a (fn [x] (= :a x)))
(def nil-flag (fn [x] (let [b (nil? x)] [b (if b :yes :no) (not b)])))
(def kws (fn [n] (loop [i 0 acc []] (if (< i 3) (recur (inc i) (conj acc :loop)) acc))))";
const CASES: &[(&str, &str)] = &[
    ("same-keyword", "[(identical? (kw-a) (kw-a2)) (identical? (kw-a) :a) (identical? (kw-a) (keyword \"a\")) (= (kw-q) :other.ns/k)]"),
    ("repeat", "[(kw-a) (kw-a) (kw-q) (kw-q) (namespace (kw-q)) (name (kw-a))]"),
    ("auto", "[(name (kw-auto)) (identical? (kw-auto) (keyword (namespace (kw-auto)) \"here\")) (identical? (kw-auto) (kw-auto))]"),
    ("loop", "(kws n)"),
    ("keyword-equals", "(mapv is-a [:a :b (keyword \"a\") \"a\" 'a nil (+ n 1) 1.5 :x/a])"),
    ("literal-equals", "[(= :a :a) (= :a :b) (= :a (kw-a)) (= :x/a :a)]"),
    ("nil-flags", "[(nil-flag nil) (nil-flag n) (nil-flag false)]"),
    ("flag-values", "(let [x (if (= n 0) nil 1) t (nil? x) s (some? x)] [t s (= t true) (str t) (hash t) (if t 1 2) (when s :s)])"),
    ("map-keys", "(let [m {:a (+ n 1) :b 2}] [(get m :a) (:b m) (m :a) (contains? m :c) (keys (assoc m :c 3))])"),
];

#[test]
fn keyword_literals_and_test_flags_match_the_repl() {
    let root = tempfile::tempdir().unwrap();
    let mut app = format!("(ns app)\n{DEFINITIONS}\n");
    for (name, source) in CASES {
        app.push_str(&format!("(defn case-{name} [n] (pr-str {source}))\n"));
    }
    std::fs::write(root.path().join("app.sus"), &app).unwrap();
    let exports = CASES.iter().map(|(name, _)| format!("export case-{name}: func(n: u32) -> string;")).collect::<Vec<_>>();
    std::fs::write(root.path().join("app.wit"), format!("package test:kw; world w {{ {} }}", exports.join(" "))).unwrap();
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
            "same-keyword" => Some("[true true true true]"),
            "keyword-equals" => Some("[true false true false false false false false false]"),
            "literal-equals" => Some("[true false true false]"),
            "nil-flags" => Some("[[true :yes false] [false :no true] [false :no true]]"),
            "flag-values" => Some("[true false true \"true\" 1231 1 nil]"),
            _ => None,
        };
        if let Some(expected) = expected {
            assert_eq!(aot, expected, "{name}: {source}");
        }
    }
}
