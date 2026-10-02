use std::{collections::HashMap, sync::Arc};
use suss_cli::{portable_macro_data::FormBridge, portable_session::Session};
use suss_compile::portable::{
    self,
    hir::{Expression, FunctionScope, LocalBinding, LocalKind},
};
use suss_reader::forms::{Form, Kind};

#[derive(Default)]
struct Observe {
    calls: Vec<serde_json::Value>,
    scopes: HashMap<String, Vec<Arc<FunctionScope>>>,
    locals: HashMap<String, HashMap<String, LocalBinding>>,
}
fn name(scope: &FunctionScope) -> String {
    let Kind::Symbol(symbol) = &scope.declaration.kind else {
        panic!("function symbol");
    };
    symbol.name.clone()
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
        if !matches!(&items[0].kind, Kind::Symbol(s) if s.namespace.is_none() && s.name == "scope")
        {
            return Ok(None);
        }
        let Kind::String(label) = &items[1].kind else {
            panic!("scope label");
        };
        let label = String::from_utf16(label).unwrap();
        let names: Vec<_> = context
            .function_scopes
            .iter()
            .map(|scope| name(scope))
            .collect();
        let mut locals: Vec<_> = context.locals.keys().cloned().collect();
        locals.sort();
        self.calls
            .push(serde_json::json!({"label": label, "names": names, "locals": locals}));
        assert!(self
            .scopes
            .insert(label.clone(), context.function_scopes.to_vec())
            .is_none());
        self.locals.insert(label, context.locals.clone());
        Ok(Some(Form {
            span: form.span.clone(),
            metadata: vec![],
            kind: Kind::Vector(
                names
                    .into_iter()
                    .map(|name| Form {
                        span: form.span.clone(),
                        metadata: vec![],
                        kind: Kind::String(name.encode_utf16().collect()),
                    })
                    .collect(),
            ),
        }))
    }
}
#[test]
fn genuine_function_scopes_match_pinned_names_and_do_not_invent_hint_bindings() {
    let fixture = include_str!("../../../tests/oracle/src/suss_oracle/function_scope_facts.cljs");
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/function-scope-observations.json"
    ))
    .unwrap();
    assert_eq!(golden["schema"], 1);
    assert_eq!(
        golden["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let body = &fixture[fixture.find("(def hinted").unwrap()..fixture.find("(defn -main").unwrap()];
    let source = format!("\n\n{body}");
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut host = Observe::default();
        session.eval_with_macros(&source, &mut host).unwrap();
        assert_eq!(serde_json::json!(host.calls), golden["calls"]);
        assert_eq!(host.calls.len(), 11);
        for label in ["hinted", "declared"] {
            let scope = &host.scopes[label][0];
            assert!(scope.self_binding.is_none());
            assert!(!host.locals[label].contains_key(&name(scope)));
            assert!(scope.parents.is_empty());
            assert_eq!(scope.origin.as_ref().unwrap().text(), source);
        }
        let explicit = &host.scopes["named"][0];
        let local = explicit.self_binding.as_ref().unwrap();
        assert_eq!(local.kind, LocalKind::FunctionName);
        assert_eq!(local.id, host.locals["named"]["n"].id);
        assert_eq!(&source[explicit.declaration.span.clone()], "n");
        assert!(host.scopes["anonymous"].is_empty());
        assert!(host.scopes["outside"].is_empty());
        let nested = &host.scopes["nested"];
        assert_eq!(nested.len(), 2);
        assert!(Arc::ptr_eq(&nested[1].parents[0], &nested[0]));
        assert_eq!(
            nested[1].self_binding.as_ref().unwrap().id,
            host.locals["nested"]["inner"].id
        );
        let inherited = &host.scopes["inherited"];
        assert_eq!(inherited.len(), 1);
        let shadowed = &host.scopes["shadowed"][0];
        let shadow = shadowed.shadow.as_ref().unwrap();
        assert_eq!(shadow.kind, LocalKind::Let);
        assert!(matches!(
            shadow.initializer.as_ref().unwrap().kind,
            Expression::Literal(_)
        ));
        assert_ne!(shadow.id, shadowed.self_binding.as_ref().unwrap().id);
        assert_eq!(
            shadowed
                .self_binding
                .as_ref()
                .unwrap()
                .shadow
                .as_ref()
                .unwrap()
                .id,
            shadow.id
        );
        assert_eq!(
            host.scopes["zero"][0].self_binding.as_ref().unwrap().id,
            host.scopes["one"][0].self_binding.as_ref().unwrap().id
        );
        assert!(matches!(
            host.locals["one"]["x"].kind,
            LocalKind::Argument {
                index: 0,
                rest: false
            }
        ));
        assert!(matches!(
            host.locals["rest"]["xs"].kind,
            LocalKind::Argument {
                index: 0,
                rest: true
            }
        ));
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        for (call, expected) in [
            "(hinted)",
            "(named)",
            "anonymous",
            "(nested)",
            "(inherited)",
            "(declared)",
            "(shadowed)",
            "(multiple)",
            "(multiple 1)",
            "(variadic 1 2)",
        ]
        .into_iter()
        .zip(golden["results"].as_array().unwrap())
        {
            let value = session.eval(call).unwrap();
            session.collect().unwrap();
            let decoded = bridge.read(&mut session, &value, 0..1).unwrap();
            let Kind::Vector(items) = decoded.kind else {
                panic!("actual result vector");
            };
            let names: Vec<_> = items
                .into_iter()
                .map(|item| {
                    let Kind::String(units) = item.kind else {
                        panic!("actual result string");
                    };
                    String::from_utf16(&units).unwrap()
                })
                .collect();
            assert_eq!(serde_json::json!(names), *expected);
        }
        // Failed analysis never publishes its partial scope into a later fragment.
        assert!(session
            .eval_with_macros("(def failed (fn doomed [] missing))", &mut host)
            .is_err());
        let mut recovered = Observe::default();
        session
            .eval_with_macros("(scope \"recovered\")", &mut recovered)
            .unwrap();
        assert!(recovered.scopes["recovered"].is_empty());
    }
}

/// Expansion-created declarations must retain honest provenance, and only the
/// direct definition initializer may carry a definition name hint.
#[test]
fn function_hints_follow_expansion_without_leaking_into_operands_or_fabricating_origins() {
    #[derive(Default)]
    struct Expand {
        observed: Observe,
        facts_only: bool,
    }
    impl portable::ExpansionHost for Expand {
        fn expand(
            &mut self,
            form: &Form,
            context: portable::ExpansionContext<'_>,
        ) -> Result<Option<Form>, portable::Diagnostic> {
            if let Kind::List(items) = &form.kind {
                if let Some(Form {
                    kind: Kind::Symbol(symbol),
                    ..
                }) = items.first()
                {
                    let generated = match symbol.name.as_str() {
                        "make-function" => Some("(fn [] (scope \"expanded\"))"),
                        "make-self" => Some("(fn actual [] (scope \"self\"))"),
                        _ => None,
                    };
                    if let Some(generated) = generated {
                        let mut generated =
                            suss_reader::forms::read_forms(generated).unwrap().remove(0);
                        // A macro result has a call-site span, not a claim that
                        // its generated self-name occurs at that source token.
                        fn call_site(form: &mut Form, span: &std::ops::Range<usize>) {
                            form.span = span.clone();
                            if let Kind::List(items) | Kind::Vector(items) = &mut form.kind {
                                for item in items {
                                    call_site(item, span);
                                }
                            }
                        }
                        call_site(&mut generated, &form.span);
                        return Ok(Some(generated));
                    }
                }
            }
            let mut expanded = self.observed.expand(form, context)?;
            if self.facts_only {
                if let Some(form) = &mut expanded {
                    form.kind = Kind::Nil;
                }
            }
            Ok(expanded)
        }
    }
    let source = r#"(def hinted (make-function))
(def overridden (make-self))
(def wrapped (let [] (fn [] (scope "wrapped"))))
(scope "after")"#;
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut host = Expand::default();
        session.eval_with_macros(source, &mut host).unwrap();
        let hinted = &host.observed.scopes["expanded"][0];
        assert_eq!(name(hinted), "hinted");
        assert!(hinted.self_binding.is_none());
        assert!(!host.observed.locals["expanded"].contains_key("hinted"));
        assert_eq!(hinted.namespace, "user");
        assert!(hinted
            .origin
            .as_ref()
            .unwrap()
            .symbol_position(&hinted.declaration)
            .is_some());
        let explicit = &host.observed.scopes["self"][0];
        assert_eq!(name(explicit), "actual");
        assert_eq!(
            explicit.self_binding.as_ref().unwrap().id,
            host.observed.locals["self"]["actual"].id
        );
        assert!(explicit
            .origin
            .as_ref()
            .unwrap()
            .symbol_position(&explicit.declaration)
            .is_none());
        assert!(host.observed.scopes["wrapped"].is_empty());
        assert!(host.observed.scopes["after"].is_empty());
        let bridge = FormBridge::new(&mut session).unwrap();
        for (call, expected) in [("(hinted)", "hinted"), ("(overridden)", "actual")] {
            let value = session.eval(call).unwrap();
            session.collect().unwrap();
            let decoded = bridge.read(&mut session, &value, 0..1).unwrap();
            let Kind::Vector(items) = decoded.kind else {
                panic!("executed scope vector")
            };
            assert_eq!(items.len(), 1);
            assert!(matches!(&items[0].kind, Kind::String(units)
                if String::from_utf16(units).unwrap() == expected));
        }
    }
    // The public owned-forms API explicitly has no source origin.
    for phase in [
        portable::resolve::Phase::Runtime,
        portable::resolve::Phase::Macro,
    ] {
        let mut host = Expand::default();
        host.facts_only = true;
        let forms = suss_reader::forms::read_forms(source).unwrap();
        let _prepared = portable::prepare_fragment_forms_with_expander(
            forms,
            0..source.len(),
            &portable::resolve::Environment::default(),
            phase,
            &mut host,
        )
        .unwrap();
        assert_eq!(host.observed.calls.len(), 4);
        assert_eq!(host.observed.scopes["expanded"].len(), 1);
        assert_eq!(host.observed.scopes["self"].len(), 1);
        for scope in host.observed.scopes.values().flatten() {
            assert!(scope.origin.is_none());
            assert_eq!(scope.namespace, "user");
            if let Some(binding) = &scope.self_binding {
                assert!(binding.origin.is_none());
            }
        }
    }
}
