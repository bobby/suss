//! Exact Decimal tooling fields and spelling against fresh pinned Clojure.
use suss_reader::edn::{self, Options, Value};
use suss_reader::forms::{ExactValue, Kind};
use std::collections::BTreeSet;

fn read(source: &str) -> Value {
    edn::read_one(source, &Options::default()).unwrap_or_else(|error| panic!("{source}: {error}"))
}

#[test]
fn decimal_fields_printing_and_admission_match_pinned_clojure() {
    let rows: Vec<_> = include_str!("../../../results/m12-decimal-tooling/primary/cases.tsv").lines().collect();
    assert_eq!(rows.len(), 126);
    for line in rows {
        let columns: Vec<_> = line.splitn(4, '\t').collect();
        let [name, kind, input, expected] = columns.as_slice() else { panic!("primary columns") };
        if *name == "distinct-integral-category" {
            let Value::Map(map) = read(input) else { panic!("map") };
            let decimal = Value::Decimal { unscaled: 1.into(), scale: 0 };
            let actual = Value::Vector(vec![Value::Long(map.len() as i64), map[&decimal].clone(), map[&Value::Long(1)].clone()]);
            assert_eq!(actual, read(expected), "{name}");
        } else if *kind == "fields" {
            let value = read(input);
            let Value::Decimal { unscaled, scale } = &value else { panic!("{name}: actual Decimal storage") };
            let printed = edn::print(&value).unwrap();
            let actual = Value::Vector(vec![
                Value::String(printed.clone()),
                Value::String(printed.strip_suffix('M').unwrap().to_owned()),
                Value::String(unscaled.to_string()),
                Value::Long(i64::from(*scale)),
            ]);
            assert_eq!(actual, read(expected), "{name}: {input}");
            assert_eq!(read(&printed), value, "{name}: printed round trip");
            let form = edn::source::value_to_form(&value);
            let Kind::Exact(number) = &form.kind else { panic!("{name}: exact form") };
            let ExactValue::Decimal { unscaled: found, scale: found_scale } = &number.value else { panic!("{name}: exact Decimal") };
            assert_eq!((found, found_scale), (unscaled, scale), "{name}");
            assert_eq!(edn::source::form_to_value(&form).unwrap(), value, "{name}");
            assert_eq!(edn::source::print_form(&form).unwrap(), printed, "{name}: emitted source");
        } else {
            let found = edn::read_one(input, &Options::default());
            match *expected {
                ":accepted" => assert!(found.is_ok(), "{name}: {found:?}"),
                ":rejected" => {
                    let error = found.expect_err(name);
                    assert!(!error.message.contains("Unsupported numeric literal"), "{name}: {error}");
                    if *kind == "duplicate" { assert!(error.message.contains("Duplicate"), "{name}: {error}"); }
                    assert!(error.span.is_some(), "{name}: located reader failure");
                }
                _ => panic!("{name}: unreviewed expectation {expected}"),
            }
        }
    }
}

#[test]
fn decimal_duplicate_identity_handles_zero_and_the_scale_floor_without_losing_storage() {
    for values in [
        [(0, i32::MIN), (0, i32::MAX)],
        [(1, i32::MIN), (10, i32::MIN + 1)],
        [(10, i32::MIN), (100, i32::MIN + 1)],
    ] {
        let set = Value::Set(values.into_iter().map(|(unscaled, scale)| Value::Decimal { unscaled: unscaled.into(), scale }).collect::<BTreeSet<_>>());
        assert!(edn::print(&set).unwrap_err().message.contains("Duplicate"));
    }
    let a = read("1.0M");
    let b = read("1.00M");
    // Tooling keeps exact storage/scale; reader key identity is language equality.
    assert_ne!(a, b);
    assert_eq!(edn::print(&a).unwrap(), "1.0M");
    assert_eq!(edn::print(&b).unwrap(), "1.00M");
}

#[test]
fn decimal_serde_refuses_a_lossy_primitive_projection() {
    let value = read("123456789012345678901234567890.00100M");
    let error = edn::from_value::<f64>(value).unwrap_err();
    assert!(error.message.contains("no exact decimal representation"));
}
