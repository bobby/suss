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
    origins: Vec<(String, Option<portable::SourceOrigin>)>,
    catalogs: Vec<(
        String,
        String,
        std::collections::BTreeMap<String, String>,
        Vec<(portable::resolve::Global, portable::resolve::DefinitionInfo)>,
    )>,
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
        let label = String::from_utf16(label).unwrap();
        self.origins.push((label.clone(), context.origin.cloned()));
        let scope = context.environment.namespace_scope(context.phase);
        self.catalogs.push((
            label.clone(),
            scope.namespace.into(),
            scope.aliases.clone(),
            scope
                .declarations
                .into_iter()
                .map(|(global, info)| (global.clone(), info.clone()))
                .collect(),
        ));
        self.snapshots.push((label, context.locals.clone()));
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

#[test]
fn compiler_macro_namespace_facts_are_phase_scoped_staged_and_distinct_from_runtime_values() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut host = Observe::default();
        let source = r#"(ns catalog (:require [cljs.core :as c]))
(def ^:private tracked "original doc" 40)
(def pending (checkpoint "inside" tracked))
(checkpoint "after" (c/hash pending))"#;
        session.eval_with_macros(source, &mut host).unwrap();
        let inside = host.catalogs.iter().find(|x| x.0 == "inside").unwrap();
        assert_eq!(inside.1, "catalog");
        assert_eq!(inside.2["c"], "suss.core");
        let tracked = inside
            .3
            .iter()
            .find(|(g, _)| g.name() == "tracked")
            .unwrap();
        assert_eq!(tracked.1.declaration.metadata.len(), 1);
        assert_eq!(
            String::from_utf16(tracked.1.docstring.as_ref().unwrap()).unwrap(),
            "original doc"
        );
        assert_eq!(tracked.1.origin.as_ref().unwrap().text(), source);
        assert!(matches!(
            tracked.1.initializer.as_ref().unwrap().kind,
            Expression::Literal(_)
        ));
        let pending = inside
            .3
            .iter()
            .find(|(g, _)| g.name() == "pending")
            .unwrap();
        assert!(pending.1.initializer.is_none()); // initializer is being analyzed here
        let after = host.catalogs.iter().find(|x| x.0 == "after").unwrap();
        assert!(
            after
                .3
                .iter()
                .find(|(g, _)| g.name() == "pending")
                .unwrap()
                .1
                .initializer
                .is_some()
        );
        assert!(session.eval("(def broken no-such-name)").is_err());
        session
            .eval_with_macros(r#"(checkpoint "failed-compile" tracked)"#, &mut host)
            .unwrap();
        assert!(
            !host
                .catalogs
                .iter()
                .find(|x| x.0 == "failed-compile")
                .unwrap()
                .3
                .iter()
                .any(|(g, _)| g.name() == "broken")
        );
        assert!(matches!(
            session.eval(r#"(def tracked "changed compiler doc" (throw 1))"#),
            Err(suss_cli::portable_session::SessionError::Language(_))
        ));
        assert_eq!(scalar(&mut session, "tracked"), 40.0);
        session
            .eval_with_macros(r#"(checkpoint "failed-init" tracked)"#, &mut host)
            .unwrap();
        let declared = &host
            .catalogs
            .iter()
            .find(|x| x.0 == "failed-init")
            .unwrap()
            .3
            .iter()
            .find(|(g, _)| g.name() == "tracked")
            .unwrap()
            .1;
        assert!(matches!(
            declared.initializer.as_ref().unwrap().kind,
            Expression::Throw(_)
        ));
        session.reset().unwrap();
        session
            .eval_with_macros(r#"(checkpoint "reset" 0)"#, &mut host)
            .unwrap();
        assert!(
            !host
                .catalogs
                .iter()
                .find(|x| x.0 == "reset")
                .unwrap()
                .3
                .iter()
                .any(|(g, _)| g.name() == "tracked")
        );
    }
}

#[test]
fn compiler_macro_source_origins_keep_module_paths_separate_from_inline_inputs() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("origin_module.sus");
    let source = "(ns origin-module)\n(def x 21)\n(def y (checkpoint \"module\" (+ x 1)))";
    std::fs::write(&path, source).unwrap();
    let path = path.canonicalize().unwrap();
    for phase in [
        portable::resolve::Phase::Runtime,
        portable::resolve::Phase::Macro,
    ] {
        let mut session = Session::with_options_in(
            suss_cli::portable_session::SessionOptions {
                source_paths: vec![root.path().to_path_buf()],
                ..Default::default()
            },
            phase,
        )
        .unwrap();
        let mut host = Observe::default();
        session
            .load_namespace_with_macros("origin-module", &mut host)
            .unwrap();
        let origin = host
            .origins
            .iter()
            .find(|x| x.0 == "module")
            .unwrap()
            .1
            .as_ref()
            .unwrap();
        assert_eq!(origin.path(), Some(path.as_path()));
        assert_eq!(origin.text(), source);
        session.enter_namespace("origin-module").unwrap();
        session
            .eval_with_macros(r#"(checkpoint "inline" y)"#, &mut host)
            .unwrap();
        assert!(
            host.origins
                .iter()
                .find(|x| x.0 == "inline")
                .unwrap()
                .1
                .as_ref()
                .unwrap()
                .path()
                .is_none()
        );
        let declaration = &host
            .catalogs
            .iter()
            .find(|x| x.0 == "inline")
            .unwrap()
            .3
            .iter()
            .find(|(g, _)| g.name() == "y")
            .unwrap()
            .1;
        assert_eq!(
            declaration.origin.as_ref().unwrap().path(),
            Some(path.as_path())
        );
        assert_eq!(scalar(&mut session, "y"), 22.0);
    }
}

#[test]
fn compiler_macro_nested_definitions_keep_each_declaration_and_initializer_together() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut host = Observe::default();
        let source = r#"(def tracked "outer doc"
  (do (defonce ^:private tracked (checkpoint "inner-body" 7))
      (checkpoint "outer-body" tracked)))
(checkpoint "after-nesting" tracked)"#;
        session.eval_with_macros(source, &mut host).unwrap();
        assert_eq!(scalar(&mut session, "tracked"), 7.0);
        let declared = |label: &str| {
            &host
                .catalogs
                .iter()
                .find(|x| x.0 == label)
                .unwrap()
                .3
                .iter()
                .find(|(g, _)| g.name() == "tracked")
                .unwrap()
                .1
        };
        let inner = declared("inner-body");
        assert!(inner.once);
        assert_eq!(inner.declaration.metadata.len(), 1);
        assert!(inner.docstring.is_none());
        assert!(inner.initializer.is_none());
        assert!(matches!(
            declared("outer-body").initializer.as_ref().unwrap().kind,
            Expression::Literal(_)
        ));
        let outer = declared("after-nesting");
        assert!(!outer.once);
        assert!(outer.declaration.metadata.is_empty());
        assert_eq!(
            String::from_utf16(outer.docstring.as_ref().unwrap()).unwrap(),
            "outer doc"
        );
        assert!(matches!(
            outer.initializer.as_ref().unwrap().kind,
            Expression::Do(_)
        ));
        assert_eq!(&source[outer.declaration.span.clone()], "tracked");
    }
}
