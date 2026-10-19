use suss_reader::forms::{Kind, read_forms, resolve_conditionals};

#[test]
fn apostrophes_continue_tokens_but_quote_at_form_start() {
    let source = "form' ns/form' :name' 'form'";
    let forms = resolve_conditionals(read_forms(source).unwrap()).unwrap();
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
fn metadata_targets_are_checked_after_conditional_selection() {
    let source = "^:export #?(:suss ^:private f :cljs 1)";
    let forms = resolve_conditionals(read_forms(source).unwrap()).unwrap();
    assert!(matches!(&forms[0].kind, Kind::Symbol(symbol) if symbol.name == "f"));
    assert_eq!(forms[0].metadata.len(), 2);
    assert_eq!(&source[forms[0].metadata[0].span.clone()], ":export");
    assert_eq!(&source[forms[0].metadata[1].span.clone()], ":private");
    assert_eq!(forms[0].span.start, 0);
    let forms = resolve_conditionals(read_forms("[^:a #?(:jvm f) ^:b x]").unwrap()).unwrap();
    let Kind::Vector(items) = &forms[0].kind else {
        panic!()
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].metadata.len(), 2);
    assert!(matches!(&items[0].kind, Kind::Symbol(symbol) if symbol.name == "x"));
    let forms =
        resolve_conditionals(read_forms("^:a #_ #?(:jvm ignored) discarded retained").unwrap())
            .unwrap();
    assert_eq!(forms.len(), 1);
    assert_eq!(forms[0].metadata.len(), 1);
    assert!(matches!(&forms[0].kind, Kind::Symbol(symbol) if symbol.name == "retained"));
    for source in [
        "^:a #?(:suss 1)",
        "^:a #?(:jvm x) 1",
        "^:a #?(:jvm x)",
        "[^:a #?(:jvm x)]",
        "^:a #?(:suss #?(:jvm ignored)) retained",
    ] {
        let error = resolve_conditionals(read_forms(source).unwrap()).unwrap_err();
        assert!(error.span.start <= error.span.end && error.span.end <= source.len());
        assert!(
            error.message.contains("Metadata")
                || error.message.contains("prefix")
                || error.message.contains("conditional"),
            "{}",
            error.message
        );
    }
}

#[test]
fn reader_prefixes_continue_after_unmatched_conditionals_with_bounded_depth() {
    for (prefix, operator) in [
        ("'", "quote"),
        ("@", "deref"),
        ("#'", "var"),
        ("`", "syntax-quote"),
        ("~", "unquote"),
        ("~@", "unquote-splicing"),
    ] {
        let source = format!("{prefix}#?(:jvm ignored) retained");
        let raw = read_forms(&source).unwrap();
        let resolved = resolve_conditionals(raw).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].span, 0..source.len());
        let Kind::List(items) = &resolved[0].kind else {
            panic!()
        };
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0].kind, Kind::Symbol(symbol) if symbol.name == operator));
        assert_eq!(&source[items[0].span.clone()], prefix);
        assert!(matches!(&items[1].kind, Kind::Symbol(symbol) if symbol.name == "retained"));
        assert!(
            resolve_conditionals(read_forms(&format!("{prefix}#?(:jvm ignored)")).unwrap())
                .is_err()
        );
    }
    let source = "{:key '^:a #?(:jvm ignored) ^:b retained}";
    let resolved = resolve_conditionals(read_forms(source).unwrap()).unwrap();
    let Kind::Map(entries) = &resolved[0].kind else {
        panic!()
    };
    assert_eq!(entries.len(), 2);
    let Kind::List(quoted) = &entries[1].kind else {
        panic!()
    };
    assert_eq!(quoted[1].metadata.len(), 2);
    assert!(resolve_conditionals(read_forms("['#?(:jvm ignored)] retained").unwrap()).is_err());
    for (source, retained) in [
        ("[#_ #?(:jvm ignored) discarded next]", "next"),
        ("[#_ #_ #?(:jvm ignored) first second next]", "next"),
        ("[#_ '#?(:jvm ignored) discarded next]", "next"),
    ] {
        let forms = resolve_conditionals(read_forms(source).unwrap()).unwrap();
        let Kind::Vector(items) = &forms[0].kind else {
            panic!()
        };
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0].kind, Kind::Symbol(symbol) if symbol.name == retained));
    }
    assert!(resolve_conditionals(read_forms("[#_ #?(:jvm ignored)] next").unwrap()).is_err());
    assert!(
        resolve_conditionals(read_forms("#?(:suss '#?(:jvm ignored)) retained").unwrap()).is_err()
    );
    let flat = format!("{}retained", "'#?(:jvm ignored) ".repeat(65));
    assert_eq!(
        resolve_conditionals(read_forms(&flat).unwrap())
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn conditional_clause_pairing_follows_deferred_prefix_selection() {
    for source in [
        "#?(:suss #_ 1 2 :cljs 3)",
        "#?(:suss #_ #?(:jvm ignored) discarded 2 :cljs 3)",
        "#?(:jvm #?(:jvm 1) :suss 2)",
        "#?(:jvm '#?(:jvm 1) :suss 2)",
    ] {
        let forms = resolve_conditionals(read_forms(source).unwrap()).unwrap();
        assert_eq!(forms[0].kind, Kind::Number(2.0));
    }
    let forms = resolve_conditionals(
        read_forms("#?(:suss '#?(:jvm ignored) :cljs alternative) retained").unwrap(),
    )
    .unwrap();
    let Kind::List(items) = &forms[0].kind else {
        panic!()
    };
    assert!(matches!(&items[1].kind, Kind::Keyword(key) if key.name == "cljs"));
    assert!(matches!(&forms[1].kind, Kind::Symbol(symbol) if symbol.name == "retained"));
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
    let forms = resolve_conditionals(forms).unwrap();
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
fn javascript_object_literals_lower_to_ordered_js_obj_calls() {
    let forms = read_forms("#js {:get 1 \"configurable\" true answer 42}").unwrap();
    let Kind::List(items) = &forms[0].kind else {
        panic!()
    };
    assert!(matches!(&items[0].kind, Kind::Symbol(symbol) if symbol.name == "js-obj"));
    assert_eq!(items[1].kind, Kind::String("get".encode_utf16().collect()));
    assert_eq!(items[2].kind, Kind::Number(1.0));
    assert_eq!(
        items[3].kind,
        Kind::String("configurable".encode_utf16().collect())
    );
    assert_eq!(items[4].kind, Kind::Bool(true));
    assert_eq!(
        items[5].kind,
        Kind::String("answer".encode_utf16().collect())
    );
    assert_eq!(items[6].kind, Kind::Number(42.0));
    assert!(read_forms("#js {:odd}").is_err());
    assert!(read_forms("#js {1 :value}").is_err());
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
    assert_eq!(read_forms(&nested).unwrap().len(), 1);
    assert_eq!(
        read_forms(r#""\123z\7 \b\f""#).unwrap()[0].kind,
        Kind::String(vec![83, 122, 7, 32, 8, 12])
    );
}

#[test]
fn deep_reader_clone_selection_and_cleanup_are_stack_safe() {
    let depth = 2000;
    let source = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
    let forms = read_forms(&source).unwrap();
    let copied = forms.clone();
    assert_eq!(copied, forms);
    let selected = resolve_conditionals(copied).unwrap();
    let mut item = &selected[0];
    for _ in 0..depth {
        let Kind::Vector(items) = &item.kind else {
            panic!("lost nesting")
        };
        assert_eq!(items.len(), 1);
        item = &items[0];
    }
    assert_eq!(item.kind, Kind::Number(0.0));
    drop(forms);
    drop(selected);
    // Partial nested trees must also clean up when parsing stops at EOF.
    assert!(
        read_forms(&source[..source.len() - 1])
            .unwrap_err()
            .expected
            .contains(&"more input".to_owned())
    );
}

#[test]
fn reader_and_selection_share_a_supplied_budget_and_recover_after_errors() {
    use suss_reader::forms::{read_forms_metered, resolve_conditionals_metered};
    let source = "[^:tag #?@(:suss [x y]) #_ #?(:jvm ignored) discarded retained]";
    let mut remaining = 3_u64;
    let mut meter = |units| {
        remaining = remaining
            .checked_sub(units)
            .ok_or_else(|| suss_reader::ParseError {
                message: "caller budget exhausted".into(),
                span: 0..0,
                expected: vec![],
            })?;
        Ok(())
    };
    assert_eq!(
        read_forms_metered(source, &mut meter).unwrap_err().message,
        "caller budget exhausted"
    );
    let raw = read_forms(source).unwrap();
    let mut remaining = 2_u64;
    let mut meter = |units| {
        remaining = remaining
            .checked_sub(units)
            .ok_or_else(|| suss_reader::ParseError {
                message: "selection budget exhausted".into(),
                span: 0..0,
                expected: vec![],
            })?;
        Ok(())
    };
    assert_eq!(
        resolve_conditionals_metered(raw, &mut meter)
            .unwrap_err()
            .message,
        "selection budget exhausted"
    );
    let resolved = resolve_conditionals(read_forms(source).unwrap()).unwrap();
    let Kind::Vector(items) = &resolved[0].kind else {
        panic!()
    };
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].metadata.len(), 1);
    assert!(matches!(&items[2].kind, Kind::Symbol(x) if x.name == "retained"));
}

#[test]
fn reader_cancellation_preserves_callback_error_and_next_read_recovers() {
    use suss_reader::forms::read_forms_metered;
    let mut calls = 0;
    let error = read_forms_metered("[\"many characters\" 2]", &mut |_| {
        calls += 1;
        if calls == 7 {
            Err(suss_reader::ParseError {
                message: "cancelled by caller".into(),
                span: 1..2,
                expected: vec!["cancellation-token".into()],
            })
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(error.message, "cancelled by caller");
    assert_eq!(error.span, 1..2);
    assert_eq!(error.expected, vec!["cancellation-token"]);
    assert_eq!(calls, 7, "no further callback work after cancellation");
    assert_eq!(read_forms("[1]").unwrap().len(), 1);
}

#[test]
fn large_anonymous_indices_are_metered_before_generated_slot_allocation() {
    use suss_reader::forms::read_forms_with_work_allowance;
    assert!(
        read_forms_with_work_allowance("#(%2147483647)", 1000)
            .unwrap_err()
            .message
            .contains("work budget")
    );
    assert!(
        read_forms("#(%2147483648)")
            .unwrap_err()
            .message
            .contains("representable as i32")
    );
    let forms = read_forms_with_work_allowance("#(%65536)", 1_000_000).unwrap();
    let Kind::List(items) = &forms[0].kind else {
        panic!()
    };
    let Kind::Vector(params) = &items[1].kind else {
        panic!()
    };
    assert_eq!(params.len(), 65536);
    assert!(matches!(&params[65535].kind, Kind::Symbol(s) if s.name.starts_with("p65536__")));
}

#[test]
fn source_metadata_meter_preserves_locations_and_checks_work() {
    use suss_reader::forms::{
        read_forms_with_source_metadata, read_forms_with_source_metadata_metered,
    };
    let source = format!("{}^:tag x{}", "[".repeat(100), "]".repeat(100));
    let ordinary = read_forms_with_source_metadata(&source, Some("located.sus")).unwrap();
    let mut used = 0_u64;
    let metered =
        read_forms_with_source_metadata_metered(&source, Some("located.sus"), &mut |units| {
            used += units;
            Ok(())
        })
        .unwrap();
    assert_eq!(ordinary, metered);
    assert!(used > source.len() as u64);
    let error = read_forms_with_source_metadata_metered(&source, Some("located.sus"), &mut |_| {
        Err(suss_reader::ParseError {
            message: "metadata cancelled".into(),
            span: 0..0,
            expected: vec![],
        })
    })
    .unwrap_err();
    assert_eq!(error.message, "metadata cancelled");
}
