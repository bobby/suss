use suss_reader::forms::{Kind, read_forms, resolve_conditionals};

#[test]
fn apostrophes_continue_tokens_but_quote_at_form_start() {
    let source = "form' ns/form' :name' 'form'";
    let forms = read_forms(source).unwrap();
    assert_eq!(forms.len(), 4);
    assert!(matches!(&forms[0].kind, Kind::Symbol(symbol) if symbol.name == "form'"));
    assert!(
        matches!(&forms[1].kind, Kind::Symbol(symbol) if symbol.namespace.as_deref() == Some("ns") && symbol.name == "form'")
    );
    assert!(matches!(&forms[2].kind, Kind::Keyword(keyword) if keyword.name == "name'"));
    let Kind::List(quoted) = &forms[3].kind else {
        panic!("expected quote")
    };
    assert!(matches!(&quoted[1].kind, Kind::Symbol(symbol) if symbol.name == "form'"));
    assert_eq!(&source[forms[0].span.clone()], "form'");
    assert_eq!(read_forms("\\'").unwrap()[0].kind, Kind::String(vec![39]));
    assert!(read_forms("\\'x").is_err());
    assert_eq!(
        read_forms(r#""\1'""#).unwrap()[0].kind,
        Kind::String(vec![1, 39])
    );
}

#[test]
fn utf16_literals_preserve_lone_surrogates_and_astral_units() {
    let source = r#""\uD800\u0000😀""#;
    let forms = read_forms(source).unwrap();
    assert_eq!(forms[0].span, 0..source.len());
    assert_eq!(forms[0].kind, Kind::String(vec![0xd800, 0, 0xd83d, 0xde00]));
}

#[test]
fn ordinary_numbers_round_to_binary64_and_keep_float_zero_sign() {
    let forms = read_forms(
        "9007199254740993 -0.0 -0e0 -0 +0 1e3 1. 0x10 -0x10 010 2r101 ##Inf ##-Inf ##NaN",
    )
    .unwrap();
    let expected = [
        9007199254740992.0,
        -0.0,
        -0.0,
        0.0,
        0.0,
        1000.0,
        1.0,
        16.0,
        -16.0,
        8.0,
        5.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for (form, expected) in forms.iter().zip(expected) {
        let Kind::Number(value) = form.kind else {
            panic!("not a number")
        };
        assert_eq!(value.to_bits(), expected.to_bits());
    }
    assert!(matches!(forms[13].kind, Kind::Number(value) if value.is_nan()));
    for source in [
        "1/3",
        "100N",
        "1.0M",
        "08",
        "2r102",
        "99r1",
        "1foo",
        "1e+",
        "0x",
        "##Infinity",
    ] {
        let error = read_forms(source).unwrap_err();
        assert_eq!(error.span, 0..source.len(), "{source}");
    }
}

#[test]
fn byte_spans_exclude_padding_and_locate_nested_forms() {
    let source = " ; header\n [😀 \"é\" (x 2)] , true";
    let forms = read_forms(source).unwrap();
    let Kind::Vector(items) = &forms[0].kind else {
        panic!()
    };
    assert_eq!(&source[forms[0].span.clone()], "[😀 \"é\" (x 2)]");
    for (form, text) in items.iter().zip(["😀", "\"é\"", "(x 2)"]) {
        assert_eq!(&source[form.span.clone()], text);
    }
    let Kind::List(nested) = &items[2].kind else {
        panic!()
    };
    assert_eq!(&source[nested[1].span.clone()], "2");
    assert_eq!(&source[forms[1].span.clone()], "true");
}

#[test]
fn metadata_is_retained_on_target_with_order_and_original_spans() {
    let source = "(defn ^:export ^{:private true} greet [^string x] x)";
    let forms = read_forms(source).unwrap();
    let Kind::List(items) = &forms[0].kind else {
        panic!()
    };
    assert_eq!(items.len(), 4);
    assert_eq!(items[1].metadata.len(), 2);
    assert_eq!(&source[items[1].metadata[0].span.clone()], ":export");
    assert_eq!(
        &source[items[1].metadata[1].span.clone()],
        "{:private true}"
    );
    assert_eq!(
        &source[items[1].span.clone()],
        "^:export ^{:private true} greet"
    );
    let Kind::Vector(params) = &items[2].kind else {
        panic!()
    };
    assert_eq!(&source[params[0].metadata[0].span.clone()], "string");
    for source in ["^1 x", "^:a 1", "^:a \"x\"", "^:a"] {
        assert!(read_forms(source).is_err(), "{source}");
    }
}

#[test]
fn conditional_resolution_uses_first_portable_feature_in_source_order() {
    let forms = read_forms("[#?(:cljs 1 :suss 2) #?(:suss 3 :cljs 4) #?(:jvm 5) #?(:jvm 6 :default 7) #?(:default 8 :suss 9)]").unwrap();
    let Kind::Vector(raw) = &forms[0].kind else {
        panic!()
    };
    assert!(matches!(raw[0].kind, Kind::Conditional(_)));
    let resolved = resolve_conditionals(forms).unwrap();
    let Kind::Vector(items) = &resolved[0].kind else {
        panic!()
    };
    assert_eq!(
        items.iter().map(|x| x.kind.clone()).collect::<Vec<_>>(),
        vec![
            Kind::Number(1.0),
            Kind::Number(3.0),
            Kind::Number(7.0),
            Kind::Number(8.0)
        ]
    );
    assert!(
        resolve_conditionals(read_forms("#?(:jvm 1)").unwrap())
            .unwrap()
            .is_empty()
    );
    let repaired = resolve_conditionals(read_forms("{:a #?(:jvm 1) 2}").unwrap()).unwrap();
    let Kind::Map(entries) = &repaired[0].kind else {
        panic!()
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1].kind, Kind::Number(2.0));
    let shifted =
        resolve_conditionals(read_forms("{#?(:jvm :a) 1 :b #?(:jvm 2)}").unwrap()).unwrap();
    let Kind::Map(entries) = &shifted[0].kind else {
        panic!()
    };
    assert_eq!(entries[0].kind, Kind::Number(1.0));
    assert!(matches!(&entries[1].kind, Kind::Keyword(key) if key.name == "b"));
    let incomplete = read_forms("{#?(:jvm :key) 1}").unwrap();
    assert!(resolve_conditionals(incomplete).is_err());
    for source in ["#?(1 2)", "#?(:suss)", "#?[:suss 1]", "#?@(:suss [1])"] {
        assert!(read_forms(source).is_err(), "{source}");
    }
}

#[test]
fn quote_discard_collections_and_terminating_character_literals() {
    let forms = read_forms(
        "#_ discarded 'x #'a/f `(~x ~@xs) @cell #{1 2} {:a 1 :b 2} \\) \\space \\u0041 \\o377",
    )
    .unwrap();
    assert_eq!(forms.len(), 10);
    for (form, name) in forms.iter().zip(["quote", "var", "syntax-quote", "deref"]) {
        let Kind::List(items) = &form.kind else {
            panic!()
        };
        assert!(matches!(&items[0].kind, Kind::Symbol(symbol) if symbol.name == name));
    }
    assert_eq!(forms[6].kind, Kind::String(vec![41]));
    assert_eq!(forms[7].kind, Kind::String(vec![32]));
    assert_eq!(forms[8].kind, Kind::String(vec![65]));
    assert_eq!(forms[9].kind, Kind::String(vec![255]));
    assert!(read_forms(" ; only a comment\n #_ 1").unwrap().is_empty());
    for source in ["\\😀", "\\uD800", "\\uZZZZ", "\\o400", "\\long", "#_"] {
        assert!(read_forms(source).is_err(), "{source}");
    }
}

#[test]
fn malformed_forms_fail_with_located_diagnostics_and_bounded_nesting() {
    for source in [
        "(",
        "[1)",
        "{:a}",
        "\"\\u12\"",
        "\"\\uZZZZ\"",
        "\"\\8\"",
        "\"\\18\"",
        "\"\\1z\"",
        "\"\\400\"",
        "#unknown 1",
        "::alias/x",
        "a/",
        "a/1",
        ")",
    ] {
        let error = read_forms(source).unwrap_err();
        assert!(!error.message.is_empty());
        assert!(error.span.end <= source.len());
        assert!(
            source.is_char_boundary(error.span.start) && source.is_char_boundary(error.span.end)
        );
    }
    let nested = format!("{}0{}", "[".repeat(65), "]".repeat(65));
    assert!(
        read_forms(&nested)
            .unwrap_err()
            .message
            .contains("nesting limit")
    );
    assert_eq!(
        read_forms(r#""\123z\7 \b\f""#).unwrap()[0].kind,
        Kind::String(vec![83, 122, 7, 32, 8, 12])
    );
}
