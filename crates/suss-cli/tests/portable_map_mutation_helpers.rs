use suss_cli::portable_session::{Session, SessionError};
use wasmtime::Val;
fn number(session: &mut Session, source: &str) -> f64 {
    let value = session.eval(source).unwrap_or_else(|error| {
        panic!(
            "{source}: {}",
            suss_cli::portable_repl::error_display(session, &error)
        )
    });
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

#[test]
fn retained_hash_map_quotient_dependencies_preserve_numeric_boundaries_and_recovery() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for (source, expected) in [
            ("(suss.bootstrap/f64-ceil -1.75)", -1.0_f64),
            ("(suss.bootstrap/f64-ceil 1.75)", 2.0),
            ("(suss.bootstrap/f64-ceil -0.75)", -0.0),
            ("(suss.bootstrap/f64-ceil -0.0)", -0.0),
            ("(suss.bootstrap/f64-ceil 0.0)", 0.0),
            ("(suss.bootstrap/f64-ceil ##Inf)", f64::INFINITY),
            ("(suss.bootstrap/f64-ceil ##-Inf)", f64::NEG_INFINITY),
            ("(fix -1.75)", -1.0),
            ("(fix 1.75)", 1.0),
            ("(quot -7 3)", -2.0),
            ("(quot 7 -3)", -2.0),
            ("(quot -7 -3)", 2.0),
        ] {
            assert_eq!(
                number(&mut session, source).to_bits(),
                expected.to_bits(),
                "{source}"
            );
        }
        assert!(number(&mut session, "(suss.bootstrap/f64-ceil ##NaN)").is_nan());
        for source in [
            "(suss.bootstrap/f64-ceil (array 1))",
            "(suss.bootstrap/f64-ceil (fn [] 1))",
        ] {
            assert!(
                matches!(session.eval(source), Err(SessionError::Language(_))),
                "{source}"
            );
            assert_eq!(number(&mut session, "(quot 7 3)"), 2.0);
        }
        session.eval("(def ceil-hook-count 0)").unwrap();
        assert!(matches!(
            session.eval("(suss.bootstrap/f64-ceil (js-obj \"valueOf\" (fn [] (set! ceil-hook-count (+ ceil-hook-count 1)) 1.75) \"toString\" (fn [] (set! ceil-hook-count (+ ceil-hook-count 10)) \"1.75\")))"),
            Err(SessionError::Language(_))
        ));
        session.collect().unwrap();
        assert_eq!(number(&mut session, "ceil-hook-count"), 0.0);
        assert_eq!(number(&mut session, "(quot 7 3)"), 2.0);
        for source in [
            "(suss.bootstrap/f64-ceil)",
            "(suss.bootstrap/f64-ceil 1 2)",
            "suss.bootstrap/f64-ceil",
        ] {
            assert!(
                matches!(session.eval(source), Err(SessionError::Compile(_))),
                "{source}"
            );
        }
    }
}

#[test]
fn retained_transient_array_map_removal_uses_owned_pop_storage() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def old {:a 7 :b 11 :c 13}) (def edit (transient old)) (def changed (dissoc! edit :b)) (def result (persistent! changed))").unwrap();
        session.collect().unwrap();
        assert_eq!(number(&mut session, "(count result)"), 2.0);
        assert_eq!(number(&mut session, "(get result :b 99)"), 99.0);
        assert_eq!(number(&mut session, "(get result :c)"), 13.0);
        assert_eq!(number(&mut session, "(get old :b)"), 11.0);
    }
}

#[test]
fn source_array_pop_preserves_aliases_empty_undefined_and_physical_call_receiver() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval("(def a (array 7 11)) (def alias a) (def pop (.-pop a)) (def other (array 13))")
            .unwrap();
        assert_eq!(
            number(&mut session, "(.pop a (do (aset other 0 17) 999))"),
            11.0
        );
        session.collect().unwrap();
        assert_eq!(number(&mut session, "(alength alias)"), 1.0);
        assert_eq!(number(&mut session, "(aget other 0)"), 17.0);
        assert_eq!(number(&mut session, "(.call pop other)"), 17.0);
        assert_eq!(number(&mut session, "(alength other)"), 0.0);
        assert_eq!(number(&mut session, "(.apply pop alias (array))"), 7.0);
        assert_eq!(number(&mut session, "(if (undefined? (.pop a)) 1 0)"), 1.0);
        assert_eq!(number(&mut session, "(alength alias)"), 0.0);
        for source in ["(pop)", "(.call pop nil)", "(.call pop {:length 0})"] {
            assert!(
                matches!(session.eval(source), Err(SessionError::Language(_))),
                "{source}"
            );
            assert_eq!(number(&mut session, "(alength a)"), 0.0);
        }
    }
}
