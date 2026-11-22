//! Known numeric core results preserve Clojure values and source inlining (#337).
use suss_cli::portable_session::Session;

const DEFINITIONS: &str = r#"(def ^{:redef true :arglists '(^long [])} raw-long (fn ^long [] (identity 4)))
(def run-long (fn [] [(try (quot (raw-long) 2) (catch :default e :error)) (try (rem (raw-long) 2) (catch :default e :error)) (try (abs (raw-long)) (catch :default e :error)) (try (unchecked-add (raw-long) 1) (catch :default e :error)) (try (unchecked-negate (raw-long)) (catch :default e :error))]))
(def ^{:redef true :arglists '(^double [])} raw-double (fn ^double [] (identity 4.0)))
(def run-double (fn [] [(try (quot (raw-double) 2) (catch :default e :error)) (try (rem (raw-double) 2) (catch :default e :error)) (try (abs (raw-double)) (catch :default e :error)) (try (unchecked-add (raw-double) 1) (catch :default e :error)) (try (unchecked-negate (raw-double)) (catch :default e :error))]))"#;

// Fresh JVM Clojure 1.12.6; results/m12-raw-core retains source and output.
const CASES: &[(&str, &str)] = &[
    ("[(quot 7 3) (quot -7 3) (quot 7 -3) (quot -7 -3) (rem -7 3) (rem 7 -3) (abs -7)]", "[2 -2 -2 2 -1 1 7]"),
    ("[(quot -9223372036854775808 -1) (rem -9223372036854775808 -1) (abs -9223372036854775808)]", "[-9223372036854775808 0 -9223372036854775808]"),
    ("[(unchecked-add 9223372036854775807 1) (unchecked-subtract -9223372036854775808 1) (unchecked-multiply 9223372036854775807 2) (unchecked-negate -9223372036854775808)]", "[-9223372036854775808 9223372036854775807 -2 -9223372036854775808]"),
    ("[(quot -0.5 2.0) (quot -0.0 2.0) (rem -0.5 2.0) (rem -0.0 2.0) (abs -0.0) (unchecked-negate -0.0)]", "[0.0 0.0 -0.5 -0.0 0.0 0.0]"),
    ("[(quot 9007199254740993 2) (quot 9007199254740993 2.0) (rem 9007199254740993 2) (rem 9007199254740993 2.0)]", "[4503599627370496 4.503599627370496E15 1 0.0]"),
    ("[(quot 2.5E20 0.5) (rem 2.5E20 0.5) (quot -9.223372036854776E18 1.0) (rem -9.223372036854776E18 1.0)]", "[5.0E20 0.0 -9.223372036854776E18 0.0]"),
    ("[(abs ##NaN) (abs ##-Inf) (unchecked-add ##Inf ##-Inf) (unchecked-negate ##Inf)]", "[##NaN ##Inf ##NaN ##-Inf]"),
    ("[(try (quot ##NaN 1.0) (catch :default e :error)) (try (rem ##Inf 1.0) (catch :default e :error)) (try (quot 1.0 ##NaN) (catch :default e :error)) (try (rem 1.0 ##Inf) (catch :default e :error))]", "[:error :error :error ##NaN]"),
    ("[(try (quot 1 0) (catch :default e :error)) (try (rem 1.0 -0.0) (catch :default e :error)) (try (quot ##NaN 0.0) (catch :default e :error)) (try (rem 0.0 0.0) (catch :default e :error))]", "[:error :error :error :error]"),
    ("[(quot 1.0 ##Inf) (quot -1.0 ##Inf) (rem 1.0 ##Inf) (rem -1.0 ##Inf)]", "[0.0 0.0 ##NaN ##NaN]"),
    ("(let [effects (atom [])] [(try (quot (do (swap! effects conj :first) :bad) (do (swap! effects conj :second) 0)) (catch :default e :error)) @effects])", "[:error [:first :second]]"),
    ("(let [^long x (identity 1.0E30) effects (atom [])] [(try (quot x (do (swap! effects conj :second) 1)) (catch :default e :error)) @effects])", "[:error []]"),
    ("(with-redefs [quot (fn [x y] :replaced)] [(quot 7 2) ((identity quot) 7 2)])", "[3 :replaced]"),
    ("(with-redefs [rem (fn [x y] :replaced)] [(rem 7 2) ((identity rem) 7 2)])", "[1 :replaced]"),
    ("(with-redefs [abs (fn [x] :replaced)] [(abs -7) ((identity abs) -7)])", "[7 :replaced]"),
    ("(with-redefs [unchecked-add (fn [x y] :replaced)] [(unchecked-add 7 2) ((identity unchecked-add) 7 2)])", "[9 :replaced]"),
    ("(with-redefs [unchecked-inc (fn [x] :replaced)] [(unchecked-inc 7) (unchecked-inc (identity 7)) ((identity unchecked-inc) 7)])", "[8 8 :replaced]"),
    ("[(quot (identity 7) 2) (rem (identity 7) 2) (abs (identity -7)) (unchecked-negate (identity -7))]", "[3 1 7 7]"),
    ("[(unchecked-add 1.0 -0.0) (unchecked-subtract -0.0 0.0) (unchecked-multiply -0.0 2) (unchecked-inc 2.5) (unchecked-dec 2.5)]", "[1.0 -0.0 -0.0 3.5 1.5]"),
    ("(let [^double x (identity -7)] [(quot x 2) (rem x 2) (abs x) (unchecked-add x 2)])", "[-3.0 -1.0 7.0 -5.0]"),
    ("(let [f (fn [^long x] [(quot x 2) (rem x 2) (abs x) (unchecked-add x 2)])] [(f -7) (f -9223372036854775808)])", "[[-3 -1 7 -5] [-4611686018427387904 0 -9223372036854775808 -9223372036854775806]]"),
    ("[(try (abs nil) (catch :default e :error)) (try (unchecked-add :bad 1) (catch :default e :error)) (try (quot true 1) (catch :default e :error))]", "[:error :error :error]"),
];

// ADR-0004 preserves boxed replacements after a primitive signature is lost.
// These are Suss-only controls, also executed with the frozen before compiler.
const REBIND_CASES: &[(&str, &str)] = &[
    ("(vector (run-long) (with-redefs [raw-long (fn [] (identity 3.5))] (run-long)))", "[[2 0 4 5 -4] [1.0 1.5 3.5 4.5 -3.5]]"),
    ("(vector (run-double) (with-redefs [raw-double (fn [] (identity 5))] (run-double)))", "[[2.0 0.0 4.0 5.0 -4.0] [2 1 5 6 -5]]"),
    ("(with-redefs [raw-long (fn [] :bad) raw-double (fn [] nil)] [(run-long) (run-double)])", "[[:error :error :error :error :error] [:error :error :error :error :error]]"),
    ("(with-redefs [raw-long (fn [] (identity -0.0))] (run-long))", "[0.0 -0.0 0.0 1.0 0.0]"),
];

fn cases() -> impl Iterator<Item = &'static (&'static str, &'static str)> {
    CASES.iter().chain(REBIND_CASES)
}

fn observed(source: &str) -> String {
    format!("(pr-str (try {source} (catch :default e [:hint-error (ex-message e)])))")
}

#[test]
fn runtime_and_macro_preserve_known_numeric_core_results() {
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
fn aot_preserves_known_numeric_core_results() {
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
    for index in 0..cases().count() {
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
