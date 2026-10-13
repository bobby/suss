//! Range/repeat cases track pinned `core_test.cljs` test-run!, test-cljs-2113,
//! test-cljs-3270, and test-cljs-3271 behavior in both compiled phases.
use suss_cli::portable_session::Session;
use wasmtime::Val;

fn number(session: &mut Session, source: &str) -> f64 {
    let value = session
        .eval(source)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    session.collect().unwrap();
    session
        .inspect(&value, |mut store, value| {
            let fields = value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap()
                .fields(&mut store)?
                .collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("numeric result: {fields:?}")
            };
            Ok(f64::from_bits(*bits))
        })
        .unwrap()
}

#[test]
fn ranges_preserve_pinned_arities_direction_fractional_steps_and_zero_step() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
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
        ] {
            assert_eq!(number(&mut session, source), expected, "{source}");
        }

        assert_eq!(number(&mut session, "(if (instance? IntegerRange (range 0 3)) 1 0)"), 1.0);
        assert_eq!(number(&mut session, "(if (instance? Range (range 0 1 0.5)) 1 0)"), 1.0);
        assert_eq!(number(&mut session, "(if (.hasNext (-iterator (range 0 2))) 1 0)"), 1.0);
        assert_eq!(number(&mut session, "(.next (-iterator (range 0 2)))"), 0.0);
        assert_eq!(number(&mut session, "(if (try (nth (range 2) -2) false (catch :default e true)) 1 0)"), 1.0);
        assert_eq!(number(&mut session, "(if (try (nth (range 2 1 0) -2) false (catch :default e true)) 1 0)"), 1.0);
        assert_eq!(number(&mut session, "(if (= (nth (range 2) -2 :not-found) :not-found) 1 0)"), 1.0);
        assert_eq!(number(&mut session, "(get (meta (with-meta (range 0 3) {:m 42})) :m)"), 42.0);
        assert_eq!(number(&mut session, "(get (meta (with-meta (range 0.0 3.0 1.0) {:m 43})) :m)"), 43.0);
        assert_eq!(number(&mut session, "(reduce + 0 (range 1 6))"), 15.0);
        session.eval("(def range-reduce-effects 0)").unwrap();
        assert_eq!(number(&mut session, "(reduce (fn [a b] (set! range-reduce-effects (+ range-reduce-effects 1)) (reduced (+ a b))) 0 (range 1 100))"), 1.0);
        assert_eq!(number(&mut session, "range-reduce-effects"), 1.0);
    }
}

