//! Temporary vector elimination preserves source conversions, identity and effects (#351).
use suss_cli::portable_session::Session;

// The 17 exact value pairs in results/m12-vectors/cases.json match fresh JVM.
const CASES: &[(&str, &str)] = &[
    ("(let [[a b] [3000000000 1.5]] (+ a b))", "3.0000000015E9"),
    (
        "(let [[a] [2147483648.0]] (try (int a) (catch :default e :error)))",
        ":error",
    ),
    ("(let [[a] [127.1]] (byte a))", "127"),
    ("(let [[a] [-0.0]] [a (/ 1.0 a)])", "[-0.0 ##-Inf]"),
    (
        "(let [[a b] [9223372036854775807 1]] (try (+ a b) (catch :default e (ex-message e))))",
        "\"long overflow\"",
    ),
    (
        "(let [events (atom []) [a b] [(do (swap! events conj :a) 3000000000) (do (swap! events conj :b) 1.5)]] [(+ a b) @events])",
        "[3.0000000015E9 [:a :b]]",
    ),
    (
        "(let [events (atom [])] [(try (let [[a b] [(do (swap! events conj :a) (throw (ex-info \"boom\" {}))) (swap! events conj :b)]] (+ a b)) (catch :default e (ex-message e))) @events])",
        "[\"boom\" [:a]]",
    ),
    (
        "(with-redefs [nth (fn [& xs] 7)] (let [[a b] [1 2]] (+ a b)))",
        "3",
    ),
    (
        "(with-redefs [vector (fn [& xs] nil)] (let [[a b] [1 2]] (+ a b)))",
        "3",
    ),
    (
        "(let [v [3000000000 1.5] [a b] v] [v (+ a b)])",
        "[[3000000000 1.5] 3.0000000015E9]",
    ),
    ("(nth [3000000000 1.5] 2 :nf)", ":nf"),
    ("(let [[a b] [nil false]] [a b])", "[nil false]"),
    ("(let [[a] [3000000000] f (fn [] a)] (f))", "3000000000"),
    ("(let [[f] [(fn [x] (+ x 1))]] (f 3))", "4"),
    (
        "(loop [i 0 acc 0.0] (if (< i 4) (let [[a b] [3000000000 1.5]] (recur (inc i) (+ acc (+ a b)))) acc))",
        "1.2000000006E10",
    ),
    (
        "(let [[a] [##NaN] b (+ a 0.0)] [(= a b) (== a b) (identical? a b)])",
        "[false false false]",
    ),
    (
        "(let [x (atom 0) [a b] [x x]] [(identical? a b) (= a b)])",
        "[true true]",
    ),
];

// This is a recorded boxed-path control, NOT a JVM-conformance assertion.
// JVM gives [true false true]; the frozen f5056ffe Suss binary and current
// documented scalar predicates give [false false false]. Preserve the failed
// JVM assertion and the independent baseline probe in results/m12-vectors.
// The constructor control uses Suss's class binding, which JVM cannot rebind.
// Its result comes from the frozen compiler, not a JVM value assertion.
const BOXED_BASELINE_CONTROLS: &[(&str, &str)] = &[
    (
        "(let [[a] [##NaN]] [(= a a) (== a a) (identical? a a)])",
        "[false false false]",
    ),
    (
        "(let [f (fn [] (let [[a b] [3000000000 1.5]] (+ a b)))] [(f) (with-redefs [PersistentVector nil] (try (f) (catch :default e :ctor-error))) (f)])",
        "[3.0000000015E9 :ctor-error 3.0000000015E9]",
    ),
];

fn cases() -> impl Iterator<Item = &'static (&'static str, &'static str)> {
    CASES.iter().chain(BOXED_BASELINE_CONTROLS.iter())
}

fn observed(source: &str) -> String {
    format!("(pr-str (try {source} (catch :default e [:hint-error (ex-message e)])))")
}

#[test]
fn runtime_and_macro_preserve_nonescaping_vectors() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for (source, expected) in cases() {
            let value = session
                .eval(&observed(source))
                .unwrap_or_else(|e| panic!("{source}: {e}"));
            session.collect().unwrap();
            let units = session
                .inspect(&value, |mut store, value| {
                    let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                    Ok(array
                        .elems(&mut store)?
                        .map(|unit| unit.unwrap_i32() as u16)
                        .collect::<Vec<_>>())
                })
                .unwrap();
            assert_eq!(String::from_utf16(&units).unwrap(), *expected, "{source}");
        }
    }
}

#[test]
fn aot_preserves_nonescaping_vectors() {
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    let mut source = String::from("(ns localvectors)\n");
    let mut wit = String::from("package test:localvectors; world w {\n");
    for (index, (expression, expected)) in cases().enumerate() {
        // Only the unused WIT argument is synthesized as a primitive. The
        // helper and local bindings retain their genuine source hints.
        source.push_str(&format!(
            "(def case{index} (fn [unused] (if (= {} {expected:?}) 1 0)))\n",
            observed(expression)
        ));
        wit.push_str(&format!("export case{index}: func(unused: u32) -> u32;\n"));
    }
    wit.push('}');
    std::fs::write(root.path().join("localvectors.sus"), source).unwrap();
    std::fs::write(root.path().join("localvectors.wit"), wit).unwrap();
    let mut compile = Command::new(env!("CARGO_BIN_EXE_suss"));
    compile.current_dir(root.path()).args([
        "compile",
        "localvectors.sus",
        "-w",
        "localvectors.wit",
        "-o",
        "localvectors.wasm",
    ]);
    for index in 0..cases().count() {
        compile.args(["--export", &format!("case{index}=localvectors/case{index}")]);
    }
    let output = compile.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for (index, (expression, _)) in cases().enumerate() {
        let output = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args([
                "run",
                "localvectors.wasm",
                "--invoke",
                &format!("case{index}"),
                "--",
                "0",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{expression}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "1",
            "{expression}"
        );
    }
}
