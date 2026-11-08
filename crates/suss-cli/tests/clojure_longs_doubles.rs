//! Clojure's longs and doubles (#335): exact 64-bit long arithmetic with
//! overflow checks, Java double printing, exact reading, and the long/double
//! distinction through macro data. Expected strings are JVM Clojure 1.12's.
use suss_cli::portable_macros::CompiledMacros;
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
fn long_arithmetic_is_exact_and_checked() {
    assert_printed(
        "[(inc 1073741823) (dec -1073741824) (* 3037000499 3037000499) (+ 9007199254740992 1)
          (- 0) (abs -9223372036854775807) (/ 4 2) (/ -9 3)]",
        "[1073741824 -1073741825 9223372030926249001 9007199254740993 0 9223372036854775807 2 -3]",
    );
    for source in [
        "(+ 9223372036854775807 1)",
        "(- -9223372036854775808 1)",
        "(* 4611686018427387904 2)",
        "(inc 9223372036854775807)",
        "(dec -9223372036854775808)",
        "(- -9223372036854775808)",
    ] {
        assert_eq!(thrown(source), "\"long overflow\"", "{source}");
    }
    assert_printed(
        "[(unchecked-add 9223372036854775807 1) (unchecked-multiply 4611686018427387904 2)
          (unchecked-inc 9223372036854775807) (unchecked-negate -9223372036854775808)]",
        "[-9223372036854775808 -9223372036854775808 -9223372036854775808 -9223372036854775808]",
    );
}

#[test]
fn integer_division_follows_clojure() {
    assert_printed(
        "[(quot -7 2) (rem -7 2) (mod -7 2) (mod 7 -2) (quot 9223372036854775807 -1)
          (mod 7.5 2) (quot 7.5 2) (rem -7.5 2)]",
        "[-3 -1 1 -1 -9223372036854775807 1.5 3.0 -1.5]",
    );
    for source in ["(/ 1 0)", "(/ 0 0)", "(quot 1 0)", "(rem 1 0)", "(mod 1 0)"] {
        assert_eq!(thrown(source), "\"Divide by zero\"", "{source}");
    }
    // Floating division by zero is IEEE 754's.
    assert_printed("[(/ 1.0 0) (/ -1 0.0) (/ 0.0 0)]", "[##Inf ##-Inf ##NaN]");
}

#[test]
fn contagion_and_predicates_distinguish_longs_from_doubles() {
    assert_printed(
        "[(+ 1 0.5) (* 2 1.0) (- 3 3.0) (int? 1) (int? 1.0) (double? 1.0) (double? 1)
          (integer? 1.0) (float? 1.0) (zero? 0.0) (zero? -0.0) (== 1 1.0) (identical? 0 0.0)]",
        "[1.5 2.0 0.0 true false true false false true true true true false]",
    );
}

#[test]
fn doubles_print_as_java_does() {
    assert_printed(
        "[1.0 -0.0 0.1 100.0 1234567.0 1.0E7 1.0E21 1.0E-3 1.0E-4 123456.789 4.9E-324 9.9E-324
          1.7976931348623157E308 (str 2.5) (str -0.0)]",
        "[1.0 -0.0 0.1 100.0 1234567.0 1.0E7 1.0E21 0.001 1.0E-4 123456.789 4.9E-324 9.9E-324 \
         1.7976931348623157E308 \"2.5\" \"-0.0\"]",
    );
}

#[test]
fn reading_keeps_longs_exact_and_distinct_from_doubles() {
    assert_printed(
        "(let [forms (read-string \"[1 1.0 9223372036854775807 -9223372036854775808 0x10 2r101 4/2 -0.0 1e3]\")]
           [forms (mapv int? forms)])",
        "[[1 1.0 9223372036854775807 -9223372036854775808 16 5 2 -0.0 1000.0] \
         [true false true true true true true false false]]",
    );
    // A literal of each kind compiles as that kind.
    assert_printed(
        "[(int? 9223372036854775807) (int? 4/2) (double? 2.0) 9007199254740993]",
        "[true true true 9007199254740993]",
    );
}

#[test]
fn integral_counts_are_longs() {
    assert_printed(
        "[(count (range 5 0 -2)) (count (range 0.5 3 0.7)) (int? (count (range 0.5 3 0.7)))
          (count (drop 1.2 [0 1 2 3])) (nthrest [1 2 3] 1.5) (count (vec (range 40)))]",
        "[3 4 true 2 (3) 40]",
    );
}

#[test]
fn rest_arguments_count_from_long_zero() {
    // A rest seq's index started at the double 0.0, which `zero?` no longer
    // equates with long 0 under strict equality; array-map then copied its
    // arguments through a slow path that ran out of fuel at startup.
    assert_printed(
        "[((fn [& xs] (count xs)) 1 2 3)
          (count (array-map 0 0 1 1 2 2 3 3 4 4 5 5 6 6 7 7 8 8 9 9 10 10 11 11 12 12 13 13
                            14 14 15 15 16 16 17 17 18 18 19 19 20 20 21 21 22 22 23 23))]",
        "[3 24]",
    );
}

#[test]
fn integral_double_property_keys_name_the_long_property() {
    assert_printed(
        "(let [o (js-obj)] (unchecked-set o -0.0 7) (unchecked-set o 1.0 8)
           [(unchecked-get o 0) (unchecked-get o \"1\")])",
        "[7 8]",
    );
}

#[test]
fn macro_data_preserves_longs_and_doubles() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define("(defmacro kinds [& forms] (mapv (fn [x] [x (int? x) (double? x)]) forms))")
            .unwrap();
        let value = session
            .eval_with_macros("(pr-str (kinds 1 1.0 9223372036854775807 -0.0))", &mut macros)
            .unwrap();
        assert_eq!(
            units(&mut session, &value),
            "[[1 true false] [1.0 false true] [9223372036854775807 true false] [-0.0 false true]]"
        );
    }
}

#[test]
fn case_matches_numbers_only_within_their_kind() {
    assert_printed(
        "[(case 1.0 1 :a :b) (case 1 1.0 :a :b) (case -0.0 0 :a :b) (case -0.0 0.0 :a :b)
          (case ##NaN ##NaN :a :b) (case 2 (1 2) :a :b) (case 2.0 (1.0 2.0) :a :b)
          (case 9223372036854775807 9223372036854775807 :a :b)]",
        "[:b :b :b :b :b :a :a :a]",
    );
}

#[test]
fn ranges_reach_the_ends_of_the_long_range() {
    assert_printed(
        "[(take 2 (range -9223372036854775808 9223372036854775807))
          (take 2 (range 0 9223372036854775807 2))
          (vec (range 9223372036854775805 9223372036854775807 5))
          (vec (range -9223372036854775806 -9223372036854775808 -5))
          (into [] (range 9223372036854775805 9223372036854775807))
          (count (range -9223372036854775808 9223372036854775807 4))
          (reduce + (range 10)) (nth (range 0 100 7) 13) (count (range 10 0 -3))
          (vec (drop 30 (range 0 70 2)))]",
        "[(-9223372036854775808 -9223372036854775807) (0 2) [9223372036854775805] \
         [-9223372036854775806] [9223372036854775805 9223372036854775806] \
         4611686018427387904 45 91 4 [60 62 64 66 68]]",
    );
}

#[test]
fn double_quot_and_rem_truncate_through_a_long() {
    assert_printed(
        "[(quot -0.5 1.0) (rem -0.0 2) (quot 1e20 3.0) (rem 1e20 3.0) (rem 5.5 ##Inf)
          (quot 5.5 ##Inf) (quot -7.5 2) (rem -7.5 2) (mod -7.5 2) (mod 7 2.5)]",
        "[0.0 -0.0 3.333333333333333E19 0.0 ##NaN 0.0 -3.0 -1.5 0.5 2.0]",
    );
    for source in ["(quot ##NaN 1.0)", "(rem ##Inf 1.0)", "(mod ##NaN 1.0)"] {
        assert_eq!(thrown(source), "\"Infinite or NaN\"", "{source}");
    }
}

#[test]
fn counts_narrow_as_clojure_casts_to_long() {
    assert_printed("(drop ##NaN [1 2 3])", "(1 2 3)");
    for source in ["(doall (drop 1e20 [1 2 3]))", "(doall (drop ##Inf [1 2 3]))", "(nthrest [1 2 3] 1e20)"] {
        assert_eq!(thrown(source), "\"Value out of range for long\"", "{source}");
    }
}

#[test]
fn long_min_value_quotients_and_ratio_terms_need_bigints() {
    // Clojure divides through BigIntegers when either long is MIN, so these
    // are BigInts or ratios (#338); Suss says so rather than answer wrongly.
    for source in ["(/ -9223372036854775808 -1)", "(/ -9223372036854775808 1)", "(/ 4 -9223372036854775808)"] {
        assert!(thrown(source).contains("(#338)"), "{source}");
    }
    assert!(thrown("(read-string \"-9223372036854775808/2\")").contains("Unsupported numeric literal"));
    assert!(thrown("-9223372036854775808/1").contains("Unsupported numeric literal"));
    assert_printed("[(/ 6 -2) -9223372036854775807/1 (read-string \"-9223372036854775807/1\")]", "[-3 -9223372036854775807 -9223372036854775807]");
}

#[test]
fn double_contagion_reaches_every_arithmetic_function() {
    assert_printed(
        "[(inc 1.0) (dec 1.0) (- 1.0) (* 2 1.0) (+ 1 1.0 1) (- 3 1.0) (/ 4 2.0) (quot 7.0 2)
          (rem 7 2.0) (mod 7.0 2) (abs -2.0) (max 1 2.0) (min 1.0 2) (unchecked-add 1 1.0)
          (unchecked-multiply 2.0 3) (reduce + [1 2 3.0])]",
        "[2.0 0.0 -1.0 2.0 3.0 2.0 2.0 3.0 1.0 1.0 2.0 2.0 1.0 2.0 6.0 6.0]",
    );
}
