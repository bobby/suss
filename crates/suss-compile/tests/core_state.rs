//! Atoms, volatiles and delays follow Clojure's ARef, Atom, Volatile and
//! Delay (#30). Expected values cite Clojure's behaviour where it differs
//! from ClojureScript's.
use suss_compile::{portable_macros::CompiledMacros, portable_repl, portable_session::Session};

/// A runtime with the compiled macro phase, as the CLI evaluates source.
struct Repl {
    runtime: Session,
    macros: CompiledMacros,
}
impl Repl {
    fn new() -> Self {
        Self { runtime: Session::new_repl().unwrap(), macros: CompiledMacros::new().unwrap() }
    }
    /// Evaluate setup forms. The REPL cannot yet display references (#288),
    /// so the script ends in nil.
    fn setup(&mut self, source: &str) {
        self.eval(&format!("{source}\nnil"));
    }
    /// The printed value of `source`.
    fn eval(&mut self, source: &str) -> String {
        portable_repl::evaluate_script_compiled(&mut self.runtime, &mut self.macros, source, None)
            .unwrap_or_else(|error| panic!("{source}: {}", portable_repl::error_display(&mut self.runtime, &error)))
    }
}

/// `source`'s value as `pr-str` prints it.
fn printed(session: &mut Repl, source: &str) -> String {
    let shown = session.eval(&format!("(pr-str (do {source}))"));
    let quoted = shown.strip_prefix('"').and_then(|s| s.strip_suffix('"')).expect("a string");
    let mut text = String::new();
    let mut chars = quoted.chars();
    while let Some(c) = chars.next() {
        text.push(match (c, c == '\\') {
            (_, true) => match chars.next().unwrap() {
                'n' => '\n',
                other => other,
            },
            (c, false) => c,
        });
    }
    text
}

fn check(session: &mut Repl, source: &str, expected: &str) {
    assert_eq!(printed(session, source), expected, "{source}");
}

/// The message of what `source` throws.
fn thrown(session: &mut Repl, source: &str) -> String {
    printed(session, &format!("(try {source} :returned (catch :default e (ex-message e)))"))
}

#[test]
fn watches_run_in_hash_order_with_each_change() {
    let mut session = Repl::new();
    session
        .setup(
            "(def calls (atom []))
             (def a (atom 0))
             (def ks [:k0 :k1 :k2 :k3 :k4 :k5 :k6 :k7 :k8 :k9 :k10 :k11])
             (reduce (fn [_ k]
                       (add-watch a k (fn [key ref old new]
                                        (swap! calls conj [key (identical? ref a) old new]))))
                     nil (reverse ks))",
        );
    // Clojure keeps watches in a PersistentHashMap and calls them in its seq
    // order: hash order, not the order they were added.
    check(
        &mut session,
        "(do (swap! a inc) (= (map first @calls) (keys (zipmap ks ks))))",
        "true",
    );
    check(&mut session, "(every? #(= (rest %) [true 0 1]) @calls)", "true");
    // The keys were added in reverse, and hash order is neither that nor ks.
    check(&mut session, "[(= (map first @calls) (reverse ks)) (= (map first @calls) ks)]", "[false false]");
    check(&mut session, "(count (set (map first @calls)))", "12");
    // Each change notifies with its own old and new value.
    check(
        &mut session,
        "(do (reset! calls []) (reset! a 5) (swap! a + 2) (= #{[true 1 5] [true 5 7]} (set (map rest @calls))))",
        "true",
    );
    // remove-watch returns the reference; removing an absent key is a no-op.
    check(
        &mut session,
        "(do (reset! calls [])
             (reduce (fn [_ k] (remove-watch a k)) nil (rest ks))
             [(identical? a (remove-watch a :absent)) (reset! a 0) @calls])",
        "[true 0 [[:k0 true 7 0]]]",
    );
    // As in Clojure's notifyWatches, a nil watch function is skipped.
    check(&mut session, "[(identical? a (add-watch a :k0 nil)) (reset! a 1) @calls]", "[true 1 [[:k0 true 7 0]]]");
}

#[test]
fn validators_reject_before_any_change() {
    let mut session = Repl::new();
    session
        .setup(
            "(def calls (atom 0))
             (def a (atom 1 :validator pos?))
             (add-watch a :w (fn [& _] (swap! calls inc)))",
        );
    // Clojure's ARef: a falsy validator result is IllegalStateException
    // \"Invalid reference state\" (ClojureScript: \"Validator rejected
    // reference state\"), and the state and watches are untouched.
    check(&mut session, &format!("[{} @a @calls]", "(try (reset! a -1) (catch :default e (ex-message e)))"), "[\"Invalid reference state\" 1 0]");
    assert_eq!(thrown(&mut session, "(swap! a - 5)"), "\"Invalid reference state\"");
    assert_eq!(thrown(&mut session, "(swap-vals! a - 5)"), "\"Invalid reference state\"");
    assert_eq!(thrown(&mut session, "(reset-vals! a 0)"), "\"Invalid reference state\"");
    check(&mut session, "[@a @calls]", "[1 0]");
    // An exception thrown by the validator propagates as it is.
    check(
        &mut session,
        "(do (set-validator! a (fn [v] (if (= v 99) (throw (ex-info \"boom\" {:v v})) true)))
             (try (reset! a 99) (catch :default e [(ex-message e) (ex-data e) @a])))",
        "[\"boom\" {:v 99} 1]",
    );
    // set-validator! returns nil, and checks the current state first: a
    // rejected validator is not installed.
    check(&mut session, "(set-validator! a nil)", "nil");
    check(&mut session, "(get-validator a)", "nil");
    assert_eq!(thrown(&mut session, "(set-validator! a neg?)"), "\"Invalid reference state\"");
    check(&mut session, "[(get-validator a) (reset! a -3) @calls]", "[nil -3 1]");
}

#[test]
fn atom_options_follow_setup_reference() {
    let mut session = Repl::new();
    check(&mut session, "(meta (atom 1 :meta {:a 1}))", "{:a 1}");
    check(&mut session, "(meta (atom 1 :meta nil))", "nil");
    check(&mut session, "@(atom 1 :validator pos? :meta {:b 2})", "1");
    // Clojure validates the initial value (ClojureScript does not, CLJS-3447).
    assert_eq!(thrown(&mut session, "(atom 0 :validator pos?)"), "\"Invalid reference state\"");
    // :meta must be a map, as resetMeta requires; options must be pairs.
    assert_eq!(thrown(&mut session, "(atom 0 :meta 5)"), "\"Atom metadata must be a map\"");
    assert_eq!(thrown(&mut session, "(atom 0 :meta)"), "\"No value supplied for key: :meta\"");
}

#[test]
fn compare_and_set_compares_identity_and_validates_first() {
    let mut session = Repl::new();
    session
        .setup(
            "(def calls (atom []))
             (def v [1 2])
             (def a (atom v))
             (add-watch a :w (fn [_ _ old new] (swap! calls conj [old new])))",
        );
    // Clojure's Atom.compareAndSet compares by identity (AtomicReference);
    // ClojureScript compares with =.
    check(&mut session, "[(compare-and-set! a [1 2] :x) @a @calls]", "[false [1 2] []]");
    check(&mut session, "[(compare-and-set! a v :x) @a @calls]", "[true :x [[[1 2] :x]]]");
    check(&mut session, "[(compare-and-set! a :x :y) @a]", "[true :y]");
    // The new value is validated before the comparison, even one that fails.
    check(&mut session, "(set-validator! a keyword?)", "nil");
    assert_eq!(thrown(&mut session, "(compare-and-set! a :nope 1)"), "\"Invalid reference state\"");
    check(&mut session, "[@a (count @calls)]", "[:y 2]");
}

#[test]
fn swap_retries_when_its_function_changes_the_atom() {
    let mut session = Repl::new();
    session.setup(
        "(def calls (atom []))
         (def a (atom 0))
         (add-watch a :w (fn [_ _ old new] (swap! calls conj [old new])))",
    );
    // Clojure's Atom.swap compare-and-sets against the value it read, so an
    // update made by f itself makes f run again on the new value.
    check(
        &mut session,
        "[(swap! a (fn [x] (when (zero? x) (reset! a 10)) (inc x))) @a @calls]",
        "[11 11 [[0 10] [10 11]]]",
    );
    check(
        &mut session,
        "[(swap-vals! a (fn [x] (when (= x 11) (reset! a 20)) (* 2 x))) @a]",
        "[[20 40] 40]",
    );
    // NaN is never identical? to itself in Suss, but an atom holding it is
    // unchanged, as Clojure's reference comparison sees it.
    check(&mut session, "(let [b (atom ##NaN)] [(swap! b inc) (compare-and-set! b @b 1) @b])", "[##NaN true 1]");
}

#[test]
fn swap_and_reset_variants_return_clojure_values() {
    let mut session = Repl::new();
    session.setup("(def a (atom 1))");
    check(&mut session, "(swap! a + 1 2 3 4)", "11");
    check(&mut session, "(swap-vals! a inc)", "[11 12]");
    check(&mut session, "(swap-vals! a + 1 1 1)", "[12 15]");
    check(&mut session, "(reset-vals! a :z)", "[15 :z]");
    check(&mut session, "(swap! a (fn [& args] args) 1 2 3)", "(:z 1 2 3)");
}

#[test]
fn non_atom_references_use_the_protocols() {
    let mut session = Repl::new();
    session
        .setup(
            "(deftype Box [^:mutable v]
               IDeref (-deref [_] v)
               IReset (-reset! [_ n] (set! v n))
               ISwap (-swap! [_ f] (set! v (f v)))
                     (-swap! [_ f a] (set! v (f v a)))
                     (-swap! [_ f a b] (set! v (f v a b)))
                     (-swap! [_ f a b xs] (set! v (apply f v a b xs))))
             (def b (Box. 1))",
        );
    check(&mut session, "[(reset! b 2) (swap! b inc) (swap! b + 1 1 1) @b]", "[2 3 6 6]");
    check(&mut session, "[(reset-vals! b 0) (swap-vals! b dec)]", "[[6 0] [0 -1]]");
    check(&mut session, "[(compare-and-set! b -1 9) (compare-and-set! b -1 10) @b]", "[true false 9]");
}

#[test]
fn volatiles_are_unwatched_mutable_references() {
    let mut session = Repl::new();
    session.setup("(def v (volatile! 1))");
    check(&mut session, "[(vswap! v + 10) (vreset! v :x) @v (volatile? v) (volatile? (atom 1))]", "[11 :x :x true false]");
    // A volatile has no watches or validator, as Clojure's Volatile is no IRef.
    assert_eq!(thrown(&mut session, "(add-watch v :w (fn [& _]))"), "\"Invalid nominal operation\"");
    // Clojure's vswap! evaluates its volatile expression twice, to deref and
    // to reset, and each argument once.
    check(
        &mut session,
        "(let [n (atom 0) w (volatile! 0)] (vswap! (do (swap! n inc) w) + (do (swap! n inc) 5)) [@w @n])",
        "[5 3]",
    );
    let shown = printed(&mut session, "v");
    assert!(shown.starts_with("#object[suss.core.Volatile 0x") && shown.ends_with(" {:status :ready, :val :x}]"), "{shown}");
}

#[test]
fn delays_run_once_and_cache_exceptions() {
    let mut session = Repl::new();
    session
        .setup(
            "(def runs (atom 0))
             (def ok (delay (swap! runs inc) :ok))
             (def bad (delay (swap! runs inc) (throw (ex-info \"boom\" {}))))",
        );
    check(&mut session, "[(realized? ok) (delay? ok) (delay? 1) @runs]", "[false true false 0]");
    check(&mut session, "[@ok (force ok) (deref ok) @runs (realized? ok) (force 7)]", "[:ok :ok :ok 1 true 7]");
    // Clojure's Delay caches the exception: the body runs once, realized?
    // becomes true and every deref throws the same exception. ClojureScript
    // reruns the body.
    check(
        &mut session,
        "(let [e1 (try @bad (catch :default e e)) e2 (try (force bad) (catch :default e e))]
           [(ex-message e1) (identical? e1 e2) @runs (realized? bad)])",
        "[\"boom\" true 2 true]",
    );
    // Printed as Clojure prints a reference: pending, ready or failed.
    let pending = printed(&mut session, "(delay 1)");
    assert!(pending.contains("suss.core.Delay") && pending.contains("{:status :pending, :val nil}"), "{pending}");
    let failed = printed(&mut session, "bad");
    assert!(failed.contains("{:status :failed, :val #error {"), "{failed}");
    let ready = printed(&mut session, "ok");
    assert!(ready.contains("{:status :ready, :val :ok}"), "{ready}");
}
