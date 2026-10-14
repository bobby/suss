use std::sync::{Arc, Mutex};
use suss_compile::portable_session::{Session, SessionOptions};

fn observed_number(store: &mut wasmtime::StoreContextMut<'_, ()>, value: wasmtime::Val) -> f64 {
    value
        .unwrap_anyref()
        .unwrap()
        .as_struct(&*store)
        .unwrap()
        .unwrap()
        .field(&mut *store, 0)
        .unwrap()
        .unwrap_f64()
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
    for (source, expected) in [("token-a", 0.0), ("token-b", 1.0)] {
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
    assert_eq!(*observed.lock().unwrap(), vec![(0, 3.0), (1, 9.0)]);
    assert!(session.eval("(suss.harness.observation/capture)").is_err());
    assert!(
        session
            .eval("(suss.harness.observation/capture 1 2)")
            .is_err()
    );
    assert_eq!(*observed.lock().unwrap(), vec![(0, 3.0), (1, 9.0)]);

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
