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
            ("(apply fv (list 1))", "[1 nil]"),
            ("(apply fv 1 (list 2 3))", "[1 (2 3)]"),
            ("(try (apply + (range 10)) (catch :default e [:error (ex-message e)]))", "45"),
            ("(apply + 1 2 (range 3))", "6"),
            ("(first (apply (fn [& xs] xs) (iterate inc 0)))", "0"),
            ("[(apply (fn [& xs] [(list? xs) xs]) (list 1 2)) (apply (fn [a & xs] [(list? xs) xs]) 0 (list 1 2))]", "[[true (1 2)] [true (1 2)]]"),
            ("(apply (fn [^long a ^long b] (+ a b)) (list 3000000000 1))", "3000000001"),
            ("(try (apply (fn [^long a ^long b] (+ a b)) (list :bad 1)) (catch :default e :threw))", ":threw"),

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
            (def apply-fail-next nil)
            (deftype ApplyTraceSeq [n stop]
              ISeqable
              (-seq [this] (do (set! apply-trace (conj apply-trace [:seq n])) this))
              ISeq
              (-first [_] (do (set! apply-trace (conj apply-trace [:first n])) n))
              (-rest [_] (do (set! apply-trace (conj apply-trace [:rest n]))
                            (if (< (inc n) stop) (ApplyTraceSeq. (inc n) stop) ())))
              INext
              (-next [_] (do (set! apply-trace (conj apply-trace [:next n]))
                            (when (= n apply-fail-next) (throw (ex-info "next limit" {})))
                            (when (< (inc n) stop) (ApplyTraceSeq. (inc n) stop)))))
            (def trace-apply
              (fn [f stop]
                (set! apply-trace [])
                (let [answer (try (apply f (ApplyTraceSeq. 0 stop))
                                  (catch :default e :arity))]
                  [answer apply-trace])))
        "#).unwrap();
        for (source, expected) in [
            ("(trace-apply list 3)",
             "[(0 1 2) [[:seq 0] [:next 0] [:seq 0] [:first 0] [:next 0] [:first 1] [:next 1] [:first 2] [:next 2]]]"),
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
        session.eval("(set! apply-fail-next 1)").unwrap();
        assert_eq!(printed(&mut session, "(trace-apply list 3)"),
            "[:arity [[:seq 0] [:next 0] [:seq 0] [:first 0] [:next 0] [:first 1] [:next 1]]]");
        session.eval("(set! apply-fail-next nil)").unwrap();
        session.eval(r#"
            (def trace-list-prefix
              (fn [prefix]
                (set! apply-trace [])
                (let [s (ApplyTraceSeq. 0 3)
                      answer (case prefix
                               1 (apply list 10 s)
                               2 (apply list 10 11 s)
                               3 (apply list 10 11 12 s)
                               4 (apply list 10 11 12 13 s)
                               5 (apply list 10 11 12 13 14 s)
                               (throw (ex-info "unexpected prefix" {})))]
                  [answer apply-trace])))
        "#).unwrap();
        for (prefix, answer, opening) in [
            (1, "(10 0 1 2)", "[:seq 0] [:seq 0]"),
            (2, "(10 11 0 1 2)", "[:seq 0]"),
            (3, "(10 11 12 0 1 2)", "[:seq 0]"),
            (4, "(10 11 12 13 0 1 2)", "[:seq 0] [:seq 0]"),
            (5, "(10 11 12 13 14 0 1 2)", "[:seq 0] [:seq 0]"),
        ] {
            let expected = format!("[{answer} [{opening} [:first 0] [:next 0] [:first 1] [:next 1] [:first 2] [:next 2]]]");
            assert_eq!(printed(&mut session, &format!("(trace-list-prefix {prefix})")), expected);
        }
        // AFn's >20 path reads the first twenty, counts the remainder
        // without first, then extracts it. JVM size25 has 51 next calls.
        assert_eq!(printed(&mut session, r#"
            (let [[answer events] (trace-apply (fn [a b] [a b]) 25)]
              [answer (count events)
               (count (filter (fn [event] (= (first event) :next)) events))
               (count (filter (fn [event] (= (first event) :first)) events))
               (count (filter (fn [event] (= (first event) :rest)) events))])
        "#), "[:arity 77 51 25 0]");
        // RT.boundedLength also walks next for a Counted ISeq. Public
        // bounded-count has different semantics and must keep its fast path.
        session.eval(r#"
            (extend-type ApplyTraceSeq
              ICounted
              (-count [this]
                (set! apply-trace (conj apply-trace [:count (.-n this)]))
                (- (.-stop this) (.-n this))))
        "#).unwrap();
        assert_eq!(printed(&mut session, r#"
            (let [[answer events] (trace-apply list 25)]
              [(count answer) (first answer) (last answer)
               (count (filter (fn [event] (= (first event) :seq)) events))
               (count (filter (fn [event] (= (first event) :next)) events))
               (count (filter (fn [event] (= (first event) :first)) events))
               (count (filter (fn [event] (= (first event) :count)) events))])
        "#), "[25 0 24 2 26 25 0]");
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

#[test]
fn apply_checks_non_null_callees_before_normalizing_the_tail() {
    // JVM 1.12.6: pr474-callee-order3-jvm.log. Nil passes checkcast;
    // other non-callables fail before the seq conversion in apply's body.
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval(r#"
            (def apply-callee-events [])
            (deftype ApplyCalleeTail []
              ISeqable
              (-seq [_] (set! apply-callee-events (conj apply-callee-events :seq)) nil))
            (def trace-apply-callee
              (fn [f prefix]
                (set! apply-callee-events [])
                (let [result (try
                               (case prefix
                                 0 (apply f (ApplyCalleeTail.))
                                 1 (apply f 0 (ApplyCalleeTail.))
                                 5 (apply f 0 1 2 3 4 (ApplyCalleeTail.))
                                 (throw (ex-info "unexpected prefix" {})))
                               (catch :default e :threw))]
                  [result apply-callee-events])))
        "#).unwrap();
        for prefix in [0, 1, 5] {
            assert_eq!(printed(&mut session, &format!("(trace-apply-callee nil {prefix})")),
                "[:threw [:seq]]");
            for callee in ["42", "false", "\"x\""] {
                assert_eq!(printed(&mut session, &format!("(trace-apply-callee {callee} {prefix})")),
                    "[:threw []]", "callee {callee}, prefix {prefix}");
            }
        }
    }
}

#[test]
fn numeric_apply_respects_chunk_end_boundaries() {
    // JVM 1.12.6: pr474-chunk-bounds-jvm.log. The backing array may
    // contain values beyond the logical end of this chunk.
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        assert_eq!(printed(&mut session,
            "(try (apply + (chunk-cons (array-chunk (array 1 2 3 :bad) 0 3) nil))
                  (catch :default e [:error (ex-message e)]))"), "6");
    }
}

#[test]
fn primitive_var_calls_and_variadic_rests_coexist_after_redefinition() {
    // The ABI 7 primitive call caches and variadic rest construction both
    // live in this fragment. Exercise repeated hits and arbitrary var misses.
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval(r#"
            (def mixed-typed-step (fn ^long [^long n] (unchecked-inc n)))
            (def mixed-call-and-rest (fn [n]
              [(mixed-typed-step n) ((fn [& xs] xs) 7 8 9) (mixed-typed-step n)]))
        "#).unwrap();
        for _ in 0..2 {
            assert_eq!(printed(&mut session, "(mixed-call-and-rest 1099511627776)"),
                "[1099511627777 (7 8 9) 1099511627777]");
        }
        session.eval("(def mixed-typed-step (fn [n] :replacement))").unwrap();
        assert_eq!(printed(&mut session, "(mixed-call-and-rest 1099511627776)"),
            "[:replacement (7 8 9) :replacement]");
    }
}

#[test]
fn native_apply_families_keep_their_own_lazy_error_boundaries() {
    // Clojure 1.12.6: pr474-review-native-variadics-jvm.log. Comparison
    // lookahead and recursive aget/aset apply have different forcing limits.
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval(r#"
            (def native-apply-events [])
            (def native-apply-array (let [a (object-array 1)] (aset a 0 a) a))
            (def trace-native-apply
              (fn [f values]
                (set! native-apply-events [])
                (let [answer (try
                               (apply f (map (fn [n]
                                               (set! native-apply-events (conj native-apply-events n))
                                               (when (= n 12) (throw (ex-info "limit" {})))
                                               (nth values n 0))
                                             (iterate inc 0)))
                               (catch :default e :threw))]
                  [answer native-apply-events])))
        "#).unwrap();
        for (source, expected) in [
            ("(trace-native-apply < [2 1])", "[false [0 1 2 3]]"),
            ("(trace-native-apply <= [2 1])", "[false [0 1 2 3]]"),
            ("(trace-native-apply > [1 2])", "[false [0 1 2 3]]"),
            ("(trace-native-apply >= [1 2])", "[false [0 1 2 3]]"),
            ("(trace-native-apply == [1 2])", "[false [0 1 2 3]]"),
            ("(trace-native-apply < [0 1 2 3 4 :bad])", "[:threw [0 1 2 3 4 5 6]]"),
            ("(trace-native-apply == [1 1 1 1 1 :bad])", "[:threw [0 1 2 3 4 5 6]]"),
            ("(trace-native-apply aget [nil 0])", "[:threw [0 1 2 3]]"),
            ("(trace-native-apply aget [native-apply-array :bad])", "[:threw [0 1 2 3]]"),
            ("(trace-native-apply aget [native-apply-array 0 0 0 0 :bad])", "[:threw [0 1 2 3 4 5 6 7]]"),
            ("(trace-native-apply aset [nil 0 0])", "[:threw [0 1 2 3 4]]"),
            ("(trace-native-apply aset [native-apply-array :bad 0])", "[:threw [0 1 2 3 4]]"),
            ("(trace-native-apply aset [native-apply-array 0 0 0 0 :bad])", "[:threw [0 1 2 3 4 5 6 7 8]]"),
            ("(trace-native-apply make-array [:bad 1 :bad])", "[:threw [0 1 2 3 4 5 6 7 8 9 10 11 12]]"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}

#[test]
fn callable_apply_preserves_meta_var_and_multifn_traversal_after_gc() {
    // JVM 1.12.6: pr474-callable-rest-var-jvm.log and callable-applyto-javap.
    // Metadata wrappers and Vars delegate; MultiFn inherits AFn.applyTo.
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // Session::eval does not expand defmulti. Only this internal fixture
        // constructs core Var/MultiFn values with the trusted host API.
        session.eval_privileged(r#"
            (def lazy-callable-events [])
            (def lazy-callable-meta (with-meta (fn [a & args] a) {:doc 7}))
            (def lazy-callable-target (fn [a & args] a))
            (def lazy-callable-var (Var. (fn [] lazy-callable-target) 'lazy-callable-target nil))
            (def lazy-callable-multi
              (MultiFn. "lazy-multi" (fn [a & args] :chosen) :default
                        (atom (make-hierarchy)) {} {} {} nil nil nil))
            (-add-method lazy-callable-multi :chosen (fn [a & args] a))
            (def trace-lazy-callable
              (fn [f]
                (set! lazy-callable-events [])
                (let [answer (try
                               (apply f (map (fn [n]
                                               (set! lazy-callable-events (conj lazy-callable-events n))
                                               (when (= n 40) (throw (ex-info "limit" {}))) n)
                                             (iterate inc 0)))
                               (catch :default e :threw))]
                  [answer lazy-callable-events])))
        "#).unwrap();
        session.collect().unwrap();
        for callee in ["lazy-callable-meta", "lazy-callable-var"] {
            assert_eq!(printed(&mut session, &format!("(trace-lazy-callable {callee})")),
                "[0 [0 1 2]]", "{callee}");
        }
        assert_eq!(printed(&mut session,
            "(let [[answer events] (trace-lazy-callable lazy-callable-multi)]
               [answer (count events) (last events)])"), "[:threw 41 40]");
        session.eval("(-add-method lazy-callable-multi :chosen (fn [& args] (count args)))").unwrap();
        for count in [20, 21, 22, 25] {
            assert_eq!(printed(&mut session, &format!("(apply lazy-callable-multi (range {count}))")), count.to_string());
        }
    }
}

#[test]
fn wide_apply_reports_jvm_twenty_one_argument_errors_after_dispatch() {
    // JVM 1.12.6 AFn's final invoke overload receives twenty scalars and an
    // Object[] tail and reports21, irrespective of the tail's length.
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval_privileged(r#"
            (def wide-dispatch-events [])
            (def wide-bad-dispatch
              (MultiFn. "bad-dispatch" (fn [x] (set! wide-dispatch-events [:dispatch]) :chosen)
                        :default (atom (make-hierarchy)) {} {} {} nil nil nil))
            (-add-method wide-bad-dispatch :chosen (fn [& xs] :unused))
            (def wide-bad-target
              (MultiFn. "bad-target" (fn [& xs] (set! wide-dispatch-events [:dispatch]) :chosen)
                        :default (atom (make-hierarchy)) {} {} {} nil nil nil))
            (-add-method wide-bad-target :chosen (fn [x] :unused))
        "#).unwrap();
        for count in [21, 22, 25] {
            for (callee, events) in [("(fn [x] x)", "[]"), ("wide-bad-dispatch", "[]"), ("wide-bad-target", "[:dispatch]")] {
                session.eval("(set! wide-dispatch-events [])").unwrap();
                let actual = printed(&mut session, &format!(
                    "(try (apply {callee} (range {count})) (catch :default e (ex-message e)))"));
                assert!(actual.starts_with("\"Wrong number of args (21) passed to: "), "{callee}, {count}: {actual}");
                assert_eq!(printed(&mut session, "wide-dispatch-events"), events);
            }
        }
    }
}

#[test]
fn apply_callback_registration_is_private_and_publishes_atomically() {
    use suss_cli::portable_session::SessionError;
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for source in [
            "(suss.bootstrap/apply-sequence-ops! identity identity identity identity identity identity identity identity identity)",
            "(suss.bootstrap/apply-to-seq identity nil)",
            "(suss.bootstrap/list-function identity identity identity identity identity)",
        ] {
            let Err(SessionError::Compile(error)) = session.eval(source) else {
                panic!("private helper was not rejected: {source}");
            };
            assert_eq!(error.message, "Private apply adapters require compiled core");
        }
        // The host is trusted to compile core. A rejected replacement must
        // not publish its first valid callback before checking the others.
        session.eval_privileged(
            "(def registration-result
               (try (suss.bootstrap/apply-sequence-ops! (fn [s] :corrupt) nil nil nil nil nil nil nil nil (list nil))
                    (catch :default e :rejected)))").unwrap();
        assert_eq!(printed(&mut session, "registration-result"), ":rejected");
        assert_eq!(printed(&mut session, "(apply (fn [a b] (+ a b)) (seq [17 25]))"), "42");
        // Reject an invalid final layout sample after every callback has
        // been staged. A fast List call alone would not exercise those roots.
        session.eval_privileged(
            "(def registration-result
               (let [bad (fn [& args] :corrupt)]
                 (try (suss.bootstrap/apply-sequence-ops! bad bad bad bad bad bad bad bad bad nil)
                      (catch :default e :rejected))))").unwrap();
        assert_eq!(printed(&mut session, "registration-result"), ":rejected");
        session.collect().unwrap();
        assert_eq!(printed(&mut session, "(apply (fn [a b] (+ a b)) (seq [17 25]))"), "42");
        assert_eq!(printed(&mut session, "(apply (fn [a b] (+ a b)) (list 17 25))"), "42");
    }
}

#[test]
fn list_wrapper_retains_original_and_helper_captures_across_cache_changes() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval_privileged(r#"
            (def list-owner-a
              (suss.bootstrap/list-function
                (let [tag (atom :original-a)] (fn [& xs] [@tag (list? xs) xs]))
                (fn [] :a0)
                (fn [a] [:a1 a])
                (fn [a b] [:a2 a b])
                (fn [a b c] [:a3 a b c])))
            (def list-owner-b
              (suss.bootstrap/list-function
                (let [tag (atom :original-b)] (fn [& xs] [@tag (list? xs) xs]))
                (fn [] :b0)
                (fn [a] [:b1 a])
                (fn [a b] [:b2 a b])
                (fn [a b c] [:b3 a b c])))
        "#).unwrap();
        for owner in ["a", "b", "a", "b"] {
            session.collect().unwrap();
            let source = format!("[(list-owner-{owner}) (list-owner-{owner} 1) (list-owner-{owner} 1 2) (list-owner-{owner} 1 2 3)]");
            let expected = format!("[:{owner}0 [:{owner}1 1] [:{owner}2 1 2] [:{owner}3 1 2 3]]");
            assert_eq!(printed(&mut session, &source), expected);
            assert_eq!(printed(&mut session, &format!("(list-owner-{owner} 1 2 3 4)")),
                format!("[:original-{owner} false (1 2 3 4)]"));
            assert_eq!(printed(&mut session, &format!("(list-owner-{owner} 1 2 3 4 5 6 7 8 9)")),
                format!("[:original-{owner} false (1 2 3 4 5 6 7 8 9)]"));
            assert_eq!(printed(&mut session, &format!("(apply list-owner-{owner} (list 1 2))")),
                format!("[:original-{owner} true (1 2)]"));
        }
    }
}
