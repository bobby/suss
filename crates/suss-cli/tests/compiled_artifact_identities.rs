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
            if matches!(&form.kind, Kind::List(_)) {
                self.calls += 1;
                Ok(Some(Form {
                    kind: Kind::Long(42),
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

#[test]
fn aot_component_source_index_preserves_all_input_fields_and_macro_versions() {
    use std::collections::BTreeSet;
    use std::process::Command;
    let source = "(ns provenance.sample)\n(defmacro answer [] 41)\n(def first-answer (answer))\n(defmacro answer [] 42)\n(def second-answer (answer))\n(def check (fn [unused] (case [first-answer second-answer] [41 42] 1 0)))\n";
    let fragments = suss_cli::portable_aot::prepare_source(source, None, &[]).unwrap();
    let expected = fragments
        .iter()
        .map(|fragment| {
            portable::artifact_identity::verify(&fragment.wasm, Expected::default()).unwrap()
        })
        .collect::<Vec<_>>();
    let versions = expected
        .iter()
        .flat_map(|manifest| manifest.macro_dependencies.iter().flatten())
        .filter(|(name, _)| name == "macro:provenance.sample/answer")
        .map(|(_, version)| version)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        versions.len(),
        2,
        "retain both actual macro definition versions"
    );
    assert!(
        expected
            .iter()
            .flat_map(|manifest| manifest.macro_dependencies.iter().flatten())
            .any(|(name, _)| name == "macro:suss.core/case")
    );
    let mut resolve = portable::aot::Resolve::new();
    let package = resolve
        .push_str(
            "provenance.wit",
            "package test:provenance; world w { export check: func(unused: u32) -> u32; }",
        )
        .unwrap();
    let world = *resolve.packages[package].worlds.values().next().unwrap();
    let component = portable::aot::component(
        &fragments,
        &resolve,
        world,
        &[(
            "check".into(),
            suss_reader::Symbol::namespaced("provenance.sample", "check"),
        )],
    )
    .unwrap();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&component)
        .unwrap();
    assert_eq!(
        portable::aot::input_source_manifests(&component).unwrap(),
        expected
    );
    let modules = wasmparser::Parser::new(0)
        .parse_all(&component)
        .filter_map(|payload| {
            if let wasmparser::Payload::ModuleSection {
                unchecked_range, ..
            } = payload.unwrap()
            {
                Some(unchecked_range)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    for (index, manifest) in expected.iter().enumerate() {
        let range = &modules[2 + index];
        let body =
            &component[usize::try_from(range.start).unwrap()..usize::try_from(range.end).unwrap()];
        assert_eq!(
            portable::bootstrap::sha256(body),
            manifest.wasm_sha256,
            "the unoptimized component embeds exactly the verified input body"
        );
    }
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("probe.wasm"), &component).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(dir.path())
        .args(["run", "probe.wasm", "--invoke", "check", "--", "0"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "1");
    if let Some(path) = std::env::var_os("SUSS_COMPONENT_PROVENANCE_CAPTURE") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("probe.sus"), source).unwrap();
        std::fs::write(path.join("component.wasm"), &component).unwrap();
        for (index, fragment) in fragments.iter().enumerate() {
            let raw = wasmparser::Parser::new(0)
                .parse_all(&fragment.wasm)
                .find_map(|payload| {
                    if let wasmparser::Payload::CustomSection(section) = payload.unwrap() {
                        (section.name() == "suss.source-artifact").then(|| section.data().to_vec())
                    } else {
                        None
                    }
                })
                .unwrap();
            std::fs::write(path.join(format!("original-{index:03}.edn")), raw).unwrap();
        }
        let raw = wasmparser::Parser::new(0)
            .parse_all(&component)
            .find_map(|payload| {
                if let wasmparser::Payload::CustomSection(section) = payload.unwrap() {
                    (section.name() == "suss.input-source-artifacts")
                        .then(|| section.data().to_vec())
                } else {
                    None
                }
            })
            .unwrap();
        std::fs::write(path.join("component-inputs.edn"), raw).unwrap();
        std::fs::write(
            path.join("component.sha256"),
            portable::bootstrap::sha256(&component),
        )
        .unwrap();
    }
}
