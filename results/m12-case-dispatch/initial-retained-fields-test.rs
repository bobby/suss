
#[test]
fn case_hash_preserves_retained_fields_after_gc_and_public_rebinding() {
    let decimal = || Observation::Decimal { unscaled: "100".into(), scale: 2 };
    let key = || Observation::Keyword(None, "a".encode_utf16().collect());
    let cases = vec![
        ("1.00M", decimal()),
        ("-1", Observation::Long(-1)),
        ("(biginteger -1)", Observation::BigInt("-1".into())),
        ("2/3", Observation::Ratio("2".into(), "3".into())),
        ("-0.0", Observation::Number((-0.0_f64).to_bits())),
        ("[1.00M]", Observation::Vector(vec![decimal()])),
        ("{:a 1.00M}", Observation::Map(vec![(key(), decimal())])),
        ("#{1.00M}", Observation::Set(vec![decimal()])),
        ("'app/name", Observation::Symbol(Some("app".encode_utf16().collect()), "name".encode_utf16().collect())),
        (":Aa", Observation::Keyword(None, "Aa".encode_utf16().collect())),
        ("\"Aa\"", Observation::String("Aa".encode_utf16().collect())),
    ];
    let primary = [
        include_str!("../../../results/m12-case-dispatch/numeric-hash-cases.json"),
        include_str!("../../../results/m12-case-dispatch/composite-hash-cases.json"),
    ].iter().flat_map(|s| serde_json::from_str::<Vec<(String, String)>>(s).unwrap())
        .collect::<std::collections::BTreeMap<_, _>>();
    for phase in ["Runtime", "Macro"] {
        let mut session = if phase == "Runtime" { Session::new_repl().unwrap() } else { Session::new_macro().unwrap() };
        let mut decoder = Decoder::capture(&mut session, 10_000).unwrap();
        let functions = cases.iter().map(|(source, _)| session.eval_probe(&format!(
            "(let [x {source}] (fn [] [(suss.bootstrap/case-hash-code x) x (suss.bootstrap/case-equiv x {source})]))"
        )).unwrap()).collect::<Vec<_>>();
        session.collect().unwrap();
        session.eval_probe("(ns suss.core) (def pr-str (fn [& xs] (throw (ex-info \"guest printer called\" {})))) (def = (fn [& xs] (throw (ex-info \"public equality called\" {})))) (def hash (fn [& xs] (throw (ex-info \"public hash called\" {})))) (def symbol? (fn [x] false)) (def keyword? (fn [x] false)) (def number? (fn [x] false)) (def sequential? (fn [x] false)) (def map? (fn [x] false)) (def set? (fn [x] false))").unwrap();
        for ((source, expected), function) in cases.iter().zip(functions) {
            let hash = primary.get(&format!("(suss.bootstrap/case-hash-code {source})"))
                .or_else(|| primary.get(&format!("(suss.bootstrap/case-number-hash {source})")))
                .unwrap().parse::<i64>().unwrap();
            let value = session.invoke(&function, &[]).unwrap();
            session.collect().unwrap();
            decoder.refill(10_000);
            assert_eq!(decoder.decode_session(&mut session, &value).unwrap(),
                Observation::Vector(vec![Observation::Long(hash), expected.clone(), Observation::Bool(true)]),
                "{phase}: {source}");
        }
    }
}
