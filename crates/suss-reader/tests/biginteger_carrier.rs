use suss_reader::{
    edn::{self, Options, Value},
    forms,
};

#[test]
fn typed_biginteger_carrier_keeps_decimal_value_and_category() {
    for (payload, expected) in [
        ("0", "0"),
        ("-0", "0"),
        ("+0010", "10"),
        ("010", "10"),
        ("-010", "-10"),
        ("9223372036854775808", "9223372036854775808"),
        ("-18446744073709551616", "-18446744073709551616"),
    ] {
        let source = format!("#suss/biginteger {payload:?}");
        let value = edn::read_one(&source, &Options::default()).unwrap();
        let Value::BigInteger(number) = &value else {
            panic!("{value:?}")
        };
        assert_eq!(number.to_string(), expected);
        let canonical = format!("#suss/biginteger {expected:?}");
        assert_eq!(edn::print(&value).unwrap(), canonical);
        assert_eq!(
            edn::read_one(&canonical, &Options::default()).unwrap(),
            value
        );
        let form = edn::source::value_to_form(&value);
        assert!(
            matches!(&form.kind, forms::Kind::Exact(n) if matches!(n.value, forms::ExactValue::Integer { big: false, .. }))
        );
        let emitted = edn::source::print_form(&form).unwrap();
        assert_eq!(emitted, canonical);
        let selected = forms::resolve_conditionals(forms::read_forms(&emitted).unwrap()).unwrap();
        assert_eq!(edn::source::form_to_value(&selected[0]).unwrap(), value);
        assert_eq!(
            edn::from_value::<i128>(value).unwrap().to_string(),
            expected
        );
    }
}

#[test]
fn invalid_carriers_and_numeric_duplicate_keys_are_rejected() {
    for source in [
        "#suss/biginteger 1",
        "#suss/biginteger []",
        "#suss/biginteger \"\"",
        "#suss/biginteger \"+\"",
        "#suss/biginteger \"1N\"",
        "#suss/biginteger \"1/2\"",
        "#suss/biginteger \"0x10\"",
        "#suss/biginteger \"1_0\"",
    ] {
        assert_eq!(
            edn::read_one(source, &Options::default())
                .unwrap_err()
                .message,
            "BigInteger literal expects a decimal string."
        );
        assert_eq!(
            forms::resolve_conditionals(forms::read_forms(source).unwrap())
                .unwrap_err()
                .message,
            "BigInteger literal expects a decimal string."
        );
    }
    for source in [
        "#{1 #suss/biginteger \"1\"}",
        "{1N :a #suss/biginteger \"1\" :b}",
        "#{[9223372036854775808N] [#suss/biginteger \"9223372036854775808\"]}",
    ] {
        assert!(
            edn::read_one(source, &Options::default())
                .unwrap_err()
                .message
                .contains("Duplicate")
        );
    }
    let mut options = Options::default();
    options.readers.insert(
        suss_reader::Symbol::namespaced("suss", "biginteger"),
        Box::new(|_| Ok(Value::keyword("custom"))),
    );
    assert_eq!(
        edn::read_one("#suss/biginteger \"1\"", &options).unwrap(),
        Value::keyword("custom")
    );
    assert!(edn::read_one("#suss/unknown \"1\"", &Options::default()).is_err());
}
