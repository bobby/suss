//! Built-in `suss.core` type internals are private to core (design decision
//! 2026-10-25, #274). User code cannot read or write implementation fields of
//! built-in instances, read built-in prototypes, or write built-in class
//! properties. Methods and static reads stay public, and user deftype/defrecord
//! fields keep ordinary access.
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;

use portable_decode::{Decoder, Observation};
use suss_cli::portable_session::Session;

const PRIVATE: &str = "Built-in type internals are private to suss.core";

/// Each route by which user code could reach a built-in implementation field.
const SEALED: &[(&str, &str)] = &[
    ("keyword hash cache read", "(.-_hash :k)"),
    ("keyword hash cache write", "(set! (.-_hash :k) 91)"),
    ("symbol hash cache read", "(.-_hash 'sym)"),
    ("symbol name field read", "(.-name 'sym)"),
    ("list hash cache read", "(.-__hash (list 1 2))"),
    ("list first field read", "(.-first (list 1 2))"),
    ("vector tail read", "(.-tail [1 2 3])"),
    ("vector root read", "(.-root [1 2 3])"),
    ("vector count write", "(set! (.-cnt [1 2 3]) 0)"),
    ("array map storage read", "(.-arr {:a 1})"),
    ("array map storage mutation", "(aset (.-arr {:a 1}) 1 99)"),
    ("hash set storage read", "(.-hash-map #{1})"),
    ("map entry key read", "(.-key (first {:a 1}))"),
    ("sorted map tree read", "(.-tree (sorted-map 1 2))"),
    ("sorted map node read", "(.-left (first (sorted-map 1 2)))"),
    ("sorted set storage read", "(.-tree-map (sorted-set 1))"),
    ("queue storage read", "(.-front (conj (.-EMPTY PersistentQueue) 1))"),
    ("exception info field write", "(set! (.-data (ex-info \"x\" 7)) 8)"),
    ("lazy seq function read", "(.-fn (map inc [1]))"),
    ("class prototype read", "(.-prototype PersistentVector)"),
    ("class property write", "(set! (.-EMPTY PersistentVector) nil)"),
];

/// Routes that already reject built-in owners on `main`.
const ALREADY_REJECTED: &[(&str, &str)] = &[
    ("unchecked-get on built-in", "(unchecked-get :k \"_hash\")"),
    ("unchecked-set on built-in", "(unchecked-set :k \"_hash\" 91)"),
    ("aget on built-in", "(aget {:a 1} \"arr\")"),
];

/// Core helpers that would hand out built-in storage arrays.
const PRIVATE_HELPERS: &[&str] = &[
    "(array-for [1 2 3] 0)",
    "(unchecked-array-for [1 2 3] 0)",
    "(suss.core/first-array-for-longvec (vec (range 64)))",
    "(unchecked-editable-array-for (transient [1]) 0)",
];

fn sessions() -> [(&'static str, Session); 2] {
    [("Runtime", Session::new_repl().unwrap()), ("Macro", Session::new_macro().unwrap())]
}

fn observe(session: &mut Session, source: &str) -> Observation {
    let mut decoder = Decoder::capture(session, 1_000).unwrap();
    let value = session.eval(source).unwrap_or_else(|error| panic!("{source}: {error}"));
    decoder.decode_session(session, &value).unwrap()
}

fn caught(session: &mut Session, source: &str) -> Observation {
    observe(session, &format!("(try {source} :no-error (catch :default e (.-message e)))"))
}

fn text(value: &str) -> Observation {
    Observation::String(value.encode_utf16().collect())
}

#[test]
fn user_code_cannot_reach_builtin_internals_in_either_phase() {
    for (phase, mut session) in sessions() {
        for (label, source) in SEALED {
            assert_eq!(caught(&mut session, source), text(PRIVATE), "{phase} {label}: {source}");
        }
        for (label, source) in ALREADY_REJECTED {
            assert!(session.eval(source).is_err(), "{phase} {label}: {source}");
        }
        for source in PRIVATE_HELPERS {
            let error = session.eval(source).map(|_| ()).expect_err(source).to_string();
            assert!(error.contains("is private to suss.core"), "{phase} {source}: {error}");
        }
        // Sealing changes no value: built-ins still behave through public APIs.
        assert_eq!(observe(&mut session, "(count [1 2 3])"), Observation::Number(3f64.to_bits()));
        assert_eq!(observe(&mut session, "(get {:a 1} :a)"), Observation::Number(1f64.to_bits()));
        assert_eq!(observe(&mut session, "(= (hash :k) (hash :k))"), Observation::Bool(true));
        assert_eq!(observe(&mut session, "(name 'sym)"), text("sym"));
        // Static reads of built-in classes stay public, as in Clojure.
        assert_eq!(observe(&mut session, "(count (.-EMPTY PersistentVector))"), Observation::Number(0f64.to_bits()));
        assert_eq!(observe(&mut session, "(count (.fromArray PersistentVector (array 1 2) true))"), Observation::Number(2f64.to_bits()));
        // Methods of built-in instances stay public, as in Clojure.
        assert_eq!(observe(&mut session, "(.indexOf [1 2 3] 2)"), Observation::Number(1f64.to_bits()));
        // ExceptionInfo fields are public reads but not writable, like
        // Clojure's public final exception fields.
        assert_eq!(observe(&mut session, "(.-data (ex-info \"x\" 7))"), Observation::Number(7f64.to_bits()));
    }
}

#[test]
fn user_type_fields_keep_ordinary_access() {
    for (phase, mut session) in sessions() {
        session.eval("(deftype OpenPoint [x])").unwrap();
        session.eval("(defrecord* OpenRecord [x] nil nil)").unwrap();
        session.eval("(deftype OpenCell [^:mutable v])").unwrap();
        for (source, expected) in [
            ("(.-x (OpenPoint. 1))", 1.0),
            ("(.-x (OpenRecord. 2))", 2.0),
            ("(let [c (OpenCell. 1)] (set! (.-v c) 3) (.-v c))", 3.0),
        ] {
            assert_eq!(observe(&mut session, source), Observation::Number(f64::to_bits(expected)), "{phase}: {source}");
        }
    }
}

#[test]
fn core_internal_calls_are_direct_linked_against_user_redefinition() {
    // Core's own calls bind to its definitions (Clojure direct linking), so
    // redefining or with-redefs of a public core var cannot reach core storage.
    for (phase, mut session) in sessions() {
        session.eval("(def stolen (atom nil)) (def original-chunked-seq suss.core/chunked-seq)").unwrap();
        let rest = "(with-redefs [suss.core/chunked-seq (fn [v n i o] (reset! stolen n) (original-chunked-seq v n i o))] (count (rest (seq (vec (range 40))))))";
        assert_eq!(observe(&mut session, rest), Observation::Number(39f64.to_bits()), "{phase}");
        assert_eq!(observe(&mut session, "(nil? @stolen)"), Observation::Bool(true), "{phase}");
        // Hashing of built-ins does not follow a redefined protocol method var.
        let before = observe(&mut session, "(hash :k)");
        assert_eq!(observe(&mut session, "(with-redefs [suss.core/-hash (fn [_] 0)] (hash :k))"), before, "{phase}");
        // User call sites still see redefinition, as in Clojure.
        assert_eq!(observe(&mut session, "(with-redefs [suss.core/identity (fn [_] 0)] (identity 5))"), Observation::Number(0f64.to_bits()), "{phase}");
    }
}

#[test]
fn builtin_classes_resist_aliased_extension_and_node_singletons_stay_private() {
    for (phase, mut session) in sessions() {
        // Extension through an alias, a local or an expression is rejected at run time.
        for source in [
            "(do (def aliased-vector PersistentVector) (extend-type aliased-vector IHash (-hash [_] 7)))",
            "(let [t Keyword] (extend-type t IEquiv (-equiv [_ _] true)))",
            "(extend-type (identity PersistentArrayMap) IHash (-hash [_] 7))",
        ] {
            assert_eq!(caught(&mut session, source), text("Core protocols of built-in types cannot be extended outside suss.core"), "{phase}: {source}");
        }
        assert_eq!(observe(&mut session, "(= (hash [1]) (hash [1]))"), Observation::Bool(true), "{phase}");
        // Node classes and non-whitelisted statics are private.
        for source in [
            "(.-EMPTY BitmapIndexedNode)",
            "(.-EMPTY_NODE PersistentVector)",
            "(.-createAsIfByAssocComplexPath PersistentArrayMap)",
        ] {
            assert_eq!(caught(&mut session, source), text(PRIVATE), "{phase}: {source}");
        }
        // Remaining node and transient categories keep private fields.
        for source in [
            "(.-root (hash-map 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18))",
            "(.-arr (chunk-first (seq (vec (range 40)))))",
            "(.-node (seq (vec (range 40))))",
            "(.-first (cons 1 nil))",
            "(.-root (transient [1 2 3]))",
            "(.-tail (transient [1 2 3]))",
        ] {
            assert_eq!(caught(&mut session, source), text(PRIVATE), "{phase}: {source}");
        }
    }
}

#[test]
fn core_primitives_and_recursion_are_direct_linked_and_private_helpers_hidden() {
    for (phase, mut session) in sessions() {
        // Bootstrap primitives that core calls with internal storage.
        session.eval("(def seen (atom [])) (def original-aclone aclone) (def original-nil? nil?)").unwrap();
        let captured = "(with-redefs [suss.core/aclone (fn [a] (swap! seen conj a) (original-aclone a)) suss.core/nil? (fn [x] (swap! seen conj x) (original-nil? x))] (count (persistent! (conj! (transient {:a 1}) [:b 2]))))";
        assert_eq!(observe(&mut session, captured), Observation::Number(2f64.to_bits()), "{phase}");
        assert_eq!(observe(&mut session, "(empty? @seen)"), Observation::Bool(true), "{phase}");
        // Upstream defn- helpers are private to suss.core.
        for source in ["(suss.core/do-assoc [1] 5 nil 0 nil)", "(suss.core/pv-aset (array) 0 1)", "(suss.core/push-tail [1] 5 nil nil)", "suss.core/string-hash-cache"] {
            let error = session.eval(source).map(|_| ()).expect_err(source).to_string();
            assert!(error.contains("is private to suss.core"), "{phase} {source}: {error}");
        }
        // Exact WIT integers and runtime-built classes are closed to core protocol extension too.
        for source in [
            "(do (def aliased-wit WitSigned64) (extend-type aliased-wit IHash (-hash [_] 0)))",
            "(extend-type ExceptionInfo IEquiv (-equiv [_ _] true))",
        ] {
            assert!(session.eval(source).is_err(), "{phase}: {source}");
        }
    }
}
