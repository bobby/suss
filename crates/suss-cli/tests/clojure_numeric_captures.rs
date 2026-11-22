//! Raw capture fields preserve numeric behavior and shared object captures (#351).
use suss_cli::portable_session::Session;

const CASES: &[(&str, &str)] = &[
    ("(let [x 1.25 f (fn [] (+ x 2.0))] ((identity f)))", "3.25"),
    ("(let [x 3000000000 f (fn again [n] (if (= n 0) (+ x 1) (again (- n 1))))] (f 3))", "3000000001"),
    ("(let [x 3000000000 f (fn ([] (+ x 1)) ([a] (+ x 2)))] [(f) (f :unused)])", "[3000000001 3000000002]"),
    ("(let [x 1.25 f (fn [& xs] (+ x 2.0))] [(f) (f :a :b)])", "[3.25 3.25]"),
    ("(let [x 1.25 f (fn ^double [] (+ x 2.0))] [(f) ((identity f))])", "[3.25 3.25]"),
    ("(let [x -0.0 f (fn [] (* x 2.0))] [(f) (/ 1.0 (f))])", "[-0.0 ##-Inf]"),
    ("(let [x ##NaN f (fn [] (+ x 1.0)) a (f) b (f)] [(= a b) (== a b) (identical? a b)])", "[false false false]"),
    ("(let [x 9223372036854775807 f (fn [] (+ x 1))] (try (f) (catch :default e (ex-message e))))", "\"long overflow\""),
    ("(let [x 127.1 f (fn [] x)] (byte ((identity f))))", "127"),
    ("(let [x 2147483648.0 f (fn [] (+ x 0.0))] (try (int ((identity f))) (catch :default e :error)))", ":error"),
    ("(let [x (identity 3000000000) f (fn [] x)] (identical? ((identity f)) ((identity f))))", "true"),
    ("(let [x 1.5 f (fn [b] (if b x x))] [(f true) (f false)])", "[1.5 1.5]"),
    ("(let [x (identity 3000000000) f (fn ([] (+ x 1)) ([a] x))] [(f) (identical? (f :a) (f :b))])", "[3000000001 true]"),
    ("(let [x (identity 3000000000) f (fn [] (fn [] x)) g ((identity f))] [(g) (identical? (g) (g))])", "[3000000000 true]"),
    ("(let [x 1.5 f (with-meta (fn [] (+ x 2.0)) {:tag :kept})] [(f) (:tag (meta f))])", "[3.5 :kept]"),
    ("(let [events (atom []) x (do (swap! events conj :capture) 1.5) f (fn [] (do (swap! events conj :body) (+ x 2.0)))] [(f) (f) @events])", "[3.5 3.5 [:capture :body :body]]"),
    ("(let [x 1.5 f (fn [] (+ x 2.0))] [(f) (try (f 1) (catch :default e :arity)) (f)])", "[3.5 :arity 3.5]"),
    ("(let [x 1.5 f (fn [] (+ x 2.0)) out (atom f)] [(@out) (fn? @out)])", "[3.5 true]"),
];

fn cases() -> impl Iterator<Item = &'static (&'static str, &'static str)> {
    CASES.iter()
}

fn observed(source: &str) -> String {
    format!("(pr-str (try {source} (catch :default e [:hint-error (ex-message e)])))")
}

#[test]
fn runtime_and_macro_preserve_numeric_captures() {
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
fn aot_preserves_numeric_captures() {
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    let mut source = String::from("(ns localhints)\n");
    let mut wit = String::from("package test:localhints; world w {\n");
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
    std::fs::write(root.path().join("localhints.sus"), source).unwrap();
    std::fs::write(root.path().join("localhints.wit"), wit).unwrap();
    let mut compile = Command::new(env!("CARGO_BIN_EXE_suss"));
    compile.current_dir(root.path()).args([
        "compile",
        "localhints.sus",
        "-w",
        "localhints.wit",
        "-o",
        "localhints.wasm",
    ]);
    for index in 0..cases().count() {
        compile.args(["--export", &format!("case{index}=localhints/case{index}")]);
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
                "localhints.wasm",
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
