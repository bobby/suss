//! Reader constructors must obey the same live cells as ordinary calls.
#[path = "support/eval_with_compilation_fuel.rs"]
mod semantic_compilation;
use suss_cli::{portable_macro_data::FormBridge, portable_session::Session};
use suss_reader::forms::Kind;

// Exercise the private reader support with real imported helpers but no public
// sequence definition. Its forward declaration remains unbound for dependencies.
// Default bootstrapped behavior is tested below.
fn install_fallback_core(session: &mut Session) {
    use suss_reader::forms::{Kind, read_forms};
    let source = include_str!("../../../runtime/core-import/suss/core.sus");
    let mut omitted = Vec::new();
    let mut definitions = 0;
    for form in read_forms(source).unwrap() {
        let Kind::List(parts) = &form.kind else {
            continue;
        };
        let Some(Kind::Symbol(head)) = parts.first().map(|f| &f.kind) else {
            continue;
        };
        if head.name == "def"
            && matches!(parts.get(1).map(|f| &f.kind), Some(Kind::Symbol(name)) if name.name == "sequence")
        {
            omitted.push(form.span.clone());
            definitions += 1;
        }
    }
    assert_eq!(
        definitions, 1,
        "fixture must omit exactly the public sequence form"
    );
    let mut bytes = source.as_bytes().to_vec();
    for span in omitted {
        for byte in &mut bytes[span] {
            if *byte != b'\n' && *byte != b'\r' {
                *byte = b' ';
            }
        }
    }
    semantic_compilation::eval(session, std::str::from_utf8(&bytes).unwrap()).unwrap();
    session.enter_namespace("user").unwrap();
    let value = session.eval("(nil? sequence)").unwrap();
    let bridge = FormBridge::new(session).unwrap();
    assert_eq!(
        bridge.read(session, &value, 0..1).unwrap().kind,
        Kind::Bool(true),
        "fixture must leave sequence uninitialized"
    );
}
fn fallback_sessions() -> [Session; 2] {
    let mut sessions = [
        Session::new().unwrap(),
        Session::with_options_in(
            Default::default(),
            suss_compile::portable::resolve::Phase::Macro,
        )
        .unwrap(),
    ];
    for session in &mut sessions {
        install_fallback_core(session);
    }
    sessions
}

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
    for mut session in fallback_sessions() {
        session.eval("(def make-quoted (fn [] `(+ 1 2)))").unwrap();
        assert!(session.eval("(cljs.core/sequence (list 1))").is_err());
        session
            .eval("(ns cljs.core) (def sequence (fn [coll] (list 42)))")
            .unwrap();
        session.eval("(ns user)").unwrap();
        // Another reader expression must not overwrite the existing live cell.
        session
            .eval("(def another-quoted (fn [] `(+ 3 4)))")
            .unwrap();
        let value = session
            .eval("(+ (first (make-quoted)) (first (another-quoted)))")
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let actual = bridge.read(&mut session, &value, 0..1).unwrap();
        assert_eq!(actual.kind, Kind::Number(84.0));
    }
}

#[test]
fn failed_reader_compile_does_not_publish_the_support_cell() {
    for mut session in fallback_sessions() {
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
    for mut session in fallback_sessions() {
        assert!(session.eval("(do `(+ 1 2) (throw 7))").is_err());
        assert!(session.eval("(cljs.core/sequence (list 1))").is_err());
        session.eval("(def make-quoted (fn [] `(+ 1 2)))").unwrap();
        session.reset().unwrap();
        assert!(session.eval("(make-quoted)").is_err());
        install_fallback_core(&mut session);
        assert!(session.eval("(cljs.core/sequence (list 1))").is_err());
        let value = session.eval("(count `(+ 1 2))").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let actual = bridge.read(&mut session, &value, 0..1).unwrap();
        assert_eq!(actual.kind, Kind::Number(3.0));
    }
}

#[test]
fn first_public_defonce_replaces_private_reader_fallback_once() {
    for mut session in fallback_sessions() {
        session.eval("(def make-quoted (fn [] `(+ 1 2)))").unwrap();
        session
            .eval("(ns cljs.core) (def reader-definition-effects 0)")
            .unwrap();
        session.eval("(defonce sequence (do (set! reader-definition-effects (+ reader-definition-effects 1)) (fn [coll] (list 42))))").unwrap();
        session.eval("(defonce sequence (do (set! reader-definition-effects (+ reader-definition-effects 1)) (fn [coll] (list 99))))").unwrap();
        session.eval("(ns user)").unwrap();
        let value = session
            .eval("[(first (make-quoted)) cljs.core/reader-definition-effects]")
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let actual = bridge.read(&mut session, &value, 0..1).unwrap();
        let Kind::Vector(values) = actual.into_parts().2 else {
            panic!("reader result and initializer count")
        };
        assert_eq!(values[0].kind, Kind::Number(42.0));
        assert_eq!(values[1].kind, Kind::Number(1.0));
    }
}

#[test]
fn reader_fallback_preserves_declared_and_failed_defonce_initialization() {
    for mut session in fallback_sessions() {
        session.eval("(def make-quoted (fn [] `(+ 1 2)))").unwrap();
        session
            .eval("(ns cljs.core) (declare sequence) (def reader-definition-effects 0)")
            .unwrap();
        let unbound = session.eval("(nil? sequence)").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        assert_eq!(
            bridge.read(&mut session, &unbound, 0..1).unwrap().kind,
            Kind::Bool(true)
        );
        // Existing and newly compiled reader calls use the same default while
        // a declaration or failed initializer leaves the public cell unbound.
        let fresh_quote = session.eval("(count `(+ 1 2))").unwrap();
        assert_eq!(
            bridge.read(&mut session, &fresh_quote, 0..1).unwrap().kind,
            Kind::Number(3.0)
        );
        assert!(session.eval("(defonce sequence (do (set! reader-definition-effects (+ reader-definition-effects 1)) (throw 7)))").is_err());
        session.eval("(defonce sequence (do (set! reader-definition-effects (+ reader-definition-effects 1)) (fn [coll] (list 42))))").unwrap();
        session.eval("(defonce sequence (do (set! reader-definition-effects (+ reader-definition-effects 1)) (fn [coll] (list 99))))").unwrap();
        session.eval("(ns user)").unwrap();
        let value = session
            .eval("[(first (make-quoted)) cljs.core/reader-definition-effects]")
            .unwrap();
        let actual = bridge.read(&mut session, &value, 0..1).unwrap();
        let Kind::Vector(values) = actual.into_parts().2 else {
            panic!("reader result and failed/retried initializer count")
        };
        assert_eq!(values[0].kind, Kind::Number(42.0));
        assert_eq!(values[1].kind, Kind::Number(2.0));
    }
}

#[test]
fn reader_callee_is_selected_before_unquote_redefines_its_live_binding() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval("(ns cljs.core) (def sequence (fn [coll] (list 42))) (def reader-effects 0)")
            .unwrap();
        let first = session.eval("(first `(~(do (set! reader-effects (+ reader-effects 1)) (def sequence (fn [coll] (list 99))) 1)))").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        assert_eq!(
            bridge.read(&mut session, &first, 0..1).unwrap().kind,
            Kind::Number(42.0)
        );
        let after = session.eval("[(first `(+ 1 2)) reader-effects]").unwrap();
        let actual = bridge.read(&mut session, &after, 0..1).unwrap();
        let Kind::Vector(values) = actual.into_parts().2 else {
            panic!("new callee and once-only unquote effects")
        };
        assert_eq!(values[0].kind, Kind::Number(99.0));
        assert_eq!(values[1].kind, Kind::Number(1.0));
    }
}

#[test]
fn bootstrapped_public_sequence_survives_reader_failures_defonce_and_reset() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval("(def saved-sequence cljs.core/sequence)")
            .unwrap();
        assert!(session.eval("(do `(+ 1 2) missing-reader-name)").is_err());
        assert!(session.eval("(do `(+ 1 2) (throw 7))").is_err());
        session.eval("(ns cljs.core) (def reader-definition-effects 0) (defonce sequence (do (set! reader-definition-effects (+ reader-definition-effects 1)) (fn [coll] (list 42))))").unwrap();
        session.enter_namespace("user").unwrap();
        let value = session.eval("[(identical? saved-sequence cljs.core/sequence) cljs.core/reader-definition-effects (count `(+ 1 2)) (count (cljs.core/sequence (list 7 11)))]").unwrap();
        session.collect().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let Kind::Vector(values) = bridge.read(&mut session, &value, 0..1).unwrap().into_parts().2 else {
            panic!("public reader results")
        };
        assert_eq!(
            values.iter().map(|v| v.kind.clone()).collect::<Vec<_>>(),
            vec![
                Kind::Bool(true),
                Kind::Number(0.0),
                Kind::Number(3.0),
                Kind::Number(2.0)
            ]
        );
        session.reset().unwrap();
        assert!(session.eval("saved-sequence").is_err());
        let value = session
            .eval("(count (cljs.core/sequence (list 7 11)))")
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        assert_eq!(
            bridge.read(&mut session, &value, 0..1).unwrap().into_parts().2,
            Kind::Number(2.0)
        );
    }
}
