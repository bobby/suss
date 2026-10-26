//! Core abstractions of built-in and host types are closed to user code, as
//! Clojure's interfaces on its built-in classes are (design decision
//! 2026-10-25, #275). User protocols still extend built-ins, user types still
//! implement core protocols, and built-in hashing and equality stay native.
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;

use portable_decode::{Decoder, Observation};
use suss_cli::portable_session::Session;

const CLOSED: &str = "Core protocols of built-in types cannot be extended outside suss.core";

const REJECTED: &[&str] = &[
    "(extend-type Keyword IHash (-hash [_] 91))",
    "(extend-type PersistentArrayMap IHash (-hash [_] 91))",
    "(extend-type PersistentVector IEquiv (-equiv [_ _] true))",
    "(extend-type Symbol ICounted (-count [_] 1))",
    "(extend-type PersistentVector Object (toString [_] \"v\"))",
    "(extend-type number IHash (-hash [_] 91))",
    "(extend-type string IEquiv (-equiv [_ _] true))",
    "(extend-type default IHash (-hash [_] 91))",
    "(extend-type nil ICounted (-count [_] 7))",
];

fn sessions() -> [(&'static str, Session); 2] {
    [("Runtime", Session::new_repl().unwrap()), ("Macro", Session::new_macro().unwrap())]
}

fn observe(session: &mut Session, source: &str) -> Observation {
    let mut decoder = Decoder::capture(session, 1_000).unwrap();
    let value = session.eval(source).unwrap_or_else(|error| panic!("{source}: {error}"));
    decoder.decode_session(session, &value).unwrap()
}

fn number(value: f64) -> Observation {
    Observation::Number(value.to_bits())
}

#[test]
fn core_protocols_of_builtin_types_are_closed_in_both_phases() {
    for (phase, mut session) in sessions() {
        let before = observe(&mut session, "[(hash :k) (hash {:a 1}) (= [1] [2]) (count nil)]");
        for source in REJECTED {
            let error = session.eval(source).map(|_| ()).expect_err(source).to_string();
            assert!(error.contains(CLOSED), "{phase} {source}: {error}");
        }
        // Rejection happens before any installation: behavior is unchanged.
        assert_eq!(observe(&mut session, "[(hash :k) (hash {:a 1}) (= [1] [2]) (count nil)]"), before, "{phase}");
    }
}

#[test]
fn user_protocols_and_user_types_stay_open() {
    for (phase, mut session) in sessions() {
        session.eval("(defprotocol Shout (shout [x]))").unwrap();
        session.eval("(extend-type Keyword Shout (shout [_] 1))").unwrap();
        session.eval("(extend-type number Shout (shout [_] 2))").unwrap();
        assert_eq!(observe(&mut session, "(shout :k)"), number(1.0), "{phase}");
        assert_eq!(observe(&mut session, "(shout 5)"), number(2.0), "{phase}");
        session.eval("(deftype Fixed [] IHash (-hash [_] 42) IEquiv (-equiv [_ other] (instance? Fixed other)))").unwrap();
        assert_eq!(observe(&mut session, "(hash (Fixed.))"), number(42.0), "{phase}");
        assert_eq!(observe(&mut session, "(= (Fixed.) (Fixed.))"), Observation::Bool(true), "{phase}");
        session.eval("(deftype Later [])").unwrap();
        session.eval("(extend-type Later IHash (-hash [_] 43))").unwrap();
        assert_eq!(observe(&mut session, "(hash (Later.))"), number(43.0), "{phase}");
    }
}
