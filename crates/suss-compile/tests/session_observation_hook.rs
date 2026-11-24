use std::sync::{Arc, Mutex};
use suss_compile::portable_session::{Session, SessionOptions};

/// A long, as Clojure's integer values are.
fn observed_number(store: &mut wasmtime::StoreContextMut<'_, ()>, value: wasmtime::Val) -> i64 {
    match suss_compile::portable_session::runtime_number(&mut *store, &value).unwrap() {
        Some(suss_compile::portable_session::RuntimeNumber::Long(value)) => value,
        other => panic!("expected a long, observed {other:?}"),
    }
}

#[test]
fn hook_observes_values_synchronously_with_fixed_arity_and_expires_on_reset() {
    let mut session = Session::new().unwrap();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let output = observed.clone();
    session
        .install_observation_hook(move |mut store, value| {
            let mut output = output.lock().unwrap();
            let token = u32::try_from(output.len())
                .map_err(|_| wasmtime::Error::msg("test token overflow"))?;
            output.push((token, observed_number(&mut store, value)));
            Ok(token)
        })
        .unwrap();
    assert!(session.install_observation_hook(|_, _| Ok(0)).is_err());

    session.eval("(def hook-values (array 3)) (def token-a (suss.harness.observation/capture (aget hook-values 0))) (aset hook-values 0 9) (def token-b (suss.harness.observation/capture (aget hook-values 0)))").unwrap();
    for (source, expected) in [("token-a", 0), ("token-b", 1)] {
        let value = session.eval(source).unwrap();
        assert_eq!(
            session
                .inspect(&value, |mut store, value| Ok(observed_number(
                    &mut store, value
                )))
                .unwrap(),
            expected
        );
    }
    assert_eq!(*observed.lock().unwrap(), vec![(0, 3), (1, 9)]);
    assert!(session.eval("(suss.harness.observation/capture)").is_err());
    assert!(
        session
            .eval("(suss.harness.observation/capture 1 2)")
            .is_err()
    );
    assert_eq!(*observed.lock().unwrap(), vec![(0, 3), (1, 9)]);

    session.reset().unwrap();
    assert!(
        session
            .eval("(suss.harness.observation/capture 1)")
            .is_err()
    );
}

#[test]
fn hook_rejects_non_runtime_phase_and_filesystem_namespace_override() {
    let mut macros = Session::new_macro().unwrap();
    assert!(macros.install_observation_hook(|_, _| Ok(0)).is_err());

    let root = tempfile::tempdir().unwrap();
    let namespace = root.path().join("suss/harness");
    std::fs::create_dir_all(&namespace).unwrap();
    std::fs::write(
        namespace.join("observation.sus"),
        "(ns suss.harness.observation)",
    )
    .unwrap();
    let mut session = Session::with_options(SessionOptions {
        source_paths: vec![root.path().to_owned()],
        ..Default::default()
    })
    .unwrap();
    assert!(session.install_observation_hook(|_, _| Ok(0)).is_err());
}

#[test]
fn namespace_local_macros_require_an_explicit_isolated_observer() {
    use suss_compile::portable_macros::CompiledMacros;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("provider.sus"),
        "(ns provider (:require [suss.harness.observation :as host])) (def token (fn [x] (host/capture x)))").unwrap();
    std::fs::write(root.path().join("consumer.sus"),
        "(ns consumer (:require [provider :as p])) (defmacro answer [x] (p/token x)) (def value (answer 41))").unwrap();
    let mut session = Session::new_repl_with_options(SessionOptions {
        source_paths: vec![root.path().to_owned()], ..Default::default()
    }).unwrap();
    let runtime_values = Arc::new(Mutex::new(Vec::new()));
    let output = runtime_values.clone();
    session.install_observation_hook(move |mut store, value| {
        output.lock().unwrap().push(observed_number(&mut store, value));
        Ok(900)
    }).unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    let failure = session.load_namespace_with_macros("consumer", &mut macros).unwrap_err();
    assert!(failure.to_string().contains("No source for namespace suss.harness.observation"), "{failure}");
    assert!(runtime_values.lock().unwrap().is_empty());

    let macro_values = Arc::new(Mutex::new(Vec::new()));
    let output = macro_values.clone();
    macros.install_observation_hook(move |mut store, value| {
        output.lock().unwrap().push(observed_number(&mut store, value));
        Ok(17)
    }).unwrap();
    assert!(macros.install_observation_hook(|_, _| Ok(0)).is_err());
    session.load_namespace_with_macros("consumer", &mut macros).unwrap();
    let value = session.eval("consumer/value").unwrap();
    assert_eq!(session.inspect(&value, |mut store, value| Ok(observed_number(&mut store, value))).unwrap(), 17);
    assert_eq!(*macro_values.lock().unwrap(), vec![41]);
    assert!(runtime_values.lock().unwrap().is_empty());
    let value = session.eval("(provider/token 52)").unwrap();
    assert_eq!(session.inspect(&value, |mut store, value| Ok(observed_number(&mut store, value))).unwrap(), 900);
    assert_eq!(*runtime_values.lock().unwrap(), vec![52]);
    assert_eq!(*macro_values.lock().unwrap(), vec![41]);
}

#[test]
fn failed_macro_observation_is_an_expansion_failure_without_runtime_effects() {
    use suss_compile::portable_macros::CompiledMacros;
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    macros.install_observation_hook(|_, _| Err(wasmtime::Error::msg(
        "Macro assertion capture is unsupported",
    ))).unwrap();
    macros.define("(defmacro observe [] (suss.harness.observation/capture 17))").unwrap();
    session.eval("(def effects (atom 0))").unwrap();
    let error = session.eval_with_macros("(do (swap! effects inc) (observe))", &mut macros).unwrap_err();
    assert!(format!("{error:?}").contains("Macro assertion capture is unsupported"), "{error:?}");
    let value = session.eval("@effects").unwrap();
    assert_eq!(session.inspect(&value, |mut store, value| Ok(observed_number(&mut store, value))).unwrap(), 0);
}
