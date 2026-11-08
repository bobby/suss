#[path = "../../suss-compile/tests/support/probe_eval.rs"]
mod probe_eval;
use probe_eval::ProbeEval;
use suss_cli::{portable_macro_data::FormBridge, portable_session::Session};
use suss_reader::forms::{Form, Kind};

fn tagged(form: &Form) -> serde_json::Value {
    match &form.kind {
        Kind::Bool(value) => serde_json::json!({"tag":"bool", "value":value}),
        Kind::Nil => serde_json::json!({"tag":"nil"}),
        // The pinned corpus tags every number "f64" (all integral); Clojure
        // reads and returns each as a long, so only a long matches it.
        Kind::Long(n) => serde_json::json!({"tag":"f64", "bits":format!("{:016x}", (*n as f64).to_bits())}),
        Kind::Number(n) => serde_json::json!({"tag":if n.is_finite() && n.fract() == 0.0 && n.to_bits() != (-0.0f64).to_bits() { "double" } else { "f64" }, "bits":format!("{:016x}", n.to_bits())}),
        Kind::String(units) => serde_json::json!({"tag":"string", "units":units}),
        Kind::Symbol(symbol) => {
            serde_json::json!({"tag":"symbol", "namespace":symbol.namespace.as_ref().map(|name| serde_json::json!({"tag":"string", "units":name.encode_utf16().collect::<Vec<_>>()})).unwrap_or_else(|| serde_json::json!({"tag":"nil"})), "name":{"tag":"string", "units":symbol.name.encode_utf16().collect::<Vec<_>>()}})
        }
        Kind::Vector(items) => {
            serde_json::json!({"tag":"vector", "items":items.iter().map(tagged).collect::<Vec<_>>()})
        }
        Kind::List(items) => {
            serde_json::json!({"tag":"seq", "items":items.iter().map(tagged).collect::<Vec<_>>()})
        }
        other => panic!("Unexpected sequence observation: {other:?}"),
    }
}
/// JVM Clojure 1.12's observation of a hash-ordered case of `corpus`, which
/// replaces its pinned ClojureScript order (#336).
fn clojure_order(corpus: &str, id: &str) -> Option<serde_json::Value> {
    let observations: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/clojure-hash-order-observations.json"
    ))
    .unwrap();
    observations["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["corpus"] == corpus && case["id"] == id)
        .map(|case| case["expected"].clone())
}
fn literal(n: usize) -> String {
    format!(
        "{{{}}}",
        (0..n)
            .map(|i| format!("{i} {}", i + 100))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

#[test]
fn compiled_macro_hamt_sequences_match_exact_pinned_order_in_both_phases_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/hamt-sequence-cases.json"
    ))
    .unwrap();
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 27);
    let mut failures = Vec::new();
    for (phase, mut session) in [
        ("Runtime", Session::new_repl().unwrap()),
        ("Macro", Session::new_macro().unwrap()),
    ] {
        session.set_operation_fuel(100_000_000);
        let bridge = FormBridge::new(&mut session).unwrap();
        for case in corpus["cases"].as_array().unwrap() {
            let source = case["source"].as_str().unwrap();
            let value = match session.eval_probe(source) {
                Ok(value) => value,
                Err(error) => {
                    failures.push(format!("{phase} {} execution: {error:?}", case["id"]));
                    continue;
                }
            };
            session.collect().unwrap();
            match bridge.read(&mut session, &value, 70..80) {
                Ok(form) => {
                    assert_eq!(form.span, 70..80);
                    let actual = tagged(&form);
                    let expected = clojure_order("hamt-sequence-cases.json", case["id"].as_str().unwrap())
                        .unwrap_or_else(|| case["expected"].clone());
                    if actual != expected {
                        failures.push(format!(
                            "{phase} {}: actual {actual}, expected {expected}",
                            case["id"]
                        ));
                    }
                }
                Err(error) => failures.push(format!("{phase} {} transport: {error:?}", case["id"])),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn compiled_macro_hamt_sequences_retain_metadata_old_descriptors_and_reject_foreign_layouts() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let bridge = FormBridge::new(&mut session).unwrap();
        for (name, size) in [("saved_bitmap", 9), ("saved_array", 65)] {
            session
                .eval(&format!(
                    "(def {name} (with-meta (seq {}) {{:doc 7}}))",
                    literal(size)
                ))
                .unwrap();
        }
        let bitmap = session.eval("saved_bitmap").unwrap();
        let array = session.eval("saved_array").unwrap();
        session.enter_namespace("suss.core").unwrap();
        session.eval("(deftype NodeSeq [meta nodes i s __hash]) (deftype ArrayNodeSeq [meta nodes i s __hash])").unwrap();
        session.enter_namespace("user").unwrap();
        for source in [
            "(suss.core/NodeSeq. nil (array 7 8) 0 nil nil)",
            "(suss.core/ArrayNodeSeq. nil (array) 0 nil nil)",
        ] {
            let foreign = session.eval(source).unwrap();
            session.collect().unwrap();
            assert!(
                bridge.read(&mut session, &foreign, 70..80).is_err(),
                "{source}"
            );
        }
        for (value, size) in [(&bitmap, 9), (&array, 65)] {
            session.collect().unwrap();
            let decoded = bridge.read(&mut session, value, 70..80).unwrap();
            let Kind::List(items) = &decoded.kind else {
                panic!("sequence")
            };
            assert_eq!(items.len(), size);
            assert_eq!(decoded.metadata.len(), 1);
            let Kind::Map(meta) = &decoded.metadata[0].kind else {
                panic!("map metadata")
            };
            assert_eq!(meta.len(), 2);
            assert!(
                matches!(&meta[0].kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == "doc")
            );
            assert!(matches!(&meta[1].kind, Kind::Long(7)));
        }
    }
}

fn number(session: &mut Session, source: &str) -> i64 {
    let value = session.eval_privileged(source).unwrap();
    session
        .inspect(&value, |store, value| {
            let Some(suss_compile::portable_session::RuntimeNumber::Long(long)) =
                suss_compile::portable_session::runtime_number(store, &value)?
            else {
                panic!("number")
            };
            Ok(long)
        })
        .unwrap()
}
#[test]
fn compiled_macro_hamt_sequences_grow_persistent_array_maps_with_symbol_collisions() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session.eval("(def grown {})").unwrap();
        for i in 0..8 {
            session.eval_privileged(&format!("(def grown (assoc grown (cljs.core/Symbol. nil \"key{i}\" \"key{i}\" 7 nil) {}))", i + 100)).unwrap();
        }
        session.eval("(def old grown)").unwrap();
        for i in 8..20 {
            session.eval_privileged(&format!("(def grown (assoc grown (cljs.core/Symbol. nil \"key{i}\" \"key{i}\" 7 nil) {}))", i + 100)).unwrap();
        }
        session.collect().unwrap();
        for (source, expected) in [
            ("(count old)", 8),
            ("(count grown)", 20),
            (
                "(get grown (cljs.core/Symbol. nil \"key0\" \"key0\" 7 nil))",
                100,
            ),
            (
                "(get grown (cljs.core/Symbol. nil \"key19\" \"key19\" 7 nil))",
                119,
            ),
            (
                "(get old (cljs.core/Symbol. nil \"key19\" \"key19\" 7 nil) 999)",
                999,
            ),
        ] {
            assert_eq!(number(&mut session, source), expected, "{source}");
        }
    }
}

#[test]
fn compiled_macro_hamt_sequences_reject_corrupt_cursors_and_cycles_with_locations() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let bridge = FormBridge::new(&mut session).unwrap();
        for source in [
            "(NodeSeq. nil (array 7 8) 1 nil nil)",
            "(NodeSeq. nil (array 7) 0 nil nil)",
            "(NodeSeq. nil (make-array 4098) 0 nil nil)",
            "(NodeSeq. nil (array 7 8) 3 nil nil)",
            "(NodeSeq. nil (array 7 8) ##NaN nil nil)",
            "(NodeSeq. nil (array nil nil) 0 nil nil)",
            "(NodeSeq. nil (array 7 8) 0 (fn [] 1) nil)",
            "(ArrayNodeSeq. nil (array nil) 0 nil nil)",
            "(ArrayNodeSeq. nil (make-array 32) 33 nil nil)",
        ] {
            let value = session.eval_privileged(source).unwrap();
            session.collect().unwrap();
            let failure = bridge.read(&mut session, &value, 70..80).unwrap_err();
            assert!(
                matches!(failure, suss_cli::portable_session::SessionError::Compile(ref diagnostic) if diagnostic.span == (70..80)),
                "{source}: {failure:?}"
            );
        }
        let cycle = session
            .eval_privileged("(NodeSeq. nil (array 7 8) 0 nil nil)")
            .unwrap();
        session
            .inspect(&cycle, |mut store, value| {
                let fields = value
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&store)?
                    .unwrap()
                    .fields(&mut store)?
                    .collect::<Vec<_>>();
                let data = fields[1]
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                data.set(&mut store, 3, value.clone())?;
                Ok(())
            })
            .unwrap();
        session.collect().unwrap();
        let failure = bridge.read(&mut session, &cycle, 70..80).unwrap_err();
        assert!(
            matches!(failure, suss_cli::portable_session::SessionError::Compile(ref diagnostic) if diagnostic.span == (70..80) && diagnostic.message.contains("Cyclic runtime sequence")),
            "{failure:?}"
        );
        // Shared acyclic aliases remain valid within one decode, even though
        // a recursive alias of the same cursor must be rejected above.
        let shared = session.eval_privileged("(let [s (NodeSeq. nil (array 7 8) 0 nil nil)] [s s])").unwrap();
        session.collect().unwrap();
        let decoded = bridge.read(&mut session, &shared, 70..80).unwrap();
        let entry = serde_json::json!({"tag":"seq", "items":[{"tag":"vector", "items":[{"tag":"f64", "bits":"401c000000000000"}, {"tag":"f64", "bits":"4020000000000000"}]}]});
        assert_eq!(tagged(&decoded), serde_json::json!({"tag":"vector", "items":[entry.clone(), entry]}));
        let sparse = session
            .eval_privileged("(NodeSeq. nil (array 7 8 nil (aget (array) 0)) 0 nil nil)")
            .unwrap();
        session.collect().unwrap();
        let decoded = bridge.read(&mut session, &sparse, 70..80).unwrap();
        assert_eq!(
            tagged(&decoded),
            serde_json::json!({"tag":"seq", "items":[{"tag":"vector", "items":[{"tag":"f64", "bits":"401c000000000000"}, {"tag":"f64", "bits":"4020000000000000"}]}]})
        );
        let undefined = session.eval("(aget (array) 0)").unwrap();
        assert!(bridge.read(&mut session, &undefined, 70..80).is_err());
    }
}
