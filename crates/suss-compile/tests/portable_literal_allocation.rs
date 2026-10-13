//! Execute literal allocation through both phases, including the bounded-stack path.
#[path = "support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};
use suss_compile::portable_session::{Session, SessionValue};
use wasmtime::Val;

fn units(session: &mut Session, value: &SessionValue) -> Vec<u16> {
    session.collect().unwrap();
    session.inspect(value, |mut store, value| {
        match Decoder::new(20_000).decode(&mut store, &value)? {
            Observation::String(units) => Ok(units),
            other => panic!("expected UTF16 string, got {other:?}"),
        }
    }).unwrap()
}
fn alter_first_unit(session: &mut Session, value: &SessionValue) {
    // Private host inspection distinguishes fresh arrays without relying on
    // language string equality (which intentionally compares contents).
    session.inspect(value, |mut store, value| {
        value.unwrap_anyref().unwrap().as_array(&store)?.unwrap()
            .set(&mut store, 0, Val::I32(66))?;
        Ok(())
    }).unwrap();
}
#[test]
fn literals_preserve_all_units_and_fresh_storage_at_both_allocation_sizes() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for length in [0, 1, 60, 61, 4096] {
            let text = "A".repeat(length);
            session.eval(&format!("(def fresh (fn [] \"{text}\\uD800😀\\uDC00\"))")).unwrap();
            let first = session.eval("(fresh)").unwrap();
            let expected = [text.encode_utf16().collect::<Vec<_>>(), vec![0xd800, 0xd83d, 0xde00, 0xdc00]].concat();
            assert_eq!(units(&mut session, &first), expected);
            alter_first_unit(&mut session, &first);
            let second = session.eval("(fresh)").unwrap();
            assert_eq!(units(&mut session, &second), expected, "fresh storage at {length}");
        }
        let empty = session.eval("\"\"").unwrap();
        assert_eq!(units(&mut session, &empty), Vec::<u16>::new());
    }
}
#[test]
fn fixed_and_multi_arity_closures_eagerly_keep_fresh_long_source_names() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for name in ["short".to_owned(), "n".repeat(4096)] {
            for body in ["[x] x", "([x] x) ([x y] y)"] {
                session.eval(&format!("(def make-named (fn [] (fn {name} {body}))) (def a (make-named)) (def b (make-named))")).unwrap();
                let a = session.eval("(suss.bootstrap/function-name a)").unwrap();
                let b = session.eval("(suss.bootstrap/function-name b)").unwrap();
                let expected = format!("user$make_named_$_{name}").encode_utf16().collect::<Vec<_>>();
                assert_eq!(units(&mut session, &a), expected);
                alter_first_unit(&mut session, &a);
                assert_eq!(units(&mut session, &b), expected);
            }
        }
    }
}
