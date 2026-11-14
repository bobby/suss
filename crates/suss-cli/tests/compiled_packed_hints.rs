//! An `aget` or `aset` with one index into an array tagged with a packed
//! element type, by a hint such as `^longs` or by a local bound to
//! `(long-array …)`, reads and writes the element inline (#389). Anything else,
//! including a wrong hint, takes the generic path with the same result. Each
//! case's AOT result must equal the REPL's, and some fixed expected values.
//! Operands derive from the argument `n`, 0.
use std::process::Command;
use suss_cli::portable_session::Session;

const DEFINITIONS: &str = "(def la (long-array [1 2 3000000000 -4]))
(def ia (int-array [5 6 7]))
(def da (double-array [1.5 2.5]))
(def ba (byte-array [1 -2 3]))
(def fa (float-array [0.5 1.25]))
(def oa (object-array [:x :y]))
(def safe (fn [f] (try (f) (catch :default e :error))))";
const CASES: &[(&str, &str)] = &[
    ("hinted-get", "[(aget ^longs la (+ n 1)) (aget ^longs la 2) (aget ^longs la 3) (aget ^ints ia 1) (aget ^doubles da 1) (aget ^bytes ba 1) (aget ^floats fa 1)]"),
    ("local-get", "(let [a (long-array 4) d (double-array 2)] (aset a 2 (+ n 9)) (aset d 1 2.5) [(aget a 2) (aget a 0) (aget d 1)])"),
    ("hinted-set", "(let [l (long-array 2) i (int-array 1) d (double-array 2)] [(aset ^longs l 0 (+ n 7)) (aget l 0) (aset ^longs l 1 4000000000) (aget l 1) (aset ^ints i 0 (- n 8)) (aget i 0) (aset ^doubles d 0 3.25) (aset ^doubles d 1 (+ n 2)) (vec d)])"),
    ("wrong-hint", "[(aget ^longs da 0) (aget ^doubles la 2) (aget ^longs oa 1) (aget ^ints la 3)]"),
    ("bounds", "[(safe (fn [] (aget ^longs la 4))) (safe (fn [] (aget ^longs la -1))) (safe (fn [] (aset ^longs la 9 1))) (safe (fn [] (aget ^doubles da (+ n 2))))]"),
    ("index-kinds", "[(aget ^longs la 1.0) (safe (fn [] (aget ^longs la nil))) (aget ^ints ia (long 2))]"),
    ("int-range", "(let [i (int-array [5 6]) l (long-array [1])] [(safe (fn [] (aset ^ints i 1 3000000000))) (aget i 1) (safe (fn [] (aset ^longs l 0 1.5))) (aget l 0)])"),
];

#[test]
fn packed_array_hints_match_the_repl() {
    let root = tempfile::tempdir().unwrap();
    let mut app = format!("(ns app)\n{DEFINITIONS}\n");
    for (name, source) in CASES {
        app.push_str(&format!("(defn case-{name} [n] (pr-str {source}))\n"));
    }
    std::fs::write(root.path().join("app.sus"), &app).unwrap();
    let exports = CASES.iter().map(|(name, _)| format!("export case-{name}: func(n: u32) -> string;")).collect::<Vec<_>>();
    std::fs::write(root.path().join("app.wit"), format!("package test:packed; world w {{ {} }}", exports.join(" "))).unwrap();
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
            "hinted-get" => Some("[2 3000000000 -4 6 2.5 -2 1.25]"),
            "local-get" => Some("[9 0 2.5]"),
            "wrong-hint" => Some("[1.5 3000000000 :y -4]"),
            "hinted-set" => Some("[7 7 4000000000 4000000000 -8 -8 3.25 2.0 [3.25 2.0]]"),
            _ => None,
        };
        if let Some(expected) = expected {
            assert_eq!(aot, expected, "{name}: {source}");
        }
    }
}
