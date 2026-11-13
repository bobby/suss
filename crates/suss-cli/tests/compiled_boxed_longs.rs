//! Longs outside i31 add, subtract and compare inline in Clojure arithmetic
//! (#385), and in the runtime's checked operations: an i31 or a boxed long
//! operand is read as an i64, overflow takes the runtime path and throws, and
//! any other operand converts as before. Each case's AOT result must equal
//! the REPL's, and some fixed expected values. `n` is 0; `big` is an operand
//! the compiler can't type, beyond i31.
use std::process::Command;
use suss_cli::portable_session::Session;

const DEFINITIONS: &str = "(def big (fn [n] (+ n 3000000000)))
(def add (fn [a b] (+ a b)))
(def sub (fn [a b] (- a b)))
(def lt (fn [a b] (< a b)))
(def ge (fn [a b] (>= a b)))
(def count-up (fn [from to] (loop [i from k 0] (if (< i to) (recur (inc i) (inc k)) [i k]))))
(def overflow (fn [f a b] (try (f a b) (catch :default e :overflow))))";
const CASES: &[(&str, &str)] = &[
    ("cross-up", "(count-up (- (big n) 3000000000 -1073741820) (- (big n) 3000000000 -1073741830))"),
    ("cross-down", "[(add -1073741824 -1) (sub -1073741824 1) (add (big n) -3000000000) (sub (big n) 3000000000)]"),
    ("boxed", "[(add (big n) (big n)) (sub (big n) 1) (add 1 (big n)) (sub (- (big n)) (big n))]"),
    ("loop", "(count-up (big n) (+ (big n) 5))"),
    ("overflow", "[(overflow add 9223372036854775807 1) (overflow sub -9223372036854775808 1) (overflow add (big n) 9223372036854775807) (add 9223372036854775806 1)]"),
    ("mixed", "[(add (big n) 0.5) (sub 0.5 (big n)) (lt (big n) 3.5e9) (ge (big n) 2.5e9) (lt (big n) ##NaN) (ge ##NaN (big n))]"),
    ("compare", "[(lt (big n) (+ (big n) 1)) (lt (+ (big n) 1) (big n)) (ge (big n) (big n)) (lt -9223372036854775808 (big n)) (< (big n) 5) (> (big n) 5) (<= (- (big n)) -3000000000) (= (big n) 3000000000)]"),
    ("non-number", "[(try (add (big n) \"x\") (catch :default e :not-number)) (try (lt nil (big n)) (catch :default e :not-number))]"),
];

#[test]
fn boxed_long_arithmetic_matches_the_repl() {
    let root = tempfile::tempdir().unwrap();
    let mut app = format!("(ns app)\n{DEFINITIONS}\n");
    for (name, source) in CASES {
        app.push_str(&format!("(defn case-{name} [n] (pr-str {source}))\n"));
    }
    std::fs::write(root.path().join("app.sus"), &app).unwrap();
    let exports = CASES.iter().map(|(name, _)| format!("export case-{name}: func(n: u32) -> string;")).collect::<Vec<_>>();
    std::fs::write(root.path().join("app.wit"), format!("package test:longs; world w {{ {} }}", exports.join(" "))).unwrap();
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
            "cross-up" => Some("[1073741830 10]"),
            "cross-down" => Some("[-1073741825 -1073741825 0 0]"),
            "boxed" => Some("[6000000000 2999999999 3000000001 -6000000000]"),
            "loop" => Some("[3000000005 5]"),
            "overflow" => Some("[:overflow :overflow :overflow 9223372036854775807]"),
            "compare" => Some("[true false true true false true true true]"),
            "non-number" => Some("[:not-number :not-number]"),
            _ => None,
        };
        if let Some(expected) = expected {
            assert_eq!(aot, expected, "{name}: {source}");
        }
    }
}
