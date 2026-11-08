//! Range/repeat cases track pinned `core_test.cljs` test-run!, test-cljs-2113,
//! test-cljs-3270, and test-cljs-3271 behavior in both compiled phases.
use suss_cli::portable_session::{RuntimeNumber, Session};

fn number(session: &mut Session, source: &str) -> RuntimeNumber {
    let value = session
        .eval(source)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    session.collect().unwrap();
    session
        .inspect(&value, |store, value| {
            Ok(suss_cli::portable_session::runtime_number(store, &value)?.expect("number result"))
        })
        .unwrap()
}

#[test]
fn ranges_preserve_pinned_arities_direction_fractional_steps_and_zero_step() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let mut failures = Vec::new();
        for (source, expected) in [
            ("(count (range 5))", 5.0),
            ("(nth (range 5) 4)", 4.0),
            ("(count (range 5 0 -2))", 3.0),
            ("(nth (range 5 0 -2) 2)", 1.0),
            ("(count (range 1 2 0.25))", 4.0),
            ("(nth (range 1 2 0.25) 3)", 1.75),
            ("(count (range 0 (+ 1 (/ 9)) (/ 9)))", 10.0),
            ("(nth (range 0 1 0.1) 6)", 0.6),
            ("(count (range 5 5 1))", 0.0),
            ("(count (range 5 0 1))", 0.0),
            ("(count (range 0 5 -1))", 0.0),
            ("(count (range 0.5 40.5 1.0))", 40.0),
            ("(nth (range 0.5 40.5 1.0) 32)", 32.5),
            ("(if (instance? Range (range)) 1 0)", 1.0),
            ("(nth (range) 2)", 2.0),
            ("(first (range 7 9 0))", 7.0),
            ("(first (next (next (range 7 9 0))))", 7.0),
            ("(count (range 7 7 0))", 0.0),
            ("(count (repeat 4 7))", 4.0),
            ("(nth (repeat 4 7) 3)", 7.0),
            ("(count (repeat 0 7))", 0.0),
        ]
        .map(|(source, expected): (&str, f64)| {
            // Integral results are longs in Clojure; the rest are doubles.
            let expected = if expected.fract() == 0.0 {
                RuntimeNumber::Long(expected as i64)
            } else {
                RuntimeNumber::Double(expected)
            };
            (source, expected)
        }) {
            failures.extend(
                (number(&mut session, source) != expected).then(|| {
                    format!("{source}: {:?}, expected {expected:?}", number(&mut session, source))
                }),
            );
        }

        assert!(failures.is_empty(), "{}", failures.join("\n"));
        assert_eq!(number(&mut session, "(if (instance? IntegerRange (range 0 3)) 1 0)"), RuntimeNumber::Long(1));
        assert_eq!(number(&mut session, "(if (instance? Range (range 0 1 0.5)) 1 0)"), RuntimeNumber::Long(1));
        assert_eq!(number(&mut session, "(if (.hasNext (-iterator (range 0 2))) 1 0)"), RuntimeNumber::Long(1));
        assert_eq!(number(&mut session, "(.next (-iterator (range 0 2)))"), RuntimeNumber::Long(0));
        assert_eq!(number(&mut session, "(if (try (nth (range 2) -2) false (catch :default e true)) 1 0)"), RuntimeNumber::Long(1));
        assert_eq!(number(&mut session, "(if (try (nth (range 2 1 0) -2) false (catch :default e true)) 1 0)"), RuntimeNumber::Long(1));
        assert_eq!(number(&mut session, "(if (= (nth (range 2) -2 :not-found) :not-found) 1 0)"), RuntimeNumber::Long(1));
        assert_eq!(number(&mut session, "(get (meta (with-meta (range 0 3) {:m 42})) :m)"), RuntimeNumber::Long(42));
        assert_eq!(number(&mut session, "(get (meta (with-meta (range 0.0 3.0 1.0) {:m 43})) :m)"), RuntimeNumber::Long(43));
        assert_eq!(number(&mut session, "(reduce + 0 (range 1 6))"), RuntimeNumber::Long(15));
        session.eval("(def range-reduce-effects 0)").unwrap();
        assert_eq!(number(&mut session, "(reduce (fn [a b] (set! range-reduce-effects (+ range-reduce-effects 1)) (reduced (+ a b))) 0 (range 1 100))"), RuntimeNumber::Long(1));
        assert_eq!(number(&mut session, "range-reduce-effects"), RuntimeNumber::Long(1));
    }
}

