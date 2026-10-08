//! Library loading and compiled expansion prerequisites, independent of suspension.
use std::{collections::HashMap, sync::Arc};
use suss_compile::{
    portable::{
        AnalysisContext, ExpansionContext, ExpansionHost,
        hir::SourceNamespace,
        resolve::{Environment, Phase},
    },
    portable_macros::CompiledMacros,
    portable_session::Session,
};
use suss_reader::forms::{Kind, read_forms};

#[test]
fn bundled_async_library_loads_and_compiled_macros_preserve_ordered_body_forms() {
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    session.eval_with_macros("(ns async-library-test (:require [suss.async]) (:require-macros [suss.async :refer [future await]]))", &mut macros).unwrap();
    let mut environment = Environment::default();
    environment
        .declare_macro_exports(
            Phase::Runtime,
            "suss.async",
            &["future".into(), "await".into()],
        )
        .unwrap();
    let namespace = Arc::new(SourceNamespace::capture(&environment, Phase::Runtime));
    let locals = HashMap::new();
    let fields = HashMap::new();
    for (source, marker, expected) in [
        ("(suss.async/future 17 42)", "future*", vec![17.0f64, 42.0f64]),
        ("(suss.async/await 42)", "await*", vec![42.0f64]),
    ] {
        let form = read_forms(source).unwrap().remove(0);
        let expanded = macros
            .expand(
                &form,
                ExpansionContext {
                    environment: &environment,
                    namespace_snapshot: &namespace,
                    origin: None,
                    phase: Phase::Runtime,
                    context: AnalysisContext::Expression,
                    locals: &locals,
                    fields: &fields,
                    function_scopes: &[],
                },
            )
            .unwrap()
            .expect("bundled compiled macro must expand");
        let Kind::List(items) = expanded.kind else {
            panic!("compiler marker must be a list");
        };
        let Kind::Symbol(head) = &items[0].kind else {
            panic!("marker must be a symbol");
        };
        assert_eq!(head.namespace.as_deref(), Some("suss.async"));
        assert_eq!(head.name, marker);
        assert_eq!(items.len(), expected.len() + 1);
        for (form, expected) in items[1..].iter().zip(expected) {
            let Kind::Number(actual) = &form.kind else {
                panic!("body form must remain numeric");
            };
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
    }
    session.collect().unwrap();
    session.eval("42").unwrap();
}
