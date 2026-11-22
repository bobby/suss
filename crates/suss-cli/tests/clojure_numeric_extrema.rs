//! Clojure max/min source overloads, boxed calls, signed zero and effect order.
use suss_cli::portable_session::Session;
const DEFINITIONS: &str = "(def ^{:redef true :arglists '(^double [])} extrema-guard (fn ^double [] (identity 1.0)))";
const CASES: &[(&str, &str)] = &[
    ("(let [^long x (identity 5.5)] [x (max x 1) (min x 1) (max x) (min x) (max x 1 2)])", "[5.5 5 1 5.5 5.5 5]"),
    ("(let [^double x (identity 5)] [x (max x 1.0) (min x 6.0) (max x 6) (min x 6)])", "[5 5.0 5.0 6 5.0]"),
    ("(let [^double x (identity -0.0)] [(/ 1.0 (max x 0.0)) (/ 1.0 (max 0.0 x)) (/ 1.0 (min x 0.0)) (/ 1.0 (min 0.0 x))])", "[##Inf ##Inf ##-Inf ##-Inf]"),
    ("(let [^double x (identity -0.0)] [(/ 1.0 (max x (identity 0.0))) (/ 1.0 (max (identity 0.0) x)) (/ 1.0 (min x (identity 0.0))) (/ 1.0 (min (identity 0.0) x))])", "[##Inf ##-Inf ##Inf ##-Inf]"),
    ("(let [f (identity max) g (identity min)] [(/ 1.0 (f -0.0 0.0)) (/ 1.0 (f 0.0 -0.0)) (/ 1.0 (g -0.0 0.0)) (/ 1.0 (g 0.0 -0.0))])", "[##Inf ##-Inf ##Inf ##-Inf]"),
    ("(let [^long x (identity 5.5) ^double y (identity 6)] [(max x y) (min x y) (max x (identity 6)) (min x (identity 6))])", "[6.0 5 6 5]"),
    ("(let [^long x (identity 5.5)] [(max x 6) (max x 6.0) (max x 1 2.0) (min x 6.0) (max x 1 6) (min x 6 1) (max x (identity 6) 7.0)])", "[6 6.0 5 5 6 1 7.0]"),
    ("(let [^long x (identity 1.0E30) events (atom [])] [(try (max x 1 (do (swap! events conj :later) 1)) (catch :default e :cast-error)) @events])", "[:cast-error []]"),
    ("(let [^long x (identity 1.0E30) events (atom [])] [(try (min x 1 (do (swap! events conj :later) 1)) (catch :default e :cast-error)) @events])", "[:cast-error []]"),
    ("[((fn ^long [] (max 1 2.0))) ((fn ^double [] (max 1))) ((fn ^long [] (min 1.5)))]", "[2 1.0 1]"),
    ("(let [^double x (identity ##NaN)] [(max x 1.0) (min 1.0 x) (max x (identity 1)) (min (identity 1) x)])", "[##NaN ##NaN ##NaN ##NaN]"),
    ("(let [f (identity max) g (identity min)] [(max nil ##NaN) (min ##NaN nil) (f nil ##NaN) (g ##NaN nil)])", "[##NaN ##NaN ##NaN ##NaN]"),
    ("(let [^long x (identity 5.5) ^double y (identity ##NaN)] [(max x y) (min y x) (max x (identity ##NaN)) (min (identity ##NaN) x)])", "[##NaN ##NaN ##NaN ##NaN]"),
    ("[(max 9007199254740993 9007199254740995) (min 9007199254740993 9007199254740995) (max 9007199254740993 9007199254740992.0) (min 9007199254740992.0 9007199254740993)]", "[9007199254740995 9007199254740993 9.007199254740992E15 9007199254740993]"),
    ("(let [f (identity max) g (identity min)] [(try (max :x 1) (catch :default e :error)) (try (f :x 1) (catch :default e :error)) (try (min 1 nil) (catch :default e :error)) (try (g 1 nil) (catch :default e :error))])", "[:error :error :error :error]"),
    ("(with-redefs [max (fn [a b] :replaced)] [(max 1 2) ((identity max) 1 2)])", "[2 :replaced]"),
    ("(let [f (identity max) ^long x (identity 5.5) events (atom [])] [(f x 1 (do (swap! events conj :later) 2)) @events])", "[5.5 [:later]]"),
    ("(let [^long x (identity 5.5) y (max x 1)] [y (+ y 1) (min y 6)])", "[5 6 5]"),
    ("(let [^double x (identity 5) y (max x 6.0)] [y (+ y 1) (min y 7.0)])", "[6.0 7.0 6.0]"),
    ("(let [^long x (identity 5.5) y (max x 6.0)] [y (+ y 1) (min y 7)])", "[6.0 7.0 6.0]"),
    ("(let [^long x (identity 1.0E30) events (atom [])] [(try (max x (zero? (do (swap! events conj :later) 0))) (catch :default e :number-error)) @events])", "[:number-error [:later]]"),
    ("(let [^long x (identity 5.5) ^double y (identity -0.0)] [(max x) (min y) (max x 1.0 -1.0) (min y 1.0 0.0)])", "[5.5 -0.0 5 -0.0]"),
    ("(loop [i 0 acc 0] (if (< i 5) (recur (inc i) (max acc (min i 3))) acc))", "3"),
    ("(let [result (loop [i 0 acc -0.0] (if (< i 5) (recur (inc i) (max acc (min (double i) 0.0))) acc))] (/ 1.0 result))", "##Inf"),
    // Expression-context loops have Object source type but raw numeric storage.
    ("(let [x (loop [] -0.0) y (loop [] 0.0)] [(max x y) (max y x) (min x y) (min y x)])", "[0.0 -0.0 0.0 -0.0]"),
    ("(let [x (loop [] ##NaN) y (loop [] 1.0)] [(max x y) (max y x) (min x y) (min y x)])", "[##NaN ##NaN ##NaN ##NaN]"),
];
// ADR-0004 deliberately falls back to ordinary IFn when a var loses its
// primitive signature. JVM IFn$D casts instead fail; this regression preserves
// Suss's established replacement-value contract and compares its boxed route.
const REBIND_CASES: &[(&str, &str)] = &[
    ("(let [run (fn [] (max (extrema-guard) -0.0)) boxed (fn [] ((identity max) (extrema-guard) -0.0))] [(run) (boxed) (with-redefs [extrema-guard (fn [] (identity 5))] [(run) (boxed)]) (with-redefs [extrema-guard (fn [] (identity 0))] [(/ 1.0 (run)) (/ 1.0 (boxed))]) (with-redefs [extrema-guard (fn [] :replacement)] [(try (run) (catch :default e :threw)) (try (boxed) (catch :default e :threw))])])", "[1.0 1.0 [5 5] [##-Inf ##-Inf] [:threw :threw]]"),
];
fn cases() -> impl Iterator<Item = &'static (&'static str, &'static str)> {
    CASES.iter().chain(REBIND_CASES)
}

fn observed(source: &str) -> String {
    format!("(pr-str (try {source} (catch :default e [:hint-error (ex-message e)])))")
}

#[test]
fn runtime_and_macro_preserve_numeric_extrema_semantics() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval(DEFINITIONS).unwrap();
        for (source, expected) in cases() {
            let value = session.eval(&observed(source)).unwrap_or_else(|e| panic!("{source}: {e}"));
            session.collect().unwrap();
            let units = session.inspect(&value, |mut store, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                Ok(array.elems(&mut store)?.map(|unit| unit.unwrap_i32() as u16).collect::<Vec<_>>())
            }).unwrap();
            assert_eq!(String::from_utf16(&units).unwrap(), *expected, "{source}");
        }
    }
}

#[test]
fn aot_preserves_numeric_extrema_semantics() {
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    let mut source = format!("(ns localhints)\n{DEFINITIONS}\n");
    let mut wit = String::from("package test:localhints; world w {\n");
    for (index, (expression, expected)) in cases().enumerate() {
        // Only the unused WIT argument is synthesized as a primitive. The
        // helper and local bindings retain their genuine source hints.
        source.push_str(&format!("(def case{index} (fn [unused] (if (= {} {expected:?}) 1 0)))\n", observed(expression)));
        wit.push_str(&format!("export case{index}: func(unused: u32) -> u32;\n"));
    }
    wit.push('}');
    std::fs::write(root.path().join("localhints.sus"), source).unwrap();
    std::fs::write(root.path().join("localhints.wit"), wit).unwrap();
    let mut compile = Command::new(env!("CARGO_BIN_EXE_suss"));
    compile.current_dir(root.path()).args(["compile", "localhints.sus", "-w", "localhints.wit", "-o", "localhints.wasm"]);
    for index in 0..(CASES.len() + REBIND_CASES.len()) {
        compile.args(["--export", &format!("case{index}=localhints/case{index}")]);
    }
    let output = compile.output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    for (index, (expression, _)) in cases().enumerate() {
        let output = Command::new(env!("CARGO_BIN_EXE_suss")).current_dir(root.path())
            .args(["run", "localhints.wasm", "--invoke", &format!("case{index}"), "--", "0"])
            .output().unwrap();
        assert!(output.status.success(), "{expression}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "1", "{expression}");
    }
}


const DECLARATION_ERRORS: &[(&str, &str)] = &[
    ("(fn ^double [] (max 1 2))", "Mismatched primitive return, expected: double, had: long"),
    ("(fn ^long [] (min 1.0 2.0))", "Mismatched primitive return, expected: long, had: double"),
    ("(let [^long x (max 1 2)] x)", "Can't type hint a local with a primitive initializer"),
    ("(let [^double x (min 1.0 2.0)] x)", "Can't type hint a local with a primitive initializer"),
];
#[test]
fn extrema_source_return_types_and_initializers_are_checked() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for (source, expected) in DECLARATION_ERRORS {
            let error = session.eval(source).err().unwrap_or_else(|| panic!("accepted {source}"));
            assert!(error.to_string().contains(expected), "{source}: {error}");
        }
    }
}

#[test]
fn aot_rejects_invalid_extrema_return_types() {
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("invalid.wit"),
        "package test:invalid-hints; world w { export check: func() -> u32; }").unwrap();
    for (source, expected) in DECLARATION_ERRORS {
        std::fs::write(root.path().join("invalid.sus"),
            format!("(ns invalid) (def check (fn [] {source} 1))")).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_suss")).current_dir(root.path())
            .args(["compile", "invalid.sus", "-w", "invalid.wit", "-o", "invalid.wasm", "--export", "check=invalid/check"])
            .output().unwrap();
        assert!(!output.status.success(), "accepted {source}");
        assert!(String::from_utf8_lossy(&output.stderr).contains(expected), "{source}: {}", String::from_utf8_lossy(&output.stderr));
    }
}
