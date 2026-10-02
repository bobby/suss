use std::collections::BTreeMap;
use suss_cli::{portable_macro_data::FormBridge, portable_session::Session};
use suss_compile::portable::{self, AnalysisContext};
use suss_reader::forms::{Form, Kind};

#[derive(Default)]
struct Contexts {
    calls: BTreeMap<String, String>,
    ordered_calls: Vec<(String, String)>,
}
impl portable::ExpansionHost for Contexts {
    fn expand(
        &mut self,
        form: &Form,
        context: portable::ExpansionContext<'_>,
    ) -> Result<Option<Form>, portable::Diagnostic> {
        let Kind::List(items) = &form.kind else {
            return Ok(None);
        };
        if !matches!(&items[0].kind, Kind::Symbol(s) if s.namespace.is_none() && s.name == "context")
        {
            return Ok(None);
        }
        let Kind::String(label) = &items[1].kind else {
            panic!("context label")
        };
        let value = match context.context {
            AnalysisContext::Statement => "statement",
            AnalysisContext::Expression => "expr",
            AnalysisContext::Return => "return",
        };
        self.ordered_calls
            .push((String::from_utf16(label).unwrap(), value.into()));
        assert!(
            self.calls
                .insert(String::from_utf16(label).unwrap(), value.into())
                .is_none()
        );
        Ok(Some(Form {
            span: form.span.clone(),
            metadata: vec![],
            kind: Kind::String(value.encode_utf16().collect()),
        }))
    }
}
#[test]
fn compiler_analysis_context_matches_pinned_facts_without_granting_recur_tail_scope() {
    let fixture = include_str!("../../../tests/oracle/src/suss_oracle/context_facts.cljs");
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/context-facts-observations.json"
    ))
    .unwrap();
    assert_eq!(golden["schema"], 1);
    assert_eq!(
        golden["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let calls: BTreeMap<String, String> = golden["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| {
            (
                pair[0].as_str().unwrap().into(),
                pair[1].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(calls.len(), 24);
    assert_eq!(golden["results"].as_array().unwrap().len(), 15);
    let ordered_calls: Vec<(String, String)> = golden["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| {
            (
                pair[0].as_str().unwrap().into(),
                pair[1].as_str().unwrap().into(),
            )
        })
        .collect();
    let body =
        &fixture[fixture.find("(context \"top\")").unwrap()..fixture.find("(defn -main").unwrap()];
    let source = format!("\n\n{body}");
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut host = Contexts::default();
        session.eval_with_macros(&source, &mut host).unwrap();
        assert_eq!(host.ordered_calls, ordered_calls);
        assert_eq!(host.calls, calls);
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        for (name, expected) in [
            "direct", "doone", "domany", "letone", "letinit", "branch", "bare", "tried",
            "function", "fnbranch", "asfinal", "andmany", "ormany", "selected", "andone",
        ]
        .into_iter()
        .zip(golden["results"].as_array().unwrap())
        {
            let value = session.eval(name).unwrap();
            let decoded = bridge.read(&mut session, &value, 0..1).unwrap();
            assert!(
                matches!(decoded.kind, Kind::String(units) if units == expected.as_str().unwrap().encode_utf16().collect::<Vec<_>>())
            );
        }
        // :return is analyzer context, not a license for recur in an operand.
        let error = session
            .eval("(loop [x 1] (+ 0 (let [] (recur 2))))")
            .unwrap_err();
        assert!(format!("{error:?}").contains("tail"));
        session.eval("(def recovered 42)").unwrap();
    }
}
