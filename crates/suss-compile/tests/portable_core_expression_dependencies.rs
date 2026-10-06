//! Retained core prerequisites for the unchanged compiled expression corpus.
#![cfg(not(target_family = "wasm"))]
#[path = "support/portable_compare.rs"]
mod portable_compare;
#[path = "support/portable_decode.rs"]
mod portable_decode;
use portable_decode::Decoder;
use suss_compile::portable_session::Session;
use suss_reader::{ParserState, parse_all};

#[test]
fn retained_numeric_and_stack_definitions_preserve_scalar_edges_in_both_phases() {
    for macro_phase in [false, true] {
        let mut session = if macro_phase {
            Session::new_macro().unwrap()
        } else {
            Session::new_repl().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
        for (source, expected) in [
            ("(abs -0.0)", "0.0"),
            ("(abs 0.0)", "0.0"),
            ("(abs nil)", "0.0"),
            ("(abs \"-2\")", "2.0"),
            ("(abs ##-Inf)", "##Inf"),
            ("(abs ##NaN)", "##NaN"),
            ("(NaN? \"not a number\")", "true"),
            ("(NaN? nil)", "false"),
            ("(NaN? false)", "false"),
            ("(NaN? ##NaN)", "true"),
            ("(integer? -0.0)", "true"),
            ("(integer? 1.5)", "false"),
            ("(integer? 1e20)", "true"),
            ("(integer? 1e21)", "false"),
            ("(integer? -1e21)", "false"),
            ("(integer? ##NaN)", "false"),
            ("(integer? ##Inf)", "false"),
            ("(integer? ##-Inf)", "false"),
            ("(integer? \"1\")", "false"),
            ("(max 42)", "42"),
            ("(max 1 5 3 4)", "5"),
            ("(min 42)", "42"),
            ("(min 5 1 3 4)", "1"),
            ("(max 0.0 -0.0)", "-0.0"),
            ("(min -0.0 0.0)", "0.0"),
            ("(max ##NaN 1)", "##NaN"),
            ("(min 1 ##NaN)", "##NaN"),
            ("(max \"2\" 1)", "\"2\""),
            ("(min \"2\" 3)", "\"2\""),
            ("(even? 1e20)", "true"),
            ("(even? -2)", "true"),
            ("(odd? -3)", "true"),
            ("(peek nil)", "nil"),
            ("(pop nil)", "nil"),
            ("(peek '(1 2))", "1"),
            ("(pop '(1 2))", "(2)"),
            ("(peek [1 2])", "2"),
            ("(pop [1 2])", "[1]"),
        ] {
            let expected = parse_all(expected, &mut ParserState::new("suss"))
                .unwrap()
                .remove(0);
            let value = session
                .eval(source)
                .unwrap_or_else(|error| panic!("phase={macro_phase} {source}: {error}"));
            session.collect().unwrap();
            let actual = decoder.decode_session(&mut session, &value).unwrap();
            assert!(
                portable_compare::matches(&expected, &actual),
                "phase={macro_phase} {source}: expected {expected:?}, actual {actual:?}"
            );
        }
    }
}

#[test]
fn retained_parity_error_branch_keeps_actual_scalar_message() {
    let mut session = Session::new_repl().unwrap();
    let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
    let value = session
        .eval("(try (even? 1.5) (catch :default error (.-message error)))")
        .unwrap();
    session.collect().unwrap();
    assert_eq!(
        decoder.decode_session(&mut session, &value).unwrap(),
        portable_decode::Observation::String(
            "Argument must be an integer: 1.5".encode_utf16().collect()
        )
    );
}

#[test]
fn retained_internal_string_conversion_keeps_runtime_edges_in_both_phases() {
    for macro_phase in [false, true] {
        let mut session = if macro_phase {
            Session::new_macro().unwrap()
        } else {
            Session::new_repl().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
        // apply deliberately observes the pinned runtime helper, rather than
        // the separate compiler macro of the same name.
        for (source, expected) in [
            (r#"(apply suss.core/str_ [])"#, ""),
            (r#"(apply suss.core/str_ [nil])"#, ""),
            (r#"(apply suss.core/str_ [nil 1])"#, ""),
            (r#"(apply suss.core/str_ [false])"#, "false"),
            (r#"(apply suss.core/str_ [true])"#, "true"),
            (r#"(apply suss.core/str_ [-0.0])"#, "0"),
            (r#"(apply suss.core/str_ [1.5])"#, "1.5"),
            (r#"(apply suss.core/str_ [1e20])"#, "100000000000000000000"),
            (r#"(apply suss.core/str_ [1e21])"#, "1e+21"),
            (r#"(apply suss.core/str_ [1e-6])"#, "0.000001"),
            (r#"(apply suss.core/str_ [1e-7])"#, "1e-7"),
            (r#"(apply suss.core/str_ [5e-324])"#, "5e-324"),
            (r#"(apply suss.core/str_ [##NaN])"#, "NaN"),
            (r#"(apply suss.core/str_ [##Inf])"#, "Infinity"),
            (r#"(apply suss.core/str_ [##-Inf])"#, "-Infinity"),
            (
                r#"(apply suss.core/str_ ["prefix" nil true -0.0 1e21 ##NaN ##Inf])"#,
                "prefixtrue01e+21NaNInfinity",
            ),
            (r#"(apply suss.core/str_ [:key])"#, ":key"),
        ] {
            let value = session
                .eval(source)
                .unwrap_or_else(|error| panic!("phase={macro_phase} {source}: {error}"));
            session.collect().unwrap();
            assert_eq!(
                decoder.decode_session(&mut session, &value).unwrap(),
                portable_decode::Observation::String(expected.encode_utf16().collect()),
                "phase={macro_phase} {source}"
            );
        }
    }
}

#[test]
fn error_message_property_observes_actual_typed_error_after_gc() {
    for macro_phase in [false, true] {
        let mut session = if macro_phase {
            Session::new_macro().unwrap()
        } else {
            Session::new_repl().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        for (source, expected) in [
            (
                r#"(try (even? 1.5) (catch :default error (ex-message error)))"#,
                "Argument must be an integer: 1.5",
            ),
            (
                r#"(.-message (suss.bootstrap/error "known message"))"#,
                "known message",
            ),
        ] {
            let value = session
                .eval(source)
                .unwrap_or_else(|error| panic!("{source}: {error}"));
            session.collect().unwrap();
            assert_eq!(
                decoder.decode_session(&mut session, &value).unwrap(),
                portable_decode::Observation::String(expected.encode_utf16().collect()),
                "phase={macro_phase} {source}"
            );
        }
        let held = session
            .eval(r#"(def held-error (suss.bootstrap/error (aget "😀" 0)))"#)
            .unwrap();
        session.collect().unwrap();
        let message = session.eval("(.-message held-error)").unwrap();
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &message).unwrap(),
            portable_decode::Observation::String(vec![0xd83d])
        );
        // Retaining and inspecting a separate root must not depend on the source
        // name or on any public printing/conversion function.
        session.inspect(&held, |_store, _value| Ok(())).unwrap();
    }
}

#[test]
fn private_string_concat_rejects_non_strings_after_once_only_operand_effects() {
    for macro_phase in [false, true] {
        let mut session = if macro_phase {
            Session::new_macro().unwrap()
        } else {
            Session::new_repl().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        for (left, right) in [("\"left\"", "7"), ("7", "\"right\""), ("nil", "nil")] {
            session.eval("(def concat-effects (atom []))").unwrap();
            let source = format!(
                "(try (suss.bootstrap/concat-string
                    (do (swap! concat-effects conj :left) {left})
                    (do (swap! concat-effects conj :right) {right}))
                  (catch :default error :rejected))"
            );
            let value = session.eval(&source).unwrap();
            session.collect().unwrap();
            assert_eq!(
                decoder.decode_session(&mut session, &value).unwrap(),
                portable_decode::Observation::Keyword(None, "rejected".encode_utf16().collect())
            );
            let effects = session.eval("(deref concat-effects)").unwrap();
            session.collect().unwrap();
            assert_eq!(
                decoder.decode_session(&mut session, &effects).unwrap(),
                portable_decode::Observation::Vector(vec![
                    portable_decode::Observation::Keyword(None, "left".encode_utf16().collect()),
                    portable_decode::Observation::Keyword(None, "right".encode_utf16().collect()),
                ]),
                "phase={macro_phase} {source}"
            );
        }
    }
}
