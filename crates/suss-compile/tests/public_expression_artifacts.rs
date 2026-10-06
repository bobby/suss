//! Prepared public expressions execute actual ABI2 bytes without source replay.
#![cfg(not(target_family = "wasm"))]
#[path = "support/portable_decode.rs"]
mod portable_decode;
use suss_compile::{
    Compiler,
    portable_session::{Session, SessionError, SessionOptions},
};

fn number(session: &mut Session, value: &suss_compile::portable_session::SessionValue) -> f64 {
    session
        .inspect(value, |mut store, value| {
            let number = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            Ok(number.field(&mut store, 0)?.unwrap_f64())
        })
        .unwrap()
}

#[test]
fn compiled_macro_artifact_retains_runtime_state_and_roots() {
    let artifact = Compiler::new()
        .compile_expr_cached(
            r#"
        (defmacro require-local [local]
          (if (get (:locals &env) local) local (throw 17)))
        (def seen (atom 0))
        (defn calculate [x]
          (let [answer (+ x (swap! seen inc))] (require-local answer)))
        (calculate 41)
    "#,
        )
        .unwrap();
    assert!(artifact.prepared.as_ref().unwrap().modules().count() > 1);
    let mut session = Session::new().unwrap();
    let result = artifact.execute(&mut session).unwrap().unwrap();
    session.collect().unwrap();
    assert_eq!(number(&mut session, &result), 42.0);
    let next = session.eval("(calculate 41)").unwrap();
    assert_eq!(number(&mut session, &next), 43.0);
    session.collect().unwrap();
    assert_eq!(number(&mut session, &result), 42.0);
}

#[test]
fn cached_runtime_initializer_is_deferred_and_exception_payload_survives() {
    let artifact = Compiler::new().compile_expr_cached("(throw 17)").unwrap();
    let mut session = Session::new().unwrap();
    let Err(SessionError::Language(payload)) = artifact.execute(&mut session) else {
        panic!("compiled Runtime initializer must run only during artifact execution");
    };
    session.collect().unwrap();
    assert_eq!(number(&mut session, &payload), 17.0);
}

#[test]
fn prepared_dependencies_execute_once_even_after_source_disappears() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("artifact_dep.sus");
    std::fs::write(
        &path,
        "(ns artifact-dep) (def seen (atom 0)) (swap! seen inc)",
    )
    .unwrap();
    let paths = vec![root.path().to_owned()];
    let artifact = Compiler::new()
        .prepare_expression(
            "(ns artifact-user (:require [artifact-dep :as dep])) @dep/seen",
            &paths,
        )
        .unwrap();
    std::fs::remove_file(path).unwrap();
    let mut session = Session::with_options(SessionOptions {
        source_paths: paths,
        ..Default::default()
    })
    .unwrap();
    let value = artifact.execute(&mut session).unwrap().unwrap();
    assert_eq!(number(&mut session, &value), 1.0);
    session.load_namespace("artifact-dep").unwrap();
    let value = session.eval("@dep/seen").unwrap();
    assert_eq!(number(&mut session, &value), 1.0);
}

#[test]
fn artifact_language_exception_remains_inspectable_and_prompt_recovers() {
    let artifact = Compiler::new()
        .prepare_expression("(def value 41) (throw value)", &[])
        .unwrap();
    let mut session = Session::new().unwrap();
    let Err(SessionError::Language(payload)) = artifact.execute(&mut session) else {
        panic!("expected an owned language exception");
    };
    session.collect().unwrap();
    assert_eq!(number(&mut session, &payload), 41.0);
    let value = session.eval("(+ value 1)").unwrap();
    assert_eq!(number(&mut session, &value), 42.0);
}

#[test]
fn populated_session_rejects_bundle_before_initialization() {
    let artifact = Compiler::new()
        .prepare_expression("(throw 17)", &[])
        .unwrap();
    let mut session = Session::new_repl().unwrap();
    let before = session.stats().resident_fragments;
    assert!(matches!(
        artifact.execute(&mut session),
        Err(SessionError::Host(_))
    ));
    assert_eq!(session.stats().resident_fragments, before);
    let value = session.eval("(+ 40 2)").unwrap();
    assert_eq!(number(&mut session, &value), 42.0);
}

#[test]
fn resetting_core_observer_invalidates_pending_artifact_execution() {
    let artifact = Compiler::new()
        .compile_expr_cached("(def seen 17) seen")
        .unwrap();
    let mut session = Session::new().unwrap();
    let result = artifact.execute_with_core(&mut session, |session| session.reset());
    assert!(matches!(result, Err(SessionError::ForeignValue)));
    assert!(
        session.eval("seen").is_err(),
        "user initialization must not cross reset"
    );
    let value = session.eval("42").unwrap();
    assert_eq!(number(&mut session, &value), 42.0);
}

#[test]
fn exception_info_observation_reads_nominal_storage_and_rejects_impostors() {
    let artifact = Compiler::new()
        .compile_expr_cached("(ex-info \"probe\" {:reason :expected})")
        .unwrap();
    let mut session = Session::new().unwrap();
    let (value, mut decoder) = artifact
        .execute_with_core(&mut session, |session| {
            portable_decode::Decoder::capture(session, 100_000)
        })
        .unwrap();
    session.collect().unwrap();
    let observed = decoder
        .decode_session(&mut session, &value.unwrap())
        .unwrap();
    let portable_decode::Observation::ExceptionInfo {
        message,
        data,
        cause,
    } = observed
    else {
        panic!("expected independently decoded ExceptionInfo storage");
    };
    assert_eq!(
        *message,
        portable_decode::Observation::String("probe".encode_utf16().collect())
    );
    assert_eq!(*cause, portable_decode::Observation::Nil);
    assert_eq!(
        *data,
        portable_decode::Observation::Map(vec![(
            portable_decode::Observation::Keyword(None, "reason".encode_utf16().collect()),
            portable_decode::Observation::Keyword(None, "expected".encode_utf16().collect()),
        )])
    );
    session
        .eval("(deftype ImpostorInfo [message data cause])")
        .unwrap();
    let impostor = session
        .eval("(new ImpostorInfo \"probe\" nil nil)")
        .unwrap();
    assert!(
        decoder.decode_session(&mut session, &impostor).is_err(),
        "same-layout foreign descriptor must not be accepted as ExceptionInfo"
    );
}

#[test]
fn empty_macro_session_rejects_runtime_bundle_before_core_callback() {
    use suss_compile::portable::resolve::Phase;
    let artifact = Compiler::new().compile_expr_cached("(throw 17)").unwrap();
    let mut session = Session::with_options_in(SessionOptions::default(), Phase::Macro).unwrap();
    assert_eq!(session.stats().resident_fragments, 0);
    let mut called = false;
    let result = artifact.execute_with_core(&mut session, |_| {
        called = true;
        Ok(())
    });
    assert!(matches!(result, Err(SessionError::Host(_))));
    assert!(
        !called,
        "phase rejection must precede core initialization and callback effects"
    );
    assert_eq!(session.phase(), Phase::Macro);
    assert_eq!(session.stats().resident_fragments, 0);
    let value = session.eval("42").unwrap();
    assert_eq!(number(&mut session, &value), 42.0);
}
