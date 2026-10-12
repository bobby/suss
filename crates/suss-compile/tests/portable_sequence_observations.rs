//! Observation must read sequence storage without invoking pending guest code.
#[path = "support/portable_decode.rs"]
mod portable_decode;

use portable_decode::{Decoder, Observation};
use suss_compile::portable_session::Session;

fn numbers(values: &[f64]) -> Observation {
    Observation::List(
        values
            .iter()
            .map(|value| Observation::Number(value.to_bits()))
            .collect(),
    )
}

#[test]
fn sequence_decoding_preserves_pending_effects_and_reports_bounds() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        decoder.capture_sequences(&mut session).unwrap();
        session.eval("(def observation-effects 0)").unwrap();
        let pending = session
            .eval(
                "(def observation-pending
               (new suss.core/LazySeq nil
                 (fn [] (set! observation-effects (+ observation-effects 1)) (list 7 8))
                 nil nil)) observation-pending",
            )
            .unwrap();
        assert!(decoder.decode_session(&mut session, &pending).is_err());
        let effects = session.eval("observation-effects").unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &effects).unwrap(),
            Observation::Number(0.0_f64.to_bits())
        );
        session.eval("(doall observation-pending)").unwrap();
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &pending).unwrap(),
            numbers(&[7.0, 8.0])
        );
        let effects = session.eval("observation-effects").unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &effects).unwrap(),
            Observation::Number(1.0_f64.to_bits())
        );

        let partial = session.eval("(cons 1 (new suss.core/LazySeq nil (fn [] (throw (suss.bootstrap/error \"forced\"))) nil nil))").unwrap();
        assert!(decoder.decode_session(&mut session, &partial).is_err());
        let unbounded = session.eval("(range)").unwrap();
        assert!(decoder.decode_session(&mut session, &unbounded).is_err());
        decoder.refill(4096);
        let finite = session.eval("(drop 1 (range 1 7 2))").unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &finite).unwrap(),
            numbers(&[3.0, 5.0])
        );
        let chunked = session.eval("(doall (map (fn [x] x) [1 2 3]))").unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &chunked).unwrap(),
            numbers(&[1.0, 2.0, 3.0])
        );
    }
}

#[test]
fn stored_map_sequences_and_sorted_views_decode_without_guest_projection() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        decoder.capture_sequences(&mut session).unwrap();
        for source in [
            "(seq (array-map 1 10 2 20 3 30))",
            "(seq (hash-map 1 10 2 20 3 30))",
        ] {
            let value = session.eval(source).unwrap();
            let Observation::List(entries) = decoder.decode_session(&mut session, &value).unwrap()
            else {
                panic!("map-entry sequence: {source}");
            };
            let mut pairs = entries
                .into_iter()
                .map(|entry| {
                    let Observation::Vector(pair) = entry else {
                        panic!("map entry")
                    };
                    let [Observation::Number(key), Observation::Number(value)] = pair.as_slice()
                    else {
                        panic!("numeric map entry")
                    };
                    (f64::from_bits(*key) as i32, f64::from_bits(*value) as i32)
                })
                .collect::<Vec<_>>();
            pairs.sort_unstable();
            assert_eq!(pairs, vec![(1, 10), (2, 20), (3, 30)]);
        }
        for (source, keys) in [
            ("(keys (sorted-map 3 30 1 10 2 20))", vec![1.0, 2.0, 3.0]),
            (
                "(keys (sorted-map-by (fn [a b] (compare b a)) 3 30 1 10 2 20))",
                vec![3.0, 2.0, 1.0],
            ),
            (
                "(keys (next (-sorted-seq-from (sorted-map 1 10 2 20 3 30) 1 true)))",
                vec![2.0, 3.0],
            ),
        ] {
            let value = session.eval(source).unwrap();
            assert_eq!(
                decoder.decode_session(&mut session, &value).unwrap(),
                numbers(&keys)
            );
        }
        for source in [
            "(new suss.core/NodeSeq nil (array 7 8) 1 nil nil)",
            "(new suss.core/NodeSeq nil (array nil nil) 0 nil nil)",
            "(new suss.core/NodeSeq nil (array 7 8) 2 nil nil)",
            "(new suss.core/ArrayNodeSeq nil (array nil) 0 nil nil)",
            "(new suss.core/PersistentArrayMapSeq (array 7 8) 2 nil)",
            "(new suss.core/PersistentTreeMapSeq nil nil true 0 nil)",
        ] {
            let value = session.eval(source).unwrap();
            assert!(
                decoder.decode_session(&mut session, &value).is_err(),
                "{source}"
            );
        }
    }
}
