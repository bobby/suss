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
        ("(loop [x 1.0] (if (< x 0.001) x (recur (/ x 2))))", "(loop [x (identity 1.0)] (if (< x 0.001) x (recur (/ x 2))))", "9.765625E-4"),
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

#[test]
fn bit_tests_keep_shift_masking_flags_and_boxed_fallbacks() {
    check(&[
        (
            "[(bit-test -9223372036854775808 63) (bit-test 9223372036854775807 63) (bit-test 1 64) (bit-test -1 -1) (bit-test 2 65)]",
            "[(bit-test (identity -9223372036854775808) 63) (bit-test (identity 9223372036854775807) 63) (bit-test (identity 1) 64) (bit-test (identity -1) -1) (bit-test (identity 2) 65)]",
            "[true false true true true]",
        ),
        (
            "(let [x -9223372036854775808 y 63 p (bit-test x y) q (bit-test 0 y)] [p q (if p :yes :no) (not q) ((fn [] p)) (loop [b p i 0] (if (< i 2) (recur (not b) (inc i)) b))])",
            "(let [x (identity -9223372036854775808) y (identity 63) p (bit-test x y) q (bit-test 0 y)] [p q (if p :yes :no) (not q) ((fn [] p)) (loop [b p i 0] (if (< i 2) (recur (not b) (inc i)) b))])",
            "[true false :yes true true true]",
        ),
        (
            "(let [x 1 n 0 b (bit-test x n)] (loop [b b i 0] (if (< i 1) (recur :replacement (inc i)) b)))",
            "(let [x (identity 1) n (identity 0) b (bit-test x n)] (loop [b b i 0] (if (< i 1) (recur :replacement (inc i)) b)))",
            ":replacement",
        ),
    ]);
    let mut sessions = sessions();
    for source in [
        "(bit-test 1.0 0)", "(bit-test 1 0.0)", "(bit-test 1 nil)",
        "(let [x (if (identity true) 1.0 1)] (bit-test x 0))",
    ] {
        assert!(outcome(&mut sessions, source).starts_with("[:error "), "{source}");
    }
    assert_eq!(outcome(&mut sessions,
        "(let [seen (atom [])] [(try (bit-test (do (swap! seen conj :left) 1.0) (do (swap! seen conj :right) 0)) (catch :default _ :rejected)) @seen])"),
        "[:rejected [:left :right]]");
}

/// Core's private 32-bit storage operations, compared with explicit JVM
/// unchecked-int/Integer.bitCount calculations, not public 64-bit bit ops.
#[test]
fn core_int32_operations_keep_low_bits_shift_masks_and_signed_results() {
    let body = "[(int x) (bit-and x y) (bit-or x y) (bit-xor x y)
                 (bit-shift-left x y) (bit-shift-right x y)
                 (bit-shift-right-zero-fill x y) (imul x y)
                 (suss.bootstrap/bit-count x)]";
    let pairs = "[[0 0] [1 31] [-1 1] [2147483648 32] [4294967295 -1]
                  [4294967297 33] [9007199254740993 65]
                  [9223372036854775807 63] [-9223372036854775808 -9223372036854775808]]";
    let expected = "[[0 0 0 0 0 0 0 0 0] [1 1 31 30 -2147483648 0 0 31 1] \
        [-1 1 -1 -2 -2 -1 2147483647 -1 32] \
        [-2147483648 0 -2147483616 -2147483616 -2147483648 -2147483648 2147483648 0 1] \
        [-1 -1 -1 0 -2147483648 -1 1 1 32] [1 1 33 32 2 0 0 33 1] \
        [1 1 65 64 2 0 0 65 1] [-1 63 -1 -64 -2147483648 -1 1 -63 32] \
        [0 0 0 0 0 0 0 0 0]]";
    let mut sessions = sessions();
    for session in &mut sessions {
        session.eval("(ns suss.core)").unwrap();
        session.eval(&format!(
            "(def core-bits-raw (fn [^long x ^long y] {body}))
             (def core-bits-boxed (fn [x y] {body}))
             (def core-bits-calls (atom []))
             (def ^{{:arglists '(^long [x])}} core-bits-left
               (fn ^long [x] (swap! core-bits-calls conj :left) x))
             (def ^{{:arglists '(^long [x])}} core-bits-right
               (fn ^long [x] (swap! core-bits-calls conj :right) x))
             (def core-bits-guarded (fn [x y]
               (let [x (core-bits-left x) y (core-bits-right y)] {body})))",
        )).unwrap();
    }
    for name in ["core-bits-raw", "core-bits-boxed", "core-bits-guarded"] {
        assert_eq!(outcome(&mut sessions,
            &format!("(mapv (fn [p] ({name} (nth p 0) (nth p 1))) {pairs})")),
            expected, "{name}");
    }
    assert_eq!(outcome(&mut sessions,
        "(do (reset! core-bits-calls []) (core-bits-guarded 1 31) @core-bits-calls)"),
        "[:left :right]");
    for session in &mut sessions {
        session.eval("(def core-bits-left
          (fn [x] (swap! core-bits-calls conj :left) \"4294967295\"))").unwrap();
        session.collect().unwrap();
    }
    assert_eq!(outcome(&mut sessions, "(core-bits-guarded 0 1)"),
        "[-1 1 -1 -2 -2 -1 2147483647 -1 32]");
    for session in &mut sessions {
        session.eval("(def core-bits-left
          (fn ^double [x] (swap! core-bits-calls conj :left) x))").unwrap();
        session.collect().unwrap();
    }
    assert_eq!(outcome(&mut sessions, "(core-bits-guarded 1.5 1)"),
        "[1 1 1 0 2 0 0 1 1]");
    for session in &mut sessions {
        session.eval("(def core-bits-right
          (fn [x] (swap! core-bits-calls conj :right) \"-1\"))").unwrap();
        session.collect().unwrap();
    }
    assert_eq!(outcome(&mut sessions, "(core-bits-guarded 1.5 0)"),
        "[1 1 -1 -2 -2147483648 0 0 -1 1]");
    assert_eq!(outcome(&mut sessions,
        "(do (reset! core-bits-calls []) (core-bits-guarded 1.5 0) @core-bits-calls)"),
        "[:left :right]");
    for session in &mut sessions {
        session.eval("(def core-bits-left
          (fn ^long [x] (swap! core-bits-calls conj :left) x))").unwrap();
        session.collect().unwrap();
    }
    assert_eq!(outcome(&mut sessions, "(core-bits-guarded 4294967295 0)"),
        "[-1 -1 -1 0 -2147483648 -1 1 1 32]");
    assert_eq!(outcome(&mut sessions,
        "[(let [x (if (identity true) 1.5 1)] (bit-and x 3))
          (let [x (if (identity true) -1.5 1)] (bit-and x 3))
          (bit-and ##NaN 3) (bit-shift-right-zero-fill ##Inf 1)
          (loop [x 4294967297 i 0] (if (< i 2) (recur (bit-shift-left x 1) (inc i)) x))]"),
        "[1 3 0 0 4]");
    for session in &mut sessions {
        session.eval("(def core-boxed-chain (fn [x]
            (let [a (bit-shift-right-zero-fill x 0) b (+ a 1)]
              [(< a b) (bit-and b 255) a b a])))").unwrap();
    }
    assert_eq!(outcome(&mut sessions,
        "(mapv core-boxed-chain [4294967295 9007199254740993 1.5 ##NaN \"2\"] )"),
        "[[true 0 4294967295 4294967296 4294967295] \
          [true 2 1 2 1] [true 2 1 2 1] [true 1 0 1 0] [true 3 2 3 2]]");
}

#[test]
fn guarded_numeric_comparisons_keep_redefinitions_and_evaluation_order() {
    let mut sessions = sessions();
    for session in &mut sessions {
        session.eval("(def comparison-trace (atom []))
          (def ^{:arglists '(^long [x])} comparison-left (fn ^long [x] (swap! comparison-trace conj :left) x))
          (def ^{:arglists '(^double [x])} comparison-right (fn ^double [x] (swap! comparison-trace conj :right) x))
          (def comparison-pair (fn [x y]
            (let [a (comparison-left x) b (comparison-right y)]
              [(< a b) (<= a b) (> a b) (>= a b) (== a b)])))").unwrap();
    }
    assert_eq!(outcome(&mut sessions, "[(comparison-pair 2 2.5) @comparison-trace]"),
        "[[true true false false false] [:left :right]]");
    assert_eq!(outcome(&mut sessions, "(comparison-pair 2 ##NaN)"),
        "[false false false false false]");
    assert_eq!(outcome(&mut sessions, "(comparison-pair 0 -0.0)"),
        "[false true false true true]");
    for session in &mut sessions {
        session.eval("(def comparison-wide (fn [x y]
            (< (comparison-left x) (comparison-left y))))").unwrap();
    }
    assert_eq!(outcome(&mut sessions,
        "(comparison-wide 9007199254740992 9007199254740993)"), "true");
    for session in &mut sessions {
        session.eval("(def comparison-left (fn [x] (swap! comparison-trace conj :left) \"v\"))").unwrap();
    }
    assert_eq!(outcome(&mut sessions, "(do (reset! comparison-trace [])
        [(try (comparison-pair 2 2.5) (catch :default e :bad)) @comparison-trace])"),
        "[:bad [:left :right]]");
    for session in &mut sessions {
        session.eval("(def comparison-left (fn ^double [x] (swap! comparison-trace conj :left) x))").unwrap();
    }
    assert_eq!(outcome(&mut sessions, "(do (reset! comparison-trace [])
        [(comparison-pair 2.75 2.5) @comparison-trace])"),
        "[[false false true true false] [:left :right]]");
}

#[test]
fn core_known_numeric_math_keeps_overflow_nan_zero_and_coercing_fallbacks() {
    let longs = "[(try (+ x y) (catch :default e (if (= (ex-message e) \"long overflow\") :overflow [:unexpected (ex-message e)])))
                  (try (- x y) (catch :default e (if (= (ex-message e) \"long overflow\") :overflow [:unexpected (ex-message e)])))
                  (try (* x y) (catch :default e (if (= (ex-message e) \"long overflow\") :overflow [:unexpected (ex-message e)])))
                  (< x y) (<= x y) (> x y) (>= x y) (== x y)]";
    // Division is deliberately separate: JVM boxed/primitive zero-divisor
    // overloads differ, and the boxed path has an independently tracked gap.
    let doubles = "[(+ x y) (- x y) (* x y) (< x y) (<= x y) (> x y) (>= x y) (== x y)]";
    let mut sessions = sessions();
    for session in &mut sessions {
        session.eval("(ns suss.core)").unwrap();
        session.eval(&format!(
            "(def core-math-longs (fn [^long x ^long y] {longs}))
             (def core-math-doubles (fn [^double x ^double y] {doubles}))
             (def core-math-boxed-longs (fn [x y] {longs}))
             (def core-math-boxed-doubles (fn [x y] {doubles}))",
        )).unwrap();
    }
    for name in ["core-math-longs", "core-math-boxed-longs"] {
        assert_eq!(outcome(&mut sessions, &format!(
            "(mapv (fn [p] ({name} (nth p 0) (nth p 1)))
               [[6 2] [3000000000 4] [9007199254740992 9007199254740993]
                [9223372036854775807 1] [-9223372036854775808 -1]])")),
            "[[8 4 12 false false true true false] \
              [3000000004 2999999996 12000000000 false false true true false] \
              [18014398509481985 -1 :overflow true true false false false] \
              [:overflow 9223372036854775806 9223372036854775807 false false true true false] \
              [:overflow -9223372036854775807 :overflow true true false false false]]");
    }
    for name in ["core-math-doubles", "core-math-boxed-doubles"] {
        assert_eq!(outcome(&mut sessions, &format!(
            "(mapv (fn [p] ({name} (nth p 0) (nth p 1)))
               [[1.5 2.0] [0.0 -0.0] [##NaN 1.0] [##Inf 0.0]])")),
            "[[3.5 -0.5 3.0 true true false false false] \
              [0.0 0.0 -0.0 false true false true true] \
              [##NaN ##NaN ##NaN false false false false false] \
              [##Inf ##Inf ##NaN false false true true false]]");
    }
    assert_eq!(outcome(&mut sessions,
        "[(+ \"x\" 1) (< \"10\" \"2\") (+ nil 1) (+ true 1)
          (< (identity \"10\") (identity 2))
          (loop [x 0 i 0] (if (< i 1) (recur \"x\" (inc i)) (+ x 1)))]"),
        "[\"x1\" true 1.0 2.0 false \"x1\"]");
    for session in &mut sessions {
        session.eval("(def ^{:arglists '(^long [^long x])} core-hinted-result (fn ^long [^long x] (+ x 1)))
            (def core-use-hinted-result (fn [x] (+ (core-hinted-result x) 1)))
            (def core-compare-hinted-result (fn [x]
              (let [n (core-hinted-result x)]
                [(< n 5) (> n 1) (<= n 3) (>= n 3) (== n 3)])))").unwrap();
    }
    assert_eq!(outcome(&mut sessions, "(core-use-hinted-result 2)"), "4");
    assert_eq!(outcome(&mut sessions, "(core-compare-hinted-result 2)"), "[true true true true true]");
    for session in &mut sessions {
        session.eval("(def core-hinted-result (fn [x] \"v\"))").unwrap();
    }
    assert_eq!(outcome(&mut sessions, "(core-use-hinted-result 2)"), "\"v1\"");
    assert_eq!(outcome(&mut sessions, "(core-compare-hinted-result 2)"), "[false false false false false]");
    for session in &mut sessions {
        session.eval("(def core-hinted-result (fn ^double [x] (+ x 0.5)))").unwrap();
    }
    assert_eq!(outcome(&mut sessions, "(core-use-hinted-result 2)"), "3.5");
    assert_eq!(outcome(&mut sessions, "(core-compare-hinted-result 2)"), "[true true true false false]");
    for session in &mut sessions {
        session.eval("(def core-hinted-result (fn [x] false))").unwrap();
    }
    assert_eq!(outcome(&mut sessions, "(core-use-hinted-result 2)"), "1.0");
    assert_eq!(outcome(&mut sessions, "(core-compare-hinted-result 2)"), "[true false true false false]");
    for session in &mut sessions {
        session.eval("(def ^{:arglists '(^long [x])} core-hinted-left (fn ^long [x] x))
          (def ^{:arglists '(^double [x])} core-hinted-right (fn ^double [x] x))
          (def core-compare-two-hints (fn [x y] (< (core-hinted-left x) (core-hinted-right y))))").unwrap();
    }
    assert_eq!(outcome(&mut sessions, "(core-compare-two-hints 1 2.0)"), "true");
    for session in &mut sessions {
        session.eval("(def core-hinted-right (fn [x] \"0\"))").unwrap();
    }
    assert_eq!(outcome(&mut sessions, "(core-compare-two-hints 1 2.0)"), "false");
    for session in &mut sessions {
        session.eval("(def core-hinted-left (fn [x] \"-1\"))").unwrap();
    }
    assert_eq!(outcome(&mut sessions, "(core-compare-two-hints 1 2.0)"), "true");
}
