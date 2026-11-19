//! `apply` calls a fn with fixed arities only through an ordinary call, with
//! up to twenty arguments counted as Clojure's applyToHelper counts them
//! (#466); a variadic fn's rest is the same IndexedSeq as before.
use suss_cli::portable_session::Session;

fn printed(session: &mut Session, source: &str) -> String {
    let value = session
        .eval(&format!("(pr-str {source})"))
        .unwrap_or_else(|error| panic!("{source}: {error}"));
    let units = session
        .inspect(&value, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(array.elems(&mut store)?.map(|unit| unit.unwrap_i32() as u16).collect::<Vec<_>>())
        })
        .unwrap();
    String::from_utf16(&units).unwrap()
}

#[test]
fn apply_over_seqs_answers_as_before() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval(
                "(def f2 (fn [a b] [a b]))
                 (def f0-3 (fn ([] :zero) ([a] [a]) ([a b] [a b]) ([a b c] [a b c])))
                 (def f20 (fn [a b c d e f g h i j k l m n o p q r s t] [a t]))
                 (def fv (fn [a & more] [a more]))
                 (def outcome (fn [f] (try (f) (catch :default e :threw))))",
            )
            .unwrap();
        for (source, expected) in [
            ("[(apply f2 1 (list 2)) (apply f2 (list 1 2)) (apply f2 (seq [1 2])) (apply f2 1 (range 2 3))]", "[[1 2] [1 2] [1 2] [1 2]]"),
            ("[(apply f0-3 ()) (apply f0-3 nil) (apply f0-3 (list 1)) (apply f0-3 1 2 (list 3)) (apply f0-3 1 2 3 ())]", "[:zero :zero [1] [1 2 3] [1 2 3]]"),
            ("(apply f20 (range 20))", "[0 19]"),
            ("(apply f20 0 1 2 3 (range 4 20))", "[0 19]"),
            // Wrong arities and too many arguments throw, finite or not.
            ("[(outcome #(apply f2 (list 1))) (outcome #(apply f2 (list 1 2 3))) (outcome #(apply f2 (range 30)))]", "[:threw :threw :threw]"),
            // More than twenty take apply-to-simple, which reaches fns of more
            // fixed parameters (portable_source_apply.rs).
            // Callables that aren't fns, and variadic fns, as before.
            ("[(apply :a (list {:a 1})) (apply {:a 1} (list :a)) (apply #{3} (list 3)) (apply str (list 1 2 3))]", "[1 1 3 \"123\"]"),
            ("[(apply fv (list 1)) (apply fv 1 (list 2 3)) (apply + (range 10)) (apply + 1 2 (range 3))]", "[[1 nil] [1 (2 3)] 45 6]"),
            ("(first (apply (fn [& xs] xs) (iterate inc 0)))", "0"),
            // A variadic call's rest is an IndexedSeq over its arguments.
            ("(let [xs ((fn [& xs] xs) 1 2 3)] [xs (count xs) (rest xs) (list? xs) (seq? xs)])", "[(1 2 3) 3 (2 3) false true]"),
            ("[(list 1) (list 1 2 3) (list? (list 1 2)) (count (list 1 2 3))]", "[(1) (1 2 3) true 3]"),
            // list's fixed arities and its variadic one build the same lists.
            ("[(list) (list? (list)) (list nil) (list 1 nil 3) (list 1 2 3 4 5) (count (list 1 2 3 4 5)) (list? (list 1 2 3 4))]",
             "[() true (nil) (1 nil 3) (1 2 3 4 5) 5 true]"),
            ("[(= (list 1 2) (apply list [1 2])) (= (list 1 2 3 4) (apply list (range 1 5))) (peek (list 1 2)) (pop (list 1 2 3))]",
             "[true true 1 (2 3)]"),
            ("[(meta (list 1)) (meta (with-meta (list 1 2) {:a 1})) (conj (list 2 3) 1)]", "[nil {:a 1} (1 2 3)]"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}

#[test]
fn apply_sequence_callbacks_follow_clojure_1_12_6() {
    // Fresh JVM oracle: target/probe/pr474-apply-iseq-jvm.{clj,log}.
    // first, next and rest are observable on an ISeq. In particular, a
    // variadic apply counts a bounded prefix before reading its arguments,
    // then advances with next, including after the last required argument.
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval(r#"
            (def apply-trace [])
            (deftype ApplyTraceSeq [n stop]
              ISeqable
              (-seq [this] (do (set! apply-trace (conj apply-trace [:seq n])) this))
              ISeq
              (-first [_] (do (set! apply-trace (conj apply-trace [:first n])) n))
              (-rest [_] (do (set! apply-trace (conj apply-trace [:rest n]))
                            (if (< (inc n) stop) (ApplyTraceSeq. (inc n) stop) ())))
              INext
              (-next [_] (do (set! apply-trace (conj apply-trace [:next n]))
                            (when (< (inc n) stop) (ApplyTraceSeq. (inc n) stop)))))
            (def trace-apply
              (fn [f stop]
                (set! apply-trace [])
                (let [answer (try (apply f (ApplyTraceSeq. 0 stop))
                                  (catch :default e :arity))]
                  [answer apply-trace])))
        "#).unwrap();
        for (source, expected) in [
            ("(trace-apply (fn [& xs] :ok) 3)",
             "[:ok [[:seq 0] [:next 0]]]"),
            ("(trace-apply (fn [a b & xs] [a b]) 3)",
             "[[0 1] [[:seq 0] [:next 0] [:next 1] [:next 2] [:first 0] [:next 0] [:first 1] [:next 1]]]"),
            ("(trace-apply (fn [a b] [a b]) 2)",
             "[[0 1] [[:seq 0] [:next 0] [:next 1] [:first 0] [:next 0] [:first 1]]]"),
            ("(trace-apply (fn [a b] [a b]) 3)",
             "[:arity [[:seq 0] [:next 0] [:next 1] [:next 2] [:first 0] [:next 0] [:first 1] [:next 1] [:first 2]]]"),
            ("(trace-apply (fn [a b] [a b]) 4)",
             "[:arity [[:seq 0] [:next 0] [:next 1] [:next 2] [:next 3] [:first 0] [:next 0] [:first 1] [:next 1] [:first 2] [:next 2] [:first 3]]]"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
        // RT.boundedLength also walks next for a Counted ISeq. Public
        // bounded-count has different semantics and must keep its fast path.
        session.eval(r#"
            (extend-type ApplyTraceSeq
              ICounted
              (-count [this]
                (set! apply-trace (conj apply-trace [:count (.-n this)]))
                (- (.-stop this) (.-n this))))
        "#).unwrap();
        assert_eq!(
            printed(&mut session, "(trace-apply (fn [a b & xs] [a b]) 3)"),
            "[[0 1] [[:seq 0] [:next 0] [:next 1] [:next 2] [:first 0] [:next 0] [:first 1] [:next 1]]]"
        );
        assert_eq!(
            printed(&mut session, "(do (set! apply-trace []) [(bounded-count 1 (ApplyTraceSeq. 0 3)) apply-trace])"),
            "[3 [[:count 0]]]"
        );
    }
}

#[test]
fn numeric_apply_stops_realizing_at_the_conversion_error() {
    // JVM 1.12.6: pr474-apply-numeric-map-jvm.log. The bounded arity
    // probe realizes indices 0..3; the arithmetic fold then realizes only as
    // far as the bad operand. Realizing the whole input before calling a
    // universal native arithmetic closure changes observable effects.
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval(r#"
            (def numeric-apply-events [])
            (def numeric-apply-input
              (fn [bad]
                (map (fn [i]
                       (set! numeric-apply-events (conj numeric-apply-events i))
                       (when (>= i 10) (throw (ex-info "realization limit" {})))
                       (if (= i bad) :bad (inc i)))
                     (iterate inc 0))))
            (def trace-numeric-apply
              (fn [f bad]
                (set! numeric-apply-events [])
                (let [answer (try (apply f (numeric-apply-input bad))
                                  (catch :default e :threw))]
                  [answer numeric-apply-events])))
        "#).unwrap();
        for name in ["+", "-", "*", "/", "bit-and"] {
            for (bad, expected) in [
                (0, "[:threw [0 1 2 3]]"),
                (5, "[:threw [0 1 2 3 4 5]]"),
            ] {
                let source = format!("(trace-numeric-apply {name} {bad})");
                assert_eq!(printed(&mut session, &source), expected, "{source}");
            }
        }
    }
}

#[test]
fn fixed_apply_reaches_an_error_in_the_unbounded_remainder() {
    // JVM 1.12.6 reaches the throwing sentinel at index 40. It does not
    // replace this traversal with an early bounded wrong-arity exception.
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        assert_eq!(printed(&mut session, r#"
            (do
              (def fixed-apply-events [])
              (def fixed-apply-limit false)
              (let [answer
                    (try
                      (apply (fn [a b] a)
                             (map (fn [i]
                                    (set! fixed-apply-events (conj fixed-apply-events i))
                                    (when (= i 40)
                                      (set! fixed-apply-limit true)
                                      (throw (ex-info "realization limit" {})))
                                    i)
                                  (iterate inc 0)))
                      (catch :default e :threw))]
                [answer fixed-apply-limit (count fixed-apply-events)]))
        "#), "[:threw true 41]");
    }
}
