//! Bounded retained Subvec regressions; native execution remains a separate gate.
use suss_cli::portable_session::{Session, SessionError};
const FIXTURE: &str = include_str!("../../../tests/oracle/fixtures/subvector-probe.sus");
fn number(session: &mut Session, source: &str) -> u64 {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |mut store, value| {
            let fields = value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap()
                .fields(&mut store)?
                .collect::<Vec<_>>();
            let [wasmtime::Val::F64(bits)] = fields.as_slice() else {
                panic!("Number layout")
            };
            Ok(*bits)
        })
        .unwrap()
}
fn truth(session: &mut Session, source: &str) {
    let value = session.eval(source).unwrap();
    let raw = session
        .inspect(&value, |store, value| {
            Ok(value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32())
        })
        .unwrap();
    assert_eq!(raw, 4, "{source}");
}
fn expect_language_error(
    session: &mut Session,
    error: SessionError,
    expected_descriptor: i64,
    expected_message: &str,
    source: &str,
) {
    let SessionError::Language(payload) = error else {
        panic!("expected language payload for {source}: {error}")
    };
    session.collect().unwrap();
    let (descriptor_id, message) = session
        .inspect(&payload, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let descriptor = object
                .field(&mut store, 0)?
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap();
            let descriptor_id = descriptor.field(&mut store, 0)?.unwrap_i64();
            let text = object
                .field(&mut store, 1)?
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            let message = text
                .elems(&mut store)?
                .map(|unit| unit.unwrap_i32() as u16)
                .collect::<Vec<_>>();
            Ok((descriptor_id, message))
        })
        .unwrap();
    assert_eq!(descriptor_id, expected_descriptor, "{source}");
    assert_eq!(
        message,
        expected_message.encode_utf16().collect::<Vec<_>>(),
        "{source}"
    );
}

#[test]
fn public_subvectors_match_pinned_facts_in_both_phases() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/subvector-cases.json")).unwrap();
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 34);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session.eval(FIXTURE).unwrap();
        let bridge = suss_cli::portable_macro_data::FormBridge::new(&mut session).unwrap();
        for case in corpus["cases"].as_array().unwrap() {
            let source = case["source"].as_str().unwrap();
            let observed = if case["expected"]["tag"] == "f64" {
                serde_json::json!({"tag":"f64", "bits":format!("{:016x}", number(&mut session, source))})
            } else {
                let result = session.eval(source).unwrap();
                let form = bridge.read(&mut session, &result, 0..0).unwrap();
                let suss_reader::forms::Kind::Vector(items) = form.kind else {
                    panic!("actual projected vector")
                };
                let items: Vec<_> = items
                    .into_iter()
                    .map(|item| {
                        let suss_reader::forms::Kind::Number(n) = item.kind else {
                            panic!("actual numeric element")
                        };
                        serde_json::json!({"tag":"f64", "bits":format!("{:016x}", n.to_bits())})
                    })
                    .collect();
                serde_json::json!({"tag":"vector", "items":items})
            };
            assert_eq!(observed, case["expected"], "{}", case["id"]);
            session.collect().unwrap();
        }
    }
}
#[test]
fn subvector_bounds_are_typed_errors_and_recover_in_both_phases() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/subvector-errors.json")).unwrap();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session.eval(FIXTURE).unwrap();
        for case in cases.as_array().unwrap() {
            let source = case["source"].as_str().unwrap();
            let error = session.eval(source).unwrap_err();
            expect_language_error(
                &mut session,
                error,
                7,
                case["message"].as_str().unwrap(),
                source,
            );
            truth(&mut session, "(= (nth sv-nested 1) 32)");
        }
        session.eval("(set! sv-effects 0)").unwrap();
        let error = session
            .eval("(subvec (sv-mark 1 sv-base) (sv-mark 2 33) (sv-mark 3 31))")
            .unwrap_err();
        expect_language_error(
            &mut session,
            error,
            7,
            "Index out of bounds",
            "invalid ordered bounds",
        );
        assert_eq!(number(&mut session, "sv-effects"), 123f64.to_bits());
    }
}
#[test]
fn subvector_retains_actual_backing_and_rooted_contents_across_gc() {
    use suss_cli::portable_macro_data::FormBridge;
    use suss_reader::forms::Kind;
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session.eval(FIXTURE).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let original = session.eval("sv-view").unwrap();
        let project = session.eval("(fn [v] (into [] v))").unwrap();
        truth(
            &mut session,
            "(and (identical? (.-v sv-view) sv-base) (identical? (.-v sv-nested) sv-base) (= (.-start sv-nested) 31) (= (.-end sv-nested) 34) (identical? (.-v (pop sv-view)) sv-base) (identical? (.-v (with-meta sv-view {:x 1})) sv-base))",
        );
        session
            .eval("(def sv-updated (assoc sv-view 1 99)) (def sv-extended (conj sv-view 99))")
            .unwrap();
        session.collect().unwrap();
        truth(
            &mut session,
            "(and (not (identical? (.-v sv-updated) sv-base)) (not (identical? (.-v sv-extended) sv-base)) (= (nth sv-base 32) 32) (= (nth sv-base 64) 64) (identical? (suss.core/pv-aget (.-root (.-v sv-updated)) 0) (suss.core/pv-aget (.-root sv-base) 0)))",
        );
        session.eval("(set! sv-view nil) (set! sv-nested nil) (set! sv-parent nil) (set! sv-base nil) (set! sv-updated nil) (set! sv-extended nil)").unwrap();
        session.collect().unwrap();
        let projected = session.invoke(&project, &[&original]).unwrap();
        let form = bridge.read(&mut session, &projected, 0..0).unwrap();
        let Kind::Vector(items) = form.kind else {
            panic!("canonical projected vector")
        };
        assert_eq!(items.len(), 33);
        for (i, item) in items.iter().enumerate() {
            assert!(
                matches!(item.kind, Kind::Number(n) if n.to_bits() == ((i+31) as f64).to_bits())
            );
        }
    }
}
