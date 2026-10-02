use suss_cli::{portable_macro_data::FormBridge, portable_session::Session};
use suss_reader::forms::{Kind, read_forms};

#[test]
fn compiled_macro_metadata_transports_actual_maps_on_nested_syntax_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let bridge = FormBridge::new(&mut session).unwrap();
        let input =
            read_forms("^{:line 17 :doc \"macro\"} [^:private local ^{:tag Item} (call 7)]")
                .unwrap()
                .remove(0);
        let value = bridge.quote(&mut session, input).unwrap();
        session.collect().unwrap();
        let output = bridge.read(&mut session, &value, 300..350).unwrap();
        assert_eq!(output.span, 300..350);
        assert_eq!(output.metadata.len(), 1);
        assert!(matches!(&output.metadata[0].kind, Kind::Map(entries) if entries.len() == 4));
        let Kind::Vector(items) = output.kind else {
            panic!("Vector");
        };
        assert_eq!(items[0].metadata.len(), 1);
        assert_eq!(items[1].metadata.len(), 1);
        assert!(matches!(items[1].kind, Kind::List(_)));
    }
}

#[test]
fn compiled_macro_metadata_keeps_maps_on_returned_sequence_kinds_and_rejects_invalid_data() {
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    for source in [
        "(with-meta 'local {:tag 'Item})",
        "(with-meta '(call x) {:private true})",
        "(with-meta () {:doc \"empty\"})",
        "(with-meta [1] {:doc \"vector\"})",
        "(with-meta {:x 1} {:doc \"map\"})",
        "(with-meta (seq (array 1 2)) {:doc \"indexed\"})",
        "(with-meta (seq {:x 1}) {:doc \"entries\"})",
    ] {
        let value = session.eval(source).unwrap();
        session.collect().unwrap();
        let form = bridge.read(&mut session, &value, 500..510).unwrap();
        assert_eq!(form.metadata.len(), 1, "{source}");
        assert!(matches!(form.metadata[0].kind, Kind::Map(_)));
    }
    for source in [
        "(with-meta '(x) 1)",
        "(with-meta [1] (js-obj))",
        "(cons 1 (with-meta '(x) 1))",
    ] {
        let value = session.eval(source).unwrap();
        session.collect().unwrap();
        assert!(
            bridge.read(&mut session, &value, 510..520).is_err(),
            "{source}"
        );
    }
    session
        .eval("(def cyclic-metadata {:tag 1}) (set! (.-meta cyclic-metadata) cyclic-metadata)")
        .unwrap();
    let value = session.eval("(with-meta [1] cyclic-metadata)").unwrap();
    assert!(bridge.read(&mut session, &value, 520..530).is_err());
    let value = session.eval("[1]").unwrap();
    assert!(bridge.read(&mut session, &value, 530..540).is_ok());
}

#[test]
fn compiled_macro_metadata_desugars_and_merges_prefixes_with_outer_precedence() {
    let mut session = Session::new_macro().unwrap();
    for source in [
        "(== (get (meta '^:private x) :private) true)",
        "(= (get (meta '^Item x) :tag) 'Item)",
        "(= (get (meta '^\"Item\" x) :tag) \"Item\")",
        "(== (get (meta '^{:answer 1} ^{:answer 2 :other 3} x) :answer) 1)",
        "(== (get (meta '^{:answer 1} ^{:answer 2 :other 3} x) :other) 3)",
        "(nil? (meta '^{:line 10 :column 20 :file \"source\"} x))",
        "(== (get (meta ^{:line 10 :doc 7} [1]) :doc) 7)",
        "(nil? (get (meta ^{:line 10 :doc 7} [1]) :line))",
        "(== (get (meta ^{:doc 7} {:x 1}) :doc) 7)",
    ] {
        let value = session
            .eval(source)
            .unwrap_or_else(|failure| panic!("{source}: {failure:?}"));
        assert!(
            session
                .inspect(&value, |store, value| Ok(value
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .unwrap()
                    .get_u32()
                    == 4))
                .unwrap(),
            "{source}"
        );
    }
}

#[test]
fn compiled_macro_metadata_match_fresh_pinned_scalar_observations_in_both_phases() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/macro-metadata-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 45);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let mut macros = suss_cli::portable_macros::CompiledMacros::new().unwrap();
        macros
            .define("(defmacro form-tag [] (get (meta &form) :probe))")
            .unwrap();
        macros
            .define("(defmacro form-line [] (get (meta &form) :line))")
            .unwrap();
        for case in cases {
            let value = session
                .eval_with_macros(case["source"].as_str().unwrap(), &mut macros)
                .unwrap_or_else(|error| panic!("{}: {error:?}", case["id"]));
            session.collect().unwrap();
            let actual = session
                .inspect(&value, |mut store, value| {
                    Ok(if case["expected"]["tag"] == "bool" {
                        let n = value
                            .unwrap_anyref()
                            .unwrap()
                            .as_i31(&store)?
                            .unwrap()
                            .get_u32();
                        let v = match n {
                            2 => false,
                            4 => true,
                            _ => panic!("Boolean layout"),
                        };
                        serde_json::json!({"tag":"bool","value":v})
                    } else {
                        let fields = value
                            .unwrap_anyref()
                            .unwrap()
                            .as_struct(&store)?
                            .unwrap()
                            .fields(&mut store)?
                            .collect::<Vec<_>>();
                        let [wasmtime::Val::F64(bits)] = fields.as_slice() else {
                            panic!("Number layout");
                        };
                        serde_json::json!({"tag":"f64","bits":format!("{bits:016x}")})
                    })
                })
                .unwrap();
            assert_eq!(actual, case["expected"], "{}", case["id"]);
        }
    }
}

#[test]
fn compiled_macro_metadata_transports_real_vector_chunk_sequences_across_trie_boundaries() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let bridge = FormBridge::new(&mut session).unwrap();
        let entries = (0..65).map(|n| n.to_string()).collect::<Vec<_>>().join(" ");
        session
            .eval(&format!("(def source-vector [{entries}])"))
            .unwrap();
        let value = session
            .eval("(with-meta (rest (seq source-vector)) {:doc 7})")
            .unwrap();
        session.collect().unwrap();
        let form = bridge.read(&mut session, &value, 700..710).unwrap();
        assert_eq!(form.metadata.len(), 1);
        let Kind::List(entries) = form.kind else {
            panic!("Chunked sequence");
        };
        assert_eq!(entries.len(), 64);
        for (i, entry) in entries.iter().enumerate() {
            assert!(matches!(entry.kind, Kind::Number(n) if n == (i + 1) as f64));
        }
        for source in [
            "(suss.core/ChunkedSeq. source-vector (array) 0 0 nil nil)",
            "(suss.core/ChunkedSeq. source-vector (array 1) 65 0 nil nil)",
            "(suss.core/ChunkedSeq. source-vector (array 1) 0 2 nil nil)",
            "(suss.core/ChunkedSeq. (js-obj) (array 1) 0 0 nil nil)",
            "(suss.core/ChunkedSeq. (suss.core/PersistentVector. nil 1 5 nil (array 1) nil) (array 1) 0 0 nil nil)",
        ] {
            let value = session.eval(source).unwrap();
            assert!(
                bridge.read(&mut session, &value, 710..720).is_err(),
                "{source}"
            );
        }
    }
}

#[test]
fn compiled_macro_metadata_captures_with_meta_before_literal_entry_effects() {
    let mut session = Session::new_repl().unwrap();
    session
        .eval("(def prior-with-meta with-meta) (def effects 0)")
        .unwrap();
    session.eval("(def result ^{:doc 7} [(do (set! effects 1) (set! suss.core/with-meta (fn [value metadata] (throw 29))) 42)])").unwrap();
    session.collect().unwrap();
    let value = session
        .eval("(and (== effects 1) (== (nth result 0) 42) (== (get (meta result) :doc) 7))")
        .unwrap();
    assert!(
        session
            .inspect(&value, |store, value| Ok(value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32()
                == 4))
            .unwrap()
    );
    assert!(session.eval("^:doc [1]").is_err());
    session
        .eval("(set! suss.core/with-meta prior-with-meta)")
        .unwrap();
}

#[test]
fn compiled_macro_remainder_preserves_operand_order_arity_errors_and_recovers_after_gc() {
    let mut session = Session::new_macro().unwrap();
    session.eval("(def trace 0) (def result (js-mod (do (set! trace (+ (* trace 10) 1)) 7.5) (do (set! trace (+ (* trace 10) 2)) 2)))").unwrap();
    session.collect().unwrap();
    let value = session.eval("(and (== trace 12) (== result 1.5))").unwrap();
    assert!(
        session
            .inspect(&value, |store, value| Ok(value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32()
                == 4))
            .unwrap()
    );
    assert!(
        session
            .eval("(js-mod (do (set! trace 1) 7) (do (set! trace 2) 3) (do (set! trace 3) 1))")
            .is_err()
    );
    let value = session.eval("(== trace 3)").unwrap();
    assert!(
        session
            .inspect(&value, |store, value| Ok(value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32()
                == 4))
            .unwrap()
    );
    assert!(session.eval("(js-mod (js-obj) 3)").is_err());
    session.collect().unwrap();
    assert!(session.eval("(js-mod 7 3)").is_ok());
}
