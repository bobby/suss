use suss_cli::portable_session::{Session, SessionError};
use wasmtime::Val;
fn number(session: &mut Session, source: &str) -> f64 {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |mut store, value| {
            let fields = value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap()
                .fields(&mut store)?
                .collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Number")
            };
            Ok(f64::from_bits(*bits))
        })
        .unwrap()
}
#[test]
fn retained_map_mutation_helpers_execute_all_dissoc_arities_and_sequence_dependencies() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def m {:a 1 :b 2 :c 3})").unwrap();
        session.collect().unwrap();
        for (source, expected) in [
            ("(count (dissoc m))", 3.0),
            ("(count (dissoc m :a))", 2.0),
            ("(count (dissoc m :a :b :c))", 0.0),
            ("(count m)", 3.0),
            ("(if (nil? (dissoc nil :a :b)) 1 0)", 1.0),
            ("(second [7 11 13])", 11.0),
            ("(first (nnext [7 11 13]))", 13.0),
            ("(if (nil? (second [])) 1 0)", 1.0),
            ("(if (nil? (nnext [7])) 1 0)", 1.0),
        ] {
            assert_eq!(number(&mut session, source), expected, "{source}");
        }
    }
}
#[test]
fn retained_map_mutation_helpers_preserve_transient_vector_protocol_dispatch_and_lifecycle() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def old [7 11 13]) (def t (transient old)) (def changed (assoc! t 0 17 1 19 2 23)) (def p (persistent! changed))").unwrap();
        session.collect().unwrap();
        assert_eq!(number(&mut session, "(get old 0)"), 7.0);
        assert_eq!(
            number(&mut session, "(+ (get p 0) (get p 1) (get p 2))"),
            59.0
        );
        assert!(matches!(
            session.eval("(assoc! changed 0 99)"),
            Err(SessionError::Language(_))
        ));
        assert_eq!(number(&mut session, "(get p 0)"), 17.0);
    }
}
#[test]
fn retained_map_mutation_helpers_dissoc_bang_dispatches_each_key_once_and_returns_receiver() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(deftype MutationMapProbe [^:mutable trace] ITransientMap (-dissoc! [this key] (do (set! trace (+ (* trace 10) key)) this))) (def probe (MutationMapProbe. 0))").unwrap();
        assert_eq!(
            number(
                &mut session,
                "(if (identical? (dissoc! probe 3) probe) 1 0)"
            ),
            1.0
        );
        session.collect().unwrap();
        assert_eq!(
            number(
                &mut session,
                "(if (identical? (dissoc! probe 5 7) probe) 1 0)"
            ),
            1.0
        );
        assert_eq!(number(&mut session, "(.-trace probe)"), 357.0);
    }
}
