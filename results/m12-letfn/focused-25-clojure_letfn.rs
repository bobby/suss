//! Recursive lexical function groups preserve capture and call semantics (#351).
use suss_cli::portable_session::Session;

const CASES: &[(&str, &str)] = &[
    ("(letfn [(f [] 1) (f [] 2)] (f))", "2"),
    ("(let [x 3000000000] (letfn [(f [n] (if (zero? n) (+ x 1) (g (dec n)))) (g [n] (if (zero? n) (+ x 2) (f (dec n))))] [(f 0) (f 1) (g 1)]))", "[3000000001 3000000002 3000000001]"),
    ("(letfn [(f [] g) (g [] 9)] [((f)) (identical? (f) g)])", "[9 true]"),
    ("(letfn [(factorial [n] (if (zero? n) 1 (* n (factorial (dec n)))))] (factorial 6))", "720"),
    ("(let [f (fn [] :outer)] (letfn [(g [] (f)) (f [] :inner)] [(g) (f)]))", "[:inner :inner]"),
    ("(let [events (atom []) x (do (swap! events conj :capture) 1.5)] (letfn [(f [] (+ x 1.0)) (g [] (+ x 2.0))] [(f) (g) @events]))", "[2.5 3.5 [:capture]]"),
    ("(letfn [(f ([] (g 2)) ([x] (+ x 1))) (g [x] (f x))] [(f) (g 4)])", "[3 5]"),
    ("(let [x 3000000000] (letfn [(f [& xs] (+ x (count xs))) (g [] (f :a :b))] [(f) (g)]))", "[3000000000 3000000002]"),
    ("(letfn [(f [[x y]] (+ x y)) (g [] (f [3 4]))] (g))", "7"),
    ("(letfn [] 7)", "7"),
    ("(letfn [])", "nil"),
    ("(loop [i 0 acc 0] (if (< i 5) (letfn [(f [x] (+ x 1))] (recur (inc i) (+ acc (f i)))) acc))", "15"),
    ("(letfn* [f (fn [n] (if (zero? n) 7 (g (dec n)))) g (fn [n] (if (zero? n) 8 (f (dec n))))] [(f 1) (g 1)])", "[8 7]"),
    ("(let [pair (letfn [(f [] g) (g [] f)] [f g]) f (nth pair 0) g (nth pair 1)] [(identical? (f) g) (identical? (g) f)])", "[true true]"),
    ("(let [x (identity 3000000000)] (letfn [(f [] x) (g [] (identical? (f) (f)))] [(f) (g)]))", "[3000000000 true]"),
    ("(let [x -0.0] (letfn [(f [] (* x 2.0)) (g [] (/ 1.0 (f)))] [(f) (g)]))", "[-0.0 ##-Inf]"),
    ("(let [x ##NaN] (letfn [(f [] (+ x 1.0)) (g [] (let [a (f) b (f)] [(= a b) (== a b) (identical? a b)]))] (g)))", "[false false false]"),
    ("(let [x 9223372036854775807] (letfn [(f [] (+ x 1)) (g [] (f))] (try (g) (catch :default e (ex-message e)))))", "\"long overflow\""),
    ("(let [x 127.1] (letfn [(f [] x) (g [] (byte (f)))] (g)))", "127"),
    ("(let [x 2147483648.0] (letfn [(f [] (+ x 0.0)) (g [] (int (f)))] (try (g) (catch :default e :error))))", ":error"),
    ("(letfn [(f [x] (+ x 1)) (g [] (f 2))] [(g) (try (f) (catch :default e :arity)) (g)])", "[3 :arity 3]"),
    ("(letfn [(f [] (throw (ex-info \"bad\" {:x 1}))) (g [] (try (f) (catch :default e (ex-message e))))] (g))", "\"bad\""),
    ("(let [letfn (fn [x] x)] (letfn 7))", "7"),
    ("(letfn [(f [x] (loop [n x acc 0] (if (zero? n) acc (recur (dec n) (+ acc n))))) (g [] (f 5))] (g))", "15"),
    ("(letfn [(^long f [^long x] (+ x 1)) (g [] (f 3))] (g))", "4"),
];

fn cases() -> impl Iterator<Item = &'static (&'static str, &'static str)> {
    CASES.iter()
}

#[test]
fn recursive_groups_and_nested_captures_survive_gc_in_both_phases() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def saved-letfn (let [x 1.5] (letfn [(f [] (fn [] (+ x 1.0))) (g [] f)] [f g])))").unwrap();
        for _ in 0..3 { session.collect().unwrap(); }
        let value = session.eval(&observed("(let [f (nth saved-letfn 0) g (nth saved-letfn 1)] [((f)) (identical? (g) f)])")).unwrap();
        session.collect().unwrap();
        let units = session.inspect(&value, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(array.elems(&mut store)?.map(|unit| unit.unwrap_i32() as u16).collect::<Vec<_>>())
        }).unwrap();
        assert_eq!(String::from_utf16(&units).unwrap(), "[2.5 true]");
    }
}

fn observed(source: &str) -> String {
    format!("(pr-str (try {source} (catch :default e [:hint-error (ex-message e)])))")
}

#[test]
fn runtime_and_macro_preserve_letfn() {
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
fn aot_preserves_letfn() {
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
