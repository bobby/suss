//! Exact primary transformations exercised through the real public core API.
use super::*;
use crate::{portable_macro_data::FormBridge, portable_macros::CompiledMacros};
use suss_reader::forms::Kind;

#[test]
fn random_public_core_matches_controlled_primary_transformations_in_both_phases() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../../results/m12-random/controlled-transformations.json"
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
                "(try (let [x ({} {})] [:value (pr-str x) (cond (double? x) :double (integer? x) :integer :else :other)]) (catch :default e [:error]))",
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
            assert_eq!(
                suss_reader::edn::source::print_form(&observed).unwrap(),
                row["expected"].as_str().unwrap(),
                "{row}"
            );
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
