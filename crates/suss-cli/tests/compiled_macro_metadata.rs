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
    assert_eq!(cases.len(), 56);
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

#[test]
fn compiled_macro_metadata_evaluates_ordinary_literal_expressions_after_reader_merging() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for source in [
            "(let [answer 7] (== (get (meta ^{:answer answer} [0]) :answer) 7))",
            "(== (get (meta ^{:answer (+ 1 2)} {:x 0}) :answer) 3)",
            "(let [tag 7] (== (get (meta ^tag [0]) :tag) 7))",
            "(let [value ^{(1 2) 7} ^{[1 2] (throw 29)} []] (== (get (meta value) [1 2]) 7))",
            "(= (get (meta '^{:answer (+ 1 2)} [0]) :answer) '(+ 1 2))",
            "(= (get (meta ^{:answer (+ 1 2)} ()) :answer) '(+ 1 2))",
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
        session.eval("(def metadata-trace 0)").unwrap();
        session.eval("(def metadata-result ^{:answer (do (set! metadata-trace (+ (* metadata-trace 10) 2)) 7)} ^{:answer (throw 29) :extra (do (set! metadata-trace (+ (* metadata-trace 10) 3)) 8)} [(do (set! metadata-trace (+ (* metadata-trace 10) 1)) 42)])").unwrap();
        session.collect().unwrap();
        let value = session.eval("(and (== metadata-trace 123) (== (get (meta metadata-result) :answer) 7) (== (get (meta metadata-result) :extra) 8) (== (nth metadata-result 0) 42))").unwrap();
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
        let bridge = FormBridge::new(&mut session).unwrap();
        let input = read_forms("^{:answer (+ 1 2)} [0]").unwrap().remove(0);
        let value = bridge.quote(&mut session, input).unwrap();
        session.collect().unwrap();
        let output = bridge.read(&mut session, &value, 800..810).unwrap();
        let Kind::Map(entries) = &output.metadata[0].kind else {
            panic!("metadata map");
        };
        assert!(matches!(&entries[1].kind, Kind::List(items) if items.len() == 3));
    }
}

#[test]
fn compiled_macro_metadata_merges_data_before_constructing_discarded_values() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let bridge = FormBridge::new(&mut session).unwrap();
        session.eval("(def metadata-constructions 0) (def prior-vector-from-array (.-fromArray suss.core/PersistentVector))").unwrap();
        session.eval("(set! (.-fromArray suss.core/PersistentVector) (fn [entries no-clone] (set! metadata-constructions (inc metadata-constructions)) (prior-vector-from-array entries no-clone)))").unwrap();
        let discarded = (0..32).map(|n| n.to_string()).collect::<Vec<_>>().join(" ");
        let syntax = format!("^{{:answer 7}} ^{{:answer [{discarded}]}} x");
        let value = session
            .eval(&format!("(get (meta '{syntax}) :answer)"))
            .unwrap();
        session.collect().unwrap();
        let form = bridge.read(&mut session, &value, 810..820).unwrap();
        assert!(matches!(form.kind, Kind::Number(7.0)));
        let input = read_forms(&syntax).unwrap().remove(0);
        let value = bridge.quote(&mut session, input).unwrap();
        session.collect().unwrap();
        let output = bridge.read(&mut session, &value, 820..830).unwrap();
        assert!(matches!(output.kind, Kind::Symbol(_)));
        let value = session.eval("(== metadata-constructions 0)").unwrap();
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
        session
            .eval("(set! (.-fromArray suss.core/PersistentVector) prior-vector-from-array)")
            .unwrap();
    }
}

#[test]
fn compiled_macro_metadata_keeps_distinct_duplicate_map_syntax_keys() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for source in [
            "(== (count (meta '^{{:a 1 :a 1} 7} ^{{:a 1 :b 1} 8} x)) 2)",
            "(== (count (meta '^{{:a 1 :b 1} 7} ^{{:a 1 :a 1} 8} x)) 2)",
            "(let [value ^{{:a 1 :a 1} 7} ^{{:a 1 :b 1} 8} []] (and (== (get (meta value) {:a 1}) 7) (== (get (meta value) {:a 1 :b 1}) 8)))",
            "(== (get (meta '^{{:a 1 :a 2} 7} ^{{:a 2} 8} x) {:a 2}) 7)",
        ] {
            let value = session.eval(source).unwrap();
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
        let bridge = FormBridge::new(&mut session).unwrap();
        let input = read_forms("^{{:a 1 :a 1} 7} ^{{:a 1 :b 1} 8} x")
            .unwrap()
            .remove(0);
        let value = bridge.quote(&mut session, input).unwrap();
        session.collect().unwrap();
        let output = bridge.read(&mut session, &value, 830..840).unwrap();
        assert!(matches!(&output.metadata[0].kind, Kind::Map(entries) if entries.len() == 4));
    }
}
