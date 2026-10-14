use suss_reader::forms;
fn text(form: &forms::Form) -> String {
    use forms::Kind;
    fn join(items: &[forms::Form]) -> String {
        items.iter().map(text).collect::<Vec<_>>().join(" ")
    }
    match &form.kind {
        Kind::Nil => "nil".into(),
        Kind::Bool(x) => x.to_string(),
        Kind::Number(x) => x.to_string(),
        Kind::String(x) => format!("{:?}", String::from_utf16_lossy(x)),
        Kind::Symbol(x) => {
            let spelling = x.to_string();
            if spelling.starts_with("rest__") && spelling.ends_with('#') {
                "arg-rest".into()
            } else if let Some((index, _)) =
                spelling.strip_prefix('p').and_then(|s| s.split_once("__"))
            {
                if spelling.ends_with('#') && index.parse::<i32>().is_ok() {
                    format!("arg-{index}")
                } else {
                    spelling
                }
            } else {
                spelling
            }
        }
        Kind::Keyword(x) => format!(":{}", x),
        Kind::List(x) => format!("({})", join(x)),
        Kind::Vector(x) => format!("[{}]", join(x)),
        Kind::Map(x) => format!("{{{}}}", join(x)),
        Kind::Set(x) => format!("#{{{}}}", join(x)),
        _ => panic!("unresolved raw reader variant"),
    }
}
#[test]
fn pinned_reader_shapes_match() {
    let cases: &[(&str, &str, bool)] = &[
        ("#(+ % 1)", "(fn* [arg-1] (+ arg-1 1))", false),
        (
            "#(vector % %1 %3)",
            "(fn* [arg-1 arg-2 arg-3] (vector arg-1 arg-1 arg-3))",
            false,
        ),
        (
            "#(list %& %2)",
            "(fn* [arg-1 arg-2 & arg-rest] (list arg-rest arg-2))",
            false,
        ),
        (
            "#(vector '% %)",
            "(fn* [arg-1] (vector (quote arg-1) arg-1))",
            false,
        ),
        (
            "#(vector #_ %9 %)",
            "(fn* [arg-1 arg-2 arg-3 arg-4 arg-5 arg-6 arg-7 arg-8 arg-9] (vector arg-1))",
            false,
        ),
        (
            "#(vector #?(:cljs %2 :default %9) %)",
            "(fn* [arg-1 arg-2 arg-3 arg-4 arg-5 arg-6 arg-7 arg-8 arg-9] (vector arg-2 arg-1))",
            false,
        ),
        (
            "#(vector #?(:jvm %9 :cljs %) %)",
            "(fn* [arg-1 arg-2 arg-3 arg-4 arg-5 arg-6 arg-7 arg-8 arg-9] (vector arg-1 arg-1))",
            false,
        ),
        (
            "#(vector ^:tag %2)",
            "(fn* [arg-1 arg-2] (vector arg-2))",
            false,
        ),
        (
            "#(fn [x] (+ x %))",
            "(fn* [arg-1] (fn [x] (+ x arg-1)))",
            false,
        ),
        ("#()", "(fn* [] ())", false),
        ("#(#(+ % 1))", "", true),
        ("#(%0)", "(fn* [] (arg-0))", false),
        ("#(%-1)", "(fn* [& arg-rest] (arg-rest))", false),
        ("#(%2.0)", "", true),
        ("#(%foo)", "", true),
        ("[1 #?@(:cljs [2 3]) 4]", "[1 2 3 4]", false),
        ("(a #?@(:suss (b c)) d)", "(a b c d)", false),
        ("{:a 1 #?@(:cljs [:b 2])}", "{:a 1, :b 2}", false),
        ("[1 #?@(:jvm [2]) 3]", "[1 3]", false),
        ("[#?@(:cljs []) 4]", "[4]", false),
        ("[#?@(:cljs nil)]", "", true),
        ("#?@(:cljs [1])", "", true),
        ("[#?@(:default [1] :cljs [2])]", "[1]", false),
        ("['#?@(:cljs [x y]) z]", "[(quote x) y z]", false),
    ];
    for (source, expected, read_error) in cases {
        let result = forms::read_forms(source).and_then(forms::resolve_conditionals);
        if *read_error {
            assert!(result.is_err(), "{source}: expected read error");
            continue;
        }
        let actual = result.unwrap_or_else(|error| panic!("{source}: {error:?}"));
        let expected = forms::resolve_conditionals(forms::read_forms(expected).unwrap()).unwrap();
        assert_eq!(
            actual.iter().map(text).collect::<Vec<_>>(),
            expected.iter().map(text).collect::<Vec<_>>(),
            "{source}"
        );
    }
}

#[test]
fn anonymous_literals_have_distinct_names_and_consistent_references() {
    let forms =
        forms::resolve_conditionals(forms::read_forms("[#(vector % %1) #(vector % %1)]").unwrap())
            .unwrap();
    let forms::Kind::Vector(literals) = &forms[0].kind else {
        panic!()
    };
    let name = |literal: &forms::Form| {
        let forms::Kind::List(parts) = &literal.kind else {
            panic!()
        };
        let forms::Kind::Vector(args) = &parts[1].kind else {
            panic!()
        };
        let forms::Kind::Symbol(arg) = &args[0].kind else {
            panic!()
        };
        let forms::Kind::List(body) = &parts[2].kind else {
            panic!()
        };
        assert_eq!(body[1].kind, forms::Kind::Symbol(arg.clone()));
        assert_eq!(body[2].kind, forms::Kind::Symbol(arg.clone()));
        arg.clone()
    };
    assert_ne!(name(&literals[0]), name(&literals[1]));
}
#[test]
fn anonymous_indexing_metadata_matches_pinned_generated_and_real_nodes() {
    let result = forms::read_forms_with_source_metadata("#(+ % 1)", Some("reader.sus")).unwrap();
    assert!(result[0].metadata.is_empty());
    let forms::Kind::List(parts) = &result[0].kind else {
        panic!()
    };
    assert!(parts[0].metadata.is_empty());
    assert!(parts[1].metadata.is_empty());
    let forms::Kind::Vector(args) = &parts[1].kind else {
        panic!()
    };
    assert!(args[0].metadata.is_empty());
    assert!(!parts[2].metadata.is_empty());
    assert_eq!(parts[2].span, 1..8);
    let forms::Kind::List(body) = &parts[2].kind else {
        panic!()
    };
    assert!(!body[0].metadata.is_empty());
    assert!(body[1].metadata.is_empty());
    let result =
        forms::read_forms_with_source_metadata("#(vector ^:tag %2)", Some("reader.sus")).unwrap();
    let forms::Kind::List(parts) = &result[0].kind else {
        panic!()
    };
    let forms::Kind::List(body) = &parts[2].kind else {
        panic!()
    };
    assert_eq!(body[1].metadata.len(), 1);
    assert!(matches!(&body[1].metadata[0].kind,forms::Kind::Keyword(key) if key.name=="tag"));
}

#[test]
fn pinned_reader_edges_match() {
    let cases: &[(&str, &str, bool)] = &[
        ("#?(:cljs #?@(:cljs [x]))", "", true),
        ("[#?@(:cljs #?@(:cljs [[x y]]))]", "[x y]", false),
        ("[^:m #?@(:cljs [x y])]", "[x y]", false),
        ("[^:m #?@(:cljs []) z]", "[z]", false),
        ("[#_#?@(:cljs [x y]) z]", "[y z]", false),
        ("[#?@(:cljs {:a 1})]", "", true),
        (
            "#(%0x10)",
            "(fn* [arg-1 arg-2 arg-3 arg-4 arg-5 arg-6 arg-7 arg-8 arg-9 arg-10 arg-11 arg-12 arg-13 arg-14 arg-15 arg-16] (arg-16))",
            false,
        ),
        (
            "#(%010)",
            "(fn* [arg-1 arg-2 arg-3 arg-4 arg-5 arg-6 arg-7 arg-8] (arg-8))",
            false,
        ),
        ("#(%2N)", "(fn* [arg-1 arg-2] (arg-2))", false),
        ("#(%08)", "", true),
        (
            "#(vector %&junk)",
            "(fn* [& arg-rest] (vector arg-rest junk))",
            false,
        ),
        ("[% %1 %&]", "[% %1 %&]", false),
    ];
    for (source, expected, read_error) in cases {
        let result = forms::read_forms(source).and_then(forms::resolve_conditionals);
        if *read_error {
            assert!(result.is_err(), "{source}: expected read error");
            continue;
        }
        let actual = result.unwrap_or_else(|error| panic!("{source}: {error:?}"));
        let expected = forms::resolve_conditionals(forms::read_forms(expected).unwrap()).unwrap();
        assert_eq!(
            actual.iter().map(text).collect::<Vec<_>>(),
            expected.iter().map(text).collect::<Vec<_>>(),
            "{source}"
        );
    }
}

#[test]
fn pinned_logical_argument_indices_match() {
    let cases: &[(&str, &str, bool)] = &[
        ("#(%#?(:cljs 2))", "(fn* [arg-1 arg-2] (arg-2))", false),
        ("#(%#_ 9 2)", "(fn* [arg-1 arg-2] (arg-2))", false),
        (
            "#(%#?@(:cljs [2 3]))",
            "(fn* [arg-1 arg-2] (arg-2 3))",
            false,
        ),
    ];
    for (source, expected, read_error) in cases {
        let result = forms::read_forms(source).and_then(forms::resolve_conditionals);
        if *read_error {
            assert!(result.is_err(), "{source}: expected read error");
            continue;
        }
        let actual = result.unwrap_or_else(|error| panic!("{source}: {error:?}"));
        let expected = forms::resolve_conditionals(forms::read_forms(expected).unwrap()).unwrap();
        assert_eq!(
            actual.iter().map(text).collect::<Vec<_>>(),
            expected.iter().map(text).collect::<Vec<_>>(),
            "{source}"
        );
    }
}

#[test]
fn metadata_on_splice_follows_first_retained_logical_form() {
    for source in ["[^:m #?@(:cljs [x y])]", "[^:m #?@(:cljs []) z]"] {
        let forms = forms::resolve_conditionals(forms::read_forms(source).unwrap()).unwrap();
        let forms::Kind::Vector(items) = &forms[0].kind else {
            panic!()
        };
        assert_eq!(items[0].metadata.len(), 1);
        assert!(matches!(&items[0].metadata[0].kind,forms::Kind::Keyword(key) if key.name=="m"));
        if items.len() > 1 {
            assert!(items[1].metadata.is_empty());
        }
    }
}

#[test]
fn logical_index_discard_requests_following_raw_forms() {
    let source = "#(%#_ #?(:jvm ignored) 2 3)";
    let result = forms::resolve_conditionals(forms::read_forms(source).unwrap()).unwrap();
    let expected = forms::resolve_conditionals(
        forms::read_forms("(fn* [arg-1 arg-2 arg-3] (arg-3))").unwrap(),
    )
    .unwrap();
    assert_eq!(text(&result[0]), text(&expected[0]));
}

#[test]
fn closed_inner_queue_errors_do_not_consume_outer_siblings() {
    let failure = forms::read_forms("#(%#?(:cljs ['#?(:jvm ignored)]) 2)").unwrap_err();
    assert_eq!(failure.message, "Reader prefix requires a retained form");
    assert!(
        !failure
            .expected
            .iter()
            .any(|hint| hint == "logical continuation")
    );
}
