//! ADR-0004's calling convention (#353) against JVM Clojure 1.12.6: typed
//! entries, the spread entry, variadic adapters, the out-of-line path for
//! callables that aren't closures, and Clojure's arity errors. Expected strings
//! are JVM Clojure's, run with each case's definitions (as `defn`s and
//! `declare`; here `def` of a named `fn`, which compiled macros aren't needed
//! for), except where noted:
//! JVM Clojure names an anonymous function after its generated class
//! (`user/eval172/fn--173`), where Suss reports `fn`, so those cases compare
//! the message up to the name.
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
const DEFINITIONS: &[&str] = &[
    "(def f (fn f [x] x))",
    "(def g (fn g [a b & r] [a b r]))",
    "(def h (fn h ([a] 1) ([a b c] 3)))",
    "(def wide (fn wide [a b c d e f g h i j] [a j]))",
    "(def wide-v (fn wide-v [a b c d e f g h i & more] [i more]))",
    "(def nine (fn nine [a b c d e f g h i] i))",
    "(def fact (fn fact [n] (if (zero? n) 1 (* n (fact (dec n))))))",
    "(defprotocol P (m [this] [this a]))",
    "(extend-type nil P (m ([this] :one)))",
    "(deftype Inv [] IFn (-invoke [_ x] [:inv x]))",
];
fn sessions() -> [Session; 2] {
    [Session::new_repl().unwrap(), Session::new_macro().unwrap()].map(|mut session| {
        for definition in DEFINITIONS {
            session.eval(definition).unwrap_or_else(|error| panic!("{definition}: {error}"));
        }
        session
    })
}
/// `source`'s printed value, or `[:error message]`, the same in both phases.
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
fn check(cases: &[(&str, &str)]) {
    let mut sessions = sessions();
    for (source, expected) in cases {
        assert_eq!(outcome(&mut sessions, source), *expected, "{source}");
    }
}

#[test]
fn arity_errors_are_clojures_for_every_entry() {
    check(&[
        // A missing fixed arity's entry, a variadic minimum, multiple arities.
        ("(f 1 2)", r#"[:error "Wrong number of args (2) passed to: user/f"]"#),
        ("(f)", r#"[:error "Wrong number of args (0) passed to: user/f"]"#),
        ("(g 1)", r#"[:error "Wrong number of args (1) passed to: user/g"]"#),
        ("(h 1 2)", r#"[:error "Wrong number of args (2) passed to: user/h"]"#),
        // Through the spread entry.
        ("(apply f [1 2])", r#"[:error "Wrong number of args (2) passed to: user/f"]"#),
        ("(nine 1 2 3 4 5 6 7 8)", r#"[:error "Wrong number of args (8) passed to: user/nine"]"#),
        // A core function.
        ("(map)", r#"[:error "Wrong number of args (0) passed to: clojure.core/map"]"#),
    ]);
    let mut sessions = sessions();
    // A known closure called with another count, and a native protocol
    // implementation called with an arity it lacks: JVM Clojure names these
    // `user/evalN/fn--M`.
    for (source, count) in [("((fn [a b] [a b]) 1)", 1), ("(m nil 1)", 2)] {
        let outcome = outcome(&mut sessions, source);
        assert_eq!(outcome, format!(r#"[:error "Wrong number of args ({count}) passed to: fn"]"#), "{source}");
    }
}

#[test]
fn entries_adapters_and_spread_take_every_argument_count() {
    check(&[
        ("(g 1 2)", "[1 2 nil]"),
        ("(g 1 2 3 4)", "[1 2 (3 4)]"),
        ("(h 1 2 3)", "3"),
        ("(apply f [1])", "1"),
        ("(take 3 (apply (fn [& xs] xs) (range)))", "(0 1 2)"),
        ("(apply g 1 2 (range 3))", "[1 2 (0 1 2)]"),
        ("(apply + 1 2 [3 4])", "10"),
        // More than eight arguments: the spread entry.
        ("(wide 1 2 3 4 5 6 7 8 9 10)", "[1 10]"),
        ("(wide-v 1 2 3 4 5 6 7 8 9 10 11)", "[9 (10 11)]"),
        ("(wide-v 1 2 3 4 5 6 7 8 9)", "[9 nil]"),
        ("(nine 1 2 3 4 5 6 7 8 9)", "9"),
        ("(apply wide (range 10))", "[0 9]"),
        ("(apply nine (range 9))", "8"),
        // Known callees: a local fn, a named fn's self reference, recursion.
        ("(let [k (fn [x] (* 2 x))] (k 21))", "42"),
        ("((fn self [n] (if (pos? n) (self (dec n)) :done)) 10000)", ":done"),
        ("(fact 20)", "2432902008176640000"),
        ("(map (fn [x y] (+ x y)) [1 2] [3 4])", "(4 6)"),
        ("(reduce (fn [a b] (+ a b)) (range 5))", "10"),
    ]);
}

#[test]
fn values_that_arent_closures_are_called_out_of_line() {
    check(&[
        ("(:a {:a 1})", "1"),
        ("(:b {:a 1} :d)", ":d"),
        ("({:a 1} :a)", "1"),
        ("({:a 1} :b 2)", "2"),
        ("([4 5 6] 1)", "5"),
        ("(#{1 2} 2)", "2"),
        ("(#{1 2} 3)", "nil"),
        ("((Inv.) 5)", "[:inv 5]"),
        ("(apply (Inv.) [6])", "[:inv 6]"),
        ("(m nil)", ":one"),
        // JVM Clojure appends the var, `: #'user/u`.
        ("(do (def u) (u))", r#"[:error "Attempting to call unbound fn"]"#),
        // JVM Clojure throws ClassCastException (NullPointerException for
        // nil), naming JVM classes; Suss's message is its own.
        ("(1 2)", r#"[:error "Not callable"]"#),
        ("(nil 1)", r#"[:error "Not callable"]"#),
        (r#"("s" 1)"#, r#"[:error "Not callable"]"#),
    ]);
}
