//! Exact primary transformations exercised through the real public core API.
use super::*;
use crate::{portable_macro_data::FormBridge, portable_macros::CompiledMacros};
use suss_reader::forms::Kind;

#[test]
fn random_public_core_matches_controlled_primary_transformations_in_both_phases() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../../results/m12-random/object-transformations-boxed/cases.json"
    ))
    .unwrap();
    assert_eq!(rows.len(), 126);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let word = Arc::new(AtomicU64::new(0));
        let draws = Arc::new(AtomicUsize::new(0));
        let selected = word.clone();
        let consumed = draws.clone();
        let provider = Func::wrap(&mut session.store, move || {
            consumed.fetch_add(1, Ordering::SeqCst);
            selected.load(Ordering::SeqCst) as i64
        });
        session
            .runtime
            .get_func(&mut session.store, "random-source-install")
            .unwrap()
            .call(&mut session.store, &[Val::FuncRef(Some(provider))], &mut [])
            .unwrap();
        let mut values = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            word.store(
                match row["draw"].as_str().unwrap() {
                    "0.0" => 0,
                    "0.5" => 1 << 63,
                    "0.9999999999999999" => u64::MAX,
                    other => panic!("unassessed draw {other}"),
                },
                Ordering::SeqCst,
            );
            let expression = format!(
                "(try (let [x ({} {})] [x [:value (pr-str x) (cond (double? x) :double (integer? x) :integer :else :other)]]) (catch :default e [:error [:error]]))",
                row["operation"].as_str().unwrap(),
                row["bound"].as_str().unwrap()
            );
            values.push(
                session
                    .eval_with_macros(&expression, &mut macros)
                    .unwrap_or_else(|error| panic!("{row}: {error}")),
            );
            assert_eq!(draws.load(Ordering::SeqCst), index + 1, "{row}");
        }
        let mut bridge = FormBridge::new(&mut session).unwrap();
        session.eval("(def rand (fn [& xs] (throw :rand))) (def rand-int (fn [& xs] (throw :rand-int))) (def pr-str (fn [& xs] (throw :printer))) (def = (fn [& xs] (throw :equality)))").unwrap();
        session.collect().unwrap();
        for (value, row) in values.iter().zip(&rows) {
            let observed = bridge.read(&mut session, value, 0..0).unwrap();
            let Kind::Vector(ref envelope) = observed.kind else {
                panic!("{observed:?}")
            };
            assert_eq!(envelope.len(), 2);
            assert_eq!(
                suss_reader::edn::source::print_form(&envelope[1]).unwrap(),
                row["expected"].as_str().unwrap(),
                "{row}"
            );
            // Read the actual numeric datum independently of its guest printer
            // and type predicates. The primary's round-tripping print and JVM
            // category supply expected fields; NaN payloads are not a contract.
            use suss_reader::edn::Value;
            let expected =
                suss_reader::edn::read_one(row["expected"].as_str().unwrap(), &Default::default())
                    .unwrap();
            let Value::Vector(ref fields) = expected else {
                panic!("{expected:?}")
            };
            let Value::Keyword(ref tag) = fields[0] else {
                panic!("{fields:?}")
            };
            if tag.name == "error" {
                assert!(
                    matches!(&envelope[0].kind, Kind::Keyword(value)
                    if value.namespace.is_none() && value.name == "error"),
                    "{row}"
                );
            } else {
                let (Value::String(text), Value::Keyword(kind)) = (&fields[1], &fields[2]) else {
                    panic!("{fields:?}")
                };
                match kind.name.as_str() {
                    "integer" => assert!(
                        matches!(envelope[0].kind, Kind::Long(value)
                        if value == text.parse::<i64>().unwrap()),
                        "{row}"
                    ),
                    "double" => {
                        let expected = match text.as_str() {
                            "##NaN" => f64::NAN,
                            "##Inf" => f64::INFINITY,
                            "##-Inf" => f64::NEG_INFINITY,
                            _ => text.parse::<f64>().unwrap(),
                        };
                        let Kind::Number(actual) = envelope[0].kind else {
                            panic!("{row}: {:?}", envelope[0])
                        };
                        if expected.is_nan() {
                            assert!(actual.is_nan(), "{row}");
                        } else {
                            assert_eq!(actual.to_bits(), expected.to_bits(), "{row}");
                        }
                    }
                    other => panic!("unassessed primary numeric category {other}"),
                }
            }
        }
        assert_eq!(draws.load(Ordering::SeqCst), 126);
    }
}

#[test]
fn random_native_default_uses_fresh_execution_time_entropy() {
    let mut streams = Vec::new();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let expression = format!(
            "[{}]",
            std::iter::repeat_n("(rand)", 16)
                .collect::<Vec<_>>()
                .join(" ")
        );
        let value = session.eval_with_macros(&expression, &mut macros).unwrap();
        let mut bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let form = bridge.read(&mut session, &value, 0..0).unwrap();
        let Kind::Vector(ref values) = form.kind else {
            panic!("{form:?}")
        };
        let bits = values
            .iter()
            .map(|value| {
                let Kind::Number(number) = value.kind else {
                    panic!("{value:?}")
                };
                assert!((0.0..1.0).contains(&number));
                number.to_bits()
            })
            .collect::<Vec<_>>();
        assert!(bits.iter().any(|value| *value != bits[0]));
        streams.push(bits);
    }
    assert_ne!(
        streams[0], streams[1],
        "separate phases must not replay a fixed seed"
    );
}
