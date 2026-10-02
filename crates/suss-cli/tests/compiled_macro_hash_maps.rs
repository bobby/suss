use suss_cli::{
    portable_macro_data::FormBridge,
    portable_session::{Session, SessionError},
};
use suss_reader::forms::{Kind, read_forms};
use wasmtime::Val;

fn number(session: &mut Session, source: &str) -> f64 {
    let value = session
        .eval(source)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
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
                panic!("Number layout")
            };
            Ok(f64::from_bits(*bits))
        })
        .unwrap()
}
fn literal(size: usize) -> String {
    format!(
        "{{{}}}",
        (0..size)
            .map(|i| format!("{i} {}", i + 100))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

#[test]
fn compiled_macro_hash_maps_transport_large_literals_in_both_phases_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let bridge = FormBridge::new(&mut session).unwrap();
        for size in [9, 16, 17, 32, 33, 65] {
            let input = read_forms(&literal(size)).unwrap().remove(0);
            let value = bridge.quote(&mut session, input).unwrap();
            session.collect().unwrap();
            let output = bridge.read(&mut session, &value, 50..60).unwrap();
            assert_eq!(output.span, 50..60);
            let Kind::Map(entries) = output.kind else {
                panic!("Map syntax")
            };
            assert_eq!(entries.len(), size * 2);
            let mut pairs = entries
                .chunks_exact(2)
                .map(|pair| {
                    let (Kind::Number(key), Kind::Number(value)) = (&pair[0].kind, &pair[1].kind)
                    else {
                        panic!("Numeric map pair")
                    };
                    assert_eq!(*value, *key + 100.0);
                    *key as usize
                })
                .collect::<Vec<_>>();
            pairs.sort_unstable();
            assert_eq!(pairs, (0..size).collect::<Vec<_>>());
        }
    }
}

#[test]
fn compiled_macro_hash_maps_preserve_old_roots_and_nil_entries_across_growth() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval(&format!("(def original {}) (def changed (assoc original nil 700 32 999)) (def removed (dissoc changed 0 nil))", literal(33))).unwrap();
        session.collect().unwrap();
        for (source, expected) in [
            ("(count original)", 33.0),
            ("(get original 32)", 132.0),
            ("(count changed)", 34.0),
            ("(get changed 32)", 999.0),
            ("(get changed nil)", 700.0),
            ("(count removed)", 32.0),
            ("(get removed 0 701)", 701.0),
            ("(get removed nil 702)", 702.0),
            ("(get original 0)", 100.0),
        ] {
            assert_eq!(number(&mut session, source), expected, "{source}");
        }
        assert_eq!(
            number(
                &mut session,
                "(if (= original (assoc (dissoc original 0) 0 100)) 1 0)"
            ),
            1.0
        );
        assert_eq!(
            number(
                &mut session,
                "(if (== (hash original) (hash (assoc (dissoc original 0) 0 100))) 1 0)"
            ),
            1.0
        );
    }
}

#[test]
fn compiled_macro_hash_maps_resolve_collision_nodes_and_preserve_earlier_values() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(deftype CollisionKey [id] IHash (-hash [_] 7) IEquiv (-equiv [_ other] (and (instance? CollisionKey other) (== id (.-id other)))))").unwrap();
        let pairs = (0..20)
            .map(|i| format!("(CollisionKey. {i}) {}", i + 100))
            .collect::<Vec<_>>()
            .join(" ");
        session.eval(&format!("(def collision-map {{{pairs}}}) (def updated (assoc collision-map (CollisionKey. 3) 999)) (def removed (dissoc updated (CollisionKey. 4)))")).unwrap();
        session.collect().unwrap();
        assert_eq!(number(&mut session, "(count collision-map)"), 20.0);
        assert_eq!(
            number(&mut session, "(get collision-map (CollisionKey. 3))"),
            103.0
        );
        assert_eq!(
            number(&mut session, "(get updated (CollisionKey. 3))"),
            999.0
        );
        assert_eq!(
            number(&mut session, "(get removed (CollisionKey. 4) 888)"),
            888.0
        );
        assert_eq!(number(&mut session, "(count removed)"), 19.0);
        assert_eq!(
            number(&mut session, "(get removed (CollisionKey. 19))"),
            119.0
        );
    }
}

#[test]
fn compiled_macro_hash_maps_transients_cross_array_boundary_and_close_after_persistence() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval(&format!("(def old {}) (def edit (transient old)) (def grown (loop [m edit i 8] (if (< i 40) (recur (assoc! m i (+ i 100)) (+ i 1)) m))) (def finished (persistent! (dissoc! grown 3)))", literal(8))).unwrap();
        session.collect().unwrap();
        assert_eq!(number(&mut session, "(count old)"), 8.0);
        assert_eq!(number(&mut session, "(get old 3)"), 103.0);
        assert_eq!(number(&mut session, "(count finished)"), 39.0);
        assert_eq!(number(&mut session, "(get finished 39)"), 139.0);
        for source in [
            "(assoc! grown 50 150)",
            "(dissoc! grown 0)",
            "(persistent! grown)",
        ] {
            assert!(
                matches!(session.eval(source), Err(SessionError::Language(_))),
                "{source}"
            );
        }
        assert_eq!(number(&mut session, "(get finished 39)"), 139.0);
    }
}

#[test]
fn compiled_macro_hash_maps_match_fresh_pinned_observations_in_both_phases() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/hash-map-cases.json")).unwrap();
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 35);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval(include_str!("../../../tests/oracle/hash-map-fixture.sus"))
            .unwrap();
        for case in corpus["cases"].as_array().unwrap() {
            session.collect().unwrap();
            let actual = number(&mut session, case["source"].as_str().unwrap()).to_bits();
            let expected =
                u64::from_str_radix(case["expected"]["bits"].as_str().unwrap(), 16).unwrap();
            assert_eq!(actual, expected, "{}", case["id"]);
        }
    }
}

#[test]
fn compiled_macro_hash_maps_keep_canonical_roots_after_class_redefinition() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let bridge = FormBridge::new(&mut session).unwrap();
        session
            .eval(&format!(
                "(def saved (with-meta {} {{:doc 7}}))",
                literal(9)
            ))
            .unwrap();
        let saved = session.eval("saved").unwrap();
        session.enter_namespace("suss.core").unwrap();
        session
            .eval("(deftype PersistentHashMap [meta cnt root has-nil? nil-val __hash])")
            .unwrap();
        session.enter_namespace("user").unwrap();
        let foreign = session
            .eval("(suss.core/PersistentHashMap. nil 0 nil false nil nil)")
            .unwrap();
        session.collect().unwrap();
        assert!(bridge.read(&mut session, &foreign, 0..1).is_err());
        let decoded = bridge.read(&mut session, &saved, 50..60).unwrap();
        assert!(matches!(decoded.kind, Kind::Map(entries) if entries.len() == 18));
        assert_eq!(decoded.metadata.len(), 1);
    }
}
