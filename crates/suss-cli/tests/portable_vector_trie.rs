use suss_cli::portable_session::Session;
fn boolean(session: &mut Session, source: &str) -> bool {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |mut store, value| {
            match suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap()
            {
                2 => Ok(false),
                4 => Ok(true),
                other => panic!("Boolean layout {other}"),
            }
        })
        .unwrap()
}
/// The observation of a runtime number, distinguishing a long from a double.
fn number_json(number: suss_cli::portable_session::RuntimeNumber) -> serde_json::Value {
    match number {
        suss_cli::portable_session::RuntimeNumber::Long(value) => serde_json::json!({"tag":"long", "value":value}),
        suss_cli::portable_session::RuntimeNumber::Double(value) => {
            serde_json::json!({"tag":"f64", "bits":format!("{:016x}", value.to_bits())})
        }
    }
}
/// The ClojureScript oracle encodes every number as a double; JVM Clojure,
/// Suss's oracle, gives a long for these integral results.
fn clojure_number(expected: &serde_json::Value) -> serde_json::Value {
    if expected["tag"] == "f64" {
        let bits = u64::from_str_radix(expected["bits"].as_str().unwrap(), 16).unwrap();
        let value = f64::from_bits(bits);
        if value.is_finite() && value.fract() == 0.0 && bits != (-0.0f64).to_bits() {
            return serde_json::json!({"tag":"long", "value":value as i64});
        }
    }
    expected.clone()
}

#[test]
fn vector_node_clone_owns_outer_array_and_preserves_edit_and_child() {
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def trie-edit (js-obj)) (def trie-child (js-obj)) (def trie-node (suss.core/pv-fresh-node trie-edit)) (suss.core/pv-aset trie-node 0 trie-child) (def trie-copy (suss.core/pv-clone-node trie-node))").unwrap();
    session.collect().unwrap();
    assert!(boolean(&mut session,"(and (identical? (.-edit trie-copy) trie-edit) (identical? (suss.core/pv-aget trie-copy 0) trie-child) (not (identical? (.-arr trie-node) (.-arr trie-copy))))"));
    session.eval("(suss.core/pv-aset trie-copy 0 17)").unwrap();
    session.collect().unwrap();
    assert!(boolean(&mut session,"(and (= (suss.core/pv-aget trie-copy 0) 17) (identical? (suss.core/pv-aget trie-node 0) trie-child))"));
}

#[test]
fn vector_trie_matches_independently_decoded_primary_values() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/vector-trie-cases.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 29);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(deftype VectorFixture [cnt])").unwrap();
    for case in cases {
        let value = session
            .eval(case["source"].as_str().unwrap())
            .unwrap_or_else(|error| panic!("{}: {error:?}", case["id"]));
        let observed = session
            .inspect(&value, |mut store, value| {
                Ok(match case["expected"]["tag"].as_str().unwrap() {
                    "f64" => {
                        number_json(
                            suss_cli::portable_session::runtime_number(&mut store, &value)?
                                .expect("Number layout"),
                        )
                    }
                    "bool" => {
                        let boolean = match suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap()
                        {
                            2 => false,
                            4 => true,
                            other => panic!("Boolean {other}"),
                        };
                        serde_json::json!({"tag":"bool","value":boolean})
                    }
                    tag => panic!("Unexpected corpus tag {tag}"),
                })
            })
            .unwrap();
        assert_eq!(observed, clojure_number(&case["expected"]), "{}", case["id"]);
        session.collect().unwrap();
    }
}

#[test]
fn association_copies_only_selected_path_across_fragments_and_gc() {
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def trie-left (suss.core/pv-fresh-node nil)) (def trie-leaf (suss.core/pv-fresh-node nil)) (suss.core/pv-aset trie-leaf 31 17) (def trie-root (suss.core/pv-fresh-node nil)) (suss.core/pv-aset trie-root 0 trie-left) (suss.core/pv-aset trie-root 1 trie-leaf) (def trie-associated (suss.core/do-assoc nil 5 trie-root 63 19))").unwrap();
    session.collect().unwrap();
    assert!(boolean(&mut session, "(and (identical? (suss.core/pv-aget trie-associated 0) trie-left) (not (identical? (suss.core/pv-aget trie-associated 1) trie-leaf)) (= (suss.core/pv-aget (suss.core/pv-aget trie-associated 1) 31) 19) (= (suss.core/pv-aget trie-leaf 31) 17))"));
    session
        .eval("(suss.core/pv-aset (suss.core/pv-aget trie-associated 1) 31 23)")
        .unwrap();
    session.collect().unwrap();
    assert!(boolean(&mut session, "(and (= (suss.core/pv-aget trie-leaf 31) 17) (= (suss.core/pv-aget (suss.core/pv-aget trie-associated 1) 31) 23))"));
}

#[test]
fn throwing_live_clone_prevents_source_parent_mutation_and_recovers() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def trie-guarded (suss.core/pv-fresh-node nil)) (suss.core/pv-aset trie-guarded 31 17) (def trie-effects 0)").unwrap();
    let Err(SessionError::Language(thrown)) = session.eval("(with-redefs [suss.core/pv-clone-node (fn [_] (do (set! trie-effects (inc trie-effects)) (throw 19)))] (suss.core/do-assoc nil 0 trie-guarded 31 23))") else { panic!("expected numeric language exception") };
    session
        .inspect(&thrown, |mut store, value| {
            assert_eq!(
                suss_cli::portable_session::runtime_number(&mut store, &value)?,
                Some(suss_cli::portable_session::RuntimeNumber::Long(19))
            );
            Ok(())
        })
        .unwrap();
    session.collect().unwrap();
    assert!(boolean(&mut session,"(and (= trie-effects 1) (= (suss.core/pv-aget trie-guarded 31) 17) (= (suss.core/pv-aget (suss.core/do-assoc nil 0 trie-guarded 31 23) 31) 23) (= (suss.core/pv-aget trie-guarded 31) 17))"));
}

#[test]
fn throwing_outer_pop_clone_preserves_original_recursive_path_after_gc() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(deftype VectorFixture [cnt]) (def pop-child (suss.core/pv-fresh-node 1)) (def pop-root (suss.core/pv-fresh-node 2)) (suss.core/pv-aset pop-child 1 17) (suss.core/pv-aset pop-root 0 pop-child) (def pop-trace 0) (def pop-clone suss.core/pv-clone-node)").unwrap();
    let Err(SessionError::Language(thrown)) = session.eval("(with-redefs [suss.core/pv-clone-node (fn [n] (do (set! pop-trace (+ (* pop-trace 10) (.-edit n))) (if (= (.-edit n) 2) (throw 23) (pop-clone n))))] (suss.core/pop-tail (VectorFixture. 65) 10 pop-root))") else {
        panic!("expected numeric language exception")
    };
    session
        .inspect(&thrown, |mut store, value| {
            assert_eq!(
                suss_cli::portable_session::runtime_number(&mut store, &value)?,
                Some(suss_cli::portable_session::RuntimeNumber::Long(23))
            );
            Ok(())
        })
        .unwrap();
    session.collect().unwrap();
    assert!(boolean(&mut session,"(and (= pop-trace 12) (identical? (suss.core/pv-aget pop-root 0) pop-child) (= (suss.core/pv-aget pop-child 1) 17))"));
    session
        .eval("(def pop-result (suss.core/pop-tail (VectorFixture. 65) 10 pop-root))")
        .unwrap();
    session.collect().unwrap();
    assert!(boolean(&mut session,"(and (not (identical? pop-result pop-root)) (not (identical? (suss.core/pv-aget pop-result 0) pop-child)) (nil? (suss.core/pv-aget (suss.core/pv-aget pop-result 0) 1)) (= (suss.core/pv-aget pop-child 1) 17))"));
}
