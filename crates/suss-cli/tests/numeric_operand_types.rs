//! Clojure numeric operands (#286): arithmetic and numeric comparison reject
//! non-numbers with a catchable language error instead of JavaScript coercion,
//! in both compiled phases, inline and as first-class function values.
use suss_cli::portable_session::Session;

fn truthy(session: &mut Session, source: &str) {
    let value = session
        .eval(source)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    session.collect().unwrap();
    session
        .inspect(&value, |store, value| {
            let bits = value
                .unwrap_anyref()
                .and_then(|value| value.as_i31(&store).ok().flatten())
                .map(|value| value.get_u32());
            assert_eq!(bits, Some(4), "{source} is not true");
            Ok(())
        })
        .unwrap();
}

fn throws(session: &mut Session, source: &str, operation: &str) {
    truthy(
        session,
        &format!(
            "(= \"{operation} requires number operands\" \
             (try {source} :returned (catch :default e (ex-message e))))"
        ),
    );
}

#[test]
fn non_numeric_operands_throw_naming_the_operation() {
    let cases = [
        // Each reproduction from #286.
        ("(> :a 1)", ">"),
        ("(< \"a\" 1)", "<"),
        ("(+ :a 1)", "+"),
        ("(:a (sorted-map-by > 1 :a 2 :b))", ">"),
        // Every operator and operand kind, inline.
        ("(- \"a\" 1)", "-"),
        ("(* nil 2)", "*"),
        ("(/ [] 2)", "/"),
        ("(+ 1 {})", "+"),
        ("(<= 1 #{})", "<="),
        ("(>= '(1) 1)", ">="),
        ("(== :a :a)", "=="),
        ("(== 1 \"1\")", "=="),
        ("(< 1 2 :a)", "<"),
        ("(+ (fn []) 1)", "+"),
        ("(let [x (identity :a)] (+ x 1))", "+"),
        // Unary forms check their operand, as in Clojure.
        ("(+ :a)", "+"),
        ("(* \"x\")", "*"),
        ("(- nil)", "-"),
        ("(/ :a)", "/"),
        // Inline core macros and shipped core functions.
        ("(inc :a)", "+"),
        ("(dec \"a\")", "-"),
        ("(zero? :a)", "zero?"),
        ("(pos? nil)", "pos?"),
        ("(neg? \"x\")", "neg?"),
        ("(min :a 1)", "min"),
        ("(max 1 :a)", "max"),
        ("(min 1 2 nil)", "min"),
        ("(quot :a 2)", "quot"),
        ("(rem 3 :a)", "rem"),
        ("(mod :a 2)", "mod"),
        ("(mod 7 nil)", "mod"),
        // First-class function values.
        ("(apply + [1 :a])", "+"),
        ("(reduce + [:a 1])", "+"),
        ("(apply + [:a])", "+"),
        ("(apply * [:a])", "*"),
        ("(apply - [\"a\"])", "-"),
        ("(apply / [:a])", "/"),
        ("(apply < [1 :a])", "<"),
        ("(apply == [:a :a])", "=="),
        ("(first (map inc [:a]))", "inc"),
        ("(first (map dec [nil]))", "dec"),
        ("(first (map zero? [:a]))", "zero?"),
        ("(first (map pos? [nil]))", "pos?"),
        ("(first (map neg? [\"x\"]))", "neg?"),
        ("(repeat :a :a)", "repeat"),
        ("(reduce max [1 :a])", "max"),
        ("(apply mod [:a 2])", "mod"),
    ];
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for (source, operation) in cases {
            throws(&mut session, source, operation);
        }
    }
}

#[test]
fn numeric_operands_keep_clojure_results() {
    let cases = [
        "(= 3 (+ 1 2))",
        "(= 3.5 (+ 1.5 2))",
        "(= 0 (+))",
        "(= 1 (*))",
        "(= -2 (- 2))",
        "(= 0.5 (/ 2))",
        "(= 4 (apply + [4]))",
        "(= 6 (reduce + [1 2 3]))",
        "(= [2 3] (mapv inc [1 2]))",
        // Unary comparisons do not check; variadic ones stop at the first false.
        "(< :a)",
        "(== \"x\")",
        "(false? (< 3 2 :a))",
        "(== 1 1.0)",
        "(false? (== ##NaN ##NaN))",
        "(< 1 2 3)",
        "(apply < [1 2 3])",
        "(zero? 0)",
        "(zero? -0.0)",
        "(pos? 1)",
        "(neg? -1)",
        "(= 3 (max 1 3 2))",
        "(= 1 (min 3 1 2))",
        "(NaN? (max ##NaN 1))",
        "(= :a (max :a))",
        "(= 2 (mod -7 3))",
        "(= -1 (rem -7 2))",
        "(= 3 (quot 7 2))",
        // Non-numeric comparison remains compare's job, and str concatenates.
        "(= -1 (compare \"a\" \"b\"))",
        "(= \"Cannot compare :a to 1\" (try (compare :a 1) nil (catch :default e (ex-message e))))",
        "(= \"a1\" (str \"a\" 1))",
        "(= :b (get (sorted-map-by > 1 :a 2 :b) 2))",
        "(= [3 2 1] (keys (sorted-map-by > 1 :a 3 :c 2 :b)))",
        "(= [:a :b] (vals (sorted-map :y :b :x :a)))",
        // First-class callee shapes whose pinned corpus cases now throw.
        "(= 3 (let [f cljs.core/+] (f 1 2)))",
        "(= 5 ((fn [f x] (f x 3)) + 2))",
        "(= 6 ((if false + *) 2 3))",
        "(= 1 (((fn [] +)) 1 0))",
        "(= 2 (let [f /] (f 24 3 4)))",
        "(= 5 (let [f -] (f 10 2 3)))",
        "(= -0.25 (let [f /] (f -4)))",
        // Bitwise operations keep the ClojureScript coercions of section 4.
        "(= 1 (bit-or nil 1))",
    ];
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for source in cases {
            truthy(&mut session, source);
        }
    }
}
