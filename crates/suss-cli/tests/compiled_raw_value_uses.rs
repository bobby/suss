//! Raw longs and doubles in value uses (#387, #389): a hash, `identical?`,
//! `=` of small integers, `bit-and`/`bit-or`/`bit-xor` of longs, and a hinted
//! array access's index and stored value read their operands unboxed. Each
//! case's value is the one the boxed paths gave before, including at the i31
//! boundary, for longs outside it, NaN, wrong hints and failing operations.
//! Operands derive from the argument `n`, 0, so nothing folds at compile
//! time.
use std::process::Command;

const DEFINITIONS: &str = "(def neg -1) (def big 3221225472) (def d 3.0) (def three 3) (def kw :k)";
/// Case name, an expression of `n` whose value is a long, and that value.
const CASES: &[(&str, &str, i64)] = &[
    ("hash-loop-across-i31", "(loop [i (- n 5) a 0] (if (< i 5) (recur (inc i) (+ a (hash (+ i 1073741820)))) a))", -1956452453),
    ("hash-raw-large", "(hash (* (+ n 3) 1073741824))", -1175684108),
    ("hash-raw-min", "(hash (+ n -9223372036854775807 -1))", 1366273829),
    ("hash-sentinel-count", "(loop [i (- n 2000) c 0] (if (< i 2000) (recur (inc i) (if (identical? (hash i) -1) (inc c) c)) c))", 0),
    ("identical-long-i31-var", "(loop [i (- n 3) c 0] (if (< i 3) (recur (inc i) (if (identical? i neg) (inc c) c)) c))", 1),
    ("identical-long-boxed-var", "(if (identical? (* (+ n 3) 1073741824) big) 1 0)", 1),
    ("identical-long-boxed-other", "(if (identical? (* (+ n 3) 1073741824) neg) 1 0)", 0),
    ("identical-double-var", "(if (identical? (* (+ n 1.5) 2) d) 1 0)", 1),
    ("identical-long-double", "(if (identical? (+ n 3) d) 1 0)", 0),
    ("identical-double-long", "(if (identical? (+ n 3.0) three) 1 0)", 0),
    ("identical-long-keyword", "(if (identical? (+ n 3) kw) 1 0)", 0),
    ("identical-nan", "(if (identical? (/ (+ n 0.0) 0.0) (/ (+ n 0.0) 0.0)) 1 0)", 0),
    ("identical-raw-longs", "(if (identical? (* (+ n 3) 1073741824) (* (+ n 1) 3221225472)) 1 0)", 1),
    ("eq-raw-across-i31", "(loop [i (+ n 1073741822) c 0] (if (< i 1073741826) (recur (inc i) (+ (* c 4) (if (= i 1073741823) 1 0) (if (= i 1073741824) 2 0))) c))", 24),
    ("bitwise-raw", "(bit-xor (bit-and (+ n 5000000000) 4294967295) (bit-or (- n 1) 8))", -705032705),
    ("bitwise-double-throws", "(try (bit-and (+ n 1.5) 1) (catch :default e -1))", -1),
    ("aget-raw-index", "(let [a (long-array [1 2 5000000000])] (loop [i n s 0] (if (< i 3) (recur (inc i) (+ s (aget ^longs a (bit-and i 3)))) s)))", 5000000003),
    ("aget-raw-negative", "(try (aget ^longs (long-array 3) (- n 1)) (catch :default e -1))", -1),
    ("aget-raw-huge", "(try (aget ^longs (long-array 3) (+ n 4294967296)) (catch :default e -1))", -1),
    ("aget-int-raw-index", "(aget ^ints (int-array [4 5 6]) (bit-or n 2))", 6),
    ("aset-raw-long", "(let [a (long-array 2)] (aset ^longs a (bit-and n 1) (* (+ n 3) 1073741824)) (aget a 0))", 3221225472),
    ("aset-raw-result", "(let [a (long-array 1)] (aset ^longs a 0 (* (+ n 3) 1073741824)))", 3221225472),
    ("aset-int-raw-boundary", "(let [a (int-array 2)] (aset ^ints a 0 (+ n 1073741824)) (aget a 0))", 1073741824),
    ("aset-int-raw-overflow", "(try (aset ^ints (int-array 2) 0 (+ n 5000000000)) (catch :default e -1))", -1),
    ("aset-double-raw", "(let [a (double-array 2)] (aset ^doubles a 1 (* (+ n 1.5) 2)) (long (* 2 (aget a 1))))", 6),
    ("aset-double-raw-long", "(let [a (double-array 2)] (aset ^doubles a 1 (+ n 3)) (long (* 2 (aget a 1))))", 6),
    ("aset-wrong-hint", "(let [a (object-array 2)] (aset ^longs a 0 (+ n 7)) (aget a 0))", 7),
    ("aget-wrong-hint", "(let [a (object-array [9])] (aget ^longs a (bit-and n 1)))", 9),
];

#[test]
fn raw_value_uses_match_the_boxed_paths() {
    let root = tempfile::tempdir().unwrap();
    let mut app = format!("(ns app)\n{DEFINITIONS}\n");
    for (name, source, _) in CASES {
        app.push_str(&format!("(defn case-{name} [n] {source})\n"));
    }
    std::fs::write(root.path().join("app.sus"), &app).unwrap();
    let exports = CASES.iter().map(|(name, _, _)| format!("export case-{name}: func(n: u32) -> s64;")).collect::<Vec<_>>();
    std::fs::write(root.path().join("app.wit"), format!("package test:raw; world w {{ {} }}", exports.join(" "))).unwrap();
    let mut compile = Command::new(env!("CARGO_BIN_EXE_suss"));
    compile.current_dir(root.path()).args(["compile", "app.sus", "-w", "app.wit", "-o", "app.wasm"]);
    for (name, _, _) in CASES {
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
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), expected.to_string(), "{name}: {source}");
    }
}
