//! #266: authored regressions; no execution result is implied by this source.
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;

#[path = "support/compilation_fuel.rs"]
mod compilation_fuel;
use portable_decode::{Decoder, Observation};
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};

fn number(value: f64) -> Observation {
    Observation::Number(value.to_bits())
}

fn observe(session: &mut Session, macros: &mut CompiledMacros, source: &str) -> Observation {
    let mut decoder = Decoder::capture(session, 16_384).unwrap();
    let value = session.eval_with_macros(source, macros).unwrap();
    session.collect().unwrap();
    decoder.decode_session(session, &value).unwrap()
}

#[test]
fn when_let_preserves_truthiness_once_only_initialization_scope_and_body_order() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
        let actual = observe(&mut session, &mut macros, r#"
          (def when-let-events (atom []))
          [(when-let [x nil] x)
           (when-let [x false] x)
           (when-let [x 0] x)
           (when-let [x ""] x)
           (when-let [x []] x)
           (when-let [x {}] x)
           (when-let [x true])
           (when-let [x nil])
           (when-let [x (do (swap! when-let-events conj 1) 17)]
             (swap! when-let-events conj 10)
             (swap! when-let-events conj 20)
             x)
           (when-let [x (do (swap! when-let-events conj 2) false)]
             (swap! when-let-events conj 999))
           (let [x 31] (when-let [x (+ x 1)] x))
           (let [temp 77] (when-let [x 17] [temp x]))
           (when-let [[a b] [17 23]] [a b])
           (when-let [{:keys [a b] :or {b 23}} {:a 17}] [a b])
           (loop [n 3] (if (= n 0) 7 (when-let [x n] (recur (- x 1)))))
           @when-let-events]
        "#);
        assert_eq!(actual, Observation::Vector(vec![
            Observation::Nil, Observation::Nil, number(0.0), Observation::String(vec![]),
            Observation::Vector(vec![]), Observation::Map(vec![]),
            Observation::Nil, Observation::Nil, number(17.0), Observation::Nil,
            number(32.0), Observation::Vector(vec![number(77.0), number(17.0)]),
            Observation::Vector(vec![number(17.0), number(23.0)]),
            Observation::Vector(vec![number(17.0), number(23.0)]), number(7.0),
            Observation::Vector(vec![number(1.0), number(10.0), number(20.0), number(2.0)]),
        ]));
    }
}

#[test]
fn when_let_respects_core_aliases_exclusion_and_lexical_runtime_and_macro_shadowing() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        assert_eq!(observe(&mut session, &mut macros, r#"
          (ns when-let-routing (:require [suss.core :as c]))
          [(c/when-let [x 17] x)
           (cljs.core/when-let [x 23] x)
           (let [when-let (fn [x] (+ x 1))] (when-let 41))]
        "#), Observation::Vector(vec![number(17.0), number(23.0), number(42.0)]));
        assert_eq!(observe(&mut session, &mut macros, r#"
          (ns when-let-macro-alias (:require-macros [suss.core :as cm]))
          (cm/when-let [x 29] x)
        "#), number(29.0));
        session.eval_with_macros("(ns when-let-excluded (:refer-clojure :exclude [when-let]))", &mut macros).unwrap();
        assert!(session.eval_with_macros("(when-let [x 1] x)", &mut macros).is_err());
        assert_eq!(observe(&mut session, &mut macros, "(suss.core/when-let [x 31] x)"), number(31.0));
        assert_eq!(observe(&mut session, &mut macros, r#"
          (ns when-let-runtime-shadow)
          (def when-let (fn [x] (+ x 2)))
          [(when-let 40) (suss.core/when-let [x 9] x)]
        "#), Observation::Vector(vec![number(42.0), number(9.0)]));
        macros.enter_namespace("when-let-macro-shadow").unwrap();
        macros.define("(defmacro when-let [x] (list '+ x 3))").unwrap();
        assert_eq!(observe(&mut session, &mut macros, "(ns when-let-macro-shadow) (when-let 39)"), number(42.0));
    }
}

#[test]
fn when_let_invalid_bindings_reject_in_compiled_expansion_and_allow_subsequent_work() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        for source in ["(when-let 17 1)", "(when-let [] 1)", "(when-let [x] 1)",
                       "(when-let [x 1 y 2] x)", "(when-let (x 1) 1)"] {
            let error = session.eval_with_macros(source, &mut macros).unwrap_err();
            // Public transport currently wraps the language exception. Exact
            // message/ex-data require separate macro-store/oracle observations;
            // an unresolved name or read error cannot satisfy this assertion.
            assert!(error.to_string().contains("Compiled macro expansion failed during compiled macro invocation: Uncaught language exception"),
                    "{source}: {error}");
            assert_eq!(observe(&mut session, &mut macros, "(+ 20 22)"), number(42.0));
            assert_eq!(observe(&mut session, &mut macros, "(when-let [x 17] x)"), number(17.0));
        }
    }
}
