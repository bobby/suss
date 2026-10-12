//! Compiled type-label helper used by the pinned record hash seed.
//! Authored source-only regressions; not an execution/record acceptance claim.
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};

#[test]
fn compiled_record_type_labels_follow_complete_scalar_utf16_munging_rules() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros.define("(defmacro record-label [value] (list 'quote (suss.internal.record-label/type-label value)))").unwrap();
        for (source, expected) in [
            ("(record-label user/Record)", "user.Record"),
            ("(record-label user/for)", "user.for$"),
            ("(record-label a..b/C-d?)", "a_DOT__DOT_b.C_d_QMARK_"),
            ("(record-label \".class.\")", ".class$."),
            ("(record-label \"a/\")", "a_SLASH_"),
            ("(record-label \"a/\\n\")", "a_SLASH_\n"),
            ("(record-label \"a/😀\")", "a.😀"),
        ] {
            decoder.refill(4096);
            let value = session.eval_with_macros(source, &mut macros).unwrap();
            session.collect().unwrap();
            assert_eq!(
                decoder.decode_session(&mut session, &value).unwrap(),
                Observation::String(expected.encode_utf16().collect()),
                "{source}"
            );
        }
        let value = session
            .eval_with_macros("(record-label \"\\uD800\")", &mut macros)
            .unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::String(vec![0xd800])
        );
        assert!(
            session
                .eval_with_macros("(record-label 7)", &mut macros)
                .is_err()
        );
    }
}
