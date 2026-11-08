//! Actual retained constructor execution required by syntax quote.
use suss_cli::portable_session::Session;

fn number(session: &mut Session, source: &str) -> f64 {
    let value = session.eval(source).unwrap();
    session.collect().unwrap();
    session
        .inspect(&value, |store, value| {
            // Integral results are longs, as in Clojure; only a fractional result
            // may be a double.
            match suss_cli::portable_session::runtime_number(store, &value)? {
                Some(suss_cli::portable_session::RuntimeNumber::Long(value)) => Ok(value as f64),
                Some(suss_cli::portable_session::RuntimeNumber::Double(value))
                    if value.fract() != 0.0 =>
                {
                    Ok(value)
                }
                other => panic!("{source}: number result, got {other:?}"),
            }
        })
        .unwrap()
}

#[test]
fn retained_vec_executes_all_source_branches_in_both_phases() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        assert_eq!(number(&mut session, "(count (vec nil))"), 0.0);
        assert_eq!(number(&mut session, "(reduce + (vec (list 20 22)))"), 42.0);
        assert_eq!(
            number(
                &mut session,
                "(if (nil? (meta (vec (with-meta [42] {:marker 7})))) 1 0)"
            ),
            1.0
        );
        assert_eq!(number(&mut session, "(nth (vec (first {:k 42})) 1)"), 42.0);
        session
            .eval("(def source (array 20 21)) (def aliased (vec source)) (aset source 1 22)")
            .unwrap();
        assert_eq!(number(&mut session, "(reduce + aliased)"), 42.0);
        let entries = (0..33).map(|i| i.to_string()).collect::<Vec<_>>().join(" ");
        session
            .eval(&format!(
                "(def rebuilt (vec (concat [{entries}] (list 10))))"
            ))
            .unwrap();
        assert_eq!(number(&mut session, "(count rebuilt)"), 34.0);
        assert_eq!(number(&mut session, "(nth rebuilt 31)"), 31.0);
        assert_eq!(number(&mut session, "(nth rebuilt 32)"), 32.0);
        assert_eq!(number(&mut session, "(nth rebuilt 33)"), 10.0);
        assert_eq!(
            number(&mut session, "(alength (into-array nil (list 20 22)))"),
            2.0
        );
        assert_eq!(number(&mut session, "(aget (into-array [42]) 0)"), 42.0);
    }
}

#[test]
fn retained_map_constructors_execute_variadic_and_apply_paths() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for constructor in ["array-map", "hash-map"] {
            assert_eq!(
                number(&mut session, &format!("(count ({constructor}))")),
                0.0
            );
            session
                .eval(&format!(
                    "(def constructed ({constructor} :a 20 :a 22 :b 20 nil 42))"
                ))
                .unwrap();
            assert_eq!(number(&mut session, "(count constructed)"), 3.0);
            assert_eq!(
                number(
                    &mut session,
                    "(+ (get constructed :a) (get constructed :b))"
                ),
                42.0
            );
            assert_eq!(number(&mut session, "(get constructed nil)"), 42.0);
            assert_eq!(
                number(
                    &mut session,
                    &format!("(get (apply {constructor} (list :answer 42)) :answer)")
                ),
                42.0
            );
            let entries = (0..18)
                .map(|i| format!(":k{i} {i}"))
                .collect::<Vec<_>>()
                .join(" ");
            assert_eq!(
                number(&mut session, &format!("(count ({constructor} {entries}))")),
                18.0
            );
        }
    }
}
