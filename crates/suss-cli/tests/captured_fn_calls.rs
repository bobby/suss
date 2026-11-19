//! A var holding a fn that captures a let-bound fn (#460) is called through
//! its var, not as a direct link, which only a capture-free fn can take: each
//! arity, a variadic fn, a wrong arity, recursion, captures two levels deep
//! and redefinition answer as Clojure's do, in the REPL and in an AOT build,
//! which direct-links. Named and multi-arity fns still direct-link.
use std::process::Command;
use suss_cli::portable_session::Session;

const DEFINITIONS: &str = "(def w0 (let [g (fn [] 10)] (fn [] (g))))
(def w1 (let [g (fn [a] (+ a 1))] (fn [x] (g x))))
(def w2 (let [g (fn [a b] (+ a b))] (fn [x] (g x x))))
(def w3 (let [g (fn [a b c] (+ a b c))] (fn [x] (g x x x))))
(def wv (let [g (fn [a & r] (+ a (count r)))] (fn [x] (+ (g x) (g x 1 2)))))
(def wbad (let [g (fn [a] a)] (fn [x] (try (g x x) (catch :default e -1)))))
(def wrec (let [g (fn count-down [n] (if (pos? n) (count-down (dec n)) 99))] (fn [x] (g x))))
(def wdeep (let [g (fn [a] (* 2 a))] (fn [x] ((fn [y] ((fn [z] (g z)) y)) x))))
(def wmulti (let [g (fn ([] 0) ([a] a) ([a b] (+ a b)))] (fn [x] (+ (g) (g x) (g x x)))))
(def named (fn self [n] (if (pos? n) (self (dec n)) 5)))
(def multi (fn ([] 0) ([a] a)))";
/// Case, source, Clojure's answer.
const CASES: &[(&str, &str, i64)] = &[
    ("w0", "(w0)", 10),
    ("w1", "(w1 7)", 8),
    ("w2", "(w2 7)", 14),
    ("w3", "(w3 7)", 21),
    ("wv", "(wv 1)", 4),
    ("wbad", "(wbad 1)", -1),
    ("wrec", "(wrec 5)", 99),
    ("wdeep", "(wdeep 21)", 42),
    ("wmulti", "(wmulti 3)", 9),
    ("loop", "(loop [i 0 acc 0] (if (< i 100) (recur (inc i) (+ acc (w1 i))) acc))", 5050),
    ("named", "(+ (named 3) (multi) (multi 4))", 9),
];

fn number(session: &mut Session, source: &str) -> i64 {
    let value = session.eval(&format!("(str {source})")).unwrap_or_else(|error| panic!("{source}: {error}"));
    let units = session
        .inspect(&value, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(array.elems(&mut store)?.map(|unit| unit.unwrap_i32() as u16).collect::<Vec<_>>())
        })
        .unwrap();
    String::from_utf16(&units).unwrap().parse().unwrap()
}

#[test]
fn captured_fns_answer_in_the_repl() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval(DEFINITIONS).unwrap();
        for (_, source, expected) in CASES {
            assert_eq!(number(&mut session, source), *expected, "{source}");
        }
        session.eval("(def w1 (let [g (fn [a] (- a 1))] (fn [x] (g x))))").unwrap();
        assert_eq!(number(&mut session, "(w1 7)"), 6);
    }
}

#[test]
fn captured_fns_answer_in_a_direct_linked_build() {
    let root = tempfile::tempdir().unwrap();
    let mut app = format!("(ns app)\n{DEFINITIONS}\n");
    for (name, source, _) in CASES {
        app.push_str(&format!("(defn case-{name} [n] {source})\n"));
    }
    std::fs::write(root.path().join("app.sus"), app).unwrap();
    let exports = CASES.iter().map(|(name, ..)| format!("export case-{name}: func(n: u32) -> s64;")).collect::<Vec<_>>();
    std::fs::write(root.path().join("app.wit"), format!("package test:cf; world w {{ {} }}", exports.join(" "))).unwrap();
    let mut compile = Command::new(env!("CARGO_BIN_EXE_suss"));
    compile.current_dir(root.path()).args(["compile", "app.sus", "-w", "app.wit", "-o", "app.wasm", "--src", "."]);
    for (name, ..) in CASES {
        compile.args(["--export", &format!("case-{name}=app/case-{name}")]);
    }
    let output = compile.output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    for (name, source, expected) in CASES {
        let output = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args(["run", "app.wasm", "--invoke", &format!("case-{name}"), "--", "0"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{source}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), expected.to_string(), "{source}");
    }
}
