//! Core's numeric coercions compile inline as raw results (#448): `long`,
//! `double`, `unchecked-long` and `unchecked-double` answer and throw as a
//! call of the var does, for raw and boxed longs and doubles and other
//! values. Ratios and BigInts have no literals yet (#338, #339).
use suss_cli::portable_session::Session;

fn printed(session: &mut Session, source: &str) -> String {
    let value = session
        .eval(&format!("(pr-str {source})"))
        .unwrap_or_else(|error| panic!("{source}: {error}"));
    let units = session
        .inspect(&value, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(array.elems(&mut store)?.map(|unit| unit.unwrap_i32() as u16).collect::<Vec<_>>())
        })
        .unwrap();
    String::from_utf16(&units).unwrap()
}

const VALUES: &str = "[0 1 -1 1073741823 1073741824 -1073741825 9223372036854775807 -9223372036854775808
  1.5 -1.5 2.5 -0.0 0.0 1.0e30 -1.0e30 9.2233720368547758E18 ##NaN ##Inf ##-Inf
  nil \"x\" :k [] (atom 1)]";

#[test]
fn coercions_answer_as_calls_do() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval(
                "(def outcome (fn [f] (try (f) (catch :default e :threw))))
                 (def inline (fn [x] [(outcome (fn [] (long x))) (outcome (fn [] (double x)))
                                      (outcome (fn [] (unchecked-long x))) (outcome (fn [] (unchecked-double x)))]))
                 (def called (fn [x] [(outcome (fn [] (apply long [x]))) (outcome (fn [] (apply double [x])))
                                      (outcome (fn [] (apply unchecked-long [x]))) (outcome (fn [] (apply unchecked-double [x])))]))",
            )
            .unwrap();
        let inline = printed(&mut session, &format!("(mapv inline {VALUES})"));
        let called = printed(&mut session, &format!("(mapv called {VALUES})"));
        assert_eq!(inline, called);
        // Spot checks against Clojure's answers.
        for (source, expected) in [
            ("(long 1.5)", "1"),
            ("(long -1.5)", "-1"),
            ("(double 3)", "3.0"),
            ("(double -0.0)", "-0.0"),
            ("(unchecked-long 1.0e30)", "9223372036854775807"),
            ("(unchecked-long ##NaN)", "0"),
            ("(try (long 1.0e30) (catch :default e :threw))", ":threw"),
            ("(try (long nil) (catch :default e :threw))", ":threw"),
            ("(try (double \"x\") (catch :default e :threw))", ":threw"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}

#[test]
fn raw_operands_coerce_inline() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for (source, expected) in [
            // A raw long counter widened, added and truncated.
            ("(loop [i 0 acc 0.0] (if (< i 1000) (recur (inc i) (+ acc (double i) 0.5)) acc))", "500000.0"),
            ("(loop [i 0 acc 0] (if (< i 100) (recur (inc i) (+ acc (unchecked-long (+ (double i) 0.75)))) acc))", "4950"),
            ("(loop [i 0 acc 0] (if (< i 100) (recur (inc i) (+ acc (long i))) acc))", "4950"),
            // A raw double out of a long's range still throws when checked.
            ("(try (loop [d 1.0 n 0] (if (< n 70) (recur (* d 2.0) (inc n)) (long d))) (catch :default e :threw))", ":threw"),
            ("(loop [d 1.0 n 0] (if (< n 70) (recur (* d 2.0) (inc n)) (unchecked-long d)))", "9223372036854775807"),
            // Past the i31 range, and the overflow checks around it.
            ("(let [x (long 1073741824)] [(* x 2) (inc x) (double x)])", "[2147483648 1073741825 1.073741824E9]"),
            ("(try (inc (long 9223372036854775807)) (catch :default e :threw))", ":threw"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}

#[test]
fn primitive_return_hints_cast_after_the_body_in_both_phases() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def ret-long (fn ^long [x] x))
                      (def ret-double (fn ^double [x] x))
                      (def ret-multi (fn (^long [x] x) (^double [x y] (+ x y))))
                      (def ret-recur (fn again ^long [n x] (if (zero? n) x (recur (dec n) x))))
                      (def ^long ret-plain (fn [x] x))
                      (def outcome (fn [f] (try (f) (catch :default e :threw))))").unwrap();
        for (source, expected) in [
            ("[(ret-long 1.75) (apply ret-long [1.75]) (ret-double 3) (apply ret-double [3])]", "[1 1 3.0 3.0]"),
            ("[(ret-multi 2.75) (ret-multi 2 3) (ret-recur 10000 4.75)]", "[2 5.0 4]"),
            ("(apply ret-plain [1.75])", "1.75"),
            ("[(ret-double -0.0) (ret-double ##NaN) (ret-double ##Inf) (ret-long 9223372036854775807)]", "[-0.0 ##NaN ##Inf 9223372036854775807]"),
            ("[(ret-long 1.0e30) (ret-long -1.0e30) (ret-long ##Inf) (ret-long ##-Inf) (ret-long ##NaN)]", "[9223372036854775807 -9223372036854775808 9223372036854775807 -9223372036854775808 0]"),
            ("[(outcome (fn [] (ret-long nil))) (outcome (fn [] (ret-double :k))) (outcome (fn [] ((fn [^long x] x) 1.0e30)))]", "[:threw :threw :threw]"),
            // A return conversion runs only after the body has completed.
            ("(let [a (atom [])] [(outcome (fn [] ((fn ^long [] (swap! a conj :body) nil)))) @a])", "[:threw [:body]]"),
            // The body's handler does not catch a later return conversion.
            ("(outcome (fn [] ((fn ^long [] (try nil (catch :default e 7))))))", ":threw"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}

#[test]
fn checked_long_of_a_raw_double_matches_the_boxed_cast() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def raw-long (fn [^double x] (long x)))
                      (def outcome (fn [f x] (try (f x) (catch :default e (.-message e)))))").unwrap();
        // JVM Clojure 1.12.6: NaN -> 0, 2^63 -> MAX, and comparisons
        // against the original double reject the next value on either side.
        for value in ["1.75", "-1.75", "0.0", "-0.0", "##NaN",
                      "9223372036854775808.0", "-9223372036854775808.0",
                      "9223372036854777856.0", "-9223372036854777856.0",
                      "##Inf", "##-Inf", "1.0e30", "-1.0e30"] {
            assert_eq!(printed(&mut session, &format!("(outcome raw-long {value})")),
                       printed(&mut session, &format!("(outcome (fn [x] (apply long [x])) {value})")), "{value}");
        }
    }
}

#[test]
fn return_hints_reject_opposite_source_primitives_but_unbox_objects() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // defn's declaration facts without requiring macro expansion in Session.
        session.eval("(def ^{:tag long :arglists '(^long [])} primitive-global-long (fn ^long [] 1))
                      (def ^{:tag double :arglists '(^double [])} primitive-global-double (fn ^double [] 1.0))
                      (def ^{:tag long :arglists '([x])} name-default-long (fn ^long [x] x))
                      (def plain-global-long (fn ^long [] 1))").unwrap();
        for source in [
            "(fn ^long [] 1.75)", "(fn ^double [] 1)",
            "(fn ^long [^double x] x)", "(fn ^double [^long x] x)",
            "(fn ^long [] (let [x 1.75] x))",
            "(fn ^long [x] (+ x 0.5))", "(fn ^long [x] (+ x 1 0.5))",
            "(fn ^long [x] (double x))", "(fn ^double [x] (long x))",
            "(fn ^long [x] (nil? x))", "(fn ^long [x] (zero? x))",
            "(fn ^double [x] (count x))",
            "(fn ^double [] (primitive-global-long))", "(fn ^long [] (primitive-global-double))",
            "(fn ^double [^longs a] (aget a 0))", "(fn ^long [^doubles a] (aget a 0))",
            "(fn ^double [b] (loop [i 0] (if b (recur (int i)) i)))",
            "(fn ^double [b] (loop [i (int 0)] (if b (recur (long i)) i)))",
        ] {
            let error = session.eval(source).err().unwrap_or_else(|| panic!("accepted {source}"));
            assert!(error.to_string().contains("Mismatched primitive return"), "{source}: {error}");
        }
        for (source, expected) in [
            ("((fn ^double [x] (name-default-long x)) 3.75)", "3.0"),
            ("((fn ^long [] (identity 1.75)))", "1"),
            ("((fn ^double [] (identity 1)))", "1.0"),
            ("((fn ^long [] (if (identity true) 1.75 nil)))", "1"),
            ("((fn ^long [] (try 1.75 (catch :default e 2.75))))", "1"),
            ("((fn ^double [] (try 1 (catch :default e 2))))", "1.0"),
            ("((fn ^long [^double x] (+ x)) 1.75)", "1"),
            ("((fn ^long [x] (quot x 0.5)) 1.75)", "3"),
            ("((fn ^double [] (let [^double x (identity 3)] x)))", "3.0"),
            ("(double? ((fn ^double [x] (hash x)) 7))", "true"),
            ("((fn ^double [] (plain-global-long)))", "1.0"),
            ("((fn ^long [b] (if b (throw 7) 1.5)) false)", "1"),
            ("((fn ^double [b] (if b 1 (throw 7))) true)", "1.0"),
            ("((fn ^double [n] (loop [i 0 x 0] (if (< i n) (recur (inc i) (+ x 0.5)) x))) 3)", "1.5"),
            ("((fn ^long [n] (loop [i 0 x 0.5] (if (< i n) (recur (inc i) (inc i)) x))) 3)", "3"),
            ("(try ((fn ^double [] (-))) (catch :default e :arity))", ":arity"),
            ("(try ((fn ^double [] (/))) (catch :default e :arity))", ":arity"),
            ("((fn ^double [a] (aget a 0)) (long-array [3]))", "3.0"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}

#[test]
fn guarded_primitive_calls_preserve_replacement_values_and_numeric_joins() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def ^{:arglists '(^long [^long x])} typed-live (fn ^long [^long x] (* x 2)))
                      (def live-pass (fn [^long n] (typed-live n)))
                      (def live-add (fn [^long n] (+ 1 (typed-live n))))
                      (def live-loop (fn [^long n] (loop [i 0 acc 0] (if (< i n) (recur (inc i) (+ acc (typed-live i))) acc))))
                      (def live-capture (fn [^long n] (let [x (typed-live n)] (fn [] x))))
                      (def live-cast (fn ^double [^long n] (double (typed-live n))))
                      (def saved-capture (live-capture 3000000000))").unwrap();
        assert_eq!(printed(&mut session, "[(live-pass 3000000000) (live-add 3000000000) (live-loop 4) (saved-capture) (live-cast 3)]"),
            "[6000000000 6000000001 12 6000000000 6.0]");
        session.eval("(def typed-live (fn [x] (+ x 0.5)))").unwrap();
        assert_eq!(printed(&mut session, "[(live-pass 3) (live-add 3) (live-loop 4) ((live-capture 3)) (saved-capture) (live-cast 3)]"),
            "[3.5 4.5 8.0 3.5 6000000000 3.5]");
        session.eval("(def typed-live (fn [x] [:changed x]))").unwrap();
        assert_eq!(printed(&mut session, "[(live-pass 3) ((live-capture 3)) (try (live-add 3) (catch :default e :threw))]"),
            "[[:changed 3] [:changed 3] :threw]");
        session.eval("(def typed-live (fn ^long [^long x] (* x 3)))").unwrap();
        assert_eq!(printed(&mut session, "[(live-pass 3) (live-add 3) (live-loop 4)]"), "[9 10 18]");
        session.eval("(def ^{:arglists '([^long x y])} typed-pair (fn [^long x y] [x y]))
                      (def pair-pass (fn [] (typed-pair 2 3)))").unwrap();
        assert_eq!(printed(&mut session, "(pair-pass)"), "[2 3]");
        // A variadic body has the same raw formal types, but its last formal
        // is a seq. The old fixed site must go through the new adapter.
        session.eval("(def typed-pair (fn [^long x & more] [x more]))").unwrap();
        assert_eq!(printed(&mut session, "[(pair-pass) (apply typed-pair [2 3])]"), "[[2 (3)] [2 (3)]]");
    }
}

// JVM Clojure 1.12.6 source kinds, independent of the backend's i64/f64
// representation. The review's full oracle matrix is retained in target/probe.
fn primitive_source_case(session: &mut Session, parameters: &str, body: &str, accepted: [bool; 2]) {
    for (tag, accepted) in ["long", "double"].into_iter().zip(accepted) {
        let source = format!("(fn ^{tag} [{parameters}] {body})");
        let result = session.eval(&source);
        if accepted { result.unwrap_or_else(|error| panic!("{source}: {error}")); }
        else {
            let error = result.err().unwrap_or_else(|| panic!("accepted {source}"));
            assert!(error.to_string().contains("Mismatched primitive return"), "{source}: {error}");
        }
    }
}

#[test]
fn unary_and_unchecked_source_results_follow_clojure_overloads() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for operation in ["inc", "dec", "abs", "unchecked-inc", "unchecked-dec", "unchecked-negate"] {
            for (operand, accepted) in [
                ("x", [true, true]), ("(long x)", [true, false]),
                ("(int x)", [true, false]), ("(double x)", [false, true]),
                ("(float x)", [false, true]),
            ] {
                primitive_source_case(&mut session, "x", &format!("({operation} {operand})"), accepted);
            }
        }
        for operation in ["unchecked-add", "unchecked-subtract", "unchecked-multiply"] {
            for (operands, accepted) in [
                ("x y", [true, true]), ("(long x) y", [true, true]),
                ("x (long y)", [true, true]), ("(double x) y", [false, true]),
                ("x (double y)", [false, true]), ("(long x) (long y)", [true, false]),
                ("(int x) (int y)", [true, false]), ("(float x) (float y)", [false, true]),
            ] {
                primitive_source_case(&mut session, "x y", &format!("({operation} {operands})"), accepted);
            }
        }
        // Boxed operands remain boxed source calls, even when execution
        // happens to return a double or a long before return unboxing.
        assert_eq!(printed(&mut session, "[((fn ^long [x] (inc x)) 2.75) ((fn ^double [x] (abs x)) -3) ((fn ^double [x y] (unchecked-add x y)) 2 3)]"), "[3 3.0 5.0]");
    }
}

#[test]
fn packed_array_source_kinds_are_distinct_from_backend_representations() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for (hint, cast, accepted, loop_accepted) in [
            ("bytes", "byte", [false, false], [false, false]),
            ("shorts", "short", [false, false], [false, false]),
            ("ints", "int", [true, false], [true, false]),
            ("floats", "float", [false, true], [false, true]),
            ("longs", "long", [true, false], [true, false]),
            ("doubles", "double", [false, true], [false, true]),
            ("objects", "identity", [true, true], [true, true]),
        ] {
            let parameters = format!("^{hint} a");
            primitive_source_case(&mut session, &parameters, "(aget a 0)", accepted);
            primitive_source_case(&mut session, &parameters, &format!("(aset a 0 ({cast} 3))"), accepted);
            primitive_source_case(&mut session, &parameters, "(let [x (aget a 0)] x)", accepted);
            primitive_source_case(&mut session, &parameters, "(loop [x (aget a 0)] x)", loop_accepted);
            primitive_source_case(&mut session, &parameters, "(identity (aget a 0))", [true, true]);
        }
        primitive_source_case(&mut session, "^floats a b", "(loop [x (aget a 0)] (if b (recur (aget a 0)) x))", [false, true]);
        primitive_source_case(&mut session, "^floats a b", "(loop [x (aget a 0)] (if b (recur (double x)) x))", [false, true]);
        primitive_source_case(&mut session, "^floats a b", "(loop [x 0.0] (if b (recur (aget a 0)) x))", [false, true]);
        primitive_source_case(&mut session, "^ints a b", "(loop [x (aget a 0)] (if b (recur (aget a 0)) x))", [true, false]);
        assert_eq!(printed(&mut session, "[((fn ^long [^ints a] (aget a 0)) (int-array [3])) ((fn ^double [^floats a] (aget a 0)) (float-array [3]))]"), "[3 3.0]");
    }
}

#[test]
fn boxed_bit_functions_do_not_inherit_backend_primitive_kinds() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        primitive_source_case(&mut session, "x", "(bit-test x 1)", [true, true]);
        for operation in ["bit-clear", "bit-flip", "bit-set"] {
            primitive_source_case(&mut session, "x", &format!("({operation} x 1)"), [true, true]);
            primitive_source_case(&mut session, "^long x", &format!("({operation} x 1)"), [true, true]);
        }
        primitive_source_case(&mut session, "x", "(bit-and x 1)", [true, false]);
        primitive_source_case(&mut session, "x", "(bit-or x 1)", [true, false]);
        primitive_source_case(&mut session, "x", "(bit-shift-left x 1)", [true, false]);
        assert_eq!(printed(&mut session, "[(try ((fn ^long [x] (bit-test x 1)) 3) (catch :default e :threw)) (try ((fn ^double [x] (bit-test x 1)) 3) (catch :default e :threw))]"), "[:threw :threw]");
        assert_eq!(printed(&mut session, "[((fn ^double [^long x] (bit-clear x 1)) 4) ((fn ^double [^long x] (bit-flip x 1)) 4) ((fn ^double [^long x] (bit-set x 1)) 4)]"), "[4.0 6.0 6.0]");
    }
}

#[test]
fn array_length_source_results_require_a_known_array_class() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        primitive_source_case(&mut session, "a", "(alength a)", [true, true]);
        primitive_source_case(&mut session, "^ints a", "(alength a)", [true, false]);
        primitive_source_case(&mut session, "^objects a", "(alength a)", [true, false]);
        primitive_source_case(&mut session, "a", "(alength ^ints a)", [true, false]);
        primitive_source_case(&mut session, "", "(alength (int-array 3))", [true, false]);
        primitive_source_case(&mut session, "", "(alength (object-array 3))", [true, false]);
        primitive_source_case(&mut session, "", "(let [a (int-array 3)] (alength a))", [true, false]);
        primitive_source_case(&mut session, "", "(loop [a (int-array 3)] (alength a))", [true, false]);
        primitive_source_case(&mut session, "b", "(loop [a (int-array 3)] (if b (recur (object-array 3)) (alength a)))", [true, false]);
        primitive_source_case(&mut session, "b", "(loop [a (int-array 3)] (if b (recur :changed) (alength a)))", [true, false]);
        primitive_source_case(&mut session, "b", "(alength (if b (int-array 3) nil))", [true, false]);
        primitive_source_case(&mut session, "b", "(alength (if b nil (int-array 3)))", [true, false]);
        primitive_source_case(&mut session, "b", "(alength (if b (int-array 3) (int-array 4)))", [true, false]);
        primitive_source_case(&mut session, "b", "(alength (if b (int-array 3) (object-array 3)))", [true, true]);
        primitive_source_case(&mut session, "", "(alength (identity (int-array 3)))", [true, true]);
        assert_eq!(printed(&mut session, "[((fn ^long [^ints a] (alength a)) (int-array 3)) ((fn ^double [a] (alength a)) (int-array 3))]"), "[3 3.0]");
    }
}

#[test]
fn shared_array_initializers_do_not_expand_source_kind_work_exponentially() {
    let mut body = "(let [a0 (int-array 3)".to_owned();
    for i in 1..=28 { body.push_str(&format!(" a{i} (if b a{} a{})", i - 1, i - 1)); }
    body.push_str("] (alength a28))");
    let source = format!("(let [f (fn ^long [b] {body})] [(f true) (f false)])");
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // Default compilation fuel; the linear-size shared graph must not
        // require visiting its 2^28 expanded initializer paths.
        assert_eq!(printed(&mut session, &source), "[3 3]");
    }
}
