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

#[test]
fn first_public_defonce_replaces_private_reader_fallback_once() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def make-quoted (fn [] `(+ 1 2)))").unwrap();
        session.eval("(ns cljs.core) (def reader-definition-effects 0)").unwrap();
        session.eval("(defonce sequence (do (set! reader-definition-effects (+ reader-definition-effects 1)) (fn [coll] (list 42))))").unwrap();
        session.eval("(defonce sequence (do (set! reader-definition-effects (+ reader-definition-effects 1)) (fn [coll] (list 99))))").unwrap();
        session.eval("(ns user)").unwrap();
        let value = session.eval("[(first (make-quoted)) cljs.core/reader-definition-effects]").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let actual = bridge.read(&mut session, &value, 0..1).unwrap();
        let Kind::Vector(values) = actual.kind else { panic!("reader result and initializer count") };
        assert_eq!(values[0].kind, Kind::Number(42.0));
        assert_eq!(values[1].kind, Kind::Number(1.0));
    }
}

#[test]
fn reader_fallback_preserves_declared_and_failed_defonce_initialization() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def make-quoted (fn [] `(+ 1 2)))").unwrap();
        session.eval("(ns cljs.core) (declare sequence) (def reader-definition-effects 0)").unwrap();
        let unbound = session.eval("(nil? sequence)").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        assert_eq!(bridge.read(&mut session, &unbound, 0..1).unwrap().kind, Kind::Bool(true));
        // Existing and newly compiled reader calls use the same default while
        // a declaration or failed initializer leaves the public cell unbound.
        let fresh_quote = session.eval("(count `(+ 1 2))").unwrap();
        assert_eq!(bridge.read(&mut session, &fresh_quote, 0..1).unwrap().kind, Kind::Number(3.0));
        assert!(session.eval("(defonce sequence (do (set! reader-definition-effects (+ reader-definition-effects 1)) (throw 7)))").is_err());
        session.eval("(defonce sequence (do (set! reader-definition-effects (+ reader-definition-effects 1)) (fn [coll] (list 42))))").unwrap();
        session.eval("(defonce sequence (do (set! reader-definition-effects (+ reader-definition-effects 1)) (fn [coll] (list 99))))").unwrap();
        session.eval("(ns user)").unwrap();
        let value = session.eval("[(first (make-quoted)) cljs.core/reader-definition-effects]").unwrap();
        let actual = bridge.read(&mut session, &value, 0..1).unwrap();
        let Kind::Vector(values) = actual.kind else { panic!("reader result and failed/retried initializer count") };
        assert_eq!(values[0].kind, Kind::Number(42.0));
        assert_eq!(values[1].kind, Kind::Number(2.0));
    }
}

#[test]
fn reader_callee_is_selected_before_unquote_redefines_its_live_binding() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(ns cljs.core) (def sequence (fn [coll] (list 42))) (def reader-effects 0)").unwrap();
        let first = session.eval("(first `(~(do (set! reader-effects (+ reader-effects 1)) (def sequence (fn [coll] (list 99))) 1)))").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        assert_eq!(bridge.read(&mut session, &first, 0..1).unwrap().kind, Kind::Number(42.0));
        let after = session.eval("[(first `(+ 1 2)) reader-effects]").unwrap();
        let actual = bridge.read(&mut session, &after, 0..1).unwrap();
        let Kind::Vector(values) = actual.kind else { panic!("new callee and once-only unquote effects") };
        assert_eq!(values[0].kind, Kind::Number(99.0));
        assert_eq!(values[1].kind, Kind::Number(1.0));
    }
}
