use suss_cli::{portable_macro_data::FormBridge, portable_session::Session};
use suss_reader::forms::{Kind, read_forms};

#[test]
fn compiled_macro_maps_use_canonical_persistent_data_and_preserve_old_values() {
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let input = read_forms("{:answer 42 :binding [local (call 7)]}")
        .unwrap()
        .remove(0);
    let value = bridge.quote(&mut session, input).unwrap();
    session.collect().unwrap();
    let form = bridge.read(&mut session, &value, 301..320).unwrap();
    let Kind::Map(entries) = form.kind else {
        panic!("Real map syntax");
    };
    assert_eq!(entries.len(), 4);
    assert!(matches!(entries[0].kind, Kind::Keyword(_)));
    assert!(matches!(entries[1].kind, Kind::Number(n) if n == 42.0));
    assert!(matches!(entries[3].kind, Kind::Vector(_)));
    session
        .eval(
            "(def m {:a 1 :b 2}) (def changed (-assoc m :a 99)) (def removed (-dissoc changed :b))",
        )
        .unwrap();
    for source in [
        "(== (get m :a) 1)",
        "(== (get changed :a) 99)",
        "(== (count removed) 1)",
        "(== (get removed :b 91) 91)",
        "(= m {:b 2 :a 1})",
        "(== (hash m) (hash {:b 2 :a 1}))",
    ] {
        let value = session
            .eval(source)
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
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
fn compiled_macro_maps_transport_actual_entries_and_sequences_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let bridge = FormBridge::new(&mut session).unwrap();
        let value = session.eval("(seq {:a [7] :b '(call x)})").unwrap();
        session.collect().unwrap();
        let form = bridge.read(&mut session, &value, 70..80).unwrap();
        let Kind::List(entries) = form.kind else {
            panic!("Map sequence");
        };
        assert_eq!(entries.len(), 2);
        assert!(
            matches!(&entries[0].kind, Kind::Vector(pair) if pair.len() == 2 && matches!(pair[1].kind, Kind::Vector(_)))
        );
        let value = session.eval("(first {:a '(call x)})").unwrap();
        session.collect().unwrap();
        let form = bridge.read(&mut session, &value, 80..90).unwrap();
        assert!(
            matches!(form.kind, Kind::Vector(pair) if pair.len() == 2 && matches!(pair[1].kind, Kind::List(_)))
        );
    }
}

#[test]
fn compiled_macro_maps_reject_foreign_and_malformed_storage_and_recover() {
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    for source in [
        "(suss.core/PersistentArrayMap. nil 2 (array :a 1) nil)",
        "(suss.core/PersistentArrayMap. nil -1 (array) nil)",
        "(suss.core/PersistentArrayMap. nil 1.5 (array :a 1) nil)",
        "(suss.core/PersistentArrayMap. nil 1 (js-obj) nil)",
        "(suss.core/PersistentArrayMap. true 1 (array :a 1) nil)",
        "(suss.core/PersistentArrayMap. nil 2049 (make-array 4098) nil)",
        "(suss.core/PersistentArrayMapSeq. (array :a 1) 1 nil)",
        "(suss.core/PersistentArrayMapSeq. (array :a 1) 2 nil)",
        "(suss.core/PersistentArrayMapSeq. (array :a) 0 nil)",
        "(suss.core/PersistentArrayMapSeq. (array :a 1) -1 nil)",
        "(suss.core/PersistentArrayMapSeq. (array :a 1) 0.5 nil)",
        "(suss.core/PersistentArrayMapSeq. (array :a 1) 0 true)",
        "(suss.core/PersistentArrayMapSeq. (make-array 4098) 0 nil)",
    ] {
        let value = session.eval(source).unwrap();
        session.collect().unwrap();
        assert!(bridge.read(&mut session, &value, 0..1).is_err(), "{source}");
    }
    session.eval("(def saved-map {:a 1})").unwrap();
    session.enter_namespace("suss.core").unwrap();
    session
        .eval("(deftype PersistentArrayMap [meta cnt arr __hash])")
        .unwrap();
    session.enter_namespace("user").unwrap();
    let foreign = session
        .eval("(suss.core/PersistentArrayMap. nil 1 (array :a 1) nil)")
        .unwrap();
    assert!(bridge.read(&mut session, &foreign, 0..1).is_err());
    let old = session.eval("saved-map").unwrap();
    session.collect().unwrap();
    assert!(matches!(
        bridge.read(&mut session, &old, 0..1).unwrap().kind,
        Kind::Map(_)
    ));
}

#[test]
fn compiled_macro_maps_match_fresh_pinned_scalar_observations_in_both_phases() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/macro-map-cases.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 24);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for case in cases {
            let value = session
                .eval(case["source"].as_str().unwrap())
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
fn compiled_macro_maps_capture_factory_before_ordered_entries_and_stop_on_throw() {
    let mut session = Session::new_macro().unwrap();
    session.eval("(def trace 0) (def original-factory (.-createAsIfByAssoc suss.core/PersistentArrayMap))").unwrap();
    session.eval("(def result {(do (set! trace (+ (* trace 10) 1)) (set! (.-createAsIfByAssoc suss.core/PersistentArrayMap) (fn [_] (throw 77))) :a) (do (set! trace (+ (* trace 10) 2)) 11) (do (set! trace (+ (* trace 10) 3)) :b) (do (set! trace (+ (* trace 10) 4)) 12)})").unwrap();
    let value = session
        .eval("(and (== trace 1234) (== (get result :a) 11) (== (get result :b) 12))")
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
    assert!(session.eval("{:a 1}").is_err());
    session.eval("(set! (.-createAsIfByAssoc suss.core/PersistentArrayMap) original-factory) (set! trace 0)").unwrap();
    assert!(
        session
            .eval("{(do (set! trace 1) :a) (throw 19) (do (set! trace 3) :b) 12}")
            .is_err()
    );
    let value = session.eval("(== trace 1)").unwrap();
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
}

#[test]
fn compiled_macro_map_sequences_count_entry_vectors_in_nesting_bound() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let bridge = FormBridge::new(&mut session).unwrap();
        for (wrappers, accepted) in [(61, true), (62, false)] {
            let source = format!(
                "(loop [v (seq {{:a 1}}) i 0] (if (< i {wrappers}) (recur (cons v nil) (inc i)) v))"
            );
            let value = session.eval(&source).unwrap();
            session.collect().unwrap();
            let result = bridge.read(&mut session, &value, 0..1);
            assert_eq!(result.is_ok(), accepted, "{wrappers} wrappers");
        }
        let value = session.eval("(loop [v (seq {:a 1}) i 0] (if (< i 1500) (recur (cons (seq {:a 1}) v) (inc i)) v))").unwrap();
        session.collect().unwrap();
        let error = bridge.read(&mut session, &value, 0..1).unwrap_err();
        assert!(
            format!("{error}").contains("bounded array map sequence pair storage"),
            "{error}"
        );
        let value = session.eval("(seq {:recovered 7})").unwrap();
        assert!(bridge.read(&mut session, &value, 0..1).is_ok());
    }
}
