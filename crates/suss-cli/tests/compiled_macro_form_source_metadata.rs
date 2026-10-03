use suss_cli::{
    portable_macro_data::FormBridge, portable_macros::CompiledMacros, portable_session::Session,
};
use suss_reader::forms::{Form, Kind};

fn assert_primary(label: &str, actual: &Form) {
    fn check(actual: &Form, expected: &serde_json::Value) {
        match expected {
            serde_json::Value::Null => assert_eq!(actual.kind, Kind::Nil),
            serde_json::Value::Bool(value) => assert_eq!(actual.kind, Kind::Bool(*value)),
            serde_json::Value::Number(value) => {
                assert_eq!(actual.kind, Kind::Number(value.as_f64().unwrap()))
            }
            serde_json::Value::String(value) => {
                assert_eq!(actual.kind, Kind::String(value.encode_utf16().collect()))
            }
            serde_json::Value::Array(values) => {
                let Kind::Vector(items) = &actual.kind else {
                    panic!("primary source metadata vector: {actual:?}")
                };
                assert_eq!(items.len(), values.len());
                for (item, value) in items.iter().zip(values) {
                    check(item, value);
                }
            }
            _ => panic!("unsupported primary source metadata projection"),
        }
    }
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/form-source-metadata-observations.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(
        cases
            .iter()
            .map(|case| case[0].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "nested",
            "unicode-crlf",
            "overrides",
            "file",
            "generated",
            "conditional",
            "chained",
            "slash",
            "tag"
        ]
    );
    check(
        actual,
        &cases.iter().find(|case| case[0] == label).unwrap()[1],
    );
}

#[test]
fn compiled_source_macro_form_retains_reader_positions_on_nested_syntax() {
    let definition = include_str!("../../../tests/oracle/form-source-metadata-macro.sus");
    for (label, source) in [
        (
            "nested",
            include_str!("../../../tests/oracle/form-source-metadata-input.sus"),
        ),
        (
            "unicode-crlf",
            include_str!("../../../tests/oracle/form-source-metadata-unicode.sus"),
        ),
    ] {
        for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
            let mut macros = CompiledMacros::new().unwrap();
            macros.define(definition).unwrap();
            let value = session.eval_with_macros(source, &mut macros).unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            session.collect().unwrap();
            let decoded = bridge.read(&mut session, &value, 0..source.len()).unwrap();
            assert_primary(label, &decoded);
            let Kind::Vector(rows) = decoded.kind else {
                panic!("source metadata rows")
            };
            assert_eq!(rows.len(), 4);
            for (index, row) in rows.iter().enumerate() {
                let Kind::Vector(fields) = &row.kind else {
                    panic!("source metadata fields")
                };
                assert_eq!(fields.len(), if index == 1 { 5 } else { 6 });
                for (field, value) in fields[..4].iter().enumerate() {
                    assert!(
                        matches!(value.kind, Kind::Number(n) if n > 0.0 && n.fract() == 0.0),
                        "actual reader position missing at row {index}, field {field}: {value:?}"
                    );
                }
                assert_eq!(fields[4].kind, Kind::Nil, "string input has no filename");
                if index != 1 {
                    assert_eq!(fields[5].kind, Kind::Bool(true));
                }
            }
        }
    }
}

#[test]
fn compiled_source_macro_form_preserves_explicit_location_overrides() {
    let source = include_str!("../../../tests/oracle/form-source-metadata-overrides.sus");
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(include_str!(
                "../../../tests/oracle/form-source-metadata-macro.sus"
            ))
            .unwrap();
        let value = session.eval_with_macros(source, &mut macros).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let decoded = bridge.read(&mut session, &value, 0..source.len()).unwrap();
        assert_primary("overrides", &decoded);
        let Kind::Vector(rows) = decoded.kind else {
            panic!("metadata rows")
        };
        for (row, expected) in [
            (0, &[50.0, 60.0, 70.0, 80.0][..]),
            (2, &[11.0, 12.0][..]),
            (3, &[21.0, 22.0][..]),
        ] {
            let Kind::Vector(fields) = &rows[row].kind else {
                panic!("metadata fields")
            };
            for (field, expected) in fields.iter().zip(expected) {
                assert_eq!(field.kind, Kind::Number(*expected));
            }
            assert_eq!(fields[5].kind, Kind::Bool(true));
        }
    }
}

#[test]
fn compiled_source_macro_generated_syntax_does_not_invent_reader_locations() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(include_str!(
                "../../../tests/oracle/form-source-metadata-macro.sus"
            ))
            .unwrap();
        macros
            .define(include_str!(
                "../../../tests/oracle/form-source-metadata-generator.sus"
            ))
            .unwrap();
        let value = session
            .eval_with_macros("\n  (make-source-form)", &mut macros)
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let decoded = bridge.read(&mut session, &value, 0..1).unwrap();
        assert_primary("generated", &decoded);
        let Kind::Vector(rows) = decoded.kind else {
            panic!("generated metadata rows")
        };
        assert_eq!(rows.len(), 4);
        for (index, row) in rows.iter().enumerate() {
            let Kind::Vector(fields) = &row.kind else {
                panic!("generated metadata fields")
            };
            assert_eq!(fields.len(), if index == 1 { 5 } else { 6 });
            assert!(
                fields.iter().all(|field| field.kind == Kind::Nil),
                "generated syntax acquired source facts: {fields:?}"
            );
        }
    }
}

#[test]
fn compiled_source_macro_file_metadata_uses_the_loaded_source_path() {
    use suss_cli::portable_session::SessionOptions;
    use suss_compile::portable::resolve::Phase;
    let root = tempfile::tempdir().unwrap();
    let tools = root.path().join("tools.sus");
    let app = root.path().join("app.sus");
    std::fs::write(
        &tools,
        format!(
            "(ns tools)\n{}",
            include_str!("../../../tests/oracle/form-source-metadata-macro.sus")
        ),
    )
    .unwrap();
    std::fs::write(
        &app,
        concat!(
            "(ns app (:require-macros [tools :refer [form-source-facts]]))\n",
            "(def facts ^:call-mark (form-source-facts ^:arg-mark [^:sym-mark item]))\n"
        ),
    )
    .unwrap();
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(
            SessionOptions {
                source_paths: vec![root.path().to_owned()],
                ..SessionOptions::default()
            },
            phase,
        )
        .unwrap();
        session
            .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
            .unwrap();
        session.enter_namespace("user").unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        session
            .load_namespace_with_macros("app", &mut macros)
            .unwrap();
        let value = session.eval("app/facts").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let decoded = bridge.read(&mut session, &value, 0..1).unwrap();
        let Kind::Vector(rows) = decoded.kind else {
            panic!("file metadata rows")
        };
        assert_eq!(rows.len(), 4);
        for (index, row) in rows.iter().enumerate() {
            let Kind::Vector(fields) = &row.kind else {
                panic!("file metadata fields")
            };
            assert_eq!(fields.len(), if index == 1 { 5 } else { 6 });
            assert_eq!(
                fields[4].kind,
                Kind::String(
                    app.canonicalize()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .encode_utf16()
                        .collect()
                ),
                "loaded file provenance missing at row {index}"
            );
        }
    }
}

#[test]
fn compiled_source_macro_conditional_and_chained_metadata_match_actual_reader_data() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(include_str!(
                "../../../tests/oracle/form-source-metadata-macro.sus"
            ))
            .unwrap();
        for (label, source) in [
            (
                "conditional",
                include_str!("../../../tests/oracle/form-source-metadata-conditional.sus"),
            ),
            (
                "chained",
                include_str!("../../../tests/oracle/form-source-metadata-chained.sus"),
            ),
            (
                "slash",
                include_str!("../../../tests/oracle/form-source-metadata-slash.sus"),
            ),
        ] {
            let value = session.eval_with_macros(source, &mut macros).unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            session.collect().unwrap();
            let decoded = bridge.read(&mut session, &value, 0..source.len()).unwrap();
            assert_primary(label, &decoded);
        }
    }
}

#[test]
fn compiled_source_macro_tag_values_retain_their_own_reader_metadata() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(include_str!(
                "../../../tests/oracle/form-source-metadata-tag-macro.sus"
            ))
            .unwrap();
        let source = include_str!("../../../tests/oracle/form-source-metadata-tag.sus");
        let value = session.eval_with_macros(source, &mut macros).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let decoded = bridge.read(&mut session, &value, 0..source.len()).unwrap();
        assert_primary("tag", &decoded);
    }
}

#[test]
fn source_metadata_snapshot_rejects_invalid_source_instead_of_erasing_failure() {
    let source = "(form-source-facts [item]) (";
    let form = suss_reader::forms::read_forms("(form-source-facts [item])")
        .unwrap()
        .remove(0);
    let origin = suss_compile::portable::SourceOrigin::new(source, None);
    let error = origin.macro_form_data(&form).unwrap_err();
    assert_eq!(error.span, form.span);
    assert!(error.message.contains("source metadata snapshot"));
}

#[cfg(unix)]
#[test]
fn source_metadata_snapshot_rejects_non_unicode_paths_instead_of_replacement_characters() {
    use std::os::unix::ffi::OsStringExt;
    let source = "(form-source-facts [item])";
    let form = suss_reader::forms::read_forms(source).unwrap().remove(0);
    let path = std::path::PathBuf::from(std::ffi::OsString::from_vec(b"app-\xff.sus".to_vec()));
    let origin = suss_compile::portable::SourceOrigin::new(source, Some(path));
    let error = origin.macro_form_data(&form).unwrap_err();
    assert_eq!(error.span, form.span);
    assert!(error.message.contains("Unicode"));
}

#[test]
fn compiled_source_macro_metadata_is_data_and_never_replays_assignment_syntax() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define("(defmacro metadata-data [] (list 'quote (get (meta &form) :payload)))")
            .unwrap();
        session.eval("(def metadata-effects 0)").unwrap();
        let value = session
            .eval_with_macros(
                "^{:payload (set! metadata-effects 99)} (metadata-data)",
                &mut macros,
            )
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let decoded = bridge.read(&mut session, &value, 0..1).unwrap();
        let Kind::List(items) = decoded.kind else {
            panic!("assignment syntax remains data")
        };
        assert_eq!(items.len(), 3);
        assert!(matches!(&items[0].kind, Kind::Symbol(symbol) if symbol.name == "set!"));
        assert!(
            matches!(&items[1].kind, Kind::Symbol(symbol) if symbol.name == "metadata-effects")
        );
        assert_eq!(items[2].kind, Kind::Number(99.0));
        let effects = session.eval("metadata-effects").unwrap();
        assert_eq!(
            bridge.read(&mut session, &effects, 0..1).unwrap().kind,
            Kind::Number(0.0)
        );
    }
}

#[test]
fn compiled_source_macro_metadata_bound_failure_preserves_bindings_and_recovers() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define("(defmacro metadata-data [] (list 'quote (get (meta &form) :payload)))")
            .unwrap();
        session
            .eval("(def metadata-keep 7) (def metadata-effects 0)")
            .unwrap();
        let source = format!(
            "(def metadata-keep (do (set! metadata-effects 1) (metadata-data)))\n;{}",
            "x".repeat(1_048_576)
        );
        let error = session.eval_with_macros(&source, &mut macros).unwrap_err();
        let suss_cli::portable_session::SessionError::Compile(error) = error else {
            panic!("located compile diagnostic")
        };
        assert!(error.message.contains("snapshot exceeds 1 MiB"));
        assert_eq!(&source[error.span], "(metadata-data)");
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        for (name, expected) in [("metadata-keep", 7.0), ("metadata-effects", 0.0)] {
            let value = session.eval(name).unwrap();
            assert_eq!(
                bridge.read(&mut session, &value, 0..1).unwrap().kind,
                Kind::Number(expected)
            );
        }
        let value = session
            .eval_with_macros("^{:payload 42} (metadata-data)", &mut macros)
            .unwrap();
        assert_eq!(
            bridge.read(&mut session, &value, 0..1).unwrap().kind,
            Kind::Number(42.0)
        );
    }
}
