use suss_cli::{portable_macro_data::FormBridge, portable_session::Session};
use suss_reader::forms::{read_forms, Kind};

fn truth(session: &mut Session, source: &str) {
    let value = session.eval(source).unwrap_or_else(|error| panic!("{source}: {error}"));
    session.collect().unwrap();
    assert_eq!(session.inspect(&value, |store, value| Ok(value.unwrap_anyref().unwrap().as_i31(&store)?.unwrap().get_u32())).unwrap(), 4, "{source}");
}

#[test]
fn compiled_sets_keep_canonical_lookup_metadata_and_persistent_versions_in_both_phases() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def original (with-meta #{nil 1 2} {:doc 7})) (def changed (conj original 3)) (def removed (disj changed 1 nil))").unwrap();
        for source in [
            "(set? original)", "(== (count original) 3)",
            "(== (hash original) (hash #{2 nil 1}))",
            "(== (count changed) 4)", "(== (count removed) 2)",
            "(identical? original (conj original 2))",
            "(identical? original (disj original 99))",
            "(== (:doc (meta changed)) 7)", "(== (:doc (meta removed)) 7)",
            "(== (get removed 2) 2)", "(== (removed 3) 3)",
            "(== (get removed nil 99) 99)", "(contains? original nil)",
            "(= #{1 2 nil} original)", "(not (= #{1 2} original))",
            "(== (count (hash-set 1 1 2 nil nil)) 3)",
            "(== (count (set [1 2 2 3])) 3)",
        ] { truth(&mut session, source); }
    }
}

#[test]
fn compiled_set_transients_reject_all_operations_after_persistence() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def t (transient #{1 2})) (conj! t 3) (disj! t 1)").unwrap();
        truth(&mut session, "(and (== (count t) 2) (== (get t 2) 2) (== (t 2) 2) (== (t 99 7) 7))");
        session.eval("(def kept (persistent! t))").unwrap();
        truth(&mut session, "(= kept #{2 3})");
        for operation in ["(count t)", "(get t 2)", "(t 2)", "(conj! t 4)", "(disj! t 2)", "(persistent! t)"] {
            truth(&mut session, &format!("(try {operation} false (catch :default ex (suss.bootstrap/error? ex)))"));
        }
        truth(&mut session, "(= kept #{2 3})");
    }
}

#[test]
fn compiled_macro_sets_transport_canonical_small_and_hamt_values_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let bridge = FormBridge::new(&mut session).unwrap();
        for size in [0, 1, 8, 9, 17, 33, 65] {
            let syntax = format!("#{{{}}}", (0..size).map(|i| i.to_string()).collect::<Vec<_>>().join(" "));
            let input = read_forms(&syntax).unwrap().remove(0);
            let before = session.stats();
            let value = bridge.quote(&mut session, input).unwrap();
            session.collect().unwrap();
            let output = bridge.read(&mut session, &value, 10..20).unwrap();
            let Kind::Set(items) = output.kind else { panic!("canonical set syntax") };
            let mut actual = items.iter().map(|item| { let Kind::Number(n) = item.kind else { panic!("number") }; n as usize }).collect::<Vec<_>>();
            actual.sort_unstable();
            assert_eq!(actual, (0..size).collect::<Vec<_>>());
            assert_eq!(session.stats().resident_fragments, before.resident_fragments);
            assert_eq!(session.stats().resident_artifact_bytes, before.resident_artifact_bytes);
        }
    }
}

#[test]
fn compiled_set_literals_preserve_textual_effect_order_and_stop_at_throw() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        truth(&mut session, "(do (def effects 0) (def result #{(do (set! effects (+ (* effects 10) 1)) 1) (do (set! effects (+ (* effects 10) 2)) 2) (do (set! effects (+ (* effects 10) 3)) 3)}) (== effects 123))");
        truth(&mut session, "(do (set! effects 0) (try #{(do (set! effects 1) 1) (throw 7) (do (set! effects 3) 3)} (catch :default ex (== ex 7))) (== effects 1))");
    }
}

#[test]
fn compiled_macro_key_sequences_ignore_values_and_keep_their_own_metadata() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // This fixture constructs up to 33 separately compiled function values.
        // Use an explicit bounded stress allowance for the retained HAMT factory.
        session.set_operation_fuel(100_000_000);
        let bridge = FormBridge::new(&mut session).unwrap();
        for size in [1, 8, 9, 17, 33] {
            let map = format!("{{{}}}", (0..size).map(|i| format!("{i} (fn [] nil)")).collect::<Vec<_>>().join(" "));
            let value = session.eval(&format!("(with-meta (keys {map}) {{:doc 7}})")).unwrap();
            session.collect().unwrap();
            let output = bridge.read(&mut session, &value, 1..2).unwrap();
            assert_eq!(output.metadata.len(), 1);
            let Kind::List(items) = output.kind else { panic!("actual key sequence") };
            let mut keys = items.iter().map(|item| { let Kind::Number(n) = item.kind else { panic!("key") }; n as usize }).collect::<Vec<_>>();
            keys.sort_unstable(); assert_eq!(keys, (0..size).collect::<Vec<_>>());
        }
    }
}

#[test]
fn compiled_large_set_literals_use_the_canonical_hamt_backing_map() {
    fn descriptor_id(session: &mut Session, value: &suss_cli::portable_session::SessionValue, set: bool) -> i64 {
        session.inspect(value, |mut store, value| {
            let mut object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            if set {
                let storage = object.field(&mut store, 1)?.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                object = storage.get(&mut store, 1)?.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            }
            let descriptor = object.field(&mut store, 0)?.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            Ok(descriptor.field(&mut store, 0)?.unwrap_i64())
        }).unwrap()
    }
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let map = session.eval("{0 0 1 1 2 2 3 3 4 4 5 5 6 6 7 7 8 8}").unwrap();
        let expected = descriptor_id(&mut session, &map, false);
        let set = session.eval("#{0 1 2 3 4 5 6 7 8}").unwrap();
        assert_eq!(descriptor_id(&mut session, &set, true), expected);
    }
}

#[test]
fn compiled_macro_sets_retain_canonical_roots_and_reject_forged_storage_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let bridge = FormBridge::new(&mut session).unwrap();
        let saved = session.eval("(with-meta (hash-set nil 1 1 2) {:doc 7})").unwrap();
        let keys = session.eval("(keys {1 10 2 20})").unwrap();
        for source in [
            "(PersistentHashSet. nil [] nil)",
            "(KeySeq. nil (fn [] nil) nil)",
        ] {
            let malformed = session.eval(source).unwrap();
            session.collect().unwrap();
            assert!(bridge.read(&mut session, &malformed, 10..20).is_err(), "{source}");
        }
        session.enter_namespace("suss.core").unwrap();
        session.eval("(deftype PersistentHashSet [meta hash-map __hash]) (deftype KeySeq [s meta __hash])").unwrap();
        session.enter_namespace("user").unwrap();
        for source in [
            "(suss.core/PersistentHashSet. nil {} nil)",
            "(suss.core/KeySeq. nil nil nil)",
        ] {
            let forged = session.eval(source).unwrap();
            session.collect().unwrap();
            assert!(bridge.read(&mut session, &forged, 10..20).is_err(), "{source}");
        }
        session.collect().unwrap();
        let decoded = bridge.read(&mut session, &saved, 10..20).unwrap();
        assert!(matches!(&decoded.kind, Kind::Set(items) if items.len() == 3 && items.iter().any(|item| matches!(item.kind, Kind::Nil))));
        assert_eq!(decoded.metadata.len(), 1);
        assert!(matches!(bridge.read(&mut session, &keys, 10..20).unwrap().kind, Kind::List(items) if items.len() == 2));
        let before = session.stats();
        let quoted = bridge.quote(&mut session, decoded).unwrap();
        session.collect().unwrap();
        let roundtrip = bridge.read(&mut session, &quoted, 10..20).unwrap();
        assert!(matches!(roundtrip.kind, Kind::Set(items) if items.len() == 3));
        assert_eq!(roundtrip.metadata.len(), 1);
        assert_eq!(session.stats().resident_fragments, before.resident_fragments);
        assert_eq!(session.stats().resident_artifact_bytes, before.resident_artifact_bytes);
    }
}
