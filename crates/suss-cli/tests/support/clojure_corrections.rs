//! Clojure governs (#286): a recorded primary observation that coerced a
//! non-number operand is corrected to check that the form throws Clojure's
//! rejection instead.
use serde_json::{Value, json};

/// A correction: case id, the form that must throw (empty for the whole case
/// source), and the operand-rejection message it throws.
pub type Correction = (&'static str, &'static str, &'static str);

/// `case`, with its source and expectation replaced when `corrections` names
/// it. Every correction must be used, so the table cannot go stale.
pub fn corrected(case: &Value, corrections: &[Correction], used: &mut usize) -> Value {
    let id = case["id"].as_str().unwrap();
    let Some((_, form, message)) = corrections.iter().find(|(name, _, _)| *name == id) else {
        return case.clone();
    };
    *used += 1;
    let form = if form.is_empty() { case["source"].as_str().unwrap() } else { form };
    json!({
        "id": id,
        "source": format!(
            "(try (do {form} false) (catch :default e (= (ex-message e) {message:?})))"
        ),
        "expected": {"tag": "bool", "value": true},
    })
}
