//! `^long` and `^double` function parameters are Clojure's checked longCast
//! and doubleCast of the argument, kept raw in the body (#385): arithmetic and
//! loops seeded from them stay unboxed beyond i31. Each case's AOT result must
//! equal the REPL's, and fixed expected values. Operands derive from the
//! argument `n`, 0.
use std::process::Command;
use suss_cli::portable_session::Session;

const DEFINITIONS: &str = "(def safe (fn [f] (try (f) (catch :default e :error))))
(def ident (fn [^long x] x))
(def dbl (fn [^double x] x))
(def count-from (fn [^long start ^long steps] (loop [i start k 0] (if (< k steps) (recur (inc i) (inc k)) i))))
(def sum-to (fn [^long n] (loop [i 0 acc 0] (if (< i n) (recur (inc i) (+ acc i)) acc))))
(def down (fn [^long n acc] (if (zero? n) acc (recur (dec n) (conj acc n)))))
(def arities (fn ([^long a] [:one (inc a)]) ([^long a ^double b] [:two (+ a b)])))
(def variadic (fn [^long a & more] [(inc a) more]))
(def mixed (fn [^long a {:keys [b]}] (+ a b)))
(def capture (fn [^long a] (fn [] (+ a 1))))
(def ret (fn ^long [^long a] (* a 2)))";
const CASES: &[(&str, &str)] = &[
    ("casts", "[(ident (+ n 5)) (ident 3000000000) (ident 2.7) (ident -2.7) (ident 9223372036854775807) (dbl (+ n 3)) (dbl 1.5) (dbl 3000000000)]"),
    ("errors", "[(safe (fn [] (ident :k))) (safe (fn [] (ident nil))) (safe (fn [] (ident 1.0e30))) (safe (fn [] (dbl \"x\")))]"),
    ("loops", "[(count-from (+ n 3000000000) 5) (count-from -1073741826 3) (sum-to (+ n 100000)) (safe (fn [] (count-from 9223372036854775806 3)))]"),
    ("recur", "(down (+ n 4) [])"),
    ("arities", "[(arities (+ n 1)) (arities 2 0.5) (variadic (+ n 1) 2 3) (variadic 0) (mixed 2 {:b 3})]"),
    ("closures", "[((capture (+ n 4))) ((capture 3000000000)) (ret (+ n 21))]"),
];

#[test]
fn primitive_parameter_hints_match_the_repl() {
    let root = tempfile::tempdir().unwrap();
    let mut app = format!("(ns app)\n{DEFINITIONS}\n");
    for (name, source) in CASES {
        app.push_str(&format!("(defn case-{name} [n] (pr-str {source}))\n"));
    }
    std::fs::write(root.path().join("app.sus"), &app).unwrap();
    let exports = CASES.iter().map(|(name, _)| format!("export case-{name}: func(n: u32) -> string;")).collect::<Vec<_>>();
    std::fs::write(root.path().join("app.wit"), format!("package test:prim; world w {{ {} }}", exports.join(" "))).unwrap();
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
            "casts" => Some("[5 3000000000 2 -2 9223372036854775807 3.0 1.5 3.0E9]"),
            "errors" => Some("[:error :error :error :error]"),
            "loops" => Some("[3000000005 -1073741823 4999950000 :error]"),
            "recur" => Some("[4 3 2 1]"),
            "arities" => Some("[[:one 2] [:two 2.5] [2 (2 3)] [1 nil] 5]"),
            "closures" => Some("[5 3000000001 42]"),
            _ => None,
        };
        if let Some(expected) = expected {
            assert_eq!(aot, expected, "{name}: {source}");
        }
    }
}
