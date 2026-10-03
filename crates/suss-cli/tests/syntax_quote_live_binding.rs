//! Reader constructors must obey the same live cells as ordinary calls.
use suss_cli::{portable_macro_data::FormBridge, portable_session::Session};
use suss_reader::forms::Kind;

#[test]
fn syntax_quote_constructor_observes_core_sequence_redefinition() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval("(ns cljs.core) (def sequence (fn [coll] (list 42)))")
            .unwrap();
        session.eval("(ns user)").unwrap();
        let value = session.eval("(first `(+ 1 2))").unwrap();
        session.collect().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let actual = bridge.read(&mut session, &value, 0..16).unwrap();
        assert_eq!(actual.kind, Kind::Number(42.0));
    }
}

#[test]
fn syntax_quote_old_closure_observes_later_core_sequence_definition() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def make-quoted (fn [] `(+ 1 2)))").unwrap();
        session
            .eval("(ns cljs.core) (def sequence (fn [coll] (list 42)))")
            .unwrap();
        session.eval("(ns user)").unwrap();
        let value = session.eval("(first (make-quoted))").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let actual = bridge.read(&mut session, &value, 0..21).unwrap();
        assert_eq!(actual.kind, Kind::Number(42.0));
    }
}

#[test]
fn reader_fallback_stays_private_and_does_not_replay_after_definition() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def make-quoted (fn [] `(+ 1 2)))").unwrap();
        assert!(session.eval("(cljs.core/sequence (list 1))").is_err());
        session.eval("(ns cljs.core) (def sequence (fn [coll] (list 42)))").unwrap();
        session.eval("(ns user)").unwrap();
        // Another reader expression must not overwrite the existing live cell.
        session.eval("(def another-quoted (fn [] `(+ 3 4)))").unwrap();
        let value = session.eval("(+ (first (make-quoted)) (first (another-quoted)))").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let actual = bridge.read(&mut session, &value, 0..1).unwrap();
        assert_eq!(actual.kind, Kind::Number(84.0));
    }
}

#[test]
fn failed_reader_compile_does_not_publish_the_support_cell() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        assert!(session.eval("(do `(+ 1 2) missing-reader-name)").is_err());
        assert!(session.eval("(cljs.core/sequence (list 1))").is_err());
        session.eval("(def make-quoted (fn [] `(+ 1 2)))").unwrap();
        let value = session.eval("(count (make-quoted))").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let actual = bridge.read(&mut session, &value, 0..1).unwrap();
        assert_eq!(actual.kind, Kind::Number(3.0));
    }
}

#[test]
fn reader_support_recovers_from_failed_initializer_and_reset() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        assert!(session.eval("(do `(+ 1 2) (throw 7))").is_err());
        assert!(session.eval("(cljs.core/sequence (list 1))").is_err());
        session.eval("(def make-quoted (fn [] `(+ 1 2)))").unwrap();
        session.reset().unwrap();
        assert!(session.eval("(make-quoted)").is_err());
        assert!(session.eval("(cljs.core/sequence (list 1))").is_err());
        let value = session.eval("(count `(+ 1 2))").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let actual = bridge.read(&mut session, &value, 0..1).unwrap();
        assert_eq!(actual.kind, Kind::Number(3.0));
    }
}
