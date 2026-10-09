//! Genuine compiler-catalog transport authoring gate. UNCOMPILED/UNEXECUTED.
//! This exercises compiled source macros, not a host macro evaluator or reify.
use suss_cli::{
    portable_macro_data::FormBridge,
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionOptions, SessionValue},
};
use suss_reader::forms::{Form, Kind};

const FACTS: &str = r#"(defmacro ^{:suss/compiler-catalog true} compiler-facts []
  (let [catalog (:suss/compiler-catalog &env)
        revision (get (:namespaces catalog) 'record.live)
        info (get (:defs revision) 'value)
        methods (get (:protocol-signatures catalog) 'record.live/Pair)]
    (list 'quote [(:phase catalog) (:current catalog) (:name revision)
                  (:name info) (:doc info) (get methods '-pair)])))"#;

fn decode(session: &mut Session, value: &SessionValue) -> Form {
    session.collect().unwrap();
    let bridge = FormBridge::new(session).unwrap();
    bridge.read(session, value, 0..0).unwrap()
}
fn expect_facts(form: &Form, phase: &str, doc: &str, arities: &[f64]) {
    let Kind::Vector(items) = &form.kind else {
        panic!("decoded fact vector")
    };
    assert_eq!(items.len(), 6);
    assert!(
        matches!(&items[0].kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == phase)
    );
    for (index, expected) in [(1, "record.caller"), (2, "record.live")] {
        assert!(
            matches!(&items[index].kind, Kind::Symbol(name) if name.namespace.is_none() && name.name == expected)
        );
    }
    assert!(
        matches!(&items[3].kind, Kind::Symbol(name) if name.namespace.as_deref() == Some("record.live") && name.name == "value")
    );
    assert!(
        matches!(&items[4].kind, Kind::String(units) if *units == doc.encode_utf16().collect::<Vec<_>>())
    );
    let Kind::Vector(actual) = &items[5].kind else {
        panic!("actual declared overloads")
    };
    assert_eq!(actual.len(), arities.len());
    for (actual, expected) in actual.iter().zip(arities) {
        let Kind::Number(number) = &actual.kind else {
            panic!("numeric arity")
        };
        assert_eq!(number.to_bits(), expected.to_bits());
    }
}

#[test]
fn source_macro_consumes_actual_other_namespace_and_protocol_revisions_after_gc() {
    for (mut session, phase) in [
        (Session::new_repl().unwrap(), "runtime"),
        (Session::new_macro().unwrap(), "macro"),
    ] {
        let mut macros = CompiledMacros::new().unwrap();
        // New authored finite allowance, not measured or certified yet.
        session.set_operation_fuel(100_000_000);
        macros.set_operation_fuel(100_000_000);
        session.enter_namespace("record.live").unwrap();
        session.eval("(def ^{:doc \"first revision\"} value 17) (defprotocol Pair (-pair [this] [this x]))").unwrap();
        session.enter_namespace("record.caller").unwrap();
        macros.enter_namespace("record.caller").unwrap();
        macros.define(FACTS).unwrap();
        let retained = session
            .eval_with_macros("(compiler-facts)", &mut macros)
            .unwrap();
        expect_facts(
            &decode(&mut session, &retained),
            phase,
            "first revision",
            &[1.0, 2.0],
        );
        session.enter_namespace("record.live").unwrap();
        session.eval("(def ^{:doc \"second revision\"} value 23) (defprotocol Pair (-pair [this] [this x y]))").unwrap();
        session.enter_namespace("record.caller").unwrap();
        let fresh = session
            .eval_with_macros("(compiler-facts)", &mut macros)
            .unwrap();
        expect_facts(
            &decode(&mut session, &fresh),
            phase,
            "second revision",
            &[1.0, 3.0],
        );
        expect_facts(
            &decode(&mut session, &retained),
            phase,
            "first revision",
            &[1.0, 2.0],
        );
        assert_eq!(session.current_namespace(), "record.caller");
    }
}

#[test]
fn compiler_catalog_policy_is_opt_in_and_invalid_metadata_keeps_old_macro() {
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    session.set_operation_fuel(100_000_000);
    macros.set_operation_fuel(100_000_000);
    macros
        .define("(defmacro policy [] (list 'quote (:suss/compiler-catalog &env)))")
        .unwrap();
    let value = session.eval_with_macros("(policy)", &mut macros).unwrap();
    assert!(matches!(decode(&mut session, &value).kind, Kind::Nil));
    macros.define("(defmacro ^{:suss/compiler-catalog true} policy [] (list 'quote (:current (:suss/compiler-catalog &env))))").unwrap();
    let value = session.eval_with_macros("(policy)", &mut macros).unwrap();
    assert!(
        matches!(decode(&mut session, &value).kind, Kind::Symbol(name) if name.namespace.is_none() && name.name == "user")
    );
    let error = macros
        .define("(defmacro ^{:suss/compiler-catalog 1} policy [] nil)")
        .unwrap_err();
    assert!(error.to_string().contains("must be Boolean or nil"));
    let value = session.eval_with_macros("(policy)", &mut macros).unwrap();
    assert!(matches!(decode(&mut session, &value).kind, Kind::Symbol(name) if name.name == "user"));
    macros.define("(defmacro ^{:suss/compiler-catalog false} policy [] (list 'quote (:suss/compiler-catalog &env)))").unwrap();
    let value = session.eval_with_macros("(policy)", &mut macros).unwrap();
    assert!(matches!(decode(&mut session, &value).kind, Kind::Nil));
}

#[test]
fn failed_macro_reload_restores_catalog_policy_with_the_original_binding() {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("tools.sus");
    std::fs::write(&source, "(ns tools) (defmacro ^{:suss/compiler-catalog true} policy [] (list 'quote (:current (:suss/compiler-catalog &env))))").unwrap();
    let mut session = Session::with_options(SessionOptions {
        source_paths: vec![project.path().to_owned()],
        ..Default::default()
    })
    .unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    session.set_operation_fuel(100_000_000);
    macros.set_operation_fuel(100_000_000);
    session
        .eval_with_macros("(ns user (:require-macros [tools :as t]))", &mut macros)
        .unwrap();
    let value = session.eval_with_macros("(t/policy)", &mut macros).unwrap();
    assert!(matches!(decode(&mut session, &value).kind, Kind::Symbol(name) if name.name == "user"));
    // The replacement publishes a false policy before a later runtime failure.
    // The transaction must restore both its original callable and catalog flag.
    std::fs::write(
        &source,
        "(ns tools) (defmacro ^{:suss/compiler-catalog false} policy [] nil) (throw 73)",
    )
    .unwrap();
    assert!(
        session
            .eval_with_macros(
                "(ns user (:require-macros ^{:reload :reload} [tools :as t]))",
                &mut macros
            )
            .is_err()
    );
    let value = session.eval_with_macros("(t/policy)", &mut macros).unwrap();
    assert!(matches!(decode(&mut session, &value).kind, Kind::Symbol(name) if name.name == "user"));
}
