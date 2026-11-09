//! Unboxed numeric locals give the boxed path's results (#337). Each case is
//! computed once from literals, `let` and `loop` bindings the compiler keeps
//! raw, and once from the same numbers passed through `identity`, which the
//! compiler can't see through. Expected strings are JVM Clojure 1.12's, except
//! for operand type errors, which use Suss's messages on both paths.
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
/// `source`'s printed value, or `[:error message]`, in both phases' sessions.
fn outcome(sessions: &mut [Session; 2], source: &str) -> String {
    let [first, second] = sessions.each_mut().map(|session| {
        let value = session
            .eval(&format!(
                "(pr-str (try {source} (catch :default error [:error (ex-message error)])))"
            ))
            .unwrap_or_else(|error| panic!("{source}: {error}"));
        units(session, &value)
    });
    assert_eq!(first, second, "{source}");
    first
}
fn sessions() -> [Session; 2] {
    [Session::new_repl().unwrap(), Session::new_macro().unwrap()]
}
/// Each `(unboxed, boxed, expected)` case gives `expected` on both paths.
fn check(cases: &[(&str, &str, &str)]) {
    let mut sessions = sessions();
    for (unboxed, boxed, expected) in cases {
        assert_eq!(outcome(&mut sessions, unboxed), *expected, "{unboxed}");
        assert_eq!(outcome(&mut sessions, boxed), *expected, "{boxed}");
    }
}

#[test]
fn long_overflow_throws_on_both_paths() {
    check(&[
        ("(let [x 9223372036854775807] (+ x 1))", "(+ (identity 9223372036854775807) 1)", "[:error \"long overflow\"]"),
        ("(let [x -9223372036854775808] (- x))", "(- (identity -9223372036854775808))", "[:error \"long overflow\"]"),
        ("(let [x -9223372036854775808] (- x 1))", "(- (identity -9223372036854775808) 1)", "[:error \"long overflow\"]"),
        ("(let [x 3037000500] (* x x))", "(* (identity 3037000500) 3037000500)", "[:error \"long overflow\"]"),
        ("(let [x -9223372036854775808] (* x -1))", "(* (identity -9223372036854775808) -1)", "[:error \"long overflow\"]"),
        ("(let [x -9223372036854775808] (* -1 x))", "(* -1 (identity -9223372036854775808))", "[:error \"long overflow\"]"),
        ("(let [x 9223372036854775807] (inc x))", "(inc (identity 9223372036854775807))", "[:error \"long overflow\"]"),
        ("(let [x -9223372036854775808] (dec x))", "(dec (identity -9223372036854775808))", "[:error \"long overflow\"]"),
        ("(let [x 4611686018427387904] [(+ x x -1) (* 2 (dec x))])", "(let [x (identity 4611686018427387904)] [(+ x x -1) (* 2 (dec x))])", "[:error \"long overflow\"]"),
        (
            "(let [x (dec 0) y (inc 9223372036854775806)] [x y (dec -9223372036854775807) (* 3037000499 3037000499)])",
            "[(dec (identity 0)) (inc (identity 9223372036854775806)) (dec (identity -9223372036854775807)) (* (identity 3037000499) 3037000499)]",
            "[-1 9223372036854775807 -9223372036854775808 9223372030926249001]",
        ),
    ]);
}

#[test]
fn doubles_keep_nan_and_signed_zero() {
    check(&[
        ("(let [x 1.5] (* x x))", "(* (identity 1.5) 1.5)", "2.25"),
        ("(let [x (if (pos? 1) 1.5 2.5)] (* x x))", "(let [x (identity 1.5)] (* x x))", "2.25"),
        ("(let [x 0.0] [(- x) (* x -1) (+ (- x) 0) (- x 0.0)])", "(let [x (identity 0.0)] [(- x) (* x -1) (+ (- x) 0) (- x 0.0)])", "[-0.0 -0.0 0.0 0.0]"),
        (
            "(let [x ##NaN] [(< x 1) (== x x) (> x 1) (<= x x) (>= 1 x) (zero? x) (pos? x) (neg? x)])",
            "(let [x (identity ##NaN)] [(< x 1) (== x x) (> x 1) (<= x x) (>= 1 x) (zero? x) (pos? x) (neg? x)])",
            "[false false false false false false false false]",
        ),
        (
            "(let [x (- 5)] [(zero? x) (pos? x) (neg? x) (zero? 0.0) (neg? -0.0) (zero? -0.0)])",
            "(let [x (identity -5)] [(zero? x) (pos? x) (neg? x) (zero? (identity 0.0)) (neg? (identity -0.0)) (zero? (identity -0.0))])",
            "[false false true true false true]",
        ),
        (
            "(let [x 7.0 y 0 z -7.0 w 0.0] [(/ x y) (/ z w) (/ w w)])",
            "(let [x (identity 7.0) y (identity 0) z (identity -7.0) w (identity 0.0)] [(/ x y) (/ z w) (/ w w)])",
            "[##Inf ##-Inf ##NaN]",
        ),
    ]);
}

#[test]
fn longs_and_doubles_mix_as_in_clojure() {
    check(&[
        (
            "(let [x 1 y 1.0] [(== x y) (< x 1.5) (+ x y) (- x y) (* x y) (/ x y) (/ y x)])",
            "(let [x (identity 1) y (identity 1.0)] [(== x y) (< x 1.5) (+ x y) (- x y) (* x y) (/ x y) (/ y x)])",
            "[true true 2.0 0.0 1.0 1.0 1.0]",
        ),
        ("(let [x 6 y 2] (/ x y))", "(/ (identity 6) 2)", "3"),
        ("(let [x 7 y 0] (/ x y))", "(/ (identity 7) 0)", "[:error \"Divide by zero\"]"),
        (
            "(let [x 3000000000] [(< x 3000000001) (< x (identity 3000000001)) (< 1 (identity 2.5)) (== x 3.0E9) (> 2 (identity 1))])",
            "(let [x (identity 3000000000)] [(< x 3000000001) (< x (identity 3000000001)) (< 1 (identity 2.5)) (== x 3.0E9) (> 2 (identity 1))])",
            "[true true true true true]",
        ),
        ("(let [x 2 y (if (pos? x) 1 2.0)] (+ x y))", "(let [x (identity 2) y (if (pos? x) 1 2.0)] (+ x y))", "3"),
    ]);
}

#[test]
fn loops_keep_counters_raw_and_box_where_values_escape() {
    check(&[
        ("(loop [i 0 acc 0] (if (< i 10) (recur (inc i) (+ acc i)) acc))", "(loop [i (identity 0) acc (identity 0)] (if (< i 10) (recur (inc i) (+ acc i)) acc))", "45"),
        // A recur of another kind boxes the loop local, as Clojure does.
        ("(loop [i 0] (if (< i 3) (recur (+ i 0.5)) i))", "(loop [i (identity 0)] (if (< i 3) (recur (+ i 0.5)) i))", "3.0"),
        (
            "(let [f (fn [n] (loop [i 0 c 3000000000] (if (< i n) (recur (inc i) (+ c i)) c)))] [(f 100) (f 100.5)])",
            "(let [f (fn [n] (loop [i (identity 0) c (identity 3000000000)] (if (< i n) (recur (inc i) (+ c i)) c)))] [(f 100) (f 100.5)])",
            "[3000004950 3000005050]",
        ),
        ("(loop [i 0 acc []] (if (< i 3) (recur (inc i) (conj acc (* i 1.5))) acc))", "(loop [i (identity 0) acc []] (if (< i 3) (recur (inc i) (conj acc (* i 1.5))) acc))", "[0.0 1.5 3.0]"),
        // Closures capture boxed copies of raw locals.
        ("(let [x 5 y 2.5 g (fn [] [x y])] (g))", "(let [x (identity 5) y (identity 2.5) g (fn [] [x y])] (g))", "[5 2.5]"),
        (
            "(let [x 1000000000000] [x (str x) (= x 1000000000000) (hash x)])",
            "(let [x (identity 1000000000000)] [x (str x) (= x 1000000000000) (hash x)])",
            "[1000000000000 \"1000000000000\" true -1510912948]",
        ),
        ("(let [b (< 1 2)] [b (if b :yes :no) (str b)])", "(let [b (< (identity 1) 2)] [b (if b :yes :no) (str b)])", "[true :yes \"true\"]"),
    ]);
}

#[test]
fn non_numbers_throw_as_on_the_boxed_path() {
    let mut sessions = sessions();
    for (unboxed, boxed) in [
        ("(let [n \"a\"] (< 1 n))", "(< (identity 1) \"a\")"),
        ("(let [x 1] (+ x nil))", "(+ (identity 1) nil)"),
        ("(let [x 1.5] (< x nil))", "(< (identity 1.5) nil)"),
        ("(let [x 3000000000] (> x :k))", "(> (identity 3000000000) :k)"),
        ("(loop [i 0] (if (< i :k) (recur (inc i)) i))", "(loop [i (identity 0)] (if (< i :k) (recur (inc i)) i))"),
    ] {
        let expected = outcome(&mut sessions, boxed);
        assert!(expected.starts_with("[:error "), "{boxed}: {expected}");
        assert_eq!(outcome(&mut sessions, unboxed), expected, "{unboxed}");
    }
}
