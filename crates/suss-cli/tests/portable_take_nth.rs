//! Pinned ClojureScript `take-nth` lazy sequence and transducer behavior.
use suss_cli::portable_session::Session;
use wasmtime::Val;

fn number(session: &mut Session, source: &str) -> f64 {
    let value = session.eval(source).unwrap_or_else(|error| panic!("{source}: {error:?}"));
    session.inspect(&value, |mut store, value| {
        let fields = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap().fields(&mut store)?.collect::<Vec<_>>();
        let [Val::F64(bits)] = fields.as_slice() else { panic!("numeric result: {fields:?}") };
        Ok(f64::from_bits(*bits))
    }).unwrap()
}

#[test]
fn take_nth_preserves_lazy_and_transducer_arities() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        assert_eq!(number(&mut session, "(count (take-nth 1 (range 10)))"), 10.0);
        assert_eq!(number(&mut session, "(count (take-nth 2 (range 10)))"), 5.0);
        assert_eq!(number(&mut session, "(nth (take-nth 3 (range 10)) 3)"), 9.0);
        assert_eq!(number(&mut session, "(transduce (take-nth 2) + 0 (range 10))"), 20.0);
        assert_eq!(number(&mut session, "(transduce (take-nth -2) + 0 (range 10))"), 20.0);
        assert_eq!(number(&mut session, "(count (into [] (take-nth 0) [1 2 3]))"), 0.0);
        assert_eq!(number(&mut session, "(if (try (doall (take-nth nil (range 3))) false (catch :default e true)) 1 0)"), 1.0);
        assert_eq!(number(&mut session, "(if (try (transduce (take-nth nil) conj [] [1 2]) false (catch :default e true)) 1 0)"), 1.0);
    }
}
