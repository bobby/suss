//! Remaining bitwise/narrow cast values and raw paths follow Clojure (#337).
use suss_cli::portable_session::Session;

const DEFINITIONS: &str = r#"(def ^{:redef true :arglists '(^long [])} cast-long (fn ^long [] (identity 4)))
(def ^{:redef true :arglists '(^double [])} cast-double (fn ^double [] (identity 4.0)))
(def run-long-casts (fn [] [(try (byte (cast-long)) (catch :default e :error)) (try (int (cast-long)) (catch :default e :error)) (try (double (float (cast-long))) (catch :default e :error)) (try (double (unchecked-float (cast-long))) (catch :default e :error))]))
(def run-double-casts (fn [] [(try (byte (cast-double)) (catch :default e :error)) (try (int (cast-double)) (catch :default e :error)) (try (double (float (cast-double))) (catch :default e :error)) (try (double (unchecked-float (cast-double))) (catch :default e :error))]))
"#;

// Fresh JVM Clojure 1.12.6; results/m12-bits-casts retains source and output.
const CASES: &[(&str, &str)] = &[
    ("[(bit-not 0) (bit-not -9223372036854775808) (bit-and-not -1 255) (bit-and-not -1 1 2 4)]", "[-1 9223372036854775807 -256 -8]"),
    ("[(bit-shift-left 1 -1) (bit-shift-left 1 63) (bit-shift-left 1 64) (bit-shift-left 1 65) (bit-shift-left 1 -65)]", "[-9223372036854775808 -9223372036854775808 1 2 -9223372036854775808]"),
    ("[(bit-shift-right -9223372036854775808 -1) (bit-shift-right -9223372036854775808 64) (unsigned-bit-shift-right -1 0) (unsigned-bit-shift-right -1 63) (unsigned-bit-shift-right -1 64)]", "[-1 -9223372036854775808 -1 1 -1]"),
    ("[(bit-clear -9223372036854775808 63) (bit-set 0 -1) (bit-flip -1 64) (bit-test -9223372036854775808 63)]", "[0 -9223372036854775808 -2 true]"),
    ("[(bit-and-not (identity -1) 15) (bit-not (identity 1)) (bit-set (identity 0) 32) (bit-test (identity 1) 64)]", "[-16 -2 4294967296 true]"),
    ("(let [^long x (identity 5.5)] [(bit-not x) (bit-and-not x 1) (bit-shift-left x 1) (try (bit-set x 1) (catch :default e :error)) (try (bit-test x 1) (catch :default e :error))])", "[-6 4 10 :error :error]"),
    ("(let [^double x (identity 5)] [(bit-not x) (bit-and-not x 1) (bit-shift-left x 1) (bit-set x 1) (bit-test x 0)])", "[-6 4 10 7 true]"),
    ("[(try (bit-not 1.0) (catch :default e :error)) (try (bit-shift-left 1 1.0) (catch :default e :error)) (try (bit-clear 1.0 0) (catch :default e :error)) (try (bit-set 1 1.0) (catch :default e :error))]", "[:error :error :error :error]"),
    ("(with-redefs [bit-not (fn [x] :replaced)] [(bit-not 1) ((identity bit-not) 1)])", "[-2 :replaced]"),
    ("(with-redefs [bit-set (fn [x y] :replaced) bit-test (fn [x y] :replaced)] [(bit-set 0 1) ((identity bit-set) 0 1) (bit-test 1 0) ((identity bit-test) 1 0)])", "[:replaced :replaced :replaced :replaced]"),
    ("[(byte 127) (short -32768) (int 2147483647) (unchecked-byte 128) (unchecked-short 32768) (unchecked-int 2147483648)]", "[127 -32768 2147483647 -128 -32768 -2147483648]"),
    ("[(byte -127.5) (short -32767.75) (int -2147483647.5) (unchecked-byte 255.5) (unchecked-short 65535.5) (unchecked-int 2147483648.0)]", "[-127 -32767 -2147483647 -1 -1 2147483647]"),
    ("[(try (byte 127.1) (catch :default e :error)) (try (short 32767.1) (catch :default e :error)) (try (int 2147483647.1) (catch :default e :error)) (int ##NaN)]", "[:error :error :error 0]"),
    ("[(try (byte ##NaN) (catch :default e :error)) (try (short ##NaN) (catch :default e :error)) (try (int ##Inf) (catch :default e :error)) (try (int 9223372036854775807) (catch :default e :error))]", "[:error :error :error :error]"),
    ("[(unchecked-byte ##Inf) (unchecked-byte ##-Inf) (unchecked-short ##Inf) (unchecked-short ##-Inf) (unchecked-int ##Inf) (unchecked-int ##-Inf) (unchecked-int ##NaN)]", "[-1 0 -1 0 2147483647 -2147483648 0]"),
    ("[(unchecked-byte 9223372036854775807) (unchecked-short -9223372036854775808) (unchecked-int 9223372036854775807)]", "[-1 0 -1]"),
    ("[(double (float 1.2)) (double (unchecked-float 1.2)) (double (float -0.0)) (double (unchecked-float ##NaN))]", "[1.2000000476837158 1.2000000476837158 -0.0 ##NaN]"),
    ("[(double (float 18014399583223809)) (double (unchecked-float 18014399583223809))]", "[1.801440065696563E16 1.801440065696563E16]"),
    ("[(try (float ##Inf) (catch :default e :error)) (try (float 3.402823466385289E38) (catch :default e :error)) (double (unchecked-float ##Inf)) (double (float ##NaN))]", "[:error :error ##Inf ##NaN]"),
    ("[(double (float 3.4028234663852886E38)) (double (float -3.4028234663852886E38)) (double (unchecked-float 1.0E300))]", "[3.4028234663852886E38 -3.4028234663852886E38 ##Inf]"),
    ("(let [^long x (identity 5.5)] [(byte x) (short x) (int x) (double (float x)) (double (unchecked-float x))])", "[5 5 5 5.0 5.0]"),
    ("[(byte (identity 7)) (short (identity 300)) (try (int (identity 3000000000.0)) (catch :default e :error))]", "[7 300 :error]"),
    ("(with-redefs [byte (fn [x] :replaced)] [(byte 7) ((identity byte) 7)])", "[7 :replaced]"),
    ("(with-redefs [unchecked-float (fn [x] :replaced)] [(double (unchecked-float 7)) ((identity unchecked-float) 7)])", "[7.0 :replaced]"),
    ("[(try (byte nil) (catch :default e :error)) (try (float :bad) (catch :default e :error)) (try (unchecked-int true) (catch :default e :error))]", "[:error :error :error]"),
    ("(let [events (atom [])] [(try (do (byte 128) (swap! events conj :after) :ok) (catch :default e :error)) @events])", "[:error []]"),
    ("(let [events (atom [])] [(try (bit-shift-left (do (swap! events conj :first) :bad) (do (swap! events conj :second) 1)) (catch :default e :error)) @events])", "[:error [:first :second]]"),
    ("[(== (double (float (identity 18014399583223809))) 1.8014398509481984E16) (== (double (unchecked-float (identity 18014399583223809))) 1.8014400656965632E16)]", "[true true]"),
    ("(let [x (loop [] 18014399583223809)] [(== (double (float x)) 1.8014398509481984E16) (== (double (unchecked-float x)) 1.8014400656965632E16)])", "[true true]"),
    ("(let [^long x (identity 18014399583223809)] [(== (double (float x)) 1.8014400656965632E16) (== (double (unchecked-float x)) 1.8014400656965632E16)])", "[true true]"),
];

// Suss ADR-0004 fallback controls; these are not JVM numeric value matches.
const REBIND_CASES: &[(&str, &str)] = &[
    ("(with-redefs [cast-long (fn [] (identity 1.2))] (run-long-casts))", "[1 1 1.2000000476837158 1.2000000476837158]"),
    ("(with-redefs [cast-long (fn [] nil)] (run-long-casts))", "[:error :error :error :error]"),
    ("(with-redefs [cast-long (fn [] (identity 1.0E300))] (run-long-casts))", "[:error :error :error ##Inf]"),
    ("(with-redefs [cast-double (fn [] (identity 18014399583223809))] (let [xs (run-double-casts)] [(nth xs 0) (nth xs 1) (== (nth xs 2) 1.8014398509481984E16) (== (nth xs 3) 1.8014400656965632E16)]))", "[:error :error true true]"),
];
fn cases() -> impl Iterator<Item = &'static (&'static str, &'static str)> { CASES.iter().chain(REBIND_CASES) }

fn observed(source: &str) -> String {
    format!("(pr-str (try {source} (catch :default e [:hint-error (ex-message e)])))")
}

#[test]
fn runtime_and_macro_preserve_raw_bits_and_casts() {
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
fn aot_preserves_raw_bits_and_casts() {
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
