//! Checked character coercion follows source-selected Clojure numeric overloads.
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};

fn cases() -> Vec<serde_json::Value> {
    serde_json::from_str(include_str!("../../../results/m12-character-coercion/primary/cases.json")).unwrap()
}
fn keyword(value: &str) -> Observation {
    Observation::Keyword(None, value.encode_utf16().collect())
}
fn expected(row: &serde_json::Value) -> Observation {
    // The accepted representation identifies a one-unit String and character.
    // Retain these three actual JVM disagreements; never relabel them matches.
    if row["input"] == "\"a\"" {
        assert_eq!(row["error_class"], "java.lang.ClassCastException");
        return Observation::Vector(vec![keyword("ok"), Observation::String(vec![97])]);
    }
    if let Some(unit) = row["unit"].as_u64() {
        Observation::Vector(vec![keyword("ok"), Observation::String(vec![unit as u16])])
    } else {
        Observation::Vector(vec![keyword("throws"), Observation::String(row["message"].as_str().unwrap().encode_utf16().collect())])
    }
}

#[test]
fn character_source_overloads_match_primary_in_both_phases() {
    let cases = cases();
    assert_eq!(cases.len(), 94);
    assert_eq!(cases.iter().filter(|row| row["input"] == "\"a\"").count(), 3);
    let mut failures = Vec::new();
    for phase in ["Runtime", "Macro"] {
        let mut session = if phase == "Runtime" { Session::new_repl() } else { Session::new_macro() }.unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        for row in &cases {
            let source = format!("(try [:ok {}] (catch :default e [:throws (ex-message e)]))", row["expression"].as_str().unwrap());
            let value = session.eval_with_macros(&source, &mut macros).unwrap();
            session.collect().unwrap();
            decoder.refill(20_000);
            let actual = decoder.decode_session(&mut session, &value).unwrap();
            let expected = expected(row);
            if actual != expected {
                failures.push(format!("{phase} {} {}\nexpected {expected:?}\nactual {actual:?}", row["name"], row["expression"]));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    println!("CHAR_PRIMARY_PER_PHASE exact=91 explicit_platform_differences=3 missing=0");
}
