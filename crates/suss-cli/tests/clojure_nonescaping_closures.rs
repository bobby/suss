//! Local closure elimination preserves source conversions, captures and effects (#351).
use suss_cli::portable_session::Session;

const CASES: &[(&str, &str)] = &[
    (
        "(let [x (+ 3000000000 5) y (+ 1.25 0.5) f (fn [] (+ x y))] (f))",
        "3.00000000675E9",
    ),
    ("(let [f (fn [] 127.1)] (byte (f)))", "127"),
    ("(let [f (fn [] 2147483647.9)] (int (f)))", "2147483647"),
    (
        "(let [f (fn [] (unchecked-float 2147483648.0))] (try (int (f)) (catch :default e :error)))",
        ":error",
    ),
    (
        "(let [f (fn [x] (if x 1.5 2.5))] [(f true) (f false)])",
        "[1.5 2.5]",
    ),
    (
        "(let [x -0.0 f (fn [] x)] [(f) (/ 1.0 (f))])",
        "[-0.0 ##-Inf]",
    ),
    (
        "(let [x ##NaN f (fn [] (+ x 0.0)) a (f) b (f)] [(= a b) (== a b) (identical? a b)])",
        "[false false false]",
    ),
    (
        "(let [x 9223372036854775807 f (fn [] (inc x))] (try (f) (catch :default e (ex-message e))))",
        "\"long overflow\"",
    ),
    (
        "(let [events (atom []) f (fn [x y] (swap! events conj :body) (+ x y))] [(f (do (swap! events conj :first) 1) (do (swap! events conj :second) 2)) @events])",
        "[3 [:first :second :body]]",
    ),
    (
        "(let [events (atom []) f (fn [] (swap! events conj :body) 7)] [(f) (f) @events])",
        "[7 7 [:body :body]]",
    ),
    (
        "(let [f (fn [^long x] x)] (try (f 1.0E300) (catch :default e :error)))",
        ":error",
    ),
    (
        "(let [f (fn ^long [] (identity 1.0E300))] (f))",
        "9223372036854775807",
    ),
    (
        "(let [f (fn [n] (loop [i n acc 0] (if (zero? i) acc (recur (dec i) (+ acc i)))))] (f 5))",
        "15",
    ),
    (
        "(let [x 3000000000 f (fn [n] (if (zero? n) x (+ x n)))] [(f 0) (f 3)])",
        "[3000000000 3000000003]",
    ),
    (
        "(let [f (fn [x] x)] (try (f 1 2) (catch :default e :arity)))",
        ":arity",
    ),
    (
        "(let [x 3000000000 f (fn [] x)] ((identity f)))",
        "3000000000",
    ),
    (
        "(let [x 3000000000 f (fn [] x) out [f]] ((nth out 0)))",
        "3000000000",
    ),
    (
        "(let [x 3000000000 f (fn [] x)] (loop [i 0 acc 0] (if (< i 4) (recur (inc i) (+ acc (f))) acc)))",
        "12000000000",
    ),
    (
        "(let [f (fn [x] (try (if x (throw :bad) 1) (catch :default e 2) (finally nil)))] [(f false) (f true)])",
        "[1 2]",
    ),
    (
        "(let [events (atom []) f (fn [] (swap! events conj :body) 7)] [(try (do (f) (throw :bad)) (catch :default e (f))) @events])",
        "[7 [:body :body]]",
    ),
    (
        "(try (let [f (fn [] (throw (ex-info \"boom\" {})))] (do (f) :after)) (catch :default e (ex-message e)))",
        "\"boom\"",
    ),
    (
        "(let [f (fn [x] ((fn [] x)))] (f 3000000000))",
        "3000000000",
    ),
    (
        "(let [f (fn [x] (fn [] x)) g (f 3000000000)] (g))",
        "3000000000",
    ),
    ("(let [f (fn [] 1)] [(fn? f) (f)])", "[true 1]"),
    (
        "(let [f (fn [x] x)] [(f 1) (try (f 1 2) (catch :default e :arity)) (f 3)])",
        "[1 :arity 3]",
    ),
    (
        "(let [x 3000000000 f (fn ([a] (+ x a)) ([a b] (+ x a b)))] [(f 1) (f 2 3)])",
        "[3000000001 3000000005]",
    ),
    (
        "(let [x 3000000000 f (fn [& xs] (+ x (count xs)))] [(f) (f 1 2)])",
        "[3000000000 3000000002]",
    ),
    (
        "(let [x 3000000000 f (fn again [n] (if (zero? n) x (+ 1 (again (dec n)))))] (f 3))",
        "3000000003",
    ),
    (
        "(let [f (fn [b] (if b 1 1.0))] [(integer? (f true)) (float? (f false)) (= (f true) (f false))])",
        "[true true false]",
    ),
    (
        "(let [f (fn [x] (+ x 1))] (try (f true) (catch :default e :error)))",
        ":error",
    ),
];

fn cases() -> impl Iterator<Item = &'static (&'static str, &'static str)> {
    CASES.iter()
}

fn observed(source: &str) -> String {
    format!("(pr-str (try {source} (catch :default e [:hint-error (ex-message e)])))")
}

#[test]
fn runtime_and_macro_preserve_nonescaping_closures() {
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
fn aot_preserves_nonescaping_closures() {
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
