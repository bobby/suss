//! Full retained empty prerequisite; focused native execution is a separate gate.
use suss_cli::portable_session::{Session, SessionError, SessionValue};
const FIXTURE: &str = r#"(def empty-trace 0)
(def saved-empty cljs.core/empty)
(deftype EmptyDirectProbe [m] cljs.core/IEmptyableCollection
  (-empty [this] (do (set! empty-trace (+ (* empty-trace 10) 2)) (with-meta [] m))))
(deftype EmptyThrowProbe [payload] cljs.core/IEmptyableCollection
  (-empty [this] (do (set! empty-trace (+ (* empty-trace 10) 3)) (throw payload))))
"#;
// Original shared reference cases, also extracted unchanged for the pinned oracle.
const CASES_JSON: &str = r#"[
  {
    "id": "nil",
    "source": "(empty nil)",
    "expected": {
      "tag": "nil"
    }
  },
  {
    "id": "false",
    "source": "(empty false)",
    "expected": {
      "tag": "nil"
    }
  },
  {
    "id": "number",
    "source": "(empty 17)",
    "expected": {
      "tag": "nil"
    }
  },
  {
    "id": "string",
    "source": "(empty \"abc\")",
    "expected": {
      "tag": "nil"
    }
  },
  {
    "id": "array",
    "source": "(empty (array 1 2))",
    "expected": {
      "tag": "nil"
    }
  },
  {
    "id": "object",
    "source": "(empty (js-obj))",
    "expected": {
      "tag": "nil"
    }
  },
  {
    "id": "vector-category",
    "source": "(and (instance? PersistentVector (empty [1 2])) (= (count (empty [1 2])) 0))",
    "expected": {
      "tag": "bool",
      "value": true
    }
  },
  {
    "id": "vector-meta",
    "source": "(= (:marker (meta (empty (with-meta [1 2] {:marker 17})))) 17)",
    "expected": {
      "tag": "bool",
      "value": true
    }
  },
  {
    "id": "list-category",
    "source": "(and (instance? EmptyList (empty (list 1 2))) (= (count (empty (list 1 2))) 0))",
    "expected": {
      "tag": "bool",
      "value": true
    }
  },
  {
    "id": "list-meta",
    "source": "(= (:marker (meta (empty (with-meta (list 1 2) {:marker 19})))) 19)",
    "expected": {
      "tag": "bool",
      "value": true
    }
  },
  {
    "id": "map-meta",
    "source": "(let [v (empty (with-meta {:a 1} {:marker 23}))] (and (= (count v) 0) (= (:marker (meta v)) 23) (= v {})))",
    "expected": {
      "tag": "bool",
      "value": true
    }
  },
  {
    "id": "set-meta",
    "source": "(let [v (empty (with-meta #{1 2} {:marker 29}))] (and (= (count v) 0) (= (:marker (meta v)) 29) (= v #{})))",
    "expected": {
      "tag": "bool",
      "value": true
    }
  },
  {
    "id": "subvec-meta",
    "source": "(let [v (empty (with-meta (subvec [0 1 2 3] 1 3) {:marker 31}))] (and (instance? PersistentVector v) (= (count v) 0) (= (:marker (meta v)) 31)))",
    "expected": {
      "tag": "bool",
      "value": true
    }
  },
  {
    "id": "persistent-old",
    "source": "(let [v [1 2] m {:a 1} s #{1 2}] (do (empty v) (empty m) (empty s) (and (= v [1 2]) (= m {:a 1}) (= s #{1 2}))))",
    "expected": {
      "tag": "bool",
      "value": true
    }
  },
  {
    "id": "empty-equality-hash",
    "source": "(and (= (empty [1]) []) (= (hash (empty [1])) (hash [])) (= (empty {:a 1}) {}) (= (hash (empty {:a 1})) (hash {})) (= (hash (empty #{1})) (hash #{})))",
    "expected": {
      "tag": "bool",
      "value": true
    }
  },
  {
    "id": "source-once",
    "source": "(do (set! empty-trace 0) (empty (do (set! empty-trace (inc empty-trace)) [1])) (= empty-trace 1))",
    "expected": {
      "tag": "bool",
      "value": true
    }
  }
]"#;
fn observe(session: &mut Session, value: &SessionValue) -> serde_json::Value {
    session.collect().unwrap();
    session
        .inspect(value, |store, value| {
            let raw = value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .expect("nil/Boolean ABI")
                .get_u32();
            Ok(match raw {
                0 => serde_json::json!({"tag":"nil"}),
                2 => serde_json::json!({"tag":"bool","value":false}),
                4 => serde_json::json!({"tag":"bool","value":true}),
                x => panic!("Unexpected sentinel {x}"),
            })
        })
        .unwrap()
}
fn truth(session: &mut Session, source: &str) {
    let value = session.eval(source).unwrap();
    assert_eq!(
        observe(session, &value),
        serde_json::json!({"tag":"bool","value":true}),
        "{source}"
    );
}
fn error(session: &mut Session, source: &str, descriptor_id: i64, message: &str) {
    let SessionError::Language(payload) = session.eval(source).unwrap_err() else {
        panic!("expected language Error")
    };
    session.collect().unwrap();
    let observed = session
        .inspect(&payload, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let descriptor = object
                .field(&mut store, 0)?
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap();
            let id = descriptor.field(&mut store, 0)?.unwrap_i64();
            let text = object
                .field(&mut store, 1)?
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            let units = text
                .elems(&mut store)?
                .map(|x| x.unwrap_i32() as u16)
                .collect::<Vec<_>>();
            Ok((id, units))
        })
        .unwrap();
    assert_eq!(
        observed,
        (descriptor_id, message.encode_utf16().collect()),
        "{source}"
    );
}
#[test]
fn retained_empty_matches_pinned_reference_cases_in_both_phases() {
    let cases: serde_json::Value = serde_json::from_str(CASES_JSON).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 16);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session.eval(FIXTURE).unwrap();
        for case in cases.as_array().unwrap() {
            let value = session.eval(case["source"].as_str().unwrap()).unwrap();
            assert_eq!(
                observe(&mut session, &value),
                case["expected"],
                "{}",
                case["id"]
            );
        }
    }
}
#[test]
fn empty_retains_direct_and_native_fallback_and_live_dispatch() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session.eval(FIXTURE).unwrap();
        truth(
            &mut session,
            "(do (set! empty-trace 0) (def empty-direct (EmptyDirectProbe. {:marker 37})) (let [v (empty (do (set! empty-trace 1) empty-direct))] (and (= empty-trace 12) (= (count v) 0) (= (:marker (meta v)) 37))))",
        );
        session.eval("(extend-type number cljs.core/IEmptyableCollection (-empty [n] (do (set! empty-trace (+ (* empty-trace 10) 4)) (with-meta [] {:marker n}))))").unwrap();
        truth(
            &mut session,
            "(and (not (implements? cljs.core/IEmptyableCollection 17)) (satisfies? cljs.core/IEmptyableCollection 17))",
        );
        truth(
            &mut session,
            "(do (set! empty-trace 0) (let [v (empty (do (set! empty-trace 1) 17))] (and (= empty-trace 14) (= (count v) 0) (= (:marker (meta v)) 17))))",
        );
        truth(
            &mut session,
            "(with-redefs [cljs.core/-empty (fn [v] (do (set! empty-trace 56) []))] (do (set! empty-trace 0) (empty empty-direct) (= empty-trace 56)))",
        );
        truth(
            &mut session,
            "(with-redefs [cljs.core/-empty (fn [v] (do (set! empty-trace 78) []))] (do (set! empty-trace 0) (empty 17) (= empty-trace 78)))",
        );
        truth(
            &mut session,
            "(with-redefs [cljs.core/nil? (fn [v] false) cljs.core/-empty (fn [v] (throw 99))] (identical? (empty nil) nil))",
        );
        truth(
            &mut session,
            "(with-redefs [cljs.core/nil? (fn [v] true)] (= (count (empty [1])) 0))",
        );
        // A captured function retains its complete algorithm, while public empty stays live.
        truth(
            &mut session,
            "(with-redefs [cljs.core/empty (fn [v] false)] (and (identical? (empty []) false) (= (count (saved-empty [1])) 0)))",
        );
    }
}
#[test]
fn empty_preserves_throw_payloads_arity_and_recovery_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session.eval(FIXTURE).unwrap();
        for source in ["(empty)", "(empty [] [])"] {
            error(&mut session, source, 1, "Wrong arity");
            truth(&mut session, "(= (count (empty [1])) 0)");
        }
        for (source, expected) in [
            (
                "(empty (EmptyThrowProbe. false))",
                serde_json::json!({"tag":"bool","value":false}),
            ),
            (
                "(empty (EmptyThrowProbe. nil))",
                serde_json::json!({"tag":"nil"}),
            ),
        ] {
            session.eval("(set! empty-trace 0)").unwrap();
            let SessionError::Language(payload) = session.eval(source).unwrap_err() else {
                panic!("language payload")
            };
            assert_eq!(observe(&mut session, &payload), expected);
            truth(&mut session, "(= empty-trace 3)");
            truth(&mut session, "(= (count (empty [1])) 0)");
        }
        session.eval("(set! empty-trace 0)").unwrap();
        error(
            &mut session,
            "(empty (EmptyThrowProbe. (suss.bootstrap/error \"empty method\")))",
            7,
            "empty method",
        );
        truth(&mut session, "(= empty-trace 3)");
        truth(&mut session, "(= (count (empty [1])) 0)");
        session
            .eval("(def empty-reference (js-obj)) (set! empty-trace 0)")
            .unwrap();
        let reference = session.eval("empty-reference").unwrap();
        let root = session
            .inspect(&reference, |mut store, value| {
                value.unwrap_anyref().unwrap().to_owned_rooted(&mut store)
            })
            .unwrap();
        let SessionError::Language(method_payload) = session
            .eval("(empty (EmptyThrowProbe. empty-reference))")
            .unwrap_err()
        else {
            panic!("method reference payload")
        };
        session.collect().unwrap();
        assert!(
            session
                .inspect(&method_payload, |store, value| wasmtime::Rooted::ref_eq(
                    &store,
                    &root,
                    value.unwrap_anyref().unwrap()
                ))
                .unwrap()
        );
        truth(&mut session, "(= empty-trace 3)");
        session.eval("(set! empty-trace 0)").unwrap();
        let SessionError::Language(payload) = session
            .eval("(empty (do (set! empty-trace 1) (throw empty-reference)))")
            .unwrap_err()
        else {
            panic!("reference payload")
        };
        session.eval("(set! empty-reference nil)").unwrap();
        session.collect().unwrap();
        assert!(
            session
                .inspect(&payload, |store, value| wasmtime::Rooted::ref_eq(
                    &store,
                    &root,
                    value.unwrap_anyref().unwrap()
                ))
                .unwrap()
        );
        truth(&mut session, "(= empty-trace 1)");
        truth(&mut session, "(= (count (empty [1])) 0)");
    }
}
