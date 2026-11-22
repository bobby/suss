use suss_reader::{
    edn::{self, Options},
    forms,
};

#[test]
fn native_edn_reads_and_prints_exact_numbers_without_binary64() {
    for line in include_str!("../../../results/m12-native-edn/cases.tsv").lines() {
        let (source, expected) = line.split_once('\t').unwrap();
        let value =
            edn::read_one(source, &Options::default()).unwrap_or_else(|e| panic!("{source}: {e}"));
        let printed = edn::print(&value).unwrap();
        assert_eq!(printed, expected, "{source}");
        assert_eq!(edn::read_one(&printed, &Options::default()).unwrap(), value);
        let form = edn::source::value_to_form(&value);
        assert_eq!(edn::source::form_to_value(&form).unwrap(), value);
        let original = forms::resolve_conditionals(forms::read_forms(source).unwrap()).unwrap();
        assert_eq!(edn::source::form_to_value(&original[0]).unwrap(), value);
    }
}

#[test]
fn native_edn_detects_normalized_exact_duplicate_keys() {
    for source in [
        "{1N :a 01N :b}",
        "#{2/4 1/2}",
        "{4/2 :a 2N :b}",
        "#{[1N] [1]}",
        "{{:x 1N} :a {:x 1} :b}",
    ] {
        let error = edn::read_one(source, &Options::default()).unwrap_err();
        assert!(error.message.contains("Duplicate"), "{source}: {error}");
    }
}

#[test]
fn serde_keeps_wide_integer_bits_and_rejects_unrepresentable_ratios() {
    for value in [u64::MAX as u128, i128::MAX as u128, u128::MAX] {
        let printed = edn::to_string(&value).unwrap();
        assert_eq!(printed, format!("{value}N"));
        assert_eq!(edn::from_str::<u128>(&printed).unwrap(), value);
    }
    let value = i128::MIN;
    assert_eq!(
        edn::from_str::<i128>(&edn::to_string(&value).unwrap()).unwrap(),
        value
    );
    assert!(edn::from_str::<i64>("9223372036854775808N").is_err());
    assert!(edn::from_str::<u128>("340282366920938463463374607431768211456N").is_err());
    assert!(edn::from_str::<f64>("1/3").is_err());
}

#[test]
fn printer_rejects_exact_data_that_cannot_read_back() {
    use edn::Value;
    use std::collections::{BTreeMap, BTreeSet};
    let small = Value::Long(1);
    let big = Value::BigInt(1.into());
    // Tooling data keeps the category; reader map/set identity coalesces it.
    assert_ne!(small, big);
    for value in [
        Value::Set(BTreeSet::from([small.clone(), big.clone()])),
        Value::Map(BTreeMap::from([(small, Value::Nil), (big, Value::Nil)])),
        Value::Ratio(num_rational::BigRational::from_integer(2.into())),
    ] {
        assert!(edn::print(&value).is_err(), "{value:?}");
        assert!(edn::print_pretty(&value).is_err(), "{value:?}");
    }
}
