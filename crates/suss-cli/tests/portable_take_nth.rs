//! Pinned ClojureScript `take-nth` lazy sequence and transducer behavior.
use suss_cli::portable_session::Session;

fn number(session: &mut Session, source: &str) -> i64 {
    let value = session.eval(source).unwrap_or_else(|error| panic!("{source}: {error:?}"));
    session.inspect(&value, |store, value| {
        match suss_cli::portable_session::runtime_number(store, &value)? {
            Some(suss_cli::portable_session::RuntimeNumber::Long(value)) => Ok(value),
            other => panic!("long result: {other:?}"),
        }
    }).unwrap()
}

#[test]
fn take_nth_preserves_lazy_and_transducer_arities() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        assert_eq!(number(&mut session, "(count (take-nth 1 (range 10)))"), 10);
        assert_eq!(number(&mut session, "(count (take-nth 2 (range 10)))"), 5);
        assert_eq!(number(&mut session, "(nth (take-nth 3 (range 10)) 3)"), 9);
        assert_eq!(number(&mut session, "(transduce (take-nth 2) + 0 (range 10))"), 20);
        assert_eq!(number(&mut session, "(transduce (take-nth -2) + 0 (range 10))"), 20);
        // JVM Clojure: the transducer's (rem i 0) throws ArithmeticException.
        assert_eq!(number(&mut session, "(if (try (into [] (take-nth 0) [1 2 3]) false (catch :default e true)) 1 0)"), 1);
        assert_eq!(number(&mut session, "(if (try (doall (take-nth nil (range 3))) false (catch :default e true)) 1 0)"), 1);
        assert_eq!(number(&mut session, "(if (try (transduce (take-nth nil) conj [] [1 2]) false (catch :default e true)) 1 0)"), 1);
    }
}
