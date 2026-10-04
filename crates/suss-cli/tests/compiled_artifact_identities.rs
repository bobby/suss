use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};
use suss_compile::portable::{
    self, ExpansionHost, SourceOrigin,
    artifact_identity::{Expected, Manifest},
    resolve::{Environment, Phase},
};
use suss_reader::forms::{Form, Kind, read_forms};

fn artifact(source: &str, host: &mut dyn ExpansionHost) -> Vec<u8> {
    portable::prepare_fragment_forms_with_origin(
        read_forms(source).unwrap(),
        0..source.len(),
        &Environment::default(),
        Phase::Runtime,
        host,
        Some(&SourceOrigin::new(source, Some("record.sus".into()))),
    )
    .unwrap()
    .wasm
}
fn manifest(wasm: &[u8]) -> Manifest {
    portable::artifact_identity::verify(
        wasm,
        Expected {
            phase: Some(Phase::Runtime),
            source: Some("(answer)"),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn source_artifact_identity_records_actual_macro_versions_and_rejects_prior_graph() {
    let mut macros = CompiledMacros::new().unwrap();
    macros.define("(defmacro answer [] 42)").unwrap();
    let first = artifact("(answer)", &mut macros);
    let first_manifest = manifest(&first);
    assert_eq!(first_manifest.source_path.as_deref(), Some("record.sus"));
    let versions = first_manifest.macro_dependencies.unwrap();
    assert!(versions.iter().any(|(name, _)| name == "macro:user/answer"));
    macros
        .define("(defmacro answer [] 42) ; changed source, same expansion")
        .unwrap();
    let second = artifact("(answer)", &mut macros);
    assert_ne!(
        manifest(&second).macro_dependencies.as_ref().unwrap(),
        &versions
    );
    assert!(
        portable::artifact_identity::verify(
            &second,
            Expected {
                macro_dependencies: Some(&versions),
                ..Default::default()
            }
        )
        .unwrap_err()
        .contains("dependency identity")
    );
    // Exercise the ordinary native installation path, not just metadata parsing.
    let mut runtime = Session::new().unwrap();
    let value = runtime.eval_with_macros("(answer)", &mut macros).unwrap();
    assert_eq!(
        suss_cli::portable_repl::display(&mut runtime, &value).unwrap(),
        "42"
    );
}

#[test]
fn source_artifact_identity_keeps_external_macro_provenance_unknown() {
    struct Host {
        calls: usize,
    }
    impl ExpansionHost for Host {
        fn expand(
            &mut self,
            form: &Form,
            _: portable::ExpansionContext<'_>,
        ) -> Result<Option<Form>, portable::Diagnostic> {
            if matches!(form.kind, Kind::List(_)) {
                self.calls += 1;
                Ok(Some(Form {
                    kind: Kind::Number(42.0),
                    span: form.span.clone(),
                    metadata: vec![],
                }))
            } else {
                Ok(None)
            }
        }
    }
    let mut host = Host { calls: 0 };
    let wasm = artifact("(answer)", &mut host);
    assert_eq!(manifest(&wasm).macro_dependencies, None);
    assert!(
        portable::artifact_identity::verify(
            &wasm,
            Expected {
                macro_dependencies: Some(&[]),
                ..Default::default()
            }
        )
        .is_err()
    );
    let mut runtime = Session::new().unwrap();
    let value = runtime.eval_with_macros("(answer)", &mut host).unwrap();
    assert_eq!(
        suss_cli::portable_repl::display(&mut runtime, &value).unwrap(),
        "42"
    );
    assert_eq!(host.calls, 2);
}
