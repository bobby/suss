//! Built-in types cannot be constructed with forged internals outside
//! suss.core (#279). Clojure's persistent collection, node, keyword and symbol
//! constructors are not public; user code builds those values through core
//! functions. MapEntry's constructor is public, as in Clojure, but its hash
//! cache cannot be preset, and core's generated `->Type` factories of types
//! with private internals are closed to user code.
#[path = "support/compilation_fuel.rs"]
mod compilation_fuel;
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;

use portable_decode::{Decoder, Observation};
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};

const PRIVATE: &str = "Built-in type internals are private to suss.core";
const HASH: &str = "Built-in hash caches are private to suss.core";

const REJECTED: &[(&str, &str)] = &[
    ("(PersistentVector. nil 1 5 nil (array 1) 42)", PRIVATE),
    ("(new suss.core/PersistentVector nil 0 5 nil (array) nil)", PRIVATE),
    ("(let [c PersistentVector] (new c nil 0 5 nil (array) nil))", PRIVATE),
    ("(VectorNode. nil (array))", PRIVATE),
    ("(Keyword. nil \"k\" \"k\" 7)", PRIVATE),
    ("(Symbol. nil \"s\" \"s\" 7 nil)", PRIVATE),
    ("(PersistentArrayMap. nil 1 (array :a 1) 9)", PRIVATE),
    ("(->PersistentVector nil 0 5 nil (array) nil)", PRIVATE),
    ("(->VectorNode nil (array))", PRIVATE),
    ("(->Keyword nil \"k\" \"k\" 7)", PRIVATE),
    ("(MapEntry. :a 1 42)", HASH),
    ("(->MapEntry :a 1 42)", HASH),
    ("(LazySeq. nil (fn [] nil) nil 42)", HASH),
];

fn sessions() -> [(&'static str, Session); 2] {
    [("Runtime", Session::new_repl().unwrap()), ("Macro", Session::new_macro().unwrap())]
}

fn observe(session: &mut Session, source: &str) -> Observation {
    let mut decoder = Decoder::capture(session, 1_000).unwrap();
    let value = session.eval(source).unwrap_or_else(|error| panic!("{source}: {error}"));
    decoder.decode_session(session, &value).unwrap()
}

fn text(value: &str) -> Observation {
    Observation::String(value.encode_utf16().collect())
}

#[test]
fn builtin_internals_cannot_be_forged_by_construction() {
    for (phase, mut session) in sessions() {
        for (source, message) in REJECTED {
            assert_eq!(
                observe(&mut session, &format!("(try {source} :no-error (catch :default e (.-message e)))")),
                text(message),
                "{phase} {source}"
            );
        }
    }
}

#[test]
fn public_constructions_keep_working() {
    for (phase, mut session) in sessions() {
        assert_eq!(
            observe(&mut session, "(let [e (MapEntry. :a 1 nil)] [(key e) (val e) (= e [:a 1]) (= (hash e) (hash [:a 1]))])"),
            Observation::Vector(vec![
                Observation::Keyword(None, "a".encode_utf16().collect()),
                Observation::Number(1.0_f64.to_bits()),
                Observation::Bool(true),
                Observation::Bool(true),
            ]),
            "{phase}"
        );
        // Core macro expansions in user code construct core types: defrecord's
        // seq (MapEntry) and iterator (RecordIter), and lazy-seq.
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
        let mut decoder = Decoder::capture(&mut session, 1_000).unwrap();
        let value = session
            .eval_with_macros(
                "(do (defrecord R [a b])
                     (let [r (->R 1 2) it (-iterator r)]
                       [(vec (seq r)) (.next it) (.next it) (.hasNext it) (count (lazy-seq [1 2]))]))",
                &mut macros,
            )
            .unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            observe(&mut session, "[[[:a 1] [:b 2]] [:a 1] [:b 2] false 2]"),
            "{phase}"
        );
        assert_eq!(
            observe(&mut session, "(vec (LazySeq. nil (fn [] (list 1 2)) nil nil))"),
            observe(&mut session, "[1 2]"),
            "{phase}"
        );
        // User types still construct, with or without new.
        assert_eq!(
            observe(&mut session, "(do (deftype Mine [x]) [(.-x (Mine. 1)) (.-x (new Mine 2)) (.-x (->Mine 3))])"),
            observe(&mut session, "[1 2 3]"),
            "{phase}"
        );
    }
}

/// `array-map` copies its arguments, as Clojure's does through to-array, so
/// `(apply array-map (array-seq a))` does not share `a` (#279). `vec` of an
/// array adopts it, but Clojure leaves modifying it afterwards undefined.
#[test]
fn caller_owned_arrays_follow_clojure() {
    for (phase, mut session) in sessions() {
        assert_eq!(
            observe(&mut session, "(let [a (array :a 1) m (apply array-map (array-seq a))] (aset a 1 2) [(get m :a) (array-map :b 3)])"),
            observe(&mut session, "[1 {:b 3}]"),
            "{phase}"
        );
    }
}
