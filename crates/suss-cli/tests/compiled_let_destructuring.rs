//! Prerequisite regressions for the pinned compiled when-let expansion.
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;

use portable_decode::{Decoder, Observation};
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};

fn number(value: f64) -> Observation {
    Observation::Number(value.to_bits())
}

#[test]
fn let_patterns_preserve_order_once_defaults_scope_and_hygiene() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 16_384).unwrap();
        let value = session.eval_with_macros(r#"
          (def let-pattern-events (atom []))
          (let [outer 23
                __suss_destructure_vec_0 77
                [a b :as whole] (do (swap! let-pattern-events conj 1) [17 19])
                {:keys [x y] :or {x (do (swap! let-pattern-events conj 2) outer)
                                      y (do (swap! let-pattern-events conj 3) a)}}
                  (do (swap! let-pattern-events conj 4) {:y b})
                later (do (swap! let-pattern-events conj 5) (+ a x y))]
            [a b whole x y later __suss_destructure_vec_0 @let-pattern-events])
        "#, &mut macros).unwrap();
        session.collect().unwrap();
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Vector(vec![number(17.0), number(19.0),
                Observation::Vector(vec![number(17.0), number(19.0)]),
                number(23.0), number(19.0), number(59.0), number(77.0),
                // get's default argument is eager, including when :y exists.
                Observation::Vector(vec![number(1.0), number(4.0), number(2.0), number(3.0), number(5.0)])]));
    }
}

#[test]
fn let_patterns_preserve_outer_initializer_scope_and_tail_recur() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 16_384).unwrap();
        let value = session.eval_with_macros(r#"
          [(let [x 11] (let [[x y] [x 7] z (+ x y)] [x y z]))
           (loop [n 3 total 0]
             (if (= n 0) total
               (let [[step] [n] {:keys [delta]} {:delta step}]
                 (recur (- n 1) (+ total delta)))))]
        "#, &mut macros).unwrap();
        session.collect().unwrap();
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Vector(vec![Observation::Vector(vec![number(11.0), number(7.0), number(18.0)]), number(6.0)]));
    }
}
