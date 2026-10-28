//! Independent sequence observations must not traverse unobserved entry values.
//! Probes construct built-in storage directly, which only core may do, so
//! their sources are evaluated privileged (#279).
#[path = "support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};
use suss_compile::portable_session::Session;

#[test]
fn reverse_and_value_sequences_decode_canonical_storage_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 8192).unwrap();
        decoder.capture_sequences(&mut session).unwrap();
        for (source, expected) in [
            ("(rseq [1 2 3])", vec![3.0_f64, 2.0, 1.0]),
            ("(next (rseq [1 2 3]))", vec![2.0, 1.0]),
            ("(vals (sorted-map :a 7 :b 11))", vec![7.0, 11.0]),
        ] {
            let value = session.eval_privileged(source).unwrap();
            session.collect().unwrap();
            decoder.refill(8192);
            assert_eq!(
                decoder.decode_session(&mut session, &value).unwrap(),
                Observation::List(
                    expected
                        .into_iter()
                        .map(|n| Observation::Number(n.to_bits()))
                        .collect()
                ),
                "{source}"
            );
        }
    }
}

#[test]
fn keys_do_not_decode_unused_infinite_values_in_array_hash_and_sorted_maps() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 8192).unwrap();
        decoder.capture_sequences(&mut session).unwrap();
        for source in [
            "(keys {:a (range)})",
            "(keys (hash-map :a (range)))",
            "(keys (sorted-map :a (range)))",
        ] {
            let value = session.eval_privileged(source).unwrap();
            session.collect().unwrap();
            decoder.refill(8192);
            assert_eq!(
                decoder.decode_session(&mut session, &value).unwrap(),
                Observation::List(vec![Observation::Keyword(None, vec![97])]),
                "{source}"
            );
        }
    }
}

#[test]
fn projected_spines_remain_iterative_and_reverse_views_reject_bad_declared_bounds() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 16384).unwrap();
        decoder.capture_sequences(&mut session).unwrap();
        let value=session.eval_privileged("(loop [i 199 entries nil] (if (< i 0) (new KeySeq entries nil) (recur (dec i) (cons [i (range)] entries))))").unwrap();
        session.collect().unwrap();
        decoder.refill(16384);
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::List(
                (0..200)
                    .map(|i| Observation::Number((i as f64).to_bits()))
                    .collect()
            )
        );
        let malformed = session
            .eval_privileged("(new RSeq (new Subvec nil [7] 0 100 nil) 0 nil)")
            .unwrap();
        session.collect().unwrap();
        decoder.refill(16384);
        assert!(
            decoder.decode_session(&mut session, &malformed).is_err(),
            "A small observed prefix must not bless invalid full subvector bounds"
        );
    }
}
