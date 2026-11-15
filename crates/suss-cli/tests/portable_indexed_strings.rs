//! Strings are not arrays: as in Clojure, `aget`, `aset`, `alength` and
//! `aclone` on a string throw (#425).
use suss_cli::portable_session::Session;

#[test]
fn array_operations_on_strings_are_language_errors_and_session_recovers() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    for source in [
        "(aget \"xy\" 0)",
        "(aget \"\\uD800\" 0)",
        "(aget \"xy\" nil)",
        "(aget \"xy\" \"0\")",
        "(aget \"xy\" 0 0)",
        "(aget nil 0)",
        "(alength \"xy\")",
        "(alength \"\")",
        "(alength nil)",
        "(aset \"xy\" 0 \"z\")",
        "(aclone \"xy\")",
        "(let [f aget] (f \"xy\" 1))",
        "(let [f alength] (f \"xy\"))",
    ] {
        assert!(
            matches!(session.eval(source), Err(SessionError::Language(_))),
            "{source}"
        );
        session.collect().unwrap();
        let value = session.eval("(+ 1 2)").unwrap();
        let count = session
            .inspect(&value, |store, value| {
                Ok(suss_cli::portable_session::runtime_number(store, &value)?)
            })
            .unwrap();
        assert_eq!(count, Some(suss_cli::portable_session::RuntimeNumber::Long(3)), "{source}");
    }
}
