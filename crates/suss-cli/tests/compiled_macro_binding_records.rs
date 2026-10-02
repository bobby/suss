use std::collections::HashMap;
use suss_cli::portable_session::Session;
use suss_compile::portable::{
    self,
    hir::{Expression, LocalBinding, LocalKind},
};
use suss_reader::forms::{Form, Kind};

#[derive(Default)]
struct Observe {
    snapshots: Vec<(String, HashMap<String, LocalBinding>)>,
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
        if !matches!(&items[0].kind, Kind::Symbol(name) if name.namespace.is_none() && name.name == "checkpoint")
        {
            return Ok(None);
        }
        let Kind::String(label) = &items[1].kind else {
            panic!("label")
        };
        self.snapshots
            .push((String::from_utf16(label).unwrap(), context.locals.clone()));
        Ok(Some(items[2].clone()))
    }
}
fn snapshot<'a>(host: &'a Observe, name: &str) -> &'a HashMap<String, LocalBinding> {
    &host
        .snapshots
        .iter()
        .find(|(label, _)| label == name)
        .unwrap()
        .1
}
fn scalar(session: &mut Session, source: &str) -> f64 {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |mut store, value| {
            Ok(value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap()
                .fields(&mut store)?
                .next()
                .unwrap()
                .unwrap_f64())
        })
        .unwrap()
}

#[test]
fn compiler_macro_binding_records_preserve_initializer_shadow_scope_and_once_only_effects() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut host = Observe::default();
        let source = r#"(def effects (array)) (def result (let [^:private x (do (.push effects 1) 41)] (checkpoint "outer" x) (let [x (+ x 1)] (checkpoint "inner" x)) (checkpoint "restored" x)))"#;
        session.eval_with_macros(&source, &mut host).unwrap();
        assert_eq!(scalar(&mut session, "result"), 41.0);
        assert_eq!(scalar(&mut session, "(alength effects)"), 1.0);
        let outer = &snapshot(&host, "outer")["x"];
        let inner = &snapshot(&host, "inner")["x"];
        let restored = &snapshot(&host, "restored")["x"];
        assert_eq!(outer.kind, LocalKind::Let);
        assert_eq!(outer.ty, portable::hir::Type::Number);
        assert!(outer.shadow.is_none());
        assert_eq!(outer.declaration.metadata.len(), 1);
        assert!(matches!(
            outer.initializer.as_ref().unwrap().kind,
            Expression::Do(_)
        ));
        assert_eq!(inner.shadow.as_ref().unwrap().id, outer.id);
        assert_ne!(inner.id, outer.id);
        assert_eq!(restored.id, outer.id);
        assert_eq!(&source[outer.declaration.span.clone()], "^:private x");
        assert!(inner.initializer.is_some());
    }
}

#[test]
fn compiler_macro_binding_records_preserve_parameter_roles_catches_and_lowered_loop_identity() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut host = Observe::default();
        session.eval_with_macros(r#"(def result (let [x 41] (def f (fn named [^number x & xs] (checkpoint "args" x))) (loop [i 0] (checkpoint "loop" i)) (try (throw 2) (catch :default x (checkpoint "catch" x))) (checkpoint "restored" x)))"#, &mut host).unwrap();
        assert_eq!(scalar(&mut session, "result"), 41.0);
        assert_eq!(scalar(&mut session, "(f 42 1 2)"), 42.0);
        let args = snapshot(&host, "args");
        assert_eq!(
            args["x"].kind,
            LocalKind::Argument {
                index: 0,
                rest: false
            }
        );
        assert_eq!(
            args["xs"].kind,
            LocalKind::Argument {
                index: 1,
                rest: true
            }
        );
        assert_eq!(args["named"].kind, LocalKind::FunctionName);
        assert!(args["x"].initializer.is_none());
        assert_eq!(args["x"].declaration.metadata.len(), 1);
        assert_eq!(args["x"].shadow.as_ref().unwrap().kind, LocalKind::Let);
        let i = &snapshot(&host, "loop")["i"];
        assert_eq!(i.kind, LocalKind::Loop);
        assert!(i.initializer.is_some());
        let caught = &snapshot(&host, "catch")["x"];
        assert_eq!(caught.kind, LocalKind::Catch);
        assert!(caught.initializer.is_none());
        assert_eq!(
            caught.shadow.as_ref().unwrap().id,
            snapshot(&host, "restored")["x"].id
        );
        let binding_id = args["x"].id;
        assert_ne!(binding_id, args["x"].shadow.as_ref().unwrap().id);
        assert_eq!(snapshot(&host, "restored")["x"].kind, LocalKind::Let);
    }
}
