//! Admission, flags and exact UTF-16 source against pinned JVM observations.
use suss_reader::{
    edn::{self, Value},
    pattern::PatternSource,
};

fn units(value: &Value) -> Vec<u16> {
    let Value::Vector(values) = value else {
        panic!("unit vector")
    };
    values
        .iter()
        .map(|value| {
            let Value::Long(unit) = value else {
                panic!("exact UTF-16 integer")
            };
            u16::try_from(*unit).unwrap()
        })
        .collect()
}
fn check(source: &str, expected_count: usize) {
    let data = edn::read_one(source, &Default::default()).unwrap();
    let Value::Vector(rows) = data else {
        panic!("primary rows")
    };
    assert_eq!(rows.len(), expected_count);
    let mut discrepancies = vec![];
    for (index, row) in rows.iter().enumerate() {
        let Value::Map(row) = row else {
            panic!("primary row")
        };
        let source = units(&row[&Value::keyword("units")]);
        let actual = PatternSource::compile(&source);
        match (&row[&Value::keyword("valid")], actual) {
            (Value::Bool(true), Ok(pattern)) => {
                assert_eq!(
                    pattern.units(),
                    units(&row[&Value::keyword("pattern-units")]),
                    "row {index}"
                );
                if Value::Long(pattern.flags().into()) != row[&Value::keyword("flags")] {
                    discrepancies.push(format!(
                        "row {index}: {source:?}, flags {}, expected {:?}",
                        pattern.flags(),
                        row[&Value::keyword("flags")]
                    ));
                }
            }
            (Value::Bool(false), Err(error)) => {
                assert!(error.index <= source.len(), "row {index}: {error}")
            }
            (expected, actual) => discrepancies.push(format!(
                "row {index}: {source:?}, expected {expected:?}, actual {actual:?}"
            )),
        }
    }
    assert!(discrepancies.is_empty(), "{}", discrepancies.join("\n"));
}

#[test]
fn java_pattern_admission_flags_and_source_match_the_primary() {
    check(
        include_str!("../../../results/m12-pattern-values/admission-primary/pattern-cases.edn"),
        76,
    );
}
#[test]
fn adversarial_pattern_grammar_and_unicode_match_the_primary() {
    check(
        include_str!("../../../results/m12-pattern-values/adversarial-primary/pattern-cases.edn"),
        116,
    );
}
