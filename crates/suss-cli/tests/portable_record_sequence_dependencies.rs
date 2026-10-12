//! Record emitter dependencies, authored from the pinned complete core forms.
//! Fresh pinned and native execution remain required before acceptance.
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;

use portable_decode::{Decoder, Observation};
use suss_cli::portable_session::{Session, SessionValue};
use wasmtime::{AnyRef, OwnedRooted, Rooted};

fn observe(session: &mut Session, decoder: &mut Decoder, source: &str) -> Observation {
    decoder.refill(4096);
    let value = session
        .eval(source)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    session.collect().unwrap();
    decoder
        .decode_session(session, &value)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"))
}

fn number(value: f64) -> Observation {
    Observation::Number(value.to_bits())
}

fn vector(values: &[f64]) -> Observation {
    Observation::Vector(values.iter().map(|value| number(*value)).collect())
}

fn retain(session: &mut Session, value: &SessionValue) -> OwnedRooted<AnyRef> {
    session
        .inspect(value, |mut store, value| {
            value.unwrap_anyref().unwrap().to_owned_rooted(&mut store)
        })
        .unwrap()
}

fn same(session: &mut Session, expected: &OwnedRooted<AnyRef>, source: &str) {
    let actual = session.eval(source).unwrap();
    session.collect().unwrap();
    session
        .inspect(&actual, |mut store, actual| {
            let expected = expected.to_rooted(&mut store);
            assert!(Rooted::ref_eq(
                &store,
                &expected,
                actual.unwrap_anyref().unwrap()
            )?);
            Ok(())
        })
        .unwrap();
}

#[test]
fn replace_preserves_vector_identity_metadata_and_nil_false_replacements() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        let original = session.eval("(def record-replace-original (with-meta [1 2 3] {:marker 17})) record-replace-original").unwrap();
        let original = retain(&mut session, &original);
        same(
            &mut session,
            &original,
            "(replace {} record-replace-original)",
        );
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "(replace {2 9} record-replace-original)"
            ),
            vector(&[1.0, 9.0, 3.0])
        );
        assert_eq!(
            observe(&mut session, &mut decoder, "record-replace-original"),
            vector(&[1.0, 2.0, 3.0])
        );
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "(get (meta (replace {2 9} record-replace-original)) :marker)"
            ),
            number(17.0)
        );
        let expected = Observation::Vector(vec![
            Observation::Bool(false),
            Observation::Nil,
            Observation::Bool(false),
            Observation::Nil,
            number(9.0),
        ]);
        for source in [
            "(replace {nil false false nil :a false :b nil} [nil false :a :b 9])",
            "(into [] (replace {nil false false nil :a false :b nil} (list nil false :a :b 9)))",
            "(into [] (replace {nil false false nil :a false :b nil}) [nil false :a :b 9])",
        ] {
            assert_eq!(
                observe(&mut session, &mut decoder, source),
                expected,
                "{source}"
            );
        }
        session.eval("(def record-replace-effects 0) (def record-replace-lazy (replace {1 7} (map (fn [x] (set! record-replace-effects (+ record-replace-effects 1)) x) (list 1 2 3))))").unwrap();
        assert_eq!(
            observe(&mut session, &mut decoder, "record-replace-effects"),
            number(0.0)
        );
        assert_eq!(
            observe(&mut session, &mut decoder, "(first record-replace-lazy)"),
            number(7.0)
        );
        assert_eq!(
            observe(&mut session, &mut decoder, "record-replace-effects"),
            number(1.0)
        );
        assert_eq!(
            observe(&mut session, &mut decoder, "(into [] record-replace-lazy)"),
            vector(&[7.0, 2.0, 3.0])
        );
        assert_eq!(
            observe(&mut session, &mut decoder, "record-replace-effects"),
            number(3.0)
        );
    }
}

#[test]
fn iterate_preserves_seed_cache_next_identity_and_reduction_demand() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        session.eval("(def record-iterate-effects 0) (def record-iterate-head (iterate (fn [x] (set! record-iterate-effects (+ record-iterate-effects 1)) (+ x 10)) 1))").unwrap();
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "[(first record-iterate-head) record-iterate-effects]"
            ),
            vector(&[1.0, 0.0])
        );
        let tail = session
            .eval("(def record-iterate-tail (next record-iterate-head)) record-iterate-tail")
            .unwrap();
        let tail = retain(&mut session, &tail);
        assert_eq!(
            observe(&mut session, &mut decoder, "record-iterate-effects"),
            number(0.0)
        );
        same(&mut session, &tail, "(next record-iterate-head)");
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "[(first record-iterate-tail) (first record-iterate-tail) record-iterate-effects]"
            ),
            vector(&[11.0, 11.0, 1.0])
        );
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "[(first (with-meta record-iterate-tail {:tag 19})) (get (meta (with-meta record-iterate-tail {:tag 19})) :tag) record-iterate-effects]"
            ),
            vector(&[11.0, 19.0, 1.0])
        );
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "(do (set! record-iterate-effects 0) [(reduce (fn [_ x] (reduced x)) 0 (iterate (fn [x] (set! record-iterate-effects (+ record-iterate-effects 1)) (+ x 1)) 7)) record-iterate-effects])"
            ),
            vector(&[7.0, 0.0])
        );
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "(do (set! record-iterate-effects 0) [(reduce (fn [a b] (reduced (+ a b))) (iterate (fn [x] (set! record-iterate-effects (+ record-iterate-effects 1)) (+ x 1)) 7)) record-iterate-effects])"
            ),
            vector(&[15.0, 1.0])
        );
    }
}

#[test]
fn take_preserves_numeric_preconditions_lazy_demand_and_reduced_identity() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        assert_eq!(
            observe(&mut session, &mut decoder, "(into [] (take 2.2 [1 2 3 4]))"),
            vector(&[1.0, 2.0, 3.0])
        );
        assert_eq!(
            observe(&mut session, &mut decoder, "(into [] (take 2.2) [1 2 3 4])"),
            vector(&[1.0, 2.0, 3.0])
        );
        session.eval("(def record-take-effects 0) (def record-take-pending (new LazySeq nil (fn [] (set! record-take-effects 1) (throw (suss.bootstrap/error \"forced\"))) nil nil))").unwrap();
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "(into [] (take 0 record-take-pending))"
            ),
            vector(&[])
        );
        assert_eq!(
            observe(&mut session, &mut decoder, "record-take-effects"),
            number(0.0)
        );
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "[(try (take nil) 0 (catch :default error 1)) (try (take nil []) 0 (catch :default error 1))]"
            ),
            vector(&[1.0, 1.0])
        );
        let reduced = session
            .eval("(def record-take-reduced (reduced 17)) record-take-reduced")
            .unwrap();
        let reduced = retain(&mut session, &reduced);
        same(
            &mut session,
            &reduced,
            "(((take 1) (fn [_ _] record-take-reduced)) 0 7)",
        );
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "(let [rf ((take 2) (fn ([] 9) ([result] result) ([result input] (+ result input))))] [(rf) (rf 11) (unreduced (rf 0 3)) (unreduced (rf 3 4))])"
            ),
            vector(&[9.0, 11.0, 3.0, 7.0])
        );
    }
}
