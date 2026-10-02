use std::collections::{BTreeMap, HashMap};
use suss_cli::portable_session::Session;
use suss_compile::portable::{
    self,
    hir::{Expression, FieldBinding, LocalBinding, LocalKind, SourceRole},
};
use suss_reader::forms::{Form, Kind};

fn local_role(binding: &LocalBinding) -> serde_json::Value {
    let (kind, argument) = match binding.source_kind() {
        LocalKind::Let => ("let", None),
        LocalKind::Loop => ("loop", None),
        LocalKind::Argument { index, .. } => ("arg", Some(index)),
        LocalKind::FunctionName => ("fn", None),
        LocalKind::Catch => ("catch", None),
    };
    serde_json::json!([kind, argument, false])
}
fn field_role(field: &FieldBinding) -> serde_json::Value {
    serde_json::json!(["field", null, field.mutable])
}
fn shadow(binding: &LocalBinding) -> serde_json::Value {
    if let SourceRole::MethodThis {
        argument: Some(argument),
        ..
    } = &binding.source_role
    {
        local_role(argument)
    } else if let Some(previous) = &binding.shadow {
        local_role(previous)
    } else if let Some(previous) = &binding.shadow_field {
        field_role(previous)
    } else {
        serde_json::Value::Null
    }
}
#[derive(Default)]
struct Observe {
    calls: Vec<serde_json::Value>,
    locals: HashMap<String, HashMap<String, LocalBinding>>,
}
impl portable::ExpansionHost for Observe {
    fn expand(
        &mut self,
        form: &Form,
        context: portable::ExpansionContext<'_>,
    ) -> Result<Option<Form>, portable::Diagnostic> {
        let Kind::List(items) = &form.kind else {
            return Ok(None);
        };
        if !matches!(&items[0].kind, Kind::Symbol(s) if s.namespace.is_none() && s.name == "fact") {
            return Ok(None);
        }
        let Kind::String(label) = &items[1].kind else {
            panic!("role label");
        };
        let label = String::from_utf16(label).unwrap();
        let mut bindings: BTreeMap<String, serde_json::Value> = context
            .fields
            .iter()
            .map(|(name, field)| {
                (
                    name.clone(),
                    serde_json::json!([name, field_role(field), null]),
                )
            })
            .collect();
        for (name, binding) in context.locals {
            bindings.insert(
                name.clone(),
                serde_json::json!([name, local_role(binding), shadow(binding)]),
            );
        }
        let count = bindings.len();
        self.calls.push(serde_json::json!([
            label,
            bindings.into_values().collect::<Vec<_>>()
        ]));
        assert!(self.locals.insert(label, context.locals.clone()).is_none());
        Ok(Some(Form {
            span: form.span.clone(),
            metadata: vec![],
            kind: Kind::Number(count as f64),
        }))
    }
}
#[test]
fn source_method_roles_match_pinned_arguments_receiver_and_shadow_facts_without_fake_ids() {
    let fixture = include_str!("../../../tests/oracle/src/suss_oracle/method_role_facts.cljs");
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/method-role-observations.json"
    ))
    .unwrap();
    assert_eq!(golden["schema"], 1);
    assert_eq!(
        golden["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let source =
        &fixture[fixture.find("(defprotocol Probe").unwrap()..fixture.find("(defn -main").unwrap()];
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut host = Observe::default();
        session.eval_with_macros(source, &mut host).unwrap();
        assert_eq!(serde_json::json!(host.calls), golden["calls"]);
        assert_eq!(host.calls.len(), 6);
        for (label, expected_type, protocol) in [
            ("protocol-parameter", "Holder", true),
            ("object-parameter", "Holder", false),
            ("protocol-field", "FieldHolder", true),
            ("object-field", "FieldHolder", false),
        ] {
            let receiver = &host.locals[label]["this"];
            let SourceRole::MethodThis {
                type_declaration,
                argument,
                access,
            } = &receiver.source_role
            else {
                panic!("actual this-as role");
            };
            assert!(matches!(
                receiver.kind,
                LocalKind::Argument {
                    index: 0,
                    rest: false
                }
            ));
            assert_eq!(receiver.source_kind(), LocalKind::Let);
            assert_eq!(&source[type_declaration.span.clone()], expected_type);
            assert_eq!(receiver.origin.as_ref().unwrap().text(), source);
            assert!(matches!(access.kind, Expression::Local(id) if id == receiver.id));
            assert_eq!(argument.is_some(), protocol);
            if let Some(argument) = argument {
                assert_eq!(argument.id, receiver.id);
                assert_eq!(
                    argument.source_kind(),
                    LocalKind::Argument {
                        index: 0,
                        rest: false
                    }
                );
            }
        }
        let object_arg = &host.locals["object-parameter"]["x"];
        assert_eq!(
            object_arg.kind,
            LocalKind::Argument {
                index: 1,
                rest: false
            }
        );
        assert_eq!(
            object_arg.source_kind(),
            LocalKind::Argument {
                index: 0,
                rest: false
            }
        );
        assert!(object_arg.shadow_field.as_ref().unwrap().mutable);
        let nested = &host.locals["nested-this"]["this"];
        assert!(matches!(nested.source_role, SourceRole::Plain));
        let previous = nested.shadow.as_ref().unwrap();
        assert_eq!(previous.source_kind(), LocalKind::Let);
        assert_ne!(previous.id, nested.id);
        assert_eq!(previous.id, host.locals["protocol-restored"]["this"].id);
        session.collect().unwrap();
        for (call, expected) in [
            "(probe (Holder. 1) 2)",
            "(.method (Holder. 1) 2)",
            "(probe (FieldHolder. 1) 2)",
            "(.method (FieldHolder. 1) 2)",
        ]
        .into_iter()
        .zip(golden["results"].as_array().unwrap())
        {
            let value = session.eval(call).unwrap();
            session.collect().unwrap();
            let expected = expected.as_u64().unwrap() as f64;
            session
                .inspect(&value, |mut store, value| {
                    let number = value
                        .unwrap_anyref()
                        .unwrap()
                        .as_struct(&store)?
                        .unwrap()
                        .fields(&mut store)?
                        .next()
                        .unwrap()
                        .unwrap_f64();
                    assert_eq!(number.to_bits(), expected.to_bits());
                    Ok(())
                })
                .unwrap();
        }
    }
}
