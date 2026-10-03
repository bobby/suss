//! Source LazySeq is required by the pinned syntax quote collection expansion.
use suss_cli::portable_session::Session;
use wasmtime::Val;

fn number(session: &mut Session, source: &str) -> f64 {
    let value = session.eval(source).unwrap();
    session.collect().unwrap();
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
                panic!("numeric result: {fields:?}")
            };
            Ok(f64::from_bits(*bits))
        })
        .unwrap()
}

#[test]
fn retained_lazy_sequence_realizes_once_and_keeps_state_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def effects 0)").unwrap();
        session.eval("(def pending (new cljs.core/LazySeq nil (fn [] (do (set! effects (+ effects 1)) (list 20 22))) nil nil))").unwrap();
        assert_eq!(number(&mut session, "effects"), 0.0);
        assert_eq!(
            number(&mut session, "(if (cljs.core/-realized? pending) 1 0)"),
            0.0
        );
        assert_eq!(
            number(&mut session, "(+ (first pending) (first (next pending)))"),
            42.0
        );
        assert_eq!(number(&mut session, "effects"), 1.0);
        assert_eq!(
            number(&mut session, "(if (cljs.core/-realized? pending) 1 0)"),
            1.0
        );
        assert_eq!(
            number(&mut session, "(+ (first pending) (first (next pending)))"),
            42.0
        );
        assert_eq!(number(&mut session, "effects"), 1.0);
    }
}

#[test]
fn retained_lazy_sequence_metadata_delays_realization_and_failed_thunks_retry() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def effects 0) (def fail true)").unwrap();
        session.eval("(def pending (new cljs.core/LazySeq nil (fn [] (do (set! effects (+ effects 1)) (if fail (throw 7) (list 42)))) nil nil))").unwrap();
        session
            .eval("(def decorated (with-meta pending {:marker 42}))")
            .unwrap();
        assert_eq!(number(&mut session, "(get (meta decorated) :marker)"), 42.0);
        assert_eq!(number(&mut session, "effects"), 0.0);
        assert_eq!(
            number(&mut session, "(try (first decorated) (catch :default e e))"),
            7.0
        );
        assert_eq!(number(&mut session, "effects"), 1.0);
        session.eval("(set! fail false)").unwrap();
        assert_eq!(number(&mut session, "(first decorated)"), 42.0);
        assert_eq!(number(&mut session, "(first pending)"), 42.0);
        assert_eq!(number(&mut session, "effects"), 2.0);
    }
}

#[test]
fn retained_lazy_sequences_match_actual_pinned_shared_execution() {
    use suss_cli::portable_macro_data::FormBridge;
    use suss_reader::forms::Kind;
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/lazy-sequence-observations.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    assert_eq!(cases[0][0], "realize");
    assert_eq!(cases[1][0], "metadata-retry");
    assert_eq!(cases[2][0], "concat");
    assert_eq!(cases[3][0], "constructors");
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // The combined shared constructor artifact performs both 18-key map
        // builds in one operation; keep one explicit bounded budget throughout.
        session.set_operation_fuel(50_000_000);
        session
            .eval(include_str!(
                "../../../tests/oracle/lazy-sequence-fixture.sus"
            ))
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        for (case, source) in cases.iter().zip([
            "(lazy-realize-observations)",
            "(lazy-meta-retry-observations)",
            "(concat-observations)",
            "(reader-constructor-observations)",
        ]) {
            let value = session.eval(source).unwrap();
            session.collect().unwrap();
            let form = bridge.read(&mut session, &value, 0..source.len()).unwrap();
            let Kind::List(items) = form.kind else {
                panic!("Expected actual sequence data")
            };
            let expected = case[1].as_array().unwrap();
            assert_eq!(items.len(), expected.len());
            for (item, expected) in items.iter().zip(expected) {
                assert_eq!(item.kind, Kind::Number(expected.as_f64().unwrap()));
            }
        }
    }
}

#[test]
fn retained_concat_preserves_laziness_all_arities_and_chunk_boundaries() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        assert_eq!(number(&mut session, "(count (concat))"), 0.0);
        assert_eq!(number(&mut session, "(count (concat nil))"), 0.0);
        assert_eq!(number(&mut session, "(first (concat (list 42)))"), 42.0);
        assert_eq!(
            number(
                &mut session,
                "(reduce + (concat nil (list 1) [] (list 20 21)))"
            ),
            42.0
        );
        session.eval("(def effects 0) (def deferred (new cljs.core/LazySeq nil (fn [] (do (set! effects (+ effects 1)) (list 22))) nil nil)) (def joined (concat (list 20) deferred))").unwrap();
        assert_eq!(number(&mut session, "effects"), 0.0);
        assert_eq!(number(&mut session, "(first joined)"), 20.0);
        assert_eq!(number(&mut session, "effects"), 0.0);
        assert_eq!(number(&mut session, "(first (next joined))"), 22.0);
        assert_eq!(number(&mut session, "effects"), 1.0);
        assert_eq!(number(&mut session, "(count joined)"), 2.0);
        assert_eq!(number(&mut session, "effects"), 1.0);
        session.eval("(def chunked (concat [0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32] (list 10)))").unwrap();
        assert_eq!(number(&mut session, "(count chunked)"), 34.0);
        assert_eq!(number(&mut session, "(nth chunked 31)"), 31.0);
        assert_eq!(number(&mut session, "(nth chunked 32)"), 32.0);
        assert_eq!(number(&mut session, "(nth chunked 33)"), 10.0);
    }
}
