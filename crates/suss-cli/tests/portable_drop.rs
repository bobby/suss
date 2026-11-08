//! Pinned ClojureScript `drop` sequence behavior in the portable runtime.
use suss_cli::portable_session::Session;

/// An exact runtime number: a long, or a double by its bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Number {
    Long(i64),
    Double(u64),
}
/// The expected number for `value`: integral values are longs, as in Clojure.
fn num(value: f64) -> Number {
    if value.is_finite()
        && value.fract() == 0.0
        && !(value == 0.0 && value.is_sign_negative())
        && value.abs() < 9_223_372_036_854_775_808.0
    {
        Number::Long(value as i64)
    } else {
        Number::Double(value.to_bits())
    }
}
fn exact_number(store: impl wasmtime::AsContextMut, value: &wasmtime::Val) -> wasmtime::Result<Number> {
    Ok(
        match suss_compile::portable_session::runtime_number(store, value)?
            .expect("expected a runtime number")
        {
            suss_compile::portable_session::RuntimeNumber::Long(value) => Number::Long(value),
            suss_compile::portable_session::RuntimeNumber::Double(value) => {
                Number::Double(value.to_bits())
            }
        },
    )
}
/// An expected number compares by `num`: integral values are longs.
impl PartialEq<f64> for Number {
    fn eq(&self, expected: &f64) -> bool {
        *self == num(*expected)
    }
}

fn number(session: &mut Session, source: &str) -> Number {
    let value = session.eval(source).unwrap_or_else(|error| panic!("{source}: {error:?}"));
    session.inspect(&value, |mut store, value| {
        exact_number(&mut store, &value)
    }).unwrap()
}

#[test]
fn drop_preserves_idrop_lazy_sequence_and_transducer_contracts() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        assert_eq!(number(&mut session, "(count (drop 2 [0 1 2 3]))"), 2.0);
        assert_eq!(number(&mut session, "(first (drop 2 [0 1 2 3]))"), 2.0);
        assert_eq!(number(&mut session, "(first (drop -2 [7 8]))"), 7.0);
        assert_eq!(number(&mut session, "(count (drop 9 [0 1]))"), 0.0);
        assert_eq!(number(&mut session, "(count (drop 1.2 [0 1 2 3]))"), 2.0);
        assert_eq!(number(&mut session, "(reduce + 0 (drop 2 (range 6)))"), 14.0);
        assert_eq!(number(&mut session, "(transduce (drop 2) + 0 [0 1 2 3])"), 5.0);
        assert_eq!(number(&mut session, "(if (try (drop nil [1]) false (catch :default e true)) 1 0)"), 1.0);
        assert_eq!(number(&mut session, "(if (try (drop nil) false (catch :default e true)) 1 0)"), 1.0);
        session.eval("(def doall-effects 0) (def doall-seq (map (fn [x] (set! doall-effects (+ doall-effects 1)) x) [1 2 3]))").unwrap();
        assert_eq!(number(&mut session, "(if (identical? doall-seq (doall doall-seq)) 1 0)"), 1.0);
        assert_eq!(number(&mut session, "doall-effects"), 3.0);
        session.eval("(def dorun-effects 0) (def dorun-seq (map (fn [x] (set! dorun-effects (+ dorun-effects 1)) x) (list 1 2 3 4)))").unwrap();
        assert_eq!(number(&mut session, "(if (identical? dorun-seq (doall 2 dorun-seq)) 1 0)"), 1.0);
        assert_eq!(number(&mut session, "dorun-effects"), 3.0);
        assert_eq!(number(&mut session, "(if (nil? (dorun [1 2])) 1 0)"), 1.0);
        session.eval("(def drop-map-seq (doall (drop 1 {:a 1 :b 2 :c 3})))").unwrap();
        assert_eq!(number(&mut session, "(if (map-entry? (first drop-map-seq)) 1 0)"), 1.0);
        assert_eq!(number(&mut session, "(if (= (get {:a 1 :b 2 :c 3} (key (first drop-map-seq))) (val (first drop-map-seq))) 1 0)"), 1.0);
        assert_eq!(number(&mut session, "(if (= 2 (count drop-map-seq)) 1 0)"), 1.0);
        session.eval("(def drop-set #{:c :b :a}) (def drop-set-seq (seq drop-set))").unwrap();
        assert_eq!(number(&mut session, "(if (= (disj drop-set (first drop-set-seq)) (set (drop 1 drop-set))) 1 0)"), 1.0);
    }
}
