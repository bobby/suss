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

#[test]
fn regex_token_reading_and_printing_match_the_primary() {
    use suss_reader::{
        edn::source::print_form,
        forms::{read_forms, resolve_conditionals},
    };
    let data = edn::read_one(
        include_str!("../../../results/m12-pattern-values/reader-primary/cases.edn"),
        &Default::default(),
    )
    .unwrap();
    let Value::Vector(rows) = data else {
        panic!("primary rows")
    };
    assert_eq!(rows.len(), 20);
    for (index, row) in rows.iter().enumerate() {
        let Value::Map(row) = row else {
            panic!("primary row")
        };
        let token = String::from_utf16(&units(&row[&Value::keyword("units")])).unwrap();
        match (
            &row[&Value::keyword("valid")],
            read_forms(&token).and_then(resolve_conditionals),
        ) {
            (Value::Bool(true), Ok(forms)) => {
                assert_eq!(forms.len(), 1, "row {index}");
                let printed = print_form(&forms[0]).unwrap();
                if let Some(expected) = row.get(&Value::keyword("printed-units")) {
                    assert_eq!(
                        printed.encode_utf16().collect::<Vec<_>>(),
                        units(expected),
                        "row {index}"
                    );
                } else {
                    assert_eq!(
                        Value::String(printed),
                        row[&Value::keyword("printed")],
                        "row {index}"
                    );
                }
            }
            (Value::Bool(false), Err(_)) => {}
            (expected, actual) => {
                panic!("row {index}: {token:?}, expected {expected:?}, actual {actual:?}")
            }
        }
    }
}

#[test]
fn pattern_form_projection_preserves_checked_source_and_literal_identity() {
    use suss_reader::{
        edn::source::{form_to_value, print_form, value_to_form},
        forms::{Kind, read_forms, resolve_conditionals},
    };
    let read = |source: &str| resolve_conditionals(read_forms(source).unwrap()).unwrap();
    let forms = read(r##"[#"(?i)a\d" #"(?i)a\d"]"##);
    let Kind::Vector(items) = &forms[0].kind else {
        panic!("vector")
    };
    let Kind::Pattern(first) = &items[0].kind else {
        panic!("Pattern")
    };
    assert_eq!(first.source().flags(), 2);
    assert_eq!(items[0].kind, items[0].clone().kind);
    assert_ne!(items[0].kind, items[1].kind);
    let projected = form_to_value(&forms[0]).unwrap();
    let reconstructed = value_to_form(&projected);
    assert_eq!(form_to_value(&reconstructed).unwrap(), projected);
    assert_eq!(
        form_to_value(&read(&print_form(&reconstructed).unwrap())[0]).unwrap(),
        projected
    );
    for units in [vec![97, 34, 98], vec![0xd800], vec![92, 81, 97, 92]] {
        let literal = suss_reader::pattern::PatternLiteral::compile(&units).unwrap();
        let form = suss_reader::forms::Form {
            span: 0..0,
            metadata: vec![],
            kind: Kind::Pattern(literal),
        };
        let source = print_form(&form).unwrap();
        let forms = read(&source);
        let Kind::Pattern(restored) = &forms[0].kind else {
            panic!("Pattern from {source}")
        };
        assert_eq!(restored.source().units(), units);
        assert_eq!(
            form_to_value(&value_to_form(&form_to_value(&form).unwrap())).unwrap(),
            form_to_value(&form).unwrap()
        );
    }
    for source in [
        r##"#suss/pattern "[""##,
        "#suss/pattern []",
        "^:bad #suss/pattern \"a\"",
    ] {
        assert!(
            read_forms(source).and_then(resolve_conditionals).is_err(),
            "{source}"
        );
    }
    // EDN has no regex reader; a tooling projection is an explicit tag, never
    // an implicit regex value smuggled into the EDN contract.
    assert!(edn::read_one(r##"#"a""##, &Default::default()).is_err());
}

#[test]
fn pattern_projection_retains_distinct_collection_keys_and_shared_sites() {
    use suss_reader::{
        edn::source::{form_to_value, span_tree, value_to_form},
        forms::{Form, Kind, read_forms, resolve_conditionals},
    };
    let controls = edn::read_all(
        include_str!("../../../results/m12-pattern-values/collection-primary/stdout.log"),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(controls.len(), 6);
    let read = |source: &str| {
        resolve_conditionals(read_forms(source).unwrap())
            .unwrap()
            .remove(0)
    };
    for (index, source) in [(0, r##"#{#"a" #"a"}"##), (1, r##"{#"a" 1 #"a" 2}"##)] {
        let form = read(source);
        let projected = form_to_value(&form).unwrap();
        let count = match &projected {
            Value::Set(items) => items.len(),
            Value::Map(items) => items.len(),
            _ => panic!("collection"),
        };
        assert_eq!(Value::Long(count as i64), controls[index]);
        assert_eq!(
            form_to_value(&value_to_form(&projected)).unwrap(),
            projected
        );
        let tree = span_tree(&form, None).unwrap();
        let Value::Map(tree) = tree else {
            panic!("span map")
        };
        let key = if matches!(projected, Value::Set(_)) {
            "items"
        } else {
            "entries"
        };
        let Value::Vector(children) = &tree[&Value::keyword(key)] else {
            panic!("span children")
        };
        assert_eq!(children.len(), count);
    }
    let first = read(r##"#"a""##);
    let second = read(r##"#"a""##);
    let vector = Form {
        span: 0..0,
        metadata: vec![],
        kind: Kind::Vector(vec![first.clone(), first, second]),
    };
    let projected = form_to_value(&vector).unwrap();
    let restored = value_to_form(&projected);
    let Kind::Vector(items) = &restored.kind else {
        panic!("vector")
    };
    assert_eq!(items[0].kind, items[1].kind);
    assert_ne!(items[0].kind, items[2].kind);
    assert_eq!(form_to_value(&restored).unwrap(), projected);
}
