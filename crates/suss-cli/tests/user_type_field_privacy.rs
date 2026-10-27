//! User deftype/defrecord fields follow Clojure (#278): fields are public and
//! final unless declared mutable, and mutable fields are private to their type.
//! Only the type's own inline method bodies, including closures created in
//! them, may read or `set!` a mutable field by name.
#[path = "support/compilation_fuel.rs"]
mod compilation_fuel;
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;

use portable_decode::{Decoder, Observation};
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};

const PRIVATE: &str = "Mutable fields are private to their type";
const IMMUTABLE: &str = "Cannot assign an immutable field";

const DEFINITIONS: &str = "
(defprotocol ICell
  (cell-get [c]) (cell-set! [c x]) (named-get [c]) (named-set! [c x])
  (peer-get [c o]) (peer-set! [c o x]) (via-closure [c]) (reader [c])
  (other-get [c o]) (set-k! [c x]))
(defprotocol IPeek (peek-v [c]))
(deftype Other [^:mutable w])
(deftype Cell [^:mutable v k]
  ICell
  (cell-get [_] v)
  (cell-set! [_ x] (set! v x) v)
  (named-get [this] (.-v this))
  (named-set! [this x] (set! (.-v this) x) (.-v this))
  (peer-get [_ o] (.-v o))
  (peer-set! [_ o x] (set! (.-v o) x) (.-v o))
  (via-closure [this] ((fn [] (set! (.-v this) 9) (.-v this))))
  (reader [this] (fn [] (.-v this)))
  (other-get [_ o] (.-w o))
  (set-k! [this x] (set! (.-k this) x)))
(defrecord Rec [a])
";

/// Both phases, with DEFINITIONS loaded through the compiled `defrecord` macro.
fn sessions() -> [(&'static str, Session); 2] {
    [("Runtime", Session::new_repl().unwrap()), ("Macro", Session::new_macro().unwrap())].map(
        |(phase, mut session)| {
            let mut macros = CompiledMacros::new().unwrap();
            macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
            session.eval_with_macros(DEFINITIONS, &mut macros).unwrap();
            (phase, session)
        },
    )
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

fn number(value: f64) -> Observation {
    Observation::Number(value.to_bits())
}

#[test]
fn mutable_fields_work_inside_their_own_type_bodies() {
    for (phase, mut session) in sessions() {
        for (source, expected) in [
            ("(cell-get (Cell. 1 2))", 1.0),
            ("(cell-set! (Cell. 1 2) 5)", 5.0),
            ("(named-get (Cell. 1 2))", 1.0),
            ("(let [c (Cell. 1 2)] (named-set! c 4) (cell-get c))", 4.0),
            // Another instance of the same type, from a method body.
            ("(peer-get (Cell. 0 0) (Cell. 7 0))", 7.0),
            ("(let [o (Cell. 0 0)] (peer-set! (Cell. 0 0) o 8) (cell-get o))", 8.0),
            // Closures created in method bodies, called there and later.
            ("(via-closure (Cell. 1 2))", 9.0),
            ("(let [c (Cell. 3 2) f (reader c)] (cell-set! c 6) (f))", 6.0),
        ] {
            assert_eq!(observe(&mut session, source), number(expected), "{phase}: {source}");
        }
    }
}

#[test]
fn mutable_fields_are_private_outside_their_type() {
    for (phase, mut session) in sessions() {
        session.eval("(extend-type Cell IPeek (peek-v [c] (.-v c)))").unwrap();
        for source in [
            "(.-v (Cell. 1 2))",
            "(set! (.-v (Cell. 1 2)) 3)",
            "(.v (Cell. 1 2))",
            "(let [f (fn [c] (.-v c))] (f (Cell. 1 2)))",
            // Extension outside the type body is not part of the type.
            "(peek-v (Cell. 1 2))",
            // Another type's mutable field, from this type's method body.
            "(other-get (Cell. 0 0) (Other. 5))",
            "(.-w (Other. 5))",
            // A record's internal mutable hash cache.
            "(.-__hash (->Rec 1))",
        ] {
            assert_eq!(caught(&mut session, source), text(PRIVATE), "{phase}: {source}");
        }
        // Dynamic property access never reaches a user type's fields.
        for source in [
            "(unchecked-get (Cell. 1 2) \"v\")",
            "(aget (Cell. 1 2) \"v\")",
            "(let [c (Cell. 1 2)] (unchecked-set c \"v\" 5) c)",
            "(let [c (Cell. 1 2)] (aset c \"v\" 5) c)",
        ] {
            assert!(matches!(caught(&mut session, source), Observation::String(_)), "{phase}: {source}");
        }
        // A rejected write changes nothing.
        assert_eq!(
            observe(&mut session, "(let [c (Cell. 1 2)] (try (set! (.-v c) 3) (catch :default _ nil)) (cell-get c))"),
            number(1.0), "{phase}");
    }
}

#[test]
fn immutable_fields_are_public_reads_and_never_writable() {
    for (phase, mut session) in sessions() {
        for (source, expected) in [
            ("(.-k (Cell. 1 2))", 2.0),
            ("(.-a (->Rec 1))", 1.0),
            ("(:a (->Rec 1))", 1.0),
            ("(:a (assoc (->Rec 1) :a 3))", 3.0),
        ] {
            assert_eq!(observe(&mut session, source), number(expected), "{phase}: {source}");
        }
        assert_eq!(observe(&mut session, "(= (hash (->Rec 1)) (hash (->Rec 1)))"), Observation::Bool(true));
        for source in [
            "(set! (.-k (Cell. 1 2)) 3)",
            "(set! (.-a (->Rec 1)) 2)",
            // Inside the type body too, through named access.
            "(set-k! (Cell. 1 2) 3)",
        ] {
            assert_eq!(caught(&mut session, source), text(IMMUTABLE), "{phase}: {source}");
        }
        assert_eq!(observe(&mut session, "(let [c (Cell. 1 2)] (try (set-k! c 3) (catch :default _ nil)) (.-k c))"),
            number(2.0), "{phase}");
        // The lexical path still rejects it at compile time.
        let error = session.eval("(deftype Fixed [k] ICell (set-k! [_ x] (set! k x)))").unwrap_err();
        assert!(error.to_string().contains("Cannot assign a local or immutable field"), "{phase}: {error}");
    }
}
