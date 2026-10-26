//! Built-in `suss.core` type internals are private to core (design decision
//! 2026-10-25, #274). User code cannot read or write implementation fields or
//! class properties of built-in types, while user deftype/defrecord fields keep
//! ordinary access.
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
    ("lazy seq function read", "(.-fn (map inc [1]))"),
    ("class property read", "(.-EMPTY PersistentVector)"),
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
