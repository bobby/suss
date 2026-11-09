//! Packed primitive arrays (ADR-0003, #336). Expected strings are JVM Clojure
//! 1.12's, except where ADR-0003 differs: `float` elements are doubles, and
//! `aset` converts with the element type's cast where Clojure's reflection
//! fails to find a method.
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
fn constructors_store_elements_with_the_unchecked_casts() {
    assert_printed(
        "[(vec (byte-array [200 1.9 -129])) (vec (short-array [70000]))
          (vec (int-array [3e10 3000000000])) (vec (long-array [1.5 1e30]))
          (vec (double-array [1 2.5])) (vec (float-array [1e300 3]))
          (vec (long-array 3 [1 2])) (vec (long-array 2 7)) (vec (int-array 2 7.9))
          (vec (long-array 1 [1 2 3])) (vec (long-array nil)) (alength (long-array 2.5))
          (vec (byte-array 2)) (vec (double-array 2)) (vec (int-array (range 5)))]",
        "[[-56 1 127] [4464] [2147483647 -1294967296] [1 9223372036854775807] [1.0 2.5] \
         [##Inf 3.0] [1 2 0] [7 7] [7 7] [1] [] 2 [0 0] [0.0 0.0] [0 1 2 3 4]]",
    );
    for (source, message) in [
        ("(long-array -1)", "-1"),
        ("(int-array -3 0)", "-3"),
        ("(long-array 3e10)", "Value out of range for int: 3.0E10"),
        ("(long-array [:a])", "unchecked-long requires a number operand"),
    ] {
        assert_eq!(thrown(source), format!("{message:?}"), "{source}");
    }
}

#[test]
fn aget_returns_longs_and_doubles_and_aset_checks_the_element_type() {
    assert_printed(
        "[(aget (long-array [5 6]) 1) (aget (long-array 3) 1.5) (aget (byte-array [-1]) 0)
          (aget (int-array [2147483647]) 0) (aget (double-array [2.5]) 0)
          (= (aget (float-array [0.1]) 0) (float 0.1)) (= (aget (float-array [0.1]) 0) 0.1)
          (let [a (byte-array 1)] [(aset a 0 100) (aget a 0)])
          (let [a (byte-array 1)] (aset a 0 -128) (aget a 0))
          (let [a (int-array 1)] (aset a 0 7.9) (aget a 0))
          (let [a (long-array 1)] (aset a 0 9223372036854775807) (aget a 0))
          (let [a (float-array 1)] (aset a 0 1e300) (aget a 0))
          (let [a (double-array 1)] (aset a 0 3) (aget a 0))
          (let [a (short-array 2)] (aset a 1 -32768) (vec a))]",
        "[6 0 -1 2147483647 2.5 true false [100 100] -128 7 9223372036854775807 ##Inf 3.0 \
         [0 -32768]]",
    );
    for (source, message) in [
        ("(aget (long-array 3) 5)", "Index 5 out of bounds for length 3"),
        ("(aget (long-array 3) -1)", "Index -1 out of bounds for length 3"),
        ("(aset (long-array 0) 0 1)", "Index 0 out of bounds for length 0"),
        ("(aset (byte-array 1) 0 200)", "Value out of range for byte: 200"),
        ("(aset (short-array 1) 0 -32769)", "Value out of range for short: -32769"),
        ("(aset (int-array 1) 0 10000000000)", "integer overflow"),
        ("(aset (long-array 1) 0 1e19)", "Value out of range for long: 1.0E19"),
        ("(aset (long-array 1) 0 nil)", "long requires a number operand"),
        ("(aget (long-array 1) :a)", "int requires a number operand"),
        ("(set! (.-length (long-array 1)) 2)", "Invalid nominal operation"),
    ] {
        assert_eq!(thrown(source), format!("{message:?}"), "{source}");
    }
}

#[test]
fn core_reads_packed_arrays_as_arrays() {
    assert_printed(
        "[(seq (long-array [1 2])) (seq (long-array 0)) (count (int-array 4))
          (nth (double-array [1 2]) 1) (reduce + (int-array [1 2 3])) (get (long-array [5 6]) 1)
          (contains? (long-array [5 6]) 1) (into [] (short-array [1 2])) (first (float-array [1.5]))
          (array? (long-array 1)) (.-length (double-array 3)) (alength (byte-array 5))
          (some even? (int-array [1 2 3 4])) (= (long-array 1) (long-array 1))
          (let [a (long-array 1)] (= (hash a) (hash a))) (vec (to-array (long-array [1 2])))]",
        "[(1 2) nil 4 2.0 6 6 true [1 2] 1.5 true 3 5 true false true [1 2]]",
    );
    // aclone copies, and vec copies too, as Clojure's vec of a primitive array
    // does, so neither aliases the original.
    assert_printed(
        "(let [a (long-array [1 2]) b (aclone a) v (vec a)]
           (aset b 0 9)
           (aset a 1 7)
           [(vec a) (vec b) v (identical? a b) (count (conj (vec (long-array (range 40))) nil))])",
        "[[1 7] [9 2] [1 2] false 41]",
    );
}

#[test]
fn packed_arrays_print_and_convert_as_java_arrays() {
    // Java's Object.toString of an array and Clojure's #object form, with
    // Suss's identity hash for the JVM's identityHashCode.
    assert_printed(
        "(let [a (long-array 1) s (str a) p (pr-str a)]
           [(apply str (take 3 s)) (= s (str a)) (apply str (take 15 p)) (= p (str \"#object[\\\"[J\\\" 0x\" (apply str (drop 3 s)) \" \\\"\" s \"\\\"]\"))
            (apply str (take 3 (str (byte-array 1)))) (apply str (take 3 (str (short-array 1)))) (apply str (take 3 (str (int-array 1))))
            (apply str (take 3 (str (float-array 1)))) (apply str (take 3 (str (double-array 1))))])",
        "[\"[J@\" true \"#object[\\\"[J\\\" 0x\" true \"[B@\" \"[S@\" \"[I@\" \"[F@\" \"[D@\"]",
    );
    assert_printed(
        "[(= (type (long-array 1)) (type (long-array 2))) (= (type (long-array 1)) (type (int-array 1)))
          (= (type (long-array 1)) (type (array))) (type (double-array 1))]",
        "[true false false #object[[D]]",
    );
    assert_eq!(
        thrown("((type (long-array 1)))"),
        "\"A packed array type cannot be called; use its array function\""
    );
}
