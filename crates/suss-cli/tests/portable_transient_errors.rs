use suss_cli::portable_session::{Session, SessionError};
fn error(session: &mut Session, source: &str, descriptor_id: i64, message: &str) {
    let SessionError::Language(payload) = session.eval(source).unwrap_err() else {
        panic!("expected language Error")
    };
    session.collect().unwrap();
    let observed = session
        .inspect(&payload, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let descriptor = object
                .field(&mut store, 0)?
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap();
            let id = descriptor.field(&mut store, 0)?.unwrap_i64();
            let text = object
                .field(&mut store, 1)?
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            let units = text
                .elems(&mut store)?
                .map(|x| x.unwrap_i32() as u16)
                .collect::<Vec<_>>();
            Ok((id, units))
        })
        .unwrap();
    assert_eq!(
        observed,
        (descriptor_id, message.encode_utf16().collect()),
        "{source}"
    );
}

#[test]
fn transient_mutation_errors_preserve_exact_pinned_messages_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for (literal, operations) in [
            (
                "[1 2]",
                vec![
                    ("(conj! t 3)", "conj! after persistent!"),
                    ("(-assoc-n! t 0 3)", "assoc! after persistent!"),
                    ("(-pop! t)", "pop! after persistent!"),
                    ("(count t)", "count after persistent!"),
                    ("(nth t 0)", "nth after persistent!"),
                    ("(get t 0)", "lookup after persistent!"),
                    ("(persistent! t)", "persistent! called twice"),
                ],
            ),
            (
                "{:a 1}",
                vec![
                    ("(assoc! t :b 2)", "assoc! after persistent!"),
                    ("(dissoc! t :a)", "dissoc! after persistent!"),
                    ("(count t)", "count after persistent!"),
                    ("(get t :a)", "lookup after persistent!"),
                    ("(persistent! t)", "persistent! called twice"),
                ],
            ),
            (
                "{0 0 1 1 2 2 3 3 4 4 5 5 6 6 7 7 8 8}",
                vec![
                    ("(assoc! t 10 10)", "assoc! after persistent!"),
                    ("(dissoc! t 0)", "dissoc! after persistent!"),
                    ("(count t)", "count after persistent!"),
                    ("(persistent! t)", "persistent! called twice"),
                ],
            ),
        ] {
            session
                .eval(&format!(
                    "(def t (transient {literal})) (def kept (persistent! t))"
                ))
                .unwrap();
            for (operation, message) in operations {
                error(&mut session, operation, 7, message);
            }
            session.eval("kept").unwrap();
        }
    }
}
