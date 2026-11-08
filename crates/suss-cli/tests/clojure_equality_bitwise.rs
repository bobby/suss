//! Clojure's equality, hashing, bit operations and casts for longs and doubles
//! (#336). Expected strings are JVM Clojure 1.12's.
use suss_cli::portable_session::Session;

fn units(session: &mut Session, value: &suss_cli::portable_session::SessionValue) -> String {
    let units = session
        .inspect(value, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(array
                .elems(&mut store)?
                .map(|unit| unit.unwrap_i32() as u16)
                .collect::<Vec<_>>())
        })
        .unwrap();
    String::from_utf16(&units).unwrap()
}
/// `(pr-str source)`, evaluated in both phases' sessions.
fn printed(source: &str) -> [String; 2] {
    [Session::new_repl().unwrap(), Session::new_macro().unwrap()].map(|mut session| {
        let value = session
            .eval(&format!("(pr-str {source})"))
            .unwrap_or_else(|error| panic!("{source}: {error}"));
        units(&mut session, &value)
    })
}
fn assert_printed(source: &str, expected: &str) {
    for actual in printed(source) {
        assert_eq!(actual, expected, "{source}");
    }
}
/// The message of the error `source` throws.
fn thrown(source: &str) -> String {
    let [first, second] = printed(&format!(
        "(try {source} :no-error (catch :default error (ex-message error)))"
    ));
    assert_eq!(first, second, "{source}");
    first
}

#[test]
fn equality_keeps_longs_and_doubles_apart() {
    assert_printed(
        "[(= 1 1.0) (== 1 1.0) (= 0.0 -0.0) (= ##NaN ##NaN) (= [1 2] [1 2.0]) (not= 1 1.0)
          (= 2 2) (= 1.5 1.5) (= #{1} #{1.0}) (get {1 :a} 1.0) (contains? #{1.0} 1)
          (= {1 :a} {1.0 :a}) (count (into #{} [1 1.0 0 0.0 -0.0]))]",
        "[false true true false false true true true false nil false false 4]",
    );
}

#[test]
fn numbers_hash_as_clojure_hasheq() {
    assert_printed(
        "[(hash 1) (hash 1.0) (hash 0) (hash -0.0) (hash ##NaN) (hash ##Inf) (hash ##-Inf)
          (hash 9223372036854775807) (hash -9223372036854775808) (hash -1) (hash 2.5)
          (hash 9007199254740993) (hash 5e-324)]",
        "[1392991556 1072693248 0 0 2146959360 2146435072 -1048576 -2106506049 1366273829 \
         1651860712 1074003968 -1973637882 1]",
    );
    // Collections hash their elements' hasheq, so they iterate as in Clojure.
    assert_printed(
        "[(hash [1 2]) (hash {1 2}) (hash #{1.0}) (hash (list 1)) (hash-unordered-coll (list 1 2))
          (seq (hash-map 1 :a 2 :b 3 :c 4 :d 5 :e 6 :f 7 :g 8 :h 9 :i 10 :j))
          (seq (set (range 20)))]",
        "[156247261 1952097988 784174462 -1381383523 460223544 \
         ([7 :g] [1 :a] [4 :d] [6 :f] [3 :c] [2 :b] [9 :i] [5 :e] [10 :j] [8 :h]) \
         (0 7 1 4 15 13 6 17 3 12 2 19 11 9 5 14 16 10 18 8)]",
    );
}

#[test]
fn bit_operations_are_64_bit_and_take_longs() {
    assert_printed(
        "[(bit-and -1 4294967297) (bit-or 2147483648 3) (bit-not 4294967296) (bit-clear -1 33)
          (bit-flip 0 -1) (bit-set 0 32) (bit-shift-left 1 33) (bit-shift-right 2147483648 1)
          (unsigned-bit-shift-right -1 0) (unsigned-bit-shift-right -1 1) (bit-shift-left 1 65)
          (bit-set 0 64) (bit-test -1 63) (bit-and-not 15 5) (bit-shift-right -8 1) (bit-or 1 2 4)
          (bit-shift-left 1 63) (bit-shift-right-zero-fill -1 1) (reduce bit-xor [1 2 4 8])
          ((fn [f] (f 1 2 4)) bit-or)]",
        "[4294967297 2147483651 -4294967297 -8589934593 -9223372036854775808 4294967296 \
         8589934592 1073741824 -1 9223372036854775807 2 1 true 10 -4 7 \
         -9223372036854775808 9223372036854775807 15 7]",
    );
    for source in ["(bit-and 1.0 1)", "(bit-shift-left 3.9 1)", "(bit-shift-left 1 -1.9)"] {
        assert_eq!(thrown(source), "\"bit operation not supported for: double\"", "{source}");
    }
    for source in ["(bit-or nil 1)", "(bit-not \"1\")", "(let [f bit-and] (f :a 1))"] {
        assert_eq!(thrown(source), "\"bit operation not supported for a non-number\"", "{source}");
    }
}

#[test]
fn imul_and_int32_coercion_are_not_public() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for source in ["(imul 2 3)", "(let [f imul] (f 2 3))", "(coerce-int32 1)"] {
            let error = session.eval(source).unwrap_err();
            assert!(format!("{error:?}").contains("Unresolved"), "{source}: {error:?}");
        }
    }
}

#[test]
fn casts_check_the_range_and_return_longs_and_doubles() {
    assert_printed(
        "[(int -2147483648) (int 1.9) (int -1.9) (int 2147483647.0) (int ##NaN) (long 1.5e10)
          (long ##NaN) (long 9.223372036854775807E18) (short 32767) (byte 127) (byte -128)
          (unchecked-byte 200) (unchecked-short 70000) (unchecked-int 3000000000)
          (unchecked-int 3e10) (unchecked-long 1e30) (unchecked-byte 300.7)
          (double 3) (float 16777217) (unchecked-float 1e300) (float ##NaN) (float 3)
          (= (float 0.1) 0.1) (map int [1 2.5]) ((fn [f] (f 7.9)) int)]",
        "[-2147483648 1 -1 2147483647 0 15000000000 0 9223372036854775807 32767 127 -128 \
         -56 4464 -1294967296 2147483647 9223372036854775807 44 3.0 1.6777216E7 ##Inf ##NaN \
         3.0 false (1 2) 7]",
    );
    for (source, message) in [
        ("(int 4294967295)", "integer overflow"),
        ("(let [f int] (f 2147483648))", "integer overflow"),
        ("(int 3e9)", "Value out of range for int: 3.0E9"),
        ("(int 2147483647.5)", "Value out of range for int: 2.1474836475E9"),
        ("(int -2147483648.9)", "Value out of range for int: -2.1474836489E9"),
        ("(int 1e100)", "Value out of range for int: 1.0E100"),
        ("(short 40000)", "Value out of range for short: 40000"),
        ("(short 32767.5)", "Value out of range for short: 32767.5"),
        ("(short ##NaN)", "Value out of range for short: NaN"),
        ("(byte 128)", "Value out of range for byte: 128"),
        ("(byte -129)", "Value out of range for byte: -129"),
        ("(byte -128.9)", "Value out of range for byte: -128.9"),
        ("(long 1e19)", "Value out of range for long: 1.0E19"),
        ("(long ##Inf)", "Value out of range for long: Infinity"),
        ("(float 1e300)", "Value out of range for float: 1.0E300"),
        ("(float -1e300)", "Value out of range for float: -1.0E300"),
        ("(float ##Inf)", "Value out of range for float: Infinity"),
        ("(int nil)", "int requires a number operand"),
        ("(double \"x\")", "double requires a number operand"),
        ("(long :a)", "long requires a number operand"),
    ] {
        assert_eq!(thrown(source), format!("{message:?}"), "{source}");
    }
}
