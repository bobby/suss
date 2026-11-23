#[test]
fn sorted_collection_decoder_preserves_exact_fields_after_gc_without_guest_comparison() {
    let dec = |unscaled: &str, scale| Observation::Decimal {
        unscaled: unscaled.into(),
        scale,
    };
    let mut cases = vec![
        ("(sorted-map)".to_owned(), Observation::Map(vec![])),
        (
            "(sorted-map 2 2.00M 1 1.0M)".to_owned(),
            Observation::Map(vec![
                (Observation::Long(1), dec("10", 1)),
                (Observation::Long(2), dec("200", 2)),
            ]),
        ),
        (
            "(sorted-map-by > 2 2.00M 1 1.0M)".to_owned(),
            Observation::Map(vec![
                (Observation::Long(2), dec("200", 2)),
                (Observation::Long(1), dec("10", 1)),
            ]),
        ),
        ("(sorted-set)".to_owned(), Observation::Set(vec![])),
        (
            "(sorted-set 2M 1.00M)".to_owned(),
            Observation::Set(vec![dec("100", 2), dec("2", 0)]),
        ),
    ];
    cases.push((
        "(into (sorted-map) (map (fn [x] [x (bigdec x)]) (range 48)))".into(),
        Observation::Map(
            (0..48)
                .map(|x| (Observation::Long(x), dec(&x.to_string(), 0)))
                .collect(),
        ),
    ));
    let mut failures = Vec::new();
    for phase in ["Runtime", "Macro"] {
        let mut session = if phase == "Runtime" {
            Session::new_repl().unwrap()
        } else {
            Session::new_macro().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 10_000).unwrap();
        let values = cases
            .iter()
            .map(|(source, _)| session.eval_probe(source).unwrap())
            .collect::<Vec<_>>();
        session.collect().unwrap();
        // Decoding reads only retained canonical storage. Neither printer nor
        // guest equality can turn malformed/unknown storage into a host match.
        session.eval_probe("(ns suss.core) (def pr-str (fn [& xs] (throw (ex-info \"guest printer called\" {})))) (def = (fn [& xs] (throw (ex-info \"guest comparison called\" {}))))").unwrap();
        for ((source, expected), value) in cases.iter().zip(&values) {
            decoder.refill(10_000);
            let observed = decoder.decode_session(&mut session, value);
            match observed {
                Ok(actual) if actual == *expected => (),
                actual => failures.push(format!(
                    "{phase} {source}: {actual:?}; expected {expected:?}"
                )),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn sorted_collection_decoder_rejects_count_layout_and_budget_errors() {
    let mut session = Session::new_repl().unwrap();
    let mut decoder = Decoder::capture(&mut session, 10_000).unwrap();
    for (source, expected) in [
        (
            "(new suss.core/PersistentTreeMap compare nil 1 nil nil)",
            "count disagrees",
        ),
        (
            "(new suss.core/PersistentTreeMap compare (new suss.core/BlackNode 1 2 nil nil nil) 0 nil nil)",
            "count disagrees",
        ),
        (
            "(new suss.core/PersistentTreeMap compare [1] 1 nil nil)",
            "canonical tree node",
        ),
        (
            "(new suss.core/PersistentTreeSet nil (hash-map) nil)",
            "canonical backing tree map",
        ),
        (
            "(deftype DecoderTreeMapLookalike [comp tree cnt meta cached]) (new DecoderTreeMapLookalike compare nil 0 nil nil)",
            "Unsupported ABI2 value layout",
        ),
    ] {
        let value = session.eval_probe(source).unwrap();
        session.collect().unwrap();
        decoder.refill(10_000);
        let error = decoder.decode_session(&mut session, &value).unwrap_err();
        assert!(error.to_string().contains(expected), "{source}: {error}");
    }
    let value = session.eval_probe("(sorted-map 1 1M 2 2M)").unwrap();
    decoder.refill(2);
    let error = decoder.decode_session(&mut session, &value).unwrap_err();
    assert!(error.to_string().contains("traversal limit"), "{error}");
}
