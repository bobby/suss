//! MapEntry elimination preserves source conversions, captures and effects (#351).
use suss_cli::portable_session::Session;

const CASES: &[(&str, &str)] = &[
    (
        "(let [[a b] (MapEntry. 3000000000 1.5 nil)] (+ a b))",
        "3.0000000015E9",
    ),
    (
        "(let [[a] (MapEntry. 2147483648.0 0 nil)] (try (int a) (catch :default e :error)))",
        ":error",
    ),
    ("(let [[a] (MapEntry. 127.1 0 nil)] (byte a))", "127"),
    (
        "(let [[a] (MapEntry. -0.0 0 nil)] [a (/ 1.0 a)])",
        "[-0.0 ##-Inf]",
    ),
    (
        "(let [[a] (MapEntry. ##NaN 0 nil) b (+ a 0.0)] [(= a b) (== a b) (identical? a b)])",
        "[false false false]",
    ),
    (
        "(let [[a b] (MapEntry. 9223372036854775807 1 nil)] (try (+ a b) (catch :default e (ex-message e))))",
        "\"long overflow\"",
    ),
    (
        "(let [events (atom []) [a b] (MapEntry. (do (swap! events conj :a) 3000000000) (do (swap! events conj :b) 1.5) nil)] [(+ a b) @events])",
        "[3.0000000015E9 [:a :b]]",
    ),
    (
        "(let [events (atom [])] [(try (let [[a b] (MapEntry. (do (swap! events conj :a) (throw (ex-info \"boom\" {}))) (swap! events conj :b) nil)] (+ a b)) (catch :default e (ex-message e))) @events])",
        "[\"boom\" [:a]]",
    ),
    (
        "(with-redefs [nth (fn [& xs] 7)] (let [[a b] (MapEntry. 1 2 nil)] (+ a b)))",
        "3",
    ),
    (
        "(let [e (MapEntry. 3000000000 1.5 nil) [a b] e] [e (+ a b)])",
        "[[3000000000 1.5] 3.0000000015E9]",
    ),
    ("(nth (MapEntry. 3000000000 1.5 nil) 2 :nf)", ":nf"),
    (
        "(let [[a b] (MapEntry. nil false nil)] [a b])",
        "[nil false]",
    ),
    (
        "(let [[a] (MapEntry. 3000000000 0 nil) f (fn [] a)] (f))",
        "3000000000",
    ),
    (
        "(let [[f] (MapEntry. (fn [x] (+ x 1)) nil nil)] (f 3))",
        "4",
    ),
    (
        "(loop [i 0 acc 0.0] (if (< i 4) (let [[a b] (MapEntry. 3000000000 1.5 nil)] (recur (inc i) (+ acc (+ a b)))) acc))",
        "1.2000000006E10",
    ),
    (
        "(let [x (atom 0) [a b] (MapEntry. x x nil)] [(identical? a b) (= a b)])",
        "[true true]",
    ),
];

fn cases() -> impl Iterator<Item = &'static (&'static str, &'static str)> {
    CASES.iter()
}

fn observed(source: &str) -> String {
    format!("(pr-str (try {source} (catch :default e [:hint-error (ex-message e)])))")
}

#[test]
fn runtime_and_macro_preserve_nonescaping_map_entries() {
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
fn aot_preserves_nonescaping_map_entries() {
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    let mut source = String::from("(ns localentries)\n");
    let mut wit = String::from("package test:localentries; world w {\n");
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
    std::fs::write(root.path().join("localentries.sus"), source).unwrap();
    std::fs::write(root.path().join("localentries.wit"), wit).unwrap();
    let mut compile = Command::new(env!("CARGO_BIN_EXE_suss"));
    compile.current_dir(root.path()).args([
        "compile",
        "localentries.sus",
        "-w",
        "localentries.wit",
        "-o",
        "localentries.wasm",
    ]);
    for index in 0..cases().count() {
        compile.args(["--export", &format!("case{index}=localentries/case{index}")]);
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
                "localentries.wasm",
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
