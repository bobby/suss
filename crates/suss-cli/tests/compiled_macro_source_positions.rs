use suss_cli::{portable_macro_data::FormBridge, portable_session::Session};
use suss_compile::portable::{self, Diagnostic, ExpansionContext, ExpansionHost, SourceOrigin};
use suss_reader::forms::{Form, Kind};

#[derive(Default)]
struct Positions {
    calls: Vec<(usize, usize)>,
}
impl ExpansionHost for Positions {
    fn expand(
        &mut self,
        form: &Form,
        context: ExpansionContext<'_>,
    ) -> Result<Option<Form>, Diagnostic> {
        let Kind::List(items) = &form.kind else {
            return Ok(None);
        };
        let Kind::Symbol(operator) = &items[0].kind else {
            return Ok(None);
        };
        if operator.namespace.is_some()
            || !matches!(operator.name.as_str(), "position" | "inspect-local")
        {
            return Ok(None);
        }
        let location = if operator.name == "inspect-local" {
            let Some(Form {
                kind: Kind::Symbol(name),
                ..
            }) = items.get(1)
            else {
                panic!("local name")
            };
            let binding = context
                .locals
                .get(&name.name)
                .expect("actual lexical declaration");
            binding
                .origin
                .as_ref()
                .and_then(|origin| origin.symbol_position(&binding.declaration))
        } else {
            context
                .origin
                .and_then(|origin| origin.position(form.span.start))
        }
        .ok_or_else(|| Diagnostic {
            span: form.span.clone(),
            message: "Position requires an actual source origin".into(),
        })?;
        self.calls.push((location.line, location.column));
        Ok(Some(Form {
            span: form.span.clone(),
            metadata: vec![],
            kind: Kind::Vector(
                [location.line, location.column]
                    .into_iter()
                    .map(|n| Form {
                        span: form.span.clone(),
                        metadata: vec![],
                        kind: Kind::Number(n as f64),
                    })
                    .collect(),
            ),
        }))
    }
}

#[test]
fn compiler_macro_source_positions_execute_with_exact_pinned_utf16_and_newline_locations() {
    let fixture = include_str!("../../../tests/oracle/src/suss_oracle/environment_origin.cljs");
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/environment-origin-observations.json"
    ))
    .unwrap();
    assert_eq!(golden["schema"], 1);
    assert_eq!(
        golden["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let expected: Vec<(usize, usize)> = golden["positions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| {
            (
                pair[0].as_u64().unwrap() as usize,
                pair[1].as_u64().unwrap() as usize,
            )
        })
        .collect();
    assert_eq!(expected.len(), 4);
    // The development-only oracle ns and runner are not shipped input. Preserve
    // every call's original line/column; the shared body remains unchanged.
    let body = &fixture[fixture.find("(def ascii").unwrap()..fixture.find("(defn -main").unwrap()];
    let source = format!("\n\n{body}");
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut host = Positions::default();
        session.eval_with_macros(&source, &mut host).unwrap();
        assert_eq!(host.calls, expected);
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        for (name, (line, column)) in ["ascii", "unicode", "crlf", "cronly"]
            .into_iter()
            .zip(&expected)
        {
            let value = session.eval(name).unwrap();
            let decoded = bridge.read(&mut session, &value, 0..1).unwrap();
            let Kind::Vector(items) = decoded.kind else {
                panic!("Position vector")
            };
            assert_eq!(items.len(), 2);
            for (item, expected) in items.iter().zip([*line, *column]) {
                assert!(
                    matches!(item.kind, Kind::Number(n) if n.to_bits() == (expected as f64).to_bits())
                );
            }
        }
    }
}

#[test]
fn compiler_macro_unknown_origin_is_not_reused_or_fabricated_and_invalid_offsets_are_rejected() {
    let source = "(position)";
    let forms = suss_reader::forms::read_forms(source).unwrap();
    let mut host = Positions::default();
    let error = portable::prepare_fragment_forms_with_expander(
        forms,
        0..source.len(),
        &portable::resolve::Environment::default(),
        portable::resolve::Phase::Runtime,
        &mut host,
    )
    .err()
    .expect("no fabricated source location");
    assert_eq!(error.span, 0..source.len());
    assert!(error.message.contains("actual source origin"));
    let origin = SourceOrigin::new("𝄞\r\nλ\r", None);
    assert!(origin.position(1).is_none());
    assert!(origin.position(usize::MAX).is_none());
    assert_eq!(origin.position(4).unwrap().column, 3);
    assert_eq!(origin.position(6).unwrap().line, 2);
    assert_eq!(origin.position(origin.text().len()).unwrap().line, 3);
}

#[test]
fn compiler_local_positions_exclude_metadata_prefixes_using_actual_source_tokens() {
    let fixture = include_str!("../../../tests/oracle/src/suss_oracle/metadata_position.cljs");
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/metadata-position-observations.json"
    ))
    .unwrap();
    assert_eq!(golden["schema"], 1);
    assert_eq!(
        golden["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let expected: Vec<(usize, usize)> = golden["positions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| {
            (
                pair[0].as_u64().unwrap() as usize,
                pair[1].as_u64().unwrap() as usize,
            )
        })
        .collect();
    assert_eq!(expected.len(), 4);
    let body = &fixture[fixture.find("(def plain").unwrap()..fixture.find("(defn -main").unwrap()];
    let source = format!("\n\n{body}");
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut host = Positions::default();
        session.eval_with_macros(&source, &mut host).unwrap();
        assert_eq!(host.calls, expected);
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        for (name, (line, column)) in ["plain", "tagged", "chained", "mapped"]
            .into_iter()
            .zip(&expected)
        {
            let value = session.eval(name).unwrap();
            let decoded = bridge.read(&mut session, &value, 0..1).unwrap();
            let Kind::Vector(items) = decoded.kind else {
                panic!("position vector")
            };
            assert_eq!(items.len(), 2);
            for (item, expected) in items.iter().zip([*line, *column]) {
                assert!(
                    matches!(item.kind, Kind::Number(n) if n.to_bits() == (expected as f64).to_bits())
                );
            }
        }
    }
    let origin = SourceOrigin::new("(generated)", None);
    let mut generated = suss_reader::forms::read_forms("generated")
        .unwrap()
        .remove(0);
    generated.span = 0..origin.text().len();
    assert!(origin.symbol_position(&generated).is_none());
    let origin = SourceOrigin::new("foox", None);
    generated.kind = Kind::Symbol(suss_reader::Symbol::new("x"));
    generated.span = 0..4;
    assert!(origin.symbol_position(&generated).is_none());
}
