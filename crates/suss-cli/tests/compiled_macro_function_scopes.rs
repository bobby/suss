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
        assert!(
            self.scopes
                .insert(label.clone(), context.function_scopes.to_vec())
                .is_none()
        );
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
        assert!(
            session
                .eval_with_macros("(def failed (fn doomed [] missing))", &mut host)
                .is_err()
        );
        let mut recovered = Observe::default();
        session
            .eval_with_macros("(scope \"recovered\")", &mut recovered)
            .unwrap();
        assert!(recovered.scopes["recovered"].is_empty());
    }
}
