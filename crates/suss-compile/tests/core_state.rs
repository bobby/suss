//! Atoms, volatiles and delays follow Clojure's ARef, Atom, Volatile and
//! Delay (#30). Expected values cite Clojure's behaviour where it differs
//! from ClojureScript's.
use suss_compile::{
    portable_macros::CompiledMacros,
    portable_repl,
    portable_session::{Session, SessionOptions},
};

/// A runtime with the compiled macro phase, as the CLI evaluates source.
struct Repl {
    runtime: Session,
    macros: CompiledMacros,
}
impl Repl {
    fn new() -> Self {
        Self { runtime: Session::new_repl().unwrap(), macros: CompiledMacros::new().unwrap() }
    }
    /// A runtime that loads required namespaces from `root`.
    fn with_sources(root: &std::path::Path) -> Self {
        let options = SessionOptions { source_paths: vec![root.to_owned()], ..SessionOptions::default() };
        Self { runtime: Session::new_repl_with_options(options).unwrap(), macros: CompiledMacros::new().unwrap() }
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

#[test]
fn auto_resolved_keywords_use_the_current_namespace_and_its_aliases() {
    let root = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("core_state_auto_keywords");
    std::fs::create_dir_all(root.join("lib")).unwrap();
    std::fs::write(root.join("lib/kw.sus"), "(ns lib.kw) (def x 1)").unwrap();
    let mut session = Repl::with_sources(&root);
    check(&mut session, "[::a (namespace ::a) (= ::a :user/a)]", "[:user/a \"user\" true]");
    session.setup("(ns app.kw (:require [lib.kw :as lib]))");
    check(&mut session, "[::a ::lib/b '::c {::d [::e]} (meta ^{::f 1} [])]",
        "[:app.kw/a :lib.kw/b :app.kw/c {:app.kw/d [:app.kw/e]} {:app.kw/f 1}]");
    // Shorthand metadata and macro bodies resolve them too; the macro session
    // reads the runtime namespace's aliases, as Clojure's reader has one table.
    check(&mut session, "[(meta ^::t []) (meta ^::lib/t [])]", "[{:app.kw/t true} {:lib.kw/t true}]");
    session.setup("(defmacro kws [] [::k ::lib/k '::q])");
    check(&mut session, "(kws)", "[:app.kw/k :lib.kw/k :app.kw/q]");
    // Clojure resolves `::alias/name` only through an alias of the current namespace.
    assert!(thrown_reading(&mut session, "::nope/x").contains("Invalid token: ::nope/x"));
    assert!(thrown_reading(&mut session, "::app.kw/x").contains("Invalid token: ::app.kw/x"));
}

/// The error evaluating `source` reports.
fn thrown_reading(session: &mut Repl, source: &str) -> String {
    match portable_repl::evaluate_script_compiled(&mut session.runtime, &mut session.macros, source, None) {
        Ok(value) => panic!("{source} returned {value}"),
        Err(error) => portable_repl::error_display(&mut session.runtime, &error),
    }
}

#[test]
fn derive_builds_clojure_hierarchies() {
    let mut session = Repl::new();
    check(&mut session, "(make-hierarchy)", "{:parents {}, :descendants {}, :ancestors {}}");
    session.setup(
        "(def h (-> (make-hierarchy) (derive ::rect ::shape) (derive ::square ::rect)))
         (def diamond (-> (make-hierarchy) (derive ::b ::a) (derive ::c ::a) (derive ::d ::b) (derive ::d ::c)))",
    );
    check(&mut session, "h",
        "{:parents {:user/rect #{:user/shape}, :user/square #{:user/rect}}, \
         :ancestors {:user/rect #{:user/shape}, :user/square #{:user/rect :user/shape}}, \
         :descendants {:user/shape #{:user/rect :user/square}, :user/rect #{:user/square}}}");
    check(&mut session, "[(isa? h ::square ::shape) (isa? h ::shape ::square) (isa? h [::square ::rect] [::shape ::shape])
                          (isa? h [::square] [::shape ::shape])]", "[true false true false]");
    check(&mut session, "[(parents h ::square) (ancestors h ::square) (descendants h ::shape) (parents h ::shape)]",
        "[#{:user/rect} #{:user/rect :user/shape} #{:user/rect :user/square} nil]");
    // Deriving an existing relationship returns the same hierarchy.
    check(&mut session, "(identical? h (derive h ::rect ::shape))", "true");
    check(&mut session, "(= (underive diamond ::d ::b)
                            {:parents {::b #{::a}, ::c #{::a}, ::d #{::c}}
                             :descendants {::a #{::d ::b ::c}, ::c #{::d}}
                             :ancestors {::b #{::a}, ::c #{::a}, ::d #{::a ::c}}})", "true");
    check(&mut session, "(identical? diamond (underive diamond ::b ::d))", "true");
}

#[test]
fn derive_errors_are_clojures() {
    let mut session = Repl::new();
    session.setup("(def chain (-> (make-hierarchy) (derive ::a ::b) (derive ::b ::c)))");
    for (source, message) in [
        ("(derive ::a ::a)", "Assert failed: (not= tag parent)"),
        ("(derive ::a :b)", "Assert failed: (namespace parent)"),
        ("(derive :a ::b)", "Assert failed: (or (class? tag) (and (instance? clojure.lang.Named tag) (namespace tag)))"),
        ("(derive (make-hierarchy) 42 ::b)", "Assert failed: (or (class? tag) (instance? clojure.lang.Named tag))"),
        ("(derive (make-hierarchy) ::a \"b\")", "Assert failed: (instance? clojure.lang.Named parent)"),
        ("(derive chain ::c ::a)", "Cyclic derivation: :user/a has :user/c as ancestor"),
        ("(derive chain ::a ::c)", ":user/a already has :user/c as ancestor"),
    ] {
        assert_eq!(printed(&mut session, &format!("(try {source} :returned (catch :default e (ex-message e)))")),
            format!("{message:?}"), "{source}");
    }
}

#[test]
fn the_global_hierarchy_backs_the_two_argument_forms() {
    let mut session = Repl::new();
    check(&mut session, "[(derive ::t ::p) (derive 'n/t ::p) (isa? ::t ::p) (parents ::t) (descendants ::p)]",
        "[nil nil true #{:user/p} #{:user/t n/t}]");
    check(&mut session, "[(underive ::t ::p) (isa? ::t ::p) (parents ::t) (underive ::t ::p) (descendants ::p)]",
        "[nil false nil nil #{n/t}]");
    // Types are Clojure's classes: valid tags, without supertypes.
    session.setup("(deftype T [])");
    check(&mut session, "[(derive T ::thing) (isa? T ::thing) (parents T) (ancestors T)]",
        "[nil true #{:user/thing} #{:user/thing}]");
    check(&mut session, "(try (descendants T) (catch :default e (ex-message e)))", "\"Can't get descendants of classes\"");
    check(&mut session, "(map (fn [x] (parents x)) [nil \"x\" 42 [] {}])", "(nil nil nil nil nil)");
}

#[test]
fn sequence_helpers_and_ident_predicates_follow_clojure() {
    let mut session = Repl::new();
    check(&mut session, "[(interleave [1 2 3] [:a :b]) (interleave) (interleave [1 2]) (interleave [1 2] [3 4] [5 6])]",
        "[(1 :a 2 :b) () (1 2) (1 3 5 2 4 6)]");
    check(&mut session, "[(interpose :x [1 2 3]) (interpose :x []) (into [] (interpose :x) [1 2 3])]",
        "[(1 :x 2 :x 3) () [1 :x 2 :x 3]]");
    check(&mut session, "[(flatten [1 [2 '(3 [4])] 5]) (flatten nil) (flatten {:a 1}) (tree-seq vector? seq [1 [2]])]",
        "[(1 2 3 4 5) () () ([1 [2]] 1 [2] 2)]");
    // partition steps with the eager nthrest, as Clojure's does, so a skipped
    // element is realized with the partition before it.
    check(&mut session, "(let [log (atom [])]
                           (first (partition 1 3 (map (fn [i] (swap! log conj i) i) (list 0 1 2 3 4 5))))
                           @log)", "[0 1 2]");
    check(&mut session, "[(nthrest [1 2 3] 2) (nthrest [1 2] 0) (nthrest [1 2 3] 5) (nthrest nil 2) (nthrest (range 10) 8)]",
        "[(3) [1 2] () nil (8 9)]");
    check(&mut session, "[(partition 2 [1 2 3 4 5]) (partition 2 1 [1 2 3]) (partition 2 2 [:p] [1 2 3]) (partition 3 3 [] [1 2 3 4])]",
        "[((1 2) (3 4)) ((1 2) (2 3)) ((1 2) (3 :p)) ((1 2 3) (4))]");
    check(&mut session, "(map (juxt ident? simple-ident? qualified-ident? simple-keyword? qualified-keyword? simple-symbol? qualified-symbol?)
                            [:a ::a 'a 'n/a \"a\" nil])",
        "([true true false true false false false] [true false true false true false false] \
         [true true false false false true false] [true false true false false false true] \
         [false false false false false false false] [false false false false false false false])");
}

#[test]
fn doseq_follows_clojures_bindings_and_chunking() {
    let mut session = Repl::new();
    session.setup("(def log (atom []))");
    let run = |session: &mut Repl, source: &str| printed(session, &format!("(do (reset! log []) [{source} @log])"));
    assert_eq!(run(&mut session, "(doseq [x [1 2 3] y [:a :b] :when (odd? x)] (swap! log conj [x y]))"),
        "[nil [[1 :a] [1 :b] [3 :a] [3 :b]]]");
    assert_eq!(run(&mut session, "(doseq [x [1 2 3] y [1 2 3] :while (<= y x)] (swap! log conj [x y]))"),
        "[nil [[1 1] [2 1] [2 2] [3 1] [3 2] [3 3]]]");
    assert_eq!(run(&mut session, "(doseq [x (range 10) :while (< x 3) :let [y (* 10 x)]] (swap! log conj y))"),
        "[nil [0 10 20]]");
    assert_eq!(run(&mut session, "(doseq [[k v] (sorted-map :a 1 :b 2)] (swap! log conj k v))"),
        "[nil [:a 1 :b 2]]");
    assert_eq!(run(&mut session, "(doseq [x nil] (swap! log conj x))"), "[nil []]");
    // A chunked seq is realized a chunk at a time, before the body sees it.
    assert_eq!(run(&mut session, "(doseq [x (map (fn [i] (swap! log conj [:realize i]) i) (range 3))] (swap! log conj [:body x]))"),
        "[nil [[:realize 0] [:realize 1] [:realize 2] [:body 0] [:body 1] [:body 2]]]");
    assert_eq!(run(&mut session, "(doseq [x (map (fn [i] (swap! log conj [:realize i]) i) (list 0 1))] (swap! log conj [:body x]))"),
        "[nil [[:realize 0] [:body 0] [:realize 1] [:body 1]]]");
    // The messages are checked by portable_macros' doseq_validation_tests.
    for source in ["(doseq (x [1]) x)", "(doseq [x] x)"] {
        let shown = thrown_reading(&mut session, source);
        assert!(shown.contains("Compiled macro expansion failed"), "{source}: {shown}");
    }
}

#[test]
fn thread_last_inserts_each_form_last() {
    let mut session = Repl::new();
    check(&mut session, "[(->> [1 2 3] (map inc) (filter odd?) vec) (->> 5) (->> 2 (- 10)) (->> [1] first)]",
        "[[3] 5 8 1]");
}

#[test]
fn multimethods_dispatch_through_hierarchies_and_defaults() {
    let mut session = Repl::new();
    session.setup("(defmulti area :shape)
                   (defmethod area :square [{:keys [s]}] (* s s))
                   (defmethod area :default [x] [:unknown x])");
    check(&mut session, "[(area {:shape :square :s 3}) (area {:shape :circle})]",
        "[9 [:unknown {:shape :circle}]]");
    session.setup("(derive ::rect ::shape)
                   (derive ::sq ::rect)
                   (defmulti kind identity)
                   (defmethod kind ::shape [_] :shape)");
    check(&mut session, "(kind ::sq)", ":shape");
    // Adding a method resets the cache, and the closer ancestor dominates.
    session.setup("(defmethod kind ::rect [_] :rect)");
    check(&mut session, "[(kind ::sq) (map kind [::sq ::rect ::shape]) (apply kind [::sq])]",
        "[:rect (:rect :rect :shape) :rect]");
    check(&mut session, "[(= #{::rect ::shape} (set (keys (methods kind)))) (= (get-method kind ::sq) (get-method kind ::rect)) (get-method kind 5)]",
        "[true true nil]");
    // Vectors dispatch element-wise through isa?.
    session.setup("(defmulti pair identity) (defmethod pair [::shape ::shape] [_] :pair)");
    check(&mut session, "[(pair [::sq ::rect]) (get-method pair [::sq 1])]", "[:pair nil]");
    // Variadic dispatch, past the fixed arities.
    session.setup("(defmulti va (fn [& xs] (count xs))) (defmethod va 22 [& xs] (reduce + xs))");
    check(&mut session, "(apply va (range 22))", "231");
    // A named, multi-arity method.
    session.setup("(defmulti two (fn [x & _] x)) (defmethod two :a named ([_] 1) ([_ y] (if (= y 0) (named :a) y)))");
    check(&mut session, "[(two :a) (two :a 5) (two :a 0)]", "[1 5 1]");
    check(&mut session, "[(fn? kind) (= kind kind) (= kind pair)]", "[false true false]");
    check(&mut session, "(apply str (take 28 (pr-str kind)))", "\"#object[suss.core.MultiFn 0x\"");
}

#[test]
fn multimethod_errors_are_clojures() {
    let mut session = Repl::new();
    session.setup("(derive ::c ::a)
                   (derive ::c ::b)
                   (defmulti amb identity)
                   (defmethod amb ::a [_] :a)
                   (defmethod amb ::b [_] :b)
                   (defmulti nilly identity)");
    for (source, message) in [
        ("(nilly 5)", "No method in multimethod 'nilly' for dispatch value: 5"),
        ("(nilly nil)", "No method in multimethod 'nilly' for dispatch value: null"),
        ("(nilly \"s\")", "No method in multimethod 'nilly' for dispatch value: s"),
        ("(nilly [\"s\" nil])", "No method in multimethod 'nilly' for dispatch value: [\"s\" nil]"),
        // The method table is a hash map, walked in its seq order, as in Clojure.
        ("(amb ::c)", "Multiple methods in multimethod 'amb' match dispatch value: :user/c -> :user/b and :user/a, and neither is preferred"),
    ] {
        assert_eq!(printed(&mut session, &format!("(try {source} :returned (catch :default e (ex-message e)))")),
            format!("{message:?}"), "{source}");
    }
    check(&mut session, "[(= amb (prefer-method amb ::a ::b)) (amb ::c) (prefers amb)]",
        "[true :a {:user/a #{:user/b}}]");
    check(&mut session, "(try (prefer-method amb ::b ::a) (catch :default e (ex-message e)))",
        "\"Preference conflict in multimethod 'amb': :user/a is already preferred to :user/b\"");
}

#[test]
fn multimethod_preferences_and_hierarchies_follow_clojures_multifn() {
    let mut session = Repl::new();
    // A preference for a parent is inherited, as in Clojure's MultiFn.prefers;
    // ClojureScript's loops drop the inner result and report an ambiguity.
    session.setup("(derive ::x ::xp)
                   (derive ::k ::x)
                   (derive ::k ::y)
                   (defmulti inh identity)
                   (defmethod inh ::x [_] :x)
                   (defmethod inh ::y [_] :y)
                   (prefer-method inh ::xp ::y)");
    check(&mut session, "(inh ::k)", ":x");
    // nil and false can be preferred.
    session.setup("(defmulti nf identity)
                   (defmethod nf nil [_] :nil)
                   (defmethod nf false [_] :false)
                   (prefer-method nf nil false)");
    check(&mut session, "[(prefers nf) (nf nil) (nf false)]", "[{nil #{false}} :nil :false]");
    check(&mut session, "(try (prefer-method nf false nil) (catch :default e (ex-message e)))",
        "\"Preference conflict in multimethod 'nf': null is already preferred to false\"");
    // A multimethod's own hierarchy, checked again whenever it changes.
    session.setup("(def h (atom (make-hierarchy)))
                   (defmulti local identity :hierarchy h :default ::none)
                   (defmethod local ::p [_] :p)
                   (defmethod local ::none [x] [:none x])");
    check(&mut session, "(local ::c)", "[:none :user/c]");
    session.setup("(swap! h derive ::c ::p)");
    check(&mut session, "[(local ::c) (isa? ::c ::p)]", "[:p false]");
    session.setup("(swap! h underive ::c ::p)");
    check(&mut session, "(local ::c)", "[:none :user/c]");
    // Preferences in a multimethod's hierarchy use that hierarchy's parents.
    session.setup("(def h2 (atom (-> (make-hierarchy) (derive ::m ::mp) (derive ::m ::n))))
                   (defmulti own identity :hierarchy h2)
                   (defmethod own ::mp [_] :mp)
                   (defmethod own ::n [_] :n)
                   (prefer-method own ::mp ::n)");
    check(&mut session, "(own ::m)", ":mp");
}

#[test]
fn defmulti_defines_once_and_methods_can_be_removed() {
    let mut session = Repl::new();
    session.setup("(defmulti area :shape)
                   (defmethod area :square [{:keys [s]}] (* s s))
                   (defmethod area :default [_] :unknown)
                   (defmulti area :other)");
    // An existing multimethod is kept, with its dispatch function and methods.
    check(&mut session, "(area {:shape :square :s 4})", "16");
    check(&mut session, "[(= area (remove-method area :square)) (area {:shape :square :s 4})]", "[true :unknown]");
    check(&mut session, "[(= area (remove-all-methods area)) (methods area) (prefers area)]", "[true {} {}]");
    // Any other value is replaced.
    session.setup("(def plain 1) (defmulti plain identity) (defmethod plain 1 [_] :one)");
    check(&mut session, "(plain 1)", ":one");
    check(&mut session, "(defmulti plain identity)", "nil");
}

#[test]
fn defmulti_options_and_type_dispatch_follow_clojure() {
    let mut session = Repl::new();
    session.setup("(defmulti d \"doc\" {:added \"1\"} identity :default ::dd)
                   (defmethod d ::dd [x] [:dd x])
                   (defmulti dn identity :default nil)
                   (defmethod dn nil [x] [:nil x])");
    check(&mut session, "[(d 5) (dn 5) (dn nil)]", "[[:dd 5] [:nil 5] [:nil nil]]");
    check(&mut session, "(= d (defmethod d 1 [_] 1))", "true");
    // The global hierarchy is checked again after a derive.
    session.setup("(defmulti g identity) (defmethod g ::gp [_] :gp)");
    check(&mut session, "(try (g ::gc) (catch :default e (ex-message e)))",
        "\"No method in multimethod 'g' for dispatch value: :user/gc\"");
    session.setup("(derive ::gc ::gp)");
    check(&mut session, "(g ::gc)", ":gp");
    // A local of the same name does not hide the existing multimethod.
    session.setup("(let [g 1] (defmulti g :other))");
    check(&mut session, "(g ::gc)", ":gp");
    // Types dispatch by identity, as Clojure's classes do.
    session.setup("(deftype T [])
                   (defmulti ty type)
                   (defmethod ty (type \"\") [_] :string)
                   (defmethod ty T [_] :t)");
    check(&mut session, "[(ty \"s\") (ty (T.))]", "[:string :t]");
    for (source, message) in [
        ("(ty :k)", "No method in multimethod 'ty' for dispatch value: class suss.core.Keyword"),
        // A host type has no class name yet, so it is printed (#302).
        ("(ty 1)", "No method in multimethod 'ty' for dispatch value: #object[Number]"),
    ] {
        assert_eq!(printed(&mut session, &format!("(try {source} :returned (catch :default e (ex-message e)))")),
            format!("{message:?}"), "{source}");
    }
}

