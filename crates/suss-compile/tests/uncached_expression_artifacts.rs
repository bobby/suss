//! The uncached public expression entrypoint uses compiled macros and ABI2 roots.
#![cfg(not(target_family = "wasm"))]

use suss_compile::{
    Compiler,
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionError, SessionValue},
};

fn number(session: &mut Session, value: &SessionValue) -> f64 {
    session
        .inspect(value, |mut store, value| {
            let number = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            Ok(number.field(&mut store, 0)?.unwrap_f64())
        })
        .unwrap()
}

#[test]
fn uncached_expression_executes_lexical_macro_and_keeps_live_bindings() {
    let artifact = Compiler::new()
        .compile_expr_with_info(
            r#"
            (defmacro require-local [local]
              (if (get (:locals &env) local) local (throw 17)))
            (def seen (atom 0))
            (defn calculate [x]
              (let [answer (+ x (swap! seen inc))] (require-local answer)))
            (def captured calculate)
            (calculate 41)
            "#,
        )
        .unwrap();
    assert!(!artifact.is_component);
    assert!(artifact.prepared.as_ref().unwrap().modules().count() > 1);
    let mut session = Session::new().unwrap();
    let first = artifact.execute(&mut session).unwrap().unwrap();
    session.collect().unwrap();
    assert_eq!(number(&mut session, &first), 42.0);
    let mut macros = CompiledMacros::new().unwrap();
    session
        .eval_with_macros("(defn calculate [x] 100)", &mut macros)
        .unwrap();
    let captured = session.eval("(captured 41)").unwrap();
    let current = session.eval("(calculate 41)").unwrap();
    session.collect().unwrap();
    assert_eq!(number(&mut session, &first), 42.0);
    assert_eq!(number(&mut session, &captured), 43.0);
    assert_eq!(number(&mut session, &current), 100.0);
}

#[test]
fn uncached_runtime_throw_is_deferred_and_prompt_recovers() {
    let artifact = Compiler::new()
        .compile_expr_with_info("(def initialized 41) (throw initialized)")
        .unwrap();
    let mut session = Session::new().unwrap();
    let Err(SessionError::Language(payload)) = artifact.execute(&mut session) else {
        panic!("Runtime effects must execute in the caller's Store");
    };
    session.collect().unwrap();
    assert_eq!(number(&mut session, &payload), 41.0);
    let recovered = session.eval("(+ initialized 1)").unwrap();
    assert_eq!(number(&mut session, &recovered), 42.0);
}

#[test]
fn repeated_uncached_compilations_have_isolated_macro_namespaces() {
    let mut compiler = Compiler::new();
    let first = compiler
        .compile_expr_with_info("(defmacro private-answer [] 41) (private-answer)")
        .unwrap();
    assert!(compiler.compile_expr_with_info("(private-answer)").is_err());
    let second = compiler
        .compile_expr_with_info("(defmacro private-answer [] 42) (private-answer)")
        .unwrap();
    let mut first_session = Session::new().unwrap();
    let mut second_session = Session::new().unwrap();
    let first = first.execute(&mut first_session).unwrap().unwrap();
    let second = second.execute(&mut second_session).unwrap().unwrap();
    first_session.collect().unwrap();
    second_session.collect().unwrap();
    assert_eq!(number(&mut first_session, &first), 41.0);
    assert_eq!(number(&mut second_session, &second), 42.0);
}
