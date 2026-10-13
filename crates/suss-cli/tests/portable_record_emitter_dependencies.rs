//! Complete sequence/transducer dependencies needed by the record emitter.
//! Authored regressions; native and fresh pinned execution remain pending.
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};
use suss_cli::portable_session::Session;

fn observe(session: &mut Session, decoder: &mut Decoder, source: &str) -> Observation {
    decoder.refill(4096);
    let value = session
        .eval(source)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    session.collect().unwrap();
    decoder.decode_session(session, &value).unwrap()
}
fn number(value: f64) -> Observation {
    Observation::Number(value.to_bits())
}
fn vector(values: &[f64]) -> Observation {
    Observation::Vector(values.iter().map(|value| number(*value)).collect())
}

#[test]
fn emitter_sequence_helpers_keep_all_collection_and_transducer_branches() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "(into [] (mapcat (fn [x] [x (+ x 10)]) [1 2]))"
            ),
            vector(&[1.0, 11.0, 2.0, 12.0])
        );
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "(into [] (mapcat (fn [a b] [(+ a b)] ) [1 2 3] [10 20]))"
            ),
            vector(&[11.0, 22.0])
        );
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "(into [] (mapcat (fn [x] [x (+ x 10)])) [1 2])"
            ),
            vector(&[1.0, 11.0, 2.0, 12.0])
        );
        for source in [
            "(into [] (remove (fn [x] (> x 2)) [1 2 3 4]))",
            "(into [] (remove (fn [x] (> x 2))) [1 2 3 4])",
        ] {
            assert_eq!(
                observe(&mut session, &mut decoder, source),
                vector(&[1.0, 2.0])
            );
        }
        for source in [
            "(into [] (drop-while (fn [x] (< x 3)) [1 2 3 1]))",
            "(into [] (drop-while (fn [x] (< x 3))) [1 2 3 1])",
        ] {
            assert_eq!(
                observe(&mut session, &mut decoder, source),
                vector(&[3.0, 1.0])
            );
        }
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "(let [f (complement (fn [& xs] (count xs)))] [(f) (f 1) (f 1 2) (f 1 2 3)])"
            ),
            Observation::Vector(vec![
                Observation::Bool(false),
                Observation::Bool(false),
                Observation::Bool(false),
                Observation::Bool(false)
            ])
        );
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "(into [] (list* 1 2 3 4 5 [6 7]))"
            ),
            vector(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0])
        );
    }
}

#[test]
fn concatenating_transduction_stops_before_later_nested_and_outer_effects() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        session.eval("(def emitter-effects 0)").unwrap();
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "[(into [] (comp (mapcat (fn [x] (set! emitter-effects (+ emitter-effects 1)) [x (+ x 10)])) (take 1)) [1 2 3]) emitter-effects]"
            ),
            Observation::Vector(vec![vector(&[1.0]), number(1.0)])
        );
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "(let [rf (cat (fn ([] 9) ([result] result) ([result input] (+ result input))))] [(rf) (rf 11) (rf 0 [1 2 3])])"
            ),
            vector(&[9.0, 11.0, 6.0])
        );
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "(do (set! emitter-effects 0) (let [xf (drop-while (fn [x] (set! emitter-effects (+ emitter-effects 1)) (< x 3)))] [(into [] xf [1 2 3 1 0]) emitter-effects]))"
            ),
            Observation::Vector(vec![vector(&[3.0, 1.0, 0.0]), number(3.0)])
        );
    }
}
