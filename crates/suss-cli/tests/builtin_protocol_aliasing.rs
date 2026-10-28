//! Native protocol extension targets the protocol's own identity (#279).
//! `extend-type` of a native kind installs on the protocol value and method
//! wrappers its defprotocol created, never on whatever the public vars hold
//! now, so aliasing a core method or protocol var into a user protocol cannot
//! change core dispatch or core `satisfies?`.
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;

use portable_decode::{Decoder, Observation};
use suss_cli::portable_session::Session;

/// One value of each native kind, then core reads that would change if an
/// aliased extension reached core's tables.
const KINDS: &[(&str, &str)] = &[("default", "(fn [])"), ("array", "(array 1)"), ("string", "\"s\""), ("nil", "nil")];

fn sessions() -> [(&'static str, Session); 2] {
    [("Runtime", Session::new_repl().unwrap()), ("Macro", Session::new_macro().unwrap())]
}

fn observe(session: &mut Session, source: &str) -> Observation {
    let mut decoder = Decoder::capture(session, 1_000).unwrap();
    let value = session.eval(source).unwrap_or_else(|error| panic!("{source}: {error}"));
    decoder.decode_session(session, &value).unwrap()
}

fn core_reads(value: &str) -> String {
    format!(
        "[(try (suss.core/-count {value}) (catch :default e :error))
          (try (suss.core/-name {value}) (catch :default e :error))
          (satisfies? ICounted {value}) (satisfies? INamed {value})]"
    )
}

#[test]
fn aliased_core_method_vars_do_not_receive_native_extensions() {
    for (phase, mut session) in sessions() {
        for (kind, value) in KINDS {
            let before = observe(&mut session, &core_reads(value));
            session.eval("(defprotocol IMine (-mine [x]))").unwrap();
            session.eval("(def -mine suss.core/-count)").unwrap();
            session.eval(&format!("(extend-type {kind} IMine (-mine [_] 91))")).unwrap();
            assert_eq!(observe(&mut session, &core_reads(value)), before, "{phase} {kind}");
            // The extension is the user protocol's own, reachable through its
            // original wrapper whatever the public var now holds.
            session.eval("(defprotocol IOwn (-own [x]))").unwrap();
            session.eval("(def own-original -own)").unwrap();
            session.eval("(def -own suss.core/-name)").unwrap();
            session.eval(&format!("(extend-type {kind} IOwn (-own [_] 92))")).unwrap();
            assert_eq!(observe(&mut session, &format!("(own-original {value})")), Observation::Number(92.0_f64.to_bits()), "{phase} {kind}");
            assert_eq!(observe(&mut session, &core_reads(value)), before, "{phase} {kind}");
        }
    }
}

#[test]
fn aliased_core_protocol_vars_do_not_receive_native_markers() {
    for (phase, mut session) in sessions() {
        for (kind, value) in KINDS {
            let before = observe(&mut session, &core_reads(value));
            session.eval("(defprotocol IMark (-mark [x]))").unwrap();
            session.eval("(def IMark suss.core/ICounted)").unwrap();
            session.eval(&format!("(extend-type {kind} IMark)")).unwrap();
            session.eval("(defprotocol IMarkName (-mark-name [x]))").unwrap();
            session.eval("(def IMarkName suss.core/INamed)").unwrap();
            session.eval(&format!("(extend-type {kind} IMarkName (-mark-name [_] 1))")).unwrap();
            assert_eq!(observe(&mut session, &core_reads(value)), before, "{phase} {kind}");
        }
    }
}

#[test]
fn native_extensions_follow_protocol_redefinition() {
    for (phase, mut session) in sessions() {
        session.eval("(defprotocol IAgain (-again [x]))").unwrap();
        session.eval("(extend-type string IAgain (-again [_] 1))").unwrap();
        assert_eq!(observe(&mut session, "(-again \"s\")"), Observation::Number(1.0_f64.to_bits()), "{phase}");
        // A fresh defprotocol starts with fresh tables, as it does in Clojure.
        session.eval("(defprotocol IAgain (-again [x]))").unwrap();
        assert_eq!(
            observe(&mut session, "[(satisfies? IAgain \"s\") (try (-again \"s\") (catch :default e :error))]"),
            Observation::Vector(vec![Observation::Bool(false), Observation::Keyword(None, "error".encode_utf16().collect())]),
            "{phase}"
        );
        session.eval("(extend-type string IAgain (-again [_] 2))").unwrap();
        assert_eq!(observe(&mut session, "(-again \"s\")"), Observation::Number(2.0_f64.to_bits()), "{phase}");
    }
}

/// A user defprotocol reusing a core protocol's name gets its own native
/// tables; shipped core keeps dispatching through its own (#279).
#[test]
fn user_redefinition_of_a_core_protocol_does_not_replace_core_tables() {
    for (phase, mut session) in sessions() {
        session.eval("(def f (fn []))").unwrap();
        let before = observe(&mut session, "[(hash f) (= f f) (count nil)]");
        session.enter_namespace("suss.core").unwrap();
        session.eval("(defprotocol IHash (-hash [o]))").unwrap();
        session.eval("(extend-type default IHash (-hash [_] 91))").unwrap();
        session.enter_namespace("user").unwrap();
        // The user definition dispatches through its own tables.
        assert_eq!(observe(&mut session, "(suss.core/-hash f)"), Observation::Number(91.0_f64.to_bits()), "{phase}");
        // Core's hash still uses core's default IHash implementation.
        assert_eq!(observe(&mut session, "[(hash f) (= f f) (count nil)]"), before, "{phase}");
    }
}
